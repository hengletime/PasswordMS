// 法律文本数据源（D-63）守门测试：
// ①两份文档（免责声明 / 用户协议）× 两种语言全部存在；②条数固定（6 / 8，属需求口径，
// 改条数必须同步改本测试与 06-ui-design 规范）；③每条 heading 与每段 paragraphs 非空
// （禁占位符，实质内容要求）；④updated 统一且与生效日期一致。

import { describe, expect, it } from 'vitest';

import {
  getDisclaimer,
  getTerms,
  LEGAL_UPDATED,
  type LegalDoc,
  type LegalLang,
} from './legal-content';

const LANGS: LegalLang[] = ['zh', 'en'];

describe('legal-content —— D-63 法律文本数据源', () => {
  it('两种语言的两份文档都存在且标题非空', () => {
    for (const lang of LANGS) {
      const disclaimer = getDisclaimer(lang);
      const terms = getTerms(lang);
      expect(disclaimer.title.length, `${lang}:disclaimer.title`).toBeGreaterThan(0);
      expect(terms.title.length, `${lang}:terms.title`).toBeGreaterThan(0);
    }
  });

  it('免责声明固定 6 条、用户协议固定 8 条（需求口径）', () => {
    for (const lang of LANGS) {
      expect(getDisclaimer(lang).sections, `${lang}:disclaimer`).toHaveLength(6);
      expect(getTerms(lang).sections, `${lang}:terms`).toHaveLength(8);
    }
  });

  it('每条 heading 非空、每段内容非空（禁占位符）', () => {
    const docs: Array<[string, LegalDoc]> = [];
    for (const lang of LANGS) {
      docs.push([`${lang}:disclaimer`, getDisclaimer(lang)], [`${lang}:terms`, getTerms(lang)]);
    }
    for (const [label, doc] of docs) {
      doc.sections.forEach((section, index): void => {
        expect(section.heading.trim().length, `${label}#${index}.heading`).toBeGreaterThan(0);
        expect(section.paragraphs.length, `${label}#${index}.paragraphs`).toBeGreaterThan(0);
        section.paragraphs.forEach((paragraph, pIndex): void => {
          // 实质内容判据：非空且不是占位符样式的短串。
          expect(paragraph.trim().length, `${label}#${index}.p${pIndex}`).toBeGreaterThan(20);
          expect(paragraph.trim(), `${label}#${index}.p${pIndex}`).not.toMatch(/^(TODO|TBD|待补|占位)/);
        });
      });
    }
  });

  it('两份文档 updated 统一为生效日期 2026-10-20', () => {
    expect(LEGAL_UPDATED).toBe('2026-10-20');
    for (const lang of LANGS) {
      expect(getDisclaimer(lang).updated, `${lang}:disclaimer.updated`).toBe(LEGAL_UPDATED);
      expect(getTerms(lang).updated, `${lang}:terms.updated`).toBe(LEGAL_UPDATED);
    }
  });

  it('中英文本文档条目标题不串语（同一索引 heading 不相同）', () => {
    // 粗判：若误把中文塞进英文分支会在此暴露。
    const zhDisclaimer = getDisclaimer('zh');
    const enDisclaimer = getDisclaimer('en');
    zhDisclaimer.sections.forEach((section, index): void => {
      expect(section.heading).not.toBe(enDisclaimer.sections[index]?.heading);
    });
    const zhTerms = getTerms('zh');
    const enTerms = getTerms('en');
    zhTerms.sections.forEach((section, index): void => {
      expect(section.heading).not.toBe(enTerms.sections[index]?.heading);
    });
  });
});
