<script lang="ts">
  // 左侧分组栏（PRD F-03）。设计规范 §5.3：240px、底色 --pms-bg-2、40px 行、选中左侧 2px 青绿竖条。
  // #531：「全部条目」固定第一、不可拖，但**是条目的投放目标**（拖上去 = 移到未分组）；
  // 分组行可拖 = 钉住 / 钉住排序；右键（桌面）/ 长按（触屏）菜单：钉住 / 取消钉住。
  // 双击重命名与 .side-del 原样保留（拖拽判位移阈值，不冲突）。
  import Icon from './common/Icon.svelte';
  import ContextMenu from './common/ContextMenu.svelte';
  import { drag, beginRowDrag, DROP_ID_ALL_ITEMS } from '../lib/dnd.svelte';
  import { pinnedOrderAfterDrop } from '../lib/dnd';
  import { vault } from '../lib/stores/vault.svelte';
  import { ui } from '../lib/stores/ui.svelte';

  interface Props {
    /** 移动端抽屉：选中分组/收藏后回调，用于自动关闭抽屉（桌面端不传）。 */
    onpicked?: () => void;
  }
  const { onpicked }: Props = $props();

  let creating = $state(false);
  let newName = $state('');
  let editingId = $state<string | null>(null);
  let editingName = $state('');

  /** 拖过的那次 click 不算选中（pointerup 后浏览器仍会派发 click）。 */
  let clickSuppressed = false;
  /** 右键 / 长按菜单的目标分组与位置。 */
  let menuFor = $state<{ id: string; x: number; y: number } | null>(null);

  const menuTarget = $derived(
    menuFor === null ? null : vault.folders.find((f) => f.folder_id === menuFor?.id) ?? null,
  );

  function openMenu(folderId: string, x: number, y: number): void {
    menuFor = { id: folderId, x, y };
  }

  function onMenuSelect(key: string): void {
    const id = menuFor?.id;
    menuFor = null;
    if (id === undefined) {
      return;
    }
    if (key === 'pin' || key === 'unpin') {
      void vault.setFolderPin(id, key === 'pin');
    }
  }

  function submitNew(event: SubmitEvent): void {
    event.preventDefault();
    const name = newName.trim();
    if (name.length === 0) {
      creating = false;
      return;
    }
    void vault.createFolder(name);
    newName = '';
    creating = false;
  }

  function startEdit(folderId: string, name: string): void {
    editingId = folderId;
    editingName = name;
  }

  function submitEdit(event: SubmitEvent): void {
    event.preventDefault();
    const id = editingId;
    const name = editingName.trim();
    editingId = null;
    if (id !== null && name.length > 0) {
      void vault.updateFolder(id, name);
    }
  }

  async function removeFolder(folderId: string, name: string): Promise<void> {
    const ok = await ui.confirm('folder.delete_title', 'folder.delete_body', { name });
    if (ok) {
      await vault.deleteFolder(folderId, 'to_ungrouped');
    }
  }

  /** 选中分组（可传 null = 全部）：完成后通知父级（移动端据此关闭抽屉）。 */
  function pickFolder(folderId: string | null): void {
    void vault.selectFolder(folderId);
    onpicked?.();
  }

  /** 切换「仅收藏」：完成后通知父级关闭抽屉。 */
  function pickFavorite(): void {
    void vault.toggleFavoriteOnly();
    onpicked?.();
  }

  function onFolderPointerDown(event: PointerEvent, folderId: string, name: string): void {
    beginRowDrag('folder', folderId, event, {
      getLabel: (): string => name,
      onLongPress: (x, y): void => openMenu(folderId, x, y),
      onDrop: (draggedId): void => {
        clickSuppressed = true;
        handleFolderDrop(draggedId);
      },
      getScrollContainer: (): HTMLElement | null => null,
      bottomInset: (): number => 0,
    });
  }

  function handleFolderDrop(draggedId: string): void {
    const over = drag.state;
    // 这里只结算**分组拖拽**；条目拖到分组行的结算在发起方 ItemList.handleItemDrop
    // （拖拽控制器只挂在发起组件上，目标组件不重复结算）。
    if (drag.state.kind !== 'folder') {
      return;
    }
    if (over.overKind !== 'folder' || over.overId === null || over.overId === DROP_ID_ALL_ITEMS) {
      return;
    }
    const dragged = vault.folders.find((f) => f.folder_id === draggedId);
    const target = vault.folders.find((f) => f.folder_id === over.overId);
    if (dragged === undefined || target === undefined) {
      return;
    }
    // 后端在途时 wire 上没有 pinned（undefined）→ 一律按「未钉」处理，不误发命令。
    const draggedPinned = dragged.pinned === true;
    const targetPinned = target.pinned === true;
    if (!draggedPinned && !targetPinned) {
      return;
    }
    const pinnedIds = vault.folders.filter((f) => f.pinned === true).map((f) => f.folder_id);
    const order = pinnedOrderAfterDrop(
      pinnedIds,
      draggedId,
      targetPinned ? target.folder_id : null,
      over.overBefore,
    );
    if (order === null) {
      return;
    }
    void applyFolderPinDrop(draggedId, draggedPinned, order);
  }

  async function applyFolderPinDrop(
    draggedId: string,
    alreadyPinned: boolean,
    order: string[],
  ): Promise<void> {
    if (!alreadyPinned) {
      await vault.setFolderPin(draggedId, true);
    }
    await vault.reorderPinnedFolders(order);
  }
