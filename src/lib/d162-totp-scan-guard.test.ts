// D-162 守门：两步验证二维码扫码 / 图片导入。
//
// 这个功能有一件事比"能不能扫出来"重要得多：**支持的参数范围外，绝不把密钥写进表单**。
//
// 存量 TOTP 计算固定 6 位 / 30 秒 / SHA1（`crates/passwordms-core/src/constants.rs`）。
// 扫到一个 `digits=8` 的码时，如果只是抽出 secret 填进去，用户会得到一条
// 「存得下、算出来必然对不上」的记录 —— 而他不知道为什么，因为扫码这一步看起来完全成功了。
// 手输时代这个错误会自己暴露（网站要 8 位、这里只显示 6 位）；扫码之后没人再看参数。
//
// 所以本文件钉的是：
//   ① 拒绝分支必须在写 `totp` **之前**返回（顺序错了就是上面那个坏结局）；
//   ② 非 otpauth 的码返回 false 继续扫（视野里常有别的二维码，一律报错会让人以为坏了）；
//   ③ 标题只在为空时自动填（不覆盖用户已写的命名）；
//   ④ 摄像头统一走 `qr-camera`，且卸载时必须释放（原先配对页漏了，灯一直亮）；
//   ⑤ 四语言文案齐全，`{n}` / `{algo}` 占位符在位（缺一个就是给某语言用户少说半句）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../components/ItemEditor.svelte',
    '../components/sync/PairScan.svelte',
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

