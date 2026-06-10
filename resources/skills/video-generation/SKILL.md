---
name: video-generation
description: Use when the user asks to create or generate a text-to-video clip; load this skill before calling the VideoGeneration tool.
allowed-tools:
  - VideoGeneration
user-invocable: true
disable-model-invocation: false
---

Use `VideoGeneration` for text-to-video generation requests.

- Call `VideoGeneration` once for one logical video-generation request.
- Treat `prompt` as literal text unless it names a workspace-relative file; the tool can read that prompt file.
- Pass `parameters` only for requested scalar overrides: strings, numbers, or booleans.
- This tool is text-to-video only. If the user asks for reference images, first frames, last frames, or image-to-video behavior, state that this tool does not support that input instead of calling it with image references.
- If video generation fails or the media runtime is unavailable, report that plainly.
- Do not imply a video was created unless the tool returns a persisted video artifact.
