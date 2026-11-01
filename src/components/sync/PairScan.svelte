<script lang="ts">
  // 加入配对（U3）：扫码为主 + 手动输入配对码兜底。
  // D-69：join 成功后显示**本次配对的会话码**，与电脑屏显示的那个值逐字核对，
  // 即可判断有没有被中间人隔开。方向固定：这里显示的是电脑（对端）的会话码。
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { hasSessionCode } from '../../lib/device-label';
  import { isPairingPayload } from '../../lib/qr';
  import { startCameraScan, type CameraScan } from '../../lib/qr-camera';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  let manual = $state('');
  let scanning = $state(false);
  let videoEl = $state<HTMLVideoElement | null>(null);
  let scan: CameraScan | null = null;

  async function startScan(): Promise<void> {
    if (scan !== null) {
      return;
    }
    // 先点亮预览（哪怕还是黑帧）：用户点了按钮必须立刻有反应，不能让权限弹窗期间的空白被读成「没生效」。
    scanning = true;
    if (videoEl === null) {
      scanning = false;
      return;
    }
    scan = await startCameraScan({
      video: videoEl,
      // 视野里别的二维码（普通网址、商品条码）一律忽略并继续扫 —— 返回 false 不放停扫描。
      onText: (text) => {
        if (!isPairingPayload(text)) {
          return false;
        }
        stopScan();
        void join(text);
        return true;
      },
      onError: (kind) => {
        scanning = false;
        scan = null;
        // 权限被拒 / 无摄像头给出可行动的提示，而不是笼统的「内部错误」（D-41 的既有口径）。
        ui.toast('error', kind === 'other' ? 'err.internal' : 'sync.camera_unavailable');
      },
    });
    if (scan === null) {
      // onError 已在等待期间把状态清掉了：别把按钮留在「扫描中」。
      scanning = false;
    }
  }

  function stopScan(): void {
    scan?.stop();
    scan = null;
    scanning = false;
  }

  // D-162：卸载时必须释放摄像头。原先 startScan 里拿的 stream 无人收尾，
  // 离开配对页后摄像头指示灯仍然亮着（同一份资源，谁 start 谁 stop）。
  $effect((): (() => void) => {
    return () => {
      scan?.stop();
      scan = null;
    };
  });

  async function join(text: string): Promise<void> {
    const payload = text.trim();
    if (payload.length === 0) {
      return;
    }
    await sync.join(payload);
    manual = '';
  }

  function onManualSubmit(event: SubmitEvent): void {
    event.preventDefault();
    void join(manual);
  }
</script>

<div class="pms-card sync-card">
  <h3>{ui.t('sync.join_title')}</h3>

  <!-- D-148：配对成功 → **停在这里核对会话码**，不跳页。
       顺序有意义：核对态必须优先于扫码界面判定，否则扫码/手动输入框还在，
       用户会以为没成功又扫一次（第二次会覆盖 `lastPairSessionCode`，核对的码换了）。 -->
  {#if sync.pairAwaitingConfirm}
    <p class="pms-meta" role="status"><strong>{ui.t('sync.pair_completed')}</strong></p>

    <!-- D-69：会话码与电脑屏同值（`sha256(本次临时公钥)` 前 8 字节），全靠用户逐字核对。
         提示用 .pms-hint.is-warning —— 这是要求用户执行的安全动作，不是普通说明。 -->
    {#if hasSessionCode(sync.lastPairSessionCode)}
      <div class="sync-fp">
        <span class="pms-label">{ui.t('sync.peer_session_code')}</span>
        <span class="sync-mono sync-fp-code">{sync.lastPairSessionCode}</span>
        {#if sync.lastPairDeviceName}<p class="pms-meta">{sync.lastPairDeviceName}</p>{/if}
        <p class="pms-hint is-warning">{ui.t('sync.session_code_hint')}</p>
      </div>
    {/if}

    <!-- 只有用户点这里才「收起 + 去状态页」：核对是人的判断，不能被一次成功的 RPC 代劳。 -->
    <button class="pms-btn is-primary" type="button" onclick={() => sync.confirmPairing()}>
      {ui.t('sync.pair_done')}
    </button>
  {:else}
    <p class="pms-meta">{ui.t('sync.scan_hint')}</p>

    <video bind:this={videoEl} class="sync-preview" class:hidden={!scanning} muted playsinline></video>

    <div class="sync-row">
      {#if scanning}
        <button class="pms-btn is-ghost" type="button" onclick={stopScan}>
          <Icon name="close" size={15} />{ui.t('common.cancel')}
        </button>
      {:else}
        <button class="pms-btn" type="button" onclick={() => void startScan()}>
          <Icon name="qr" size={15} />{ui.t('common.search')}
        </button>
      {/if}
    </div>

    <form class="sync-manual" onsubmit={onManualSubmit}>
      <textarea class="pms-input sync-manual-input" rows="3" placeholder={ui.t('sync.paste_hint')} bind:value={manual}></textarea>
      <button class="pms-btn is-primary" type="submit" disabled={sync.busy || manual.trim().length === 0}>
        {#if sync.busy}<Spinner size={15} />{/if}
        {ui.t('sync.join')}
      </button>
    </form>
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
  .sync-preview {
    width: 100%;
    border-radius: var(--pms-r);
    background: #000;
    aspect-ratio: 4 / 3;
    object-fit: cover;
  }
  .sync-preview.hidden {
    display: none;
  }
  .sync-row {
    display: flex;
    gap: 0.4rem;
  }
  .sync-manual {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .sync-manual-input {
    font-family: var(--pms-mono);
    font-size: 0.76rem;
  }
  /* D-69：会话码核对块。只做布局与断行（§5.7：不得自写外观）。 */
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
