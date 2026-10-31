<script lang="ts">
  // 左侧分组栏（PRD F-03）。设计规范 §5.3：240px、底色 --pms-bg-2、40px 行、选中左侧 2px 青绿竖条。
  // #531：「全部条目」固定第一、不可拖，但**是条目的投放目标**（拖上去 = 移到未分组）；
  // 分组行可拖 = 钉住 / 钉住排序；右键（桌面）/ 长按（触屏）菜单：钉住 / 取消钉住 / 重命名（D-85）。
  // 点钉 = 直接取消钉住（D-85）；双击重命名与 .side-del 原样保留（拖拽判位移阈值，不冲突）。
  import Icon from './common/Icon.svelte';
  import ContextMenu from './common/ContextMenu.svelte';
  import Modal from './common/Modal.svelte';
  import { drag, beginRowDrag, DROP_ID_ALL_ITEMS } from '../lib/dnd.svelte';
  import { pinnedOrderAfterDrop } from '../lib/dnd';
  import { vault } from '../lib/stores/vault.svelte';
  import { ui } from '../lib/stores/ui.svelte';
  import { api } from '../lib/ipc';
  import { toAppError } from '../lib/errors';

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
  /** D-85：菜单「重命名」弹层的目标分组与输入草稿。 */
  let renameFor = $state<string | null>(null);
  let renameName = $state('');

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
      return;
    }
    if (key === 'rename') {
      // D-85：菜单重命名 —— 预填当前组名，弹 Modal 提交（提交走 renameFolder）。
      const folder = vault.folders.find((f) => f.folder_id === id);
      if (folder !== undefined) {
        renameFor = id;
        renameName = folder.name;
      }
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
      void renameFolder(id, name);
    }
  }

  /**
   * D-85：分组改名（菜单弹窗与双击改名共用一条路径）。空名/纯空白前端先挡（不发 IPC）。
   *
   * 契约核对（D-85 修复令要求，已核 `crates/passwordms-core/src/store/repository.rs::update_folder`）：
   * `update_folder(folder_id, name, icon, color)` 对 icon/color 是**整字段覆盖**
   * （`folder.icon = icon; folder.color = color`），传 None = **清除**现有值 ——
   * 所以必须把分组的 icon/color 原样回传。**不能用 `vault.updateFolder`**
   * （store 层传 null/null，会把分组的图标/颜色抹掉）。
   */
  async function renameFolder(folderId: string, name: string): Promise<void> {
    const target = vault.folders.find((f) => f.folder_id === folderId) ?? null;
    if (target === null || name.length === 0) {
      return;
    }
    try {
      await api.folders.update(target.folder_id, name, target.icon ?? null, target.color ?? null);
      await vault.load();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  async function submitRename(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const id = renameFor;
    const name = renameName.trim();
    renameFor = null;
    if (id !== null && name.length > 0) {
      await renameFolder(id, name);
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
    // D-84：未钉区也可手动排序 —— 目标 id 直接传给纯函数（含未钉目标 = 收编）。
    const pinnedIds = vault.folders.filter((f) => f.pinned === true).map((f) => f.folder_id);
    const order = pinnedOrderAfterDrop(
      pinnedIds,
      draggedId,
      target.folder_id,
      over.overBefore,
    );
    if (order === null) {
      return;
    }
    void applyFolderPinDrop(draggedId, draggedPinned, target.folder_id, targetPinned, order);
  }

  /**
   * 松手结算（D-84）：新收编的目标先钉，被拖者按需钉，最后按新序调一次 reorder
   * —— 全部 await 串行，保证 reorder 时后端 pinned 集合已含两者。
   */
  async function applyFolderPinDrop(
    draggedId: string,
    draggedPinned: boolean,
    targetId: string,
    targetPinned: boolean,
    order: string[],
  ): Promise<void> {
    if (!targetPinned) {
      await vault.setFolderPin(targetId, true);
    }
    if (!draggedPinned) {
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
      <span class="side-name">{ui.t('folder.all')}</span>
      <!-- D-152：全库条目数（与筛选无关）。用户拿它核对「同步有没有丢数据」。 -->
      <span class="side-count" class:is-zero={vault.counts.total === 0}>{vault.counts.total}</span>
    </button>
    <button
      class="side-row"
      class:active={vault.favoriteOnly}
      type="button"
      onclick={() => pickFavorite()}
    >
      <Icon name="star" size={16} />
      <span class="side-name">{ui.t('nav.favorites')}</span>
      <span class="side-count" class:is-zero={vault.counts.favorites === 0}>
        {vault.counts.favorites}
      </span>
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
            <!-- D-152：组内条目数。0 也要显示（用更淡的颜色）——一个组突然变成 0 条
                 正是「同步会不会丢数据」这个疑问最需要被看见的形态，藏起来等于帮他忽略。 -->
            <span class="side-count" class:is-zero={(folder.item_count ?? 0) === 0}>
              {folder.item_count ?? 0}
            </span>
            <!-- #531：拖拽手柄的辅助说明（视觉隐藏，进可访问名称）。 -->
            <span class="sr-only">{ui.t('dnd.handle_folder')}</span>
          </button>
          {#if folder.pinned === true}
            <!-- D-85：点钉 = 直接取消钉住。行按钮的**兄弟**节点（不嵌套 button——无效 HTML
                 且 SSR 解析会被浏览器挪位；兄弟节点也不吃行点击/拖拽/双击改名）。 -->
            <button
              class="side-pin"
              type="button"
              title={ui.t('menu.unpin')}
              aria-label={ui.t('menu.unpin')}
              onclick={(): void => void vault.setFolderPin(folder.folder_id, false)}
            >
              <Icon name="pin" size={12} />
            </button>
          {/if}
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
    <!-- D-152：未分组条目不属于任何分组行。缺这一句时「各组数字加起来 ≠ 全部」，
         用户正好会把它读成「同步丢了」。只在真有未分组条目时出现。 -->
    {#if vault.counts.ungrouped > 0}
      <li class="side-note">
        {ui.t('folder.ungrouped_note', { count: vault.counts.ungrouped })}
      </li>
    {/if}
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
        // D-85：分组重命名（提交见 submitRename；双击改名入口保留）。
        { key: 'rename', label: ui.t('menu.rename') },
      ]}
      onselect={onMenuSelect}
      onclose={() => (menuFor = null)}
    />
  {/if}

  <!-- D-85：重命名分组弹层（复用 Modal + pms-input；Esc/遮罩/取消不改动）。 -->
  <Modal open={renameFor !== null} titleKey="folder.rename_title" onclose={() => (renameFor = null)}>
    <form class="rename-form" onsubmit={submitRename}>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="pms-input"
        autofocus
        aria-label={ui.t('folder.rename_title')}
        placeholder={ui.t('folder.name_placeholder')}
        bind:value={renameName}
      />
      <button class="pms-btn is-primary is-sm" type="submit">{ui.t('folder.rename_confirm')}</button>
    </form>
  </Modal>

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
    /* D-152：吃掉剩余宽度，把条目数挤到行尾右对齐；`min-width: 0` 是 ellipsis 在
       flex 容器里生效的前提（少了它长分组名会把数字顶出可视区）。 */
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* D-152：条目数徽标。等宽数字（tabular-nums）—— 数字变化时行宽不抖，
     几组纵向扫一眼就能比大小。 */
  .side-count {
    flex: 0 0 auto;
    min-width: 1.5em;
    text-align: right;
    font-size: 0.76rem;
    font-variant-numeric: tabular-nums;
    color: var(--pms-muted);
    /* 选中行里数字跟着变强调色，否则在 accent-soft 底上看不清。 */
  }
  .side-row.active .side-count {
    color: var(--pms-accent);
  }
  .side-count.is-zero {
    /* 0 值只降不透明度、**不换到更淡的色阶**：`--pms-faint` 在 12px 上对比度不够，
       而「一个组突然变 0 条」恰恰是本轮最需要被看见的形态。 */
    opacity: 0.72;
  }
  .side-row.active .side-count.is-zero {
    opacity: 0.8;
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
  /* D-152：分组列表末尾的「另有 N 条未分组」——说明文字，不可点、不吃行高。 */
  .side-note {
    color: var(--pms-faint);
    font-size: 0.74rem;
    padding: 0.35rem 0.55rem 0.1rem;
    font-variant-numeric: tabular-nums;
  }
  /* #531：钉住标记 / 投放高亮 / 插入位指示线。D-85 起钉子可点 = 直接取消钉住，
     且是行按钮的**兄弟**节点（与 .side-del 同位型：26px flex 项），不嵌套 button。 */
  .side-pin {
    width: 26px;
    height: 26px;
    flex: 0 0 26px;
    color: var(--pms-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    cursor: pointer;
    transition: color var(--pms-dur) var(--pms-ease);
  }
  .side-pin:hover {
    color: var(--pms-fg);
  }
  /* D-85：重命名分组弹层表单（输入框 + 保存按钮同行）。 */
  .rename-form {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .rename-form .pms-input {
    flex: 1;
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
