<script lang="ts">
  // 已配对设备列表（O-07：解除配对）。设计规范 §5.4：设备行 = 图标 + 名称 + 指纹 .pms-meta + 解绑 ghost 按钮。
  import Icon from '../common/Icon.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  async function unpair(deviceId: string, name: string): Promise<void> {
    const ok = await ui.confirm('sync.unpair', 'sync.unpair_confirm', { name });
    if (ok) {
      await sync.unpair(deviceId);
    }
  }
</script>

<div class="pms-card sync-card">
  <h3>{ui.t('sync.devices')}</h3>

  {#if sync.devices.length === 0}
    <p class="pms-meta">{ui.t('sync.no_devices')}</p>
  {:else}
    <ul class="sync-list">
      {#each sync.devices as device (device.device_id)}
        <li class="sync-device">
          <span class="sync-device-icon"><Icon name="device" size={18} /></span>
          <div class="sync-device-meta">
            <span class="sync-device-name">{device.device_name}</span>
            <span class="sync-device-sub">
              {ui.t('sync.fingerprint')}: <span class="sync-mono">{device.device_id}</span>
            </span>
            <span class="sync-device-sub">
              {ui.t('sync.last_sync')}:
              {device.last_sync_at ? formatDateTime(device.last_sync_at, ui.locale) : ui.t('sync.never')}
            </span>
          </div>
          <button class="pms-btn is-ghost is-sm sync-danger" type="button" onclick={() => void unpair(device.device_id, device.device_name)}>
            {ui.t('sync.unpair')}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .sync-card {
    padding: 1.1rem;
  }
  h3 {
    margin: 0 0 0.7rem;
    font-size: 1rem;
    font-weight: 600;
  }
  .sync-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .sync-device {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    padding: 0.65rem 0.75rem;
    background: var(--pms-surface);
  }
  .sync-device-icon {
    color: var(--pms-accent);
    display: flex;
  }
  .sync-device-meta {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .sync-device-name {
    font-size: 0.9rem;
    font-weight: 600;
  }
  .sync-device-sub {
    font-size: 0.72rem;
    color: var(--pms-muted);
    word-break: break-all;
  }
  .sync-mono {
    font-family: var(--pms-mono);
  }
  .sync-danger {
    color: var(--pms-danger);
  }
</style>
