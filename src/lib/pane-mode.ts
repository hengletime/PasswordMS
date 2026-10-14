// 右侧面板状态机（D-31 的治本修复）。
//
// 事故回顾：MainLayout 曾经用两个独立字段（`editing` / `creatingKind`）表示右侧面板，
// 而 ItemList 点选条目只改 `vault.selectedItemId`、**不通知** MainLayout。于是点过「新建笔记」
// 后 `editing` 恒为 `true`，再点已保存条目右侧仍停在新建表单（客户报障 D-31）。
//
// 本模块把面板收敛成一个**纯 reducer**：唯一的进/出迁移都由事件驱动，其中
// 「列表选中」事件 `select` 无论当前处于 create 还是 edit，一律回到只读详情，
// 使「点条目 ⇒ 退出编辑态」在结构上**不可能被违反**，并可被 vitest 直接覆盖。
//
// 纯函数、无副作用、无外部依赖。

/** 右侧面板模式。 */
export type PaneMode =
  | { view: 'detail' }
  | { view: 'create'; kind: 'login' | 'secure_note' }
  | { view: 'edit' };

/** 驱动面板状态的事件。 */
export type PaneEvent =
  | { type: 'new'; kind: 'login' | 'secure_note' } // 点「新建登录 / 新建笔记」
  | { type: 'edit' } // 点「编辑」
  | { type: 'select' } // 用户在列表里点了任意条目 —— D-31 的关键事件
  | { type: 'done' } // 保存成功 / 取消
  | { type: 'section' }; // 切换顶栏分区

/** 只读详情（默认态）。 */
export const PANE_DETAIL: PaneMode = { view: 'detail' };

/**
 * 面板 reducer。
 * - `new`     → 进入新建（指定 kind）
 * - `edit`    → 进入编辑当前选中条目
 * - `select`  → **回到只读详情**（关键规则：点任意列表条目一律退出编辑/新建态）
 * - `done`    → 回到只读详情
 * - `section` → 回到只读详情（切换顶栏分区）
 *
 * 对未知事件保守返回当前态（不抛异常，保证 UI 不崩）。
 */
export function paneReducer(mode: PaneMode, event: PaneEvent): PaneMode {
  switch (event.type) {
    case 'new':
      return { view: 'create', kind: event.kind };
    case 'edit':
      return { view: 'edit' };
    case 'select':
    case 'done':
    case 'section':
      return PANE_DETAIL;
    default:
      return mode;
  }
}
