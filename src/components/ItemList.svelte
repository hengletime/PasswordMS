<script lang="ts">
  // 条目列表（PRD F-04 / F-08）。设计规范 §5.4：行高 56px、选中左侧 2px 青绿竖条、空态居中图标+文案。
  // #531：行拖拽 = 钉住 / 钉住排序；桌面可拖到侧栏分组行（含「全部条目」= 移到未分组）；
  // 右键（桌面）/ 长按（触屏）弹菜单：钉住 / 取消钉住 / 移动到分组…。
  import Icon from './common/Icon.svelte';
  import Spinner from './common/Spinner.svelte';
  import ContextMenu from './common/ContextMenu.svelte';
  import Modal from './common/Modal.svelte';
  import { drag, beginRowDrag, DROP_ID_ALL_ITEMS } from '../lib/dnd.svelte';
  import { pinnedOrderAfterDrop } from '../lib/dnd';
  import { vault } from '../lib/stores/vault.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import type { ItemDto } from '../types';

  interface Props {
    onnew: (kind: 'login' | 'secure_note') => void;
    /** 用户在列表里点选了某条目（D-31：用于让右侧面板退出编辑态）。 */
    onselect?: () => void;
  }
  const { onnew, onselect }: Props = $props();

  /** 移动端底部导航遮挡（自动滚动底部预留；§6.8：56px + 安全区由布局 padding 承担，这里取导航高）。 */
  const MOBILE_BOTTOM_NAV_PX = 56;
  function isMobileViewport(): boolean {
    return window.matchMedia('(max-width: 767px)').matches;
  }

  let query = $state('');
  let timer: ReturnType<typeof setTimeout> | null = null;

  // D-81：store 的 query 被外部重置（保存成功后回落「全部条目」、锁定 reset）时，
  // 搜索框显示值必须跟着变 —— 「搜索框显示 = 实际过滤」是验收项。
  // 用 `queryEpoch` 区分两种「store 与本地不一致」：
  //   · epoch 变了 = 外部重置 → 取消挂着的防抖、把显示同步成 store 值；
  //   · epoch 没变 = 自己打字中（store 落后于本地是 220ms 防抖的正常延迟）→ 不动输入框，
  //     否则每个按键都会被清掉。
  let seenQueryEpoch = vault.queryEpoch;
  $effect(() => {
    if (vault.queryEpoch !== seenQueryEpoch) {
      seenQueryEpoch = vault.queryEpoch;
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      query = vault.query;
    }
  });

  function onSearchInput(event: Event): void {
    const value = (event.target as HTMLInputElement).value;
    query = value;
    if (timer !== null) {
      clearTimeout(timer);
    }
    timer = setTimeout((): void => {
      timer = null;
      // 防御性兜底：防抖期间本地被外部重置清空过的话，旧词不再生效
      // （正常路径 epoch effect 已把 timer 取消，这条护栏兜的是未来新增的重置路径）。
      if (query === value) {
        void vault.setQuery(value);
      }
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

  // ===== #531：拖拽 / 菜单 / 移动到分组 =====

  let listEl = $state<HTMLElement | null>(null);
  /** 拖过的那次 click 不算选中（pointerup 后浏览器仍会派发 click）。 */
  let clickSuppressed = false;
  /** 右键 / 长按菜单的目标条目与位置。 */
  let menuFor = $state<{ id: string; x: number; y: number } | null>(null);
  /** 「移动到分组…」弹层的目标条目。 */
  let moveFor = $state<string | null>(null);

  const menuTarget = $derived(menuFor === null ? null : vault.items.find((i) => i.item_id === menuFor?.id) ?? null);

  function openMenu(itemId: string, x: number, y: number): void {
    menuFor = { id: itemId, x, y };
  }

  function onMenuSelect(key: string): void {
    const id = menuFor?.id;
    menuFor = null;
    if (id === undefined) {
      return;
    }
    if (key === 'pin' || key === 'unpin') {
      void vault.setItemPin(id, key === 'pin');
      return;
    }
    if (key === 'move') {
      moveFor = id;
    }
  }

  /** D-81 的「保存可见」保证排序场景的补充：排序改变后新条目可能在列表下部 → 滚到选中行。 */
  $effect(() => {
    const id = vault.selectedItemId;
    if (id === null || listEl === null) {
      return;
    }
    listEl
      .querySelector(`[data-drop-id="${id}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  });

  function onRowPointerDown(event: PointerEvent, item: ItemDto): void {
    // 长按弹菜单 / 拖拽都从行按下开始；移动端底部导航遮挡计入自动滚动预留。
    beginRowDrag('item', item.item_id, event, {
      getLabel: (): string => item.kind.title,
      onLongPress: (x, y): void => openMenu(item.item_id, x, y),
      onDrop: (draggedId): void => {
        clickSuppressed = true;
        handleItemDrop(draggedId);
      },
      getScrollContainer: (): HTMLElement | null => listEl,
      bottomInset: (): number => (isMobileViewport() ? MOBILE_BOTTOM_NAV_PX : 0),
    });
  }

  function handleItemDrop(draggedId: string): void {
    const over = drag.state;
    if (over.overKind === 'folder') {
      // 跨分组投放：分组行 = 移到该组；「全部条目」= 移到未分组（null）。
      const folderId = over.overId === DROP_ID_ALL_ITEMS ? null : over.overId;
      void vault.moveItemToFolder(draggedId, folderId);
      return;
    }
    if (over.overKind !== 'item' || over.overId === null) {
      return;
    }
    const dragged = vault.items.find((i) => i.item_id === draggedId);
    const target = vault.items.find((i) => i.item_id === over.overId);
    if (dragged === undefined || target === undefined) {
      return;
    }
    // 后端在途时 wire 上没有 pinned（undefined），一律按「未钉」处理 —— 不误发钉住命令。
    const draggedPinned = dragged.pinned === true;
    const targetPinned = target.pinned === true;
    // 未钉区按频度排，频度不可手调：未钉条目落在未钉区 = 无动作。
    if (!draggedPinned && !targetPinned) {
      return;
    }
    const pinnedIds = vault.items.filter((i) => i.pinned === true).map((i) => i.item_id);
    const order = pinnedOrderAfterDrop(
      pinnedIds,
      draggedId,
      targetPinned ? target.item_id : null,
      over.overBefore,
    );
    if (order === null) {
      return;
    }
    void applyPinDrop(draggedId, draggedPinned, order);
  }

  /** 松手结算：未钉 → 先钉（set_item_pin），再按新顺序调一次 reorder（不连环刷）。 */
  async function applyPinDrop(draggedId: string, alreadyPinned: boolean, order: string[]): Promise<void> {
    if (!alreadyPinned) {
      await vault.setItemPin(draggedId, true);
    }
    await vault.reorderPinnedItems(order);
  }

  function confirmMove(folderId: string | null): void {
    const id = moveFor;
    moveFor = null;
    if (id === null) {
      return;
    }
    const current = vault.items.find((i) => i.item_id === id);
    // 目标就是所在分组 = 无动作（不发 IPC，不刷新）。
    if (current !== undefined && (current.folder_id ?? null) === folderId) {
      return;
    }
    void vault.moveItemToFolder(id, folderId);
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
    <ul class="list-items" bind:this={listEl}>
      {#each vault.items as item (item.item_id)}
        <li>
          <button
            class="list-row"
            class:active={vault.selectedItemId === item.item_id}
            class:drop-above={drag.state.active &&
              drag.state.kind === 'item' &&
              drag.state.overId === item.item_id &&
              drag.state.overBefore}
            class:drop-below={drag.state.active &&
              drag.state.kind === 'item' &&
              drag.state.overId === item.item_id &&
              !drag.state.overBefore}
            data-drop-id={item.item_id}
            data-drop-kind="item"
            type="button"
            onpointerdown={(event: PointerEvent): void => onRowPointerDown(event, item)}
            oncontextmenu={(event: MouseEvent): void => {
              event.preventDefault();
              openMenu(item.item_id, event.clientX, event.clientY);
            }}
            onclick={() => {
              // 拖拽松手后浏览器仍会派发 click —— 那一下不算选中。
              if (clickSuppressed) {
                clickSuppressed = false;
                return;
              }
              vault.selectItem(item.item_id);
              onselect?.();
            }}
          >
            <span class="list-icon"><Icon name={iconFor(item)} size={16} /></span>
            <span class="list-meta">
              <span class="list-title">{item.kind.title}</span>
              <span class="list-sub pms-meta">{typeLabel(item)}</span>
            </span>
            <!-- #531：拖拽手柄的辅助说明（视觉隐藏，进可访问名称）。 -->
            <span class="sr-only">{ui.t('dnd.handle_item')}</span>
            {#if item.pinned === true}
              <span class="list-pin" title={ui.t('menu.pin')}><Icon name="pin" size={13} /></span>
            {/if}
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

  <!-- #531：拖拽浮动占位（跟随指针；pointer-events:none 不挡 elementFromPoint）。 -->
  {#if drag.state.active && drag.state.kind === 'item'}
    <div class="drag-chip" style:left="{drag.state.x + 12}px" style:top="{drag.state.y + 12}px">
      {drag.state.label}
    </div>
  {/if}

  {#if menuFor !== null && menuTarget !== null}
    <ContextMenu
      x={menuFor.x}
      y={menuFor.y}
      entries={[
        { key: menuTarget.pinned === true ? 'unpin' : 'pin', label: menuTarget.pinned === true ? ui.t('menu.unpin') : ui.t('menu.pin') },
        { key: 'move', label: ui.t('menu.move_to_folder') },
      ]}
      onselect={onMenuSelect}
      onclose={() => (menuFor = null)}
    />
  {/if}

  <Modal open={moveFor !== null} titleKey="menu.move_title" onclose={() => (moveFor = null)}>
    <div class="move-list">
      <button class="move-row" type="button" onclick={() => confirmMove(null)}>
        <Icon name="key" size={15} />
        {ui.t('folder.ungrouped')}
      </button>
      {#each vault.folders as folder (folder.folder_id)}
        <button class="move-row" type="button" onclick={() => confirmMove(folder.folder_id)}>
          <Icon name="folder" size={15} />
          {folder.name}
        </button>
      {/each}
    </div>
  </Modal>
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
  /* 视觉隐藏（可访问文本）：不影响布局，屏幕阅读器可读。 */
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
  /* #531：钉住标记（钉住块语义的可见线索）。 */
  .list-pin {
    color: var(--pms-muted);
    display: flex;
  }
  /* 拖拽插入位指示线（落在行上半 = 上缘线，下半 = 下缘线）。 */
  .list-row.drop-above {
    box-shadow: inset 0 2px 0 var(--pms-accent);
  }
  .list-row.drop-below {
    box-shadow: inset 0 -2px 0 var(--pms-accent);
  }
  /* 拖拽浮动占位：跟随指针、不拦截任何指针事件（挡住 elementFromPoint 就找不到投放目标了）。 */
  .drag-chip {
    position: fixed;
    z-index: 120;
    max-width: 220px;
    padding: 0.35rem 0.65rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    background: var(--pms-surface);
    box-shadow: var(--pms-shadow);
    font-size: 0.84rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    pointer-events: none;
  }
  /* 移动到分组弹层：与列表同构的行式选择。 */
  .move-list {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    max-height: 46vh;
    overflow: auto;
  }
  .move-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 40px;
    padding: 0 0.55rem;
    border: none;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-fg);
    font: inherit;
    font-size: 0.88rem;
    text-align: left;
    cursor: pointer;
  }
  .move-row:hover {
    background: var(--pms-bg-2);
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
