// D-162：`otpauth://` 解析的行为测试。
//
// 这个文件的重心不是「正常路径能不能解析」——那是最容易自己写对的部分；
// 重心是**拒绝路径**：`digits=8` / `period=60` / `algorithm=SHA256` 的码必须被明确拦下。
//
// 理由（写在这里防后人删）：存量 TOTP 实现固定 6 位 / 30 秒 / SHA1。手输时代，
// 用户看到「网站要 8 位、密码管理器只出 6 位」会自己发现不对；扫码之后参数没人看，
// 结果就是安静地显示一个必然被拒的码。**"能不能解析"和"算出来对不对"是两件事**，
// 这个解析器的价值全在后者。

import { describe, expect, it } from 'vitest';

import { parseOtpauth, rejectMessage } from './otpauth';

/** 一个合法的 Base32 密钥（16 字符 = 80 bit，满足 Rust 侧 ≥10 字节的下限）。 */
const SECRET = 'JBSWY3DPEHPK3PXP';

describe('D-162 正常形态：能算对的码', () => {
  it('标准 Google 样式（label 带冒号 + issuer 同时在查询参数里）', () => {
    const result = parseOtpauth(
      `otpauth://totp/GitHub:alice?secret=${SECRET}&issuer=GitHub`,
    );
    expect(result.kind).toBe('ok');
    if (result.kind !== 'ok') {
      return;
    }
    expect(result.secret).toBe(SECRET);
    expect(result.issuer).toBe('GitHub');
    expect(result.account).toBe('alice');
  });

  it('issuer 只写在 label 前缀里（查询参数没有）→ 从 label 取', () => {
    const result = parseOtpauth(`otpauth://totp/ACME:john@example.com?secret=${SECRET}`);
    expect(result.kind).toBe('ok');
    if (result.kind !== 'ok') {
      return;
    }
    expect(result.issuer).toBe('ACME');
    expect(result.account).toBe('john@example.com');
  });

  it('label 与 issuer 参数不一致时以参数为准（参数是权威值，label 只是显示名）', () => {
    const result = parseOtpauth(`otpauth://totp/Old:alice?secret=${SECRET}&issuer=New`);
    expect(result.kind).toBe('ok');
    if (result.kind !== 'ok') {
      return;
    }
    expect(result.issuer).toBe('New');
  });

  it('label 只有账号、没有冒号 → issuer 为 null（不瞎猜）', () => {
    const result = parseOtpauth(`otpauth://totp/alice?secret=${SECRET}`);
    expect(result.kind).toBe('ok');
    if (result.kind !== 'ok') {
      return;
    }
    expect(result.issuer).toBeNull();
    expect(result.account).toBe('alice');
  });

  it('完全没有 label → account 为 null（标题不该被填成空串）', () => {
    const result = parseOtpauth(`otpauth://totp/?secret=${SECRET}`);
    expect(result.kind).toBe('ok');
    if (result.kind !== 'ok') {
      return;
    }
    expect(result.issuer).toBeNull();
    expect(result.account).toBeNull();
  });

  it('label 里的百分号编码被解码（空格与 @ 是常见形态）', () => {
    const result = parseOtpauth(`otpauth://totp/ACME%20Co:john%40example.com?secret=${SECRET}`);
    expect(result.kind).toBe('ok');
    if (result.kind !== 'ok') {
      return;
    }
    expect(result.issuer).toBe('ACME Co');
    expect(result.account).toBe('john@example.com');
  });

  it('显式写默认值（digits=6 / period=30 / algorithm=sha1 小写）仍算支持', () => {
    const result = parseOtpauth(
      `otpauth://totp/X:y?secret=${SECRET}&digits=6&period=30&algorithm=sha1`,
    );
    // 「没写」与「写成默认值」都必须放行 —— 拒绝的判据是取值范围，不是字段存在与否。
    expect(result.kind).toBe('ok');
  });

  it('大小写混写的 scheme 与类型都能认（OTPAUTH://TOTP）', () => {
    const result = parseOtpauth(`OTPAUTH://TOTP/x?secret=${SECRET}`);
    expect(result.kind).toBe('ok');
  });

  it('secret 前后有空白时 trim（复制粘贴常带换行）', () => {
    const result = parseOtpauth(`otpauth://totp/x?secret=%20${SECRET}%20`);
    expect(result.kind).toBe('ok');
    if (result.kind !== 'ok') {
      return;
    }
    expect(result.secret).toBe(SECRET);
  });
});

