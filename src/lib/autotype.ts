// 自动输入（Auto-Type）前端侧的纯逻辑（1.5.39）。
//
// 与 `quick-search.ts` 同一分工：能出错又难肉眼验证的部分抽出来，组件里剩下的
// 只有 DOM 接线。这里只有三件事——窗口判定、候选行映射与失败文案映射。
//
// 一条安全边界写在这里而不是藏进组件：**`auto_type_fill` 的入参与返回不含明文；
// 浮层列表由 `autotype_candidates` 提供，只含标题、用户名、网址与使用次数**
// （没有密码、备注、TOTP 密钥）。明文只在 Rust 进程内从库里取出并注入目标窗口。
// `src/lib/autotype-guard.test.ts` 用源码断言守这条线——浮层源码里一旦出现
// 拉全量条目的调用即红。M3 的教训：早先浮层拉的是带 password 的全量列表，
// 那句「明文不经过前端」在当时是**假话**，修的是数据通路而不是措辞。

import type { RankableCandidate } from './quick-search';

/** 自动输入浮层的窗口判定（与 `isQuickWindow` 同一形状）。
 *
 * 入口由 Rust 侧以 `index.html?w=autotype` 打开，所以判定只依赖 URL，
 * 不依赖窗口 label——分流必须发生在 `mount` 之前，而前端拿 label 要额外异步调用。
 */
export function isAutotypeWindow(search: string): boolean {
  return new URLSearchParams(search).get('w') === 'autotype';
}

/**
 * 自动输入动作（1.5.39）。与 Rust `auto_type_fill` 的 wire 取值逐字一致。
 *
 * - `default`  用户名 + 密码（完整登录序列）
 * - `username` 只打用户名
 * - `password` 只打密码
 * - `url`      只打网址
 * - `totp`     只打动态口令
 *
 * 每种动作都是一个**整句**的语义，不是字符串拼装：后端按枚举分派，前端只传名字。
 */
export type AutotypeAction = 'default' | 'username' | 'password' | 'url' | 'totp';

/**
 * 自动输入候选行（`autotype_candidates` 的返回项，5 字段）。
 *
 * 列表要显示与排序需要什么就只给什么：标题、用户名、网址、使用次数、id。
 * 密码 / 备注 / TOTP 密钥一个都不给——浮层的 JS 堆里因此没有任何明文，
 * 这是 M3 修复后真正的边界。形状直接复用 `quick-search.ts` 的 `RankableCandidate`，
 * 排序走 `rankCandidates`——它与 `rankItems` 共用私有 `rankHits`（**同一套打分**，
 * 不是同一个函数），同一个条目在两个窗口的位置因此不会漂。
 */
export type AutotypeCandidate = RankableCandidate;

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
  // 选了某个动作，但这条目没填对应字段（1.5.39 修饰键直选）。
  ERR_AUTOTYPE_NO_USERNAME: 'autotype.no_username',
  ERR_AUTOTYPE_NO_URL: 'autotype.no_url',
  ERR_AUTOTYPE_NO_TOTP: 'autotype.no_totp',
  // 目标程序不接受模拟按键（UIPI / 提权窗口拦截）。
  ERR_AUTOTYPE_BLOCKED: 'autotype.failed_blocked',
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
