# Image-to-Video Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add first-frame image-to-video to the agent `VideoGeneration` tool through a single shared `image_url` field, implemented for BytePlus, with Relaydance folded into the same abstraction.

**Architecture:** A normalized optional `image_url` on `ExactMediaGenerationRequest` is the shared seam. Models declare image support via `MediaModelDescriptor.image_input`. Ingestion (agent tool) turns a local path into a base64 data URL or passes a URL through. Each video adapter serializes the URL into its own body shape; only BytePlus is wired this iteration.

**Tech Stack:** Rust workspace (`cargo`), `serde`/`serde_json`, `base64` (already a dependency), `reqwest` blocking. Spec: `docs/superpowers/specs/2026-06-10-image-to-video-design.md`.

---

## File Structure

- `crates/puffer-provider-registry/src/model.rs` — `ImageInputSpec` type; `MediaModelDescriptor.image_input`.
- `crates/puffer-provider-registry/src/model_tests.rs` — descriptor tests.
- `crates/puffer-core/runtime/media/image_format.rs` — **new**: shared `detect_image_format`, `image_mime_type`, `image_data_url`.
- `crates/puffer-core/runtime/media/mod.rs` — register the new module.
- `crates/puffer-core/runtime/media/images_json.rs` — reuse shared `detect_image_format`.
- `crates/puffer-core/media_runtime.rs` — `ExactMediaGenerationRequest.image_url`.
- `crates/puffer-core/media_runtime_tests.rs` — fix two struct literals.
- `crates/puffer-cli/src/daemon.rs` — set `image_url: None`.
- `apps/puffer-desktop/src-tauri/src/backend.rs` — set `image_url: None`.
- `crates/puffer-core/runtime/media/capabilities.rs` — `MediaCapability.image_input`.
- `crates/puffer-core/runtime/media/resolver.rs` — surface `image_input` into capabilities.
- `crates/puffer-core/media_runtime_video.rs` — modality validation; thread `image_url` to BytePlus.
- `crates/puffer-core/runtime/media/byteplus_video.rs` — serialize the image into `content[]`.
- `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs` — `image` input + ingestion.
- `resources/tools/video_generation.yaml` — schema `image` property + description.
- `crates/puffer-resources/tests/video_tool_schema.rs` — schema test for `image`.
- `crates/puffer-core/runtime/media/relaydance_video.rs` — deferred (Task 9).

---

## Task 1: Declare image input on the model descriptor

**Files:**
- Modify: `crates/puffer-provider-registry/src/model.rs` (add `ImageInputSpec`, add field to `MediaModelDescriptor` at lines 383-393)
- Test: `crates/puffer-provider-registry/src/model_tests.rs`

- [ ] **Step 1: Write the failing tests**

Add to `crates/puffer-provider-registry/src/model_tests.rs`:

```rust
#[test]
fn media_model_descriptor_defaults_image_input_to_none() {
    let model: MediaModelDescriptor = serde_json::from_value(serde_json::json!({
        "id": "m",
        "operations": ["generate"]
    }))
    .unwrap();
    assert_eq!(model.image_input, None);
}

#[test]
fn media_model_descriptor_parses_required_image_input() {
    let model: MediaModelDescriptor = serde_json::from_value(serde_json::json!({
        "id": "m",
        "operations": ["generate"],
        "image_input": { "required": true }
    }))
    .unwrap();
    assert_eq!(model.image_input, Some(ImageInputSpec { required: true }));
}
```

