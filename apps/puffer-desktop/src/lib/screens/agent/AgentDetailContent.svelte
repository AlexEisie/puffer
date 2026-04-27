<script lang="ts">
  import Icon from "../../design/Icon.svelte";
  import HighlightedLine from "../../components/HighlightedLine.svelte";
  import ActionHistoryPane from "./ActionHistoryPane.svelte";
  import ConversationView from "./ConversationView.svelte";
  import DiffView from "../../components/DiffView.svelte";
  import BrowserPane from "./BrowserPane.svelte";
  import FilesPane from "./FilesPane.svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import type {
    PermissionTimelineItem,
    SessionDetail,
    SessionListItem,
    TimelineItem,
    UserQuestionTimelineItem
  } from "../../types";
  import type { AgentState } from "../../shell/tweaks";

  type Tab = "chat" | "diff" | "terminal" | "files" | "browser" | "history";
  type DiffSubTab = "agent" | "git" | "divergence";

  type Props = {
    tab: Tab;
    session: SessionListItem | null;
    sessionDetail: SessionDetail | null;
    timeline: TimelineItem[];
    pendingPermissions: PermissionTimelineItem[];
    pendingQuestions: UserQuestionTimelineItem[];
    loading: boolean;
    displayName: string;
    pufferState: AgentState;
    projectCwd: string;
    turnRunning: boolean;
    turnStartedAtMs: number | null;
    turnThinking: boolean;
    turnStatusHint: string | null;
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
    tab,
    session,
    sessionDetail,
    timeline,
    pendingPermissions,
    pendingQuestions,
    loading,
    displayName,
    pufferState,
    projectCwd,
    turnRunning,
    turnStartedAtMs,
    turnThinking,
    turnStatusHint,
    onSubmitMessage,
    onResolvePermission,
    onResolveUserQuestion,
    onCancelTurn
  }: Props = $props();

  let diffTab = $state<DiffSubTab>("agent");
  let agentDiff = $derived(sessionDetail?.agentDiff ?? { files: [], entries: [] });
  let divergence = $derived(
    sessionDetail?.divergence ?? { agentOnly: [], gitOnly: [], agentTotal: 0, gitTotal: 0 }
  );
  let divergenceCount = $derived(divergence.agentOnly.length + divergence.gitOnly.length);

  function kindIcon(kind: string): "edit" | "file" | "x" | "branch" {
    switch (kind) {
      case "write":
        return "file";
      case "remove":
        return "x";
      case "move":
        return "branch";
      default:
        return "edit";
    }
  }
</script>

