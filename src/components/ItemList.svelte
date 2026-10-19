<script lang="ts">
  // 条目列表（PRD F-04 / F-08）。设计规范 §5.4：行高 56px、选中左侧 2px 青绿竖条、空态居中图标+文案。
  import Icon from './common/Icon.svelte';
  import Spinner from './common/Spinner.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import type { ItemDto } from '../types';

  interface Props {
    /** D-59：kind 扩展 'attachment'——MainLayout 收到后走「选文件 → 导入」流程，不进编辑器。 */
    onnew: (kind: 'login' | 'secure_note' | 'attachment') => void;
    /** 用户在列表里点选了某条目（D-31：用于让右侧面板退出编辑态）。 */
    onselect?: () => void;
  }
  const { onnew, onselect }: Props = $props();

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

<section class="list">
  <header class="list-head">
    <div class="pms-input-group list-search">
      <span class="list-search-icon"><Icon name="search" size={16} /></span>
      <input class="pms-input" placeholder={ui.t('common.search')} value={query} oninput={onSearchInput} />
    </div>
    <div class="list-actions">
      <button class="pms-btn is-primary is-sm" type="button" onclick={() => onnew('login')}>
        <Icon name="plus" size={15} />{ui.t('item.new_login')}
      </button>
      <button class="pms-btn is-ghost is-sm" type="button" onclick={() => onnew('secure_note')}>
        <Icon name="plus" size={15} />{ui.t('item.new_note')}
      </button>
      <!-- D-59：附件导入入口（此前后端就绪但前端零入口，附件功能不可达）。桌面/移动共用本组件。 -->
      <button class="pms-btn is-ghost is-sm" type="button" onclick={() => onnew('attachment')}>
        <Icon name="paperclip" size={15} />{ui.t('item.new_attachment')}
      </button>
    </div>
  </header>

  {#if vault.busy}
    <div class="list-center"><Spinner label={ui.t('common.loading')} /></div>
  {:else if vault.items.length === 0}
    <div class="list-center list-empty">
      <Icon name="key" size={26} />
      <p>{ui.t('item.empty')}</p>
    </div>
  {:else}
    <ul class="list-items">
      {#each vault.items as item (item.item_id)}
        <li>
          <button
            class="list-row"
            class:active={vault.selectedItemId === item.item_id}
            type="button"
            onclick={() => {
              vault.selectItem(item.item_id);
              onselect?.();
            }}
          >
            <span class="list-icon"><Icon name={iconFor(item)} size={16} /></span>
            <span class="list-meta">
              <span class="list-title">{item.kind.title}</span>
              <span class="list-sub pms-meta">{typeLabel(item)}</span>
            </span>
            {#if item.favorite}
              <span class="list-fav"><Icon name="star" size={14} filled /></span>
            {/if}
          </button>
        </li>
      {:else}
        <li class="list-center list-empty">{ui.t('item.no_results')}</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .list {
    width: 300px;
    flex-shrink: 0;
    border-right: 1px solid var(--pms-border);
    background: var(--pms-surface);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .list-head {
    padding: 0.6rem;
    border-bottom: 1px solid var(--pms-border);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .list-search {
    padding-left: 0.55rem;
  }
  .list-search-icon {
    display: flex;
    color: var(--pms-faint);
  }
  .list-actions {
    display: flex;
    gap: 0.35rem;
  }
  .list-actions .pms-btn {
    flex: 1;
  }
  .list-items {
    list-style: none;
    margin: 0;
    padding: 0.35rem;
    overflow: auto;
    flex: 1;
  }
  .list-row {
    position: relative;
    width: 100%;
    min-height: 56px;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.4rem 0.55rem;
    border: none;
    border-left: 2px solid transparent;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-fg);
    cursor: pointer;
    text-align: left;
    transition: background var(--pms-dur) var(--pms-ease);
  }
  .list-row:hover {
    background: var(--pms-bg-2);
  }
  .list-row.active {
    background: var(--pms-accent-soft);
    border-left-color: var(--pms-accent);
  }
  .list-icon {
    display: flex;
    color: var(--pms-muted);
  }
  .list-row.active .list-icon {
    color: var(--pms-accent);
  }
  .list-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .list-title {
    font-size: 0.88rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list-sub {
    margin: 0;
  }
  .list-fav {
    color: var(--pms-warning);
    display: flex;
  }
  .list-center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 1.8rem 1rem;
    flex: 1;
  }
  .list-empty {
    color: var(--pms-muted);
    font-size: 0.85rem;
  }
  .list-empty p {
    margin: 0;
  }
  /* 移动端：列表独占整屏，宽度自适应（桌面的固定 300px + 右边框在单栏下无意义）。 */
  @media (max-width: 767px) {
    .list {
      width: auto;
      flex: 1;
      border-right: none;
    }
  }
</style>
