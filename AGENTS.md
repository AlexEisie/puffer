# Puffer Code Agents

This repository is a production-facing Rust rebuild of Claude Code under the
name `Puffer Code`.

## Primary Goal

Match Claude Code behavior where it matters for coding workflows, while:

- removing telemetry and feedback/reporting infrastructure
- preserving Claude-compatible Anthropic request behavior where required
- supporting Anthropic and OpenAI with API key and OAuth flows
- using a native Rust TUI instead of Ink
- keeping prompts and tool metadata editable through declarative resource files

## Current Workspace

`Cargo.toml` is the source of truth for workspace membership. Keep broad crate
categories in mind instead of relying on a copied list:

- CLI/daemon/UI: `puffer-cli`, `puffer-tui`
- Core runtime: `puffer-core`, `puffer-tools`, `puffer-resources`,
  `puffer-session-store`, `puffer-workflow`, `puffer-config`
- Provider/runner layers: `puffer-transport-anthropic`,
  `puffer-provider-openai`, `puffer-provider-registry`, `puffer-runner-*`,
  `puffer-tool-runner`
- Connector/subscriber stack: `puffer-connector-*`, `puffer-subscriber-*`,
  `puffer-subscriptions`, `puffer-slack`
- Support/security/media: `puffer-test-support`, `puffer-observability`,
  `puffer-logging`, `puffer-media`, `puffer-secrets`,
  `puffer-skill-evolution`, `puffer-mcp-oauth`

See `README.md` and `docs/README.md` for the current repo map and docs index.

## Repo Guardrails

- Use ASCII unless there is an existing reason not to.
- Keep modules small and purpose-specific. Large files exist today, so use
  `scripts/report-large-files.sh` as a risk report and call out touched large
  files in PRs.
- Prefer stable, typed Rust APIs over stringly typed plumbing, especially across
  daemon RPC, resource manifests, permissions, and connector action contracts.
- Keep slash-command docs generated from
  `crates/puffer-core/command/registry.rs`; run
  `scripts/check-slash-commands.sh` after changing the command registry.
- For docs changes, run `scripts/check-doc-links.sh`.

## Supported Slash Commands

The built-in slash-command surface is generated from
`crates/puffer-core/command/registry.rs` and documented in
`docs/reference/slash-commands.md`. Do not maintain command lists by hand in
multiple files.

## Out of Scope

Do not reintroduce:

- telemetry
- analytics
- feedback upload/reporting flows
- privacy/telemetry settings flows
- Claude-branded mobile/desktop/product marketing commands

## Auth Expectations

Current auth command surface in `puffer-cli`:

- `puffer auth status`
- `puffer auth set-api-key <provider> [--stdin]`
- `puffer auth clear <provider>`
- `puffer auth oauth-url <provider>`
- `puffer auth oauth-start <provider>`
- `puffer auth oauth-exchange <provider> --verifier ... [--state ...] [--stdin]`
- `puffer auth oauth-refresh <provider>`
- `puffer auth login <provider> [value] [--stdin]`

The intended provider coverage is:

- Anthropic API key
- Anthropic OAuth
- OpenAI API key
- OpenAI/Codex OAuth

## Anthropic Compatibility Notes

Anthropic compatibility is stricter than other providers.

Preserve:

- header order where the repo models it
- Claude-style `claude-cli/...` user agent
- attribution block as a standalone first system block
- fingerprinting and CCH placeholder logic
- session-ingress auth behavior

Do not simplify the Anthropic path into generic provider code if that would
erase these details.

## Resource Model

Bundled resources live under `resources/` and currently include:

- `providers/`
- `prompts/`
- `tools/`
- `skills/`
- `plugins/`
- `mcp_servers/`
- `ides/`
- `mascots/`

Resource provenance matters. Preserve source metadata when loading resources.

## Session Model

Session state is append-only and should stay migration-friendly.

Current metadata includes:

- `id`
- `display_name`
- `cwd`
- `created_at_ms`
- `updated_at_ms`
- `parent_session_id`
- `slug`
- `tags`
- `note`

Do not replace this with opaque storage.

## TUI Direction

The TUI should keep moving toward Claude Code parity, but within current repo
constraints:

- Ratatui/Crossterm
- split modules for rendering, popup logic, markdown, and execution helpers
- transcript-first layout
- slash-command popup
- eventually tmux-aware parity testing

## Working Style

- Prefer incremental commits for small, coherent steps.
- Create any additional git worktrees under the repo-local `.worktree/`
  directory.
- Keep the workspace green with `cargo test --workspace`.
- When adding new features, wire tests in the same step where practical.
- When updating a component, write a new update spec in that component's
  `specs/<component>/` folder. Do not overwrite prior numbered specs; use the
  next unused two-digit Markdown file such as `03.md` when `00.md`, `01.md`,
  and `02.md` already exist.
- Component update specs must be concise, up-to-date, and exhaustive about the
  design, architecture, logic, contracts, and compatibility implications of the
  change.
- If there is a conflict between fidelity and maintainability, document the
  gap in code comments or commit messages rather than silently diverging.
