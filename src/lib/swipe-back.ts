// D-95 移动端「向左滑动返回」手势判定（纯函数状态机，无 DOM 依赖）。
//
// 需求（主人 2026-10-24）：手机上打开条目后返回要点左上角 ←，改为在详情页上
// 向左滑动即可返回（手指向左 = 详情内容跟手往左退 = 回到左边的列表）。
//
// 设计约束：
// - 仅触屏（pointerType === 'touch'）且主指针（isPrimary）——桌面鼠标/触控板不启用，
//   天然免疫桌面误触（鼠标拖动不构成手势）；
// - 水平主导：|dx| ≥ 垂直位移 × VERTICAL_RATIO——避免和列表/正文的垂直滚动打架
//   （垂直滚动时浏览器接管指针，dy 大，判定不成立）；
// - 一次手势最多触发一次（triggered 锁定），防止滑动过头重复返回；
// - 多指：只跟 isPrimary 的那个 pointerId，第二根手指的事件直接忽略；
// - 启用范围由调用方控制（MainLayout：仅详情查看态；编辑/新建态不启用——
//   未保存的表单内容不能被一个手势丢掉）。

/** 手势成立所需的最小水平位移（px）。约 1/5 屏宽，误触率低又不用滑很远。 */
export const SWIPE_BACK_THRESHOLD_PX = 72;
/** 水平位移必须 ≥ 垂直位移的该倍数，才算「水平主导」。 */
export const SWIPE_BACK_VERTICAL_RATIO = 2;

/** 一次手势的进行中状态；pointerup / pointercancel 后回到 null。 */
export interface SwipeBackState {
  pointerId: number;
  startX: number;
  startY: number;
  /** 已触发过返回（锁定，防重复）。 */
  triggered: boolean;
}

/**
 * pointerdown：决定是否开始一次候选手势。
 *
 * @param pointerType  PointerEvent.pointerType（'touch' / 'mouse' / 'pen'）
 * @param isPrimary    PointerEvent.isPrimary（多指时只有第一指为 true）
 * @param enabled      调用方判定的启用条件（如「详情查看态」）
 * @returns 新的手势状态；不启用时返回 null（后续 move/up 事件全部忽略）。
 */
export function swipeBackBegin(
  pointerType: string,
  isPrimary: boolean,
  enabled: boolean,
  pointerId: number,
  x: number,
  y: number,
): SwipeBackState | null {
  if (!enabled || pointerType !== 'touch' || !isPrimary) {
    return null;
  }
  return { pointerId, startX: x, startY: y, triggered: false };
}

/**
 * pointermove：更新手势并判定是否成立。
 *
 * @returns 本次调用是否触发返回（true 只会出现一次；后续 move 继续返回 false）。
 *          state 为 null（未启用的手势）时恒返回 false。
 */
export function swipeBackUpdate(
  state: SwipeBackState | null,
  pointerId: number,
  x: number,
  y: number,
): boolean {
  if (state === null || state.triggered || pointerId !== state.pointerId) {
    return false;
  }
  const dx = x - state.startX;
  const dy = y - state.startY;
  const horizontalLead = Math.abs(dx) >= Math.abs(dy) * SWIPE_BACK_VERTICAL_RATIO;
  if (dx <= -SWIPE_BACK_THRESHOLD_PX && horizontalLead) {
    state.triggered = true;
    return true;
  }
  return false;
}
