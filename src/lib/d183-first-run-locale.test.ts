// D-183 守门：首启语言校正必须「只在没有记录时跑」「结果写回」「小窗不付代价」。
//
// 这三条都不是风格问题，任何一条漏掉都会把 2026-11-03 那个 bug 原样带回来：
//   * 没有「只在没有记录时跑」→ 用户手动选的中文会被系统语言每次启动覆盖；
//   * 没有写回 → 每次启动都重新现场检测，档案瞬时不可用那天又是整页英文；
//   * 小窗也跑 → 热键唤出的延迟白加一次 IPC 往返。
import { describe, expect, it } from 'vitest';

const sources = import.meta.glob(['../main.ts', './stores/ui.svelte.ts'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function source(name: string): string {
  const hit = Object.entries(sources).find(([path]) => path.endsWith(name));
  expect(hit, `读不到源码：${name}`).toBeDefined();
  return hit![1];
}

describe('D-183 首启语言校正', () => {
  const main = source('main.ts');

  it('守门本身没空跑：两份源码都读得到且非空', () => {
    // glob 静默取空集时上面的 expect 会红，但这里再钉一次长度，防「空文件也算读到」。
    expect(main.length).toBeGreaterThan(500);
    expect(source('ui.svelte.ts').length).toBeGreaterThan(1000);
  });

  it('只在 localStorage 里**没有**语言记录时才校正', () => {
    expect(main).toContain('localStorage.getItem(LOCALE_STORAGE_KEY) !== null');
    expect(main).toContain('return;');
  });

  it('校正结果必须写回（setLocale 负责持久化），不许只改内存', () => {
    expect(main).toContain('ui.setLocale(mapSystemLanguage(tag))');
    // 反面证据：如果只赋值 `ui.locale = ...`，语言就永远不会落盘。
    expect(main).not.toContain('ui.locale =');
  });

  it('取的是系统 UI 语言（后端命令），不是 navigator.language', () => {
    expect(main).toContain('api.system.systemLocale()');
    // 注释里说明历史原因可以提 navigator.language，但**代码里不许拿它当来源**。
    expect(main).not.toMatch(/mapSystemLanguage\(\s*navigator/);
  });

  it('快速搜索小窗不跑校正（热键延迟最敏感）', () => {
    expect(main).toContain('if (!quick)');
    // 状态模块必须走动态 import：静态 import 会让小窗也解析主界面整套状态。
    expect(main).toContain("import('./lib/stores/ui.svelte')");
    expect(main).not.toContain("from './lib/stores/ui.svelte'");
  });

  it('语言键从 store 导出，两处不再各写一份字符串', () => {
    expect(source('ui.svelte.ts')).toContain('export const LOCALE_STORAGE_KEY');
  });
});
