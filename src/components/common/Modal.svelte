<script lang="ts">
  // 通用模态框（遮罩 + 居中对话框）。设计规范 §5.4：遮罩 rgba(0,0,0,.55) + 轻微模糊；卡片 r-lg + shadow。
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
      class="pms-card dialog"
      role="dialog"
      aria-modal="true"
      aria-label={ui.t(titleKey)}
      tabindex="-1"
    >
      <header class="dialog-head">
        <h2>{ui.t(titleKey)}</h2>
        <button class="pms-icon-btn" type="button" aria-label={ui.t('common.close')} onclick={() => onclose?.()}>
          <Icon name="close" />
        </button>
      </header>
      <div class="dialog-body">{@render children?.()}</div>
      {#if footer}
        <footer class="dialog-foot">{@render footer()}</footer>
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(2px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 100;
  }
  .dialog {
    width: min(560px, 100%);
    max-height: 85vh;
    display: flex;
    flex-direction: column;
  }
  .dialog-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 1rem;
    border-bottom: 1px solid var(--pms-border);
  }
  h2 {
    font-size: 1rem;
    font-weight: 600;
    margin: 0;
  }
  .dialog-body {
    padding: 1rem;
    overflow: auto;
  }
  .dialog-foot {
    padding: 0.75rem 1rem;
    border-top: 1px solid var(--pms-border);
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
