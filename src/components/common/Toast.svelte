<script lang="ts">
  // Toast 容器：渲染 `ui.toasts`。设计规范 §5.4：右下角；--pms-r；左侧 3px 状态色竖条。
  import Icon from './Icon.svelte';
  import { ui } from '../../lib/stores/ui.svelte';
</script>

<div class="toast-stack" aria-live="polite">
  {#each ui.toasts as toast (toast.id)}
    <div class="toast" class:is-error={toast.kind === 'error'} class:is-success={toast.kind === 'success'}>
      <Icon name={toast.kind === 'error' ? 'alert' : toast.kind === 'success' ? 'check' : 'shield'} size={16} />
      <span class="toast-msg">{ui.t(toast.messageKey, toast.params)}</span>
      <button
        class="toast-dismiss"
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
    gap: 0.55rem;
    padding: 0.6rem 0.7rem;
    border-radius: var(--pms-r);
    background: var(--pms-elevated);
    color: var(--pms-fg);
    border: 1px solid var(--pms-border);
    border-left: 3px solid var(--pms-accent);
    box-shadow: var(--pms-shadow);
    font-size: 0.875rem;
  }
  .toast.is-error {
    border-left-color: var(--pms-danger);
  }
  .toast.is-success {
    border-left-color: var(--pms-success);
  }
  .toast-msg {
    flex: 1;
    min-width: 0;
  }
  .toast-dismiss {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--pms-faint);
    cursor: pointer;
    padding: 0.15rem;
    line-height: 0;
    border-radius: var(--pms-r-sm);
  }
  .toast-dismiss:hover {
    color: var(--pms-fg);
    background: var(--pms-bg-2);
  }
</style>
