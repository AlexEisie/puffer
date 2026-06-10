# BytePlus Video References Design

## Context

Puffer's current video generation tool is prompt-only. The BytePlus video
adapter already uses the ModelArk task API shape, but it hardcodes
`content` to a single text item. Relaydance is configured through a separate
OpenAI-style video endpoint and currently builds only `prompt`, `seconds`,
and `metadata`.

BytePlus Seedance supports video generation from text plus image references.
VPL and real-human assets use the same content path as image-to-video, with
the image URL set to an approved `asset://...` reference instead of a public
HTTPS URL. Public image-to-video uses the same `image_url` item shape with an
HTTPS URL.

The design does not preserve the prompt-only input schema for compatibility.
It optimizes for clear ownership, stable validation, and future staging
support without turning the media runtime into a generic multimodal framework.

## Goals

- Add typed video references for BytePlus video generation.
- Support public image URLs and BytePlus `asset://` references in the first
  implementation.
- Represent local image references in the input model, but require a staging
  backend before they can be submitted to BytePlus.
- Keep Relaydance prompt-only until Relaydance documents or proves support for
  reference assets.
- Keep reference inputs separate from model parameters.
- Avoid downloading public images or inlining base64 data in generation
  requests.

## Non-Goals

- No generic cross-provider multimodal media framework.
- No Relaydance reference support without provider-specific documentation or a
  successful probe.
- No base64 image submission in video generation requests.
- No first implementation of object storage or asset staging.
- No video or audio references until a provider capability requires them.

## Input Model

Replace the current prompt-only tool input with a typed reference model:

```rust
pub struct VideoGenerationInput {
    pub prompt: String,
    pub references: Vec<VideoReference>,
    pub parameters: BTreeMap<String, String>,
    pub purpose: Option<String>,
}

pub struct VideoReference {
    pub kind: VideoReferenceKind,
    pub source: VideoReferenceSource,
    pub role: VideoReferenceRole,
}

pub enum VideoReferenceKind {
    Image,
}

pub enum VideoReferenceSource {
    Url(String),
    Asset(String),
    LocalPath(String),
}

pub enum VideoReferenceRole {
    ReferenceImage,
    FirstFrame,
    LastFrame,
}
```

Only `Image` is included initially. Adding `Video` or `Audio` later should be
a narrow extension to this enum and the provider capability table, not a new
tool shape.

The public JSON shape should be explicit and provider-neutral at the tool
boundary:

```json
{
  "prompt": "Make image 1 wave at the camera.",
  "references": [
    {
      "kind": "image",
      "source": "url",
      "url": "https://example.com/person.jpg",
      "role": "reference_image"
    }
  ],
  "parameters": {
    "duration_seconds": "5",
    "resolution": "720p",
    "aspect_ratio": "9:16"
  }
}
```

Asset-backed VPL uses the same reference item with `source: "asset"` and an
`asset_url` value such as `asset://approved-person-asset-id`.

## Reference Resolution

Add a small `VideoReferenceResolver` before provider request construction. Its
output is a list of resolved references where each item has:

- `kind`
- `role`
- a provider-usable URL string

Resolution rules:

- `Url`
  - Must use `https`.
  - Must parse as a URL.
  - Is not fetched, downloaded, or probed.
- `Asset`
  - Must use the `asset://` scheme.
  - Is not interpreted beyond scheme validation.
  - Provider-side ownership and review failures remain provider errors.
- `LocalPath`
  - Must resolve inside the current workspace.
  - Must point to a regular file.
  - Must have an allowed image extension: `jpg`, `jpeg`, `png`, or `webp`.
  - Requires a configured staging backend before submission.
  - Without staging, fails with a clear error:
    `local video references require a configured media staging backend`.

The resolver is the only layer that knows about local files. Provider adapters
only receive HTTPS URLs or `asset://` URLs.

## BytePlus Request Construction

Change `BytePlusVideoRequest` to hold typed content items instead of a plain
prompt:

