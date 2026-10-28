// D-116：安卓 v1.3.1 装完，设置页「快速解锁」卡片显示
// 「Windows Hello 快速解锁 · 未开启 · 当前平台不支持快速解锁」。
//
// 两个缺陷叠在一起：
//
// 1. **文案平台绑死**：「Windows Hello」是 Windows 专属名词，安卓上毫无意义；
//    而且手机明明有指纹和人脸（主人原话）。
// 2. **为未实现的功能做广告**：安卓适配器（Keystore + BiometricPrompt）是 P3，
//    现在压根没接。给一个还不存在的功能挂块「当前平台不支持」的牌子，既制造
//    「本该有」的预期，又用了错误的理由（真相是「我们还没做」，不是「你的手机不行」）。
//
// 修法：core 新增 `Availability::NotImplemented`（本平台没有适配器）与
// `Unsupported`（有适配器但平台此刻做不了）分开；前端只在 `not_implemented` 时
// **整卡不渲染**。

import { describe, expect, it } from 'vitest';

const rustModules = import.meta.glob('../../src-tauri/src/**/*.rs', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const coreModules = import.meta.glob('../../crates/passwordms-core/src/*.rs', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const frontModules = import.meta.glob(
  ['../components/Settings.svelte', './i18n/*.ts'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function pick(mods: Record<string, string>, suffix: string): string {
  return Object.entries(mods).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
}

const biometricRs = pick(coreModules, 'biometric.rs');
const unsupportedRs = pick(rustModules, 'platform/unsupported.rs');
const cmdRs = pick(rustModules, 'commands/biometric_cmd.rs');
const settings = pick(frontModules, 'Settings.svelte');
const zh = pick(frontModules, 'i18n/zh-CN.ts');
const zhTw = pick(frontModules, 'i18n/zh-TW.ts');
const en = pick(frontModules, 'i18n/en-US.ts');
const th = pick(frontModules, 'i18n/th.ts');

describe('D-116 未实现平台不许谎报「本机不支持」', () => {
  it('core 把「没有适配器」独立成 NotImplemented，不再与 Unsupported 混用', () => {
    expect(biometricRs).toContain('NotImplemented');
    // Unsupported 必须保留（Windows 的 WinRT 激活失败仍要用它，且必须可见）。
    expect(biometricRs).toContain('Unsupported');
    // 占位适配器必须报 NotImplemented——它就是「本平台没接」的唯一来源。
    expect(unsupportedRs).toContain('Availability::NotImplemented');
    expect(unsupportedRs).not.toContain('Availability::Unsupported');
  });

  it('wire 取值是 not_implemented（前端按它判定是否渲染整卡）', () => {
    expect(cmdRs).toContain('Availability::NotImplemented => "not_implemented"');
    // Unsupported 的 wire 值不许被顺手改掉，否则老的「平台做不了」分支会失配。
    expect(cmdRs).toContain('Availability::Unsupported => "unsupported"');
  });

  it('设置页在 not_implemented 时整块不渲染（含 bio === null 的未知态）', () => {
    expect(settings).toContain("{#if bio && bio.availability !== 'not_implemented'}");
    // 反过来：不许改成「显示但置灰」或「显示一句不支持」。
    expect(settings).not.toContain("bio.availability === 'not_implemented' &&");
  });

  it('unsupported 的文案不再声称「当前平台不支持」（那是另一回事）', () => {
    for (const dict of [zh, zhTw, en, th]) {
      const line = dict.split('\n').find((l) => l.includes('bio_avail_unsupported:')) ?? '';
      expect(line).not.toBe('');
      expect(line).not.toContain('当前平台不支持');
      expect(line).not.toContain('目前平台不支援');
      expect(line).not.toContain('not supported on this platform');
      expect(line).not.toContain('ไม่รองรับ');
    }
    // 新口径必须指向「系统组件调用不了」这个真实原因。
    expect(zh).toContain('无法调用');
  });
});
