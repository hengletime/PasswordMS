// D-92 / D-93 UI 守卫：编辑表单桌面端撑满右栏 + 操作条 flex 固定底栏（不再 sticky）。
//
// 无 DOM 测试环境，用源码结构断言（与 d85-ui-guard / d88-d90-ui-guard 同族）。
// 背景（docs/06 §5.4 v2.22）：
//   D-92 —— 桌面端 .pane 为 display:contents，form.editor 直接是 .body(flex row) 的
//           item；flex:1 一旦只写在移动端媒体查询里，桌面宽度就按内容收缩成窄列。
//   D-93 —— sticky bottom:0 吸附范围被包含块限制，贴不到容器底：其下 padding 空带
//           露出滚过的字段，内容不足一屏时工具条停在内容末尾与容器底之间留白。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(['../components/*.svelte'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('D-92：编辑表单必须桌面/移动统一撑满（flex:1 不许只写在窄屏媒体查询里）', () => {
  const src = pickSource('ItemEditor.svelte');

  it('form.editor 的样式块里直接带 flex:1 / min-width:0 / min-height:0', () => {
    const styleAt = src.indexOf('<style>');
    expect(styleAt).toBeGreaterThan(-1);
    const editorAt = src.indexOf('.editor {', styleAt);
    expect(editorAt).toBeGreaterThan(-1);
    const block = src.slice(editorAt, src.indexOf('}', editorAt));
    expect(block).toContain('flex: 1');
    expect(block).toContain('min-width: 0');
    expect(block).toContain('min-height: 0');
  });

  it('组件内不许再有针对 .editor 的 max-width 媒体查询 flex 恢复规则（防回归成「只有窄屏撑满」）', () => {
    const style = src.slice(src.indexOf('<style>'));
    expect(style).not.toMatch(/@media[^{]*\{[^@]*?\.editor\s*\{[^}]*flex:\s*1/);
  });
});

describe('D-93：操作条必须是 flex 固定底栏（sticky 在编辑器里永久禁用）', () => {
  const src = pickSource('ItemEditor.svelte');

  it('全组件不许出现 position: sticky', () => {
    expect(src).not.toContain('position: sticky');
  });

  it('字段区包进 .editor-scroll 滚动层（padding 归滚动层，操作条下方无空带）', () => {
    const scrollAt = src.indexOf('class="editor-scroll"');
    expect(scrollAt, '必须存在 .editor-scroll 滚动层').toBeGreaterThan(-1);
    const actionsAt = src.indexOf('class="editor-actions"');
    expect(actionsAt).toBeGreaterThan(scrollAt);
  });

  it('.editor-scroll 有独立滚动（overflow:auto + min-height:0）', () => {
    const styleAt = src.indexOf('<style>');
    const scrollAt = src.indexOf('.editor-scroll {', styleAt);
    expect(scrollAt).toBeGreaterThan(-1);
    const block = src.slice(scrollAt, src.indexOf('}', scrollAt));
    expect(block).toContain('overflow: auto');
    expect(block).toContain('min-height: 0');
  });

  it('.editor-actions 固定底栏（flex-shrink:0 + 不透明背景）', () => {
    const styleAt = src.indexOf('<style>');
    const actionsAt = src.indexOf('.editor-actions {', styleAt);
    expect(actionsAt).toBeGreaterThan(-1);
    const block = src.slice(actionsAt, src.indexOf('}', actionsAt));
    expect(block).toContain('flex-shrink: 0');
    expect(block).toContain('background: var(--pms-bg)');
  });
});
