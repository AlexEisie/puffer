<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { ensureLocalDaemonClient } from "../../api/daemonClient";
  import {
    browserRecording,
    type BrowserRecordedFrame
  } from "../../api/desktop";
  import Icon from "../../design/Icon.svelte";
  import HighlightedLine from "../../components/HighlightedLine.svelte";
  import type { TimelineItem, ToolTimelineItem } from "../../types";

  type Props = {
    timeline: TimelineItem[];
    sessionId?: string | null;
  };

  let { timeline, sessionId = null }: Props = $props();

  type ActionKind = "write" | "terminal" | "browser";
  type DetailRow = { kind: "ctx" | "add" | "del" | "omit"; line: number | null; text: string };
  type ActionItem = {
    id: string;
    kind: ActionKind;
    toolName: string;
    title: string;
    summary: string;
    input: Record<string, unknown> | null;
    output: Record<string, unknown> | null;
    rawInput: string;
    rawOutput: string;
    status: string;
    index: number;
  };
  type RecordingFrame = BrowserRecordedFrame & { src: string };

  const fileTools = new Set([
    "write",
    "write_file",
    "edit",
    "edit_file",
    "replace",
    "replace_in_file",
    "multiedit",
    "multi_edit",
    "notebookedit"
  ]);
  const terminalTools = new Set(["bash", "shell", "powershell"]);
  const browserTools = new Set(["browser"]);

  function parseJsonObject(text: string): Record<string, unknown> | null {
    try {
      const value = JSON.parse(text);
      return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : null;
    } catch {
      return null;
    }
  }

  function stringField(obj: Record<string, unknown> | null, names: string[]): string | null {
    if (!obj) return null;
    for (const name of names) {
      const value = obj[name];
      if (typeof value === "string") return value;
    }
    return null;
  }

  function actionKind(toolName: string): ActionKind | null {
    const normalized = toolName.toLowerCase();
    if (fileTools.has(normalized)) return "write";
    if (terminalTools.has(normalized)) return "terminal";
    if (browserTools.has(normalized)) return "browser";
    return null;
  }

  function actionLabel(action: ActionItem): string {
    if (action.kind === "write") {
      return stringField(action.input, ["file_path", "path"]) ?? action.toolName;
    }
    if (action.kind === "terminal") {
      return stringField(action.input, ["command"]) ?? action.toolName;
    }
    return [
      stringField(action.input, ["action"]) ?? action.toolName,
      stringField(action.input, ["tabId", "label", "url"])
    ].filter(Boolean).join(" ");
  }

  function buildActions(items: TimelineItem[]): ActionItem[] {
    const out: ActionItem[] = [];
    for (const item of items) {
      if (item.kind !== "tool") continue;
      const tool = item as ToolTimelineItem;
      const kind = actionKind(tool.toolName || "");
      if (!kind) continue;
      const input = tool.inputJson ?? parseJsonObject(tool.input);
      const output = parseJsonObject(tool.output);
      out.push({
        id: tool.id,
        kind,
        toolName: tool.toolName,
        title: tool.title,
        summary: tool.summary,
        input,
        output,
        rawInput: tool.input,
        rawOutput: tool.output,
        status: tool.status,
        index: out.length
      });
    }
    return out;
  }

  let actions = $derived(buildActions(timeline));
  let selectedId = $state<string | null>(null);
  let selected = $derived(actions.find((action) => action.id === selectedId) ?? actions.at(-1) ?? null);
  let recordingFrames = $state<RecordingFrame[]>([]);
  let selectedFrameId = $state<string | null>(null);
  let recordingDisposer: (() => void) | null = null;

  onMount(() => {
    if (!sessionId) return;
    void loadBrowserRecording();
    void subscribeBrowserRecording();
  });

  onDestroy(() => {
    recordingDisposer?.();
    recordingDisposer = null;
  });

  $effect(() => {
    if (!selectedId && actions.length > 0) selectedId = actions.at(-1)?.id ?? null;
    if (selectedId && !actions.some((action) => action.id === selectedId)) {
      selectedId = actions.at(-1)?.id ?? null;
    }
  });

  function compactRows(rows: DetailRow[], limit = 180): DetailRow[] {
    if (rows.length <= limit) return rows;
    const head = Math.floor(limit * 0.65);
    const tail = limit - head;
    return [
      ...rows.slice(0, head),
      { kind: "omit", line: null, text: `${rows.length - limit} unchanged lines omitted` },
      ...rows.slice(rows.length - tail)
    ];
  }

  function diffRows(oldText: string | null, newText: string): DetailRow[] {
    const oldLines = oldText === null ? [] : oldText.split("\n");
    const newLines = newText.split("\n");
    if (oldText === null) {
      return compactRows(newLines.map((line, index) => ({ kind: "add", line: index + 1, text: line })));
    }
    let prefix = 0;
    while (prefix < oldLines.length && prefix < newLines.length && oldLines[prefix] === newLines[prefix]) {
      prefix += 1;
    }
    let suffix = 0;
    while (
      suffix < oldLines.length - prefix &&
      suffix < newLines.length - prefix &&
      oldLines[oldLines.length - 1 - suffix] === newLines[newLines.length - 1 - suffix]
    ) {
      suffix += 1;
    }
    const context = 4;
    const start = Math.max(0, prefix - context);
    const oldTail = oldLines.length - suffix;
    const newTail = newLines.length - suffix;
    const rows: DetailRow[] = [];
    if (start > 0) rows.push({ kind: "omit", line: null, text: `${start} unchanged lines omitted` });
    for (let i = start; i < prefix; i += 1) rows.push({ kind: "ctx", line: i + 1, text: oldLines[i] });
    for (let i = prefix; i < oldTail; i += 1) rows.push({ kind: "del", line: null, text: oldLines[i] });
    for (let i = prefix; i < newTail; i += 1) rows.push({ kind: "add", line: i + 1, text: newLines[i] });
    const end = Math.min(newLines.length, newTail + context);
    for (let i = newTail; i < end; i += 1) rows.push({ kind: "ctx", line: i + 1, text: newLines[i] });
    if (end < newLines.length) rows.push({ kind: "omit", line: null, text: `${newLines.length - end} unchanged lines omitted` });
    return compactRows(rows);
  }

  function writePath(action: ActionItem): string {
    return stringField(action.input, ["file_path", "path"]) ?? "file";
  }

  function writeRows(action: ActionItem): DetailRow[] {
    const name = action.toolName.toLowerCase();
    if (name.includes("write")) {
      const content = stringField(action.input, ["content", "contents"]) ?? "";
      const original = stringField(action.output, ["originalFile", "original_file"]);
      return diffRows(original, content);
    }
    const oldText = stringField(action.input, ["old", "old_string", "oldText"]) ?? "";
    const newText = stringField(action.input, ["new", "new_string", "newText"]) ?? "";
    return diffRows(oldText, newText);
  }

  function stdout(action: ActionItem): string {
    return stringField(action.output, ["stdout"]) ?? "";
  }

  function stderr(action: ActionItem): string {
    return stringField(action.output, ["stderr"]) ?? "";
  }

  function terminalCommand(action: ActionItem): string {
    return stringField(action.input, ["command"]) ?? action.rawInput;
  }

  function toRecordingFrame(frame: BrowserRecordedFrame): RecordingFrame {
    return {
      ...frame,
      src: `data:${frame.mimeType || "image/jpeg"};base64,${frame.data}`
    };
  }

  function mergeRecordingFrame(frame: BrowserRecordedFrame) {
    const next = toRecordingFrame(frame);
    if (recordingFrames.some((item) => item.frameId === next.frameId)) return;
    recordingFrames = [...recordingFrames, next].slice(-240);
  }

  async function loadBrowserRecording() {
    if (!sessionId) return;
    try {
      const snapshot = await browserRecording(sessionId);
      recordingFrames = snapshot.frames.map(toRecordingFrame);
    } catch {
      recordingFrames = [];
    }
  }

  async function subscribeBrowserRecording() {
    if (!sessionId) return;
    const client = await ensureLocalDaemonClient();
    recordingDisposer?.();
    recordingDisposer = client.on<BrowserRecordedFrame>(`browser:${sessionId}:recording`, mergeRecordingFrame);
  }

  function framesForAction(action: ActionItem | null): RecordingFrame[] {
    if (!action || action.kind !== "browser") return [];
    const tabId = stringField(action.input, ["tabId"]);
    if (!tabId) return recordingFrames;
    return recordingFrames.filter((frame) => frame.tabId === tabId);
  }

  let visibleFrames = $derived(framesForAction(selected));
  let selectedFrame = $derived(
    visibleFrames.find((frame) => frame.frameId === selectedFrameId) ?? visibleFrames.at(-1) ?? null
  );

  $effect(() => {
    if (selected?.kind !== "browser") return;
    if (!selectedFrameId && visibleFrames.length > 0) selectedFrameId = visibleFrames.at(-1)?.frameId ?? null;
    if (selectedFrameId && !visibleFrames.some((frame) => frame.frameId === selectedFrameId)) {
      selectedFrameId = visibleFrames.at(-1)?.frameId ?? null;
    }
  });

  function segmentWidth(action: ActionItem): number {
    const base = action.kind === "browser" ? 34 : action.kind === "terminal" ? 38 : 32;
    return Math.min(90, base + Math.max(0, action.summary.length / 8));
  }
