// D-165 守门：截图必须拿到一张**冻结**的画面。
//
// 这一轮修的是一句错误的推断。1.5.20 把窗口藏起来、唤起系统截图工具，然后
// 相信「覆盖层一出现屏幕就冻结了」，1.2 秒后还原窗口 —— 实测覆盖层是**活画面**，
// 窗口一回来就出现在框里，用户还没开始框，图里已经有 PasswordMS 了。
//
// 现在的做法是把顺序倒过来：**先抓，再还原**。抓下来的像素不会变，所以窗口
// 什么时候回来都不影响框到的内容。本文件钉住这条不变量，以及它的三个落点：
//   ① 抓屏必须发生在窗口隐藏之后、还原之前；
//   ② 框选层画的是那张抓下来的位图，不是任何实时画面；
//   ③ 框选坐标按 canvas 的**实际渲染尺寸**换算（不是舞台尺寸，两者不相等）。
//
// 另外钉住两条容易被"优化"掉的东西：误触不许送空图去解码、识别失败不许收框选层
// （重来要多付一次整屏抓取），以及 33MB 的像素内存必须在三条路径上都释放。

import { describe, expect, it, vi } from 'vitest';

// 只测纯函数：挡掉 IPC 运行时，避免把 Tauri 拉进来。
vi.mock('./ipc', () => ({ system: {} }));

import { clampRect, MIN_CROP_PX } from './screen-capture';

