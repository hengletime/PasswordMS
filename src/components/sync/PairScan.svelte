<script lang="ts">
  // 加入配对（U3）：扫码为主 + 手动输入配对码兜底。
  // D-69：join 成功后显示**本次配对的会话码**，与电脑屏显示的那个值逐字核对，
  // 即可判断有没有被中间人隔开。方向固定：这里显示的是电脑（对端）的会话码。
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { hasSessionCode } from '../../lib/device-label';
  import { decodeQrFromVideo, isPairingPayload } from '../../lib/qr';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  let manual = $state('');
  let scanning = $state(false);
  let videoEl = $state<HTMLVideoElement | null>(null);
  let stream: MediaStream | null = null;
  let handle = 0;

  async function startScan(): Promise<void> {
    if (scanning) {
      return;
    }
    try {
      // 分辨率给 ideal 约束：桌面定焦摄像头拍手机屏幕时，高分辨率帧显著提高 jsQR 识别率（D-40）。
      stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'environment', width: { ideal: 1280 }, height: { ideal: 720 } },
      });
      if (videoEl !== null) {
        videoEl.srcObject = stream;
        await videoEl.play();
      }
      scanning = true;
      // 250ms：jsQR 单帧 720p 约 3-8ms，5fps 抓拍足够跟手。
      handle = window.setInterval(tick, 250);
    } catch (err) {
      scanning = false;
      // 权限被拒 / 无摄像头给出可行动的提示，而不是笼统的「内部错误」。
      const name = err instanceof DOMException ? err.name : '';
      ui.toast('error', name === 'NotAllowedError' || name === 'NotFoundError' ? 'sync.camera_unavailable' : 'err.internal');
      if (name !== 'NotAllowedError' && name !== 'NotFoundError') {
        console.error('[pair-scan] getUserMedia failed', err);
      }
    }
  }

  function stopScan(): void {
    scanning = false;
    if (handle !== 0) {
      clearInterval(handle);
      handle = 0;
    }
    if (stream !== null) {
      stream.getTracks().forEach((track): void => track.stop());
      stream = null;
    }
  }

  async function tick(): Promise<void> {
    if (videoEl === null) {
      return;
    }
    const text = decodeQrFromVideo(videoEl);
    if (text !== null && isPairingPayload(text)) {
      stopScan();
      await join(text);
    }
  }

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

  <!-- D-69：提示用 .pms-hint.is-warning —— 这是要求用户执行的安全动作，不是普通说明。
       两屏必须一致；不一致说明连接可能被中间人隔开。 -->
  {#if hasSessionCode(sync.lastPairSessionCode)}
    <div class="sync-fp">
      <span class="pms-label">{ui.t('sync.peer_session_code')}</span>
      <span class="sync-mono sync-fp-code">{sync.lastPairSessionCode}</span>
      {#if sync.lastPairDeviceName}<p class="pms-meta">{sync.lastPairDeviceName}</p>{/if}
      <p class="pms-hint is-warning">{ui.t('sync.session_code_hint')}</p>
    </div>
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
