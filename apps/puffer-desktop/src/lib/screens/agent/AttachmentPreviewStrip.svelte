<script lang="ts" module>
  import type { MessageAttachment } from "../../types";

  export type AttachmentPreviewVariant = "composer" | "message";
  export type AttachmentPreviewItem = MessageAttachment;
</script>

<script lang="ts">
  import Icon from "../../design/Icon.svelte";

  type Props = {
    attachments: AttachmentPreviewItem[];
    variant?: AttachmentPreviewVariant;
    removable?: boolean;
    testId?: string;
    onRemove?: (id: string) => void;
  };

  let {
    attachments,
    variant = "composer",
    removable = false,
    testId = undefined,
    onRemove
  }: Props = $props();
</script>

{#if attachments.length > 0}
  <div
    class="pf-attachment-preview-strip"
    data-variant={variant}
    data-testid={testId}
  >
    {#each attachments as attachment (attachment.id)}
      <div class="pf-attachment-preview">
        {#if attachment.previewUrl && attachment.kind === "image"}
          <div class="pf-attachment-thumb">
            <img src={attachment.previewUrl} alt={attachment.name} draggable="false" />
          </div>
        {:else}
          <div class="pf-attachment-file-card" data-kind={attachment.kind}>
            <span class="pf-attachment-file-icon">
              <Icon name="file" size={18} />
            </span>
            <span class="pf-attachment-file-copy">
              <span class="pf-attachment-file-name">{attachment.name}</span>
              <span class="pf-attachment-file-ext">{attachment.extension}</span>
            </span>
          </div>
        {/if}
        {#if removable}
          <button
            type="button"
            class="pf-attachment-remove"
            aria-label={`Remove attachment ${attachment.name}`}
            title="Remove attachment"
            onclick={() => onRemove?.(attachment.id)}
          >
            <Icon name="x" size={13} />
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .pf-attachment-preview-strip {
    display: flex;
    gap: 8px;
    max-width: 100%;
    overflow-x: auto;
    padding: 2px 24px 8px 2px;
  }
  .pf-attachment-preview-strip[data-variant="message"] {
    padding: 2px 2px 8px;
  }
  .pf-attachment-preview {
    position: relative;
    flex: 0 0 auto;
  }
  .pf-attachment-thumb {
    width: 64px;
    height: 64px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--muted);
  }
  .pf-attachment-thumb img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
  }
  .pf-attachment-file-card {
    width: 224px;
    height: 64px;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in oklab, var(--muted) 34%, var(--background));
    color: var(--foreground);
  }
  .pf-attachment-file-icon {
    width: 34px;
    height: 34px;
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 7px;
    background: var(--background);
    color: var(--muted-foreground);
    border: 1px solid color-mix(in oklab, var(--border) 80%, transparent);
  }
  .pf-attachment-file-copy {
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .pf-attachment-file-name,
  .pf-attachment-file-ext {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pf-attachment-file-name {
    font-size: 12.5px;
    line-height: 16px;
    font-weight: 650;
  }
  .pf-attachment-file-ext {
    color: var(--muted-foreground);
    font-size: 11px;
    line-height: 14px;
    font-weight: 600;
  }
  .pf-attachment-remove {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 22px;
    height: 22px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--background);
    color: var(--muted-foreground);
    box-shadow: var(--shadow-sm);
    cursor: pointer;
  }
  .pf-attachment-remove:hover {
    color: var(--foreground);
    background: var(--accent);
  }
</style>