If `model_tests.rs` does not already `use super::*;` (or import `MediaModelDescriptor`), add `use super::*;` at the top.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p puffer-provider-registry media_model_descriptor_ -- --nocapture`
Expected: FAIL — `ImageInputSpec` not found / unknown field `image_input`.

- [ ] **Step 3: Add the type and field**

In `crates/puffer-provider-registry/src/model.rs`, add this type immediately before `pub struct MediaModelDescriptor`:

```rust
/// Declares that a media model accepts an input image (image-to-video).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageInputSpec {
    /// True when the model cannot run without an input image (pure i2v).
    pub required: bool,
}
```

Then add this field inside `MediaModelDescriptor`, after the `parameters` field:

```rust
    #[serde(default)]
    pub image_input: Option<ImageInputSpec>,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p puffer-provider-registry media_model_descriptor_ -- --nocapture`
Expected: PASS (2 tests).

- [ ] **Step 5: Build the crate to catch struct-literal breakage**

Run: `cargo build -p puffer-provider-registry`
Expected: success. (`#[serde(default)]` + `Option` means existing `MediaModelDescriptor { .. }` literals that omit the field still compile only if they used `..` — if any literal breaks, add `image_input: None,` to it. Run `cargo build -p puffer-core -p puffer-cli` too and fix any descriptor literal the same way.)

- [ ] **Step 6: Commit**

```bash
git add crates/puffer-provider-registry/src/model.rs crates/puffer-provider-registry/src/model_tests.rs
git commit -m "feat(registry): declare optional image_input on media model descriptor"
```

---

## Task 2: Shared image-format detection + data URL builder

**Files:**
- Create: `crates/puffer-core/runtime/media/image_format.rs`
- Modify: `crates/puffer-core/runtime/media/mod.rs:1-15` (register module)
- Modify: `crates/puffer-core/runtime/media/images_json.rs:407-417` (reuse shared detector)

- [ ] **Step 1: Write the new module with failing tests**

Create `crates/puffer-core/runtime/media/image_format.rs`:

```rust
use anyhow::{bail, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;

/// Recognizes common image formats from their leading magic bytes.
pub(crate) fn detect_image_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if bytes.len() >= 12 && bytes[0..4] == *b"RIFF" && bytes[8..12] == *b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// Maps a sniffed image format to its MIME type.
pub(crate) fn image_mime_type(format: &str) -> &'static str {
    match format {
        "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

/// Builds a `data:<mime>;base64,<...>` URL from raw image bytes.
///
/// Errors when the bytes carry no recognized image signature, so the caller
/// fails before sending an unusable input to a provider.
pub(crate) fn image_data_url(bytes: &[u8]) -> Result<String> {
    let Some(format) = detect_image_format(bytes) else {
        bail!("unrecognized image format (expected png, jpeg, or webp)");
    };
    let mime = image_mime_type(format);
    Ok(format!("data:{mime};base64,{}", BASE64_STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes() -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"rest");
        bytes
    }

    #[test]
    fn detects_png_signature() {
        assert_eq!(detect_image_format(&png_bytes()), Some("png"));
    }

    #[test]
    fn detects_jpeg_signature() {
        assert_eq!(detect_image_format(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpeg"));
    }

    #[test]
    fn unknown_bytes_have_no_format() {
        assert_eq!(detect_image_format(b"not-an-image"), None);
    }

    #[test]
    fn data_url_embeds_mime_and_base64() {
        let url = image_data_url(&png_bytes()).expect("data url");
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn data_url_rejects_unknown_bytes() {
        let error = image_data_url(b"nope").unwrap_err().to_string();
        assert!(error.contains("unrecognized image format"));
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/puffer-core/runtime/media/mod.rs`, add after line 6 (`pub(crate) mod http_support;`), keeping alphabetical-ish order:

```rust
pub(crate) mod image_format;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p puffer-core image_format -- --nocapture`
Expected: PASS (5 tests).

- [ ] **Step 4: Reuse the shared detector in images_json**

In `crates/puffer-core/runtime/media/images_json.rs`, delete the local `detect_image_format` function (lines ~406-417, the `fn detect_image_format(bytes: &[u8]) -> Option<&'static str> { ... }`) and add an import near the other `use super::...` lines at the top:

```rust
use super::image_format::detect_image_format;
```

Leave `resolved_output_format`, `output_format_for_parameters`, and `mime_type_for_output_format` unchanged — they still call `detect_image_format`, now resolved from the shared module.

