<script lang="ts">
  // 冲突箱三选一（PRD Q3）：保留本机 / 采用远端 / 两者都留，**默认不自动选择**。
  import Icon from '../common/Icon.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { CONFLICT_CHOICES } from '../../lib/conflict';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';
  import type { ConflictChoice } from '../../types';

  function labelKey(choice: ConflictChoice): string {
    if (choice === 'keep_local') return 'conflict.keep_local';
    if (choice === 'take_remote') return 'conflict.take_remote';
    return 'conflict.keep_both';
  }
</script>

<div class="card">
  <header>
    <h3>{ui.t('conflict.title')}</h3>
    <button class="btn ghost small" type="button" onclick={() => void sync.loadConflicts()}>
      <Icon name="refresh" size={14} />{ui.t('common.retry')}
    </button>
  </header>

  {#if sync.conflicts.length === 0}
    <p class="empty">{ui.t('conflict.none')}</p>
  {:else}
    <ul>
      {#each sync.conflicts as conflict (conflict.conflict_id)}
        <li>
          <div class="head">
            <span class="id mono">{conflict.item_id}</span>
            <span class="sub">
              {ui.t('conflict.detected_at')} {formatDateTime(conflict.detected_at, ui.locale)}
            </span>
          </div>

          <div class="sides">
            <div class="side">
              <span class="label">{ui.t('conflict.local_title')}</span>
              <span class="value">{conflict.local_title}</span>
              <span class="sub">{formatDateTime(conflict.local_updated_at, ui.locale)}</span>
            </div>
            <div class="side">
              <span class="label">{ui.t('conflict.remote_title')}</span>
              <span class="value">{conflict.remote_title}</span>
              <span class="sub">{formatDateTime(conflict.remote_updated_at, ui.locale)}</span>
            </div>
          </div>

          <div class="choices" role="radiogroup" aria-label={ui.t('conflict.choose')}>
            {#each CONFLICT_CHOICES as choice (choice)}
              <button
                class="choice"
                class:active={sync.choiceOf(conflict.conflict_id) === choice}
                type="button"
                role="radio"
                aria-checked={sync.choiceOf(conflict.conflict_id) === choice}
                onclick={() => sync.setChoice(conflict.conflict_id, choice)}
              >
                {ui.t(labelKey(choice))}
              </button>
            {/each}
          </div>

          <button
            class="btn primary small"
            type="button"
            disabled={sync.choiceOf(conflict.conflict_id) === null}
            onclick={() => void sync.resolve(conflict.conflict_id)}
          >
            {ui.t('common.confirm')}
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
    gap: 0.75rem;
  }
  li {
    border: 1px solid var(--pms-border);
    border-radius: 10px;
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
  .id {
    font-size: 0.72rem;
    color: var(--pms-muted);
    word-break: break-all;
  }
  .mono {
    font-family: var(--pms-mono);
  }
  .sub {
    font-size: 0.72rem;
    color: var(--pms-muted);
  }
  .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    padding: 0.5rem;
  }
  .label {
    font-size: 0.72rem;
    color: var(--pms-muted);
  }
  .value {
    font-size: 0.85rem;
    word-break: break-word;
  }
  .choices {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .choice {
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    background: transparent;
    color: var(--pms-fg);
    cursor: pointer;
    padding: 0.35rem 0.7rem;
    font-size: 0.82rem;
  }
  .choice:hover {
    background: var(--pms-hover);
  }
  .choice.active {
    border-color: var(--pms-accent);
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
</style>
