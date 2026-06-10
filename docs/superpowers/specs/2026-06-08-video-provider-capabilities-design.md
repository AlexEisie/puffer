# Video Provider Capabilities Design

Date: 2026-06-08

## Purpose

Puffer Desktop currently shows `No video capabilities available` because the
daemon returns zero available video capabilities when no connected provider has a
usable `media.video` descriptor. The first durable fix is to make declared video
capabilities visible even before authentication, while only allowing generation
from connected providers.

This spec intentionally narrows the previous design. It does not add new vendor
video adapters in this pass. A provider may advertise video in Puffer only when
the current runtime can execute that provider's declared adapter.

## Recheck Findings

- `resources/providers/relaydance.yaml` already exists and declares one video
  model, `doubao-seedance-2-0-720p`.
- `MediaExecutionKind::OpenAiVideo`, `resolve_video_execution_descriptor`, and
  the `openai_video` runtime module already exist in this checkout.
- The current `openai_video` adapter is an OpenAI-compatible/NewAPI-style async
  video protocol used by Relaydance: create at `/v1/video/generations`, poll by
  appending `/{task_id}`, and download `metadata.url`.
- The user-facing bug is the resolver behavior: `resolve_video_capabilities`
  skips unauthenticated providers before emitting any capability metadata.
- `MediaCapabilityInfo` already has `status` and `reason`, so no TypeScript API
  shape change is needed.

## Goals

- Audit every bundled provider and record whether Puffer should declare video
  capability now.
- Keep Relaydance as the only first-pass declared video provider because it is
  the only bundled provider with an implemented video execution path.
- Make unauthenticated declared video providers visible as
  `status = "unavailable"` with `reason = "missing_auth"`.
- Keep generation validation strict: only `status = "available"` capabilities
  can be saved or executed.
- Update the desktop video settings empty state so users see a connection prompt
  when video providers exist but are not connected.
- Add resource and resolver tests so future provider descriptors cannot expose
  non-executable video capabilities by accident.

## Non-Goals

- No new OpenAI, BytePlus, MiniMax, xAI, OpenRouter, Vercel, Zhipu, or
  WorldRouter video adapters in this pass.
- No dynamic video discovery on settings modal open.
- No new provider authentication flow inside the video settings modal.
- No image-to-video, first/last-frame, reference image, video edit, video
  extension, callbacks, or batch video jobs.
- No adapter rename in this pass. The existing `openai_video` name is imperfect,
  but renaming it does not fix the capability visibility bug and would add churn.

## Provider Audit

| Provider | First-pass video declaration | Reason |
| --- | --- | --- |
| `anthropic` | No | No video generation provider descriptor or implemented Puffer adapter. |
| `byteplus` | No | BytePlus has documented Seedance video APIs, but Puffer has no BytePlus video adapter yet. |
| `cerebras` | No | Text inference provider only. |
| `groq` | No | Text inference provider only. |
| `kimi-coding` | No | Text/coding provider only. |
| `kimi-openai` | No | Text/coding provider only. |
| `llama-cpp` | No | Local text inference provider only. |
| `lmstudio` | No | Local text inference provider only. |
| `minicpm5` | No | Local text inference provider only. |
| `minimax` | No | MiniMax has documented video APIs, but Puffer only implements MiniMax image generation. |
| `minimax-cn` | No | Same as `minimax`; no Puffer video adapter yet. |
| `ollama` | No | Local text inference provider only. |
| `openai` | No | OpenAI's official `/v1/videos` protocol differs from the current `openai_video` adapter. |
| `openrouter` | No | OpenRouter has documented video APIs, but Puffer has no OpenRouter video adapter yet. |
| `relaydance` | Yes | Already bundled and executable through the current `openai_video` adapter. |
| `vercel-ai-gateway` | No | Vercel documents video through AI SDK v6 `experimental_generateVideo`; no stable Rust REST adapter exists in Puffer. |
| `vllm` | No | Local text inference provider only. |
| `worldrouter` | No | No verified Puffer video execution path. |
| `xai` | No | xAI has documented video APIs, but Puffer has no xAI video adapter yet. |
| `zhipu` | No | Zhipu has documented async video APIs, but Puffer has no Zhipu video adapter yet. |

Future video provider additions must land as one coherent change: adapter,
descriptor, resolver tests, fake-transport runtime tests, and UI behavior.

