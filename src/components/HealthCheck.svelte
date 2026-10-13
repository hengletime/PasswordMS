<script lang="ts">
  // 密码体检（PRD F-16）：弱口令 / 重复使用 / 超期三类统计与整改建议。
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
  <header>
    <h2>{ui.t('health.title')}</h2>
    <button class="btn primary" type="button" onclick={() => void run()} disabled={busy}>
      {#if busy}<Spinner size={15} />{:else}<Icon name="refresh" size={15} />{/if}
      {ui.t('health.run')}
    </button>
  </header>

  {#if report === null}
    <p class="hint">{ui.t('health.logins')}: {vault.items.length}</p>
  {:else}
    <div class="score" data-score={report.total_score}>
      <strong>{report.total_score}</strong>
      <span>{ui.t('health.score')}</span>
    </div>

    <div class="grid">
      <div class="cell">
        <span class="label">{ui.t('health.logins')}</span>
        <strong>{report.login_count}</strong>
      </div>
      <div class="cell danger">
        <span class="label">{ui.t('health.weak')}</span>
        <strong>{report.weak.length}</strong>
      </div>
      <div class="cell warning">
        <span class="label">{ui.t('health.reused')}</span>
        <strong>{report.reused.length}</strong>
      </div>
      <div class="cell warning">
        <span class="label">{ui.t('health.stale')} ({HEALTH_STALE_DAYS})</span>
        <strong>{report.stale.length}</strong>
      </div>
    </div>

    {#if report.weak.length > 0 || report.reused.length > 0 || report.stale.length > 0}
      <div class="lists">
        {#if report.weak.length > 0}
          <div class="list">
            <span class="label">{ui.t('health.weak')}</span>
            {#each report.weak as id (id)}<span class="item">{titleOf(id)}</span>{/each}
          </div>
        {/if}
        {#if report.reused.length > 0}
          <div class="list">
            <span class="label">{ui.t('health.reused')}</span>
            {#each report.reused as id (id)}<span class="item">{titleOf(id)}</span>{/each}
          </div>
        {/if}
        {#if report.stale.length > 0}
          <div class="list">
            <span class="label">{ui.t('health.stale')}</span>
            {#each report.stale as id (id)}<span class="item">{titleOf(id)}</span>{/each}
          </div>
        {/if}
      </div>
    {/if}

    <div class="suggestions">
      <span class="label">{ui.t('health.suggestions')}</span>
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
    padding: 1rem;
    overflow: auto;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .hint {
    color: var(--pms-muted);
    font-size: 0.85rem;
  }
  .score {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    margin-bottom: 1rem;
  }
  .score strong {
    font-size: 2.4rem;
    color: var(--pms-accent);
  }
  .score[data-score='0'] strong,
  .score[data-score='1'] strong,
  .score[data-score='2'] strong {
    color: var(--pms-danger);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 0.75rem;
    margin-bottom: 1rem;
  }
  .cell {
    border: 1px solid var(--pms-border);
    border-radius: 10px;
    padding: 0.7rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .cell strong {
    font-size: 1.3rem;
  }
  .cell.danger strong {
    color: var(--pms-danger);
  }
  .cell.warning strong {
    color: var(--pms-warning);
  }
  .label {
    font-size: 0.76rem;
    color: var(--pms-muted);
  }
  .lists {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin-bottom: 1rem;
  }
  .list {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
  }
  .item {
    font-size: 0.78rem;
    padding: 0.15rem 0.5rem;
    border-radius: 6px;
    background: var(--pms-hover);
  }
  .suggestions ul {
    margin: 0.35rem 0 0;
    padding-left: 1.1rem;
    font-size: 0.88rem;
    line-height: 1.6;
  }
</style>
