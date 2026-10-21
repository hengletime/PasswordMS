// `syncButtonState` 单测（D-66）。
//
// 守的是什么：「立即同步」的忙碌态必须**按设备隔离** —— 点 A 台只让 A 台转圈，
// B 台只置灰不转圈。改动前用 store 级单一布尔，点一台两台一起转，用户分不清。
// 同时守反向：整体忙碌期间**所有**按钮都不可点（禁止并发重同步）。

import { describe, expect, it } from 'vitest';

import { fingerprintText, hasSessionCode, syncButtonState } from './device-label';

describe('D-66：「立即同步」按钮忙碌态按设备隔离', () => {
  it('全空闲时所有设备行都可点、都不转圈', () => {
    expect(syncButtonState(null, 'dev-a')).toEqual({ syncing: false, disabled: false });
    expect(syncButtonState(null, 'dev-b')).toEqual({ syncing: false, disabled: false });
  });

  it('正在同步的那台：转圈 + 禁点', () => {
    expect(syncButtonState('dev-a', 'dev-a')).toEqual({ syncing: true, disabled: true });
  });

  it('没被点的那台：只置灰、**不转圈**（这是本缺陷的核心判据）', () => {
    expect(syncButtonState('dev-a', 'dev-b')).toEqual({ syncing: false, disabled: true });
  });

  it('三台设备同屏时只有一台转圈，其余两台置灰', () => {
    const ids = ['dev-a', 'dev-b', 'dev-c'];
    const states = ids.map((id) => syncButtonState('dev-b', id));
    expect(states.filter((s) => s.syncing)).toHaveLength(1);
    expect(states[1].syncing).toBe(true);
    expect(states.filter((s) => s.disabled)).toHaveLength(3);
  });

  it('空字符串设备 id 不误命中（不因空 id 等于空 id 而转圈）', () => {
    expect(syncButtonState('dev-a', '')).toEqual({ syncing: false, disabled: true });
    expect(syncButtonState('', '')).toEqual({ syncing: false, disabled: false });
    expect(syncButtonState('', 'dev-a')).toEqual({ syncing: false, disabled: false });
  });
});

// D-67 / D-69：设备行的「指纹」必须是真指纹（sha256(pubkey) 前 8 字节 = 16 位小写 hex），
// 不是 device_id（36 位 UUID）。UUID 冒充指纹 = 安全提示在说谎：它没有自校验含义，
// 中间人隔开两端时用户照着比对也永远「一致」。
//
// D-69 把「空指纹回退 device_id」这条兜底**删掉了**：空串 = 不知道对端公钥，
// 此时显示 UUID 等于凭空造一个看起来安全的值。现在的契约是「空值返回空串，
// 由 `hasSessionCode()` 判空 → 整行不渲染」。下方断言相应改为**强断言**：
// 任何空值都不得返回 UUID 形状的文本（而不只是「长度大于 0」）。
describe('D-67 / D-69：设备行指纹取值', () => {
  const UUID = '3dd4390a-1f2e-4b3c-9d8e-7f6a5b4c3d2e';
  /** UUID 形状：8-4-4-4-12。任何回退路径都不得产出这种文本。 */
  const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

  it('有真指纹时原样显示（16 位 hex），不显示 device_id', () => {
    const fp = fingerprintText('9f86d081884c7d65');
    expect(fp).toBe('9f86d081884c7d65');
    expect(fp).toHaveLength(16);
    expect(fp).not.toBe(UUID);
  });

  it('前后空白被裁掉（后端不会带空白，此处只做防御）', () => {
    expect(fingerprintText('  9f86d081884c7d65  ')).toBe('9f86d081884c7d65');
  });

  it('空串返回空串，绝不回退 device_id（D-69：UUID 冒充指纹已被判死）', () => {
    const fp = fingerprintText('');
    expect(fp).toBe('');
    expect(fp).not.toBe(UUID);
  });

  it('纯空白按缺失处理，返回空串（不是回退，也不是原样返回空白）', () => {
    expect(fingerprintText('   ')).toBe('');
    expect(fingerprintText('\t\n ')).toBe('');
  });

  it('null / undefined 返回空串', () => {
    expect(fingerprintText(null)).toBe('');
    expect(fingerprintText(undefined)).toBe('');
  });

  it('任何缺失形态都不返回 UUID 形状的文本（结构上不可能再冒充设备指纹）', () => {
    for (const value of ['', '   ', null, undefined]) {
      expect(fingerprintText(value)).not.toMatch(UUID_RE);
    }
  });

  it('缺失形态一律让 hasSessionCode 判 false → 调用方整行不渲染', () => {
    for (const value of ['', '   ', null, undefined]) {
      expect(hasSessionCode(fingerprintText(value))).toBe(false);
    }
  });
});

// D-69：核对块 / 指纹行「有没有值可显示」。缺值时整块不渲染 ——
// 「会话码：」后面留空的危害比不显示更大：用户会读成「码是空的，应该没事」。
describe('D-69：核对块显示判定 hasSessionCode', () => {
  it('有值时显示（16 位 hex 原样判 true）', () => {
    expect(hasSessionCode('9f86d081884c7d65')).toBe(true);
  });

  it('null 不显示', () => {
    expect(hasSessionCode(null)).toBe(false);
  });

  it('undefined 不显示（字段缺失 / 老后端没有该字段）', () => {
    expect(hasSessionCode(undefined)).toBe(false);
  });

  it('空串 / 纯空白不显示', () => {
    expect(hasSessionCode('')).toBe(false);
    expect(hasSessionCode('   ')).toBe(false);
    expect(hasSessionCode('\t\n ')).toBe(false);
  });

  it('单字符也算有值（判定只看非空，不猜长度）', () => {
    expect(hasSessionCode('a')).toBe(true);
  });
});
