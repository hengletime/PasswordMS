// D-164 守门：Ctrl+V 取不到图 / 截图框完不自动填。
//
// 两条失败的路径有一个共同点：**都在依赖一个我们无法控制的信号**。
//   · Ctrl+V 依赖「宿主把剪贴板位图转成 `image/*` 交给 DataTransfer」——WebView2 经常不转；
//   · 截图依赖「剪贴板序号读得到」——读不到时旧代码只给一次机会（硬等 800ms），
//     而用户框选要好几秒，那一次必然读到空剪贴板。
// 于是功能明明在，表现却都是「按了没反应」。本文件钉住四件事，防止回退：
//   ① 粘贴三路判定齐全（有图 / 有文本 / 两者皆无 → 原生读取）；
//   ② 文本粘贴必须放行（不然往密码框粘文字会被吃掉）；
//   ③ 截图先藏窗口（二维码就在窗口后面），恢复放 finally；
//   ④ 等新图的两级判据：序号是加速不是唯一；空/旧图都必须继续等，不许当结果交出。

import { describe, expect, it, vi } from 'vitest';

// 只测纯函数，挡掉两个运行时依赖，避免把 IPC / 插件拉进来。
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ readImage: vi.fn() }));
vi.mock('./ipc', () => ({ system: {} }));

import { fingerprintOf } from './clipboard-image';

