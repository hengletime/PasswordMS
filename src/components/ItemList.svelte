<script lang="ts">
  // 条目列表（PRD F-04 / F-08）。
  import Icon from './common/Icon.svelte';
  import Spinner from './common/Spinner.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import type { ItemDto } from '../types';

  interface Props {
    onnew: (kind: 'login' | 'secure_note') => void;
  }
  const { onnew }: Props = $props();

  let query = $state('');
  let timer: ReturnType<typeof setTimeout> | null = null;

  function onSearchInput(event: Event): void {
    const value = (event.target as HTMLInputElement).value;
    query = value;
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = setTimeout((): void => {
      void vault.setQuery(value);
    }, 220);
  }

  function iconFor(item: ItemDto): string {
    if (item.kind.type === 'login') return 'key';
    if (item.kind.type === 'secure_note') return 'note';
    return 'paperclip';
  }

  function typeLabel(item: ItemDto): string {
    if (item.kind.type === 'login') return ui.t('item.type_login');
    if (item.kind.type === 'secure_note') return ui.t('item.type_note');
    return ui.t('item.type_attachment');
  }
</script>

<section class="item-list">
  <header>
    <div class="search">
      <Icon name="search" size={16} />
      <input placeholder={ui.t('common.search')} value={query} oninput={onSearchInput} />
    </div>
    <div class="actions">
      <button class="btn ghost" type="button" onclick={() => onnew('login')}>
        <Icon name="plus" size={15} />{ui.t('item.new_login')}
      </button>
      <button class="btn ghost" type="button" onclick={() => onnew('secure_note')}>
        <Icon name="plus" size={15} />{ui.t('item.new_note')}
      </button>
    </div>
  </header>

  {#if vault.busy}
    <div class="center"><Spinner label={ui.t('common.loading')} /></div>
  {:else if vault.items.length === 0}
    <div class="center empty">{ui.t('item.empty')}</div>
  {:else}
    <ul>
      {#each vault.items as item (item.item_id)}
        <li>
          <button
            class="entry"
            class:active={vault.selectedItemId === item.item_id}
            type="button"
            onclick={() => vault.selectItem(item.item_id)}
          >
            <span class="icon"><Icon name={iconFor(item)} size={16} /></span>
            <span class="meta">
              <span class="title">{item.kind.title}</span>
              <span class="sub">{typeLabel(item)}</span>
            </span>
            {#if item.favorite}
              <span class="fav"><Icon name="star" size={14} /></span>
            {/if}
          </button>
        </li>
      {:else}
        <li class="center empty">{ui.t('item.no_results')}</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .item-list {
    width: 300px;
    flex-shrink: 0;
    border-right: 1px solid var(--pms-border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  header {
    padding: 0.6rem;
    border-bottom: 1px solid var(--pms-border);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--pms-border);
    border-radius: 7px;
    padding: 0 0.5rem;
    color: var(--pms-muted);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--pms-fg);
    padding: 0.45rem 0;
    outline: none;
    font-size: 0.85rem;
  }
  .actions {
    display: flex;
    gap: 0.35rem;
  }
  .actions .btn {
    flex: 1;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0.4rem;
    overflow: auto;
    flex: 1;
  }
  .entry {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--pms-fg);
    cursor: pointer;
    text-align: left;
  }
  .entry:hover {
    background: var(--pms-hover);
  }
  .entry.active {
    background: var(--pms-accent-soft);
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .title {
    font-size: 0.88rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-size: 0.72rem;
    color: var(--pms-muted);
  }
  .fav {
    color: var(--pms-warning);
    line-height: 0;
  }
  .center {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1.5rem;
  }
  .empty {
    color: var(--pms-muted);
    font-size: 0.85rem;
  }
</style>
