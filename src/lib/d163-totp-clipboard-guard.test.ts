// D-163 守门：桌面端的「粘贴截图 / 截图」两个入口，以及粘贴监听脱离面板。
//
// 这一轮修的是一个"功能在、但用户找不到"的问题：粘贴监听原本要求**先点开扫码面板**，
// 而用户看到二维码的第一反应是当场截图、当场 Ctrl+V，不会去点一个他并不知道要点的按钮。
// 于是表现成「截图在剪贴板里，按了没反应」——和没做这个功能一模一样。
//
// 本文件钉的是几件容易在后续改动中被悄悄改坏的事：
//   ① 粘贴监听不得再依赖 `scanOpen`（否则又回到"被隐形开关挡着"）；
//   ② 只有真拿到图片才 `preventDefault`（否则用户往密码框粘文字会被吃掉）；
//   ③ `readImage()` 的返回是**未压缩 RGBA**，必须经 canvas 编成真 PNG
//      （插件文档那句 `new Blob([rgba])` 造出来的不是图片文件）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../components/ItemEditor.svelte',
    './clipboard-image.ts',
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

/** 以锚点为中心取所在的 `$effect` 块（文件里有多个 effect，按签名切会抓错）。 */
function effectAround(text: string, anchor: string): string {
  const at = text.indexOf(anchor);
  if (at < 0) {
    return '';
  }
  const start = text.lastIndexOf('$effect(', at);
  const end = text.indexOf('\n  });', at);
  return start < 0 || end < 0 ? '' : text.slice(start, end);
}

describe('D-163：粘贴监听必须脱离扫码面板', () => {
  it('① 粘贴 effect 内不出现 `scanOpen`（条件化＝功能被隐形开关挡住）', () => {
    const body = effectAround(src('/ItemEditor.svelte'), 'const onPaste = (event: ClipboardEvent)');
    expect(body, 'paste effect 切片失败').not.toBe('');
    expect(
      body.includes('scanOpen'),
      '粘贴又被绑回面板了：用户截图后直接按 Ctrl+V 会再次「没反应」',
    ).toBe(false);
    // 反向钉住：监听确实挂在 window 上，且真的注册了。
    expect(body).toContain("window.addEventListener('paste', onPaste)");
  });

  it('② `preventDefault` 必须晚于 `getAsFile()`（否则普通文本粘贴会被吃掉）', () => {
    const body = effectAround(src('/ItemEditor.svelte'), 'const onPaste = (event: ClipboardEvent)');
    const fileAt = body.indexOf('item.getAsFile()');
    const preventAt = body.indexOf('event.preventDefault()');
    expect(fileAt, '没有取文件').toBeGreaterThan(-1);
    expect(preventAt, '没有拦截默认粘贴').toBeGreaterThan(-1);
    expect(preventAt, '拦截必须建立在「确实拿到了图片」之后').toBeGreaterThan(fileAt);
  });

  it('③ 编辑器的粘贴路径与扫码共用同一条判定（不许各写一套）', () => {
    const editor = src('/ItemEditor.svelte');
    // 三条入口（粘贴 / 截图 / 从图片）最终都必须落到 ingestImage，
    // 而 ingestImage 只认 handleScannedText —— 判据与键映射只有一份。
    const ingestAt = editor.indexOf('async function ingestImage(');
    expect(ingestAt).toBeGreaterThan(-1);
    const ingestBody = section(editor, 'async function ingestImage(', '\n  }');
    expect(ingestBody).toContain('decodeQrFromBlob');
    expect(ingestBody).toContain('handleScannedText');
  });
});

describe('D-163：桌面端三个入口与平台分支', () => {
  it('④ 桌面分支同时有「从图片 / 粘贴截图 / 截图」', () => {
    const editor = src('/ItemEditor.svelte');
    const start = editor.indexOf('{#if clipboardImageSupported()}');
    expect(start, '没有按平台分支').toBeGreaterThan(-1);
    const elseAt = editor.indexOf('{:else}', start);
    const block = editor.slice(start, elseAt);
    for (const key of ['editor.totp_scan_image', 'editor.totp_paste', 'editor.totp_screenshot']) {
      expect(block, `桌面分支缺 ${key}`).toContain(key);
    }
  });

  it('⑤ 移动分支不给「粘贴 / 截图」（Android 读不了剪贴板位图，给了也是骗人）', () => {
    const editor = src('/ItemEditor.svelte');
    const start = editor.indexOf('{#if clipboardImageSupported()}');
    const elseAt = editor.indexOf('{:else}', start);
    const endAt = editor.indexOf('{/if}', elseAt);
    const block = editor.slice(elseAt, endAt);
    expect(block, '移动分支缺扫码').toContain('editor.totp_scan');
    expect(block, '移动分支缺从图片').toContain('editor.totp_scan_image');
    expect(block, '移动端不该出现粘贴截图').not.toContain('editor.totp_paste');
    expect(block, '移动端不该出现截图').not.toContain('editor.totp_screenshot');
  });

  it('⑦ 每种剪贴板结局都有可见文案（不许静默失败）', () => {
    const body = section(src('/ItemEditor.svelte'), 'async function consumeClipboard(', '\n  }');
    expect(body, 'consumeClipboard 切片失败').not.toBe('');
    for (const key of [
      'editor.paste_empty',
      'editor.paste_unsupported',
      'editor.paste_failed',
    ]) {
      expect(body, `结局缺文案：${key}`).toContain(key);
    }
  });
});

describe('D-163：`readImage()` 的返回值不是图片文件', () => {
  const lib = src('/clipboard-image.ts');

  it('⑧ 不许把 rgba 直接塞进 Blob（插件文档示例的误用）', () => {
    expect(lib, '`new Blob([rgba])` 造出来的不是图片文件').not.toContain('new Blob([');
    expect(lib, '必须经 ImageData 还原像素').toContain('new ImageData(');
    expect(lib, '必须编码成真 PNG 再交给解码器').toContain('toBlob(');
  });

  it('⑨ Android / iOS 必须排除（插件对 readImage 明确标注不支持）', () => {
    expect(lib).toContain('image.size()');
    expect(lib).toContain('image.rgba()');
    // 不用正则：`/Android|iPhone/` 这类竖线在多层文本传递中最容易被转义吃掉，
    // 而这里要的只是「两个词都在」，两条 toContain 更贴切也更抗折腾。
    expect(lib, '缺少 Android 排除：手机上会「点了没反应」').toContain('Android');
    expect(lib, '缺少 iOS 排除').toContain('iPhone');
  });
});

describe('D-163：四语言文案齐全', () => {
  const keys = [
    'totp_paste',
    'totp_screenshot',
    'paste_empty',
    'paste_unsupported',
    'paste_failed',
    'shot_unavailable',
    'shot_failed',
    'crop_hint',
  ];
  for (const locale of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
    it(`${locale} 覆盖本轮全部新键`, () => {
      const dict = src(`/i18n/${locale}.ts`);
      expect(dict, `${locale} 字典读取失败`).not.toBe('');
      for (const key of keys) {
        expect(dict, `${locale} 缺 ${key}`).toContain(`${key}:`);
      }
    });
  }
});
