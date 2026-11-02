<script lang="ts">
  // 冲突箱（PRD Q3）：保留本机 / 采用远端 / 两者都留，**默认不自动选择**。
  //
  // D-171：分组冲突与条目冲突共用一个箱子，但分组只有两项——选项由
  // `conflictChoices(record_type)` 给出，模板里**不写死三选一**（写死就会出现
  // 「点得下去、点了必报错」的按钮）。
  import Icon from '../common/Icon.svelte';
  import { formatDateTime } from '../../lib/constants';
  import { conflictChoices, isChoiceAllowed } from '../../lib/conflict';
  import { sync } from '../../lib/stores/sync.svelte';
  import { ui } from '../../lib/stores/ui.svelte';
  import type { ConflictChoice } from '../../types';

  function labelKey(choice: ConflictChoice): string {
    if (choice === 'keep_local') return 'conflict.keep_local';
    if (choice === 'take_remote') return 'conflict.take_remote';
    return 'conflict.keep_both';
  }
</script>

<div class="pms-card sync-card">
  <header class="sync-head">
    <h3>{ui.t('conflict.title')}</h3>
    <button class="pms-btn is-ghost is-sm" type="button" onclick={() => void sync.loadConflicts()}>
      <Icon name="refresh" size={14} />{ui.t('common.retry')}
    </button>
  </header>

  {#if sync.conflicts.length === 0}
    <p class="pms-meta">{ui.t('conflict.none')}</p>
  {:else}
    <ul class="conflict-list">
      {#each sync.conflicts as conflict (conflict.conflict_id)}
        <li class="conflict-item">
          <div class="conflict-head">
            <span class="conflict-id">
              <span class="conflict-kind" class:is-folder={conflict.record_type === 'folder'}>
                {ui.t(
                  conflict.record_type === 'folder' ? 'conflict.kind_folder' : 'conflict.kind_item',
                )}
              </span>
              {conflict.record_id}
            </span>
            <span class="pms-meta">
              {ui.t('conflict.detected_at')} {formatDateTime(conflict.detected_at, ui.locale)}
            </span>
          </div>

          <div class="conflict-sides">
            <div class="conflict-side">
              <span class="pms-label">{ui.t('conflict.local_title')}</span>
              <span class="conflict-value">{conflict.local_title}</span>
              <span class="pms-meta">{formatDateTime(conflict.local_updated_at, ui.locale)}</span>
            </div>
            <div class="conflict-side" class:is-deleted={conflict.remote_deleted}>
              <span class="pms-label">
                {ui.t('conflict.remote_title')}
                {#if conflict.remote_deleted}
                  <!-- 远端是墓碑：这个标题是删除前的内容，不标出来会让人以为远端也要保留它。 -->
                  <span class="conflict-deleted">{ui.t('conflict.remote_deleted')}</span>
                {/if}
              </span>
              <span class="conflict-value">{conflict.remote_title}</span>
              <span class="pms-meta">{formatDateTime(conflict.remote_updated_at, ui.locale)}</span>
            </div>
          </div>

          <div class="conflict-choices" role="radiogroup" aria-label={ui.t('conflict.choose')}>
            {#each conflictChoices(conflict.record_type) as choice (choice)}
              <button
                class="conflict-choice"
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
            class="pms-btn is-primary is-sm"
            type="button"
            disabled={!isChoiceAllowed(conflict.record_type, sync.choiceOf(conflict.conflict_id))}
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
  .conflict-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .conflict-item {
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    padding: 0.8rem;
    background: var(--pms-surface);
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }
  .conflict-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
  }
  .conflict-id {
    font-family: var(--pms-mono);
    font-size: 0.72rem;
    color: var(--pms-muted);
    word-break: break-all;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .conflict-kind {
    font-family: var(--pms-font);
    border: 1px solid var(--pms-border-strong);
    border-radius: var(--pms-r-sm);
    padding: 0 0.35rem;
    font-size: 0.7rem;
    color: var(--pms-muted);
    white-space: nowrap;
  }
  .conflict-kind.is-folder {
    border-color: var(--pms-accent);
    color: var(--pms-accent);
  }
  .conflict-deleted {
    color: var(--pms-danger);
    font-size: 0.72rem;
  }
  .conflict-side.is-deleted {
    border-style: dashed;
  }
  .conflict-sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.6rem;
  }
  .conflict-side {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r-sm);
    padding: 0.55rem;
    background: var(--pms-bg-2);
  }
  .conflict-value {
    font-size: 0.85rem;
    word-break: break-word;
  }
  .conflict-choices {
    display: flex;
    gap: 0.35rem;
    flex-wrap: wrap;
  }
  .conflict-choice {
    border: 1px solid var(--pms-border-strong);
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-fg);
    cursor: pointer;
    padding: 0.35rem 0.7rem;
    font-size: 0.82rem;
    font-family: inherit;
    transition: background var(--pms-dur) var(--pms-ease), border-color var(--pms-dur) var(--pms-ease),
      color var(--pms-dur) var(--pms-ease);
  }
  .conflict-choice:hover {
    background: var(--pms-bg-2);
  }
  .conflict-choice.active {
    border-color: var(--pms-accent);
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
</style>
