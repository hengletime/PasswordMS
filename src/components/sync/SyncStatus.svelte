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

<div class="card">
  <header>
    <h3>{ui.t('sync.tab_status')}</h3>
    <button class="btn ghost small" type="button" onclick={() => void refresh()}>
      <Icon name="refresh" size={14} />{ui.t('common.retry')}
    </button>
  </header>

  <div class="stats">
    <div class="stat">
      <span>{ui.t('sync.last_sync')}</span>
      <strong>
        {sync.status.last_sync_at ? formatDateTime(sync.status.last_sync_at, ui.locale) : ui.t('sync.never')}
      </strong>
    </div>
    <div class="stat">
      <span>{ui.t('sync.pending')}</span>
      <strong>{sync.status.pending_changes}</strong>
    </div>
    <div class="stat">
      <span>{ui.t('sync.device_count')}</span>
      <strong>{sync.status.device_count}</strong>
    </div>
  </div>

  {#if sync.devices.length === 0}
    <p class="empty">{ui.t('sync.no_devices')}</p>
  {:else}
    <ul>
      {#each sync.devices as device (device.device_id)}
        <li>
          <span class="name">{device.device_name}</span>
          <button class="btn small" type="button" onclick={() => void sync.syncNow(device.device_id)} disabled={sync.busy}>
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
  .card {
    border: 1px solid var(--pms-border);
    border-radius: 12px;
    padding: 1rem;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.75rem;
  }
  h3 {
    margin: 0;
    font-size: 0.9rem;
    color: var(--pms-muted);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.6rem;
    margin-bottom: 0.9rem;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    border: 1px solid var(--pms-border);
    border-radius: 10px;
    padding: 0.6rem;
  }
  .stat span {
    font-size: 0.72rem;
    color: var(--pms-muted);
  }
  .stat strong {
    font-size: 0.95rem;
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
    gap: 0.4rem;
  }
  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.5rem 0.6rem;
    border: 1px solid var(--pms-border);
    border-radius: 10px;
  }
  .name {
    font-size: 0.88rem;
  }
</style>
