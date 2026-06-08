# Exact Media Runtime V1 Design

Date: 2026-06-08

## Summary

Puffer should replace the split image/video media settings and generation paths
with a single exact media runtime for image and video generation. The runtime is
descriptor driven, adapter executed, and job/artifact backed.

The first version supports only `generate` operations for `image` and `video`.
It intentionally does not add a workflow engine, arbitrary request-template DSL,
provider-specific UI, audio generation, video editing, video extension, or
natural-language media intent routing.

## Goals

- Support every connected provider that declares executable image or video
  generation capabilities.
- Store image and video defaults with the same typed selection contract.
- Render settings UI from capability parameters instead of hard-coded fields.
- Normalize sync image APIs and async video APIs into one job/artifact flow.
- Keep provider-specific complexity inside adapters.
- Make stale settings and unsupported parameter values fail predictably.
- Keep runtime performance bounded with capability caching and lazy artifact
  loading.

## Non-Goals

- No backward-compatible config shape preservation.
- No session-scoped media overrides in this version.
- No provider-specific settings components.
- No generic dynamic form builder beyond simple capability parameter rendering.
- No request-template execution in provider YAML.
- No DAG/workflow engine for media jobs.
- No fake progress percentages when a provider does not return real progress.
- No generated media bytes embedded directly in transcript events.

## Current Problems

The current image path is closer to the desired architecture than video, but the
contracts are still split:

- image settings persist provider id, model id, adapter, and parameters;
- video settings persist provider id, model id, aspect ratio, and duration;
- image parameters are capability driven;
- video settings are partly hard-coded in the desktop UI;
- image generation can be modeled as direct execution;
- most production video providers require submit, poll, download, and persist.

This split makes video provider onboarding fragile. Each new provider would
either need UI special cases or a growing list of hand-mapped video fields. The
long-term runtime should instead make image and video variants of the same
selection, validation, execution, and artifact lifecycle.

## Unified Settings Contract

Replace the current split media config with one selection type per media kind:

```ts
type MediaSettings = {
  image: MediaGenerationSelection | null;
  video: MediaGenerationSelection | null;
};

type MediaGenerationSelection = {
  providerId: string;
  modelId: string;
  kind: "image" | "video";
  operation: "generate";
  adapter: string;
  parameters: Record<string, string>;
};
```

Rules:

- `providerId`, `modelId`, `kind`, `operation`, and `adapter` identify the exact
  executable capability.
- `parameters` stores only capability parameter names and string values.
- Duration values are saved as capability values such as `"8"`, not as a
  separate number field.
- Display formatting belongs to UI helpers, not persisted config.
- `null` means no default is configured for that media kind.

This removes image-only and video-only config fields. It also avoids adding a
parallel video `parameters` bag while keeping legacy `aspectRatio` and
`durationSeconds`.

## Capability Contract

All selectable media models are represented by the same capability DTO:

```ts
type MediaCapabilityInfo = {
  providerId: string;
  providerDisplayName: string;
  modelId: string;
  modelDisplayName: string;
  kind: "image" | "video";
  operation: "generate";
  adapter: string;
  parameters: MediaCapabilityParameterInfo[];
  defaults: Record<string, string>;
  status: "available" | "unavailable" | "unknown";
  source: string;
  reason: string | null;
  checkedAtMs: number;
};

type MediaCapabilityParameterInfo = {
  name: string;
  label: string;
  values: string[];
  default: string;
  requestField: string | null;
};
```

Rules:

- Only `status == "available"` capabilities are selectable or executable.
- Capability identity is provider id, model id, kind, operation, and adapter.
- Parameter values are exact strings allowed by the adapter.
- Empty `values` means the parameter is fixed at `default`.
- Descriptor metadata may declare static capabilities, but the resolver is the
  authority for availability.
- Diagnostic unavailable capabilities may be returned in verbose tooling, but
  normal settings UI filters to available capabilities.

## Provider Descriptor Model

Provider descriptors may declare media support under a shared media branch:

```yaml
media:
  image:
    models:
      - id: gpt-image-1
        display_name: GPT Image 1
        operation: generate
        adapter: openai_images
        parameters:
          size: ["1024x1024", "1024x1536", "1536x1024"]
          quality: ["auto", "low", "medium", "high"]
          output_format: ["png", "jpeg", "webp"]
  video:
    models:
      - id: runway-gen-4
        display_name: Gen-4
        operation: generate
        adapter: runway_video
        parameters:
          aspect_ratio: ["16:9", "9:16"]
          duration: ["5", "10"]
```

Descriptor rules:

- Model ids must be concrete. Empty ids, `auto`, wildcards, and regexes are
  invalid.
- Adapter names must match implemented runtime adapters.
- Descriptor parameters are declarative constraints, not executable templates.
- Auth, base URL, headers, and provider provenance continue to come from the
  existing provider registry and auth store.
- Dynamic discovery can supplement descriptors only when the adapter can parse
  structured provider capability data.

## Capability Resolution

The resolver input is:

```text
ProviderRegistry + AuthStore + MediaKind + operation + descriptor metadata + discovery cache
```

Resolution:

```text
for provider in registered providers:
  if provider is not connected and not explicitly auth-free:
    emit diagnostic unavailable reason, skip selectable capability
  for descriptor model matching kind and operation:
    if model id is invalid:
      skip
    if adapter is not implemented:
      skip
    if parameters are invalid for adapter:
      skip
    emit available capability
```

The daemon caches capability results by provider auth/config fingerprint, media
kind, and operation. Cache invalidates when auth, provider descriptor, base URL,
or relevant provider settings change. The desktop UI requests only the current
kind when a settings modal opens.

