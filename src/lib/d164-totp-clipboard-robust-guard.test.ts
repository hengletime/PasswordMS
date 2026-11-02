// D-164 守门：Ctrl+V 取不到图 / 截图框完不自动填。
//
// 两条失败的路径有一个共同点：**都在依赖一个我们无法控制的信号**。
//   · Ctrl+V 依赖「宿主把剪贴板位图转成 `image/*` 交给 DataTransfer」——WebView2 经常不转；
//   · 截图依赖「剪贴板序号读得到」——读不到时旧代码只给一次机会（硬等 800ms），
//     而用户框选要好几秒，那一次必然读到空剪贴板。
// 于是功能明明在，表现却都是「按了没反应」。本文件钉住四件事，防止回退：
//   ① 粘贴三路判定齐全（有图 / 有文本 / 两者皆无 → 原生读取）；
//   ② 文本粘贴必须放行（不然往密码框粘文字会被吃掉）；
//   ③ 文本 / 文件 / 两者皆无 三路都不能互相吞掉。
// （截图流程的判据已迁到 D-165：用系统截图工具那条路已经被证明做不到「冻结」）

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../components/ItemEditor.svelte',
    '../../src-tauri/capabilities/default.json',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** 取源码；**必须归一化换行**——文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

/** 切出区间（含首尾标记）。找不到返回空串——断言前先钉长度 > 0，否则切片失败会假绿。 */
function section(text: string, open: string, close: string): string {
  const start = text.indexOf(open);
  if (start < 0) {
    return '';
  }
  const end = text.indexOf(close, start + open.length);
  return end < 0 ? '' : text.slice(start, end + close.length);
}

const editor = src('/ItemEditor.svelte');

describe('D-164：Ctrl+V 的三路判定', () => {
  // 结束标记用清理函数的开头，而不是某个 `addEventListener`：
  // 否则「删掉注册」的变异会让切片本身失败，红得对但红得含糊。
  const fx = section(editor, 'let sawPaste = false;', 'return (): void => {');

  it('⑩ 粘贴处理块切片成功（切片失败会让后面全假绿）', () => {
    expect(fx).not.toBe('');
    expect(fx, '找不到粘贴处理函数').toContain('const onPaste');
  });

  it('⑪ 三路齐全：图片文件 / 文本 / 两者皆无', () => {
    expect(fx, '缺少「按文件类型分流」的判断').toContain("item.kind !== 'file'");
    expect(fx, '缺少图片分支').toContain("item.type.startsWith('image/')");
    expect(fx, '缺少「有文本」判定').toContain('textOf(data)');
    expect(fx, '缺少「两者皆无 → 原生读取」的兜底').toContain('void pasteImage();');
  });

  it('⑫ 文本判定必须早于兜底，且命中后 `return`（不许把文本粘贴吞掉）', () => {
    const guardAt = fx.indexOf("if (textOf(data) !== '') {");
    const fallbackAt = fx.indexOf('void pasteImage();');
    expect(guardAt, '缺少文本放行判断').toBeGreaterThan(-1);
    expect(fallbackAt, '缺少原生读取兜底').toBeGreaterThan(-1);
    expect(guardAt, '文本判定晚于兜底：粘文字会被截走').toBeLessThan(fallbackAt);

    const guardBody = fx.slice(guardAt, fx.indexOf('}', guardAt));
    expect(guardBody, '文本分支没有 return，文本会被 preventDefault 掉').toContain('return');
    expect(guardBody, '文本分支不该拦粘贴').not.toContain('preventDefault');

    // 取文本这一步本身要带 try：getData 抛一次就会把 paste 处理器整个中断。
    const helper = section(editor, 'function textOf(', '\n  }');
    expect(helper, 'textOf 切片失败').not.toBe('');
    expect(helper, "没有读 text/plain").toContain("getData('text/plain')");
    expect(helper, '没有 try：个别宿主上 getData 一抛，粘贴就彻底没反应').toContain('try {');
  });

  it('⑫b 剪贴板里是文件但不是图片 → 放行，不拦也不提示', () => {
    // 从资源管理器复制一个 .txt 过来，items 里有 file 但 type 不是 image。
    // 那不是我们要接的东西，拦下来再弹「剪贴板里没有图片」纯属打扰。
    const guardAt = fx.indexOf('if (hasAnyFile) {');
    expect(guardAt, '没有「存在非图片文件」的放行判断').toBeGreaterThan(-1);
    const body = fx.slice(guardAt, fx.indexOf('}', guardAt));
    expect(body).toContain('return');
    expect(body).not.toContain('preventDefault');
  });

  it('⑬ 兜底路径必须先 preventDefault 再读图（否则浏览器会同时插一份空内容）', () => {
    // 从「放行判断」之后切到函数尾：那里才是第③路。
    const tail = fx.slice(fx.indexOf('if (hasAnyFile) {'));
    const preventAt = tail.indexOf('event.preventDefault()');
    const callAt = tail.indexOf('void pasteImage();');
    expect(preventAt, '兜底前没有 preventDefault').toBeGreaterThan(-1);
    expect(callAt).toBeGreaterThan(preventAt);
  });

  it('⑭ keydown 兜底：Ctrl+V 后若没有 paste 事件则原生读图', () => {
    expect(fx, '缺少键盘兜底：宿主不派发 paste 事件时又回到「按了没反应」').toContain(
      "event.key.toLowerCase() !== 'v'",
    );
    expect(fx, '键盘兜底没带修饰键判断（会把单独按 v 也当粘贴）').toContain('event.ctrlKey || event.metaKey');
    const guardAt = fx.indexOf('if (!sawPaste)');
    expect(guardAt, '键盘兜底没有「是否已收到 paste」的判断').toBeGreaterThan(-1);
    expect(fx.slice(guardAt), '键盘兜底没有真正去读图').toContain('void pasteImage();');
    // 写了函数不等于挂上了：漏了这行，兜底就是个死函数（变异实测过这条会漏）。
    expect(fx, 'onKeydown 定义完却没注册：兜底永远不会触发').toContain(
      "addEventListener('keydown', onKeydown)",
    );
  });

  it('⑮ 卸载时必须摘掉两个监听并清掉定时器（否则离开编辑器还在读剪贴板）', () => {
    // 锚在清理函数自身：从 `return () => {` 到它的 `};`。
    const cleanup = section(
      editor,
      'return (): void => {\n      window.clearTimeout(fallbackTimer);',
      '};',
    );
    expect(cleanup, '卸载清理块切片失败').not.toBe('');
    expect(cleanup, '卸载没清定时器').toContain('clearTimeout(fallbackTimer)');
    expect(cleanup, '卸载没摘 paste 监听').toContain("removeEventListener('paste'");
    expect(cleanup, '卸载没摘 keydown 监听').toContain("removeEventListener('keydown'");
  });
});


describe('D-164：截图的窗口权限仍然要齐（流程改到 D-165，权限没变）', () => {
  it('⑲ 权限齐全（缺 hide/show 会让「截图」闪一下就回来）', () => {
    const caps = src('capabilities/default.json');
    expect(caps, '权限文件读取失败').toContain('clipboard-manager:allow-read-image');
    for (const p of [
      'core:window:allow-hide',
      'core:window:allow-show',
      'core:window:allow-set-focus',
    ]) {
      expect(caps, `缺少窗口权限：${p}`).toContain(p);
    }
  });
});

// 截图流程本身（抓屏、冻结快照、框选）已整体重做，判据迁到 d165-screen-crop-guard.test.ts。
// 这里留下的只有粘贴链路与权限：它们不依赖「用什么方式拿到一张图」。
