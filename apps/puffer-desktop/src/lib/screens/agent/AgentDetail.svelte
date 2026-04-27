<script lang="ts">
  import Puffer from "../../design/Puffer.svelte";
  import Icon, { type IconName } from "../../design/Icon.svelte";
  import AgentDetailContent from "./AgentDetailContent.svelte";
  import ModelPicker from "./ModelPicker.svelte";
  import {
    AGENT_STATE_LABELS,
    agentPufferState,
    type AgentStatus
  } from "../../data/mockProjects";
  import { sessionDisplayName, sessionDisplayTitle } from "../../sessionDisplay";
  import type {
    PermissionTimelineItem,
    SessionDetail,
    SessionListItem,
    SettingsSnapshot,
    TimelineItem,
    UserQuestionTimelineItem
  } from "../../types";
  import type { AgentState } from "../../shell/tweaks";

  type Props = {
    // Live session data from the backend.
    session: SessionListItem | null;
    sessionDetail: SessionDetail | null;
    timeline: TimelineItem[];
    pendingPermissions: PermissionTimelineItem[];
    pendingQuestions: UserQuestionTimelineItem[];
    loading: boolean;
    turnRunning?: boolean;
    turnStartedAtMs?: number | null;
    turnThinking?: boolean;
    turnStatusHint?: string | null;
    settingsSnapshot?: SettingsSnapshot | null;
    onBack: () => void;
    onSubmitMessage: (message: string) => void;
    onResolvePermission: (permissionId: string, choice: string) => void;
    onResolveUserQuestion: (
      questionId: string,
      answers: Record<string, string | string[]>,
      annotations?: Record<string, Record<string, string>>
    ) => void;
    onCancelTurn?: () => void;
    onRenameTitle?: (title: string) => void | Promise<void>;
    onModelChange?: (providerId: string, modelId: string) => void;
  };

  let {
    session,
    sessionDetail,
    timeline,
    pendingPermissions,
    pendingQuestions,
    loading,
    turnRunning = false,
    turnStartedAtMs = null,
    turnThinking = false,
    turnStatusHint = null,
    settingsSnapshot = null,
    onBack,
    onSubmitMessage,
    onResolvePermission,
    onResolveUserQuestion,
    onCancelTurn,
    onRenameTitle,
    onModelChange
  }: Props = $props();

  type Tab = "chat" | "diff" | "terminal" | "files" | "browser" | "history";
  let tab = $state<Tab>("chat");
  let sideTab = $state<Tab | null>(null);
  let sideWidth = $state(420);
  let sideDragStart: { pointerId: number; startX: number; startWidth: number } | null = null;
  let previousActionCount = $state(0);

  // Header identity comes straight from the live session record. No
  // local board persona — the daemon is the source of truth.
  let displayName = $derived(sessionDisplayName(session));
  let displayTitle = $derived(sessionDisplayTitle(session));
  let displayBranch = $derived(sessionDetail?.repoStatus?.branch ?? "");
  let displayProject = $derived(session?.folderPath?.split("/").pop() ?? "");
  let projectCwd = $derived(sessionDetail?.repoStatus?.cwd ?? session?.cwd ?? "");
  let displayWorktree = $derived("");
  let status = $derived<AgentStatus>(inferStatusFromSession(sessionDetail));
  let editingTitle = $state(false);
  let titleDraft = $state("");
  let titleSaving = $state(false);

  $effect(() => {
    if (!editingTitle) titleDraft = displayName;
  });

  function inferStatusFromSession(d: SessionDetail | null): AgentStatus {
    if (!d) return "idle";
    const hasPending = d.timeline.some((t) => t.kind === "permission");
    if (hasPending) return "awaiting";
    if (d.repoStatus?.pullRequest) return "review";
    if (d.repoStatus?.hasUncommittedChanges) return "running";
    return "idle";
  }

  let pufferState = $derived<AgentState>(
    pendingPermissions.length > 0 || pendingQuestions.length > 0
      ? "awaiting"
      : turnRunning
        ? turnThinking
          ? "thinking"
          : "running"
        : agentPufferState(status)
  );
  let statusLabel = $derived(
    turnRunning
      ? turnThinking
        ? "thinking"
        : "running"
      : AGENT_STATE_LABELS[status] ?? status
  );
  let diffCount = $derived(timeline.filter((t) => t.kind === "diff").length);
  let actionCount = $derived(
    timeline.filter((item) => {
      if (item.kind !== "tool") return false;
      const tool = item.toolName.toLowerCase();
      return [
        "write",
        "write_file",
        "edit",
        "edit_file",
        "replace",
        "replace_in_file",
        "multiedit",
        "multi_edit",
        "notebookedit",
        "bash",
        "shell",
        "powershell",
        "browser"
      ].includes(tool);
    }).length
  );

  $effect(() => {
    if (actionCount > previousActionCount && sideTab === null) sideTab = "history";
    previousActionCount = actionCount;
  });

  function startTitleEdit() {
    if (!session || !onRenameTitle) return;
    titleDraft = displayName;
    editingTitle = true;
  }

  function cancelTitleEdit() {
    titleDraft = displayName;
    editingTitle = false;
  }

  async function saveTitleEdit() {
    if (!session || !onRenameTitle || titleSaving) return;
    titleSaving = true;
    try {
      await onRenameTitle(titleDraft);
      editingTitle = false;
    } finally {
      titleSaving = false;
    }
  }

  function handleTitleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      void saveTitleEdit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelTitleEdit();
    }
  }

  function beginSideResize(event: PointerEvent) {
    sideDragStart = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: sideWidth
    };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  function moveSideResize(event: PointerEvent) {
    if (!sideDragStart || event.pointerId !== sideDragStart.pointerId) return;
    const next = sideDragStart.startWidth + sideDragStart.startX - event.clientX;
    sideWidth = Math.max(300, Math.min(760, Math.round(next)));
  }

  function endSideResize(event: PointerEvent) {
    if (!sideDragStart || event.pointerId !== sideDragStart.pointerId) return;
    try {
      (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    } catch {
      /* ignore */
    }
    sideDragStart = null;
  }

  function handleTabClick(event: MouseEvent, nextTab: Tab) {
    if (event.metaKey || event.ctrlKey) {
      sideTab = nextTab;
      event.preventDefault();
      return;
    }
    tab = nextTab;
  }

  function tabLabel(value: Tab): string {
    switch (value) {
      case "chat":
        return "Chat";
      case "diff":
        return "Diff";
      case "terminal":
        return "Terminal";
      case "files":
        return "Files";
      case "browser":
        return "Browser";
      case "history":
        return "History";
    }
  }

  function tabIcon(value: Tab): IconName {
    switch (value) {
      case "chat":
        return "sparkles";
      case "diff":
        return "git";
      case "terminal":
        return "terminal";
      case "files":
        return "folder";
      case "browser":
        return "globe";
      case "history":
        return "layers";
    }
  }
</script>

<div class="pf-agent-detail">
  <div class="pf-agent-detail-head">
    <button type="button" class="pf-agent-back" onclick={onBack} title="Back to workspace" aria-label="Back">
      <Icon name="chevL" size={13} />
    </button>
    <Puffer size={20} state={pufferState} />
    <div class="pf-agent-identity">
      <div class="name" class:editing={editingTitle}>
        {#if editingTitle}
          <input
            class="title-input"
            bind:value={titleDraft}
            onkeydown={handleTitleKeydown}
            disabled={titleSaving}
            aria-label="Session title"
          />
          <button
            type="button"
            class="title-icon-btn"
            onclick={() => void saveTitleEdit()}
            disabled={titleSaving}
            title="Save title"
            aria-label="Save title"
          >
            <Icon name="check" size={12} />
          </button>
          <button
            type="button"
            class="title-icon-btn"
            onclick={cancelTitleEdit}
            disabled={titleSaving}
            title="Cancel"
            aria-label="Cancel title edit"
          >
            <Icon name="x" size={12} />
          </button>
        {:else}
          <span class="primary-title">{displayName}</span>
          {#if displayTitle}
            <span class="sep">·</span>
            <span class="title">{displayTitle}</span>
          {/if}
          {#if onRenameTitle}
            <button
              type="button"
              class="title-icon-btn"
              onclick={startTitleEdit}
              title="Edit title"
              aria-label="Edit session title"
            >
              <Icon name="edit" size={12} />
            </button>
          {/if}
        {/if}
      </div>
      <div class="meta">
        {#if displayProject}
          <span class="mono">{displayProject}</span>
          <span class="sep">·</span>
        {/if}
        {#if displayBranch}
          <span class="branch mono"><Icon name="branch" size={10} />{displayBranch}</span>
          {#if displayWorktree}
            <span class="sep">·</span>
          {/if}
        {/if}
        {#if displayWorktree}
          <span class="mono">{displayWorktree}</span>
        {/if}
      </div>
    </div>
    {#if onModelChange}
      <ModelPicker
        snapshot={settingsSnapshot}
        onChange={(providerId, modelId) => onModelChange?.(providerId, modelId)}
      />
    {/if}
    <span class="pf-agent-status-pill" data-status={status}>
      {#if pufferState === "running"}
        <span class="pip"></span>
      {/if}
      {statusLabel}
    </span>
    <div class="pf-agent-tabs">
      <button class="pf-agent-tab" class:on={tab === "chat"} onclick={(event) => handleTabClick(event, "chat")}>
        <Icon name="sparkles" size={12} />Chat
      </button>
      <button class="pf-agent-tab" class:on={tab === "diff"} onclick={(event) => handleTabClick(event, "diff")}>
        <Icon name="git" size={12} />Diff
        {#if diffCount > 0}
          <span class="pf-agent-tab-badge">{diffCount}</span>
        {/if}
      </button>
      <button
        class="pf-agent-tab"
        class:on={tab === "terminal"}
        onclick={(event) => handleTabClick(event, "terminal")}
      >
        <Icon name="terminal" size={12} />Terminal
      </button>
      <button class="pf-agent-tab" class:on={tab === "files"} onclick={(event) => handleTabClick(event, "files")}>
        <Icon name="folder" size={12} />Files
      </button>
      <button
        class="pf-agent-tab"
        class:on={tab === "browser"}
        onclick={(event) => handleTabClick(event, "browser")}
      >
        <Icon name="globe" size={12} />Browser
      </button>
      <button
        class="pf-agent-tab"
        class:on={tab === "history"}
        onclick={(event) => handleTabClick(event, "history")}
      >
        <Icon name="layers" size={12} />History
        {#if actionCount > 0}
          <span class="pf-agent-tab-badge">{actionCount}</span>
        {/if}
      </button>
    </div>
  </div>

  <div class="pf-agent-detail-shell" class:withSubpage={sideTab !== null}>
    <div class="pf-agent-detail-body">
      <AgentDetailContent
        {tab}
        {session}
        {sessionDetail}
        {timeline}
        {pendingPermissions}
        {pendingQuestions}
        {loading}
        {displayName}
        {pufferState}
        {projectCwd}
        {turnRunning}
        {turnStartedAtMs}
        {turnThinking}
        {turnStatusHint}
        {onSubmitMessage}
        {onResolvePermission}
        {onResolveUserQuestion}
        {onCancelTurn}
      />
    </div>
    {#if sideTab}
      <div class="pf-side-panel" style:width={`${sideWidth}px`}>
        <button
          class="pf-side-resize"
          type="button"
          aria-label="Resize side page"
          onpointerdown={beginSideResize}
          onpointermove={moveSideResize}
          onpointerup={endSideResize}
          onpointercancel={endSideResize}
        ></button>
        <div class="pf-side-head">
          <span><Icon name={tabIcon(sideTab)} size={12} />{tabLabel(sideTab)}</span>
          <button
            type="button"
            class="pf-side-close"
            aria-label="Close side page"
            onclick={() => (sideTab = null)}
          >
            <Icon name="x" size={12} />
          </button>
        </div>
        <AgentDetailContent
          tab={sideTab}
          {session}
          {sessionDetail}
          {timeline}
          {pendingPermissions}
          {pendingQuestions}
          {loading}
          {displayName}
          {pufferState}
          {projectCwd}
          {turnRunning}
          {turnStartedAtMs}
          {turnThinking}
          {turnStatusHint}
          {onSubmitMessage}
          {onResolvePermission}
          {onResolveUserQuestion}
          {onCancelTurn}
        />
      </div>
    {/if}
  </div>
</div>

<style>
  .pf-agent-detail {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--background);
  }
  .pf-agent-detail-head {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: color-mix(in oklab, var(--background) 96%, var(--muted));
    border-bottom: 1px solid var(--border);
    min-height: 52px;
  }
  .pf-agent-back {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--background);
    color: var(--muted-foreground);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 120ms, color 120ms;
  }
  .pf-agent-back:hover { background: var(--accent); color: var(--foreground); }
  .pf-agent-identity {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 0 1 auto;
    max-width: 420px;
  }
  .pf-agent-identity .name {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .pf-agent-identity .name.editing {
    align-items: center;
  }
  .pf-agent-identity .name .primary-title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pf-agent-identity .name .sep { color: var(--muted-foreground); opacity: 0.5; }
  .pf-agent-identity .name .title {
    font-weight: 500;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title-input {
    width: min(320px, 34vw);
    height: 26px;
    min-width: 140px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--background);
    color: var(--foreground);
    font: inherit;
    padding: 0 8px;
    outline: none;
  }
  .title-input:focus {
    border-color: color-mix(in oklab, var(--accent-foreground) 35%, var(--border));
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--accent) 70%, transparent);
  }
  .title-icon-btn {
    width: 24px;
    height: 24px;
    border-radius: 5px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--muted-foreground);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex-shrink: 0;
  }
  .title-icon-btn:hover:not(:disabled) {
    color: var(--foreground);
    background: var(--accent);
    border-color: var(--border);
  }
  .title-icon-btn:disabled {
    cursor: wait;
    opacity: 0.55;
  }
  .pf-agent-identity .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted-foreground);
  }
  .pf-agent-identity .meta .mono { font-family: var(--font-mono); }
  .pf-agent-identity .meta .sep { opacity: 0.4; }
  .pf-agent-identity .meta .branch {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--muted);
  }

  .pf-agent-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 10.5px;
    font-weight: 600;
    font-family: var(--font-mono);
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--muted);
    color: var(--muted-foreground);
    text-transform: lowercase;
    flex-shrink: 0;
    margin-left: auto;
  }
  .pf-agent-status-pill[data-status="running"]  { background: color-mix(in oklab, oklch(0.7 0.17 70) 15%, var(--background)); color: oklch(0.55 0.17 70); }
  .pf-agent-status-pill[data-status="awaiting"] { background: color-mix(in oklab, oklch(0.72 0.18 30) 16%, var(--background)); color: oklch(0.55 0.2 30); }
  .pf-agent-status-pill[data-status="review"]   { background: color-mix(in oklab, oklch(0.7 0.16 40) 15%, var(--background));  color: oklch(0.55 0.17 40); }
  .pf-agent-status-pill .pip {
    width: 6px; height: 6px; border-radius: 50%;
    background: oklch(0.7 0.17 70);
    animation: pf-pulse-dot 1.6s infinite;
  }
  @keyframes pf-pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }

  .pf-agent-tabs {
    display: flex;
    gap: 1px;
    background: var(--muted);
    padding: 3px;
    border-radius: 8px;
    flex-shrink: 0;
  }
  .pf-agent-tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted-foreground);
    border: 0;
    background: transparent;
    border-radius: 5px;
    cursor: pointer;
    transition: background 120ms, color 120ms;
    font: inherit;
  }
  .pf-agent-tab:hover { color: var(--foreground); }
  .pf-agent-tab.on {
    background: var(--background);
    color: var(--foreground);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.06);
  }
  .pf-agent-tab-badge {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 5px;
    border-radius: 3px;
    background: oklch(0.7 0.16 40);
    color: white;
    margin-left: 2px;
  }

  .pf-agent-detail-shell {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
  }

  .pf-agent-detail-body {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .pf-side-panel {
    flex: 0 0 auto;
    min-width: 300px;
    max-width: 760px;
    min-height: 0;
    position: relative;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border);
    box-shadow: -8px 0 20px rgb(0 0 0 / 0.04);
    background: var(--background);
  }

  .pf-side-resize {
    position: absolute;
    z-index: 5;
    top: 0;
    bottom: 0;
    left: -4px;
    width: 8px;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: col-resize;
    touch-action: none;
  }

  .pf-side-resize::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 3px;
    width: 2px;
    background: transparent;
  }

  .pf-side-resize:hover::before {
    background: color-mix(in oklab, var(--accent-foreground) 35%, var(--border));
  }

  .pf-side-head {
    height: 36px;
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 0 10px 0 12px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--background) 96%, var(--muted));
    color: var(--foreground);
    font-size: 12px;
    font-weight: 600;
  }

  .pf-side-head span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .pf-side-close {
    width: 24px;
    height: 24px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }

  .pf-side-close:hover {
    color: var(--foreground);
    background: var(--accent);
    border-color: var(--border);
  }

  .pf-side-panel :global(.pf-agent-detail-content) {
    flex: 1;
    min-height: 0;
  }

  @media (max-width: 720px) {
    .pf-agent-detail-head { flex-wrap: wrap; row-gap: 6px; padding: 8px 10px; }
    .pf-agent-tabs { order: 3; width: 100%; overflow-x: auto; }
    .pf-agent-status-pill { order: 2; margin-left: 0; }
  }
</style>
