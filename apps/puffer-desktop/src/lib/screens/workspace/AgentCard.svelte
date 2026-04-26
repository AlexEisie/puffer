<script lang="ts">
  import Puffer from "../../design/Puffer.svelte";
  import Icon from "../../design/Icon.svelte";
  import { AGENT_STATE_LABELS, agentPufferState, type MockAgent } from "../../data/mockProjects";

  type Props = { a: MockAgent; onOpen?: () => void };
  let { a, onOpen }: Props = $props();
</script>

<button class="pf-pw-agent" data-status={a.status} onclick={onOpen}>
  <div class="head">
    <Puffer size={22} state={agentPufferState(a.status)} />
    <div class="identity">
      <span class="name">{a.name}</span>
      <span class="model">{a.model}</span>
    </div>
    <span class="status-pill" data-status={a.status}>{AGENT_STATE_LABELS[a.status] ?? a.status}</span>
  </div>
  {#if a.title}
    <div class="title">{a.title}</div>
  {/if}
  {#if a.step}
    <div class="step">{a.step}</div>
  {/if}
  <div class="meta">
    {#if a.branch}
      <span><Icon name="branch" size={10} />{a.branch}</span>
    {/if}
    <span><Icon name="clock" size={10} />{a.elapsed}</span>
  </div>
</button>
