<script lang="ts">
  // 全局确认对话框：由 `ui.confirm(...)` 驱动。设计规范 §5.4：卡片 r-lg + shadow；确认按钮危险态用 is-danger。
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
      class="pms-card dialog"
      role="alertdialog"
      aria-modal="true"
      aria-label={ui.t(request.titleKey)}
      tabindex="-1"
    >
      <h2>{ui.t(request.titleKey)}</h2>
      <p class="dialog-msg">{ui.t(request.messageKey, request.params)}</p>
      <div class="dialog-actions">
        <button class="pms-btn is-ghost" type="button" onclick={() => ui.resolveConfirm(false)}>
          {ui.t('common.cancel')}
        </button>
        <button class="pms-btn is-danger" type="button" onclick={() => ui.resolveConfirm(true)}>
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
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(2px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    z-index: 150;
  }
  .dialog {
    width: min(440px, 100%);
    padding: 1.25rem;
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
    font-weight: 600;
  }
  .dialog-msg {
    margin: 0 0 1rem;
    color: var(--pms-muted);
    font-size: 0.9rem;
    line-height: 1.5;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