- [ ] **Step 5: Run images_json tests to verify no regression**

Run: `cargo test -p puffer-core images_json`
Expected: PASS (existing tests still green).

- [ ] **Step 6: Commit**

```bash
git add crates/puffer-core/runtime/media/image_format.rs crates/puffer-core/runtime/media/mod.rs crates/puffer-core/runtime/media/images_json.rs
git commit -m "refactor(media): extract shared image-format detection and data-url builder"
```

---

## Task 3: Add `image_url` to the runtime request and fix all build sites

**Files:**
- Modify: `crates/puffer-core/media_runtime.rs:76-85` (add field)
- Modify: `crates/puffer-core/media_runtime_tests.rs:643, 667` (struct literals)
- Modify: `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs:120-130` (struct literal)
- Modify: `crates/puffer-cli/src/daemon.rs:2504` (struct literal)
- Modify: `apps/puffer-desktop/src-tauri/src/backend.rs:3776` (struct literal)

- [ ] **Step 1: Add the field**

In `crates/puffer-core/media_runtime.rs`, inside `pub struct ExactMediaGenerationRequest`, add after `pub count: u8,`:

```rust
    #[serde(default)]
    pub image_url: Option<String>,
```

- [ ] **Step 2: Build to enumerate the broken literals**

Run: `cargo build -p puffer-core -p puffer-cli`
Expected: FAIL — "missing field `image_url`" at each struct literal. Use these errors as the checklist for Step 3.

- [ ] **Step 3: Fix every `ExactMediaGenerationRequest { ... }` literal**

Add `image_url: None,` to each of these literals (after `count,`):

- `crates/puffer-core/media_runtime_tests.rs` line ~643 and line ~667.
- `crates/puffer-cli/src/daemon.rs` line ~2504 (`exact_media_generation_request`).
- `apps/puffer-desktop/src-tauri/src/backend.rs` line ~3776 (`exact_media_generation_request_from_stored`).

For `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs` `exact_media_request` (line ~120), populate it from the request instead (the `VideoRequest.image_url` field is added in Task 7; for now set `image_url: None,` so it compiles, and Task 7 changes it to `request.image_url.clone()`):

```rust
        image_url: None,
```

- [ ] **Step 4: Build the whole workspace**

Run: `cargo build -p puffer-core -p puffer-cli && cargo build -p puffer-desktop 2>/dev/null || cargo build --workspace`
Expected: success.

- [ ] **Step 5: Run media runtime tests**

Run: `cargo test -p puffer-core media_runtime`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/puffer-core/media_runtime.rs crates/puffer-core/media_runtime_tests.rs crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs crates/puffer-cli/src/daemon.rs apps/puffer-desktop/src-tauri/src/backend.rs
git commit -m "feat(media): add optional image_url to exact media generation request"
```

---

## Task 4: Surface `image_input` into resolved capabilities

**Files:**
- Modify: `crates/puffer-core/runtime/media/capabilities.rs:1-31` (import + field)
- Modify: `crates/puffer-core/runtime/media/resolver.rs:242-260, 292-309` (populate field in both literals)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/puffer-core/runtime/media/resolver.rs` (reuse the existing `video_media_with_adapter` helper, which builds a model without image input):

```rust
#[test]
fn video_capability_surfaces_model_image_input() {
    let mut media = video_media_with_adapter(
        MediaExecutionKind::RelaydanceVideo,
        "doubao-seedance-2-0-720p",
    );
    media.video.as_mut().unwrap().models[0].image_input =
        Some(puffer_provider_registry::ImageInputSpec { required: true });
    let registry = registry_with(vec![provider(
        "relaydance",
        vec![AuthMode::ApiKey],
        Some(media),
    )]);

    let capabilities = resolve_media_capabilities(
        &registry,
        &auth_for("relaydance"),
        MediaKind::Video,
        MediaOperation::Generate,
        42,
        &MediaDiscoveryCache::default(),
    );

    assert_eq!(
        capabilities[0].image_input,
        Some(puffer_provider_registry::ImageInputSpec { required: true })
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p puffer-core video_capability_surfaces_model_image_input`
Expected: FAIL — no field `image_input` on `MediaCapability`.

