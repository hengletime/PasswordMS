<script lang="ts">
  // 加入配对（U3）：扫码为主 + 手动输入配对码兜底。
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
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
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' } });
      if (videoEl !== null) {
        videoEl.srcObject = stream;
        await videoEl.play();
      }
      scanning = true;
      handle = window.setInterval(tick, 400);
    } catch {
      scanning = false;
      ui.toast('error', 'err.internal');
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

<div class="card">
  <h3>{ui.t('sync.join_title')}</h3>
  <p class="hint">{ui.t('sync.scan_hint')}</p>

  <video bind:this={videoEl} class="preview" class:hidden={!scanning} muted playsinline></video>

  <div class="row">
    {#if scanning}
      <button class="btn ghost" type="button" onclick={stopScan}>
        <Icon name="close" size={15} />{ui.t('common.cancel')}
      </button>
    {:else}
      <button class="btn" type="button" onclick={() => void startScan()}>
        <Icon name="qr" size={15} />{ui.t('common.search')}
      </button>
    {/if}
  </div>

  <form class="manual" onsubmit={onManualSubmit}>
    <textarea rows="3" placeholder={ui.t('sync.paste_hint')} bind:value={manual}></textarea>
    <button class="btn primary" type="submit" disabled={sync.busy || manual.trim().length === 0}>
      {#if sync.busy}<Spinner size={15} />{/if}
      {ui.t('sync.join')}
    </button>
  </form>
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
  .preview {
    width: 100%;
    border-radius: 10px;
    background: #000;
    aspect-ratio: 4 / 3;
    object-fit: cover;
  }
  .preview.hidden {
    display: none;
  }
  .row {
    display: flex;
    gap: 0.4rem;
  }
  .manual {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .manual textarea {
    width: 100%;
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    background: var(--pms-bg);
    color: var(--pms-fg);
    padding: 0.5rem;
    font-family: var(--pms-mono);
    font-size: 0.76rem;
    resize: vertical;
  }
</style>
