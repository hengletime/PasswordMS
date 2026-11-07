// 字典 → 代码 **反向**扫描（D-194 / 审计项 7）：字典里每一句文案都该有渲染点。
//
// ## 为什么需要它
//
// `i18n-usage.test.ts` 查的是正向：代码里引用的键在字典里存不存在。它查不出反过来的
// 那一半——**字典里有、代码里没人用**的孤儿键。孤儿键的真实危害不是"多了一行字"，而是
// 给走查的人一个**不存在的判据**：D-193 的 `create.weak_hint` 被走查单 1.2 写成验收项，
// 走查的人永远看不到那一句；D-194 的 `err.unknown` 同理（它被兜底链 `errors.ts` 换成
// `err.internal`，天然渲染不出来）。
//
// ## 判定口径（三条规则，缺一条就会把动态键误判成孤儿）
//
// A **字面命中**：任一生产文件里出现这个键的原文。按子串判，不管引号形式。
// B **动态尾巴**：键的某个**以 `.` 或 `_` 结尾**的前缀，后面紧跟 `${` / `' +` / `" +`。
//    例：`ui.t(\`settings.bio_avail_${bio.availability}\`)` → `settings.bio_avail_*` 全算有引用。
//    前缀必须落在 `.` / `_` 边界上：放宽成"任意前缀+`${`"会把 `attach.choose` 判成被
//    `'a' + …` 引用（实测出现过），那是假命中，比漏报更坏。
// C **受控后缀 `_android`**：基键有引用时 `基键_android` 也算有引用。依据是
//    `ui.t(\`lock.biometric_waiting${bioSuffix}\`)`，`bioSuffix` 只取 `''` 或 `'_android'`。
//    **只放行这一个后缀**——其它长度的动态拼装仍然算孤儿（宁可误报，不许漏报）。
//
// ## 基线
//
// `scripts/i18n-orphans-baseline.txt`，逐行 `键 ← 理由`。规则：
//   · 新增孤儿必须**加一行并写理由**，不许靠删键让门变绿（删键要单独判定：是真的没人用，
//     还是漏了渲染点）；
//   · 原本的孤儿被接上以后，基线里那行要删掉——否则本文件会报"基线过期"。
// 这条门因此有两个方向：**新孤儿**红、**基线过期**红。
//
// ## 自检探针（防空转）
//
// 一个永远报 0 条的扫描器看起来最好看。四条探针钉住"它真的在看"：伪造键必须被报出、
// 有引用的键不许被报、动态键与受控后缀必须被规则 B/C 放过。

import { describe, expect, it } from 'vitest';

import { flattenKeys } from './i18n/index';
import { zhCN } from './i18n/zh-CN';

