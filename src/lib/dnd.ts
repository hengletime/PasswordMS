// #531 拖拽排序 —— 纯逻辑层（可离屏单测，不碰 DOM / 指针事件）。
//
// 排序模型（客户拍板，唯一权威）：
//   钉住块（顶部，按 pin_order）→ 未钉住按频度（use_count DESC → last_used_at DESC → updated_at DESC）。
//   拖拽 = 钉住到该位置；已钉住互拖 = 调整钉住顺序；**收藏不再置顶**（星标仅过滤语义）。
//
// 列表顺序完全由后端返回序决定，前端**不再排序**；本模块只回答三个问题：
//   ① 这一下算不算拖（阈值 / 长按）；② 松手落在哪（目标行 + 前/后）；③ 钉住集合的新顺序是什么。

/** 指针位移超过该值（px）才算拖拽（区分点击）。 */
export const DRAG_START_THRESHOLD_PX = 6;
/** 触屏长按时长（ms）：按住不动达到该值进入「已武装」态（可拖 / 松手弹菜单）。 */
export const LONG_PRESS_MS = 220;
/** 自动滚动的边缘感应区（px）。 */
export const AUTOSCROLL_EDGE_PX = 48;
/** 自动滚动的每帧最大位移（px）。 */
export const AUTOSCROLL_MAX_STEP_PX = 14;

/**
 * 指针位移是否超过拖拽启动阈值（欧氏距离）。
 *
 * @param startX 按下时 X
 * @param startY 按下时 Y
 * @param x 当前 X
 * @param y 当前 Y
 * @param threshold 阈值（px），默认 {@link DRAG_START_THRESHOLD_PX}
 * @returns 是否超过阈值
 */
export function exceededDragThreshold(
  startX: number,
  startY: number,
  x: number,
  y: number,
  threshold: number = DRAG_START_THRESHOLD_PX,
): boolean {
  const dx = x - startX;
  const dy = y - startY;
  return Math.sqrt(dx * dx + dy * dy) > threshold;
}

/**
 * 距按下是否已满长按时长（触屏「长按不拖 = 弹菜单 / 长按后拖 = 拖拽」的判定）。
 *
 * @param elapsedMs 距按下经过的毫秒
 * @param thresholdMs 长按阈值，默认 {@link LONG_PRESS_MS}
 * @returns 是否已满
 */
export function isLongPress(elapsedMs: number, thresholdMs: number = LONG_PRESS_MS): boolean {
  return elapsedMs >= thresholdMs;
}

/**
 * 指针在行上的插入位置：落在行的**上半** = 插到该行之前，**下半** = 之后。
 *
 * @param y 指针 Y
 * @param rectTop 行顶
 * @param rectBottom 行底
 * @returns true = 插到该行之前
 */
export function insertBefore(y: number, rectTop: number, rectBottom: number): boolean {
  return y < (rectTop + rectBottom) / 2;
}

/** 行几何（拖拽投放判定用）。 */
export interface RowRect {
  id: string;
  top: number;
  bottom: number;
}

/** 投放目标：落在哪一行、行前还是行后。 */
export interface DropTarget {
  targetId: string;
  before: boolean;
}

/**
 * 在一组行几何里找指针所在的投放目标（线性扫，列表规模小，无需二分）。
 *
 * @param y 指针 Y
 * @param rows 行几何（须按视觉顺序排列）
 * @returns 目标；不在任何行上返回 null
 */
export function computeDropTarget(y: number, rows: readonly RowRect[]): DropTarget | null {
  for (const row of rows) {
    if (y >= row.top && y <= row.bottom) {
      return { targetId: row.id, before: insertBefore(y, row.top, row.bottom) };
    }
  }
  return null;
}

/**
 * 自动滚动位移：指针进入容器上下边缘感应区时给出本帧滚动量。
 *
 * @param pointerY 指针 Y（视口坐标）
 * @param containerTop 滚动容器顶（视口坐标）
 * @param containerBottom 滚动容器底（视口坐标）
 * @param bottomReserve 底部预留（px）——移动端底部导航（56px）+ 安全会遮挡容器底部，
 *   预留后指针到「视觉上的容器底」就会触发滚动，而不是被导航条压住的死区
 * @param edge 感应区厚度，默认 {@link AUTOSCROLL_EDGE_PX}
 * @param maxStep 每帧最大位移，默认 {@link AUTOSCROLL_MAX_STEP_PX}
 * @returns 滚动量（正 = 向下，负 = 向上，0 = 不滚）
 */
export function autoScrollDelta(
  pointerY: number,
  containerTop: number,
  containerBottom: number,
  bottomReserve: number = 0,
  edge: number = AUTOSCROLL_EDGE_PX,
  maxStep: number = AUTOSCROLL_MAX_STEP_PX,
): number {
  const usableBottom = containerBottom - bottomReserve;
  if (pointerY < containerTop + edge) {
    // 越深入感应区滚得越快（比例 0→1 映射到 maxStep），向上滚动。
    const depth = Math.min(Math.max(containerTop + edge - pointerY, 0), edge);
    const step = Math.max(1, Math.round((depth / edge) * maxStep));
    return -step;
  }
  if (pointerY > usableBottom - edge) {
    const depth = Math.min(Math.max(pointerY - (usableBottom - edge), 0), edge);
    return Math.max(1, Math.round((depth / edge) * maxStep));
  }
  return 0;
}

/**
 * 拖拽松手后钉住集合的新顺序。
 *
 * 语义（客户拍板「拖拽 = 钉住到该位置」）：
 * - 拖**已钉住**条目：调整钉住顺序（插入到目标位置；落到未钉区 = 挪到钉住块末尾）；
 * - 拖**未钉住**条目：只有落在**钉住块内**（目标是钉住条目）才钉住并插到该位置；
 *   落在未钉区 = 无动作（未钉区按频度排，频度不可手调）。
 *
 * @param currentPinnedIds 当前钉住集合（按 pin_order，即后端返回序）
 * @param draggedId 被拖条目
 * @param targetId 目标条目；null = 落到钉住块末尾（仅对已钉住条目有意义）
 * @param before 插到目标之前还是之后
 * @returns 新顺序；无需任何 IPC 调用时返回 null
 */
export function pinnedOrderAfterDrop(
  currentPinnedIds: readonly string[],
  draggedId: string,
  targetId: string | null,
  before: boolean,
): string[] | null {
  if (draggedId === targetId) {
    return null;
  }
  const draggedPinned = currentPinnedIds.includes(draggedId);
  // 未钉条目必须落在钉住块内（目标存在且已钉住）才钉；目标 null（拖出块外）= 不钉。
  if (!draggedPinned && (targetId === null || !currentPinnedIds.includes(targetId))) {
    return null;
  }
  // 新顺序 = 先摘掉被拖者，再插到目标位置。
  const without = currentPinnedIds.filter((id) => id !== draggedId);
  let index: number;
  if (targetId === null) {
    index = without.length;
  } else {
    const at = without.indexOf(targetId);
    if (at < 0) {
      // 目标不在钉住集合（只可能：目标未钉）——按无动作处理。
      return null;
    }
    index = before ? at : at + 1;
  }
  const next = [...without.slice(0, index), draggedId, ...without.slice(index)];
  // 顺序没变（拖回原位）→ 不调 IPC。
  const unchanged =
    next.length === currentPinnedIds.length && next.every((id, i) => id === currentPinnedIds[i]);
  return unchanged ? null : next;
}