## Runtime Boundary

The runtime should expose a small internal contract:

```rust
trait MediaProviderAdapter {
    fn capabilities(&self, context: MediaProviderContext) -> Vec<MediaCapability>;
    async fn submit_generate(&self, request: MediaGenerateRequest) -> MediaJobUpdate;
    async fn poll_job(&self, job: MediaJob) -> MediaJobUpdate;
    async fn download_artifacts(&self, job: MediaJob) -> Vec<MediaArtifact>;
}
```

Sync image APIs are represented as jobs that complete immediately after
`submit_generate`. Async video APIs use submit, poll, and download. The UI and
transcript only observe unified job and artifact states.

The adapter owns:

- provider request shape;
- request field mapping through `requestField`;
- provider response parsing;
- async job id extraction;
- artifact URL expiry handling;
- provider-specific error redaction.

The shared runtime owns:

- selection validation;
- job persistence;
- polling schedule;
- artifact persistence;
- transcript-facing metadata;
- cancellation where adapters support it.

## Job And Artifact Contract

Jobs are sidecar state, not transcript content:

```ts
type MediaJob = {
  jobId: string;
  kind: "image" | "video";
  providerId: string;
  modelId: string;
  operation: "generate";
  adapter: string;
  status: "queued" | "running" | "completed" | "failed" | "canceling" | "canceled";
  prompt: string;
  parameters: Record<string, string>;
  providerJobId: string | null;
  progress: number | null;
  error: string | null;
  createdAtMs: number;
  updatedAtMs: number;
};
```

Artifacts are persisted locally:

```ts
type MediaArtifact = {
  artifactId: string;
  jobId: string;
  kind: "image" | "video";
  path: string;
  mimeType: string;
  size: number;
  index: number;
  remoteSourceUrl: string | null;
};
```

Image artifacts are stored under `.puffer/media/images`. Video artifacts are
stored under `.puffer/media/videos`. Transcript events and attachments reference
artifact ids or local paths. Provider URLs are treated as temporary unless an
adapter explicitly marks them durable.

## Desktop UI Design

`MediaSettingsModal` becomes kind-generic:

- The title remains `Image generation settings` or `Video generation settings`.
- The primary button label is always `Save`.
- Loading uses the same status block for image and video:
  - `Loading image capabilities...`
  - `Checking available image generation models.`
  - `Loading video capabilities...`
  - `Checking available video generation models.`
- Provider and model controls are rendered from available capabilities.
- Parameter controls are rendered from `capability.parameters`.
- Parameters with more than one value render as selects.
- Parameters with exactly one value or fixed defaults render read-only rows.
- Stale saved selections show an unavailable warning and cannot save until
  changed.

The modal never knows that video has aspect ratio or duration. Those are just
capability parameters with labels and values.

Generated media loading UI should use one attachment loading surface for image
and video. The surface may show kind-specific labels, but it uses the same
spinner, failure, retry, and artifact preview behavior. Video shows a real
progress indicator only when the job update includes provider progress.

## Error Handling

Validation runs twice:

- on settings save, to clamp or reject unsupported parameter values;
- on generation start, to prevent stale config from executing.

Error behavior:

- No configured media default: tell the user to configure that media kind.
- No available capability: show the no-capability state in settings and reject
  generation.
- Stale saved provider/model/adapter: show unavailable saved model warning.
- Unsupported saved parameter: reset to the capability default during settings
  save, and reject during generation if it reaches runtime.
- Provider request failure: surface a redacted adapter error.
- Artifact download failure: fail the job unless at least one artifact was
  persisted and the adapter declares partial success support.

## Performance And Stability

- Capability resolution is lazy and kind-scoped.
- Resolved capabilities are cached in the daemon, not in each Svelte component.
- UI does not scan all providers when opening a modal; it calls
  `list_media_capabilities({ kind })`.
- Polling uses bounded backoff and stops on terminal job states.
- Artifact previews are lazy loaded and size-aware.
- Video files are never loaded into memory for transcript rendering.
- Adapter tests cover request construction and response parsing without live
  provider calls.

## Testing Strategy

Unit tests:

- provider descriptor validation rejects invalid media models;
- capability resolver emits only exact available capabilities;
- settings validation accepts only capability parameter values;
- adapter request mapping sends selected model and parameters;
- sync image adapter jobs complete immediately;
- async video adapter jobs move through queued/running/completed states.

Desktop tests:

- image and video settings show matching loading UI;
- `Save` label is stable for both kinds;
- single-value parameters render read-only;
- multi-value parameters render selects and persist selected values;
- video settings do not hard-code aspect ratio or duration;
- stale selections show warnings and disabled save behavior.

Integration tests:

- `/image` and `/video` generation both create jobs and persisted artifacts;
- generated image and video attachments reference local artifact metadata;
- provider URL outputs are downloaded before completed jobs are exposed.

## Implementation Slices

1. Replace media settings shape with `MediaGenerationSelection`.
2. Update desktop fake daemon and frontend types to the unified shape.
3. Convert `MediaSettingsModal` to parameter-driven rendering for both kinds.
4. Update capability resolver identity to include kind, operation, and adapter.
5. Introduce the shared media job contract for image and video.
6. Wrap current image execution as immediate-complete jobs.
7. Add the first video adapter using submit, poll, download, and persist.
8. Align generated media attachment loading UI for image and video.
9. Add provider descriptors for each implemented adapter only.

These slices should land incrementally with tests in the same step. Provider
coverage should grow only when an adapter can execute that provider's declared
capability.