```rust
pub struct BytePlusVideoRequest {
    pub model: String,
    pub content: Vec<BytePlusContentItem>,
    pub params: Vec<(String, Value)>,
}
```

For text-only generation, build the same single text item as today:

```json
{
  "type": "text",
  "text": "A cinematic city at sunrise"
}
```

For image references, append one `image_url` item per resolved reference:

```json
{
  "type": "image_url",
  "image_url": {
    "url": "https://example.com/person.jpg"
  },
  "role": "reference_image"
}
```

VPL uses the same item shape with `asset://...` as the URL:

```json
{
  "type": "image_url",
  "image_url": {
    "url": "asset://approved-person-asset-id"
  },
  "role": "reference_image"
}
```

The BytePlus request body remains:

```json
{
  "model": "...",
  "content": [
    { "type": "text", "text": "..." },
    {
      "type": "image_url",
      "image_url": { "url": "https://... or asset://..." },
      "role": "reference_image"
    }
  ],
  "duration": 5,
  "ratio": "9:16",
  "resolution": "720p",
  "generate_audio": false
}
```

`generate_audio` should keep the current conservative default of `false`,
with provider parameters allowed to override it later if the descriptor
declares that parameter.

## Relaydance Behavior

Relaydance remains prompt-only. If the selected provider is Relaydance and
`references` is non-empty, request building fails before any HTTP call:

```text
provider relaydance does not support video references
```

This avoids silently dropping references and avoids assuming Relaydance shares
BytePlus asset namespaces or content semantics.

## Error Handling

Errors should fail as early as possible:

- Empty prompt: tool input validation.
- Unknown reference fields: deserialization validation.
- Invalid `kind`, `source`, or `role`: deserialization validation.
- Invalid HTTPS URL or `asset://` URL: reference resolution.
- Unsafe or missing local file: reference resolution.
- Missing staging backend for local files: reference resolution.
- Unsupported provider reference capability: provider request building.
- BytePlus moderation or generation failure: provider response handling with
  provider, model, adapter, and task context.

Provider errors should preserve the provider message while adding enough
Puffer context for debugging.

## Performance and Stability

- Do not download public image URLs for validation.
- Do not inline local files as base64.
- Keep local file staging outside the provider adapter.
- Keep request construction deterministic and allocation-light.
- Cache staged local files in a future staging backend by file identity and
  content digest, not by prompt.
- Keep reference ordering stable because prompts refer to `image 1`,
  `image 2`, and similar indexes.

## Tests

Unit coverage:

- `VideoGenerationInput` accepts typed references.
- Unknown input fields are rejected.
- Empty prompt is rejected.
- Invalid reference kind, source, and role are rejected.
- HTTPS URL references resolve without network access.
- HTTP URL references are rejected.
- `asset://` references resolve.
- Local paths outside the workspace are rejected.
- Missing local files are rejected.
- Local files without staging fail with the configured staging error.
- BytePlus text-only requests still produce one text content item.
- BytePlus public image references serialize to `content[].image_url.url`.
- BytePlus asset references serialize to `asset://...`.
- BytePlus roles serialize as `reference_image`, `first_frame`, and
  `last_frame`.
- BytePlus numeric parameters such as `duration` remain numeric.
- Relaydance rejects non-empty references before submission.
- Relaydance prompt-only request bodies remain unchanged.

Integration-style fake HTTP coverage:

- BytePlus submit test server receives the expected `content[]`.
- Polling and download continue to use the existing video job flow.
- Failed provider responses include provider, adapter, and task context.

No real BytePlus or Relaydance calls are required for this change.

## Rollout Order

1. Replace the video generation tool schema with typed `references`.
2. Add `VideoReferenceResolver` with URL, asset, and local validation.
3. Change BytePlus request construction from prompt-only to typed content.
4. Reject references for Relaydance.
5. Add focused unit and fake HTTP tests.
6. Add a later staging backend design for true local image upload support.

The first implementation should support public URL and asset references end to
end, validate local images, and return a clear staging error for local images
until a staging backend exists.
