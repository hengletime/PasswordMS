<script lang="ts">
  // 回收站（PRD F-15）：还原 / 彻底删除 / 清空（二次确认）。
  // 设计规范 §5.4：与 ItemList 同构；到期时间用 .pms-badge.is-warning。
  import Icon from './common/Icon.svelte';
  import { formatDateTime } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { ui } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import type { TrashDto } from '../types';

  let entries = $state<TrashDto[]>([]);
  let busy = $state(false);

  async function load(): Promise<void> {
    busy = true;
    try {
      entries = await api.trash.list();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      busy = false;
    }
  }

  $effect((): void => {
    void load();
  });

  async function restore(entry: TrashDto): Promise<void> {
    try {
      await api.trash.restore(entry.trash_id);
      await load();
      await vault.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  async function purge(entry: TrashDto): Promise<void> {
    const ok = await ui.confirm('trash.purge_confirm_title', 'trash.purge_confirm_body');
    if (!ok) {
      return;
    }
    try {
      await api.trash.purge(entry.trash_id);
      await load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  async function emptyAll(): Promise<void> {
    const ok = await ui.confirm('trash.empty_confirm_title', 'trash.empty_confirm_body');
    if (!ok) {
      return;
    }
    try {
      await api.trash.empty();
      await load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }
</script>

<section class="trash">
  <header class="trash-head">
    <h2>{ui.t('trash.title')}</h2>
    <button
      class="pms-btn is-danger"
      type="button"
      onclick={() => void emptyAll()}
      disabled={entries.length === 0 || busy}
    >
      <Icon name="trash" size={15} />{ui.t('trash.empty')}
    </button>
  </header>

  {#if entries.length === 0}
    <p class="pms-meta">{ui.t('trash.none')}</p>
  {:else}
    <ul class="trash-list">
      {#each entries as entry (entry.trash_id)}
        <li class="trash-row">
          <div class="trash-meta">
            <span class="trash-title">{entry.title}</span>
            <span class="trash-sub">
              {ui.t('trash.deleted_at')} {formatDateTime(entry.deleted_at, ui.locale)} ·
              <span class="pms-badge is-warning">{ui.t('trash.expire_at')} {formatDateTime(entry.expire_at, ui.locale)}</span>
            </span>
          </div>
          <div class="trash-actions">
            <button class="pms-btn is-ghost is-sm" type="button" onclick={() => void restore(entry)}>
              {ui.t('trash.restore')}
            </button>
            <button class="pms-btn is-ghost is-sm trash-danger" type="button" onclick={() => void purge(entry)}>
              {ui.t('trash.purge')}
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .trash {
    flex: 1;
    padding: 1.1rem;
    overflow: auto;
  }
  .trash-head {
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
  .trash-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .trash-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 0.75rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    background: var(--pms-surface);
  }
  .trash-meta {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    min-width: 0;
  }
  .trash-title {
    font-size: 0.9rem;
    font-weight: 600;
  }
  .trash-sub {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.35rem;
    font-size: 0.74rem;
    color: var(--pms-muted);
  }
  .trash-actions {
    display: flex;
    gap: 0.35rem;
  }
  .trash-danger {
    color: var(--pms-danger);
  }
</style>
