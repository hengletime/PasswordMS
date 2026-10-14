// 主题状态（Svelte 5 runes）：跟随系统 / 显式浅色 / 显式深色。
//
// 地基说明：`src/styles/global.css` 已定义四套令牌：
//   :root                              —— 浅色默认值
//   @media (prefers-color-scheme: dark)—— 系统偏好深色时
//   :root[data-theme='dark']           —— 显式深色
//   :root[data-theme='light']          —— 显式浅色
// 其中 `[data-theme]` 选择器特异性为 (0,2,0)，高于媒体查询的 (0,1,0)，
// 故只要在 <html> 上设置 / 移除 `data-theme` 属性，即可强制覆盖系统偏好。
//
// 本 store 只负责「应用与持久化模式」，不改动任何令牌值、不新增配色。

/** 主题模式。 */
export type ThemeMode = 'system' | 'light' | 'dark';

/** localStorage 持久化键名。 */
const STORAGE_KEY = 'passwordms.theme';

/** 合法模式集合（用于校验读入值，避免脏数据）。 */
const MODES: readonly ThemeMode[] = ['system', 'light', 'dark'];

/** 判定任意字符串是否为合法主题模式。 */
function isThemeMode(value: string | null): value is ThemeMode {
  return value !== null && (MODES as readonly string[]).includes(value);
}

/** 读取持久化模式；隐私模式 / 无 localStorage 时兜底为 `system`。 */
function readStoredMode(): ThemeMode {
  try {
    if (typeof localStorage === 'undefined') {
      return 'system';
    }
    const raw = localStorage.getItem(STORAGE_KEY);
    return isThemeMode(raw) ? raw : 'system';
  } catch {
    return 'system';
  }
}

/** 写入持久化模式；失败（隐私模式 / 配额不足）时静默忽略，界面仍按内存态生效。 */
function writeStoredMode(mode: ThemeMode): void {
  try {
    if (typeof localStorage === 'undefined') {
      return;
    }
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // 忽略：持久化失败不应阻塞切换，更不应导致白屏。
  }
}

/** 读取系统是否偏好深色；无 window / 不支持 matchMedia 时兜底为 false。 */
function readSystemDark(): boolean {
  try {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return false;
    }
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  } catch {
    return false;
  }
}

/** 主题 store。 */
class ThemeStore {
  /** 当前主题模式（默认 `system`）。 */
  mode = $state<ThemeMode>(readStoredMode());
  /** 系统当前是否偏好深色（由 matchMedia 监听驱动，保证响应系统切换）。 */
  systemDark = $state<boolean>(readSystemDark());

  constructor() {
    // 模块加载即应用一次，避免刷新后先闪一下默认主题再跳到目标主题。
    this.apply();
    this.watchSystem();
  }

  /** 当前**实际生效**的主题（`system` 模式解析为 `light` | `dark`）。 */
  get effective(): ThemeMode {
    return this.mode === 'system' ? (this.systemDark ? 'dark' : 'light') : this.mode;
  }

  /** 设置模式：持久化 + 立即生效。 */
  setMode(mode: ThemeMode): void {
    this.mode = mode;
    writeStoredMode(mode);
    this.apply();
  }

  /** 把当前模式应用到 <html>（`data-theme` + 原生控件配色方案），并同步窗口标题栏。 */
  private apply(): void {
    if (typeof document !== 'undefined') {
      const root = document.documentElement;
      if (this.mode === 'system') {
        // 交还给媒体查询：移除显式属性，恢复系统偏好。
        root.removeAttribute('data-theme');
        root.style.colorScheme = '';
      } else {
        // 显式主题：设置属性强制覆盖，并同步 colorScheme 让滚动条 / 原生表单控件跟随。
        root.setAttribute('data-theme', this.mode);
        root.style.colorScheme = this.mode;
      }
    }
    // 让原生窗口标题栏跟随应用主题（D-32）。签名保持同步，内部自行 void + catch。
    this.syncWindowChrome();
  }

  /**
   * 同步原生窗口标题栏主题（D-32）。
   * - 仅 Tauri 运行时生效：浏览器 / dev / vitest（无 `__TAURI_INTERNALS__`）直接跳过。
   * - `system` 模式传 `null` = 交还系统；否则传显式 `'light' | 'dark'`。
   * - 动态 import 避免在非 Tauri 环境加载窗口模块；`void` + `catch` 保证不产生未处理 rejection。
   * - Android / 无窗口 / 缺权限：静默忽略，绝不阻塞主题切换。
   */
  private syncWindowChrome(): void {
    if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
      return;
    }
    const next: 'light' | 'dark' | null = this.mode === 'system' ? null : this.mode;
    void import('@tauri-apps/api/window')
      .then(({ getCurrentWindow }) => getCurrentWindow().setTheme(next))
      .catch((): void => {
        // 忽略：无窗口 / 缺权限 / 平台不支持标题栏着色。
      });
  }

  /** 监听系统配色变化：仅影响 `system` 模式下的 `effective` 与系统偏好缓存的时效性。 */
  private watchSystem(): void {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }
    try {
      const mql = window.matchMedia('(prefers-color-scheme: dark)');
      this.systemDark = mql.matches;
      mql.addEventListener('change', (event: MediaQueryListEvent): void => {
        this.systemDark = event.matches;
      });
    } catch {
      // 环境不支持时保持兜底值，不抛异常。
    }
  }
}

/** 单例。 */
export const theme = new ThemeStore();
