// D-97 UI 守卫：TOTP 密钥入库前校验 + otpauth 链接提取 + 详情页错误态。
//
// 组成：lib 层真单测（normalizeTotpInput / isValidBase32 主例回归）
// + 源码结构断言（与 d88-d90-ui-guard / d85-ui-guard 同族，无 DOM 环境）。

import { describe, expect, it } from 'vitest';

import { isValidBase32, normalizeTotpInput } from './totp';

const modules = import.meta.glob(['../components/*.svelte', './i18n/*.ts'], {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('D-97（C）：normalizeTotpInput', () => {
  it('普通 Base32 密钥原样返回（仅 trim）', () => {
    expect(normalizeTotpInput('JBSWY3DPEHPK3PXP')).toBe('JBSWY3DPEHPK3PXP');
    expect(normalizeTotpInput('  jbswy3dpehpk3pxp  ')).toBe('jbswy3dpehpk3pxp');
  });

  it('otpauth:// 链接自动抽出 secret 参数', () => {
    const uri = 'otpauth://totp/GitHub:user?secret=JBSWY3DPEHPK3PXP&issuer=GitHub';
    expect(normalizeTotpInput(uri)).toBe('JBSWY3DPEHPK3PXP');
  });

  it('链接带空格 / 大写 scheme 也能提取', () => {
    const uri = '  OTPAUTH://totp/acme?secret=jbswy3dpehpk3pxp&algorithm=SHA1  ';
    expect(normalizeTotpInput(uri)).toBe('jbswy3dpehpk3pxp');
  });

  it('链接缺 secret 参数时原样返回（交给保存时试算拦截）', () => {
    expect(normalizeTotpInput('otpauth://totp/nosecret?issuer=x')).toBe(
      'otpauth://totp/nosecret?issuer=x',
    );
  });

  it('非链接输入不误伤（含 otpauth 字样的普通文本也原样）', () => {
    expect(normalizeTotpInput('qwer242411321')).toBe('qwer242411321');
  });
});

describe('D-97（A）：isValidBase32 主例回归', () => {
  it('主人的 test 条目输入（含 1）必须判非法', () => {
    // 1 不在 Base32 字母表（A-Z、2-7）内——正是 D-97 现场「反复弹输入无效」的根因。
    expect(isValidBase32('qwer242411321')).toBe(false);
    expect(isValidBase32('ABC0')).toBe(false); // 0 非法
    expect(isValidBase32('JBSWY3DPEHPK3PXP')).toBe(true);
  });
});

describe('D-97（A）：编辑器入库前校验（源码结构断言）', () => {
  const src = pickSource('ItemEditor.svelte');

  it('save() 必须先 normalizeTotpInput 再试算 currentTotp，ERR_VALIDATION 拦截保存', () => {
    expect(src).toContain('normalizeTotpInput(totp)');
    const saveAt = src.indexOf('async function save()');
    expect(saveAt).toBeGreaterThan(-1);
    const body = src.slice(saveAt, src.indexOf('</script>'));
    expect(body).toContain('await currentTotp(totpSecret)');
    // 拦截路径：判 ERR_VALIDATION → 置错误态 → return（阻止落库）。
    const guardAt = body.indexOf('ErrorCode.ERR_VALIDATION');
    const invalidSetAt = body.indexOf('totpInvalid = true;', guardAt);
    const returnAfter = body.indexOf('return;', invalidSetAt);
    expect(invalidSetAt).toBeGreaterThan(guardAt);
    expect(returnAfter).toBeGreaterThan(invalidSetAt);
  });

  it('草稿 totp_secret 必须取校验后的 totpSecret（不再直接读原始输入）', () => {
    expect(src).toContain('totp_secret: totpSecret');
    expect(src).not.toContain('totp_secret: totp.trim()');
  });

  it('校验失败必须渲染行内错误（err.totp_invalid），不得静默', () => {
    expect(src).toContain("ui.t('err.totp_invalid')");
    expect(src).toContain('aria-invalid={totpInvalid}');
  });
});

describe('D-97（B）：详情页错误态（源码结构断言）', () => {
  const src = pickSource('ItemDetail.svelte');

  it('取码失败进入 totpInvalid 错误态并停止重试循环，不得每窗口弹 toast', () => {
    expect(src).toContain('if (cancelled || totpInvalid)');
    expect(src).toContain('totpInvalid = true;');
  });

  it('错误态必须渲染可见的一行字（err.totp_invalid_detail），不渲染 = 伪装成「本来就没有」', () => {
    expect(src).toContain("ui.t('err.totp_invalid_detail')");
    const invalidAt = src.indexOf('{#if totpInvalid}');
    expect(invalidAt).toBeGreaterThan(-1);
    // 错误态分支里不得再显示倒计时（B 要求：无有效验证码不显示倒计时）。
    const branchEnd = src.indexOf('{:else}', invalidAt);
    const errorBranch = src.slice(invalidAt, branchEnd);
    expect(errorBranch).not.toContain('totpRemaining');
    expect(errorBranch).not.toContain('formatCode(totpCode)');
  });

  it('切换条目时复位 totpInvalid', () => {
    const resetAt = src.indexOf("const id = item?.item_id ?? null;");
    const effectBody = src.slice(resetAt, src.indexOf('});', resetAt));
    expect(effectBody).toContain('totpInvalid = false;');
  });
});

describe('D-97：i18n 两键双语齐备，zh 文案必须点名 Base32 字符范围', () => {
  const zh = pickSource('zh-CN.ts');
  const en = pickSource('en-US.ts');

  it('err.totp_invalid / err.totp_invalid_detail 双语齐备', () => {
    for (const lang of [zh, en]) {
      expect(lang).toContain('totp_invalid:');
      expect(lang).toContain('totp_invalid_detail:');
    }
  });

  it('zh 文案点名 Base32 字母表范围（不含 0/1/8/9）——只说「输入无效」用户不知道改什么', () => {
    const at = zh.indexOf('totp_invalid:');
    // 文案值可能换行（prettier 折行），取键起 3 行范围断言。
    const block = zh.slice(at, zh.indexOf('\n', zh.indexOf('\n', at) + 1));
    expect(block).toContain('A-Z');
    expect(block).toContain('2-7');
    expect(block).toContain('0/1/8/9');
    expect(block).toContain('otpauth://');
  });
});