## Capability Semantics

For video capabilities, resolver output should include static descriptors even
when the provider is not authenticated:

- `status = "available"` when the provider is connected and the adapter is
  executable for `MediaKind::Video`.
- `status = "unavailable"` and `reason = "missing_auth"` when the provider has
  video descriptors but no stored credential.
- `status = "unavailable"` and `reason = "adapter_unavailable"` when the
  descriptor uses an adapter that is not executable for video.

This first pass changes video capability resolution only. Image capabilities
keep their existing connected-provider behavior to avoid surprising image
settings changes.

`validate_media_generate_selection` must reject any matching capability whose
status is not `available`. This preserves the existing safety contract for
`generate_media`.

## Descriptor Governance

Add tests that assert:

- `relaydance.yaml` parses and validates.
- Relaydance declares `media.video.execution.adapter = openai_video`.
- Relaydance declares `/v1/video/generations`.
- Relaydance declares the expected Seedance model and the bounded parameters
  used by the UI: duration, resolution, and ratio.
- No other bundled provider declares `media.video` in this first pass.

The last assertion is intentional. It prevents partially declared vendor video
support from appearing before runtime execution exists.

## Desktop Behavior

The video settings modal should distinguish three states after loading:

- Available video capabilities exist: render the existing provider/model form.
- Only unavailable video capabilities exist: show a concise connection prompt,
  such as "Connect Relaydance to enable video generation."
- No declared video capabilities exist: show the true empty state,
  `No video capabilities available.`

The form should still derive provider and model options only from
`status = "available"` capabilities. The save button remains unreachable when
there is no available selection.

## Runtime Flow

1. `list_media_capabilities({ kind: "video" })` loads provider descriptors and
   auth state.
2. The resolver emits the Relaydance video capability as unavailable when
   Relaydance has no stored API key.
3. The desktop modal shows the connect prompt instead of the old empty state.
4. After a Relaydance credential is stored, the same capability becomes
   available.
5. The user can save the available Relaydance model.
6. `generate_media` validates the saved selection against available video
   capabilities and routes generation through the existing `openai_video`
   adapter.

## Stability and Performance

- Capability listing remains local and static; it must not call provider APIs.
- The resolver does not allocate network clients or perform model discovery for
  video.
- The UI does not add a new authentication workflow; it points users to connect
  an existing provider.
- Existing video generation polling and download behavior remain unchanged.
- Tests use local descriptors and fake resolver inputs only.

## Tests

Add or update focused tests for:

- Unauthenticated Relaydance-style video descriptors appear with
  `status = "unavailable"` and `reason = "missing_auth"`.
- Connected video descriptors appear with `status = "available"` and no reason.
- Video descriptors with image-only adapters appear as unavailable with
  `adapter_unavailable`, not as selectable.
- `validate_media_generate_selection` rejects unavailable matching capabilities.
- Relaydance provider YAML is the only first-pass bundled `media.video`
  declaration.
- The desktop modal renders the connection state when capabilities exist but no
  available video capability exists.

## Source References

- Current Relaydance provider descriptor:
  `resources/providers/relaydance.yaml`
- Current resolver:
  `crates/puffer-core/runtime/media/resolver.rs`
- Current video runtime adapter:
  `crates/puffer-core/runtime/media/openai_video.rs`
- Current desktop media settings modal:
  `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
- OpenAI Videos API, future adapter only:
  https://developers.openai.com/api/reference/resources/videos/
- xAI video REST API, future adapter only:
  https://docs.x.ai/developers/rest-api-reference/inference/videos
- OpenRouter video API, future adapter only:
  https://openrouter.ai/docs/api/api-reference/video-generation/create-videos
- MiniMax video generation, future adapter only:
  https://platform.minimax.io/docs/guides/video-generation
- BytePlus Seedance 2.0, future adapter only:
  https://docs.byteplus.com/en/docs/ModelArk/1520757
- Zhipu async video API, future adapter only:
  https://docs.bigmodel.cn/api-reference/%E6%A8%A1%E5%9E%8B-api/%E8%A7%86%E9%A2%91%E7%94%9F%E6%88%90%E5%BC%82%E6%AD%A5
- Vercel AI Gateway video docs, future adapter only:
  https://vercel.com/docs/ai-gateway/capabilities/video-generation
