// 自动输入（Auto-Type）前端侧的纯逻辑（1.5.39）。
//
// 与 `quick-search.ts` 同一分工：能出错又难肉眼验证的部分抽出来，组件里剩下的
// 只有 DOM 接线。这里只有两件事——窗口判定与失败文案映射。
//
// 一条安全边界写在这里而不是藏进组件：**明文不经过前端**。
// 用户在浮层里选定条目后，前端只把 `item_id` 交给 `auto_type_fill`；
// 用户名与密码由 Rust 侧自取并注入目标窗口，全程不进任何 IPC 返回值。
// `src/lib/autotype-guard.test.ts` 用源码断言守这条线。

/** 自动输入浮层的窗口判定（与 `isQuickWindow` 同一形状）。
 *
 * 入口由 Rust 侧以 `index.html?w=autotype` 打开，所以判定只依赖 URL，
 * 不依赖窗口 label——分流必须发生在 `mount` 之前，而前端拿 label 要额外异步调用。
 */
export function isAutotypeWindow(search: string): boolean {
  return new URLSearchParams(search).get('w') === 'autotype';
}

/**
 * `auto_type_fill` 失败原因 → 文案键的**显式映射表**（D-185 铁律：禁止后端枚举现拼）。
 *
 * 后端的 wire 取值是 IPC 错误的 `code` 字段（`IpcErrorShape.code`），所以键写全码。
 * 映射不到的一律落 [`AUTOTYPE_FAILURE_FALLBACK_KEY`] 那句人话兜底，
 * 绝不回显键名、绝不模板拼接。
 */
export const AUTOTYPE_FAILURE_KEY_BY_CODE: Readonly<Record<string, string>> = {
  // 目标窗口已切换，注入失去意义（后端在注入前复核前台窗口所属）。
  ERR_AUTOTYPE_FOCUS_LOST: 'autotype.failed_focus',
  // 库已锁定 / 没有库。
  ERR_NOT_UNLOCKED: 'autotype.failed_not_unlocked',
  // 平台不具备注入能力。
  ERR_NOT_SUPPORTED: 'autotype.failed_not_supported',
};

/** 兜底文案键：映射表没收录的失败都用它，永远不显示键名。 */
export const AUTOTYPE_FAILURE_FALLBACK_KEY = 'autotype.failed';

/**
 * 从任意抛出物取 IPC 错误码（非该形态返回空串，由映射表兜底兜住）。
 *
 * 刻意不引 `toAppError`：浮层是独立 JS 上下文，少一层依赖就少一份解析成本；
 * 而且这里只需要 `code` 一个字段。
 */
export function autotypeCodeOf(err: unknown): string {
  if (typeof err === 'object' && err !== null && 'code' in err) {
    const code = (err as { code: unknown }).code;
    if (typeof code === 'string') {
      return code;
    }
  }
  return '';
}

/** 失败原因的文案键：映射表查得到就精确说，查不到就用人话兜底。 */
export function autotypeFailureKey(code: string): string {
  return AUTOTYPE_FAILURE_KEY_BY_CODE[code] ?? AUTOTYPE_FAILURE_FALLBACK_KEY;
}
