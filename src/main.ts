import { mount } from 'svelte';
import './styles/global.css';
import { isQuickWindow } from './lib/quick-search';

// 应用引导入口。
//
// 同一个 `index.html` 服务两个窗口（见 `src-tauri/src/quick_search.rs`）：
//   * 主窗口     → `App.svelte`
//   * 快速搜索小窗 → `QuickSearch.svelte`（URL 带 `?w=quick`）
//
// 分流必须发生在 `mount` 之前，而且**必须走动态 import**：小窗只要一块输入框
// 加一个列表，没理由为了它把主界面那 500KB 全套组件也解析一遍。热键唤出对延迟
// 最敏感，多解析半秒就是肉眼可见的"按了没反应"。
const root = document.getElementById('app');
if (!root) {
  // 挂载点缺失属于开发期装配错误，直接抛出让引导阶段尽早暴露问题。
  throw new Error('PasswordMS: mount target #app not found');
}

const quick = isQuickWindow(window.location.search);

/**
 * 【D-183】全新安装（还没有任何语言记录）时，用**系统 UI 语言**校正首启语言，
 * 并把结果立刻写回 localStorage。
 *
 * 为什么需要校正：首启只能现场检测，而 WebView2 的 `navigator.language` 在档案
 * 瞬时不可用的瞬间可能报 `en-US`——2026-11-03 真机实例：升级后首次打开整页英文，
 * 重启即自愈，因为那天那次检测结果根本没被记住。后端 `system_locale` 直接问
 * Win32，不经过 WebView 的任何状态。
 *
 * 只在「没有记录」时跑：用户手动选过的语言永远优先，不许被系统语言覆盖。
 * 写回是有意的——检测一次就钉死，下次启动不再重赌。
 */
async function correctFirstRunLocale(): Promise<void> {
  // 动态 import：小窗不该为了这条校正把主界面的状态模块也解析一遍。
  const [{ LOCALE_STORAGE_KEY, ui }, { mapSystemLanguage }, { api }] = await Promise.all([
    import('./lib/stores/ui.svelte'),
    import('./lib/i18n'),
    import('./lib/ipc'),
  ]);
  try {
    if (localStorage.getItem(LOCALE_STORAGE_KEY) !== null) {
      return;
    }
  } catch {
    // 存储读不到（隐私模式 / 档案瞬时不可用）：保持 navigator 的结果，不猜。
    return;
  }
  try {
    const tag = await api.system.systemLocale();
    if (tag !== null) {
      // `setLocale` 负责持久化；即使与当前值相同也要写，否则下次还要再检测一次。
      ui.setLocale(mapSystemLanguage(tag));
    }
  } catch {
    // 命令不可用（旧后端 / 权限）不阻断启动：保持现状即可。
  }
}

// 挂载点作为参数传入，而不是在闭包里读模块级变量：`boot` 是会被提升的函数声明，
// 在它内部引用 `root` 时 TypeScript 不再保留 `if (!root)` 的收窄结果。
async function boot(target: HTMLElement): Promise<void> {
  if (!quick) {
    // 在 mount 之前校正：晚一步就是先画一帧英文再闪成中文。
    await correctFirstRunLocale();
  }
  const component = quick
    ? (await import('./components/QuickSearch.svelte')).default
    : (await import('./App.svelte')).default;
  mount(component, { target });
}

void boot(root);
