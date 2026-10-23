// #531 拖拽基建 —— 响应式状态 + 指针事件控制器（桌面 + 触屏统一，零依赖）。
//
// 为什么是 `.svelte.ts` 而不是规格里写的 `dnd.ts`：这里要持有**跨组件共享的响应式**
// 拖拽状态（ItemList 拖、FolderSidebar 高亮投放目标、浮动占位跟随指针），Svelte 5 的
// `$state` 只能在 `.svelte.ts` 里用。纯逻辑（阈值 / 插入位置 / 钉住顺序）仍在 `dnd.ts`
// —— 两个文件按「纯逻辑 / 有状态」分家，各自的测试面也分开。
//
// 触屏关键点：Android WebView 上 HTML5 DnD 不可靠 → 全程自写 pointer 事件；
// 拖拽激活后用 `touchmove` 的 `preventDefault`（non-passive 监听）压住滚动抢占
// —— 触屏「长按 ≈220ms 才武装」，此前手指不动、浏览器尚未开始滚动，此时再压滚动是安全的。

import {
  LONG_PRESS_MS,
  autoScrollDelta,
  exceededDragThreshold,
} from './dnd';

/** 可拖拽 / 可投放的对象种类。 */
export type DragKind = 'item' | 'folder';
/** 「全部条目」行的特殊投放 id（= 移到未分组）。 */
export const DROP_ID_ALL_ITEMS = '__all__';

/** 共享拖拽状态（浮动占位与目标行高亮都读它）。 */
export interface DragState {
  active: boolean;
  kind: DragKind | null;
  draggedId: string | null;
  /** 浮动占位文本（条目 / 分组标题）。 */
  label: string;
  x: number;
  y: number;
  /** 当前悬停的投放目标（null = 不在任何可投放行上）。 */
  overKind: DragKind | null;
  overId: string | null;
  overBefore: boolean;
}

function initialState(): DragState {
  return {
    active: false,
    kind: null,
    draggedId: null,
    label: '',
    x: 0,
    y: 0,
    overKind: null,
    overId: null,
    overBefore: false,
  };
}

/** 拖拽状态单例（ItemList 与 FolderSidebar 共享）。 */
class DragStore {
  state = $state<DragState>(initialState());

  begin(kind: DragKind, draggedId: string, label: string, x: number, y: number): void {
    this.state.active = true;
    this.state.kind = kind;
    this.state.draggedId = draggedId;
    this.state.label = label;
    this.state.x = x;
    this.state.y = y;
  }

  move(x: number, y: number): void {
    this.state.x = x;
    this.state.y = y;
  }

  setOver(overKind: DragKind | null, overId: string | null, overBefore: boolean): void {
    this.state.overKind = overKind;
    this.state.overId = overId;
    this.state.overBefore = overBefore;
  }

  end(): void {
    const fresh = initialState();
    this.state.active = fresh.active;
    this.state.kind = fresh.kind;
    this.state.draggedId = fresh.draggedId;
    this.state.label = fresh.label;
    this.state.x = fresh.x;
    this.state.y = fresh.y;
    this.state.overKind = fresh.overKind;
    this.state.overId = fresh.overId;
    this.state.overBefore = fresh.overBefore;
  }
}

export const drag = new DragStore();

/**
 * 松手结算：**先**把投放目标交给 `onDrop`，**再**清共享状态。
 * 顺序不可交换——D-83 教训：`onDrop` 在同步段读 `drag.state` 取投放目标
 * （`handleItemDrop` / `handleFolderDrop` 首行即 `drag.state`），若先 `end()`
 * 读到的永远是 null，一切投放静默 no-op（「有边缘动画但松手没反应」）。
 * `onDrop` 同步段只读 state + 发起异步 IPC，无重入风险；`end()` 随后清视觉状态。
 * 抽成独立函数以便对「结算时序」做防回归断言（dnd-settle.test.ts）。
 */
export function finishDrag(draggedId: string, onDrop: (id: string) => void): void {
  onDrop(draggedId);
  drag.end();
}

/** 拖拽期间压住触屏滚动（non-passive touchmove + preventDefault）。返回解除函数。 */
export function blockTouchScroll(): () => void {
  const handler = (event: TouchEvent): void => {
    event.preventDefault();
  };
  window.addEventListener('touchmove', handler, { passive: false });
  return (): void => {
    window.removeEventListener('touchmove', handler);
  };
}

/** 指针所在的可投放行（组件在可投放行上标 `data-drop-kind` / `data-drop-id`）。 */
export interface PointTarget {
  kind: DragKind;
  id: string;
  before: boolean;
}

/**
 * 视口坐标 → 投放目标。用 `elementFromPoint` + `closest('[data-drop-id]')`，
 * 因此**跨组件投放**（列表行拖到侧栏分组行）不需要两个组件互相知道对方的存在。
 * 浮动占位自身必须 `pointer-events: none`，否则它会挡住 elementFromPoint。
 *
 * @param x 视口 X
 * @param y 视口 Y
 * @returns 目标；不在任何可投放行上返回 null
 */
export function targetFromPoint(x: number, y: number): PointTarget | null {
  const elements = document.elementsFromPoint(x, y);
  for (const el of elements) {
    const drop = (el as HTMLElement).closest?.('[data-drop-id]') as HTMLElement | null;
    if (drop !== null) {
      const kind = drop.getAttribute('data-drop-kind');
      const id = drop.getAttribute('data-drop-id');
      if ((kind === 'item' || kind === 'folder') && id !== null) {
        const rect = drop.getBoundingClientRect();
        return { kind, id, before: y < rect.top + rect.height / 2 };
      }
      return null;
    }
  }
  return null;
}

