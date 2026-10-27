// D-105：英/繁全量审校守门——防止大陆用语回流 zh-TW、开发行话残留 en-US、
// 法律文本菜单路径与实际 UI 不符再次发生。
// 源码断言走 import.meta.glob('?raw')（svelte-check 不认 node:fs）。

import { describe, expect, it } from 'vitest';

const sources = import.meta.glob('./i18n/*.ts', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
const legal = import.meta.glob('./legal-content.ts', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

const zhTW = sources['./i18n/zh-TW.ts'] ?? '';
const enUS = sources['./i18n/en-US.ts'] ?? '';
const legalSrc = Object.values(legal)[0] ?? '';

describe('D-105 zh-TW 大陆用语禁止回流', () => {
  // 每个词都是审校实锤过的大陆特有用语；台灣慣用替代已落入值中。
  const forbidden = ['兜底', '校驗', '閾值', '檔位', '數據庫', '軟件', '信息', '屏幕鎖定'];

  it('zh-TW.ts 不含大陆特有用语', () => {
    for (const word of forbidden) {
      expect(zhTW, `zh-TW 不应出现「${word}」`).not.toContain(word);
    }
  });

  it('zh-TW 会话码用语统一为「核對碼」', () => {
    expect(zhTW).not.toContain('會話碼');
  });
});

describe('D-105 en-US 开发行话与残句', () => {
  it('面向用户的文案不含 blob / Tap / conflict box 等行话或歧义表述', () => {
    expect(enUS).not.toContain('the blob arrives');
    expect(enUS).not.toContain('Tap to choose');
    expect(enUS).not.toContain('conflict box');
    expect(enUS).not.toContain('is a fallback');
  });
});

describe('D-105 法律文本菜单路径与 UI 一致', () => {
  // 设置页没有「数据」分组——导出保险库是独立区块，四语言同修。
  it('四语言免责声明不再引用不存在的「数据」分组', () => {
    expect(legalSrc).not.toContain('数据 → 导出保险库');
    expect(legalSrc).not.toContain('Data → Export vault');
    expect(legalSrc).not.toContain('資料 → 匯出保險庫');
    expect(legalSrc).not.toContain('ข้อมูล → ส่งออกตู้เซฟ');
  });
});
