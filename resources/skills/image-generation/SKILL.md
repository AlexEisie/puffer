---
name: image-generation
description: Use when the user asks to create, generate, or render images; load this skill before calling the ImageGeneration tool.
allowed-tools:
  - ImageGeneration
user-invocable: true
disable-model-invocation: false
---

Use `ImageGeneration` for image generation requests.

- Call `ImageGeneration` once for one logical image-generation request.
- When the user asks for multiple images from one prompt, set `count` to the requested number within the tool's supported range instead of issuing repeated single-image calls.
- Treat `prompt` as literal text unless it names a workspace-relative file; the tool can read that prompt file.
- Use `promptReference` only when the request supplies additional prompt context.
- If image generation fails or the media runtime is unavailable, report that plainly.
- Do not hand-author SVG, ASCII art, placeholder files, or other substitutes and present them as generated images.
