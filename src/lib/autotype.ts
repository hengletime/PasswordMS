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
 * 键位判据用到的字段（结构型，不绑 `KeyboardEvent`：单测里直接传字面量，
 * 不需要在测试环境里造 DOM 事件）。
 */
export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/**
 * 键位 → 动作。**这是浮层唯一的键位映射，别在组件里另写一份**。
 *
 * 键位**照抄 KeePassXC 2.8.0 的 Auto-Type 选择窗**（`src/autotype/
 * AutoTypeSelectDialog.cpp:326-350` 的 `setShortcut`），主人 2026-11-04 指定参考它：
 *
 * | 键 | 动作 | 打什么 |
 * |---|---|---|
 * | `Enter` | `default` | 用户名 + Tab + 密码 + 回车（默认序列） |
 * | `Ctrl+1` | `username` | 仅用户名（KeePassXC：Type `{USERNAME}`） |
 * | `Ctrl+2` | `password` | 仅密码（Type `{PASSWORD}`） |
 * | `Ctrl+3` | `totp` | 仅动态口令（Type `{TOTP}`） |
 * | `Ctrl+4` | `url` | 仅网址（Type `{URL}`） |
 *
 * **顺序必须与 KeePassXC 完全一致，不许按"我们觉得顺"重排**：这套键位是肌肉
 * 记忆。差一位就会让人按 `Ctrl+1` 以为只打用户名、实际把密码一起打进只有用户名
 * 的那一栏（1.5.40 开发时我自己第一版就这么错过，靠读参考实现才拦住）。
 *
 * KeePassXC 还有两个键我们**刻意不做**：
 * - `Ctrl+5` = 虚拟键盘（仅 Windows/macOS，需要它自己的安全桌面键盘，我们没有）；
 * - `Ctrl+Shift+1..4` = 复制用户名/密码/TOTP/网址到剪贴板——本应用"复制"是
 *   快速搜索小窗（主热键）的职责，同一个浮层里混两套语义会让"选中之后到底
 *   打进去了还是进剪贴板了"变成新的困惑。
 *
 * 为什么不用 1.5.39 那套修饰键组合：`Alt+Enter` 在真机上按不出来——Alt 会被
 * Windows / 输入法先吃掉一层（单独按下就进菜单模式或响铃），事件到不了 WebView。
 * 当初把「网址」押在 `Alt+Enter` 上，等于把最常用的功能之一压在唯一不可用的键上。
 *
 * `Shift` 不参与判定（`Shift+Enter` 与 `Enter` 同义、`Ctrl+Shift+2` 仍是仅密码）：
 * 左手小指压在 Shift 上很常见，判成"无效"会让按键时灵时不灵。`Alt` 参与判定的
 * 结果是**拒绝**，避免和窗口菜单抢键时误触发。
 *
 * 返回 `null` = 这个键不是动作键，调用方什么都不该做。
 */
export function actionForKey(e: KeyLike): AutotypeAction | null {
  if (e.altKey) {
    return null;
  }
  if (e.key === 'Enter') {
    // Ctrl+Enter 不是动作（它是 1.5.39 的旧键位）：按下时直接拒绝，
    // 免得用户以为自己在用旧快捷键而实际走了别的路径。
    return e.ctrlKey ? null : 'default';
  }
  if (!e.ctrlKey) {
    return null;
  }
  switch (e.key) {
    case '1':
      return 'username';
    case '2':
      return 'password';
    case '3':
      return 'totp';
    case '4':
      return 'url';
    default:
      // '5' 及以后留空：Ctrl+5 在 KeePassXC 是虚拟键盘，本应用没有该能力，
      // 硬绑一个"看起来能用"的动作比不绑更坏。
      return null;
  }
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