- [ ] **Step 3: Add the field to `MediaCapability`**

In `crates/puffer-core/runtime/media/capabilities.rs`, change the import line:

```rust
use puffer_provider_registry::{ImageInputSpec, MediaParameterWireType};
```

Add the field inside `pub(crate) struct MediaCapability`, after `pub(crate) parameters: Vec<MediaCapabilityParameter>,`:

```rust
    pub(crate) image_input: Option<ImageInputSpec>,
```

- [ ] **Step 4: Populate it in both resolver constructors**

In `crates/puffer-core/runtime/media/resolver.rs`:

- In `resolve_image_capabilities`, in the `capabilities.push(MediaCapability { ... })` literal, add:

```rust
                image_input: None,
```

- In `resolve_video_capabilities`, in the `capabilities.push(MediaCapability { ... })` literal, add:

```rust
                image_input: model.image_input.clone(),
```

- [ ] **Step 5: Build to find any other `MediaCapability` literal**

Run: `cargo build -p puffer-core`
Expected: success. If a compile error names another `MediaCapability { ... }` literal (e.g. in a test), add `image_input: None,` to it.

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p puffer-core video_capability_surfaces_model_image_input`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/puffer-core/runtime/media/capabilities.rs crates/puffer-core/runtime/media/resolver.rs
git commit -m "feat(media): surface model image_input in resolved video capabilities"
```

---

## Task 5: Validate image modality before submit

**Files:**
- Modify: `crates/puffer-core/media_runtime_video.rs:48-79` (call validation after capability resolution; add helper)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/puffer-core/media_runtime_video.rs`:

`MediaCapability` is already in scope via the test module's `use super::*;`, so do not re-import it. Fully-qualify only the capability `MediaKind` to avoid colliding with the `MediaKind` already imported in this file:

```rust
fn video_capability_with_image(image_input: Option<puffer_provider_registry::ImageInputSpec>) -> MediaCapability {
    MediaCapability {
        provider_id: "byteplus".to_string(),
        provider_display_name: "BytePlus".to_string(),
        model_id: "dreamina-seedance-2-0-260128".to_string(),
        model_display_name: "Seedance".to_string(),
        kind: crate::runtime::media::capabilities::MediaKind::Video,
        operation: "generate".to_string(),
        adapter: "byteplus_video".to_string(),
        parameters: Vec::new(),
        defaults: std::collections::BTreeMap::new(),
        status: "available".to_string(),
        source: "static".to_string(),
        reason: None,
        checked_at_ms: 0,
        image_input,
    }
}

