# Video Provider Capabilities Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Puffer Desktop show a useful video-provider connection state for Relaydance when no video provider is authenticated, while keeping generation restricted to executable connected capabilities.

**Architecture:** Keep the existing Relaydance provider and `openai_video` runtime adapter. Change video capability resolution to emit unavailable static descriptors with reasons, add descriptor governance tests so only executable video providers can be declared, and update the settings modal through a small tested TypeScript helper.

**Tech Stack:** Rust, serde/serde_yaml, existing `MediaCapability` resolver, Svelte 5, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-06-08-video-provider-capabilities-design.md`

---

## File Structure

- Modify: `crates/puffer-core/runtime/media/resolver.rs`
  - Emits unavailable video capabilities.
  - Rejects unavailable matching selections during generation validation.
- Modify: `crates/puffer-resources/tests/image_catalog_governance.rs`
  - Adds provider-level governance for first-pass video descriptors.
- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.ts`
  - Pure UI helper for available capabilities and video connection state copy.
- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts`
  - Vitest coverage for helper behavior.
- Modify: `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`
  - Uses the helper and renders a connect state before the true empty state.

---

## Task 1: Emit Unavailable Video Capabilities

**Files:**
- Modify: `crates/puffer-core/runtime/media/resolver.rs`

- [ ] **Step 1: Replace the image-adapter video test with the new expected behavior**

In `crates/puffer-core/runtime/media/resolver.rs`, replace the existing
`video_descriptor_with_image_adapter_is_not_available` test with:

```rust
#[test]
fn video_descriptor_with_image_adapter_is_unavailable_with_adapter_reason() {
    let registry = registry_with(vec![provider(
        "replicate",
        vec![AuthMode::ApiKey],
        Some(video_media_with_adapter(
            MediaExecutionKind::ImagesJson,
            "owner/model-version",
        )),
    )]);
    let auth = auth_for("replicate");

    let capabilities = resolve_media_capabilities(
        &registry,
        &auth,
        MediaKind::Video,
        MediaOperation::Generate,
        42,
        &MediaDiscoveryCache::default(),
    );

    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].provider_id, "replicate");
    assert_eq!(capabilities[0].adapter, "images_json");
    assert_eq!(capabilities[0].status, "unavailable");
    assert_eq!(
        capabilities[0].reason.as_deref(),
        Some("adapter_unavailable")
    );
}
```

- [ ] **Step 2: Add missing-auth and connected-video tests**

Add these tests in the same resolver test module, near the existing video tests:

```rust
#[test]
fn unauthenticated_video_descriptor_appears_unavailable_with_missing_auth() {
    let registry = registry_with(vec![provider(
        "relaydance",
        vec![AuthMode::ApiKey],
        Some(video_media_with_adapter(
            MediaExecutionKind::OpenAiVideo,
            "doubao-seedance-2-0-720p",
        )),
    )]);

    let capabilities = resolve_media_capabilities(
        &registry,
        &AuthStore::default(),
        MediaKind::Video,
        MediaOperation::Generate,
        42,
        &MediaDiscoveryCache::default(),
    );

    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].provider_id, "relaydance");
    assert_eq!(capabilities[0].adapter, "openai_video");
    assert_eq!(capabilities[0].status, "unavailable");
    assert_eq!(capabilities[0].reason.as_deref(), Some("missing_auth"));
    assert_eq!(capabilities[0].defaults["duration"], "5");
}

