// Bitwarden CSV 字段映射（U8）：仅同名基础列 + folder，其余入 custom_fields。

import { describe, expect, it } from 'vitest';

import { enUS } from './i18n/en-US';
import { zhCN } from './i18n/zh-CN';
import { isKnownColumn, KNOWN_COLUMNS, mapCsvHeader } from './import';

/**
 * D-72：core 里 `import/` 全部 `.rs` 原文（用于把前端列名与 Rust 真相源绑在一起）。
 *
 * 与 `ipc-contract.test.ts` 同一手法（`?raw` + `import.meta.glob`），不依赖 Node API。
 */
const coreImportSources = import.meta.glob('../../crates/passwordms-core/src/import/*.rs', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

describe('mapCsvHeader', () => {
  it('maps known columns and routes the rest to custom fields', () => {
    const mapping = mapCsvHeader(['name', 'login_uri', 'custom_x', 'notes', 'my_column']);
    expect(mapping.mapped).toEqual(['name', 'login_uri', 'notes']);
    expect(mapping.customFields).toEqual(['custom_x', 'my_column']);
  });

  it('normalizes case and trims whitespace', () => {
    const mapping = mapCsvHeader(['  Name ', 'LOGIN_URI', 'Folder']);
    expect(mapping.mapped).toEqual(['name', 'login_uri', 'folder']);
    expect(mapping.customFields).toEqual([]);
  });

  it('ignores empty and duplicate columns', () => {
    const mapping = mapCsvHeader(['name', '', 'name', '   ', 'name']);
    expect(mapping.mapped).toEqual(['name']);
    expect(mapping.customFields).toEqual([]);
  });

  it('keeps insertion order of custom fields', () => {
    const mapping = mapCsvHeader(['alpha', 'name', 'beta', 'gamma']);
    expect(mapping.customFields).toEqual(['alpha', 'beta', 'gamma']);
  });

  it('known column set matches the U8 ruling', () => {
    expect(KNOWN_COLUMNS).toEqual([
      'folder',
      'favorite',
      'type',
      'name',
      'notes',
      'login_uri',
      'login_username',
      'login_password',
      'login_totp',
      'fields',
    ]);
    expect(isKnownColumn('LOGIN_PASSWORD')).toBe(true);
    expect(isKnownColumn('secret_extra')).toBe(false);
  });
});

// D-72：CSV 列名有**四处副本**，其中「给用户看的那一句」原先完全没人守。
//
//   ① `crates/passwordms-core/src/import/bitwarden_csv.rs` 的 `KNOWN_COLUMNS` —— **真相源**
//   ② `crates/passwordms-core/src/import/template.rs` 的 `CSV_TEMPLATE` 表头
//      —— 已被 Rust 侧 `tests/import_template.rs::template_header_matches_known_columns` 锁住 ✅
//   ③ `src/lib/import.ts` 的 `KNOWN_COLUMNS` —— 原先只和本文件里的字面量比，**没和 ① 比** ⚠️
//   ④ `src/lib/i18n/{zh-CN,en-US}.ts` 的 `import.template_hint` 文本 —— **零守卫** ❌
//
// 第 ④ 条才是危险的那条：它是**对用户的承诺**。有人给 Rust 加一列、② 跟着改（Rust 测试照样绿），
// ③④ 会静默落后 —— 界面继续写「识别这 10 列」，用户照着模板填反而漏字段。
// 这类「规范与代码不一致直接制造 bug」正是 `docs/06-ui-design.md` 硬约束第 3 条点过的（D-27）。
describe('D-72：CSV 列名四方一致性（真相源 = Rust `KNOWN_COLUMNS`）', () => {
  it('TS 的 KNOWN_COLUMNS 与 Rust 的 KNOWN_COLUMNS 逐字相等', () => {
    const src = coreImportSources['../../crates/passwordms-core/src/import/bitwarden_csv.rs'];
    // 防空扫：glob 路径写错时 `src` 是 undefined，下面的断言若直接跑会「在空值上转绿」。
    expect(typeof src).toBe('string');
    if (typeof src !== 'string') {
      throw new Error('未读到 bitwarden_csv.rs（glob 路径或文件名变了？）');
    }

    // 只认 `KNOWN_COLUMNS: [&str; N] = [ ... ];` 这一处声明，`pub const` 前缀无所谓。
    const decl = /KNOWN_COLUMNS\s*:\s*\[&str;\s*(\d+)\]\s*=\s*\[([\s\S]*?)\];/.exec(src);
    expect(decl, '在 bitwarden_csv.rs 里没找到 KNOWN_COLUMNS 的数组声明').not.toBeNull();
    if (decl === null) {
      throw new Error('KNOWN_COLUMNS 声明正则没匹配上（Rust 侧改写法了？）');
    }
    const declaredLen = Number(decl[1]);
    const rustColumns = [...decl[2].matchAll(/"([^"]+)"/g)].map((m): string => m[1]);

    // 自检：声明长度 == 实际元素数。防「加了一列却忘改长度」（那会编译失败，但正则也可能半途截断）。
    expect(rustColumns).toHaveLength(declaredLen);
    // 主断言：前端与真相源逐字相等，且**顺序一致**（顺序影响 UI 提示的可读性）。
    expect(rustColumns).toEqual([...KNOWN_COLUMNS]);
  });

  it('i18n 两种语言的 template_hint 都按顺序包含全部列名', () => {
    for (const [locale, dict] of [
      ['zh-CN', zhCN],
      ['en-US', enUS],
    ] as const) {
      const hint: string = dict.import.template_hint;
      let last = -1;
      for (const col of KNOWN_COLUMNS) {
        const at = hint.indexOf(col);
        expect(at, `${locale} 的 import.template_hint 漏了列名「${col}」`).toBeGreaterThan(-1);
        // 顺序必须与 KNOWN_COLUMNS 一致：用户是照着这句从左往右对表头的。
        expect(at, `${locale} 的 import.template_hint 列名顺序与 KNOWN_COLUMNS 不一致（「${col}」）`).toBeGreaterThan(
          last,
        );
        last = at;
      }
    }
  });
});
