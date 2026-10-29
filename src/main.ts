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

// 挂载点作为参数传入，而不是在闭包里读模块级变量：`boot` 是会被提升的函数声明，
// 在它内部引用 `root` 时 TypeScript 不再保留 `if (!root)` 的收窄结果。
async function boot(target: HTMLElement): Promise<void> {
  const component = isQuickWindow(window.location.search)
    ? (await import('./components/QuickSearch.svelte')).default
    : (await import('./App.svelte')).default;
  mount(component, { target });
}

void boot(root);