const raw = import.meta.glob(
  [
    '../components/ItemEditor.svelte',
    '../components/ScreenCrop.svelte',
    './screen-capture.ts',
    '../../src-tauri/src/commands/system_cmd.rs',
    './i18n/zh-CN.ts',
    './i18n/zh-TW.ts',
    './i18n/en-US.ts',
    './i18n/th.ts',
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
const crop = src('/ScreenCrop.svelte');
const shot = section(editor, 'async function startScreenshot(', '\n  }');

describe('D-165：抓屏必须发生在「藏起来之后、还原之前」', () => {
  it('① startScreenshot 切片成功（切片失败会让后面全假绿）', () => {
    expect(shot, 'startScreenshot 切片失败').not.toBe('');
    expect(shot).toContain('captureScreen()');
  });

  it('② 顺序：hide → 等窗口消失 → 抓屏 → show', () => {
    const hideAt = shot.indexOf('win.hide()');
    const settleAt = shot.indexOf('HIDE_SETTLE_MS');
    const captureAt = shot.indexOf('captureScreen()');
    const showAt = shot.indexOf('win.show()');
    expect(hideAt, '截图前没藏窗口：抓到的就是自己').toBeGreaterThan(-1);
    expect(settleAt, '藏完立刻抓：抓到的是还没消失的那一帧').toBeGreaterThan(hideAt);
    expect(captureAt, '没有抓屏').toBeGreaterThan(settleAt);
    expect(showAt, '没有还原窗口').toBeGreaterThan(-1);
    // 这条是整轮的核心：抓到像素**之后**才还原，画面因此不会跟着窗口变。
    expect(captureAt, '窗口在抓屏之前就回来了：画面里的窗口会被框进去').toBeLessThan(showAt);
  });

  it('③ 还原必须在 finally 里（藏起来回不去的窗口比截图失败严重得多）', () => {
    const finallyAt = shot.indexOf('} finally {');
    expect(finallyAt, '没有 finally：抓屏抛错时窗口会一直藏着').toBeGreaterThan(-1);
    const after = shot.slice(finallyAt);
    expect(after, 'finally 里没有还原窗口').toContain('win.show()');
    expect(after, '还原后没有取回焦点：用户还得再点一下').toContain('win.setFocus()');
    expect(after.indexOf('win.show()'), '条件判断之前就 show，条件形同虚设').toBeGreaterThan(
      after.indexOf('if (hidden)'),
    );
  });

  it('④ 抓屏失败要有话说（不许「点了没反应」）', () => {
    expect(shot, '没处理抓屏失败').toContain("result.kind !== 'ok'");
    expect(shot, '失败没给提示').toContain('editor.shot_failed');
    expect(shot, '平台不支持与抓屏失败混成一句').toContain('editor.shot_unavailable');
  });
});

describe('D-165：框选层画的是那张冻结的位图', () => {
  it('⑤ 画面来源只有一个：抓下来的 bitmap', () => {
    expect(crop, '没有把快照画进 canvas').toContain('drawImage(shot.bitmap');
    // 反向钉住：不许在框选层里再开一路实时画面（摄像头 / 再次抓屏）。
    expect(crop, '框选层不该再抓一次屏：那是另一个时刻的画面').not.toContain('captureScreen');
    expect(crop, '框选层不该开摄像头').not.toContain('getUserMedia');
  });

  it('⑥ 坐标换算按 canvas 的实际渲染框，而不是舞台尺寸', () => {
    expect(crop, '取的是 canvas 的渲染框').toContain('const box = el.getBoundingClientRect();');
    expect(crop, '映射没有按渲染尺寸/图像尺寸的比例缩放').toContain('box.width');
    // 框选块的定位同样必须按 canvas 的真实位置换算 —— 图片在舞台里是居中缩放的，
    // 用舞台尺寸会把框整体放大并偏移（本轮实际踩过的形态）。
    expect(crop, '框选块没按 canvas 相对舞台的偏移换算').toContain('shotBox.left - stageBox.left');
  });

  it('⑦ Esc 必须能取消（全屏遮罩没有别的出口）', () => {
    const effect = section(crop, 'window.addEventListener(\'keydown\', onKey);', '\n  });');
    expect(effect, 'Esc 监听切片失败').not.toBe('');
    expect(crop, "Esc 没有接住").toContain("event.key !== 'Escape'");
    expect(crop, '取消没接上').toContain('oncancel()');
    expect(crop, '卸载没摘监听').toContain("removeEventListener('keydown', onKey)");
  });

  it('⑧ 误触不送空图去解码（点一下、拖一条线）', () => {
    const up = section(crop, 'function onPointerUp(', '\n  }');
    expect(up, 'onPointerUp 切片失败').not.toBe('');
    expect(up, '没做合法性判定').toContain('clampRect(');
    const guardAt = up.indexOf('if (picked === null)');
    const pickAt = up.indexOf('onpick(picked)');
    expect(guardAt, '缺少「框选不成立」的分支').toBeGreaterThan(-1);
    expect(pickAt, '不成立的框也交给了上层').toBeGreaterThan(guardAt);
    // 该分支必须 return，不能只是清一下然后继续往下走。
    expect(up.slice(guardAt, pickAt), '不成立的框没有 return').toContain('return');
  });
});

describe('D-165：识别失败不收框选层', () => {
  it('⑨ 只有真的填进去了才关闭（否则用户要重付一次整屏抓取）', () => {
    const pick = section(editor, 'async function onCropPick(', '\n  }');
    expect(pick, 'onCropPick 切片失败').not.toBe('');
    const condAt = pick.indexOf('if (await ingestImage(blob))');
    expect(condAt, '关闭框选层没看识别结果：框偏一次就得重来').toBeGreaterThan(-1);
    const closeAt = pick.indexOf('closeCrop()', condAt);
    expect(closeAt, '成功路径没关闭框选层').toBeGreaterThan(condAt);
  });

  it('⑩ 像素内存在三条路径上都释放（4K 一张 33MB，不能等 GC）', () => {
    const close = section(editor, 'function closeCrop(', '\n  }');
    expect(close, 'closeCrop 切片失败').not.toBe('');
    expect(close, '取消/成功后没释放位图').toContain('disposeShot(');

    const unmount = section(editor, 'return () => {\n      disposeShot(', '\n    };');
    expect(unmount, '卸载清理切片失败').not.toBe('');
    expect(unmount, '离开编辑器没释放位图').toContain('disposeShot(');

    // 连续两次截图时，旧快照必须先放掉，否则每点一次就多留 33MB。
    const replaceAt = shot.indexOf('disposeShot(cropShot)');
    const assignAt = shot.indexOf('cropShot = result.shot');
    expect(replaceAt, '新快照来了但旧的没释放').toBeGreaterThan(-1);
    expect(assignAt, '先赋值再释放等于释放掉了新图').toBeGreaterThan(replaceAt);
  });
});

describe('D-165：Rust 侧抓屏（GDI）', () => {
  const rust = src('/commands/system_cmd.rs');

  it('⑪ 用 BitBlt 拷贝屏幕，并且整块虚拟桌面', () => {
    expect(rust, '没有 capture_screen 命令').toContain('pub fn capture_screen()');
    expect(rust, '没有真实抓屏调用').toContain('BitBlt(');
    expect(rust, '只抓主屏：多显示器时另一块屏是黑的').toContain('SM_CXVIRTUALSCREEN');
    expect(rust, '取像素没有用 GetDIBits').toContain('GetDIBits(');
  });

  it('⑫ 行序必须自顶向下（否则整屏上下翻转，框选位置全错）', () => {
    expect(rust, 'biHeight 没取负：DIB 会按自底向上排列').toContain('biHeight: -high');
  });

  it('⑬ 返回前把 GDI 的 BGRA 换成 RGBA', () => {
    expect(rust, '颜色通道没换：红蓝互换，界面上会是一片偏色的图').toContain(
      'px.swap(0, 2);',
    );
  });

  it('⑭ 不再唤起系统截图工具（那条路的覆盖层是活画面）', () => {
    // 只钉「调用」而不是「这个词」：文档里还要解释为什么不用它，
    // 把整段说明也判红，等于逼下一个人删掉理由。
    expect(rust, '还在调 ms-screenclip').not.toContain('"ms-screenclip:"');
    expect(editor, '前端还在调旧的截图命令').not.toContain('openScreenshotTool');
  });
});

describe('D-165：四语言文案齐全', () => {
  for (const locale of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
    it(`${locale} 有框选提示与抓屏失败文案`, () => {
      const dict = src(`/i18n/${locale}.ts`);
      expect(dict, `${locale} 字典读取失败`).not.toBe('');
      for (const key of ['crop_hint', 'shot_failed', 'shot_unavailable']) {
        expect(dict, `${locale} 缺 ${key}`).toContain(`${key}:`);
      }
    });
  }
});

// ── 行为判据：clampRect ───────────────────────────────────────────────────────
//
// 上面全是文本判据，它们只能证明「代码里调了 clampRect」。真正决定用户看到什么的
// 是它算出来的数：拖到图外、反向拖、拖成一条线 —— 只有喂坐标才测得出。
describe('D-165：框选区域换算（行为）', () => {
  it('⑮ 拖出图像右边 / 下边 → 裁到边界，不产生越界区域', () => {
    const out = clampRect({ x: 90, y: 80, width: 100, height: 100 }, 120, 100);
    expect(out).not.toBeNull();
    expect(out).toEqual({ x: 90, y: 80, width: 30, height: 20 });
  });

  it('⑯ 反向拖（宽高是负数）→ 归一成正矩形', () => {
    const out = clampRect({ x: 100, y: 100, width: -40, height: -60 }, 200, 200);
    expect(out).toEqual({ x: 60, y: 40, width: 40, height: 60 });
  });

  it('⑰ 完全在图外 → null（不许把空区域送去解码）', () => {
    expect(clampRect({ x: 300, y: 300, width: 50, height: 50 }, 200, 200)).toBeNull();
  });

  it('⑱ 小于阈值 → null（点一下、拖一条线都是误触）', () => {
    expect(clampRect({ x: 10, y: 10, width: MIN_CROP_PX - 1, height: 40 }, 200, 200)).toBeNull();
    expect(clampRect({ x: 10, y: 10, width: 40, height: MIN_CROP_PX - 1 }, 200, 200)).toBeNull();
    expect(clampRect({ x: 10, y: 10, width: MIN_CROP_PX, height: MIN_CROP_PX }, 200, 200)).toEqual({
      x: 10,
      y: 10,
      width: MIN_CROP_PX,
      height: MIN_CROP_PX,
    });
  });

  it('⑲ 小数坐标取整 → 裁切不出现半像素（canvas 会四舍五入，尺寸对不上会错位一行）', () => {
    const out = clampRect({ x: 10.4, y: 20.6, width: 50.2, height: 30.9 }, 200, 200);
    expect(out).toEqual({ x: 10, y: 21, width: 51, height: 31 });
  });
});
