# Multi Image Generation Artifacts Design

- Date: 2026-06-07
- Status: Approved design, pending implementation
- Scope: Multi-image generation result model for desktop `generate_media` and
  agent `ImageGeneration`

## Summary

Puffer should support requests such as "create two images" as one media
generation job that produces multiple image artifacts. The durable identity
model should stay simple:

- `jobId` identifies one generation request.
- `artifactId` identifies one persisted media file.
- A multi-image response returns `artifacts[]`, not a shared artifact id.

This design intentionally does not preserve the current single-artifact
response shape. The long-term contract should remove single `artifactId` and
`path` fields from generation results and require consumers to read
`artifacts[]`.

## Context

Generated image persistence already uses workspace-local media sidecars:

```text
.puffer/media/jobs/<job-id>.json
.puffer/media/images/<artifact-id>/image.*
.puffer/media/artifact-sidecars/<artifact-id>.json
```

`MediaJob` already stores `artifact_ids: Vec<String>`, but the public image
generation result currently exposes only one `artifactId` and one `path`.
Adapters also persist a single artifact per execution. The desktop timeline
then synthesizes one generated-media attachment from that single artifact id.

The current model is correct for one image, but it does not represent one
user intent that produces multiple images. Reusing one `artifactId` for
multiple files would break the current path, sidecar, preview, and frontend
identity assumptions.

## Goals

- Support one image generation request producing multiple images.
- Keep one image file per artifact id.
- Use one job id as the grouping identity for a multi-image request.
- Keep preview reads lightweight and single-artifact scoped.
- Let existing attachment strips display multiple generated images without a
  new gallery system.
- Prefer provider-native multi-image execution when available.
- Keep the implementation narrow and avoid a generic artifact container.
- Avoid derived fields that can drift from persisted artifact ids.

## Non-Goals

- Do not reuse one `artifactId` for multiple images.
- Do not add a global media gallery, artifact browser, object storage layer, or
  database.
- Do not add per-image prompt editing in this change.
- Do not add a generic batch framework beyond image generation count.
- Do not preserve old single `artifactId` or `path` response fields.
- Do not make preview reads accept artifact indexes or arbitrary file paths.
- Do not add complex partial retry UI.
- Do not add `count` to persisted media settings or user-selectable provider
  parameters.

## Recommended Architecture

Add a count-aware image generation path that normalizes every adapter into a
vector of image outputs. Keep the implementation in the current media adapter
shape: adapters resolve provider responses and persist artifacts through
`MediaGenerationService`; the top-level runtime selects an adapter and maps the
persisted artifacts into the public result.

The request lifecycle:

1. Validate request and `count`.
2. Create one `MediaJob`.
3. Execute the selected adapter for the requested count.
4. Persist each returned image as a unique `MediaArtifact`.
5. Attach each artifact id to the job in stable output order.
6. Save the final job state.
7. Return a result with `jobId` and `artifacts[]`.

The stable invariant is:

```text
one job -> one user generation request
one artifact -> one generated media file
```

## Request Contract

`ImageGeneration` input should add `count`:

```json
{
  "prompt": "Create two Gundam images",
  "aspect": "square",
  "count": 2
}
```

Rules:

- `count` defaults to `1`.
- `count` must be an integer.
- Valid range is `1..4`.
- Invalid values fail before provider execution.
- The prompt is shared by all generated images.

The same `count` field should be available to desktop `generate_media` image
requests. Chat text may be interpreted by the agent, but the runtime contract
only consumes the explicit count.

`count` is a runtime request field, not a media settings parameter. The runtime
may map it to a provider field such as `n` when the selected adapter supports
native multi-output generation, but users should not configure `n` through
global image defaults.

Provider count fields must be written as typed request fields. Do not route
`count` through the existing string-valued media `parameters` map, because that
would serialize numeric provider fields such as `n` as strings.
Provider descriptors must not expose `n` or equivalent count fields as
user-selectable media parameters.

## Result Contract

Replace single-artifact generation responses with:

```ts
type GenerateMediaResult = {
  jobId: string;
  kind: "image";
  providerId: string;
  modelId: string;
  status: "succeeded" | "failed";
  prompt: string;
  requestedCount: number;
  artifacts: GeneratedMediaArtifact[];
};

type GeneratedMediaArtifact = {
  artifactId: string;
  index: number;
  path: string;
  mimeType: string;
  size: number;
};
```

`ImageGeneration` tool output should use the same shape. This avoids a second
agent-only schema and keeps desktop session synthesis deterministic.

`index` is the output order within the job. It is metadata for stable display,
not a preview lookup key.

## Runtime Contract

Change the exact image result from single artifact to multiple artifacts:

```rust
pub struct ExactImageGenerationResult {
    pub job_id: String,
    pub requested_count: u8,
    pub artifacts: Vec<ExactGeneratedArtifact>,
    pub provider_id: String,
    pub model_id: String,
    pub status: String,
}

pub struct ExactGeneratedArtifact {
    pub artifact_id: String,
    pub index: usize,
    pub path: PathBuf,
    pub mime_type: String,
    pub byte_count: u64,
}
```

Adapters should normalize provider responses into a small internal vector:

```rust
struct ImageOutput {
    bytes: Vec<u8>,
    revised_prompt: Option<String>,
    remote_source_url: Option<String>,
}
```