describe('D-162 拒绝形态：算出来必然对不上的码', () => {
  it('digits=8 → 拒绝，detail 带回 8（文案要说清是 8 位还是别的）', () => {
    const result = parseOtpauth(`otpauth://totp/X:y?secret=${SECRET}&digits=8`);
    expect(result).toEqual({ kind: 'rejected', reason: 'digits', detail: '8' });
  });

  it('period=60 → 拒绝', () => {
    const result = parseOtpauth(`otpauth://totp/X:y?secret=${SECRET}&period=60`);
    expect(result).toEqual({ kind: 'rejected', reason: 'period', detail: '60' });
  });

  it('algorithm=SHA256 → 拒绝，detail 保留原始大小写便于展示', () => {
    const result = parseOtpauth(`otpauth://totp/X:y?secret=${SECRET}&algorithm=SHA256`);
    expect(result).toEqual({ kind: 'rejected', reason: 'algorithm', detail: 'SHA256' });
  });

  it('algorithm=MD5 也拒绝（只认 SHA1，不因为"听说过 MD5"就放行）', () => {
    const result = parseOtpauth(`otpauth://totp/X:y?secret=${SECRET}&algorithm=MD5`);
    expect(result.kind).toBe('rejected');
  });

  it('digits 写成非数字 → 拒绝（detail 保留原文；NaN !== 6 也走这条）', () => {
    const result = parseOtpauth(`otpauth://totp/X:y?secret=${SECRET}&digits=abc`);
    expect(result).toEqual({ kind: 'rejected', reason: 'digits', detail: 'abc' });
  });

  it('hotp:// → 拒绝（计数器式没有时间维度，不能假装支持）', () => {
    const result = parseOtpauth(`otpauth://hotp/X:y?secret=${SECRET}&counter=1`);
    expect(result).toEqual({ kind: 'rejected', reason: 'hotp', detail: 'hotp' });
  });

  it('没有 secret → 拒绝', () => {
    const result = parseOtpauth('otpauth://totp/X:y?issuer=X');
    expect(result).toEqual({ kind: 'rejected', reason: 'no_secret', detail: '' });
  });

  it('secret 只有空白 → 同样算没有（不能把空格当密钥存进去）', () => {
    const result = parseOtpauth('otpauth://totp/X:y?secret=%20%20');
    expect(result.kind).toBe('rejected');
  });

  it('普通网址不是两步验证码 → not_otpauth', () => {
    expect(parseOtpauth('https://github.com/login')).toEqual({
      kind: 'rejected',
      reason: 'not_otpauth',
      detail: '',
    });
  });

  it('空串 / 纯文本 → not_otpauth（条形码、Wi-Fi 码都走这条）', () => {
    expect(parseOtpauth('').kind).toBe('rejected');
    expect(parseOtpauth('WIFI:S:home;T:WPA;P:secret;;').kind).toBe('rejected');
  });

  it('otpauth:// 但类型既不是 totp 也不是 hotp → 拒绝，detail 带上类型名', () => {
    const result = parseOtpauth(`otpauth://steam/X:y?secret=${SECRET}`);
    expect(result).toEqual({ kind: 'rejected', reason: 'not_otpauth', detail: 'steam' });
  });
});

describe('D-162 拒绝文案与判据同源', () => {
  it('每种拒绝原因都映射到一个 i18n 键，且参数名与文案占位符对得上', () => {
    expect(rejectMessage({ kind: 'rejected', reason: 'digits', detail: '8' })).toEqual({
      key: 'editor.scan_digits',
      params: { n: '8' },
    });
    expect(rejectMessage({ kind: 'rejected', reason: 'period', detail: '60' })).toEqual({
      key: 'editor.scan_period',
      params: { n: '60' },
    });
    expect(rejectMessage({ kind: 'rejected', reason: 'algorithm', detail: 'SHA256' })).toEqual({
      key: 'editor.scan_algorithm',
      params: { algo: 'SHA256' },
    });
    expect(rejectMessage({ kind: 'rejected', reason: 'hotp', detail: 'hotp' }).key).toBe(
      'editor.scan_hotp',
    );
    expect(rejectMessage({ kind: 'rejected', reason: 'no_secret', detail: '' }).key).toBe(
      'editor.scan_no_secret',
    );
    // not_otpauth 刻意不带参数：不回显扫到的内容（那可能是任意私密串）。
    expect(rejectMessage({ kind: 'rejected', reason: 'not_otpauth', detail: 'steam' })).toEqual({
      key: 'editor.scan_not_otpauth',
    });
  });

  it('粘贴场景只换措辞（"二维码"→"链接"），判据与参数完全不变', () => {
    expect(
      rejectMessage({ kind: 'rejected', reason: 'not_otpauth', detail: '' }, 'paste'),
    ).toEqual({ key: 'editor.link_not_otpauth' });
    expect(rejectMessage({ kind: 'rejected', reason: 'hotp', detail: 'hotp' }, 'paste')).toEqual({
      key: 'editor.link_hotp',
    });
    // 这三条文案本身不含"二维码"字样，两种场景都通顺，刻意不分叉（少两份文案 = 少两处漂移）。
    expect(rejectMessage({ kind: 'rejected', reason: 'digits', detail: '8' }, 'paste')).toEqual({
      key: 'editor.scan_digits',
      params: { n: '8' },
    });
    expect(rejectMessage({ kind: 'rejected', reason: 'period', detail: '60' }, 'paste').key).toBe(
      'editor.scan_period',
    );
    expect(
      rejectMessage({ kind: 'rejected', reason: 'algorithm', detail: 'SHA256' }, 'paste').key,
    ).toBe('editor.scan_algorithm');
  });
});
