<script lang="ts">
  import BrandLogo from "../design/BrandLogo.svelte";
  import Puffer from "../design/Puffer.svelte";
  import Icon from "../design/Icon.svelte";
  import type { AgentState } from "./tweaks.ts";

  export type TitleTab = { id: string; title: string; state: AgentState };

  type Props = {
    tabs: TitleTab[];
    activeTab: string;
    onSelectTab: (id: string) => void;
    onNewTab?: () => void;
    onSearch?: () => void;
    onOpenSettings?: () => void;
  };

  let { tabs, activeTab, onSelectTab, onNewTab, onSearch, onOpenSettings }: Props = $props();
</script>

<div class="pf-titlebar" data-tauri-drag-region>
  <div class="pf-titlebar-brand" data-tauri-drag-region>
    <BrandLogo size={20} />
  </div>
  <div class="pf-titlebar-tabs">
    {#each tabs as tab (tab.id)}
      <button
        type="button"
        class="pf-tab-pill"
        data-active={activeTab === tab.id}
        onclick={() => onSelectTab(tab.id)}
      >
        <Puffer size={12} state={tab.state} />
        <span>{tab.title}</span>
      </button>
    {/each}
    {#if onNewTab}
      <button
        type="button"
        class="pf-tab-pill"
        style="padding: 0 6px;"
        onclick={onNewTab}
        aria-label="New tab"
      >
        <Icon name="plus" size={12} />
      </button>
    {/if}
  </div>
  <div style="flex: 1;" data-tauri-drag-region></div>
  {#if onSearch}
    <button
      type="button"
      class="sc-btn"
      data-variant="ghost"
      data-size="icon-sm"
      style="height: 24px; width: 24px;"
      onclick={onSearch}
      aria-label="Search"
    >
      <Icon name="search" size={13} />
    </button>
  {/if}
  {#if onOpenSettings}
    <button
      type="button"
      class="sc-btn"
      data-variant="ghost"
      data-size="icon-sm"
      style="height: 24px; width: 24px;"
      onclick={onOpenSettings}
      aria-label="Settings"
    >
      <Icon name="settings" size={13} />
    </button>
  {/if}
</div>
