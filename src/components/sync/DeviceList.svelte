<script lang="ts">
  // 已配对设备列表（O-07：解除配对）。
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

<div class="card">
  <h3>{ui.t('sync.devices')}</h3>

  {#if sync.devices.length === 0}
    <p class="empty">{ui.t('sync.no_devices')}</p>
  {:else}
    <ul>
      {#each sync.devices as device (device.device_id)}
        <li>
          <span class="icon"><Icon name="device" size={18} /></span>
          <div class="meta">
            <span class="name">{device.device_name}</span>
            <span class="sub mono">{device.device_id}</span>
            <span class="sub">
              {ui.t('sync.last_sync')}:
              {device.last_sync_at ? formatDateTime(device.last_sync_at, ui.locale) : ui.t('sync.never')}
            </span>
          </div>
          <button class="btn ghost small danger-text" type="button" onclick={() => void unpair(device.device_id, device.device_name)}>
            {ui.t('sync.unpair')}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .card {
    border: 1px solid var(--pms-border);
    border-radius: 12px;
    padding: 1rem;
  }
  h3 {
    margin: 0 0 0.6rem;
    font-size: 0.9rem;
    color: var(--pms-muted);
  }
  .empty {
    color: var(--pms-muted);
    font-size: 0.82rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  li {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    border: 1px solid var(--pms-border);
    border-radius: 10px;
    padding: 0.6rem 0.7rem;
  }
  .icon {
    color: var(--pms-accent);
    line-height: 0;
  }
  .meta {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-size: 0.88rem;
  }
  .sub {
    font-size: 0.72rem;
    color: var(--pms-muted);
    word-break: break-all;
  }
  .mono {
    font-family: var(--pms-mono);
  }
  .danger-text {
    color: var(--pms-danger);
  }
</style>
