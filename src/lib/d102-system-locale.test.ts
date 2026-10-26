// D-102：首启跟随系统语言 —— `mapSystemLanguage` 的映射口径守门。
//
// 口径（主人 2026-10-26 拍板）：
// - zh-Hant*/zh-TW/zh-HK/zh-MO → 繁體中文；其余 zh-* → 简体中文；
// - th* → ไทย；en* → English；
// - 未支持的语言（fr/de/ja/…）一律兜底 English（「非简体中文切换到英文」）。
//
// 接线端（readStoredLocale）只有一行调用：仅当 localStorage **无存储值**时才走
// 检测，老用户已存语言不受影响；navigator 缺失（Node 测试环境）落 DEFAULT_LOCALE。

import { describe, expect, it } from 'vitest';

import { mapSystemLanguage } from './i18n/index';

describe('D-102 mapSystemLanguage', () => {
  it('simplified Chinese variants map to zh-CN', () => {
    expect(mapSystemLanguage('zh-CN')).toBe('zh-CN');
    expect(mapSystemLanguage('zh-Hans')).toBe('zh-CN');
    expect(mapSystemLanguage('zh-Hans-CN')).toBe('zh-CN');
    expect(mapSystemLanguage('zh-SG')).toBe('zh-CN');
    expect(mapSystemLanguage('zh-MY')).toBe('zh-CN');
    expect(mapSystemLanguage('zh')).toBe('zh-CN');
  });

  it('traditional Chinese variants map to zh-TW', () => {
    expect(mapSystemLanguage('zh-TW')).toBe('zh-TW');
    expect(mapSystemLanguage('zh-Hant')).toBe('zh-TW');
    expect(mapSystemLanguage('zh-Hant-TW')).toBe('zh-TW');
    expect(mapSystemLanguage('zh-HK')).toBe('zh-TW');
    expect(mapSystemLanguage('zh-MO')).toBe('zh-TW');
  });

  it('Thai variants map to th', () => {
    expect(mapSystemLanguage('th')).toBe('th');
    expect(mapSystemLanguage('th-TH')).toBe('th');
    expect(mapSystemLanguage('th-TH-u-nu-thai')).toBe('th');
  });

  it('English variants map to en-US', () => {
    expect(mapSystemLanguage('en')).toBe('en-US');
    expect(mapSystemLanguage('en-US')).toBe('en-US');
    expect(mapSystemLanguage('en-GB')).toBe('en-US');
    expect(mapSystemLanguage('en-AU-oed')).toBe('en-US');
  });

  it('unsupported languages fall back to en-US (主人口径：非简体中文切英文)', () => {
    expect(mapSystemLanguage('fr')).toBe('en-US');
    expect(mapSystemLanguage('fr-FR')).toBe('en-US');
    expect(mapSystemLanguage('de-DE')).toBe('en-US');
    expect(mapSystemLanguage('ja-JP')).toBe('en-US');
    expect(mapSystemLanguage('ko-KR')).toBe('en-US');
    expect(mapSystemLanguage('es-ES')).toBe('en-US');
    expect(mapSystemLanguage('ru-RU')).toBe('en-US');
  });

  it('is case-insensitive (WebView 拼写不保证统一)', () => {
    expect(mapSystemLanguage('ZH-TW')).toBe('zh-TW');
    expect(mapSystemLanguage('ZH-Hant-HK')).toBe('zh-TW');
    expect(mapSystemLanguage('TH-th')).toBe('th');
    expect(mapSystemLanguage('EN-us')).toBe('en-US');
  });
});
