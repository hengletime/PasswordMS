// i18n 键完整性：全部语言键集合必须与 zh-CN（键的唯一事实来源）完全一致。

import { describe, expect, it } from 'vitest';

import { enUS } from './i18n/en-US';
import { th } from './i18n/th';
import { zhCN } from './i18n/zh-CN';
import { zhTW } from './i18n/zh-TW';
import { dictionaries, flattenKeys, hasKey, LOCALES, translate } from './i18n/index';

describe('i18n key parity', () => {
  it('every locale exposes the identical key set as zh-CN', () => {
    const zhKeys = flattenKeys(zhCN as unknown as Record<string, unknown>).sort();
    for (const [name, dict] of [
      ['zh-TW', zhTW],
      ['en-US', enUS],
      ['th', th],
    ] as const) {
      const keys = flattenKeys(dict as unknown as Record<string, unknown>).sort();
      expect(keys, `${name} key set`).toEqual(zhKeys);
    }
  });

  it('every locale has no empty strings', () => {
    for (const locale of LOCALES) {
      const dict = dictionaries[locale] as unknown as Record<string, unknown>;
      for (const key of flattenKeys(dict)) {
        expect(translate(locale, key).length, `${locale}:${key}`).toBeGreaterThan(0);
      }
    }
  });

  it('all backend error message keys exist', () => {
    const errorKeys = [
      'err.unlock_failed',
      'err.no_vault',
      'err.vault_exists',
      'err.weak_password',
      'err.not_unlocked',
      'err.item_not_found',
      'err.folder_not_found',
      'err.folder_name',
      'err.validation',
      'err.attachment_too_large',
      'err.vault_capacity',
      'err.unsafe_url',
      'err.import_parse',
      'err.import_encrypted',
      'err.pair_token_invalid',
      'err.pair_token_expired',
      'err.pair_token_used',
      'err.not_paired',
      'err.replay',
      'err.fingerprint',
      'err.sync_aborted',
      'err.conflict',
      'err.vk_adopt_requires_empty',
      'err.io',
      'err.crypto',
      'err.internal',
    ];
    for (const locale of LOCALES) {
      for (const key of errorKeys) {
        expect(hasKey(locale, key), `${locale}:${key}`).toBe(true);
      }
    }
  });

  it('interpolates {n} placeholders', () => {
    expect(translate('en-US', 'common.items_count', { n: 3 })).toBe('3 items');
    expect(translate('zh-CN', 'common.items_count', { n: 3 })).toBe('3 条');
  });
});

// D-96 后续：旧代目录两格提示**不许断言"里面有什么"**。
//
// 这两句说的是我们**没看见**的东西：
// - 「认不出/读不出」都是"我没看进去"，所以不能说里面占着多少空间、有没有东西；
// - `uncounted` 那格连有几个都不知道，更不能出现数字。
//
// 曾经两格都写着「它们仍然占用磁盘空间」。改判据容易、防回流难——文案是四份翻译，
// 少改一份就是一句看起来精确的假话，所以把判据钉成测试而不是写在 doc 里。
//
// ⚠️ 这是**绊线**，不是"意思正确"的证明：词表只在我们想得到词的时候红，换个说法说
// 同一件事它就看不见（比如「占着地方」「还留着占用」）。真正的判据是有人在 review 时
// 读一遍中文——绊线只是把"改回去不会被发现"变成"会被发现"。
describe('旧代目录提示不许断言内容（绊线，非"意思正确"的证明）', () => {
  // 这些词都表示"对占用/内容下了我们没看见的结论"。
  const forbidden = [
    '占用磁盘空间',
    '占用空间',
    '佔用磁碟空間',
    '佔用空間',
    'disk space',
    'กินพื้นที่ดิสก์',
  ];

  it('有数字那格：必须有 {count}，但不得断言占用或内容', () => {
    for (const locale of LOCALES) {
      const text = translate(locale, 'settings.restore_point_old_unreadable');
      expect(text, `${locale} 必须保留 {count}`).toContain('{count}');
      for (const word of forbidden) {
        expect(
          text.toLowerCase().includes(word.toLowerCase()),
          `${locale} 出现「${word}」——这是对没看见的东西下结论`,
        ).toBe(false);
      }
    }
  });

  it('数不清那格：不得出现 {count}，也不得断言占用或内容', () => {
    for (const locale of LOCALES) {
      const text = translate(locale, 'settings.restore_point_old_unreadable_uncounted');
      // 数不清时连有几个都不知道，出现占位符就等于界面会渲染一个数字。
      expect(text.includes('{count}'), `${locale} 不许出现 {count}`).toBe(false);
      for (const word of forbidden) {
        expect(
          text.toLowerCase().includes(word.toLowerCase()),
          `${locale} 出现「${word}」——这是对没看见的东西下结论`,
        ).toBe(false);
      }
    }
  });
});
