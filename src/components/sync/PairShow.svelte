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

<div class="card">
  <h3>{ui.t('sync.tab_pair')}</h3>

  {#if sync.pairing === null}
    <button class="btn primary" type="button" onclick={() => void start()} disabled={sync.busy}>
      {#if sync.busy}<Spinner size={15} />{:else}<Icon name="qr" size={15} />{/if}
      {ui.t('sync.start_pairing')}
    </button>
  {:else}
    <p class="hint">{ui.t('sync.qr_hint')}</p>
    <div class="qr">
      <!-- 后端返回受信的纯矢量 SVG（无脚本）。 -->
      {@html sync.pairing.qr_svg}
    </div>

    <p class="hint">{ui.t('sync.manual_hint')}</p>
    <div class="code-row">
      <code>{sync.pairing.qr_text}</code>
      <button class="icon-btn" type="button" aria-label={ui.t('sync.copy_code')} onclick={() => void copyCode()}>
        <Icon name="copy" size={16} />
      </button>
    </div>
    <p class="expires">{ui.t('common.expires')}: {formatDateTime(sync.pairing.expires_at, ui.locale)}</p>

    <button class="btn ghost" type="button" onclick={() => void cancel()}>{ui.t('sync.cancel')}</button>
  {/if}
</div>

<style>
  .card {
    border: 1px solid var(--pms-border);
    border-radius: 12px;
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  h3 {
    margin: 0;
    font-size: 0.9rem;
    color: var(--pms-muted);
  }
  .hint {
    margin: 0;
    font-size: 0.82rem;
    color: var(--pms-muted);
  }
  .qr {
    align-self: center;
    background: #fff;
    padding: 0.6rem;
    border-radius: 10px;
    line-height: 0;
  }
  .qr :global(svg) {
    width: 220px;
    height: 220px;
  }
  .code-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    padding: 0.4rem 0.55rem;
  }
  .code-row code {
    flex: 1;
    font-family: var(--pms-mono);
    font-size: 0.72rem;
    word-break: break-all;
    max-height: 3.6rem;
    overflow: auto;
  }
  .expires {
    margin: 0;
    font-size: 0.74rem;
    color: var(--pms-muted);
  }
</style>