Each adapter should persist `Vec<ImageOutput>` into one job's artifacts using
the existing `MediaGenerationService` APIs. A tiny shared helper for the
repeated "write artifact, save sidecar, attach id" sequence is acceptable only
if it stays image-specific and does not become a generic batch framework.

## Adapter Strategy

Use provider-native multi-image generation when it is deterministic and
validated by the adapter.

- `images_json`: send provider-supported `n` or equivalent count when the
  adapter supports it. Parse all `data[]` entries instead of only the first.
- `chat_image_output`: parse all image outputs from choices, message images,
  and content parts in stable response order. Because this adapter has no
  explicit typed count field today, repeat single-image calls serially until
  `count` outputs are collected or all attempts fail.
- adapters without native count support: execute repeated single-image calls
  under the same job. Start with serial execution for stability and rate-limit
  safety. This design does not add parallel execution.

If a provider returns more images than requested, keep only the first `count`
outputs. If it returns fewer, persist the images it returned and record the
shortfall in job metadata.

## Job And Artifact Metadata

The job sidecar owns grouping state:

```json
{
  "id": "<job uuid>",
  "artifactIds": ["<artifact uuid 1>", "<artifact uuid 2>"],
  "requestedCount": 2
}
```

Do not persist `producedCount`. The produced count is
`artifactIds.length`; storing it separately creates a second source of truth.
Do not persist per-image failure lists in this change. For partial success, the
requested count and artifact id list are enough to observe a shortfall without
creating a retry model.

Each artifact sidecar remains single-file scoped. Add only narrow metadata:

```json
{
  "jobId": "<job uuid>",
  "index": 0,
  "providerId": "...",
  "modelId": "...",
  "prompt": "...",
  "parameters": {}
}
```

Do not nest multiple file records inside one artifact sidecar. The job sidecar
already owns the artifact list.

## Timeline And Attachment Contract

Generated media attachments stay one image per attachment. The source shape
should include enough grouping metadata for display and debugging:

```ts
type AttachmentPreviewSource =
  | { kind: "user_upload" }
  | {
      kind: "generated_media";
      jobId: string;
      artifactId: string;
      index: number;
    };
```

Timeline synthesis should parse `artifacts[]` from successful `ImageGeneration`
tool outputs and create one assistant attachment per artifact. All artifacts
from one tool output should attach to the same assistant message in `index`
order.

The frontend should continue to render the existing thumbnail strip. No new
gallery, route, or artifact browser is needed.

## Preview Read Contract

Keep preview reads single-artifact scoped:

```json
{
  "sessionId": "<session uuid>",
  "artifactId": "<artifact uuid>"
}
```

The backend continues to resolve the session cwd, load the artifact sidecar,
validate image provenance, validate the path under
`.puffer/media/images/<artifact-id>/`, sniff MIME when needed, and return bytes
or an unavailable state.

`jobId` and `index` are not required for preview reads. Requiring them would
make the read path slower and more fragile without improving safety.

## Error Handling

Use a simple partial-success model:

- `0` produced images: job `failed`; the tool or RPC returns an error after
  saving the failed job sidecar.
- `1..count` produced images: job `succeeded`, result includes the produced
  artifacts.
- The job metadata records `requestedCount`. A partial shortfall is observable
  when `artifactIds.length < requestedCount`.

Do not drop successfully generated images because a subsequent output failed.
Do not add per-image retry UI, partial-warning fields, or a partial status in
this change. A future retry can be another generation job with a new job id.

## Performance

The default upper bound of four images keeps memory and network usage bounded.
Provider-native multi-output requests should be preferred because they reduce
round trips. Repeated-call fallback should start serially to avoid surprise rate
limit failures and easier cancellation semantics.

Timeline loading should remain metadata-only. Image bytes should still be read
lazily through preview requests.

## Testing

Core tests:

- `count` validation accepts `1..4` and rejects invalid values.
- `images_json` persists every item in `data[]` up to requested count.
- Generated artifacts have unique ids and stable indexes.
- One job sidecar records every artifact id.
- The job sidecar stores `requestedCount`; produced count is derived from
  `artifactIds`.
- Artifact sidecars point at distinct files under
  `.puffer/media/images/<artifact-id>/`.
- Preview reads work for every generated artifact.

CLI and desktop API tests:

- `generate_media count=2` returns one job id and two artifacts.
- `ImageGeneration count=2` tool output uses the same `artifacts[]` schema.
- Timeline synthesis creates two generated-media attachments from one tool
  output.
- Live desktop preview appends one assistant item containing two attachments
  and does not replace the first attachment with the second.

Frontend tests:

- Generated attachment source carries `jobId`, `artifactId`, and `index`.
- The preview helper still reads by `artifactId`.
- Thumbnail strip renders multiple generated image attachments in order.

## Acceptance Criteria

- A prompt asking for two images can complete through one runtime generation
  job.
- The result exposes one `jobId` and two unique `artifactId` values.
- Reloaded chat history shows both generated images as assistant attachments.
- Each image preview can be read independently by `sessionId + artifactId`.
- No generated image shares an artifact id with another generated image.
- No global gallery, artifact container, database, or object storage layer is
  introduced.
