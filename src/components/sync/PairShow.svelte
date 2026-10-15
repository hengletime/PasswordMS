<script lang="ts">
  // 配对二维码展示（U3：扫码为主 + 文本配对码兜底）。
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { api } from '../../lib/ipc';
  import { toAppError } from '../../lib/errors';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  async function start(): Promise<void> {
    await sync.startPairing();
  }

  async function cancel(): Promise<void> {
    await sync.cancelPairing();
  }

  async function copyCode(): Promise<void> {
    if (sync.pairing === null) {
      return;
    }
    try {
      await api.system.copySecret(sync.pairing.qr_text, 0);
      ui.toast('success', 'common.copied');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }
</script>

<div class="pms-card sync-card">
  <h3>{ui.t('sync.tab_pair')}</h3>

  {#if sync.pairing === null}
    {#if sync.pairEnded}
      <p class="pms-hint pair-ended" role="alert">{ui.t('sync.pair_ended')}</p>
    {/if}
    <button class="pms-btn is-primary" type="button" onclick={() => void start()} disabled={sync.busy}>
      {#if sync.busy}<Spinner size={15} />{:else}<Icon name="qr" size={15} />{/if}
      {ui.t('sync.start_pairing')}
    </button>
  {:else}
    <p class="pms-meta">{ui.t('sync.qr_hint')}</p>
    <div class="pms-card sync-qr">
      <!-- 后端返回受信的纯矢量 SVG（无脚本）。 -->
      {@html sync.pairing.qr_svg}
    </div>

    <p class="pms-meta">{ui.t('sync.pair_ip', { ip: sync.pairing.ip })}</p>
    <p class="pms-hint">{ui.t('sync.firewall_hint')}</p>
    <p class="pms-meta">{ui.t('sync.manual_hint')}</p>
    <div class="pms-input-group sync-code">
      <code>{sync.pairing.qr_text}</code>
      <button class="pms-icon-btn" type="button" aria-label={ui.t('sync.copy_code')} onclick={() => void copyCode()}>
        <Icon name="copy" size={16} />
      </button>
    </div>
    <p class="pms-hint">{ui.t('common.expires')}: {formatDateTime(sync.pairing.expires_at, ui.locale)}</p>

    <button class="pms-btn is-ghost" type="button" onclick={() => void cancel()}>{ui.t('sync.cancel')}</button>
  {/if}
</div>

<style>
  .sync-card {
    padding: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }
  h3 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }
  .sync-qr {
    align-self: center;
    background: #fff;
    padding: 0.6rem;
    border-radius: var(--pms-r);
    line-height: 0;
  }
  .sync-qr :global(svg) {
    width: 220px;
    height: 220px;
  }
  .sync-code {
    padding-left: 0.7rem;
  }
  .sync-code code {
    flex: 1;
    min-width: 0;
    font-family: var(--pms-mono);
    font-size: 0.72rem;
    word-break: break-all;
    max-height: 3.6rem;
    overflow: auto;
  }
  .pair-ended {
    color: var(--pms-warning);
  }
</style>
