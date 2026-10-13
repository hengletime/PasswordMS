<script lang="ts">
  // 全局确认对话框：由 `ui.confirm(...)` 驱动。
  import { ui } from '../../lib/stores/ui.svelte';

  let request = $derived(ui.confirmRequest);
</script>

{#if request}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="overlay"
    role="presentation"
    onclick={(event: MouseEvent): void => {
      // 仅点击遮罩本身时视为取消。
      if (event.target === event.currentTarget) {
        ui.resolveConfirm(false);
      }
    }}
  >
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-label={ui.t(request.titleKey)}
      tabindex="-1"
    >
      <h2>{ui.t(request.titleKey)}</h2>
      <p>{ui.t(request.messageKey, request.params)}</p>
      <div class="actions">
        <button class="btn ghost" type="button" onclick={() => ui.resolveConfirm(false)}>
          {ui.t('common.cancel')}
        </button>
        <button class="btn danger" type="button" onclick={() => ui.resolveConfirm(true)}>
          {ui.t('common.confirm')}
        </button>
      </div>
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
    z-index: 150;
  }
  .dialog {
    background: var(--pms-surface);
    color: var(--pms-fg);
    border: 1px solid var(--pms-border);
    border-radius: 12px;
    width: min(440px, 100%);
    padding: 1.25rem;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }
  p {
    margin: 0 0 1rem;
    color: var(--pms-muted);
    font-size: 0.9rem;
    line-height: 1.5;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