const raw = import.meta.glob(
  [
    '../components/ItemEditor.svelte',
    './clipboard-image.ts',
    './screenshot-flow.ts',
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
const lib = src('/clipboard-image.ts');

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

describe('D-164：截图先藏窗口，恢复放 finally', () => {
  const shot = section(editor, 'async function startScreenshot(', '\n  }');

  it('⑯ 唤起截图工具必须包在「藏窗口」里（二维码就在窗口后面）', () => {
    expect(shot, 'startScreenshot 切片失败').not.toBe('');
    const hideAt = shot.indexOf('runWithWindowHidden(');
    const openAt = shot.indexOf('system.openScreenshotTool()');
    expect(hideAt, '截图前没有藏窗口：框到的是我们自己').toBeGreaterThan(-1);
    expect(openAt, '没有唤起系统截图工具').toBeGreaterThan(-1);
    expect(openAt, 'openScreenshotTool 没在 runWithWindowHidden 里').toBeGreaterThan(hideAt);
  });

  it('⑰ 窗口恢复必须在 finally 里，且以 hidden 为条件', () => {
    const flow = src('/screenshot-flow.ts');
    const finallyAt = flow.indexOf('} finally {');
    expect(finallyAt, '没有 finally：action 抛错时窗口会一直藏着').toBeGreaterThan(-1);
    const after = flow.slice(finallyAt);
    expect(after, 'finally 里没有恢复窗口').toContain('win.show()');
    expect(after, '恢复后没有取回焦点：用户还得再点一下').toContain('win.setFocus()');
    // 恢复必须以「真的藏过」为条件：无条件 show 会把用户自己最小化的窗口也弹出来。
    const guardAt = after.indexOf('if (hidden)');
    expect(guardAt, '恢复没有 hidden 前置判断').toBeGreaterThan(-1);
    expect(after.indexOf('win.show()'), 'show 跑在判断之前，条件形同虚设').toBeGreaterThan(
      guardAt,
    );
  });

  it('⑱ 唤起前先等窗口消失（否则覆盖层截到的是还没消失的那一帧）', () => {
    const flow = src('/screenshot-flow.ts');
    const hideAt = flow.indexOf('await win.hide()');
    const settleAt = flow.indexOf('await delay(HIDE_SETTLE_MS)');
    expect(hideAt).toBeGreaterThan(-1);
    expect(settleAt, '藏完立刻唤起：动画未结束，截到的还是自己').toBeGreaterThan(hideAt);
  });

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

describe('D-164：等新图的两级判据', () => {
  const wait = section(lib, 'export async function waitForClipboardImage(', '\n}');

  it('⑳ 等待函数切片成功', () => {
    expect(wait).not.toBe('');
    expect(wait).toContain('while (Date.now() < deadline)');
  });

  it('㉑ 序号读不到时必须持续轮询，不能「只给一次机会」', () => {
    expect(wait, '缺少无序号时的直读节奏常量').toContain('FALLBACK_READ_INTERVAL_MS');
    expect(wait, '又回到「拿不到序号就只试一次」的老写法').not.toContain('if (baseline === null) {');
    // 退化分支必须在循环里（否则等于没循环）
    const loopAt = wait.indexOf('while (');
    const fallbackAt = wait.indexOf('FALLBACK_READ_INTERVAL_MS');
    expect(fallbackAt).toBeGreaterThan(loopAt);
  });

  it('㉒ 读到空剪贴板要继续等，不许当结果返回', () => {
    const at = wait.indexOf("if (got.kind === 'empty') {");
    expect(at, '没有处理「读到空」的情况').toBeGreaterThan(-1);
    const body = wait.slice(at, wait.indexOf('}', at));
    expect(body, '读到空就 continue，用户还在框选').toContain('continue');
    expect(body, '读到空就返回，会把「还没框完」误报成失败').not.toContain('return');
  });

  it('㉓ 读到「截图前那张旧图」要继续等，不许填进去', () => {
    const at = wait.indexOf('got.fingerprint === baseline.fingerprint');
    expect(at, '缺少旧图判定：会把上一个二维码填进表单').toBeGreaterThan(-1);
    const body = wait.slice(at, wait.indexOf('}', at));
    expect(body, '旧图必须 continue').toContain('continue');
    expect(body, '旧图不许当结果交出').not.toContain('return got');
  });

  it('㉔ 指纹必须来自真实像素，不能只看尺寸', () => {
    const fp = section(lib, 'function fingerprintOf(', '\n}');
    expect(fp, '指纹函数切片失败').not.toBe('');
    expect(fp, '指纹没混入宽度').toContain('mix(width)');
    expect(fp, '指纹没混入高度').toContain('mix(height)');
    expect(fp, '指纹没混入像素：所有同尺寸截图会被当成同一张').toContain('mix(rgba[at])');
    expect(lib, 'readClipboardImage 没把指纹带出来').toContain(
      'fingerprint: fingerprintOf(width, height, rgba)',
    );
  });

  it('㉕ 等待上限不超过 45 秒（窗口已恢复，没有理由让用户干等一分钟）', () => {
    const m = /const WAIT_TIMEOUT_MS = ([\d_]+);/.exec(lib);
    expect(m, '找不到等待上限常量').not.toBeNull();
    const ms = Number((m as RegExpExecArray)[1].replace(/_/g, ''));
    expect(ms).toBeGreaterThan(5_000);
    expect(ms, '等待太久：取消截图后用户能察觉到一个卡住的流程').toBeLessThanOrEqual(45_000);
  });
});

describe('D-164：指纹必须真的跟着像素走（行为判据）', () => {
  // 文本判据只能证明「代码里调了 mix(rgba)」。如果有人加一行提前 return，
  // 文本照样绿，而用户会看到「每次截图都被当成旧图，永远填不进去」。
  // 唯一能挡住这种改动的判据，是喂两份数据看结果是否不同。

  it('㉖ 同尺寸、只差一个像素 → 指纹必须不同', () => {
    const w = 16;
    const h = 16;
    const a = new Uint8Array(w * h * 4).fill(200);
    const b = new Uint8Array(w * h * 4).fill(200);
    b[0] = 10;
    expect(fingerprintOf(w, h, b), '指纹没跟着像素变：所有截图都会被视为同一张').not.toBe(
      fingerprintOf(w, h, a),
    );
    expect(fingerprintOf(w, h, a), '同一份数据两次结果不一致').toBe(fingerprintOf(w, h, a));
  });

  it('㉗ 尺寸不同 → 指纹不同（1080p 与 4K 不能被当成同一张）', () => {
    const px = new Uint8Array(8 * 8 * 4).fill(120);
    const taller = new Uint8Array(8 * 9 * 4).fill(120);
    expect(fingerprintOf(8, 9, taller)).not.toBe(fingerprintOf(8, 8, px));
  });

  it('㉘ 像素长度不足时不许崩溃（插件侧异常不该把编辑器带下去）', () => {
    expect(() => fingerprintOf(64, 64, new Uint8Array(8))).not.toThrow();
    expect(fingerprintOf(64, 64, new Uint8Array(8))).toContain('invalid');
  });
});
