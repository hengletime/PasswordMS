<script lang="ts">
  // 密码体检（PRD F-16）：弱口令 / 重复使用 / 超期三类统计与整改建议。
  // 设计规范 §5.4：健康分做大字 + 4 个统计卡，弱项用 .pms-badge.is-danger。
  import Icon from './common/Icon.svelte';
  import Spinner from './common/Spinner.svelte';
  import { HEALTH_STALE_DAYS } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { ui } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import type { HealthReport } from '../types';

  let report = $state<HealthReport | null>(null);
  let busy = $state(false);

  function titleOf(itemId: string): string {
    return vault.items.find((i): boolean => i.item_id === itemId)?.kind.title ?? itemId;
  }

  async function run(): Promise<void> {
    busy = true;
    try {
      report = await api.health.check(null);
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      busy = false;
    }
  }
</script>

<section class="health">
  <header class="health-head">
    <h2>{ui.t('health.title')}</h2>
    <button class="pms-btn is-primary" type="button" onclick={() => void run()} disabled={busy}>
      {#if busy}<Spinner size={15} />{:else}<Icon name="refresh" size={15} />{/if}
      {ui.t('health.run')}
    </button>
  </header>

  {#if report === null}
    <p class="pms-meta">{ui.t('health.logins')}: {vault.items.length}</p>
  {:else}
    <div class="pms-card health-score" data-score={report.total_score}>
      <strong>{report.total_score}</strong>
      <span class="pms-label">{ui.t('health.score')}</span>
    </div>

    <div class="health-grid">
      <div class="pms-card health-cell">
        <span class="pms-label">{ui.t('health.logins')}</span>
        <strong>{report.login_count}</strong>
      </div>
      <div class="pms-card health-cell is-danger">
        <span class="pms-label">{ui.t('health.weak')}</span>
        <strong>{report.weak.length}</strong>
      </div>
      <div class="pms-card health-cell is-warning">
        <span class="pms-label">{ui.t('health.reused')}</span>
        <strong>{report.reused.length}</strong>
      </div>
      <div class="pms-card health-cell is-warning">
        <span class="pms-label">{ui.t('health.stale')} ({HEALTH_STALE_DAYS})</span>
        <strong>{report.stale.length}</strong>
      </div>
    </div>

    {#if report.weak.length > 0 || report.reused.length > 0 || report.stale.length > 0}
      <div class="health-lists">
        {#if report.weak.length > 0}
          <div class="health-list">
            <span class="pms-label">{ui.t('health.weak')}</span>
            <div class="health-chips">
              {#each report.weak as id (id)}<span class="pms-badge is-danger">{titleOf(id)}</span>{/each}
            </div>
          </div>
        {/if}
        {#if report.reused.length > 0}
          <div class="health-list">
            <span class="pms-label">{ui.t('health.reused')}</span>
            <div class="health-chips">
              {#each report.reused as id (id)}<span class="pms-badge is-warning">{titleOf(id)}</span>{/each}
            </div>
          </div>
        {/if}
        {#if report.stale.length > 0}
          <div class="health-list">
            <span class="pms-label">{ui.t('health.stale')}</span>
            <div class="health-chips">
              {#each report.stale as id (id)}<span class="pms-badge is-warning">{titleOf(id)}</span>{/each}
            </div>
          </div>
        {/if}
      </div>
    {/if}

    <div class="health-suggestions">
      <span class="pms-label">{ui.t('health.suggestions')}</span>
      <ul>
        {#each report.suggestions as suggestion (suggestion)}
          <li>{suggestion}</li>
        {/each}
      </ul>
    </div>
  {/if}
</section>

<style>
  .health {
    flex: 1;
    padding: 1.1rem;
    overflow: auto;
  }
  .health-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }
  h2 {
    margin: 0;
    font-size: 1.2rem;
    font-weight: 600;
  }
  .health-score {
    display: inline-flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.6rem 1rem;
    margin-bottom: 1rem;
  }
  .health-score strong {
    font-size: 2.6rem;
    font-weight: 700;
    color: var(--pms-accent);
    line-height: 1;
  }
  .health-score[data-score='0'] strong,
  .health-score[data-score='1'] strong,
  .health-score[data-score='2'] strong {
    color: var(--pms-danger);
  }
  .health-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 0.75rem;
    margin-bottom: 1rem;
  }
  .health-cell {
    padding: 0.8rem;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .health-cell strong {
    font-size: 1.4rem;
    font-weight: 700;
  }
  .health-cell.is-danger strong {
    color: var(--pms-danger);
  }
  .health-cell.is-warning strong {
    color: var(--pms-warning);
  }
  .health-lists {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    margin-bottom: 1rem;
  }
  .health-list {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .health-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .health-suggestions ul {
    margin: 0.4rem 0 0;
    padding-left: 1.1rem;
    font-size: 0.88rem;
    line-height: 1.6;
  }
</style>
