<script lang="ts">
  interface Props {
    url: string;
    name: string;
    description?: string;
    onClose: () => void;
  }
  let { url, name, description, onClose }: Props = $props();

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="cip-overlay" role="dialog" aria-modal="true" aria-label="Character Preview">
  <!-- Full-bleed button behind the panel: clicking outside the panel closes. -->
  <button type="button" class="cip-backdrop" aria-label="Close preview" onclick={onClose}></button>
  <div class="cip-panel">
    <button type="button" class="cip-close" aria-label="Close preview" onclick={onClose}>✕</button>
    <img class="cip-image" src={url} alt={name} />
    <div class="cip-meta">
      <strong class="cip-name">{name}</strong>
      {#if description}<p class="cip-desc">{description}</p>{/if}
    </div>
  </div>
</div>

<style>
  .cip-overlay {
    position: fixed; inset: 0; z-index: 60;
    display: flex; align-items: center; justify-content: center;
    padding: 24px;
  }
  .cip-backdrop {
    position: absolute; inset: 0;
    border: none; padding: 0; cursor: default;
    background: rgba(0, 0, 0, 0.66);
  }
  .cip-panel {
    position: relative; z-index: 1;
    max-width: min(86vw, 720px); max-height: 88vh;
    display: flex; flex-direction: column; align-items: center; gap: 12px;
    background: var(--ic-surface, #1b1b1f); border-radius: 12px; padding: 20px;
    overflow: auto;
  }
  .cip-close {
    position: absolute; top: 8px; right: 8px;
    background: transparent; border: none; color: inherit;
    font-size: 16px; cursor: pointer; line-height: 1;
  }
  .cip-image { max-width: 100%; max-height: 64vh; object-fit: contain; border-radius: 8px; }
  .cip-meta { text-align: center; }
  .cip-name { display: block; font-size: 15px; }
  .cip-desc { margin: 6px 0 0; font-size: 13px; opacity: 0.85; }
</style>