describe('D-162：扫码结果的处理顺序', () => {
  it('① 拒绝分支在写 `totp` 之前返回（顺序反了＝存进一个必然算错的密钥）', () => {
    const editor = src('/ItemEditor.svelte');
    const body = section(editor, 'function handleScannedText(', '\n  }');
    expect(body, 'handleScannedText 切片失败').not.toBe('');
    const rejectAt = body.indexOf('totpHint = rejectMessage(parsed);');
    const assignAt = body.indexOf('totp = parsed.secret;');
    expect(rejectAt, '没有走 rejectMessage（文案与判据必须同源）').toBeGreaterThan(-1);
    expect(assignAt, '没有把 secret 写进表单').toBeGreaterThan(-1);
    expect(rejectAt, '拒绝分支落在写 totp 之后 —— 拒绝的码会被写进表单').toBeLessThan(assignAt);
    // 拒绝之后必须真的离开这个函数，否则会继续往下执行赋值。
    expect(body.slice(rejectAt, assignAt), '拒绝分支没有 return').toContain('return true;');
  });

  it('② 非 otpauth 的码返回 false（继续扫），且不弹错误', () => {
    const editor = src('/ItemEditor.svelte');
    const body = section(editor, 'function handleScannedText(', '\n  }');
    expect(body, 'handleScannedText 切片失败').not.toBe('');
    // 前缀判断必须存在且返回 false —— 视线里别的二维码不该中断扫描。
    expect(body).toContain('startsWith(');
    expect(body).toContain('return false;');
    const prefixAt = body.indexOf('startsWith(');
    const firstReturnFalse = body.indexOf('return false;');
    expect(prefixAt).toBeGreaterThan(-1);
    expect(firstReturnFalse, '前缀判断之后没有返回 false').toBeGreaterThan(prefixAt);
  });

  it('③ 标题只在为空时自动填，不覆盖用户已经写的名字', () => {
    const editor = src('/ItemEditor.svelte');
    const body = section(editor, 'function handleScannedText(', '\n  }');
    expect(body, 'handleScannedText 切片失败').not.toBe('');
    expect(body).toContain("if (title.trim() === '')");
    // 自动填的值来自 issuer / account（解析器给出），不是凭空拼字符串。
    expect(body).toContain('parsed.issuer');
    expect(body).toContain('parsed.account');
  });

  it('④ 扫码入口与保存路径不脱钩：仍走 normalizeTotpInput + Rust 试算（不新开旁路）', () => {
    const editor = src('/ItemEditor.svelte');
    // 扫码只负责把 secret 填进输入框；入库前的真校验必须还是原来那条路。
    expect(editor).toContain('normalizeTotpInput(raw)');
    expect(editor).toContain('await currentTotp(totpSecret);');
  });

  it('⑤ 粘贴 otpauth 整串走**同一套**参数判定（不许"扫进来不行、粘进来能存"）', () => {
    const editor = src('/ItemEditor.svelte');
    const save = section(editor, 'async function save(', '\n  }');
    expect(save, 'save 切片失败').not.toBe('');
    // 粘贴路径必须先经过 parseOtpauth —— 只抽 secret 会放过 digits=8 / period=60。
    const parseAt = save.indexOf('parseOtpauth(raw)');
    const normalizeAt = save.indexOf('normalizeTotpInput(raw)');
    expect(parseAt, '粘贴路径没有过参数判定').toBeGreaterThan(-1);
    expect(normalizeAt, '裸 Base32 的手输路径被删了').toBeGreaterThan(-1);
    // 判定在前、回退到"原样当 secret"在后：顺序反了等于先放行再检查。
    expect(parseAt).toBeLessThan(normalizeAt);
    // 拒绝时用 paste 措辞（同一个 rejectMessage，只是场景不同），并且不落库。
    expect(save).toContain("rejectMessage(parsed, 'paste')");
    expect(save.slice(save.indexOf("rejectMessage(parsed, 'paste')"))).toContain('return;');

    // 两个入口都调 parseOtpauth：同一份判据只有一处实现。
    const calls = editor.match(/parseOtpauth\(/g) ?? [];
    expect(calls.length, '两个入口应当共用 parseOtpauth').toBeGreaterThanOrEqual(2);
  });
});

describe('D-162：两个入口都真的接上了', () => {
  it('有「扫码」与「从图片」两个按钮，都落在同一套处理上', () => {
    const editor = src('/ItemEditor.svelte');
    expect(editor).toContain(`ui.t('editor.totp_scan')`);
    expect(editor).toContain(`ui.t('editor.totp_scan_image')`);
    // 图片入口触发隐藏的 file input（桌面与 Android 都走系统选择器，零插件权限）。
    expect(editor).toContain('fileInput?.click()');
    expect(editor).toContain('accept="image/*"');
    expect(editor).toContain('pickImage');
    // 图片解码只在 qr.ts 里做一次，编辑器不重复实现像素处理。
    expect(editor).toContain('decodeQrFromBlob');
  });

  it('摄像头走统一模块，且卸载时释放（D-162 修的既有泄漏）', () => {
    const editor = src('/ItemEditor.svelte');
    expect(editor).toContain('startCameraScan(');
    // $effect 必须返回清理函数并在里面 stop —— 否则摄像头指示灯常亮。
    // 用注释锚定到「摄像头那个 effect」：文件里现在有两个 `$effect`（粘贴、摄像头），
    // 只按 `$effect((): (() => void) => {` 切片会抓到先出现的粘贴那个（本轮踩过一次）。
    // close 必须带行首 `\n`：只写 `  });` 会命中 `    });` 内部的子串。
    const effect = section(editor, '// 面板展开时开摄像头', '\n  });');
    expect(effect, '摄像头 effect 切片失败').not.toBe('');
    expect(effect).toContain('scan?.stop()');
    // 授权期间被关掉的兜底：cancelled 后再到手的 stream 也要还回去。
    expect(effect).toContain('cancelled');
    expect(effect).toContain('handle.stop()');
  });

  it('粘贴截图这条路真的接了（文案说了"可以粘贴"，代码就得能做）', () => {
    const editor = src('/ItemEditor.svelte');
    expect(editor).toContain("addEventListener('paste'");
    expect(editor).toContain('getAsFile()');
    // 没有图片时必须放行 —— 抢走别人的粘贴比不响应更烦人。
    expect(editor).toContain('event.preventDefault()');
    // 监听器必须成对移除，否则每开关一次面板就多挂一个。
    expect(editor).toContain("removeEventListener('paste'");
  });

  it('配对页也改用统一模块，且不再自己碰 getUserMedia', () => {
    const pair = src('/PairScan.svelte');
    expect(pair).toContain('startCameraScan(');
    expect(pair, '配对页仍在手写设备管理（两处会漂移）').not.toContain('getUserMedia');
    // 卸载清理：stopScan 里一次、$effect 清理里一次。
    const stops = pair.match(/scan\?\.stop\(\)/g) ?? [];
    expect(stops.length, '配对页缺少卸载清理').toBeGreaterThanOrEqual(2);
  });
});

describe('D-162：四语言文案', () => {
  const KEYS = [
    'totp_scan',
    'totp_scan_image',
    'scan_hint',
    'scan_no_qr',
    'scan_not_otpauth',
    'scan_hotp',
    'scan_no_secret',
    'scan_digits',
    'scan_period',
    'scan_algorithm',
    'scan_camera_denied',
  ];

  it('11 个键在四种语言里都存在', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const text = src(`/i18n/${lang}.ts`);
      for (const key of KEYS) {
        expect(text, `${lang} 缺 ${key}`).toContain(`${key}:`);
      }
    }
  });

  it('带数字的文案都留了 {n}，算法文案留了 {algo}（缺占位符就变成"用 位验证码"）', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const text = src(`/i18n/${lang}.ts`);
      expect(text, `${lang} 的 scan_digits 缺 {n}`).toMatch(/scan_digits: '[^']*\{n\}[^']*'/);
      expect(text, `${lang} 的 scan_period 缺 {n}`).toMatch(/scan_period: '[^']*\{n\}[^']*'/);
      expect(text, `${lang} 的 scan_algorithm 缺 {algo}`).toMatch(
        /scan_algorithm: '[^']*\{algo\}[^']*'/,
      );
    }
  });
});
