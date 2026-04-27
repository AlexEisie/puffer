<script lang="ts">
  import "../../design/chat.css";

  import { tick } from "svelte";
  import Puffer from "../../design/Puffer.svelte";
  import Icon, { type IconName } from "../../design/Icon.svelte";
  import MessageBody from "../../components/MessageBody.svelte";
  import ToolCard from "./ToolCard.svelte";
  import DiffCard from "./DiffCard.svelte";
  import Approval from "./Approval.svelte";
  import QuestionPrompt from "./QuestionPrompt.svelte";
  import type {
    PermissionTimelineItem,
    SessionListItem,
    TimelineItem,
    ToolTimelineItem,
    DiffTimelineItem,
    MessageTimelineItem,
    UserQuestionTimelineItem
  } from "../../types";
  import type { AgentState } from "../../shell/tweaks";

  type Props = {
    session: SessionListItem | null;
    agentName?: string;
    agentState?: AgentState;
    timeline: TimelineItem[];
    pendingPermissions: PermissionTimelineItem[];
    pendingQuestions: UserQuestionTimelineItem[];
    loading: boolean;
    /** True while an agent turn is running on the current session. Flips
     *  the composer's send button into a red "Stop" so the user can
     *  interrupt a runaway loop. */
    turnRunning?: boolean;
    turnStartedAtMs?: number | null;
    turnThinking?: boolean;
    turnStatusHint?: string | null;
    onSubmitMessage: (message: string) => void;
    onResolvePermission: (permissionId: string, choice: string) => void;
    onResolveUserQuestion: (
      questionId: string,
      answers: Record<string, string | string[]>,
      annotations?: Record<string, Record<string, string>>
    ) => void;
    onCancelTurn?: () => void;
  };

  let {
    session,
    agentName = "Puffer",
    agentState = "idle",
    timeline,
    pendingPermissions,
    pendingQuestions,
    loading,
    turnRunning = false,
    turnStartedAtMs = null,
    turnThinking = false,
    turnStatusHint = null,
    onSubmitMessage,
    onResolvePermission,
    onResolveUserQuestion,
    onCancelTurn
  }: Props = $props();

  let draft = $state("");
  let threadEl: HTMLDivElement | undefined;
  let lastSessionId: string | null = null;
  let nowMs = $state(Date.now());
  let expandedIntermediateIds = $state<string[]>([]);
  let expandedActivityIds = $state<string[]>([]);
  let selectedActivityChildren = $state<Record<string, string>>({});
  let activityGridWidths = $state<Record<string, number>>({});

  // Rolled-up thread: tool / diff items stay attached to the related assistant
  // message, but render before the prose because the tool work happened first.
  type RowKind =
    | { kind: "user"; item: MessageTimelineItem }
    | { kind: "system"; item: MessageTimelineItem }
    | {
        kind: "agent";
        item: MessageTimelineItem | null;
        children: (ToolTimelineItem | DiffTimelineItem)[];
        approvals: PermissionTimelineItem[];
        questions: UserQuestionTimelineItem[];
      };

  function buildRows(items: TimelineItem[]): RowKind[] {
    const rows: RowKind[] = [];
    let current:
      | Extract<RowKind, { kind: "agent" }>
      | null = null;
    for (const item of items) {
      if (item.kind === "user") {
        if (current) { rows.push(current); current = null; }
        rows.push({ kind: "user", item: item as MessageTimelineItem });
      } else if (item.kind === "system") {
        if (current) { rows.push(current); current = null; }
        rows.push({ kind: "system", item: item as MessageTimelineItem });
      } else if (item.kind === "assistant" || item.kind === "command") {
        if (current && !current.item) {
          current.item = item as MessageTimelineItem;
        } else {
          if (current) rows.push(current);
          current = {
            kind: "agent",
            item: item as MessageTimelineItem,
            children: [],
            approvals: [],
            questions: []
          };
        }
      } else if (item.kind === "tool") {
        if (!current) current = { kind: "agent", item: null, children: [], approvals: [], questions: [] };
        current.children.push(item as ToolTimelineItem);
      } else if (item.kind === "diff") {
        if (!current) current = { kind: "agent", item: null, children: [], approvals: [], questions: [] };
        current.children.push(item as DiffTimelineItem);
      } else if (item.kind === "question") {
        if (!current) current = { kind: "agent", item: null, children: [], approvals: [], questions: [] };
        current.questions.push(item as UserQuestionTimelineItem);
      }
    }
    if (current) rows.push(current);
    return rows;
  }

  let rows = $derived(
    buildRows(
      timeline.filter((i) => i.kind !== "permission" && !(i.kind === "question" && i.status === "pending"))
    )
  );

  function formatTime(ms: number | undefined): string {
    if (!ms) return "";
    const d = new Date(ms);
    const h = d.getHours();
    const m = d.getMinutes().toString().padStart(2, "0");
    const hh = h < 10 ? `0${h}` : `${h}`;
    return `${hh}:${m}`;
  }

  function formatElapsed(startedAtMs: number | null): string {
    if (!startedAtMs) return "";
    const elapsed = Math.max(0, nowMs - startedAtMs) / 1000;
    return elapsed < 10 ? `${elapsed.toFixed(1)}s` : `${Math.floor(elapsed)}s`;
  }

  $effect(() => {
    // On session change, reset scroll to top so users see the start.
    if (session?.id !== lastSessionId) {
      lastSessionId = session?.id ?? null;
      void tick().then(() => threadEl?.scrollTo({ top: 0, behavior: "auto" }));
    }
  });

  $effect(() => {
    if (!turnRunning || !turnStartedAtMs) return;
    nowMs = Date.now();
    const interval = window.setInterval(() => {
      nowMs = Date.now();
    }, 100);
    return () => window.clearInterval(interval);
  });

  async function submit() {
    const v = draft.trim();
    if (!v) return;
    onSubmitMessage(v);
    draft = "";
    await tick();
    threadEl?.scrollTo({ top: threadEl.scrollHeight, behavior: "smooth" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  // Distribute any pending permissions under the latest agent row so the
  // approval prompt sits with the tool call it's asking about.
  let distributedRows = $derived.by(() => {
    const out = [...rows];
    if (!pendingPermissions.length && !pendingQuestions.length) return out;
    // attach to the last agent row (or append a synthetic one)
    const lastAgentIdx = (() => {
      for (let i = out.length - 1; i >= 0; i--) if (out[i].kind === "agent") return i;
      return -1;
    })();
    if (lastAgentIdx >= 0 && out[lastAgentIdx].kind === "agent") {
      const prev = out[lastAgentIdx] as Extract<RowKind, { kind: "agent" }>;
      out[lastAgentIdx] = {
        ...prev,
        approvals: [...prev.approvals, ...pendingPermissions],
        questions: [...prev.questions, ...pendingQuestions]
      };
    } else {
      out.push({
        kind: "agent",
        item: null,
        children: [],
        approvals: [...pendingPermissions],
        questions: [...pendingQuestions]
      });
    }
    return out;
  });

  let typingLabel = $derived.by(() => {
    const elapsed = formatElapsed(turnStartedAtMs);
    const suffix = elapsed ? ` (${elapsed})` : "";
    if (turnRunning) {
      if (turnStatusHint) return `${turnStatusHint}${suffix}`;
      if (turnThinking) return `Thinking${suffix}`;
      return `Running${suffix}`;
    }
    if (agentState === "awaiting") return `${agentName} paused - waiting for your response`;
    return null;
  });

  let finalResponseIndex = $derived.by(() => {
    if (turnRunning) return -1;
    for (let i = distributedRows.length - 1; i >= 0; i -= 1) {
      const row = distributedRows[i];
      if (row.kind === "agent" && row.item?.body.trim()) return i;
    }
    return -1;
  });

  function isIntermediateAgentMessage(row: RowKind, idx: number): boolean {
    return !turnRunning
      && row.kind === "agent"
      && Boolean(row.item?.body.trim())
      && idx !== finalResponseIndex;
  }

  function intermediateId(row: Extract<RowKind, { kind: "agent" }>, idx: number): string {
    return row.item?.id ?? `agent-${idx}`;
  }

  function intermediatePreview(row: Extract<RowKind, { kind: "agent" }>): string {
    const body = row.item?.body.trim().replace(/\s+/g, " ") ?? "";
    return body.length > 160 ? `${body.slice(0, 160).trimEnd()}...` : body;
  }

  function intermediateExpanded(id: string): boolean {
    return expandedIntermediateIds.includes(id);
  }

  function toggleIntermediate(id: string) {
    expandedIntermediateIds = intermediateExpanded(id)
      ? expandedIntermediateIds.filter((value) => value !== id)
      : [...expandedIntermediateIds, id];
  }

  type ActivityCategory = "write" | "read" | "browser" | "terminal" | "search" | "diff" | "other";

  type ActivitySummary = {
    icons: IconName[];
    text: string;
    failed: number;
  };

  const activityOrder: ActivityCategory[] = ["write", "read", "browser", "terminal", "search", "diff", "other"];
  const activityMinItemWidth = 260;
  const activityGridGap = 8;

  function shouldCollapseActivity(row: Extract<RowKind, { kind: "agent" }>): boolean {
    return !turnRunning && row.children.length > 0 && Boolean(row.item?.body.trim());
  }

  function activityGroupId(row: Extract<RowKind, { kind: "agent" }>, idx: number): string {
    return row.item?.id ?? row.children[0]?.id ?? `activity-${idx}`;
  }

  function activityExpanded(id: string): boolean {
    return expandedActivityIds.includes(id);
  }

  function toggleActivity(id: string) {
    expandedActivityIds = activityExpanded(id)
      ? expandedActivityIds.filter((value) => value !== id)
      : [...expandedActivityIds, id];
  }

  function trackActivityGrid(node: HTMLDivElement, id: string) {
    const resize = () => {
      activityGridWidths = { ...activityGridWidths, [id]: node.clientWidth };
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(node);
    return {
      destroy() {
        observer.disconnect();
        const { [id]: _removed, ...rest } = activityGridWidths;
        activityGridWidths = rest;
      }
    };
  }

  function activityColumns(id: string, count: number): number {
    const width = activityGridWidths[id] ?? 0;
    if (width <= 0) return 1;
    return Math.max(1, Math.min(count, Math.floor((width + activityGridGap) / (activityMinItemWidth + activityGridGap))));
  }

  function activityActionOrder(activityId: string, childIdx: number, count: number): number {
    return Math.floor(childIdx / activityColumns(activityId, count)) * 2;
  }

  function activityPanelOrder(activityId: string, childIdx: number, count: number): number {
    return activityActionOrder(activityId, childIdx, count) + 1;
  }

  function activityChildSelected(activityId: string, childId: string): boolean {
    return selectedActivityChildren[activityId] === childId;
  }

  function toggleActivityChild(activityId: string, childId: string) {
    const { [activityId]: current, ...rest } = selectedActivityChildren;
    selectedActivityChildren = current === childId ? rest : { ...rest, [activityId]: childId };
  }

  function selectedActivityChild(
    children: (ToolTimelineItem | DiffTimelineItem)[],
    activityId: string
  ): { child: ToolTimelineItem | DiffTimelineItem; idx: number } | null {
    const childId = selectedActivityChildren[activityId];
    if (!childId) return null;
    const idx = children.findIndex((child) => child.id === childId);
    if (idx < 0) return null;
    return { child: children[idx], idx };
  }

  function activityIcon(category: ActivityCategory): IconName {
    if (category === "write") return "edit";
    if (category === "read") return "file";
    if (category === "browser") return "globe";
    if (category === "terminal") return "terminal";
    if (category === "search") return "search";
    if (category === "diff") return "git";
    return "bolt";
  }

  function childActivityCategory(child: ToolTimelineItem | DiffTimelineItem): ActivityCategory {
    if (child.kind === "diff") return "diff";
    const name = child.toolName.toLowerCase();
    if (name.includes("browser") || name.includes("web") || name.includes("fetch")) return "browser";
    if (name.includes("edit") || name.includes("write") || name.includes("replace") || name.includes("patch")) return "write";
    if (name.includes("read") || name.includes("view")) return "read";
    if (name.includes("bash") || name.includes("shell") || name.includes("exec") || name.includes("terminal")) return "terminal";
    if (name.includes("grep") || name.includes("glob") || name.includes("search")) return "search";
    if (name.includes("git") || name.includes("diff")) return "diff";
    return "other";
  }

  function childFailed(child: ToolTimelineItem | DiffTimelineItem): boolean {
    const status = (child.status ?? "").toLowerCase();
    return status.includes("err") || status.includes("fail");
  }

  function activityStatus(child: ToolTimelineItem | DiffTimelineItem): string {
    const status = (child.status ?? "").toLowerCase();
    if (status.includes("run") || status === "pending") return "running";
    if (status.includes("err") || status.includes("fail")) return "failed";
    return "done";
  }

  function parseInputObject(child: ToolTimelineItem): Record<string, unknown> | null {
    if (child.inputJson) return child.inputJson;
    try {
      const parsed = JSON.parse(child.input);
      return typeof parsed === "object" && parsed !== null ? (parsed as Record<string, unknown>) : null;
    } catch {
      return null;
    }
  }

  function inputString(input: Record<string, unknown> | null, names: string[]): string | null {
    if (!input) return null;
    for (const name of names) {
      const value = input[name];
      if (typeof value === "string" && value.trim()) return value;
    }
    return null;
  }

  function browserActionArg(input: Record<string, unknown> | null): string | null {
    const action = inputString(input, ["action"]);
    if (!action) return null;
    const label = action.charAt(0).toUpperCase() + action.slice(1);
    const url = inputString(input, ["url"]);
    const ref = inputString(input, ["ref"]);
    const text = inputString(input, ["text"]);
    const key = inputString(input, ["key"]);
    if (action === "list") return "List";
    if ((action === "open" || action === "navigate") && url) return `${label} ${url}`;
    if ((action === "click" || action === "focus" || action === "close") && ref) return `${label} ${ref}`;
    if ((action === "type" || action === "fill") && text) return `${label} ${text}`;
    if (action === "press" && key) return `${label} ${key}`;
    return label;
  }

  function activityActionName(child: ToolTimelineItem | DiffTimelineItem): string {
    if (child.kind === "diff") return "Diff";
    return child.toolName && child.toolName !== "undefined" ? child.toolName : "Tool";
  }

  function activityActionArg(child: ToolTimelineItem | DiffTimelineItem): string {
    if (child.kind === "diff") return child.diff.title;
    const input = parseInputObject(child);
    if (child.toolName.toLowerCase() === "browser") return browserActionArg(input) ?? "Action";
    const value = inputString(input, ["path", "file_path", "url", "command", "pattern", "query", "cwd"]);
    const fallback = value ?? child.summary ?? child.title ?? "";
    const compact = fallback.replace(/\s+/g, " ").trim();
    return compact.length > 96 ? `${compact.slice(0, 95)}...` : compact;
  }

  function plural(count: number, singular: string, pluralValue = `${singular}s`): string {
    return `${count} ${count === 1 ? singular : pluralValue}`;
  }

  function activitySummary(children: (ToolTimelineItem | DiffTimelineItem)[]): ActivitySummary {
    const counts = new Map<ActivityCategory, number>();
    const failures = new Map<ActivityCategory, number>();
    for (const child of children) {
      const category = childActivityCategory(child);
      counts.set(category, (counts.get(category) ?? 0) + 1);
      if (childFailed(child)) failures.set(category, (failures.get(category) ?? 0) + 1);
    }

    const parts: string[] = [];
    const writeCount = counts.get("write") ?? 0;
    const writeFailures = failures.get("write") ?? 0;
    if (writeCount > 0) {
      parts.push(writeFailures === writeCount ? `Tried writing ${plural(writeCount, "file")}` : `Wrote ${plural(writeCount, "file")}`);
    }
    const readCount = counts.get("read") ?? 0;
    if (readCount > 0) parts.push(`Read ${plural(readCount, "file")}`);
    const browserCount = counts.get("browser") ?? 0;
    if (browserCount > 0) parts.push("Interacted with browser");
    const terminalCount = counts.get("terminal") ?? 0;
    if (terminalCount > 0) parts.push(`Ran ${plural(terminalCount, "command")}`);
    const searchCount = counts.get("search") ?? 0;
    if (searchCount > 0) parts.push(searchCount === 1 ? "Searched" : `Searched ${searchCount} times`);
    const diffCount = counts.get("diff") ?? 0;
    if (diffCount > 0) parts.push(`Updated ${plural(diffCount, "diff", "diffs")}`);
    const otherCount = counts.get("other") ?? 0;
    if (otherCount > 0) parts.push(`Used ${plural(otherCount, "tool")}`);

    const icons = activityOrder
      .filter((category) => (counts.get(category) ?? 0) > 0)
      .map(activityIcon);
    const failed = Array.from(failures.values()).reduce((sum, count) => sum + count, 0);

    return {
      icons,
      text: parts.join(", ") || `Used ${plural(children.length, "tool")}`,
      failed
    };
  }
</script>

<div class="pf-chat">
  <div class="pf-chat-thread" bind:this={threadEl}>
    <div class="pf-chat-thread-inner">
      {#if loading && rows.length === 0}
        <div class="state">Loading conversation…</div>
      {:else if rows.length === 0 && !typingLabel}
        <div class="state">No messages in this session yet. Send a prompt to get started.</div>
      {:else}
        {#each distributedRows as row, idx (idx)}
          {#if row.kind === "user"}
            <div class="pf-msg" data-role="user">
              <div class="pf-msg-avatar">Y</div>
              <div class="pf-msg-body">
                <div class="pf-msg-meta">
                  <span class="name">you</span>
                  <span class="time">{formatTime((row.item as MessageTimelineItem & { createdAtMs?: number }).createdAtMs)}</span>
                </div>
                <div class="pf-msg-text">
                  <MessageBody body={row.item.body} />
                </div>
              </div>
            </div>
          {:else if row.kind === "system"}
            {@const isError = row.item.status === "error" || row.item.meta.includes("error")}
            <div class="pf-msg" data-role="system" data-error={isError}>
              <div class="pf-msg-avatar">{isError ? "err" : "sys"}</div>
              <div class="pf-msg-body">
                {#if isError}
                  <div class="pf-msg-meta">
                    <span class="name">{row.item.title || "Error"}</span>
                  </div>
                {/if}
                <div class="pf-msg-text">
                  <MessageBody body={row.item.body} />
                </div>
              </div>
            </div>
          {:else}
            <div class="pf-msg" data-role="agent">
              <div class="pf-msg-avatar"><Puffer size={26} state="idle" /></div>
              <div class="pf-msg-body">
                <div class="pf-msg-meta">
                  <span class="name">{agentName}</span>
                </div>
                {#if row.children.length || row.approvals.length || row.questions.length}
                  <div class="agent-tools">
                    {#if row.children.length}
                      {#if shouldCollapseActivity(row)}
                        {@const activityId = activityGroupId(row, idx)}
                        {@const summary = activitySummary(row.children)}
                        <div class="activity-group" data-expanded={activityExpanded(activityId)}>
                          <button
                            type="button"
                            class="activity-head"
                            onclick={() => toggleActivity(activityId)}
                            aria-expanded={activityExpanded(activityId)}
                          >
                            <span class="activity-chevron">
                              <Icon name={activityExpanded(activityId) ? "chevD" : "chevR"} size={11} />
                            </span>
                            <span class="activity-icons" aria-hidden="true">
                              {#each summary.icons as icon, iconIdx (`${icon}-${iconIdx}`)}
                                <span class="activity-icon">
                                  <Icon name={icon} size={13} />
                                </span>
                              {/each}
                            </span>
                            <span class="activity-copy">
                              <strong>Agent activity</strong>
                              <em>{summary.text}</em>
                            </span>
                            {#if summary.failed > 0}
                              <span class="activity-failed">{summary.failed} failed</span>
                            {/if}
                            <span class="activity-count">{row.children.length}</span>
                          </button>
                          {#if activityExpanded(activityId)}
                            {@const selected = selectedActivityChild(row.children, activityId)}
                            <div class="activity-details" use:trackActivityGrid={activityId}>
                              {#each row.children as child, childIdx (child.id)}
                                <button
                                  type="button"
                                  class="activity-action"
                                  class:selected={activityChildSelected(activityId, child.id)}
                                  style:order={activityActionOrder(activityId, childIdx, row.children.length)}
                                  onclick={() => toggleActivityChild(activityId, child.id)}
                                  aria-expanded={activityChildSelected(activityId, child.id)}
                                >
                                  <span class="activity-action-icon">
                                    <Icon name={activityIcon(childActivityCategory(child))} size={13} />
                                  </span>
                                  <span class="activity-action-name">{activityActionName(child)}</span>
                                  <span class="activity-action-arg" title={activityActionArg(child)}>
                                    {activityActionArg(child)}
                                  </span>
                                  <span class="activity-action-status" data-state={activityStatus(child)}>
                                    <span class="dot"></span>{activityStatus(child)}
                                  </span>
                                  <span class="activity-action-chevron" aria-hidden="true">
                                    <Icon name={activityChildSelected(activityId, child.id) ? "chevD" : "chevR"} size={11} />
                                  </span>
                                </button>
                              {/each}
                              {#if selected}
                                <div
                                  class="activity-panel"
                                  style:order={activityPanelOrder(activityId, selected.idx, row.children.length)}
                                >
                                  {#if selected.child.kind === "tool"}
                                    <ToolCard item={selected.child as ToolTimelineItem} sessionId={session?.id ?? null} defaultCollapsed={false} />
                                  {:else if selected.child.kind === "diff"}
                                    <DiffCard item={selected.child as DiffTimelineItem} defaultCollapsed={false} />
                                  {/if}
                                </div>
                              {/if}
                            </div>
                          {/if}
                        </div>
                      {:else}
                        {#each row.children as child (child.id)}
                          {#if child.kind === "tool"}
                            <ToolCard item={child as ToolTimelineItem} sessionId={session?.id ?? null} />
                          {:else if child.kind === "diff"}
                            <DiffCard item={child as DiffTimelineItem} />
                          {/if}
                        {/each}
                      {/if}
                    {/if}
                    {#each row.approvals as p (p.id)}
                      <Approval item={p} onResolve={onResolvePermission} />
                    {/each}
                    {#each row.questions as q (q.id)}
                      <QuestionPrompt item={q} onResolve={onResolveUserQuestion} />
                    {/each}
                  </div>
                {/if}
                {#if row.item}
                  {#if isIntermediateAgentMessage(row, idx)}
                    {@const msgId = intermediateId(row, idx)}
                    <div class="intermediate-card">
                      <button
                        type="button"
                        class="intermediate-head"
                        onclick={() => toggleIntermediate(msgId)}
                        aria-expanded={intermediateExpanded(msgId)}
                      >
                        <span><Icon name={intermediateExpanded(msgId) ? "chevD" : "chevR"} size={11} /></span>
                        <strong>Intermediate message</strong>
                        <em>{intermediatePreview(row)}</em>
                      </button>
                      {#if intermediateExpanded(msgId)}
                        <div class="intermediate-body pf-msg-text">
                          <MessageBody body={row.item.body} />
                        </div>
                      {/if}
                    </div>
                  {:else}
                    <div class="pf-msg-text">
                      <MessageBody body={row.item.body} />
                    </div>
                  {/if}
                {/if}
              </div>
            </div>
          {/if}
        {/each}

        {#if typingLabel}
          <div class="pf-msg" data-role="agent" style="opacity: 0.85;">
            <div class="pf-msg-avatar"><Puffer size={26} state={agentState} /></div>
            <div class="pf-msg-body">
              <div class="typing">{typingLabel}</div>
            </div>
          </div>
        {/if}
      {/if}
    </div>
  </div>

  <div class="pf-composer-wrap">
    <div class="pf-composer">
      <textarea
        bind:value={draft}
        placeholder={session ? `Reply to ${agentName}…` : "Select a session to continue"}
        onkeydown={onKeydown}
        disabled={!session}
      ></textarea>
      <div class="pf-composer-foot">
        {#if session}
          <button type="button" class="pf-chip"><Icon name="folder" size={11} />{session.folderPath ? session.folderPath.split("/").pop() : "cwd"}</button>
        {/if}
        <span class="spacer"></span>
        <span style="font-size: 11px; color: var(--muted-foreground); font-family: var(--font-mono);">
          ⏎ to send · ⇧⏎ for newline
        </span>
        {#if turnRunning}
          <button
            type="button"
            class="pf-send-btn pf-stop-btn"
            onclick={onCancelTurn}
            aria-label="Stop turn"
            title="Stop the running agent turn"
          >
            <Icon name="pause2" size={14} />
          </button>
        {:else}
          <button type="button" class="pf-send-btn" disabled={!draft.trim() || !session} onclick={submit} aria-label="Send">
            <Icon name="arrowUp" size={15} />
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .pf-chat {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--background);
  }
  .pf-chat-thread {
    flex: 1;
    overflow-y: auto;
    padding: 24px 0 24px;
  }
  .pf-chat-thread-inner {
    max-width: 820px;
    margin: 0 auto;
    padding: 0 32px;
    display: flex;
    flex-direction: column;
    gap: var(--puffer-row-gap, 14px);
  }
  .pf-composer-wrap {
    border-top: 1px solid var(--border);
    background: color-mix(in oklab, var(--background) 96%, var(--muted));
    padding: 14px 32px 18px;
    flex-shrink: 0;
  }
  .pf-composer {
    max-width: 820px;
    margin: 0 auto;
  }
  .agent-tools {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
  }
  .activity-group {
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: var(--background);
  }
  .activity-head {
    width: 100%;
    min-height: 42px;
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 9px;
    padding: 8px 10px;
    border: 0;
    background: color-mix(in oklab, var(--muted) 42%, var(--background));
    color: var(--foreground);
    cursor: pointer;
    font: inherit;
    text-align: left;
  }
  .activity-chevron {
    display: inline-flex;
    color: var(--muted-foreground);
  }
  .activity-icons {
    display: inline-flex;
    align-items: center;
    min-width: 0;
  }
  .activity-icon {
    width: 24px;
    height: 24px;
    border: 1px solid color-mix(in oklab, var(--accent) 22%, var(--border));
    border-radius: 7px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in oklab, var(--accent) 10%, var(--background));
    color: var(--muted-foreground);
  }
  .activity-icon + .activity-icon {
    margin-left: -5px;
  }
  .activity-copy {
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .activity-copy strong {
    flex: 0 0 auto;
    font-size: 12px;
    font-weight: 650;
  }
  .activity-copy em {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted-foreground);
    font-style: normal;
    font-size: 12px;
  }
  .activity-count,
  .activity-failed {
    flex: 0 0 auto;
    border-radius: 999px;
    padding: 2px 7px;
    font-size: 11px;
    line-height: 16px;
    font-family: var(--font-mono);
  }
  .activity-count {
    background: var(--background);
    color: var(--muted-foreground);
    border: 1px solid var(--border);
  }
  .activity-failed {
    background: color-mix(in oklab, var(--destructive, #dc2626) 10%, var(--background));
    color: color-mix(in oklab, var(--destructive, #dc2626) 80%, var(--foreground));
    border: 1px solid color-mix(in oklab, var(--destructive, #dc2626) 20%, var(--border));
  }
  .activity-details {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 260px), 1fr));
    gap: 8px;
    padding: 8px;
    border-top: 1px solid var(--border);
    background: color-mix(in oklab, var(--background) 94%, var(--muted));
  }
  .activity-action {
    min-width: 0;
    min-height: 38px;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 38px;
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: color-mix(in oklab, var(--muted) 50%, var(--background));
    box-shadow: var(--shadow-xs);
    color: var(--foreground);
    cursor: pointer;
    font: inherit;
    font-family: var(--font-mono);
    font-size: 12.5px;
    text-align: left;
  }
  .activity-action:hover,
  .activity-action.selected {
    border-color: color-mix(in oklab, var(--puffer-accent) 26%, var(--border));
    background: color-mix(in oklab, var(--puffer-accent) 8%, var(--background));
  }
  .activity-action-icon {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    background: color-mix(in oklab, var(--puffer-accent) 14%, var(--background));
    color: var(--puffer-accent);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .activity-action-name {
    flex: 0 0 auto;
    font-weight: 600;
  }
  .activity-action-arg {
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted-foreground);
  }
  .activity-action-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    color: var(--muted-foreground);
    font-size: 11px;
    flex: 0 0 auto;
  }
  .activity-action-status .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: oklch(0.65 0.18 145);
  }
  .activity-action-status[data-state="failed"] .dot {
    background: oklch(0.62 0.22 25);
  }
  .activity-action-status[data-state="running"] .dot {
    background: var(--puffer-accent);
  }
  .activity-action-chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    color: var(--muted-foreground);
    flex-shrink: 0;
  }
  .activity-panel {
    grid-column: 1 / -1;
    min-width: 0;
  }
  .activity-panel :global(.pf-tool) {
    width: 100%;
  }
  .activity-panel :global(.pf-tool > .pf-tool-head) {
    display: none;
  }
  .activity-panel :global(.pf-tool-body) {
    max-height: 360px;
  }
  .typing {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 6px;
    font-size: 13px;
    color: var(--muted-foreground);
    font-family: var(--font-mono);
  }
  .intermediate-card {
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: var(--background);
    margin-top: 8px;
  }
  .intermediate-head {
    width: 100%;
    min-height: 34px;
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr);
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: 0;
    background: color-mix(in oklab, var(--muted) 45%, var(--background));
    color: var(--foreground);
    cursor: pointer;
    font: inherit;
    text-align: left;
  }
  .intermediate-head span {
    display: inline-flex;
    color: var(--muted-foreground);
  }
  .intermediate-head strong {
    font-size: 12px;
    font-weight: 600;
  }
  .intermediate-head em {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted-foreground);
    font-style: normal;
    font-size: 12px;
  }
  .intermediate-body {
    padding: 10px 12px;
    border-top: 1px solid var(--border);
  }
  .state {
    text-align: center;
    color: var(--muted-foreground);
    padding: 40px 0;
    font-size: 14px;
  }

  @media (max-width: 720px) {
    .pf-chat-thread-inner { padding: 0 16px; }
    .pf-composer-wrap { padding: 12px 16px 16px; }
    .activity-head {
      grid-template-columns: auto auto minmax(0, 1fr) auto;
    }
    .activity-copy {
      display: grid;
      gap: 1px;
    }
    .activity-failed {
      display: none;
    }
  }
</style>