</script>

<aside class="side">
  <nav class="side-nav">
    <!-- #531：「全部条目」固定第一、不可拖，但是**条目**的投放目标（= 移到未分组）。 -->
    <button
      class="side-row"
      class:active={vault.selectedFolderId === null && !vault.favoriteOnly}
      class:drop-hover={drag.state.active &&
        drag.state.kind === 'item' &&
        drag.state.overId === DROP_ID_ALL_ITEMS}
      data-drop-id={DROP_ID_ALL_ITEMS}
      data-drop-kind="folder"
      type="button"
      onclick={() => pickFolder(null)}
    >
      <Icon name="key" size={16} />
      <span>{ui.t('folder.all')}</span>
    </button>
    <button
      class="side-row"
      class:active={vault.favoriteOnly}
      type="button"
      onclick={() => pickFavorite()}
    >
      <Icon name="star" size={16} />
      <span>{ui.t('nav.favorites')}</span>
    </button>
  </nav>

  <div class="side-head">
    <span>{ui.t('folder.all')}</span>
    <button class="pms-icon-btn side-add" type="button" aria-label={ui.t('folder.new')} onclick={() => (creating = true)}>
      <Icon name="plus" size={16} />
    </button>
  </div>

  {#if creating}
    <form class="side-form" onsubmit={submitNew}>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="pms-input"
        autofocus
        placeholder={ui.t('folder.name_placeholder')}
        bind:value={newName}
        onblur={() => (creating = false)}
      />
    </form>
  {/if}

  <ul class="side-folders">
    {#each vault.folders as folder (folder.folder_id)}
      <li>
        {#if editingId === folder.folder_id}
          <form class="side-form" onsubmit={submitEdit}>
            <!-- svelte-ignore a11y_autofocus -->
            <input class="pms-input" autofocus bind:value={editingName} onblur={() => (editingId = null)} />
          </form>
        {:else}
          <button
            class="side-row"
            class:active={vault.selectedFolderId === folder.folder_id && !vault.favoriteOnly}
            class:drop-hover={drag.state.active &&
              drag.state.kind === 'item' &&
              drag.state.overId === folder.folder_id}
            class:drop-above={drag.state.active &&
              drag.state.kind === 'folder' &&
              drag.state.overId === folder.folder_id &&
              drag.state.overBefore}
            class:drop-below={drag.state.active &&
              drag.state.kind === 'folder' &&
              drag.state.overId === folder.folder_id &&
              !drag.state.overBefore}
            data-drop-id={folder.folder_id}
            data-drop-kind="folder"
            type="button"
            onpointerdown={(event: PointerEvent): void =>
              onFolderPointerDown(event, folder.folder_id, folder.name)}
            ondblclick={() => startEdit(folder.folder_id, folder.name)}
            oncontextmenu={(event: MouseEvent): void => {
              event.preventDefault();
              openMenu(folder.folder_id, event.clientX, event.clientY);
            }}
            onclick={() => {
              // 拖拽松手后浏览器仍会派发 click —— 那一下不算选中。
              if (clickSuppressed) {
                clickSuppressed = false;
                return;
              }
              pickFolder(folder.folder_id);
            }}
          >
            <Icon name="folder" size={16} />
            <span class="side-name">{folder.name}</span>
            <!-- #531：拖拽手柄的辅助说明（视觉隐藏，进可访问名称）。 -->
            <span class="sr-only">{ui.t('dnd.handle_folder')}</span>
            {#if folder.pinned === true}
              <span class="side-pin" title={ui.t('menu.pin')}><Icon name="pin" size={12} /></span>
            {/if}
          </button>
          <button
            class="pms-icon-btn side-del"
            type="button"
            aria-label={ui.t('common.delete')}
            onclick={() => void removeFolder(folder.folder_id, folder.name)}
          >
            <Icon name="close" size={14} />
          </button>
        {/if}
      </li>
    {:else}
      <li class="side-empty">{ui.t('folder.empty')}</li>
    {/each}
  </ul>

  {#if menuFor !== null && menuTarget !== null}
    <ContextMenu
      x={menuFor.x}
      y={menuFor.y}
      entries={[
        {
          key: menuTarget.pinned === true ? 'unpin' : 'pin',
          label: menuTarget.pinned === true ? ui.t('menu.unpin') : ui.t('menu.pin'),
        },
      ]}
      onselect={onMenuSelect}
      onclose={() => (menuFor = null)}
    />
  {/if}

  {#if drag.state.active && drag.state.kind === 'folder'}
    <div class="drag-chip" style:left="{drag.state.x + 12}px" style:top="{drag.state.y + 12}px">
      {drag.state.label}
    </div>
  {/if}
</aside>

<style>
  .side {
    width: 240px;
    flex-shrink: 0;
    border-right: 1px solid var(--pms-border);
    background: var(--pms-bg-2);
    padding: 0.75rem;
    overflow: auto;
  }
  .side-nav,
  .side-folders {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .side-folders {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .side-folders li {
    display: flex;
    align-items: center;
  }
  .side-row {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 40px;
    padding: 0 0.55rem;
    border: none;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-fg);
    cursor: pointer;
    font-size: 0.88rem;
    font-family: inherit;
    text-align: left;
    transition: background var(--pms-dur) var(--pms-ease), color var(--pms-dur) var(--pms-ease);
  }
  .side-row:hover {
    background: var(--pms-elevated);
  }
  .side-row.active {
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .side-row.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 6px;
    bottom: 6px;
    width: 2px;
    border-radius: var(--pms-r-pill);
    background: var(--pms-accent);
  }
  .side-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .side-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0.9rem 0 0.3rem;
    padding-left: 0.55rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--pms-muted);
  }
  .side-add {
    width: 26px;
    height: 26px;
    flex: 0 0 26px;
  }
  .side-del {
    width: 26px;
    height: 26px;
    flex: 0 0 26px;
    opacity: 0;
  }
  li:hover .side-del {
    opacity: 1;
  }
  .side-form {
    padding: 0.2rem 0.1rem;
  }
  .side-form .pms-input {
    min-height: 34px;
    font-size: 0.85rem;
  }
  .side-empty {
    color: var(--pms-faint);
    font-size: 0.8rem;
    padding: 0.4rem 0.55rem;
  }
  /* #531：钉住标记 / 投放高亮 / 插入位指示线。 */
  .side-pin {
    margin-left: auto;
    color: var(--pms-muted);
    display: flex;
  }
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
  .side-row.drop-hover {
    background: var(--pms-accent-soft);
    outline: 1px dashed var(--pms-accent);
  }
  .side-row.drop-above {
    box-shadow: inset 0 2px 0 var(--pms-accent);
  }
  .side-row.drop-below {
    box-shadow: inset 0 -2px 0 var(--pms-accent);
  }
  /* 拖拽浮动占位：跟随指针、不拦截指针事件（不挡 elementFromPoint）。 */
  .drag-chip {
    position: fixed;
    z-index: 120;
    max-width: 200px;
    padding: 0.3rem 0.6rem;
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
  /* 移动端：分组栏被收进抽屉，占满抽屉宽高，否则会留下 240px 固定宽度的留白。 */
  @media (max-width: 767px) {
    .side {
      width: 100%;
      height: 100%;
      border-right: none;
    }
  }
</style>