/** 控制器回调（组件提供，控制器只管手势与状态）。 */
export interface DragControllerCallbacks {
  /** 拖拽浮层的显示文本。 */
  getLabel: (id: string) => string;
  /** 触屏「长按不拖」——弹上下文菜单。桌面右键走组件自己的 contextmenu 事件，不经这里。 */
  onLongPress: (x: number, y: number) => void;
  /** 松手（真的拖过才回调；点按不回调）。 */
  onDrop: (draggedId: string) => void;
  /** 自动滚动的容器（列表 / 侧栏的可滚祖先）；null = 不自动滚。 */
  getScrollContainer: () => HTMLElement | null;
  /** 容器底部预留（移动端底部导航 + 安全区遮挡）。 */
  bottomInset: () => number;
}

interface PendingDrag {
  id: string;
  startX: number;
  startY: number;
  startTs: number;
  touch: boolean;
  longPressTimer: ReturnType<typeof setTimeout> | null;
}

/**
 * 行 `pointerdown` 入口：按住阈值/长按语义驱动一次完整拖拽（或长按菜单）。
 * 内部自挂 window 级 pointermove / pointerup / pointercancel，松手自动清理。
 *
 * @param kind 拖拽对象种类
 * @param id 被拖对象 id
 * @param event 指针按下事件
 * @param callbacks 组件回调
 */
export function beginRowDrag(
  kind: DragKind,
  id: string,
  event: PointerEvent,
  callbacks: DragControllerCallbacks,
): void {
  if (event.button !== 0 && event.pointerType === 'mouse') {
    return;
  }
  const pending: PendingDrag = {
    id,
    startX: event.clientX,
    startY: event.clientY,
    startTs: Date.now(),
    touch: event.pointerType !== 'mouse',
    longPressTimer: null,
  };
  let dragging = false;
  let longPressArmed = false;
  let lastX = pending.startX;
  let lastY = pending.startY;
  let releaseTouchBlock: (() => void) | null = null;
  let scrollRaf: number | null = null;

  const cleanup = (): void => {
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
    window.removeEventListener('pointercancel', onCancel);
    if (pending.longPressTimer !== null) {
      clearTimeout(pending.longPressTimer);
      pending.longPressTimer = null;
    }
    releaseTouchBlock?.();
    releaseTouchBlock = null;
    if (scrollRaf !== null) {
      cancelAnimationFrame(scrollRaf);
      scrollRaf = null;
    }
  };

  const autoScrollTick = (): void => {
    if (!dragging) {
      scrollRaf = null;
      return;
    }
    const container = callbacks.getScrollContainer();
    if (container !== null) {
      const rect = container.getBoundingClientRect();
      const delta = autoScrollDelta(
        drag.state.y,
        rect.top,
        rect.bottom,
        callbacks.bottomInset(),
      );
      if (delta !== 0) {
        container.scrollTop += delta;
      }
    }
    scrollRaf = requestAnimationFrame(autoScrollTick);
  };

  const startDrag = (x: number, y: number): void => {
    dragging = true;
    drag.begin(kind, id, callbacks.getLabel(id), x, y);
    if (pending.touch) {
      releaseTouchBlock = blockTouchScroll();
    }
    scrollRaf = requestAnimationFrame(autoScrollTick);
  };

  const onMove = (ev: PointerEvent): void => {
    lastX = ev.clientX;
    lastY = ev.clientY;
    if (dragging) {
      ev.preventDefault();
      drag.move(ev.clientX, ev.clientY);
      const target = targetFromPoint(ev.clientX, ev.clientY);
      // 同类目标 = 排序 / 钉住；条目拖到分组行（含「全部条目」）= 跨分组移动。
      // 分组不能投放到条目行上（反向不成立）。
      if (
        target !== null &&
        (target.kind === kind || (kind === 'item' && target.kind === 'folder'))
      ) {
        drag.setOver(target.kind, target.id, target.before);
      } else {
        drag.setOver(null, null, false);
      }
      return;
    }
    const moved = exceededDragThreshold(pending.startX, pending.startY, ev.clientX, ev.clientY);
    if (pending.touch) {
      if (longPressArmed) {
        // 长按已武装 + 移动 = 进入拖拽。
        if (moved) {
          startDrag(ev.clientX, ev.clientY);
        }
        return;
      }
      if (moved) {
        // 没到长按时长就动了 = 用户在滚动列表，取消本次手势。
        cleanup();
      }
      return;
    }
    // 鼠标：超阈值即拖。
    if (moved) {
      startDrag(ev.clientX, ev.clientY);
    }
  };

  const finish = (): void => {
    const wasDragging = dragging;
    const wasArmed = longPressArmed;
    const at = { x: lastX, y: lastY };
    const draggedId = pending.id;
    cleanup();
    if (wasDragging) {
      // D-83：先 onDrop（同步段仍可读 drag.state 的投放目标），end() 最后清视觉状态。
      finishDrag(draggedId, callbacks.onDrop);
      return;
    }
    if (wasArmed && pending.touch) {
      // 长按不拖 = 上下文菜单（位置取指针最后已知处）。
      callbacks.onLongPress(at.x, at.y);
    }
  };

  const onUp = (ev: PointerEvent): void => {
    if (dragging) {
      drag.move(ev.clientX, ev.clientY);
    }
    finish();
  };

  const onCancel = (): void => {
    cleanup();
    drag.end();
  };

  window.addEventListener('pointermove', onMove, { passive: false });
  window.addEventListener('pointerup', onUp);
  window.addEventListener('pointercancel', onCancel);

  if (pending.touch) {
    // 触屏：长按武装（此前移动超阈值即取消 —— 那是滚动）。
    pending.longPressTimer = setTimeout((): void => {
      pending.longPressTimer = null;
      longPressArmed = true;
    }, LONG_PRESS_MS);
  }
}
