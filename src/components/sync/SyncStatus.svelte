<script lang="ts">
  // 同步状态与手动同步（F-09）。失败原因按错误码映射为文案（由 store 统一 Toast）。
  // D-66：「立即同步」的忙碌态**按设备隔离**（`sync.syncingDeviceId`）——
  // 被点的那台转圈 + `sync.syncing`，其余台只置灰不转圈（判定见 lib/device-label.ts）。
  // D-134：页面常驻「本机网络」与每台设备的「配对记录地址」——手动同步失败时，
  // 用户唯一能自己判断的是「两端地址/端口是否对得上」，看不到就只剩一句「网络不可达」。
  import { onMount } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { RESUME_EVENTS, hasTauriHost } from '../../lib/app-resume';
  import { formatDateTime } from '../../lib/constants';
  import { syncButtonState } from '../../lib/device-label';
  import { toAppError } from '../../lib/errors';
  import { api } from '../../lib/ipc';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';
  import type { LanPermissionDto, SyncNetInfoDto } from '../../types';

  /** 本机网络诊断（只读）。`null` = 尚未取到（不显示，也不据此下结论）。 */
  let net = $state<SyncNetInfoDto | null>(null);

  /**
   * D-149：Android 17 的局域网访问权限。
   *
   * `null` = 还没查到（什么都不渲染）。三种「不该显示警示」的情形都由数据本身表达：
   * 桌面 / 旧版 Android 是 `supported=false`，已授权是 `granted=true`。
   */
  let lanPerm = $state<LanPermissionDto | null>(null);
  let permBusy = $state(false);

  async function refreshPerm(): Promise<void> {
    try {
      lanPerm = await api.sync.lanPermission();
    } catch {
      // 查不到就退回「不适用」：这里弹红字只会给用户一条他无法处理的错误，
      // 而真正的信息（同步能不能成）本来就由下面的失败提示负责。
      lanPerm = null;
    }
  }

  /**
   * 用户点「授予权限」。
   *
   * 结果**以回包为准**而不是以「对话框弹过」为准：用户可能按返回键取消，也可能在
   * 系统设置里已把它设成永久拒绝（此时系统直接返回 denied，不弹窗）。两种情况都
   * 保留警示，并补一条「可去系统设置手动打开」的 toast——这是用户唯一还有的路。
   */
  async function grantPerm(): Promise<void> {
    if (permBusy) {
      return;
    }
    permBusy = true;
    try {
      lanPerm = await api.sync.requestLanPermission();
      if (lanPerm.supported && !lanPerm.granted) {
        // `info` 而不是 `error`：用户刚刚做了正确的事（点了授予），系统没批不等于他操作错。
        // 带 dedupeKey：连点几次只留最后一条（D-146 的机制），否则提示会堆成一摞。
        ui.toast('info', 'sync.lan_perm_denied', undefined, 'lan-perm');
      }
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      permBusy = false;
    }
  }

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
    await refreshPerm();
  }

  /** 点「立即同步」：无论成败都顺手刷新本机网络——失败时用户要看的正是那两个数。 */
  async function syncOne(deviceId: string): Promise<void> {
    await sync.syncNow(deviceId);
    await refreshNet();
  }

  // D-149：从系统设置里手动开/关过权限再切回来，警示必须跟着变——否则用户按提示
  // 去设置里开完，回到这里仍是一片红字，只会认为「开了也没用」。
  // 与 App.svelte 同一套两条链路（`app://resumed` + `tauri://resumed`），只刷权限状态，
  // 不动设备列表（那两份查询互不相关，`$effect` 也不该把两件事串在一起）。
  onMount((): (() => void) => {
    void refreshNet();
    void refreshPerm();
    const pending: Promise<UnlistenFn>[] = [];
    if (hasTauriHost()) {
      for (const name of RESUME_EVENTS) {
        pending.push(listen(name, () => void refreshPerm()));
      }
    }
    return () => {
      for (const off of pending) {
        void off.then((f) => f());
      }
    };
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

  <!-- D-149：权限没给时**必须顶在最上面**。下面的「上次同步 / 待同步 / 设备数」看起来
       一切正常，可只要这个权限缺着，任何一次同步都必然超时——用户会拿着正常的数据
       去查 WiFi、路由器、防火墙，全错方向。 -->
  {#if lanPerm !== null && lanPerm.supported && !lanPerm.granted}
    <div class="pms-alert is-warning sync-perm" role="alert">
      <Icon name="shield" size={16} />
      <span class="sync-perm-text">{ui.t('sync.lan_perm_missing')}</span>
      <button class="pms-btn is-sm" type="button" disabled={permBusy} onclick={() => void grantPerm()}>
        {#if permBusy}<Spinner size={13} />{/if}
        {ui.t('sync.lan_perm_grant')}
      </button>
    </div>
  {/if}

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

  <!-- D-151：给「待同步」这个数字一个主语。没有它，一个恒定的 58 分不清是
       「同步一直失败」还是「有条历史残影设备一条都没同步过」——用户只会反复点
       「立即同步」。数字口径不变（宁多勿少），只是把它归到具体设备上。 -->
  {#if sync.status.pending_changes > 0 && sync.status.pending_device_name}
    <p class="pms-meta sync-pending-who" class:is-stale={sync.status.pending_device_never_synced}>
      {#if sync.status.pending_device_never_synced}
        {ui.t('sync.pending_from_never', { name: sync.status.pending_device_name })}
      {:else}
        {ui.t('sync.pending_from', { name: sync.status.pending_device_name })}
      {/if}
    </p>
  {/if}

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
  </div>
</div>

<style>
  .sync-card {
    padding: 1.1rem;
  }
  /* D-149：局域网权限警示条。图标 + 说明 + 按钮，窄屏按钮自动掉到下一行——
     说明文字是**两行中文**，和按钮挤一行必然把按钮压成竖排。 */
  .sync-perm {
    margin-bottom: 0.9rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .sync-perm-text {
    flex: 1 1 12rem;
    min-width: 0;
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
  /* D-151：待同步归因行。必须**紧贴上面的数字**（负 margin 抵掉统计栅格的
     bottom gap）——它是那个数字的注解，不是页面底部的脚注。 */
  .sync-pending-who {
    margin: -0.55rem 0 1rem;
  }
  /* 「从未同步过」用警示色：它意味着这个数字不会因为同步而下降，
     与「只是落后一点」是两种完全不同的处境。 */
  .sync-pending-who.is-stale {
    color: var(--pms-warning);
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
  /* D-149：窄屏下「本机网络」标签与「IP / 端口」挤在同一行会折成两三行参差不齐
     （标签 4 字 + 一个 IP 就吃掉半行，多网卡时更甚）。改为上下两行：
     标签在上，值在下并左对齐——读数时视线只走一列，不必在两端来回跳。 */
  @media (max-width: 767px) {
    .sync-net-line {
      flex-direction: column;
      align-items: flex-start;
      gap: 0.15rem;
    }
    .sync-net-value {
      text-align: left;
    }
  }
</style>
