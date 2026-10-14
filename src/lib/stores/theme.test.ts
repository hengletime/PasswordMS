// 主题 store：窗口标题栏同步（D-32）与持久化。
//
// 说明：本仓库 vitest 环境为 `node`（无 DOM），故这里用 `vi.stubGlobal` 注入最小
// 的 window / document / localStorage 假体，并用 `vi.mock` 伪造
// `@tauri-apps/api/window`（`getCurrentWindow().setTheme`）。
// 每个用例前 `vi.resetModules()`，以便重新触发模块顶层的 `new ThemeStore()` 单例。

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { setThemeMock } = vi.hoisted(() => ({ setThemeMock: vi.fn() }));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setTheme: setThemeMock }),
}));

interface FakeWindow {
  __TAURI_INTERNALS__: Record<string, unknown>;
  matchMedia: (query: string) => { matches: boolean; addEventListener: (type: string, cb: unknown) => void };
}

interface FakeRootElement {
  dataset: Record<string, string>;
  style: { colorScheme?: string };
  setAttribute: (name: string, value: string) => void;
  removeAttribute: (name: string) => void;
}

interface FakeStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
  removeItem: (key: string) => void;
}

/** 注入最小 window / document / localStorage 假体，返回 localStorage 以便断言。 */
function installGlobals(stored: Record<string, string>): FakeStorage {
  const store = new Map<string, string>(Object.entries(stored));

  const storage: FakeStorage = {
    getItem: (key) => (store.has(key) ? (store.get(key) as string) : null),
    setItem: (key, value) => {
      store.set(key, value);
    },
    removeItem: (key) => {
      store.delete(key);
    },
  };

  const root: FakeRootElement = {
    dataset: {},
    style: {},
    setAttribute: (name, value) => {
      // 模拟真实 DOM 对 data-* 的 dataset 反映（store 用 setAttribute 写 data-theme）。
      if (name.startsWith('data-')) {
        root.dataset[name.slice(5).replace(/-([a-z])/g, (_m, c: string) => c.toUpperCase())] = value;
      }
    },
    removeAttribute: (name) => {
      if (name.startsWith('data-')) {
        delete root.dataset[name.slice(5).replace(/-([a-z])/g, (_m, c: string) => c.toUpperCase())];
      }
    },
  };

  const win: FakeWindow = {
    __TAURI_INTERNALS__: {},
    matchMedia: () => ({
      matches: false,
      addEventListener: () => {
        /* no-op：本测试不驱动系统配色变化 */
      },
    }),
  };

  vi.stubGlobal('window', win);
  vi.stubGlobal('document', { documentElement: root });
  vi.stubGlobal('localStorage', storage);
  return storage;
}

/** 读取主题根元素（断言 data-theme 用）。 */
function rootElement(): FakeRootElement {
  return (globalThis as unknown as { document: { documentElement: FakeRootElement } }).document.documentElement;
}

describe('theme store —— 窗口标题栏同步 (D-32)', () => {
  beforeEach((): void => {
    vi.resetModules();
    setThemeMock.mockClear();
  });

  afterEach((): void => {
    vi.unstubAllGlobals();
  });

  it("默认 system 模式 → setTheme(null)（标题栏跟随系统）", async () => {
    installGlobals({});
    const { theme } = await import('./theme.svelte');
    expect(theme.mode).toBe('system');
    await vi.waitFor(() => {
      expect(setThemeMock).toHaveBeenCalledWith(null);
    });
  });

  it("setMode('dark') → setTheme('dark')，并持久化 + 落 data-theme", async () => {
    const storage = installGlobals({});
    const { theme } = await import('./theme.svelte');
    theme.setMode('dark');
    expect(storage.getItem('passwordms.theme')).toBe('dark');
    expect(rootElement().dataset.theme).toBe('dark');
    await vi.waitFor(() => {
      expect(setThemeMock).toHaveBeenCalledWith('dark');
    });
  });

  it("setMode('light') → setTheme('light')，data-theme=light", async () => {
    installGlobals({});
    const { theme } = await import('./theme.svelte');
    theme.setMode('light');
    expect(rootElement().dataset.theme).toBe('light');
    await vi.waitFor(() => {
      expect(setThemeMock).toHaveBeenCalledWith('light');
    });
  });

  it('持久化：重新加载时恢复上次模式', async () => {
    installGlobals({ 'passwordms.theme': 'dark' });
    const { theme } = await import('./theme.svelte');
    expect(theme.mode).toBe('dark');
    await vi.waitFor(() => {
      expect(setThemeMock).toHaveBeenCalledWith('dark');
    });
  });
});
