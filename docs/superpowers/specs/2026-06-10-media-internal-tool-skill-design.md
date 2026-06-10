# Media Internal Tool Skill Design

## Purpose

Move image and video generation away from model-facing `ImageGeneration` and
`VideoGeneration` tool calls. The long-term path is skill-directed internal
tool execution:

1. The model invokes the existing `image-generation` or `video-generation`
   skill.
2. The skill tells the model to run a foreground Bash command.
3. The Bash command calls `puffer internal-tool image-generation` or
   `puffer internal-tool video-generation`.
4. The internal CLI sends a structured execution request back to the parent
   runtime.
5. The parent runtime executes the existing media generation backend with the
   current provider registry, auth store, and media discovery cache.

This design intentionally does not preserve the old model-facing media tool
surface.

## Explicit Constraint

Do not modify `crates/puffer-core/runtime/system_prompt.rs`.

The active bundled prompt already comes from `resources/prompts/system-base.yaml`
when resources load successfully, and that prompt does not contain the old
direct media-tool instructions. The hardcoded fallback in `system_prompt.rs`
may remain unchanged for this work.

## Current State

`resources/skills/image-generation/SKILL.md` and
`resources/skills/video-generation/SKILL.md` already exist. Today they are only
skill wrappers around direct model-facing tools:

- `allowed-tools` is `ImageGeneration` or `VideoGeneration`.
- The body instructs the model to call the corresponding direct tool.

`resources/tools/image_generation.yaml` and
`resources/tools/video_generation.yaml` define those direct model-facing tools.

The Bash internal permission broker already supports two request classes:
permission requests and execution requests. Subscriber-backed internal tools
already use execution requests through `require_internal_tool_execution_from_env`.

The current parent execution handler only maps email, telegram, and
request-user-browser-action. It cannot execute media yet because it does not
receive provider/auth/media-discovery context.

Desktop and daemon timelines currently synthesize generated media attachments
from successful direct `ImageGeneration` and `VideoGeneration` tool outputs.
After this migration, the media JSON will be nested inside successful Bash
stdout, so attachment extraction must be updated.

## Architecture

### Resources

Move the media tool manifests from `resources/tools/` to
`resources/internal_tools/`.

Keep the canonical internal tool ids as `ImageGeneration` and
`VideoGeneration` so existing backend names remain stable. Add user-facing
aliases such as `image-generation`, `imagegen`, `video-generation`, and
`videogen` for CLI and permission lookup.

Update resource tests so they assert:

- `ImageGeneration` and `VideoGeneration` are no longer model-facing tools.
- They are registered as internal tools.
- The video parameter schema still accepts scalar string, number, and boolean
  overrides.

### Skills

Rewrite the existing media generation skills instead of adding new duplicate
skills.

The skills should allow only `Bash`. Their bodies should instruct the model to
run a foreground internal CLI command:

- `puffer internal-tool image-generation ...`
- `puffer internal-tool video-generation ...`

The image skill must preserve the existing behavioral rules: one logical image
request maps to one internal tool command, `count` carries multi-image requests,
prompt files are workspace-relative, generation failures are reported plainly,
and handcrafted fallback art must not be presented as generated output.

The video skill must preserve the existing text-to-video limitation and must
not imply success unless a persisted video artifact is returned.

Because the command runs under Bash, the skills should instruct the model to
use a foreground Bash call with an explicit long timeout. Do not use
background Bash for these internal tools: the background path does not expose
the broker address/token required by internal execution requests.

### CLI

Add two hidden internal-tool subcommands:

- `puffer internal-tool image-generation`
- `puffer internal-tool video-generation`

The CLI should be a thin request adapter. It parses flags, builds the same JSON
payload shape currently accepted by the media backend, sends that payload to the
parent runtime with `require_internal_tool_execution_from_env`, prints the
successful output exactly, and fails if the internal execution endpoint is not
available.

The CLI must not independently load provider config, auth state, resources, or
media discovery. Standalone use outside the agent's Bash environment is not a
goal.

### Parent Runtime Execution

Extend the Bash internal execution handler so media requests can reach the
existing backend with the same context as direct workflow tools had.

The current call site in the Bash tool branch already has access to:

- `ProviderRegistry`
- `AuthStore`
- `AppState`
- `LoadedResources`
- `ToolRegistry`
- current `cwd`

Add a small media-specific internal execution helper rather than exposing a
generic workflow executor. It should map canonical internal tool names to:

- `workflow::image_generation::execute_image_generation`
- `workflow::video_generation::execute_video_generation`

Build `ImageGenerationMediaContext` and `VideoGenerationMediaContext` there
from the current provider registry, auth store, and exact media discovery
cache. Keep non-media internal execution mappings narrow and explicit.

### Timeline And Attachments

Update desktop and daemon timeline attachment extraction for the new shape.

For new sessions, successful media generation appears as a successful `Bash`
tool invocation whose output is Bash JSON. The actual media result is in the
Bash `stdout` field and should parse as the existing media result JSON.

Attachment extraction should only parse Bash stdout as generated media when the
Bash input command is a Puffer media internal tool command or one of its
documented helper aliases. This avoids treating arbitrary shell JSON as a
generated artifact.

The output media JSON schema should remain the same as the existing direct
backend result, so downstream generated image/video attachment construction can
reuse the current artifact parsing logic.

## Error Handling

Unknown internal tool ids remain denied by the internal tool permission layer.

Missing image/video provider, model, adapter, operation, or auth still returns
the existing backend error text. The internal CLI should not reinterpret these
errors beyond prefixing them as an internal tool failure when the parent runtime
returns a failed execution response.

If Bash times out, the user sees the Bash timeout error. The skills should
prevent avoidable timeout failures by instructing the model to set a long
foreground timeout for media generation.

## Non-Goals

- Do not modify `crates/puffer-core/runtime/system_prompt.rs`.
- Do not make `Skill` execute tools directly.
- Do not add a generic internal workflow runner.
- Do not support standalone media internal CLI execution outside the parent
  runtime broker.
- Do not duplicate provider/auth/resource loading in the CLI.
- Do not merge image and video into a single broad `MediaGeneration` command.
- Do not preserve the model-facing direct media tool surface.

## Test Plan

Update or add focused tests for:

- Resource loading: media generation tools are internal tools, not model-facing
  tools.
- Media skill frontmatter and body: `allowed-tools` is `Bash`; bodies mention
  the internal CLI and preserve current safety rules.
- Internal CLI argument parsing: image and video commands produce the expected
  JSON payloads.
- Parent internal execution: `image-generation` and `video-generation` map to
  the existing media backend and receive media context.
- Bash broker integration: a foreground Bash command can call the media
  internal CLI and receive the backend JSON output.
- Desktop and daemon timelines: generated media attachments are synthesized
  from Bash stdout for Puffer media internal tool commands.

Existing image/video backend tests should stay focused on generation behavior
and should not be rewritten around CLI details.

## Compatibility Position

This migration does not preserve direct provider tool calls for image or video
generation. Historical sessions with direct `ImageGeneration` or
`VideoGeneration` events may continue to render if the old parser branches are
left in place, but that is not required by this design.
