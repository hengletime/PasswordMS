// D-73：全仓禁用 `structuredClone`（源码结构断言）。
//
// 为什么必须在全仓禁用，而不是「小心点用」：
// `structuredClone` 对 **任何 Proxy** 都会抛 `DataCloneError`，而 Svelte 5 的 `$state`
// 深层响应式值是 Proxy。于是这个坑不是「某处写错了」，而是「这个 API 与本框架的响应式
// 模型天然不兼容」——只要有人再写一次，就会再炸一次。
//
// 更坏的是**炸法**：它通常被写在 `$effect` / `onMount` / 事件处理器里，抛在**第一行**
// 时，同一块里后面的逻辑**从不执行**、也不会报错到界面（只有控制台）。1.2.6 那四处症状
// （库文件卡片空白 / 版本显示「—」/ 自动锁定档位永远是 5 分钟 / 连带整块不刷新）
// 全部来自这一行。
//
// 正确写法：深拷 `$state` 用 `$state.snapshot(value)`（返回普通对象）。
// 注意 `{ ...value }` 只是浅拷贝：嵌套的 `generator` / `health` 仍是 Proxy，
// 后续 `bind:value` 会**直接写穿到 store**，变成「取消不掉」的隐藏写穿——同样不能用。

import { describe, expect, it } from 'vitest';

const sources = import.meta.glob('../../src/**/*.{svelte,ts}', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

describe('D-73：src 内禁用 `structuredClone`（Svelte 5 `$state` 代理必抛 DataCloneError）', () => {
  it('扫描面有效（防 glob 写错退化成 0 === 0 假绿）', () => {
    const paths = Object.keys(sources);
    // 防空扫：路径写错时 `sources` 是空对象，下面的断言会「在空集合上转绿」。
    expect(paths.length).toBeGreaterThan(60);
    // 本缺陷的发生地**必须**在扫描集里 —— 比「路径里有没有 /components/」这种形状判断更直接。
    // （注：`import.meta.glob` 的键相对于**本文件所在目录**，所以同目录文件是 `./x.ts`、
    //  跨目录是 `../components/x.svelte`，按目录字符串判断会误判。）
    expect(paths.some((p) => p.endsWith('/Settings.svelte'))).toBe(true);
    // 两种扩展名都要扫到：只扫到 .svelte（漏 lib 纯逻辑层）或只扫到 .ts 都是半漏。
    expect(paths.some((p) => p.endsWith('.svelte'))).toBe(true);
    expect(paths.some((p) => p.endsWith('.ts'))).toBe(true);
  });

  it('全仓无该 API 的调用形态（含本文件自身——它的正则字面量不构成调用）', () => {
    // 正则 `(^|[^.\w])` 前置边界：排除 `foo.structuredClone` 这类假设性方法名，
    // 要求它是独立标识符。`\s*\(` 要求真的带左圆括号 ——
    // 所以注释里只提名字、不写调用括号，不会被误判。
    const hits = Object.entries(sources)
      .filter(([, src]) => /(^|[^.\w])structuredClone\s*\(/.test(src))
      .map(([path]) => path);

    expect(hits).toEqual([]);
  });
});