</script>

<aside class="pf-action-history">
  <header class="pf-action-head">
    <div>
      <div class="eyebrow">Action history</div>
      <h2>Trajectory</h2>
    </div>
    <div class="counts">
      <span data-kind="write">{actions.filter((a) => a.kind === "write").length}</span>
      <span data-kind="terminal">{actions.filter((a) => a.kind === "terminal").length}</span>
      <span data-kind="browser">{actions.filter((a) => a.kind === "browser").length}</span>
    </div>
  </header>

  {#if !selected}
    <div class="empty">
      <Icon name="layers" size={22} />
      <div>No agent actions yet</div>
    </div>
  {:else}
    <section class="detail" data-kind={selected.kind}>
      <div class="detail-head">
        <span class="kind-dot"></span>
        <div class="detail-title" title={actionLabel(selected)}>{actionLabel(selected)}</div>
        <span class="status">{selected.status}</span>
      </div>

      {#if selected.kind === "write"}
        <div class="file-shell">
          <div class="file-path" title={writePath(selected)}>{writePath(selected)}</div>
          <div class="diff-lines">
            {#each writeRows(selected) as row, i (i)}
              <div class="diff-row {row.kind}">
                <span class="gutter">{row.line ?? ""}</span>
                <span class="mark">{row.kind === "add" ? "+" : row.kind === "del" ? "-" : row.kind === "omit" ? "..." : ""}</span>
                <code><HighlightedLine text={row.text || " "} path={writePath(selected)} /></code>
              </div>
            {/each}
          </div>
        </div>
      {:else if selected.kind === "terminal"}
        <div class="pty-shell">
          <div class="pty-stdin">
            <span>stdin</span>
            <pre>{terminalCommand(selected)}</pre>
          </div>
          <div class="pty-stream">
            <div class="stream-label">stdout</div>
            <pre>{stdout(selected) || "(no stdout)"}</pre>
          </div>
          {#if stderr(selected)}
            <div class="pty-stream err">
              <div class="stream-label">stderr</div>
              <pre>{stderr(selected)}</pre>
            </div>
          {/if}
        </div>
      {:else}
        <div class="browser-recording">
          {#if selectedFrame}
            <figure class="browser-screen">
              <img src={selectedFrame.src} alt={selectedFrame.title || selectedFrame.url || "Browser recording frame"} />
              <figcaption>
                <span>{selectedFrame.title || "Browser"}</span>
                <span>{selectedFrame.url}</span>
              </figcaption>
            </figure>
            <div class="browser-strip" aria-label="Browser screen recording">
              {#each visibleFrames as frame (frame.frameId)}
                <button
                  type="button"
                  class="browser-frame"
                  class:selected={frame.frameId === selectedFrame.frameId}
                  onclick={() => (selectedFrameId = frame.frameId)}
                  title={frame.title || frame.url}
                >
                  <img src={frame.src} alt="" />
                </button>
              {/each}
            </div>
          {:else}
            <div class="recording-empty">
              <Icon name="globe" size={22} />
              <div>No browser frames recorded yet</div>
            </div>
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  <footer class="trajectory" aria-label="Agent action trajectory">
    {#if actions.length === 0}
      <div class="trajectory-empty">No trajectory</div>
    {:else}
      {#each actions as action (action.id)}
        <button
          type="button"
          class="segment"
          class:selected={action.id === selected?.id}
          data-kind={action.kind}
          style:width={`${segmentWidth(action)}px`}
          title={`${action.kind}: ${actionLabel(action)}`}
          onclick={() => (selectedId = action.id)}
        >
          <span></span>
        </button>
      {/each}
    {/if}
  </footer>
</aside>

<style>
  .pf-action-history {
    height: 100%;
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    background: var(--background);
    color: var(--foreground);
  }

  .pf-action-head {
    height: 58px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .eyebrow {
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0;
  }

  h2 {
    margin: 1px 0 0;
    font-size: 14px;
    letter-spacing: 0;
  }

  .counts {
    display: flex;
    gap: 4px;
  }

  .counts span {
    min-width: 22px;
    height: 20px;
    border-radius: 5px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: 11px;
    border: 1px solid var(--border);
  }

  [data-kind="write"] { --action: oklch(0.62 0.16 145); }
  [data-kind="terminal"] { --action: oklch(0.58 0.14 255); }
  [data-kind="browser"] { --action: oklch(0.66 0.17 35); }

  .counts [data-kind] {
    color: var(--action);
    background: color-mix(in oklab, var(--action) 10%, transparent);
  }

  .empty {
    display: grid;
    place-items: center;
    align-content: center;
    gap: 8px;
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .detail {
    min-height: 0;
    overflow: auto;
    padding: 12px;
  }

  .detail-head {
    height: 34px;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin-bottom: 10px;
  }

  .kind-dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--action);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--action) 14%, transparent);
    flex-shrink: 0;
  }

  .detail-title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
    font-size: 13px;
  }

  .status {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--muted-foreground);
  }

  .file-shell,
  .pty-shell,
  .browser-recording {
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    background: color-mix(in oklab, var(--muted) 20%, var(--background));
  }

  .file-path {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .diff-lines {
    padding: 6px 0;
    overflow: auto;
  }

  .diff-row {
    display: grid;
    grid-template-columns: 42px 24px minmax(0, 1fr);
    min-height: 20px;
    line-height: 20px;
    font-family: var(--font-mono);
    font-size: 11px;
  }

  .diff-row .gutter,
  .diff-row .mark {
    color: var(--muted-foreground);
    user-select: none;
    text-align: right;
    padding-right: 8px;
  }

  .diff-row code {
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .diff-row.add { background: color-mix(in oklab, oklch(0.72 0.18 145) 12%, transparent); }
  .diff-row.del { background: color-mix(in oklab, oklch(0.62 0.18 25) 12%, transparent); }
  .diff-row.omit { color: var(--muted-foreground); background: var(--muted); }

  .pty-shell {
    padding: 0;
    background: #0b0f14;
    color: #d7e0ea;
  }

  .pty-stdin,
  .pty-stream {
    border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  }

  .pty-stdin span,
  .stream-label {
    display: block;
    padding: 7px 10px 0;
    color: #7d8b99;
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
  }

  pre {
    margin: 0;
    padding: 8px 10px 10px;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.45;
  }

  .pty-stream.err {
    color: #ffb4a9;
  }

  .browser-recording {
    display: grid;
    gap: 8px;
    padding: 8px;
    background: color-mix(in oklab, var(--muted) 30%, var(--background));
  }

  .browser-screen {
    margin: 0;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
    background: var(--background);
  }

  .browser-screen img {
    width: 100%;
    display: block;
    background: #fff;
    aspect-ratio: 16 / 10;
    object-fit: contain;
  }

  .browser-screen figcaption {
    display: grid;
    gap: 8px;
    padding: 7px 9px;
    border-top: 1px solid var(--border);
    font-size: 11px;
  }

  .browser-screen figcaption span:first-child {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .browser-screen figcaption span:last-child {
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .browser-strip {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(92px, 1fr));
    gap: 6px;
  }

  .browser-frame {
    border: 1px solid var(--border);
    background: var(--background);
    border-radius: 5px;
    padding: 2px;
    cursor: pointer;
    aspect-ratio: 16 / 10;
    overflow: hidden;
  }

  .browser-frame img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
  }

  .browser-frame.selected {
    border-color: var(--action);
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--action) 18%, transparent);
  }

  .recording-empty {
    min-height: 220px;
    display: grid;
    place-items: center;
    align-content: center;
    gap: 8px;
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .trajectory {
    min-height: 64px;
    border-top: 1px solid var(--border);
    padding: 10px;
    display: flex;
    align-items: end;
    gap: 3px;
    overflow-x: auto;
    background: color-mix(in oklab, var(--muted) 36%, var(--background));
  }

  .trajectory-empty {
    color: var(--muted-foreground);
    font-size: 12px;
    margin: auto;
  }

  .segment {
    height: 28px;
    min-width: 16px;
    border: 0;
    border-radius: 4px;
    background: color-mix(in oklab, var(--action) 28%, var(--background));
    padding: 0;
    cursor: pointer;
    position: relative;
  }

  .segment span {
    position: absolute;
    inset: auto 3px 3px;
    height: 4px;
    border-radius: 999px;
    background: var(--action);
  }

  .segment.selected {
    outline: 2px solid var(--action);
    outline-offset: 1px;
  }
</style>
