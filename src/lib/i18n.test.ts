// i18n 键完整性：zh-CN 与 en-US 键集合必须完全一致。

import { describe, expect, it } from 'vitest';

import { enUS } from './i18n/en-US';
import { zhCN } from './i18n/zh-CN';
import { dictionaries, flattenKeys, hasKey, LOCALES, translate } from './i18n/index';

describe('i18n key parity', () => {
  it('zh-CN and en-US expose the identical key set', () => {
    const zhKeys = flattenKeys(zhCN as unknown as Record<string, unknown>).sort();
    const enKeys = flattenKeys(enUS as unknown as Record<string, unknown>).sort();
    expect(enKeys).toEqual(zhKeys);
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