#[test]
fn rejects_image_when_model_has_no_image_input() {
    let capability = video_capability_with_image(None);
    let error = validate_image_modality(&capability, Some("data:image/png;base64,AAAA"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not accept an image"));
}

#[test]
fn rejects_missing_image_when_required() {
    let capability =
        video_capability_with_image(Some(puffer_provider_registry::ImageInputSpec { required: true }));
    let error = validate_image_modality(&capability, None).unwrap_err().to_string();
    assert!(error.contains("requires an image"));
}

#[test]
fn accepts_optional_image_input() {
    let capability =
        video_capability_with_image(Some(puffer_provider_registry::ImageInputSpec { required: false }));
    assert!(validate_image_modality(&capability, Some("https://x/y.png")).is_ok());
    assert!(validate_image_modality(&capability, None).is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p puffer-core validate_image_modality_ rejects_image_when_model_has_no_image_input rejects_missing_image_when_required accepts_optional_image_input`
Expected: FAIL — `validate_image_modality` not found.

- [ ] **Step 3: Add the helper and call it**

In `crates/puffer-core/media_runtime_video.rs`, add the helper near the other free functions (e.g. above `selected_parameters_with_defaults`):

```rust
/// Rejects image/model modality mismatches before any provider HTTP call.
fn validate_image_modality(
    capability: &crate::runtime::media::capabilities::MediaCapability,
    image_url: Option<&str>,
) -> Result<()> {
    let has_image = image_url.map(|url| !url.trim().is_empty()).unwrap_or(false);
    match (&capability.image_input, has_image) {
        (None, true) => bail!(
            "video model {}/{} does not accept an image input",
            capability.provider_id,
            capability.model_id
        ),
        (Some(spec), false) if spec.required => bail!(
            "video model {}/{} requires an image input",
            capability.provider_id,
            capability.model_id
        ),
        _ => Ok(()),
    }
}
```

In `generate_exact_video_from_media_request`, after the `let capability = validate_media_generate_selection(...)?;` block and before `validate_video_count(...)`, add:

```rust
    validate_image_modality(&capability, request.image_url.as_deref())?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p puffer-core rejects_image_when_model_has_no_image_input rejects_missing_image_when_required accepts_optional_image_input`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/puffer-core/media_runtime_video.rs
git commit -m "feat(media): validate image modality against model capability before submit"
```

---

## Task 6: Serialize the image into the BytePlus request body

**Files:**
- Modify: `crates/puffer-core/runtime/media/byteplus_video.rs:17-101` (struct field, body, builder)
- Modify: `crates/puffer-core/media_runtime_video.rs:233-254` (`generate_byteplus_video` passes `image_url`)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/puffer-core/runtime/media/byteplus_video.rs`:

```rust
#[test]
fn byteplus_request_body_appends_image_url_content_item() {
    let request = BytePlusVideoRequest {
        model: "dreamina-seedance-2-0-260128".to_string(),
        prompt: "animate this".to_string(),
        image_url: Some("data:image/png;base64,AAAA".to_string()),
        params: vec![],
    };
    let body = request.request_body();
    let content = body["content"].as_array().expect("content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["type"], json!("text"));
    assert_eq!(content[1]["type"], json!("image_url"));
    assert_eq!(content[1]["image_url"]["url"], json!("data:image/png;base64,AAAA"));
}

#[test]
fn byteplus_request_body_omits_image_when_absent() {
    let request = BytePlusVideoRequest {
        model: "dreamina-seedance-2-0-260128".to_string(),
        prompt: "a cat".to_string(),
        image_url: None,
        params: vec![],
    };
    let body = request.request_body();
    assert_eq!(body["content"].as_array().unwrap().len(), 1);
}
```

Also update the existing literals in this test module that build `BytePlusVideoRequest { ... }` (the tests `byteplus_request_body_contains_model_and_prompt`, `byteplus_request_body_encodes_duration_as_number`, `byteplus_request_body_defaults_to_silent_video`, `byteplus_request_body_allows_generate_audio_override`, and `poll_parser_failure_is_transient_and_keeps_polling`) by adding `image_url: None,` to each literal.

- [ ] **Step 2: Run the new tests to verify they fail**

Run: `cargo test -p puffer-core byteplus_request_body_appends_image_url_content_item byteplus_request_body_omits_image_when_absent`
Expected: FAIL — `BytePlusVideoRequest` has no field `image_url`.

- [ ] **Step 3: Add the field and serialize it**

In `crates/puffer-core/runtime/media/byteplus_video.rs`, add to `struct BytePlusVideoRequest` after `pub(crate) prompt: String,`:

```rust
    pub(crate) image_url: Option<String>,
```

Replace the content insertion in `request_body` (the `body.insert("content".to_string(), json!([ ... ]))` block) with:

```rust
        let mut content = vec![json!({
            "type": "text",
            "text": self.prompt.trim()
        })];
        if let Some(url) = self.image_url.as_deref() {
            let url = url.trim();
            if !url.is_empty() {
                content.push(json!({
                    "type": "image_url",
                    "image_url": { "url": url }
                }));
            }
        }
        body.insert("content".to_string(), Value::Array(content));
```

- [ ] **Step 4: Thread `image_url` through the builder**

Change the signature of `byteplus_video_request_from_parameters` to accept the image, adding a parameter after `prompt: String,`:

```rust
pub(crate) fn byteplus_video_request_from_parameters(
    model_id: String,
    prompt: String,
    image_url: Option<String>,
    capability_parameters: &[MediaCapabilityParameter],
    selected: &BTreeMap<String, String>,
) -> Result<BytePlusVideoRequest> {
```

and set the field when building the request:

```rust
    let request = BytePlusVideoRequest {
        model: model_id,
        prompt,
        image_url,
        params,
    };
```

Update the existing builder test `byteplus_request_body_uses_parameter_wire_type_for_numbers` and `byteplus_rejects_invalid_number_wire_value_before_http` to pass `None` for the new `image_url` argument.

- [ ] **Step 5: Pass `image_url` from the runtime**

In `crates/puffer-core/media_runtime_video.rs`, in `generate_byteplus_video`, change the `byteplus_video_request_from_parameters(...)` call to forward the request image:

```rust
    let video_request = byteplus_video_request_from_parameters(
        request.model_id.clone(),
        request.prompt.clone(),
        request.image_url.clone(),
        &capability.parameters,
        &parameters,
    )?;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p puffer-core byteplus`
Expected: PASS (all byteplus tests, including the two new ones).

- [ ] **Step 7: Commit**

```bash
git add crates/puffer-core/runtime/media/byteplus_video.rs crates/puffer-core/media_runtime_video.rs
git commit -m "feat(media): serialize image_url into byteplus image-to-video request"
```

---

## Task 7: Ingest the image in the agent tool

**Files:**
- Modify: `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs` (input field, `VideoRequest`, ingestion, `exact_media_request`)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs`:

```rust
#[test]
fn accepts_image_field_and_passes_data_url() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("frame.png"),
        [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, b'x'],
    )
    .unwrap();

    let request = build_video_request(
        dir.path(),
        VideoGenerationInput {
            prompt: "animate the frame".to_string(),
            image: Some("frame.png".to_string()),
            parameters: BTreeMap::new(),
            purpose: None,
        },
        &video_settings(),
    )
    .unwrap();

    assert!(request
        .image_url
        .as_deref()
        .unwrap()
        .starts_with("data:image/png;base64,"));
}

#[test]
fn passes_http_image_url_through() {
    let dir = tempdir().unwrap();
    let request = build_video_request(
        dir.path(),
        VideoGenerationInput {
            prompt: "animate".to_string(),
            image: Some("https://example.com/frame.png".to_string()),
            parameters: BTreeMap::new(),
            purpose: None,
        },
        &video_settings(),
    )
    .unwrap();

    assert_eq!(
        request.image_url.as_deref(),
        Some("https://example.com/frame.png")
    );
}

#[test]
fn rejects_oversize_image() {
    let dir = tempdir().unwrap();
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.resize(9 * 1024 * 1024, 0);
    std::fs::write(dir.path().join("big.png"), &bytes).unwrap();

    let error = build_video_request(
        dir.path(),
        VideoGenerationInput {
            prompt: "animate".to_string(),
            image: Some("big.png".to_string()),
            parameters: BTreeMap::new(),
            purpose: None,
        },
        &video_settings(),
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("exceeds"));
}

#[test]
fn accepts_image_field_in_tool_input() {
    let input = serde_json::from_value::<VideoGenerationInput>(json!({
        "prompt": "animate",
        "image": "frame.png"
    }))
    .unwrap();
    assert_eq!(input.image.as_deref(), Some("frame.png"));
}
```

Replace the existing `rejects_unknown_video_generation_fields` test body so it asserts a genuinely unknown field is rejected (since `image` is now known):

```rust
#[test]
fn rejects_unknown_video_generation_fields() {
    let error = serde_json::from_value::<VideoGenerationInput>(json!({
        "prompt": "make a ship launch video",
        "bogusField": "frame.png"
    }))
    .unwrap_err();

    assert!(error.to_string().contains("bogusField"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p puffer-core video_generation`
Expected: FAIL — `VideoGenerationInput`/`VideoRequest` have no `image`/`image_url` field.

- [ ] **Step 3: Add the input and request fields**

In `video_generation.rs`, add to `struct VideoGenerationInput` (keep `deny_unknown_fields`), after the `parameters` field:

```rust
    #[serde(default)]
    image: Option<String>,
```

Add to `struct VideoRequest`, after `prompt: String,`:

```rust
    image_url: Option<String>,
```

Add `image: None,` to the existing `VideoGenerationInput { ... }` struct literals in the test module that the new field would otherwise break: `build_request_rejects_empty_prompt` (~line 522) and `build_request_merges_saved_parameters_and_tool_overrides` (~line 591).

- [ ] **Step 4: Add the ingestion function**

In `video_generation.rs`, add a constant near `MAX_PROMPT_CHARS`:

```rust
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
```

Add the function near `prompt_text`:

```rust
/// Resolves the optional image input to a URL the adapter can send: a passed
/// through `http(s)`/`data:` URL, or a `data:` URL built from a workspace file.
fn resolve_image_url(cwd: &Path, value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        bail!("VideoGeneration image is empty");
    }
    if value.starts_with("data:")
        || value.starts_with("http://")
        || value.starts_with("https://")
    {
        return Ok(value.to_string());
    }
    if !safe_relative_path(value) {
        bail!("VideoGeneration image must be a workspace-relative path or http(s)/data URL");
    }
    let path = cwd.join(value);
    let bytes = fs::read(&path)
        .with_context(|| format!("read VideoGeneration `image` {}", path.display()))?;
    if bytes.len() > MAX_IMAGE_BYTES {
        bail!("VideoGeneration image exceeds {MAX_IMAGE_BYTES} bytes");
    }
    crate::runtime::media::image_format::image_data_url(&bytes)
}
```

- [ ] **Step 5: Populate `image_url` in `build_video_request` and `exact_media_request`**

In `build_video_request`, after computing `prompt`, add:

```rust
    let image_url = match input.image.as_deref() {
        Some(value) => Some(resolve_image_url(cwd, value)?),
        None => None,
    };
```

and add `image_url,` to the returned `VideoRequest { ... }` literal.

In `exact_media_request`, change `image_url: None,` (set in Task 3) to:

```rust
        image_url: request.image_url.clone(),
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p puffer-core video_generation`
Expected: PASS (new tests + updated unknown-field test + existing tests).

- [ ] **Step 7: Commit**

```bash
git add crates/puffer-core/runtime/claude_tools/workflow/video_generation.rs
git commit -m "feat(media): ingest image input in VideoGeneration tool as data url or passthrough"
```

---

## Task 8: Expose `image` in the tool schema

**Files:**
- Modify: `resources/tools/video_generation.yaml:3-36`
- Test: `crates/puffer-resources/tests/video_tool_schema.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/puffer-resources/tests/video_tool_schema.rs`:

```rust
#[test]
fn video_generation_tool_schema_exposes_image_property() {
    let tool: serde_json::Value = serde_yaml::from_str(include_str!(
        "../../../resources/tools/video_generation.yaml"
    ))
    .expect("VideoGeneration tool YAML");
    assert_eq!(
        tool["input_schema"]["properties"]["image"]["type"],
        serde_json::Value::String("string".to_string())
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p puffer-resources video_generation_tool_schema_exposes_image_property`
Expected: FAIL — `image` property is `null`.

- [ ] **Step 3: Update the YAML**

In `resources/tools/video_generation.yaml`, replace the description paragraph that says it is "text-to-video only" / "does not accept existing images..." with:

```yaml
  Generate one video clip through Puffer's configured video media settings and
  write the resulting video artifact into the workspace media folder.

  Use one VideoGeneration call for one logical video-generation request. The
  tool reads a workspace-relative prompt file when the prompt value names one;
  otherwise it treats prompt as literal text. Provide `image` to drive
  image-to-video on models that accept an input image (first frame). The result
  includes one media job id and one persisted video artifact entry.
```

Add an `image` property under `input_schema.properties` (after `prompt`):

```yaml
    image:
      type: string
      description: >-
        Optional first-frame image for image-to-video: a workspace-relative
        image path, or an http(s)/data URL. Only valid for models that accept
        image input.
```

Leave `additionalProperties: false` and `required: [prompt]` unchanged.

- [ ] **Step 4: Run both schema tests to verify they pass**

Run: `cargo test -p puffer-resources video_generation_tool_schema`
Expected: PASS (the scalar-parameter test and the new image-property test).

- [ ] **Step 5: Commit**

```bash
git add resources/tools/video_generation.yaml crates/puffer-resources/tests/video_tool_schema.rs
git commit -m "feat(media): expose image input in VideoGeneration tool schema"
```

---

## Task 9 (deferred): Relaydance image serialization

**Status:** Blocked on confirming the request-side image field name in the New API gateway `dto/relaydance_video.go` request struct. Not part of this iteration's end-to-end scope. The shared layers (Tasks 1-8) already pass `image_url` to the Relaydance builder path, so this task touches only `relaydance_video.rs`.

**Files:**
- Modify: `crates/puffer-core/runtime/media/relaydance_video.rs:16-89`
- Modify: `crates/puffer-core/media_runtime_video.rs` (`generate_relaydance_video` forwards `image_url`)

- [ ] **Step 1: Confirm the field name**

Read the request struct in the gateway `dto/relaydance_video.go`. Record whether the image is a top-level field (e.g. `image`) or nested (e.g. `metadata.image`), and whether it expects a URL or bare base64.

- [ ] **Step 2: Write the failing test**

In `relaydance_video.rs` tests, mirror the BytePlus test using the confirmed field name, e.g. (top-level `image` placeholder — replace with the confirmed name):

```rust
#[test]
fn relaydance_request_body_includes_image_when_present() {
    let request = RelaydanceVideoRequest {
        model: "doubao-seedance-2-0-720p".to_string(),
        prompt: "animate".to_string(),
        image_url: Some("https://x/y.png".to_string()),
        params: vec![],
    };
    let body = request.request_body();
    assert_eq!(body["image"], serde_json::json!("https://x/y.png")); // confirmed field name
}
```

- [ ] **Step 3: Add the field and serialize it**

Add `pub(crate) image_url: Option<String>,` to `RelaydanceVideoRequest`. In `request_body`, when `image_url` is `Some` and non-empty, insert it at the confirmed location (top-level or under `metadata`). Add `image_url` to `relaydance_video_request_from_parameters` and update existing `RelaydanceVideoRequest { ... }` test literals with `image_url: None,`.

- [ ] **Step 4: Forward from the runtime**

In `generate_relaydance_video` (media_runtime_video.rs), pass `request.image_url.clone()` into `relaydance_video_request_from_parameters`.

- [ ] **Step 5: Run tests and commit**

Run: `cargo test -p puffer-core relaydance`
Expected: PASS.

```bash
git add crates/puffer-core/runtime/media/relaydance_video.rs crates/puffer-core/media_runtime_video.rs
git commit -m "feat(media): serialize image_url into relaydance image-to-video request"
```

---

## Final verification

- [ ] **Run the full media test suite**

Run: `cargo test -p puffer-core -p puffer-provider-registry -p puffer-resources`
Expected: PASS.

- [ ] **Workspace build**

Run: `cargo build --workspace`
Expected: success.
