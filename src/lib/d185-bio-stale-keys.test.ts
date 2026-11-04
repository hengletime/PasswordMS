// D-185 守门：后端说的失效原因，前端必须都能翻成人话。
//
// 2026-11-04 主人真机截图：设置页「快速解锁」那行显示
// `settings.bio_state_stale_master_password_changed` —— 裸键名。
// 根因是前端用 `settings.bio_state_stale_${reason}` 现拼，而后端的 wire 值叫
// `master_password_changed`、字典键却叫 `bio_state_stale_password`，
// 五个原因里两个永远拼空（`translate` 三级回落全落空才回显键名）。
//
// 这里不做"再抄一份原因清单"：**直接解析后端 Rust 源**里的 `reason_name` 映射，
// 把每个真·wire 值拿去对四语言字典。后端新增一个 InvalidatedReason 时，
// 这条守门立刻红——而不是等主人在真机上再看到一次裸键名。
import { describe, expect, it } from 'vitest';
import { BIO_STALE_UNKNOWN_KEY, bioStaleStateKey } from './bio-platform';
import { hasKey, LOCALES } from './i18n';

const sources = import.meta.glob(
  ['../../src-tauri/src/commands/biometric_cmd.rs', '../components/Settings.svelte'],
  { query: '?raw', eager: true, import: 'default' },
) as Record<string, string>;

function source(name: string): string {
  const hit = Object.entries(sources).find(([path]) => path.endsWith(name));
  expect(hit, `读不到源码：${name}`).toBeDefined();
  return hit![1];
}

/** 从 `reason_name` 的 match 分支里抠出 wire 取值（`InvalidatedReason::X => "y"`）。 */
function wireReasons(): string[] {
  const rs = source('biometric_cmd.rs');
  const at = rs.indexOf('fn reason_name');
  expect(at, '找不到 reason_name（后端改结构了？）').toBeGreaterThan(-1);
  const body = rs.slice(at, rs.indexOf('\n}', at));
  const out = [...body.matchAll(/InvalidatedReason::\w+\s*=>\s*"([a-z_]+)"/g)].map((m) => m[1]);
  return [...new Set(out)];
}

describe('D-185 失效原因必须都有文案', () => {
  it('守门本身没空跑：从 Rust 源拿到 5 个原因，字典至少 100 个键', () => {
    // 五个原因是 InvalidatedReason 的全部变体；数量变了说明后端加了/删了原因。
    expect(wireReasons().length).toBe(5);
  });

  it('每个 wire 原因在**四种语言**里都有键（含平台后缀两种）', () => {
    const reasons = wireReasons();
    for (const reason of reasons) {
      // 两个平台后缀都要成立：Windows 用无后缀键、Android 用 `_android`。
      for (const suffix of ['' as const, '_android' as const]) {
        const key = bioStaleStateKey(reason, suffix);
        expect(key, `${reason} 拼出了空键`).not.toContain('undefined');
        for (const locale of LOCALES) {
          expect(hasKey(locale, key), `${reason} → ${key} 缺 ${locale}`).toBe(true);
        }
      }
    }
  });

  it('没收录的原因也落到人话上，绝不回显键名', () => {
    const key = bioStaleStateKey('something_the_backend_added_later', '');
    expect(key).toBe(BIO_STALE_UNKNOWN_KEY);
    for (const locale of LOCALES) {
      expect(hasKey(locale, key), `兜底键缺 ${locale}`).toBe(true);
    }
  });

  it('界面不许再靠模板现拼这个键', () => {
    const svelte = source('Settings.svelte');
    expect(svelte).not.toContain('settings.bio_state_stale_${reason}');
    expect(svelte).toContain('bioStaleStateKey(reason, bioSuffix)');
  });
});
