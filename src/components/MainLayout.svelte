<script lang="ts">
  // 主界面骨架（PRD F-03/F-04）：顶栏 + 分区导航 + 三栏内容区。
  // 右侧面板状态由 `paneReducer` 统一驱动（D-31）：点列表条目 ⇒ `select` ⇒ 退出编辑态。
  //
  // 响应式布局（D-35，客户拍板「方案 A：单栏推进 + 底部导航」）：
  //   · ≥768px（默认）：完全沿用三栏布局，一个像素都不动。
  //   · <768px（移动端）：顶栏收敛为 [☰/←] + 分区标题 + [⋯]；5 个 tab 移到固定底部导航栏；
  //     分组栏收进可滑出的抽屉；列表与详情两栏同屏只显示一栏。
  //   单栏切换以 `vault.selectedItemId` 为**唯一真相**（`editing` 亦派生自单一 `pane` reducer），
  //   与 D-31 的教训一致：同一件事只能有一个状态源。
  import Icon from './common/Icon.svelte';
  import Logo from './common/Logo.svelte';
  import WindowControls from './common/WindowControls.svelte';
  import FolderSidebar from './FolderSidebar.svelte';
  import HealthCheck from './HealthCheck.svelte';
  import ItemDetail from './ItemDetail.svelte';
  import ItemEditor from './ItemEditor.svelte';
  import ItemList from './ItemList.svelte';
  import Settings from './Settings.svelte';
  import Trash from './Trash.svelte';
  import ConflictPanel from './sync/ConflictPanel.svelte';
  import DeviceList from './sync/DeviceList.svelte';
  import PairScan from './sync/PairScan.svelte';
  import PairShow from './sync/PairShow.svelte';
  import SyncStatus from './sync/SyncStatus.svelte';
  import PasswordGeneratorModal from './PasswordGeneratorModal.svelte';
  import { createAutoLock } from '../lib/auto-lock';
  import { api } from '../lib/ipc';
  import { session } from '../lib/stores/session.svelte';
  import { settings } from '../lib/stores/settings.svelte';
  import { sync } from '../lib/stores/sync.svelte';
  import { ui, type MainSection } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { PANE_DETAIL, paneReducer, type PaneEvent, type PaneMode } from '../lib/pane-mode';
  import { swipeBackBegin, swipeBackUpdate, type SwipeBackState } from '../lib/swipe-back';

  type SyncTab = 'pair' | 'devices' | 'status' | 'conflicts';

  // 右侧面板状态机（D-31）：单一 `pane`，`select` 事件保证点条目即退出编辑/新建态。
  let pane = $state<PaneMode>(PANE_DETAIL);
  const editing = $derived(pane.view !== 'detail');
  let syncTab = $state<SyncTab>('pair');
  // D-58b：用户是否手动点过同步页 tab。点过即永久尊重用户选择，
  // 之后进入同步页不再按配对状态调整初始标签（不做持久化）。
  let syncTabTouched = $state(false);
  // 顶栏「生成密码」入口（客户反馈：生成器原先只藏在密码框的图标里，根本发现不了）。
  // 这里不带 onuse —— 只生成 + 复制，不写回任何条目。
  let genOpen = $state(false);
  // 移动端状态：分组抽屉开合、顶栏「⋯」溢出菜单开合。
  let drawerOpen = $state(false);
  let menuOpen = $state(false);
  // 「⋯」菜单容器：用于判定点击是否落在菜单外（点外部关闭）。
  let menuEl = $state<HTMLDivElement | null>(null);
  // D-61：窗口变 hidden 的时刻（visibilitychange；回前台补判用，null = 未在后台）。
  let hiddenAt = $state<number | null>(null);

  // 分区清单。`label` = 桌面顶栏与移动端标题用（完整文案）；`tab` = 移动端底部导航用（短标签）。
  const NAV: { key: MainSection; icon: string; label: string; tab: string }[] = [
    { key: 'items', icon: 'key', label: 'nav.items', tab: 'nav.tab_items' },
    { key: 'health', icon: 'shield', label: 'nav.health', tab: 'nav.tab_health' },
    { key: 'trash', icon: 'trash', label: 'nav.trash', tab: 'nav.tab_trash' },
    { key: 'sync', icon: 'sync', label: 'nav.sync', tab: 'nav.tab_sync' },
    { key: 'settings', icon: 'settings', label: 'nav.settings', tab: 'nav.tab_settings' },
  ];

  // 解锁后加载数据（会话状态由 App 驱动）。
  //
  // **D-133（v1.4.16）**：原实现在这个 `.then` 上挂自动同步调度器的 `start()`，
  // 自动同步已随本次回退删除（理由见 `docs/02-arch.md` §3.21 的回退说明）。
  $effect((): void => {
    if (session.status.locked) {
      return;
    }
    void loadAfterUnlock();
  });

  /**
   * D-108（豆包审查 3.5-10）：解锁后五路并行加载失败不再各自弹 toast 堆叠——
   * 全部走静默模式 + `allSettled`，任一失败立即整组重试一次（解锁瞬间的 IPC
   * 抖动多为瞬态），仍失败才合并弹一条提示。
   */
  async function loadAfterUnlock(): Promise<void> {
    const run = (): Promise<PromiseSettledResult<unknown>[]> =>
      Promise.allSettled([
        vault.load(true),
        settings.load(true),
        sync.loadStatus(true),
        sync.loadDevices(true),
        sync.loadConflicts(true),
      ]);
    let results = await run();
    if (results.every((r) => r.status === 'fulfilled')) {
      return;
    }
    results = await run();
    if (results.some((r) => r.status === 'rejected')) {
      ui.toast('error', 'err.load_partial');
    }
  }

  // ===== D-61 自动锁定（A 案：闲置达档位即锁，无「从不」档）=====
  // 本组件只在 main 路由挂载（App.svelte 条件渲染）——锁定/切页即卸载，
  // 卸载 cleanup 即 `stop()`；再次解锁重新挂载 → `start()` 重读档位。
  const autoLock = createAutoLock({
    getMinutes: async (): Promise<number> => api.vault.getAutoLock(),
    lock: async (): Promise<void> => {
      // 统一走既有 lockNow（reset 三个 store + 切锁屏路由）；IPC 失败不得让
      // 未处理 rejection 冒泡（自动锁定失败静默，用户仍在主界面可手动锁）。
      try {
        await lockNow();
      } catch {
        // 静默：锁定失败不弹错（错弹本身也会打断用户）。
      }
    },
    now: (): number => Date.now(),
    schedule: (fn, ms): (() => void) => {
      const id = window.setTimeout(fn, ms);
      return (): void => {
        window.clearTimeout(id);
      };
    },
  });

  $effect((): (() => void) => {
    void autoLock.start();
    const onActivity = (): void => {
      autoLock.touch();
    };
    const onVisibility = (): void => {
      if (document.hidden) {
        hiddenAt = Date.now();
      } else if (hiddenAt !== null) {
        const at = hiddenAt;
        hiddenAt = null;
        // 回前台补判（Android 后台冻结计时器）；失败不冒泡。
        void autoLock.onReturnFromBackground(at).catch((): void => {});
      }
    };
    window.addEventListener('pointerdown', onActivity, { passive: true });
    window.addEventListener('keydown', onActivity, { passive: true });
    document.addEventListener('visibilitychange', onVisibility);
    return (): void => {
      autoLock.stop();
      window.removeEventListener('pointerdown', onActivity);
      window.removeEventListener('keydown', onActivity);
      document.removeEventListener('visibilitychange', onVisibility);
    };
  });

  function dispatch(event: PaneEvent): void {
    pane = paneReducer(pane, event);
  }

  function startCreate(kind: 'login' | 'secure_note'): void {
    dispatch({ type: 'new', kind });
  }

  function startEdit(): void {
    dispatch({ type: 'edit' });
  }

  function finishEdit(): void {
    dispatch({ type: 'done' });
  }

  // D-95 移动端「向左滑动返回」：仅详情查看态启用。编辑/新建态**刻意不启用**——
  // 未保存的表单内容不能被一个手势丢掉（取消按钮语义保留）。判定逻辑在
  // src/lib/swipe-back.ts（纯函数可单测）：单指触屏、水平向左 ≥72px 且水平主导
  // （|dx| ≥ 2|dy|，不与垂直滚动打架）、一次手势只触发一次。桌面鼠标被
  // pointerType 过滤天然免疫；触发即调既有 goBack()，不新增导航状态（D-31）。
  // （启用条件的 $derived 在下方 inItems / editing 声明之后：swipeBackEnabled。）
  let swipeState: SwipeBackState | null = null;

  function onDetailPointerDown(event: PointerEvent): void {
    swipeState = swipeBackBegin(
      event.pointerType,
      event.isPrimary,
      swipeBackEnabled,
      event.pointerId,
      event.clientX,
      event.clientY,
    );
  }

  function onDetailPointerMove(event: PointerEvent): void {
    // swipeBackEnabled 二次判定：down 之后状态可能已变（如切进编辑态）。
    if (swipeBackUpdate(swipeState, event.pointerId, event.clientX, event.clientY) && swipeBackEnabled) {
      goBack();
    }
  }

  function onDetailPointerEnd(): void {
    swipeState = null;
  }

  /** 切换分区（桌面顶栏 / 移动端底部导航共用；行为完全一致）。 */
  function pickSection(key: MainSection): void {
    ui.setSection(key);
    // D-58b：进入同步页且用户从未手动选过 tab 时，按当前配对状态决定初始标签
    // （已有配对设备 → 优先「状态」；无配对设备 → 优先「配对」）。
    if (key === 'sync' && !syncTabTouched) {
      syncTab = sync.devices.length > 0 ? 'status' : 'pair';
    }
    dispatch({ type: 'section' });
  }

  /** D-58b：手动切换同步页 tab（标记 touched，之后不再自动劫持初始标签）。 */
  function pickSyncTab(tab: SyncTab): void {
    syncTabTouched = true;
    syncTab = tab;
  }

  async function lockNow(): Promise<void> {
    // D-91（审查 O3）：后端锁定失败（如锁中毒）时不得 reset store / 切锁屏——
    // 否则界面显示锁屏但后端会话仍 Unlocked、明文仍在内存。保持主界面，错误已 toast。
    const locked = await session.lock();
    if (!locked) {
      return;
    }
    vault.reset();
    settings.reset();
    sync.reset();
    ui.setRoute('lock');
  }

  /** 点击「⋯」菜单外部即关闭（菜单按钮本身也在容器内，故不会被误关）。 */
  function onWindowClick(event: MouseEvent): void {
    if (menuOpen && menuEl !== null && !menuEl.contains(event.target as Node)) {
      menuOpen = false;
    }
  }

  /** 移动端左侧按钮：列表态 = 打开分组抽屉；详情/编辑态 = 返回上一屏。 */
  function onLeadClick(): void {
    if (inItems && !listState) {
      goBack();
    } else {
      drawerOpen = true;
    }
  }

  /**
   * 单栏「返回」：编辑/新建态 → 回到只读详情（新建态因 selectedItemId 为 null 自然落到列表）；
   * 只读详情态 → 清空选中项回到列表。
   */
  function goBack(): void {
    if (editing) {
      finishEdit();
    } else if (vault.selectedItemId !== null) {
      vault.selectItem(null);
    }
  }

  // —— 移动端派生状态（全部由既有单一状态源推导，不新增任何布尔真相）——
  const inItems = $derived(ui.section === 'items');
  // 列表态：在条目分区、未选中条目、且未处于编辑/新建态。
  const listState = $derived(inItems && vault.selectedItemId === null && !editing);
  // 详情/编辑态：在条目分区但不在列表态 → 显示返回箭头。
  const showBack = $derived(inItems && !listState);
  // 移动端顶栏标题：当前分区名（桌面端仍显示 app.title）。
  const sectionLabel = $derived(NAV.find((e): boolean => e.key === ui.section)?.label ?? 'nav.items');
  // 单栏切换判据：有选中条目或处于编辑态即「详情屏」。
  const showingDetail = $derived(vault.selectedItemId !== null || editing);
  // D-95 滑动返回启用条件：详情**查看**态（编辑/新建态手势关闭，防丢未保存内容）。
  const swipeBackEnabled = $derived(inItems && !editing && vault.selectedItemId !== null);

  let detailKind = $derived(
    pane.view === 'create'
      ? pane.kind
      : vault.selectedItem?.kind.type === 'secure_note'
        ? 'secure_note'
        : 'login',
  );
