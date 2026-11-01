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
  import { folderLabelOf, folderLabelText, type FolderLabel } from '../lib/folder-label';
  import { shouldShowRowFolder } from '../lib/list-scope';
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

  /**
   * 搜索框**显示值**（与「实际生效的筛选词」`vault.query` 是两个东西，但那两者在空闲时
   * 必须相等 —— 这是验收项）。
   *
   * **初值必须取 `vault.query` 现值**：本组件会因切分区、手机端列表↔详情推进而被
   * **卸载重建**。初值写成 `''` 时，重建后 store 里那个仍在生效的关键词就没有任何界面
   * 痕迹了 —— 表现为「列表只有 5 条、搜索框是空的、切分组切标签都回不到全部」，
   * 用户会读成「数据丢了、软件卡住了」（D-158，v1.5.14 修复）。
   */
  let query = $state(vault.query);
  let timer: ReturnType<typeof setTimeout> | null = null;
  /**
   * 正在防抖、尚未提交给 store 的词（`null` = 空闲）。
   *
   * 判「本地显示与 store 不一致是谁的锅」全靠它：`pending !== null` = 是我自己还在打字，
   * store 落后是 220ms 防抖的正常延迟，**不许**拿 store 覆盖输入框（否则每个按键都会被清掉）；
   * `pending === null` = 空闲，此时两者不同只可能是外部改写，必须跟上 store。
   */
  let pending: string | null = null;
  /** 搜索框引用（v1.4.10：全局热键唤出后把焦点打进来）。 */
  let searchInput = $state<HTMLInputElement | undefined>(undefined);
  /**
   * 已消费的聚焦信号序号。
   *
   * 初始化为**挂载那一刻**的值：这样「用户在别的分区按过热键、随后切回条目列表」
   * 不会突然把搜索框抢走焦点——只响应挂载之后新产生的请求。
   */
  let consumedFocusSeq = ui.searchFocusSeq;

  $effect(() => {
    const seq = ui.searchFocusSeq;
    if (seq === consumedFocusSeq || !searchInput) {
      return;
    }
    consumedFocusSeq = seq;
    // `select()` 而不是只 `focus()`：按热键来就是为了搜东西，直接打字即覆盖旧关键词。
    searchInput.focus();
    searchInput.select();
  });

  /**
   * 取消防抖：**timer 与 pending 必须一起清**。
   *
   * 只清 timer 会留下一个非 null 的 `pending`，空闲对齐分支就永久瘫痪 —— 输入框从此不再
   * 跟随 store（比原 bug 更隐蔽）。所以这两件事封在一个函数里，不留给调用方记。
   */
  function cancelDebounce(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    pending = null;
  }

  /**
   * 卸载时丢弃未提交的词。
   *
   * 220ms 防抖窗口内切分区，timer 会在组件已经消失之后才 fire，替用户把一个**他已经
   * 放弃的词**写进 store。下一次重建时那个词又冒出来 —— 一个没有任何来源的幽灵筛选。
   * 组件销毁 = 用户离开了搜索框，没提交的输入就该作废。
   *
   * 只丢弃**未提交**的词；已提交的（store 里那份）由挂载初值 + 空闲对齐负责恢复，
   * 这正是「切分区回来还能看到自己在搜什么」的正常路径。
   */
  $effect((): (() => void) => () => {
    cancelDebounce();
  });

  // D-81：store 的 query 被外部重置（保存成功后回落「全部条目」、锁定 reset）时，
  // 搜索框显示值必须跟着变 —— 「搜索框显示 = 实际过滤」是验收项。
  // 用 `queryEpoch` 区分两种「store 与本地不一致」：
  //   · epoch 变了 = 外部重置 → 取消挂着的防抖、把显示同步成 store 值；
  //   · epoch 没变 = 自己打字中（store 落后于本地是 220ms 防抖的正常延迟）→ 不动输入框，
  //     否则每个按键都会被清掉。
  //
  // D-158：再加一条**空闲对齐**。只靠 epoch 等于要求「以后每条改 query 的路径都记得推进
  // epoch」，而这次出问题的恰恰**没有路径**：组件卸载重建、本地归零，store 没变、epoch 也没变。
  // `pending === null` 是「我没在打字」的准确判据，此时两者不同只可能是外部改写，跟上它。
  let seenQueryEpoch = vault.queryEpoch;
  $effect(() => {
    const storeQuery = vault.query;
    if (vault.queryEpoch !== seenQueryEpoch) {
      seenQueryEpoch = vault.queryEpoch;
      cancelDebounce();
      query = storeQuery;
      return;
    }
    if (pending === null && query !== storeQuery) {
      query = storeQuery;
    }
  });

  function onSearchInput(event: Event): void {
    const value = (event.target as HTMLInputElement).value;
    query = value;
    if (timer !== null) {
      clearTimeout(timer);
    }
    // 先立 pending 再挂 timer：Svelte 的 effect 在本次事件处理结束后才跑，那时 pending
    // 必须已经是非 null，否则上面那条空闲对齐会把刚敲的字判成「外部改写」清掉。
    pending = value;
    timer = setTimeout((): void => {
      timer = null;
      // 防御性兜底：防抖期间本地被外部重置清空过的话，旧词不再生效
      // （正常路径 epoch effect 已把 timer 取消，这条护栏兜的是未来新增的重置路径）。
      if (query !== value) {
        pending = null;
        return;
      }
      // 顺序有讲究：**先 setQuery，再清 pending**。
      // `setQuery` 首行 `this.query = query` 在第一个 `await` 之前同步执行，所以它返回时
      // store 已经是新词；反过来先清 pending 的话，中间那一瞬会被空闲对齐读成「外部改写」，
      // 把用户刚打的词清回旧值。
      void vault.setQuery(value);
      pending = null;
    }, 220);
  }

  // D-101：一键清空搜索——必须**先取消挂着的 220ms 防抖**（否则旧词随后 setQuery
  // 又写回去），再同步本地显示与 store（setQuery 内部走 load()，与手输路径同源）。
  // 不碰 queryEpoch：epoch 只由外部重置路径推进（D-81），本按钮是自己发起的清空。
  function clearSearch(): void {
    cancelDebounce();
    query = '';
    void vault.setQuery('');
  }

  /**
   * D-160：副行要不要显示「所属分组」。
   *
   * 只在**列表里可能出现不同分组条目**时显示（全部 / 收藏）；进了某个分组或未分组视图
   * 就隐藏 —— 那时每行都写同一个组名，重复 20 遍只会挤掉副行本来的信息（长组名尤甚）。
   * 判定本身在纯函数里（收藏是「跨组」的那一个例外，见 `list-scope.ts`）。
   */
  const showRowFolder = $derived(
    shouldShowRowFolder({
      folderId: vault.selectedFolderId,
      favoriteOnly: vault.favoriteOnly,
      ungroupedOnly: vault.ungroupedOnly,
    }),
  );

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

  /**
   * D-154：所属分组的文案。
   *
   * `vault.folders` 是独立 IPC 拉回的**全量**分组（与列表筛选无关），所以在「全部」
   * 视图里也能正确显示每个条目所属的组名 —— 这正是本轮要解决的那件事。
   * 传箭头函数而不是 `ui.t`：`t` 内部读 `this.locale`，裸传方法引用会丢 `this`。
   */
  function folderText(label: FolderLabel): string {
    return folderLabelText(label, (key): string => ui.t(key));
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
        // D-106（豆包审查 3.5-6）：兜底复位——若拖拽后浏览器未派发 click
        // （指针落行外/窗口失焦），flag 会永真吞掉下一次真实点击。
        // 正常路径 click 事件先于 setTimeout(0) 执行，语义不变。
        setTimeout((): void => {
          clickSuppressed = false;
        }, 0);
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
    // D-84：未钉区也可手动排序 —— 拖 A 到 B 上把 B 一并收编进钉住块（视觉与拖放位置一致）。
    // 目标 id 直接传给纯函数（含未钉目标 = 收编），不再按 targetPinned 折叠成 null。
    const pinnedIds = vault.items.filter((i) => i.pinned === true).map((i) => i.item_id);
    const order = pinnedOrderAfterDrop(
      pinnedIds,
      draggedId,
      target.item_id,
      over.overBefore,
    );
    if (order === null) {
      return;
    }
    void applyPinDrop(draggedId, draggedPinned, target.item_id, targetPinned, order);
  }

  /**
   * 松手结算（D-84）：新收编的目标先钉，被拖者按需钉，最后按新序调一次 reorder
   * —— 全部 await 串行，保证 reorder 时后端 pinned 集合已含两者（否则整体校验 ERR_VALIDATION）。
   * order 数组原样下发（已含新收编目标），无需预过滤。
   */
  async function applyPinDrop(
    draggedId: string,
    draggedPinned: boolean,
    targetId: string,
    targetPinned: boolean,
    order: string[],
  ): Promise<void> {
    if (!targetPinned) {
      await vault.setItemPin(targetId, true);
    }
    if (!draggedPinned) {
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
      <input
        class="pms-input"
        bind:this={searchInput}
        placeholder={ui.t('common.search')}
        value={query}
        oninput={onSearchInput}
      />
      <!-- D-101：搜索词非空才渲染清空按钮（空态不占位）。 -->
      {#if query.length > 0}
        <button
          class="list-search-clear"
          type="button"
          aria-label={ui.t('common.clear_search')}
          title={ui.t('common.clear_search')}
          onclick={clearSearch}
        >
          <Icon name="close" size={14} />
        </button>
      {/if}
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
    <!-- D-158：空态要区分「库里没有」和「被搜索词筛掉了」。
         统一写「还没有条目」时，搜一个不存在的词会让人以为数据丢了（D-152 同型焦虑）。
         判据用 `vault.query`（**实际生效**的筛选词）而不是本地 `query`：空态解释的是
         「列表为什么是空的」，而列表是按 store 里的词过滤的。
         D-159：搜索词跨分组保留后，空列表**一半是「这组没东西」一半是「这组没有 j」**，
         所以还要把词本身写出来、并就地给一个清空出口——否则用户得先意识到「上面那个框里
         有字」才找得到 ×。 -->
    <div class="list-center list-empty">
      {#if vault.query.length > 0}
        <Icon name="search" size={26} />
        <p>{ui.t('item.empty_search', { q: vault.query })}</p>
        <button class="pms-btn is-ghost is-sm" type="button" onclick={clearSearch}>
          <Icon name="close" size={14} />{ui.t('common.clear_search')}
        </button>
      {:else}
        <Icon name="key" size={26} />
        <p>{ui.t('item.empty')}</p>
      {/if}
    </div>
  {:else}
    <ul class="list-items" bind:this={listEl}>
      {#each vault.items as item (item.item_id)}
        <!-- D-154：所属分组标签（三态：组名 / 未分组 / 未知分组），每行算一次复用两处。 -->
        {@const folderTag = folderLabelOf(item.folder_id, vault.folders)}
        <li class:has-pin={item.pinned === true}>
          <button
            class="list-row"
            class:has-pin={item.pinned === true}
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
              <!-- D-154：类型 · 所属分组。先类型后归属：扫列表时第一眼认类型，第二眼才问
                   「这条在哪个组」；组名可截断，类型不可（类型只有固定几个短词）。
                   D-160：组名只在全部 / 收藏（跨组）显示 —— 进了某个组，每行都是同一个
                   组名，那是噪音；组内视图里副行只留类型。 -->
              <span class="list-sub pms-meta">
                <span class="list-sub-type">{typeLabel(item)}</span>
                {#if showRowFolder}
                  <span class="list-sub-sep" aria-hidden="true">·</span>
                  <span
                    class="list-sub-folder"
                    class:is-none={folderTag.kind !== 'folder'}
                    title={folderText(folderTag)}
                  >{folderText(folderTag)}</span>
                {/if}
              </span>
            </span>
            <!-- #531：拖拽手柄的辅助说明（视觉隐藏，进可访问名称）。 -->
            <span class="sr-only">{ui.t('dnd.handle_item')}</span>
            {#if item.favorite}
              <span class="list-fav"><Icon name="star" size={14} filled /></span>
            {/if}
          </button>
          {#if item.pinned === true}
            <!-- D-85：点钉 = 直接取消钉住（掉回频度区是 D-84 定案的自然结果）。
                 必须是行按钮的**兄弟**节点（li 内绝对定位）：嵌套 button 是无效 HTML
                 （SSR 解析会被浏览器挪位）；兄弟节点也不吃行点击 / 行拖拽 / 行双击。 -->
            <button
              class="list-pin"
              type="button"
              title={ui.t('menu.unpin')}
              aria-label={ui.t('menu.unpin')}
              onclick={(): void => void vault.setItemPin(item.item_id, false)}
            >
              <Icon name="pin" size={13} />
            </button>
          {/if}
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
  /* D-101：清空按钮（input-group 尾部）。圆形 hitbox 22px，空态不渲染不占位。 */
  .list-search-clear {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--pms-faint);
    cursor: pointer;
    transition:
      color var(--pms-dur) var(--pms-ease),
      background var(--pms-dur) var(--pms-ease);
  }
  .list-search-clear:hover {
    color: var(--pms-danger);
    background: var(--pms-danger-soft);
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
  /* D-154：副行 = 类型 · 所属分组。整行可截断，但类型不参与截断
     （`flex: 0 0 auto`）：类型只有几个固定短词，被挤掉会让「组名」显得像类型。 */
  .list-sub {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    min-width: 0;
  }
  .list-sub-type {
    flex: 0 0 auto;
  }
  .list-sub-sep {
    flex: 0 0 auto;
    opacity: 0.55;
  }
  .list-sub-folder {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* 「未分组 / 未知分组」不是真组名：弱化但**不隐藏**——这行本身就是本轮要给的答案。 */
  .list-sub-folder.is-none {
    color: var(--pms-faint);
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
  /* #531：钉住标记（钉住块语义的可见线索）；D-85 起可点 = 直接取消钉住。
     钉住按钮是行按钮的**兄弟**节点（li 相对定位 + 绝对定位右缘）：不嵌套 button，
     也天然不触发行点击/拖拽/长按。has-pin 给行留出右缘空间（收藏星不被盖住）。 */
  .list-items li {
    position: relative;
  }
  .list-row.has-pin {
    padding-right: 2.3rem;
  }
  .list-pin {
    position: absolute;
    right: 0.45rem;
    top: 50%;
    transform: translateY(-50%);
    color: var(--pms-muted);
    display: flex;
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    cursor: pointer;
    transition: color var(--pms-dur) var(--pms-ease);
  }
  .list-pin:hover {
    color: var(--pms-fg);
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