#[test]
fn connected_openai_video_descriptor_is_available() {
    let registry = registry_with(vec![provider(
        "relaydance",
        vec![AuthMode::ApiKey],
        Some(video_media_with_adapter(
            MediaExecutionKind::OpenAiVideo,
            "doubao-seedance-2-0-720p",
        )),
    )]);

    let capabilities = resolve_media_capabilities(
        &registry,
        &auth_for("relaydance"),
        MediaKind::Video,
        MediaOperation::Generate,
        42,
        &MediaDiscoveryCache::default(),
    );

    assert_eq!(capabilities.len(), 1);
    assert_eq!(capabilities[0].status, "available");
    assert_eq!(capabilities[0].reason, None);
}
```

- [ ] **Step 3: Run the focused tests and verify failure**

Run:

```bash
cargo test -p puffer-core video_descriptor_with_image_adapter_is_unavailable_with_adapter_reason
cargo test -p puffer-core unauthenticated_video_descriptor_appears_unavailable_with_missing_auth
cargo test -p puffer-core connected_openai_video_descriptor_is_available
```

Expected: the first two fail because the resolver still hides unauthenticated
providers and drops video descriptors whose adapter is not executable. The
connected-provider test passes.

- [ ] **Step 4: Replace `resolve_video_capabilities`**

Replace the whole `resolve_video_capabilities` function with:

```rust
fn resolve_video_capabilities(
    registry: &ProviderRegistry,
    auth_store: &AuthStore,
    operation: MediaOperation,
    checked_at_ms: u64,
) -> Vec<MediaCapability> {
    let mut capabilities = Vec::new();
    for provider in registry.providers() {
        let Some(video) = provider
            .media
            .as_ref()
            .and_then(|media| media.video.as_ref())
        else {
            continue;
        };
        let provider_connected = provider_is_connected(provider, auth_store);
        for model in &video.models {
            if !media_model_is_available(model, operation) {
                continue;
            }
            let Some(execution) = image_execution(video.execution.as_ref(), model) else {
                continue;
            };
            let adapter_available =
                execution_adapter_is_available_for_kind(MediaKind::Video, execution.adapter);
            let (status, reason) = if !adapter_available {
                ("unavailable", Some("adapter_unavailable".to_string()))
            } else if !provider_connected {
                ("unavailable", Some("missing_auth".to_string()))
            } else {
                ("available", None)
            };
            let parameters = media_parameters(model);
            capabilities.push(MediaCapability {
                provider_id: provider.id.clone(),
                provider_display_name: provider.display_name.clone(),
                model_id: model.id.clone(),
                model_display_name: model
                    .display_name
                    .clone()
                    .unwrap_or_else(|| model.id.clone()),
                kind: MediaKind::Video,
                operation: operation_wire_name(operation).to_string(),
                adapter: adapter_id(execution.adapter).to_string(),
                defaults: media_defaults(&parameters),
                parameters,
                status: status.to_string(),
                source: "static".to_string(),
                reason,
                checked_at_ms,
            });
        }
    }
    capabilities
}
```

- [ ] **Step 5: Add the unavailable-validation test**

Add this test in the same resolver test module:

```rust
#[test]
fn unavailable_video_capability_cannot_validate_generation_selection() {
    let registry = registry_with(vec![provider(
        "relaydance",
        vec![AuthMode::ApiKey],
        Some(video_media_with_adapter(
            MediaExecutionKind::OpenAiVideo,
            "doubao-seedance-2-0-720p",
        )),
    )]);
    let selected = BTreeMap::new();

    let error = validate_media_generate_selection(
        &registry,
        &AuthStore::default(),
        &MediaGenerationSelection {
            kind: MediaKind::Video,
            provider_id: "relaydance",
            model_id: "doubao-seedance-2-0-720p",
            operation: MediaOperation::Generate,
            adapter: "openai_video",
            parameters: &selected,
        },
        42,
        &MediaDiscoveryCache::default(),
    )
    .unwrap_err()
    .to_string();

    assert!(
        error.contains(
            "selected video model unavailable: relaydance/doubao-seedance-2-0-720p via openai_video"
        ),
        "{error}"
    );
}
```

- [ ] **Step 6: Run the unavailable-validation test and verify failure**

Run:

```bash
cargo test -p puffer-core unavailable_video_capability_cannot_validate_generation_selection
```

Expected: FAIL because the matching unavailable capability is returned and
validation has not rejected non-available status yet.

- [ ] **Step 7: Update `validate_media_generate_selection`**

In `validate_media_generate_selection`, immediately after the `let Some(capability) = capability else { ... };`
block and before `validate_parameter_values(...)`, add:

```rust
if capability.status != "available" {
    bail!(
        "selected {} model unavailable: {}/{} via {}",
        media_kind_error_name(selection.kind),
        selection.provider_id,
        selection.model_id,
        selection.adapter
    );
}
```

- [ ] **Step 8: Run resolver tests**

Run:

```bash
cargo test -p puffer-core video_descriptor_with_image_adapter_is_unavailable_with_adapter_reason
cargo test -p puffer-core unauthenticated_video_descriptor_appears_unavailable_with_missing_auth
cargo test -p puffer-core connected_openai_video_descriptor_is_available
cargo test -p puffer-core unavailable_video_capability_cannot_validate_generation_selection
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add crates/puffer-core/runtime/media/resolver.rs
git commit -m "fix(media): expose unavailable video capabilities"
```

---

## Task 2: Add Video Descriptor Governance

**Files:**
- Modify: `crates/puffer-resources/tests/image_catalog_governance.rs`

- [ ] **Step 1: Add all provider YAMLs to the governance test**

Below `IMAGE_PROVIDER_YAMLS`, add:

```rust
const ALL_PROVIDER_YAMLS: &[(&str, &str)] = &[
    (
        "anthropic",
        include_str!("../../../resources/providers/anthropic.yaml"),
    ),
    (
        "byteplus",
        include_str!("../../../resources/providers/byteplus.yaml"),
    ),
    (
        "cerebras",
        include_str!("../../../resources/providers/cerebras.yaml"),
    ),
    ("groq", include_str!("../../../resources/providers/groq.yaml")),
    (
        "kimi-coding",
        include_str!("../../../resources/providers/kimi-coding.yaml"),
    ),
    (
        "kimi-openai",
        include_str!("../../../resources/providers/kimi-openai.yaml"),
    ),
    (
        "llama-cpp",
        include_str!("../../../resources/providers/llama-cpp.yaml"),
    ),
    (
        "lmstudio",
        include_str!("../../../resources/providers/lmstudio.yaml"),
    ),
    (
        "minicpm5",
        include_str!("../../../resources/providers/minicpm5.yaml"),
    ),
    (
        "minimax",
        include_str!("../../../resources/providers/minimax.yaml"),
    ),
    (
        "minimax-cn",
        include_str!("../../../resources/providers/minimax-cn.yaml"),
    ),
    ("ollama", include_str!("../../../resources/providers/ollama.yaml")),
    (
        "openai",
        include_str!("../../../resources/providers/openai.yaml"),
    ),
    (
        "openrouter",
        include_str!("../../../resources/providers/openrouter.yaml"),
    ),
    (
        "relaydance",
        include_str!("../../../resources/providers/relaydance.yaml"),
    ),
    (
        "vercel-ai-gateway",
        include_str!("../../../resources/providers/vercel-ai-gateway.yaml"),
    ),
    ("vllm", include_str!("../../../resources/providers/vllm.yaml")),
    (
        "worldrouter",
        include_str!("../../../resources/providers/worldrouter.yaml"),
    ),
    ("xai", include_str!("../../../resources/providers/xai.yaml")),
    ("zhipu", include_str!("../../../resources/providers/zhipu.yaml")),
];
```

- [ ] **Step 2: Add Relaydance descriptor tests**

Add these tests near the image catalog governance tests:

```rust
#[test]
fn relaydance_declares_executable_video_descriptor() {
    let descriptor = provider_descriptor(
        "relaydance",
        include_str!("../../../resources/providers/relaydance.yaml"),
    );
    descriptor
        .validate_media_descriptors()
        .expect("relaydance media descriptor validates");
    let video = descriptor
        .media
        .as_ref()
        .and_then(|media| media.video.as_ref())
        .expect("relaydance video media descriptor");
    let execution = video
        .execution
        .as_ref()
        .expect("relaydance video execution descriptor");

    assert_eq!(execution.adapter, MediaExecutionKind::OpenAiVideo);
    assert_eq!(execution.path, "/v1/video/generations");

    let model = video
        .models
        .iter()
        .find(|model| model.id == "doubao-seedance-2-0-720p")
        .expect("relaydance should include Seedance 2.0 720p");
    assert_eq!(model.display_name.as_deref(), Some("Seedance 2.0 (720p)"));
    assert_eq!(model.operations, vec![MediaOperation::Generate]);

    let parameters = model
        .parameters
        .iter()
        .map(|parameter| {
            (
                parameter.name.as_str(),
                parameter.default.as_str(),
                parameter.request_field.as_deref(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(parameters.get("duration"), Some(&("5", Some("seconds"))));
    assert_eq!(
        parameters.get("resolution"),
        Some(&("720p", Some("metadata.resolution")))
    );
    assert_eq!(
        parameters.get("ratio"),
        Some(&("16:9", Some("metadata.ratio")))
    );
}

#[test]
fn only_relaydance_declares_first_pass_video_media() {
    for (provider_id, yaml) in ALL_PROVIDER_YAMLS {
        let descriptor = provider_descriptor(provider_id, yaml);
        let has_video = descriptor
            .media
            .as_ref()
            .and_then(|media| media.video.as_ref())
            .is_some();
        assert_eq!(
            has_video,
            *provider_id == "relaydance",
            "{provider_id} must not declare media.video until a Puffer video adapter exists"
        );
    }
}
```

- [ ] **Step 3: Run the governance tests**

Run:

```bash
cargo test -p puffer-resources relaydance_declares_executable_video_descriptor
cargo test -p puffer-resources only_relaydance_declares_first_pass_video_media
```

Expected: both pass.

- [ ] **Step 4: Commit**

```bash
git add crates/puffer-resources/tests/image_catalog_governance.rs
git commit -m "test(resources): govern video provider descriptors"
```

---

## Task 3: Add a Tested Desktop Capability State Helper

**Files:**
- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.ts`
- Create: `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts`

- [ ] **Step 1: Create the helper tests**

Create `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts`:

```ts
import { expect, test } from "vitest";
import type { MediaCapabilityInfo, MediaKind } from "../../types";
import {
  availableMediaCapabilities,
  mediaCapabilityConnectStateMessage,
  unavailableMediaProviderLabels
} from "./mediaCapabilityState";

function capability(overrides: Partial<MediaCapabilityInfo> = {}): MediaCapabilityInfo {
  return {
    providerId: "relaydance",
    providerDisplayName: "Relaydance",
    modelId: "doubao-seedance-2-0-720p",
    modelDisplayName: "Seedance 2.0 (720p)",
    kind: "video",
    operation: "generate",
    adapter: "openai_video",
    parameters: [],
    defaults: {},
    status: "unavailable",
    source: "static",
    reason: "missing_auth",
    checkedAtMs: 42,
    ...overrides
  };
}

test("availableMediaCapabilities filters by kind and available status", () => {
  const capabilities = [
    capability({ status: "available", reason: null }),
    capability({ kind: "image" as MediaKind, status: "available", reason: null }),
    capability({ providerId: "xai", status: "unavailable", reason: "missing_auth" })
  ];

  expect(availableMediaCapabilities(capabilities, "video")).toEqual([
    capability({ status: "available", reason: null })
  ]);
});

test("unavailableMediaProviderLabels deduplicates provider display names", () => {
  const capabilities = [
    capability(),
    capability({ modelId: "another-model" }),
    capability({ providerId: "xai", providerDisplayName: "", reason: "missing_auth" })
  ];

  expect(unavailableMediaProviderLabels(capabilities, "video")).toEqual(["Relaydance", "xai"]);
});

test("mediaCapabilityConnectStateMessage appears only for unavailable video providers", () => {
  expect(mediaCapabilityConnectStateMessage([capability()], "video")).toBe(
    "Connect Relaydance to enable video generation."
  );
  expect(
    mediaCapabilityConnectStateMessage(
      [capability(), capability({ providerId: "xai", providerDisplayName: "xAI" })],
      "video"
    )
  ).toBe("Connect Relaydance or xAI to enable video generation.");
  expect(
    mediaCapabilityConnectStateMessage([capability({ status: "available", reason: null })], "video")
  ).toBeNull();
  expect(mediaCapabilityConnectStateMessage([capability({ kind: "image" })], "image")).toBeNull();
});
```

- [ ] **Step 2: Run the test and verify failure**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/screens/agent/mediaCapabilityState.test.ts
```

Expected: FAIL because `mediaCapabilityState.ts` does not exist.

- [ ] **Step 3: Create the helper**

Create `apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.ts`:

```ts
import type { MediaCapabilityInfo, MediaKind } from "../../types";

export function availableMediaCapabilities(
  capabilities: MediaCapabilityInfo[],
  kind: MediaKind
): MediaCapabilityInfo[] {
  return capabilities.filter((capability) => capability.kind === kind && capability.status === "available");
}

export function unavailableMediaProviderLabels(
  capabilities: MediaCapabilityInfo[],
  kind: MediaKind
): string[] {
  const labels: string[] = [];
  const seen = new Set<string>();
  for (const capability of capabilities) {
    if (capability.kind !== kind || capability.status === "available") continue;
    if (capability.reason !== "missing_auth") continue;
    if (seen.has(capability.providerId)) continue;
    seen.add(capability.providerId);
    labels.push(capability.providerDisplayName || capability.providerId);
  }
  return labels;
}

export function mediaCapabilityConnectStateMessage(
  capabilities: MediaCapabilityInfo[],
  kind: MediaKind
): string | null {
  if (kind !== "video") return null;
  if (availableMediaCapabilities(capabilities, kind).length > 0) return null;
  const labels = unavailableMediaProviderLabels(capabilities, kind);
  if (labels.length === 0) return null;
  return `Connect ${formatProviderList(labels)} to enable video generation.`;
}

function formatProviderList(labels: string[]): string {
  if (labels.length <= 1) return labels[0] ?? "";
  return `${labels.slice(0, -1).join(", ")} or ${labels[labels.length - 1]}`;
}
```

- [ ] **Step 4: Run the helper test**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/screens/agent/mediaCapabilityState.test.ts
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.ts apps/puffer-desktop/src/lib/screens/agent/mediaCapabilityState.test.ts
git commit -m "test(desktop): add media capability state helper"
```

---

## Task 4: Render the Video Connect State in the Modal

**Files:**
- Modify: `apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte`

- [ ] **Step 1: Import the helper**

Add this import below the existing `Icon` import:

```ts
import {
  availableMediaCapabilities,
  mediaCapabilityConnectStateMessage
} from "./mediaCapabilityState";
```

- [ ] **Step 2: Replace the `availableCapabilities` derived expression**

Replace:

```ts
let availableCapabilities = $derived(
  capabilities.filter((capability) => capability.kind === kind && capability.status === "available")
);
```

with:

```ts
let availableCapabilities = $derived(availableMediaCapabilities(capabilities, kind));
let connectStateMessage = $derived(mediaCapabilityConnectStateMessage(capabilities, kind));
```

- [ ] **Step 3: Render the connect state before the true empty state**

Replace:

```svelte
      {:else if !hasAvailableCapabilities}
        <p class="pf-media-state">No {kind} capabilities available.</p>
```

with:

```svelte
      {:else if !hasAvailableCapabilities && connectStateMessage}
        <p class="pf-media-state" data-warning="true">{connectStateMessage}</p>
      {:else if !hasAvailableCapabilities}
        <p class="pf-media-state">No {kind} capabilities available.</p>
```

- [ ] **Step 4: Run desktop typecheck and helper test**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/screens/agent/mediaCapabilityState.test.ts
npm run check
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/puffer-desktop/src/lib/screens/agent/MediaSettingsModal.svelte
git commit -m "fix(desktop): show video provider connect state"
```

---

## Task 5: Verify End-to-End Capability Behavior Locally

**Files:**
- No code changes unless a previous task exposed a bug.

- [ ] **Step 1: Run focused Rust tests**

Run:

```bash
cargo test -p puffer-core unauthenticated_video_descriptor_appears_unavailable_with_missing_auth
cargo test -p puffer-core connected_openai_video_descriptor_is_available
cargo test -p puffer-core unavailable_video_capability_cannot_validate_generation_selection
cargo test -p puffer-resources relaydance_declares_executable_video_descriptor
cargo test -p puffer-resources only_relaydance_declares_first_pass_video_media
```

Expected: all pass.

- [ ] **Step 2: Run focused desktop tests**

Run:

```bash
cd apps/puffer-desktop
npx vitest run src/lib/screens/agent/mediaCapabilityState.test.ts
npm run check
```

Expected: all pass.

- [ ] **Step 3: Probe the daemon without a Relaydance key**

Start or restart the Puffer desktop app, then call:

```bash
node -e 'const fs=require("fs"); const hs=JSON.parse(fs.readFileSync("/Users/zhangxiao/.puffer/daemon.handshake","utf8")); const url=new URL(hs.url); url.searchParams.set("token",hs.token); const ws=new WebSocket(url); ws.onopen=()=>ws.send(JSON.stringify({type:"request",id:"video",method:"list_media_capabilities",params:{kind:"video"}})); ws.onmessage=(event)=>{const msg=JSON.parse(event.data); console.log(JSON.stringify((msg.result?.capabilities||[]).map(c=>({providerId:c.providerId,modelId:c.modelId,status:c.status,reason:c.reason})),null,2)); ws.close();}; ws.onerror=()=>{console.error("ws error"); process.exit(1);}; setTimeout(()=>{console.error("timeout"); process.exit(2);},10000);'
```

Expected output includes:

```json
[
  {
    "providerId": "relaydance",
    "modelId": "doubao-seedance-2-0-720p",
    "status": "unavailable",
    "reason": "missing_auth"
  }
]
```

- [ ] **Step 4: Check the desktop modal manually**

Open `Video generation settings`.

Expected: the modal shows `Connect Relaydance to enable video generation.` instead of `No video capabilities available.`
