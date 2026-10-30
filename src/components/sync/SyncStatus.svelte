<script lang="ts">
  // 同步状态与手动同步（F-09）。失败原因按错误码映射为文案（由 store 统一 Toast）。
  // D-66：「立即同步」的忙碌态**按设备隔离**（`sync.syncingDeviceId`）——
  // 被点的那台转圈 + `sync.syncing`，其余台只置灰不转圈（判定见 lib/device-label.ts）。
  import Icon from '../common/Icon.svelte';
  import Spinner from '../common/Spinner.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { syncButtonState } from '../../lib/device-label';
  import { sync } from '../../lib/stores/sync.svelte';
  import { settings } from '../../lib/stores/settings.svelte';
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
        {@const st = syncButtonState(sync.syncingDeviceId, device.device_id)}
        <li class="sync-row">
          <span class="sync-name">{device.device_name}</span>
          <button class="pms-btn is-sm" type="button" onclick={() => void sync.syncNow(device.device_id)} disabled={st.disabled}>
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

  <!-- D-130：自动同步的**如实回执**。原实现失败全部吞掉，界面上只剩「上次同步时间没变」，
       而「没变」同时对应「没跑」「跑了失败」「跑成功了但没变更」三种现实 —— 用户只能猜。 -->
  <div class="sync-auto">
    <div class="sync-auto-line">
      <span class="pms-label">{ui.t('sync.auto_title')}</span>
      <span class="sync-auto-state">
        {ui.t('sync.auto_switch_line', {
          auto: settings.value.auto_sync ? ui.t('sync.auto_state_on') : ui.t('sync.auto_state_off'),
          onstart: settings.value.sync_on_start ? ui.t('sync.auto_state_on') : ui.t('sync.auto_state_off'),
        })}
      </span>
    </div>
    <div class="sync-auto-line">
      <span class="pms-label">{ui.t('sync.auto_last')}</span>
      {#if sync.autoLast === null}
        <span class="sync-auto-state">{ui.t('sync.auto_not_yet')}</span>
      {:else}
        {@const a = sync.autoLast}
        <span class="sync-auto-state" class:is-bad={!a.ok}>
          {formatDateTime(a.at, ui.locale)} · {a.ok
            ? a.okCount === a.total
              ? ui.t('sync.auto_ok', { n: a.okCount })
              : ui.t('sync.auto_partial', { ok: a.okCount, total: a.total })
            : ui.t('sync.auto_failed', { total: a.total })}{#if !a.ok && a.errorKey}&nbsp;· {ui.t(a.errorKey)}{/if}
        </span>
      {/if}
    </div>
    {#if sync.autoLast !== null && !sync.autoLast.ok}
      <!-- 这条是给「为什么同步不上」的**行动指引**，不是免责声明。 -->
      <p class="pms-meta sync-auto-hint">{ui.t('sync.auto_failed_hint')}</p>
    {/if}
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
  .sync-name {
    font-size: 0.88rem;
  }
  /* D-130：自动同步回执区。与上方设备列表之间用一条分隔线隔开——
     它是「诊断信息」，不是可点的操作，混在列表里会被当成又一条设备。 */
  .sync-auto {
    margin-top: 0.9rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--pms-border);
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .sync-auto-line {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
    font-size: 0.82rem;
  }
  .sync-auto-state {
    color: var(--pms-fg);
    text-align: right;
    /* 失败原因可能较长（含错误文案），允许换行而不是挤出卡片。 */
    overflow-wrap: anywhere;
  }
  .sync-auto-state.is-bad {
    color: var(--pms-danger);
  }
  .sync-auto-hint {
    margin: 0.15rem 0 0;
    line-height: 1.5;
  }
</style>
