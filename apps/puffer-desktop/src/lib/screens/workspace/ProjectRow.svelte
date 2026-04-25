<script lang="ts">
  import Puffer from "../../design/Puffer.svelte";
  import Icon from "../../design/Icon.svelte";
  import AgentCard from "./AgentCard.svelte";
  import type { MockAgent, MockProject } from "../../data/mockProjects";

  type Props = {
    project: MockProject;
    agents: MockAgent[];
    onOpenAgent?: (id: string) => void;
    onOpenBoard?: (projectId: string) => void;
    onNewAgent?: () => void;
  };

  let { project, agents, onOpenAgent, onOpenBoard, onNewAgent }: Props = $props();

  let running = $derived(agents.filter((a) => a.status === "running").length);
  let review = $derived(agents.filter((a) => a.status === "review").length);
</script>

<div class="pf-pw-project">
  <div class="pf-pw-project-head">
    <span class="pf-pw-project-swatch" style="background: {project.color};"></span>
    <div class="pf-pw-project-title">
      <div class="name">
        {project.name}
        {#if project.remoteHost}
          <span class="remote-chip">remote</span>
        {/if}
      </div>
      <div class="meta">
        <span class="mono">{project.path}</span>
        <span class="sep">·</span>
        <span class="branch"><Icon name="branch" size={10} />{project.branch}</span>
      </div>
    </div>
    <div class="pf-pw-project-counts">
      {#if running > 0}
        <span class="count running" title="Running"><Puffer size={12} state="running" />{running}</span>
      {/if}
      {#if review > 0}
        <span class="count review" title="Review"><span class="pip review"></span>{review}</span>
      {/if}
    </div>
    <button
      type="button"
      class="sc-btn"
      data-variant="ghost"
      data-size="sm"
      onclick={() => onOpenBoard?.(project.id)}
      title="Open project details"
    ><Icon name="layers" size={12} />Details</button>
    <button type="button" class="sc-btn" data-variant="ghost" data-size="sm" aria-label="Open terminal">
      <Icon name="terminal" size={12} />
    </button>
    <button
      type="button"
      class="sc-btn"
      data-variant="default"
      data-size="sm"
      onclick={onNewAgent}
      disabled={!onNewAgent}
    >
      <Icon name="plus" size={12} />New agent
    </button>
  </div>

  <div class="pf-pw-agents-strip">
    {#each agents as a (a.id)}
      <AgentCard {a} onOpen={() => onOpenAgent?.(a.id)} />
    {/each}
    {#if agents.length === 0}
      <div class="pf-pw-agents-empty">
        <span class="icon"><Icon name="sparkles" size={14} color="var(--muted-foreground)" /></span>
        <span>No active agents.</span>
        <button type="button" class="sc-btn" data-variant="outline" data-size="sm">
          <Icon name="plus" size={11} />Start one
        </button>
      </div>
    {/if}
  </div>
</div>