</script>

<svelte:window onclick={onWindowClick} />

<div class="layout">
  <!-- 去掉原生标题栏后，顶栏兼作拖动区（拖空白处移动窗口、双击最大化）；
       data-tauri-drag-region 只在"点中的正是带该属性的元素"时生效，
       所以里面的按钮/输入框不会被误判成拖动。 -->
  <header class="topbar" data-tauri-drag-region>
    {#if inItems}
      <button
        class="topbar-lead pms-icon-btn"
        type="button"
        aria-label={showBack ? ui.t('common.back') : ui.t('nav.open_folders')}
        onclick={onLeadClick}
      >
        <Icon name={showBack ? 'arrow_left' : 'menu'} size={20} />
      </button>
    {/if}

    <div class="topbar-brand">
      <span class="topbar-logo"><Logo size={28} /></span>
      <span class="topbar-title tb-title-desktop">{ui.t('app.title')}</span>
      <span class="topbar-title tb-title-mobile">{ui.t(sectionLabel)}</span>
    </div>

    <nav class="topbar-nav">
      {#each NAV as entry (entry.key)}
        <button
          class="topbar-tab"
          class:active={ui.section === entry.key}
          type="button"
          onclick={() => pickSection(entry.key)}
        >
          <Icon name={entry.icon} size={16} />
          <span>{ui.t(entry.label)}</span>
        </button>
      {/each}
    </nav>

    <!-- 右区只放高频动作（D-38 信息架构）：生成器 / 锁定。
         低频项（语言 / 主题 / 导入）已移入「设置」页分组，不再挤顶栏。 -->
    <div class="topbar-right">
      <button
        class="pms-btn is-ghost is-sm"
        type="button"
        title={ui.t('gen.title')}
        onclick={() => (genOpen = true)}
      >
        <Icon name="dice" size={15} /><span class="btn-text">{ui.t('gen.title')}</span>
      </button>
      <button class="pms-btn is-ghost is-sm" type="button" title={ui.t('nav.lock')} onclick={() => void lockNow()}>
        <Icon name="lock" size={15} /><span class="btn-text">{ui.t('nav.lock')}</span>
      </button>
    </div>

    <!-- 移动端（<768px）：把上面 2 个按钮收进一个「⋯」溢出菜单（桌面端隐藏）。 -->
    <div class="topbar-more-wrap" bind:this={menuEl}>
      <button
        class="pms-icon-btn topbar-more"
        type="button"
        aria-label={ui.t('nav.more')}
        aria-expanded={menuOpen}
        onclick={() => (menuOpen = !menuOpen)}
      >
        <Icon name="more" size={20} />
      </button>
      {#if menuOpen}
        <div class="topbar-more-menu">
          <button
            class="pms-btn is-ghost is-sm menu-item"
            type="button"
            onclick={() => {
              genOpen = true;
              menuOpen = false;
            }}
          >
            <Icon name="dice" size={15} />{ui.t('gen.title')}
          </button>
          <button
            class="pms-btn is-ghost is-sm menu-item"
            type="button"
            onclick={() => {
              menuOpen = false;
              void lockNow();
            }}
          >
            <Icon name="lock" size={15} />{ui.t('nav.lock')}
          </button>
        </div>
      {/if}
    </div>

    <!-- 自绘窗口控制钮：永远挂在最右，绝不进 .topbar-right（那条在 <768px 会被隐藏，
         而桌面窗口缩窄时恰恰最需要关闭钮）。安卓上由组件自己不渲染。 -->
    <WindowControls />
  </header>

  <main class="body" class:showing-detail={showingDetail}>
    {#if ui.section === 'items'}
      <!-- 分组抽屉（移动端）：遮罩 + 可滑出的侧栏。桌面端 `.drawer` 为 display:contents，包裹层透明。 -->
      <button
        class="drawer-backdrop"
        class:open={drawerOpen}
        type="button"
        aria-label={ui.t('common.close')}
        tabindex={-1}
        onclick={() => (drawerOpen = false)}
      ></button>
      <div class="drawer" class:open={drawerOpen}>
        <FolderSidebar onpicked={() => (drawerOpen = false)} />
      </div>

      <!-- 单栏切换：桌面端两个 pane 均为 display:contents（=原来的三栏）；移动端同屏只显示一个。 -->
      <div class="pane pane-list">
        <ItemList onnew={startCreate} onselect={() => dispatch({ type: 'select' })} />
      </div>
      <div
        class="pane pane-detail"
        role="presentation"
        onpointerdown={onDetailPointerDown}
        onpointermove={onDetailPointerMove}
        onpointerup={onDetailPointerEnd}
        onpointercancel={onDetailPointerEnd}
      >
        {#if editing && (pane.view === 'create' || vault.selectedItem !== null)}
          <ItemEditor
            item={pane.view === 'edit' ? vault.selectedItem : null}
            kind={detailKind}
            ondone={finishEdit}
            oncancel={finishEdit}
          />
        {:else}
          <ItemDetail onedit={startEdit} />
        {/if}
      </div>
    {:else if ui.section === 'health'}
      <HealthCheck />
    {:else if ui.section === 'trash'}
      <Trash />
    {:else if ui.section === 'settings'}
      <Settings />
    {:else}
      <section class="sync-view">
        <nav class="sync-tabs">
          <button
            class="sync-tab"
            class:active={syncTab === 'pair'}
            type="button"
            onclick={() => pickSyncTab('pair')}
          >
            {ui.t('sync.tab_pair')}
          </button>
          <button
            class="sync-tab"
            class:active={syncTab === 'devices'}
            type="button"
            onclick={() => pickSyncTab('devices')}
          >
            {ui.t('sync.tab_devices')}
          </button>
          <button
            class="sync-tab"
            class:active={syncTab === 'status'}
            type="button"
            onclick={() => pickSyncTab('status')}
          >
            {ui.t('sync.tab_status')}
          </button>
          <button
            class="sync-tab"
            class:active={syncTab === 'conflicts'}
            type="button"
            onclick={() => pickSyncTab('conflicts')}
          >
            {ui.t('sync.tab_conflicts')}
          </button>
        </nav>

        {#if syncTab === 'pair'}
          <div class="sync-grid">
            <PairShow />
            <PairScan />
          </div>
        {:else if syncTab === 'devices'}
          <DeviceList />
        {:else if syncTab === 'status'}
          <SyncStatus />
        {:else}
          <ConflictPanel />
        {/if}
      </section>
    {/if}
  </main>

  <!-- 移动端底部导航（桌面端 display:none）：5 个 tab 与顶栏 NAV 一一对应。 -->
  <nav class="bottom-nav">
    {#each NAV as entry (entry.key)}
      <button
        class="bottom-nav-tab"
        class:active={ui.section === entry.key}
        type="button"
        onclick={() => pickSection(entry.key)}
      >
        <!-- 图标单独包一层 .tab-glyph：选中态只让这颗胶囊亮，不给整格铺色 -->
        <span class="tab-glyph"><Icon name={entry.icon} size={20} /></span>
        <span class="tab-label">{ui.t(entry.tab)}</span>
      </button>
    {/each}
  </nav>

  <!-- 顶栏入口：只生成 + 复制，不传 onuse（不写回条目） -->
  <PasswordGeneratorModal open={genOpen} onclose={() => (genOpen = false)} />
</div>

<style>
  .layout {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 0.85rem;
    min-height: 56px;
    border-bottom: 1px solid var(--pms-border);
    background: var(--pms-surface);
  }
  .topbar-brand {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  /* D-37：顶栏一律不许换行。英文文案比中文长一大截（Health check / Password generator），
     之前 "All items" 在 1100px 窗口被折成两行、还把整页撑出横向滚动条 —— 根因就是这里没锁 nowrap。 */
  .topbar-brand,
  .topbar-tab,
  .topbar-right .pms-btn {
    white-space: nowrap;
  }
  /* 品牌标本体自带渐变方牌（Logo.svelte），这里只负责占位对齐，不再叠自己的底色 */
  .topbar-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
  }
  .topbar-title {
    font-size: 1rem;
    font-weight: 600;
  }
  .topbar-nav {
    display: flex;
    gap: 0.2rem;
  }
  .topbar-tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    min-height: 40px;
    padding: 0 0.7rem;
    border: none;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-muted);
    cursor: pointer;
    font-size: 0.84rem;
    font-family: inherit;
    transition: background var(--pms-dur) var(--pms-ease), color var(--pms-dur) var(--pms-ease);
  }
  .topbar-tab:hover {
    background: var(--pms-bg-2);
    color: var(--pms-fg);
  }
  .topbar-tab.active {
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .topbar-tab.active::after {
    content: '';
    position: absolute;
    left: 0.4rem;
    right: 0.4rem;
    bottom: -0.5rem;
    height: 2px;
    border-radius: var(--pms-r-pill);
    background: var(--pms-accent);
  }
  .topbar-right {
    margin-left: auto;
    display: flex;
    gap: 0.35rem;
  }
  /* 移动端专属控件：桌面端一律隐藏，保证 ≥768px 布局与改动前逐像素一致。 */
  .topbar-lead,
  .tb-title-mobile,
  .topbar-more-wrap,
  .topbar-more,
  .bottom-nav {
    display: none;
  }
  /* 「⋯」溢出菜单面板。 */
  .topbar-more-wrap {
    position: relative;
    margin-left: auto;
  }
  .topbar-more-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    min-width: 250px;
    z-index: 70;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.4rem;
    background: var(--pms-elevated);
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    box-shadow: var(--pms-shadow);
  }
  .menu-item {
    width: 100%;
    justify-content: flex-start;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
    background: var(--pms-bg);
  }
  /* 桌面端：抽屉包裹层与单栏 pane 均为 display:contents —— 布局上「透明」，
     三栏（240 / 300 / flex）与改动前完全一致。 */
  .drawer {
    display: contents;
  }
  .drawer-backdrop {
    display: none;
  }
  .pane {
    display: contents;
  }
  .sync-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: auto;
  }
  .sync-tabs {
    display: flex;
    gap: 0.3rem;
    padding: 0.7rem 0.85rem;
    border-bottom: 1px solid var(--pms-border);
  }
  .sync-tab {
    min-height: 34px;
    padding: 0 0.8rem;
    border: 1px solid var(--pms-border-strong);
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-muted);
    cursor: pointer;
    font-size: 0.82rem;
    font-family: inherit;
    transition: background var(--pms-dur) var(--pms-ease), border-color var(--pms-dur) var(--pms-ease),
      color var(--pms-dur) var(--pms-ease);
  }
  .sync-tab:hover {
    background: var(--pms-bg-2);
  }
  .sync-tab.active {
    border-color: var(--pms-accent);
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .sync-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 1rem;
    padding: 1rem;
  }

  /* ============================================================
   * 移动端布局（<768px）—— 客户方案 A：单栏推进 + 底部导航。
   * ============================================================ */
  @media (max-width: 767px) {
    /* —— 顶栏：隐藏桌面导航 + 右区，露出左侧按钮与「⋯」。 —— */
    .topbar {
      gap: 0.4rem;
      padding: 0.5rem 0.6rem;
    }
    .topbar-nav,
    .topbar-right {
      display: none;
    }
    /* 顶栏 tab 本已随父级 .topbar-nav 隐藏；再显式给一条 display:none，
       便于自动化断言直接读 .topbar-tab 的 computed style（避免依赖父级链）。 */
    .topbar-tab {
      display: none;
    }
    .topbar-lead {
      display: inline-flex;
    }
    .tb-title-desktop {
      display: none;
    }
    .tb-title-mobile {
      display: inline;
    }
    .topbar-more-wrap {
      display: block;
    }
    .topbar-more {
      display: inline-flex;
    }
    .topbar-title {
      font-size: 0.95rem;
    }

    /* —— 内容区：给固定底栏预留高度（含安卓手势条 safe-area）。 —— */
    .body {
      padding-bottom: calc(56px + env(safe-area-inset-bottom));
    }

    /* —— 分组抽屉：固定层滑出。 —— */
    .drawer {
      display: block;
      position: fixed;
      top: 0;
      bottom: 0;
      left: 0;
      width: min(84vw, 300px);
      z-index: 60;
      transform: translateX(-100%);
      transition: transform var(--pms-dur) var(--pms-ease);
      background: var(--pms-bg-2);
      box-shadow: var(--pms-shadow);
    }
    .drawer.open {
      transform: none;
    }
    .drawer-backdrop {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 55;
      margin: 0;
      padding: 0;
      border: none;
      background: rgba(0, 0, 0, 0.5);
      opacity: 0;
      visibility: hidden;
      pointer-events: none;
      transition: opacity var(--pms-dur) var(--pms-ease), visibility var(--pms-dur) var(--pms-ease);
    }
    .drawer-backdrop.open {
      opacity: 1;
      visibility: visible;
      pointer-events: auto;
    }

    /* —— 单栏切换：同屏只显示列表或详情（`.body` 保持横向 flex）。 —— */
    .pane {
      display: flex;
      flex: 1;
      min-width: 0;
    }
    .body:not(.showing-detail) .pane-detail {
      display: none;
    }
    .body.showing-detail .pane-list {
      display: none;
    }

    /* —— 同步页双列网格在 390px 下会溢出（minmax(300px)），改单列。 —— */
    .sync-grid {
      grid-template-columns: 1fr;
    }

    /* —— 底部导航栏：固定、5 等分、图标在上文字在下。 —— */
    .bottom-nav {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      z-index: 50;
      display: flex;
      height: calc(56px + env(safe-area-inset-bottom));
      padding-bottom: env(safe-area-inset-bottom);
      background: var(--pms-surface);
      border-top: 1px solid var(--pms-border);
    }
    .bottom-nav-tab {
      flex: 1;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 0.15rem;
      padding: 0.3rem 0.1rem;
      border: none;
      background: transparent;
      color: var(--pms-muted);
      cursor: pointer;
      font-family: inherit;
      font-size: 0.68rem;
      transition: color var(--pms-dur) var(--pms-ease), background var(--pms-dur) var(--pms-ease);
    }
    .bottom-nav-tab:hover {
      color: var(--pms-fg);
    }
    /* 选中态：不再给整格铺底色（原来那块方块太重，像"被框住"），
       改成「图标背后亮起一颗胶囊 + 一圈柔光 + 轻微上浮」——
       底栏始终是一条干净的横线，视线却自然落到当前分区上。 */
    .bottom-nav-tab.active {
      color: var(--pms-accent);
      background: transparent;
    }
    .tab-glyph {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 46px;
      height: 26px;
      border-radius: var(--pms-r-pill);
      transition: background var(--pms-dur) var(--pms-ease),
        box-shadow var(--pms-dur) var(--pms-ease),
        transform var(--pms-dur) var(--pms-ease);
    }
    .bottom-nav-tab:hover .tab-glyph {
      background: var(--pms-bg-2);
    }
    .bottom-nav-tab.active .tab-glyph {
      background: var(--pms-accent-soft);
      /* 柔光：浅色下是淡青晕，暗色下是青绿辉光 */
      box-shadow: 0 0 16px -6px var(--pms-accent);
      transform: translateY(-1px);
    }
    /* 只给文字做省略号，胶囊那层不能 overflow:hidden（会裁掉柔光） */
    .tab-label {
      max-width: 100%;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  /* ================= 桌面收缩档（D-37/D-38，≥768px 才有） =================
     右区只留「生成器 / 锁定」两个高频钮（低频项已移入设置页），仍需分档防英文长文案撑爆：
     900–1199：tab 全文字，右区按钮图标化（title 悬停提示仍在）。
     768–899 ：tab 也图标化。「⋯」菜单从桌面收缩序列中退役，仅移动端（<768px）使用。 */

  @media (min-width: 900px) and (max-width: 1199px) {
    .topbar-nav {
      gap: 0.1rem;
    }
    .topbar-tab {
      padding: 0 0.55rem;
    }
    .topbar-right .btn-text {
      display: none;
    }
  }

  @media (min-width: 768px) and (max-width: 899px) {
    .topbar-nav {
      gap: 0.1rem;
    }
    .topbar-tab {
      padding: 0 0.55rem;
    }
    .topbar-tab span {
      display: none;
    }
    .topbar-right .btn-text {
      display: none;
    }
  }
</style>
