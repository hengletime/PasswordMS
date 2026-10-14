<script lang="ts">
  // 同步状态与手动同步（F-09）。失败原因按错误码映射为文案（由 store 统一 Toast）。
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  async function refresh(): Promise<void> {
    await sync.loadStatus();
    await sync.loadDevices();
  }
</script>

<div class="pms-card sync-card">
  <header class="sync-head">
    <h3>{ui.t('sync.tab_status')}</h3>
    <button class="pms-btn is-ghost is-sm" type="button" onclick={() => void refresh()}>
      <Icon name="refresh" size={14} />{ui.t('common.retry')}
    </button>
  </header>

  <div class="sync-stats">
    <div class="sync-stat">
      <span class="pms-label">{ui.t('sync.last_sync')}</span>
      <strong>
        {sync.status.last_sync_at ? formatDateTime(sync.status.last_sync_at, ui.locale) : ui.t('sync.never')}
      </strong>
    </div>
    <div class="sync-stat">
      <span class="pms-label">{ui.t('sync.pending')}</span>
      <strong>{sync.status.pending_changes}</strong>
    </div>
    <div class="sync-stat">
      <span class="pms-label">{ui.t('sync.device_count')}</span>
      <strong>{sync.status.device_count}</strong>
    </div>
  </div>

  {#if sync.devices.length === 0}
    <p class="pms-meta">{ui.t('sync.no_devices')}</p>
  {:else}
    <ul class="sync-list">
      {#each sync.devices as device (device.device_id)}
        <li class="sync-row">
          <span class="sync-name">{device.device_name}</span>
          <button class="pms-btn is-sm" type="button" onclick={() => void sync.syncNow(device.device_id)} disabled={sync.busy}>
            {#if sync.busy}
              <Spinner size={13} />
              {ui.t('sync.syncing')}
            {:else}
              <Icon name="sync" size={14} />
              {ui.t('sync.sync_now')}
            {/if}
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
  .sync-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.85rem;
  }
  h3 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }
  .sync-stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.6rem;
    margin-bottom: 1rem;
  }
  .sync-stat {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    padding: 0.65rem;
    background: var(--pms-surface);
  }
  .sync-stat strong {
    font-size: 0.95rem;
  }
  .sync-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .sync-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.55rem 0.65rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    background: var(--pms-surface);
  }
  .sync-name {
    font-size: 0.88rem;
  }
</style>