/** 生产文件面：前端全部 .ts/.svelte（去掉字典自身与测试）+ Rust 侧（message_key 写在那儿）。 */
const production = import.meta.glob(
  [
    '../**/*.ts',
    '../**/*.svelte',
    '../../src-tauri/src/**/*.rs',
    '../../crates/*/src/**/*.rs',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** 字典自身与测试文件不是"渲染点"：前者定义键，后者断言键。 */
function isProduction(path: string): boolean {
  if (path.includes('/i18n/')) return false;
  if (/\.test\.ts$/.test(path)) return false;
  return true;
}

const corpus = Object.entries(production)
  .filter(([path]) => isProduction(path))
  .map(([path, text]) => ({ path, text }));

const KEYS = flattenKeys(zhCN as unknown as Record<string, unknown>);
const KEY_SET = new Set(KEYS);

/** 规则 B 的判据：以 `.` / `_` 结尾的前缀后面紧跟模板或字符串拼接。 */
function dynamicPrefixHit(prefix: string): boolean {
  const escaped = prefix.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const re = new RegExp(`${escaped}(\\$\\{|\\s*['"]\\s*\\+)`);
  return corpus.some((f) => re.test(f.text));
}

/** 这个键在代码里有没有落点；返回命中的规则（便于报错时说清为什么）。 */
function coverage(key: string): 'literal' | `dynamic:${string}` | `suffix-of:${string}` | null {
  if (corpus.some((f) => f.text.includes(key))) return 'literal';
  for (let i = 0; i < key.length; i += 1) {
    const ch = key[i];
    if (ch !== '.' && ch !== '_') continue;
    const prefix = key.slice(0, i + 1);
    if (dynamicPrefixHit(prefix)) return `dynamic:${prefix}`;
  }
  // 受控后缀：只认 `_android`，且基键必须在字典里（否则说明这是个普通长键名）。
  if (key.endsWith('_android')) {
    const base = key.slice(0, -'_android'.length);
    if (KEY_SET.has(base) && coverage(base) !== null) return `suffix-of:${base}`;
  }
  return null;
}

/** 基线：`键 ← 理由`。`#` 开头是注释，空行忽略。 */
const baselineRaw =
  (import.meta.glob('../../scripts/i18n-orphans-baseline.txt', {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>)['../../scripts/i18n-orphans-baseline.txt'] ?? '';

type BaselineRow = { key: string; reason: string; line: number };

function parseBaseline(text: string): BaselineRow[] {
  const rows: BaselineRow[] = [];
  text.split('\n').forEach((raw, idx) => {
    const line = raw.trim();
    if (!line || line.startsWith('#')) return;
    const at = line.indexOf('←');
    if (at === -1) {
      rows.push({ key: line, reason: '', line: idx + 1 });
      return;
    }
    rows.push({ key: line.slice(0, at).trim(), reason: line.slice(at + 1).trim(), line: idx + 1 });
  });
  return rows;
}

const BASELINE = parseBaseline(baselineRaw);
const BASELINE_KEYS = new Set(BASELINE.map((r) => r.key));

describe('字典 → 代码 反向扫描：孤儿键（D-194 / 审计项 7）', () => {
  it('守门别空跑：扫描面与字典都不是空的', () => {
    expect(corpus.length, '生产文件一个都没读到，下面的断言全是假绿').toBeGreaterThan(100);
    expect(KEYS.length, '字典键读成空 —— flattenKeys 或导入出错').toBeGreaterThan(600);
    expect(baselineRaw.length, '基线文件读成空串').toBeGreaterThan(50);
  });

  it('探针：扫描器既不许瞎，也不许过宽', () => {
    // 不许瞎：一个绝不可能存在的键必须被判成孤儿。
    expect(coverage('zz.probe.not.a.real.key'), '扫描器对伪造键也判"有引用"= 它根本没在看').toBeNull();
    // 不许过宽：真被引用的键必须判有引用。
    expect(coverage('common.cancel'), 'common.cancel 明明在用，却被判成孤儿').not.toBeNull();
    // 规则 B：模板拼出来的键（settings.bio_avail_*）必须被放过。
    expect(coverage('settings.bio_avail_busy'), '规则 B 失效：动态前缀没被认出来').toContain('dynamic:');
    // 规则 C：受控后缀 _android 必须被放过。
    expect(coverage('lock.biometric_waiting_android'), '规则 C 失效：_android 变体没被认出来').toContain(
      'suffix-of:',
    );
  });

  it('每条基线都写了理由（只写键名等于没写）', () => {
    const mute = BASELINE.filter((r) => r.reason.length < 6).map((r) => `第 ${r.line} 行 ${r.key}`);
    expect(mute, `这些基线行没写理由：${mute.join('；')}`).toEqual([]);
  });

  it('没有新的孤儿键（新增必须进基线并写理由）', () => {
    const unknown: string[] = [];
    for (const key of KEYS) {
      if (BASELINE_KEYS.has(key)) continue;
      if (coverage(key) === null) unknown.push(key);
    }
    expect(
      unknown,
      `字典里有、代码里没人用：${unknown.join(', ')}\n` +
        '处理方式二选一：① 真没人用 → 连界面一起补，或按 D-193/D-194 的流程删键；' +
        '② 确实是动态引用没被规则覆盖 → 加进 scripts/i18n-orphans-baseline.txt 并写清为什么。',
    ).toEqual([]);
  });

  it('基线里没有已经过期的行（被接上或已删的键要移除）', () => {
    const stale = BASELINE.filter(
      (r) => !KEY_SET.has(r.key) || coverage(r.key) !== null,
    ).map((r) => `第 ${r.line} 行 ${r.key}`);
    expect(stale, `这些基线行已经不再是孤儿（键被接上或已删），请从基线删掉：${stale.join('；')}`).toEqual(
      [],
    );
  });
});
