<script lang="ts">
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
    <div class="pf-pw-project-title">
      <span class="name">
        {project.name}
        {#if project.remoteHost}
          <span class="remote-chip">remote</span>
        {/if}
      </span>
      {#if project.branch}
        <span class="branch"><Icon name="branch" size={10} />{project.branch}</span>
      {/if}
    </div>
    <div class="pf-pw-project-counts">
      <span class="count">{agents.length} agents</span>
      <span class="sep">·</span>
      <span class="count running">{running} running</span>
      <span class="sep">·</span>
      <span class="count review">{review} review</span>
    </div>
    <button
      type="button"
      class="sc-btn"
      data-variant="ghost"
      data-size="sm"
      onclick={() => onOpenBoard?.(project.id)}
      title="Open project details"
    >Details</button>
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
        <button type="button" class="sc-btn" data-variant="outline" data-size="sm" onclick={onNewAgent} disabled={!onNewAgent}>
          <Icon name="plus" size={11} />Start one
        </button>
      </div>
    {/if}
  </div>
</div>
