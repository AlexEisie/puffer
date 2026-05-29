# Provider Logo Icons Design

## Problem

Desktop provider cards currently use `providerVisuals.ts` to select a card icon
and accent. Only OpenAI and Vercel AI Gateway map to brand assets. The remaining
known providers mostly fall back to generic `ai.svg` or `llm.svg`, including the
currently visible Puffer, Codex, and Claude cards.

## Provider Coverage

The implementation must cover every provider declared in `resources/providers`:

- `anthropic`
- `cerebras`
- `groq`
- `kimi-coding`
- `kimi-openai`
- `llama-cpp`
- `lmstudio`
- `minimax-cn`
- `minimax`
- `ollama`
- `openai`
- `openrouter`
- `vercel-ai-gateway`
- `vllm`
- `worldrouter`
- `xai`
- `zhipu`

Desktop-native aliases must also be covered because they appear in Settings:

- `puffer`
- `codex`
- `claude`

## Design

Use the smallest reliable logo strategy:

1. Use existing bundled brand assets where the repository already has them.
   `puffer` uses `/brand-logo.svg`; `openai` and `codex` use the existing
   OpenAI icon; `vercel-ai-gateway` uses the existing Vercel icon.
2. For every other known provider, add a simple local monogram SVG. These are
   not meant to impersonate official logos; they are provider-specific visual
   identifiers that keep the current Settings UI readable without importing
   third-party assets with unclear reuse terms.
3. Keep `ai.svg` and `llm.svg` only as unknown-provider fallbacks, not as
   visuals for known providers.

The monogram set should cover: `anthropic`, `cerebras`, `groq`, `kimi-coding`,
`kimi-openai`, `llama-cpp`, `lmstudio`, `minimax-cn`, `minimax`, `ollama`,
`openrouter`, `vllm`, `worldrouter`, `xai`, `zhipu`, and the desktop-native
`claude` alias. This is enough to remove generic icons from known provider
cards without creating a logo licensing project.

## Non-Goals

Do not add provider registry behavior, config behavior, credential behavior, or
model discovery behavior. Do not download, vendor, or trace third-party brand
assets in this pass. Do not redesign the provider card layout.

## Data Flow

`providerVisual(provider)` remains the single lookup point for Settings provider
cards. The provider id selects an explicit icon key and accent. The function can
return either a `/service-icons/*.svg` URL or the existing root
`/brand-logo.svg` URL.

## UI Rules

Provider cards keep the existing 36 px logo container and subdued card styling.
New monogram SVGs should use a consistent 24x24 viewBox, currentColor-friendly
or restrained fills, and short text or geometric marks that remain legible in
the existing 36 px logo container. Avoid saturated brand-color blocks unless the
asset already exists in the repository.

## Testing

Add a focused desktop test that reads `resources/providers/*.yaml` and
`providerVisuals.ts`, then asserts every known provider id has an explicit icon
mapping. The same test must include desktop-native aliases `puffer`, `codex`,
and `claude`. Known providers must not map to generic `ai` or `llm` unless they
are deliberately listed as unknown fallbacks in the test. The test should also
assert each mapped local SVG exists.

## Compatibility

No provider registry, credential, model, or daemon behavior changes. This is a
desktop visual mapping change only.
