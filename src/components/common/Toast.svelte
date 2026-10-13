<script lang="ts">
  // Toast 容器：渲染 `ui.toasts`。
  import Icon from './Icon.svelte';
  import { ui } from '../../lib/stores/ui.svelte';
</script>

<div class="toast-stack" aria-live="polite">
  {#each ui.toasts as toast (toast.id)}
    <div class="toast {toast.kind}">
      <Icon name={toast.kind === 'error' ? 'alert' : toast.kind === 'success' ? 'check' : 'shield'} size={16} />
      <span>{ui.t(toast.messageKey, toast.params)}</span>
      <button
        class="dismiss"
        type="button"
        aria-label={ui.t('common.close')}
        onclick={() => ui.dismiss(toast.id)}
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-stack {
    position: fixed;
    right: 1rem;
    bottom: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    z-index: 200;
    max-width: min(360px, 90vw);
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.6rem 0.75rem;
    border-radius: 8px;
    background: var(--pms-surface);
    color: var(--pms-fg);
    border: 1px solid var(--pms-border);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
    font-size: 0.875rem;
  }
  .toast.error {
    border-color: var(--pms-danger);
    color: var(--pms-danger);
  }
  .toast.success {
    border-color: var(--pms-success);
    color: var(--pms-success);
  }
  .dismiss {
    margin-left: auto;
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    line-height: 0;
  }
</style>
