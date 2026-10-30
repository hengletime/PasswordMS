<script lang="ts">
  // 同步状态与手动同步（F-09）。失败原因按错误码映射为文案（由 store 统一 Toast）。
  // D-66：「立即同步」的忙碌态**按设备隔离**（`sync.syncingDeviceId`）——
  // 被点的那台转圈 + `sync.syncing`，其余台只置灰不转圈（判定见 lib/device-label.ts）。
  // D-134：页面常驻「本机网络」与每台设备的「配对记录地址」——手动同步失败时，
  // 用户唯一能自己判断的是「两端地址/端口是否对得上」，看不到就只剩一句「网络不可达」。
  import { onMount } from 'svelte';
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { syncButtonState } from '../../lib/device-label';
  import { api } from '../../lib/ipc';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';
  import type { SyncNetInfoDto } from '../../types';

  /** 本机网络诊断（只读）。`null` = 尚未取到（不显示，也不据此下结论）。 */
  let net = $state<SyncNetInfoDto | null>(null);

  async function refreshNet(): Promise<void> {
    try {
      net = await api.sync.netInfo();
    } catch {
      // 诊断取不到不影响同步本身：保持上一份，绝不让它把页面变成错误态。
    }
  }

  async function refresh(): Promise<void> {
    await sync.loadStatus();
    await sync.loadDevices();
    await refreshNet();
  }

  /** 点「立即同步」：无论成败都顺手刷新本机网络——失败时用户要看的正是那两个数。 */
  async function syncOne(deviceId: string): Promise<void> {
    await sync.syncNow(deviceId);
    await refreshNet();
  }

  onMount((): void => {
    void refreshNet();
  });

  /** 设备行的「配对记录地址」：IP 与端口可能各自缺失，缺哪个就显示未知。 */
  function recordedAddr(ip?: string | null, port?: number | null): string {
    if (!ip) {
      return ui.t('sync.addr_unknown');
    }
    return port ? `${ip}:${port}` : ip;
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
        {@const st = syncButtonState(sync.syncingDeviceId, device.device_id)}
        <li class="sync-row">
          <span class="sync-who">
            <span class="sync-name">{device.device_name}</span>
            <span class="sync-addr">{recordedAddr(device.peer_ip, device.peer_port)}</span>
          </span>
          <button class="pms-btn is-sm" type="button" onclick={() => void syncOne(device.device_id)} disabled={st.disabled}>
            {#if st.syncing}
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

  <!-- D-134：本机网络。与设备列表之间用分隔线隔开——它是「诊断信息」，不是可点的操作。 -->
  <div class="sync-net">
    <div class="sync-net-line">
      <span class="pms-label">{ui.t('sync.local_net')}</span>
      <span class="sync-net-value">
        {#if net === null}
          {ui.t('sync.net_unknown')}
        {:else}
          {net.local_ips.length > 0 ? net.local_ips.join(' / ') : ui.t('sync.net_unknown')}
          <span class="sync-net-sep">·</span>
          {#if net.listen_port === null}
            {ui.t('sync.not_listening')}
          {:else}
            {ui.t('sync.listen_port', { port: net.listen_port })}
          {/if}
        {/if}
      </span>
    </div>
    {#if net !== null && !net.discovery_enabled}
      <!-- 发现应答没起来 = 这台设备不会回应别人的「换网后自动找对端」广播。 -->
      <p class="pms-meta sync-net-warn">{ui.t('sync.discovery_off', { port: net.discovery_port })}</p>
    {/if}
    <!-- 这条是**行动指引**，不是免责声明：手动同步最常见的失败原因是另一台设备息屏后
         被系统掐掉后台网络，用户看到的却只有一句「网络不可达」。 -->
    <p class="pms-meta">{ui.t('sync.manual_hint')}</p>
  </div>
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
  /* 名称与记录地址上下叠放：地址是次要信息，但必须留在同一行里才好对照。 */
  .sync-who {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }
  .sync-name {
    font-size: 0.88rem;
  }
  .sync-addr {
    font-size: 0.75rem;
    color: var(--pms-muted);
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
  .sync-net {
    margin-top: 0.9rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--pms-border);
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .sync-net-line {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
    font-size: 0.82rem;
  }
  .sync-net-value {
    color: var(--pms-fg);
    text-align: right;
    font-variant-numeric: tabular-nums;
    /* 多网卡机器会列出多条地址，必须允许换行而不是挤出卡片。 */
    overflow-wrap: anywhere;
  }
  .sync-net-sep {
    margin: 0 0.3rem;
    color: var(--pms-muted);
  }
  .sync-net-warn {
    color: var(--pms-warning);
  }
</style>