<div class="pf-agent-detail-content">
  {#if tab === "chat"}
    <ConversationView
      session={session}
      agentName={displayName}
      agentState={pufferState}
      timeline={timeline}
      pendingPermissions={pendingPermissions}
      pendingQuestions={pendingQuestions}
      loading={loading}
      turnRunning={turnRunning}
      turnStartedAtMs={turnStartedAtMs}
      turnThinking={turnThinking}
      turnStatusHint={turnStatusHint}
      onSubmitMessage={onSubmitMessage}
      onResolvePermission={onResolvePermission}
      onResolveUserQuestion={onResolveUserQuestion}
      onCancelTurn={onCancelTurn}
    />
  {:else if tab === "history"}
    <ActionHistoryPane timeline={timeline} sessionId={session?.id ?? null} />
  {:else if tab === "diff"}
    <div class="diff-subtabs">
      <button class="diff-subtab" class:on={diffTab === "agent"} onclick={() => (diffTab = "agent")}>
        <Icon name="sparkles" size={11} />Agent
        {#if agentDiff.files.length > 0}
          <span class="pf-agent-tab-badge">{agentDiff.files.length}</span>
        {/if}
      </button>
      <button class="diff-subtab" class:on={diffTab === "git"} onclick={() => (diffTab = "git")}>
        <Icon name="git" size={11} />Git
        {#if divergence.gitTotal > 0}
          <span class="pf-agent-tab-badge">{divergence.gitTotal}</span>
        {/if}
      </button>
      <button
        class="diff-subtab"
        class:on={diffTab === "divergence"}
        onclick={() => (diffTab = "divergence")}
        title={divergenceCount > 0 ? "Agent and git disagree on which files changed" : "Agent and git agree"}
      >
        <Icon name="bolt" size={11} />Divergence
        {#if divergenceCount > 0}
          <span class="pf-agent-tab-badge warn">{divergenceCount}</span>
        {/if}
      </button>
    </div>

    {#if diffTab === "agent"}
      {#if agentDiff.files.length > 0}
        <div class="diff-wrap">
          <div class="agent-diff-list">
            {#each agentDiff.files as file (file.path)}
              <article class="agent-diff-card">
                <header>
                  <Icon name={kindIcon(file.latestKind)} size={12} color="var(--muted-foreground)" />
                  <span class="path mono" title={file.path}>{file.path}</span>
                  <span class="kind">{file.latestKind}</span>
                  {#if file.editCount > 1}
                    <span class="count">x{file.editCount}</span>
                  {/if}
                </header>
                <pre class="diff-snippet"><code>{#each file.latestSummary.split("\n") as line, i (i)}<span><HighlightedLine text={line || " "} path={file.path} /></span>{/each}</code></pre>
              </article>
            {/each}
          </div>
        </div>
      {:else}
        <div class="pane-empty">
          <Icon name="sparkles" size={20} color="var(--muted-foreground)" />
          <div class="title">No agent edits yet</div>
          <div class="sub">
            Once the agent writes or replaces a file, the per-edit summary lands here,
            independent of git.
          </div>
        </div>
      {/if}
    {:else if diffTab === "git"}
      {#if sessionDetail?.latestDiff}
        <div class="diff-wrap">
          <DiffView diff={sessionDetail.latestDiff} />
        </div>
      {:else}
        <div class="pane-empty">
          <Icon name="git" size={20} color="var(--muted-foreground)" />
          <div class="title">No git changes</div>
          <div class="sub">The session has no working-tree changes against HEAD.</div>
        </div>
      {/if}
    {:else}
      <div class="diff-wrap divergence-pane">
        {#if divergenceCount === 0}
          <div class="pane-empty">
            <Icon name="check" size={20} color="var(--muted-foreground)" />
            <div class="title">Agent and git agree</div>
            <div class="sub">
              Every file the agent edited shows up in git diff, and nothing else has changed on disk.
              {divergence.agentTotal} agent - {divergence.gitTotal} git.
            </div>
          </div>
        {:else}
          {#if divergence.agentOnly.length > 0}
            <section class="diverge-block">
              <header><Icon name="sparkles" size={12} />Agent edited, not in git ({divergence.agentOnly.length})</header>
              <p class="hint">
                The agent claims to have edited these files but they do not appear in the current git diff.
              </p>
              <ul>
                {#each divergence.agentOnly as path (path)}
                  <li class="mono">{path}</li>
                {/each}
              </ul>
            </section>
          {/if}
          {#if divergence.gitOnly.length > 0}
            <section class="diverge-block">
              <header><Icon name="git" size={12} />Changed on disk, no agent edit ({divergence.gitOnly.length})</header>
              <p class="hint">
                Git sees these files as modified but no agent tool call touched them.
              </p>
              <ul>
                {#each divergence.gitOnly as path (path)}
                  <li class="mono">{path}</li>
                {/each}
              </ul>
            </section>
          {/if}
        {/if}
      </div>
    {/if}
  {:else if tab === "terminal"}
    <TerminalPane cwd={projectCwd} sessionId={session?.id ?? "preview"} />
  {:else if tab === "files"}
    <FilesPane cwd={projectCwd} sessionId={session?.id ?? "preview"} />
  {:else if tab === "browser"}
    <BrowserPane sessionId={session?.id ?? "preview"} />
  {/if}
</div>

<style>
  .pf-agent-detail-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .diff-wrap {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .pane-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 40px;
    color: var(--muted-foreground);
    text-align: center;
  }

  .pane-empty .title { font-size: 14px; font-weight: 600; color: var(--foreground); }
  .pane-empty .sub { font-size: 12.5px; max-width: 360px; line-height: 1.55; }

  .diff-subtabs {
    display: flex;
    gap: 4px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--background) 96%, var(--muted));
  }

  .diff-subtab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border: 1px solid transparent;
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: color 100ms, border-color 100ms, background 100ms;
  }

  .diff-subtab:hover { color: var(--foreground); }
  .diff-subtab.on {
    color: var(--foreground);
    background: var(--background);
    border-color: var(--border);
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

  .pf-agent-tab-badge.warn {
    background: color-mix(in oklab, oklch(0.62 0.22 25) 18%, var(--background));
    color: oklch(0.55 0.2 30);
    border: 1px solid color-mix(in oklab, oklch(0.62 0.22 25) 35%, var(--border));
  }

  .agent-diff-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
  }

  .agent-diff-card {
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--background);
  }

  .agent-diff-card header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: color-mix(in oklab, var(--background) 96%, var(--muted));
    border-bottom: 1px solid var(--border);
    font-size: 12px;
  }

  .agent-diff-card .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--foreground);
  }

  .agent-diff-card .kind,
  .agent-diff-card .count {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--muted);
    color: var(--muted-foreground);
  }

  .diff-snippet {
    margin: 0;
    padding: 10px 12px;
    max-height: 260px;
    overflow: auto;
    background: var(--background);
    font-family: var(--font-mono);
    font-size: 11.5px;
    line-height: 1.5;
  }

  .diff-snippet code {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .divergence-pane {
    padding: 14px 16px;
  }

  .diverge-block {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px;
    margin-bottom: 12px;
    background: var(--background);
  }

  .diverge-block header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    font-size: 13px;
  }

  .diverge-block .hint {
    margin: 8px 0;
    color: var(--muted-foreground);
    font-size: 12px;
    line-height: 1.45;
  }

  .diverge-block ul {
    margin: 0;
    padding-left: 18px;
  }

  .mono {
    font-family: var(--font-mono);
  }
</style>
