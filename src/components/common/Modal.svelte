<script lang="ts">
  // 通用模态框（遮罩 + 居中对话框）。
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  interface Props {
    open: boolean;
    titleKey: string;
    onclose?: () => void;
    children?: Snippet;
    footer?: Snippet;
  }
  const { open, titleKey, onclose, children, footer }: Props = $props();
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="overlay"
    role="presentation"
    onclick={(event: MouseEvent): void => {
      // 仅点击遮罩本身（而非对话框内部）时关闭。
      if (event.target === event.currentTarget) {
        onclose?.();
      }
    }}
  >
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label={ui.t(titleKey)}
      tabindex="-1"
    >
      <header>
        <h2>{ui.t(titleKey)}</h2>
        <button class="icon-btn" type="button" aria-label={ui.t('common.close')} onclick={() => onclose?.()}>
          <Icon name="close" />
        </button>
      </header>
      <div class="body">{@render children?.()}</div>
      {#if footer}
        <footer>{@render footer()}</footer>
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 100;
  }
  .dialog {
    background: var(--pms-surface);
    color: var(--pms-fg);
    border: 1px solid var(--pms-border);
    border-radius: 12px;
    width: min(560px, 100%);
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.25);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 1rem;
    border-bottom: 1px solid var(--pms-border);
  }
  h2 {
    font-size: 1rem;
    margin: 0;
  }
  .body {
    padding: 1rem;
    overflow: auto;
  }
  footer {
    padding: 0.75rem 1rem;
    border-top: 1px solid var(--pms-border);
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
  .icon-btn {
    background: transparent;
    border: none;
    color: var(--pms-muted);
    cursor: pointer;
    padding: 0.25rem;
    line-height: 0;
  }
  .icon-btn:hover {
    color: var(--pms-fg);
  }
</style>
