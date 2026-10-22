<script lang="ts">
  // 配对二维码展示（U3：扫码为主 + 文本配对码兜底）。
  // D-69：二维码下方显示**本次配对的会话码**，供手机屏逐字核对，防中间人劫持。
  // 这里显示的必须与会话（临时密钥）绑定，**不能**显示本机长期身份指纹 ——
  // 手机屏算的是临时公钥的指纹，两者永远不等（详见 store 的 lastPairSessionCode 注释）。
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { hasSessionCode } from '../../lib/device-label';
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

  <!-- D-75：成功态**优先**判定 —— 此时 `pairing` 仍在（后端发 completed 时前端刻意不清），
       所以不能落到下面那个「已生成」分支，否则会继续显示二维码、把核对码挤在下面。
       成功态下刻意**不渲染二维码**：码已经扫过了，留着会让人以为还能再扫一次。 -->
  {#if sync.pairCompleted && sync.pairing !== null}
    <p class="pms-meta" role="status"><strong>{ui.t('sync.pair_completed')}</strong></p>
    <p class="pms-hint">{ui.t('sync.pair_completed_hint')}</p>

    <!-- 会话码：与手机屏同一值（`sha256(本次临时公钥)` 前 8 字节），全靠用户逐字核对。 -->
    {#if hasSessionCode(sync.pairing.session_code)}
      <div class="sync-fp">
        <span class="pms-label">{ui.t('sync.session_code')}</span>
        <span class="sync-mono sync-fp-code">{sync.pairing.session_code}</span>
      </div>
    {/if}

    <button class="pms-btn is-primary" type="button" onclick={() => sync.endPairing()}>
      {ui.t('sync.pair_done')}
    </button>
  {:else if sync.pairing === null}
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

    <!-- D-69：本次会话码。手机端配对成功后显示的是同一个值（同一把临时公钥算出来的），
         用户对一眼即可判中间人。 -->
    {#if sync.pairing !== null && hasSessionCode(sync.pairing.session_code)}
      <div class="sync-fp">
        <span class="pms-label">{ui.t('sync.session_code')}</span>
        <span class="sync-mono sync-fp-code">{sync.pairing.session_code}</span>
        <p class="pms-meta">{ui.t('sync.session_code_hint')}</p>
      </div>
    {/if}

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
  /* D-69：会话码核对块。只做布局与断行（§5.7：不得自写外观 —— 字号/颜色一律继承
     `.pms-label` / `.pms-meta` / `.pms-hint`，边框底色也交给既有类）。 */
  .sync-fp {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .sync-mono {
    font-family: var(--pms-mono);
  }
  .sync-mono.sync-fp-code {
    word-break: break-all;
  }
</style>
