#!/usr/bin/env node
/**
 * check-css-contract.mjs —— CSS 级联契约检查（零依赖）
 *
 * 存在理由（事故 D-27）：
 *   `.pms-btn:hover:not(:disabled)`        → 特异性 (0,3,0)，设 background
 *   `.pms-btn.is-primary`                  → 特异性 (0,2,0)，设 background + color
 *   `.pms-btn.is-primary:hover:not(:disabled)` → (0,4,0)，但**只补了 filter/border-color**
 *   结果：悬停时 background 被 (0,3,0) 的通用规则抢走，color 仍是 --pms-accent-ink
 *   → 浅色下白字压 #EAEEF3、暗色下 #04211D 压 #0E141C，按钮悬停"一片空白"。
 *
 * 靠肉眼排查漏了两次（D-24 / D-27），因此改为机器判定两条不变量：
 *   INV-1（所有权）带变体类的按钮，其悬停态的 background / border-color / color 三项，
 *          胜出规则必须来自「含该变体类」的选择器 —— 变体必须自己负责自己的悬停。
 *   INV-2（可读性）每个变体 × 每个状态 × 每个主题，解析成实际颜色后算对比度：
 *          < 3.0 判 FAIL（WCAG 1.4.11 非文本/控件底线），< 4.5 判 WARN（AA 正文线）。
 *   INV-3（主题一致性）data-theme='dark' 与 @media(prefers-color-scheme:dark) 令牌必须逐位相同，
 *          否则"跟随系统"与"手动选暗色"会渲染出两种效果。
 *
 * 用法：node scripts/check-css-contract.mjs [cssPath]
 * 退出码：0 = 通过（可能带 WARN），1 = 有 FAIL。
 */

import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CSS_PATH = process.argv[2] ? resolve(process.argv[2]) : resolve(ROOT, 'src/styles/global.css');

/* ============================================================
 * 0. 读文件 + 去注释（保留换行，便于报行号）
 * ============================================================ */
const raw = readFileSync(CSS_PATH, 'utf8');
const src = raw.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '));

function lineOf(index) {
  let n = 1;
  for (let i = 0; i < index && i < src.length; i++) if (src[i] === '\n') n++;
  return n;
}

/* ============================================================
 * 1. 花括号配对解析 —— 只下钻一层 @media / @supports，跳过 @keyframes
 *    （上一版用正则平铺解析，被 @media 嵌套带偏，吐出"未发现泄漏"的假结论）
 * ============================================================ */
const rules = [];   // { selector, decls:[[prop,val]], media, order, line }
let order = 0;

function parseBlock(start, end, media) {
  let i = start;
  while (i < end) {
    // 跳空白
    while (i < end && /\s/.test(src[i])) i++;
    if (i >= end) break;

    // 找当前语句的结束：'{' 或 ';' 或到 end
    let j = i, depthParen = 0;
    while (j < end) {
      const c = src[j];
      if (c === '(') depthParen++;
      else if (c === ')') depthParen--;
      else if (depthParen === 0 && (c === '{' || c === ';')) break;
      j++;
    }
    if (j >= end) break;

    if (src[j] === ';') { i = j + 1; continue; }   // @charset / @import 之类

    const prelude = src.slice(i, j).trim();
    // 配对找到 '}' 
    let k = j + 1, depth = 1;
    while (k < end && depth > 0) {
      if (src[k] === '{') depth++;
      else if (src[k] === '}') depth--;
      k++;
    }
    const bodyStart = j + 1, bodyEnd = k - 1;

    if (prelude.startsWith('@')) {
      const name = prelude.slice(1).split(/[\s({]/)[0].toLowerCase();
      if (name === 'media' || name === 'supports') {
        const cond = media ? `${media} && ${prelude}` : prelude;
        parseBlock(bodyStart, bodyEnd, cond);
      } else if (name === 'keyframes' || name === 'font-face' || name === 'layer') {
        /* 跳过：内部无选择器语义，或与本检查无关 */
      } else {
        parseBlock(bodyStart, bodyEnd, media);   // @-rule 未知：保守下钻
      }
    } else {
      const decls = [];
      for (const part of src.slice(bodyStart, bodyEnd).split(';')) {
        const p = part.trim();
        if (!p || !p.includes(':')) continue;
        const idx = p.indexOf(':');
        decls.push([p.slice(0, idx).trim(), p.slice(idx + 1).trim()]);
      }
      for (const sel of prelude.split(',')) {
        const s = sel.trim();
        if (!s) continue;
        rules.push({ selector: s, decls, media, order: order++, line: lineOf(i) });
      }
    }
    i = k;
  }
}
parseBlock(0, src.length, null);

/* ============================================================
 * 2. 特异性 + 单复合选择器匹配
 * ============================================================ */
function specificity(sel) {
  let a = 0, b = 0, c = 0;
  let i = 0;
  const stripWhere = (s) => s.replace(/:where\(([^()]*)\)/g, '');  // :where 记 0
  // :not(...) / :is(...) 取其参数中最高的特异性
  let rest = sel, subSpec = [0, 0, 0];
  rest = rest.replace(/:not\(([^()]*)\)/g, (_m, inner) => {
    const s = specificity(inner);
    subSpec = maxSpec(subSpec, s);
    return '';
  });
  rest = rest.replace(/:is\(([^()]*)\)/g, (_m, inner) => {
    let best = [0, 0, 0];
    for (const part of inner.split(',')) best = maxSpec(best, specificity(part.trim()));
    subSpec = maxSpec(subSpec, best);
    return '';
  });
  rest = stripWhere(rest);
  // 属性选择器
  b += (rest.match(/\[[^\]]*\]/g) || []).length;
  rest = rest.replace(/\[[^\]]*\]/g, '');
  // 伪元素
  c += (rest.match(/::[a-zA-Z-]+/g) || []).length;
  rest = rest.replace(/::[a-zA-Z-]+/g, '');
  // 伪类
  b += (rest.match(/:[a-zA-Z-]+/g) || []).length;
  rest = rest.replace(/:[a-zA-Z-]+(\([^()]*\))?/g, '');
  // 类
  b += (rest.match(/\.[a-zA-Z0-9_-]+/g) || []).length;
  rest = rest.replace(/\.[a-zA-Z0-9_-]+/g, '');
  // ID
  a += (rest.match(/#[a-zA-Z0-9_-]+/g) || []).length;
  rest = rest.replace(/#[a-zA-Z0-9_-]+/g, '');
  // 元素
  c += (rest.match(/[a-zA-Z][a-zA-Z0-9-]*/g) || []).length;
  return [a, b, c].map((v, k) => v + subSpec[k]);
}
const maxSpec = (x, y) => {
  for (let i = 0; i < 3; i++) { if (x[i] !== y[i]) return x[i] > y[i] ? x : y; }
  return x;
};
const cmpSpec = (x, y) => { for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i]; return 0; };

// 我们只判定「单个复合选择器」（无空格/+/~/> 组合器）——它覆盖全部 .pms-btn* 规则。
// 含组合器或未知伪类的选择器一律跳过，但下面的覆盖断言会要求跳过的选择器里不含目标类。
const KNOWN_PSEUDO = new Set(['hover', 'active', 'focus', 'focus-visible', 'focus-within',
  'disabled', 'enabled', 'checked', 'first-child', 'last-child', 'not', 'is', 'where']);

function parseCompound(sel) {
  if (/[\s>+~]/.test(sel)) return null;              // 有组合器 → 跳过
  const tokens = [];
  let i = 0;
  while (i < sel.length) {
    const c = sel[i];
    if (c === ':') {
      const m = /^::?([a-zA-Z-]+)/.exec(sel.slice(i));
      if (!m) return null;
      const name = m[1].toLowerCase().slice(0, 2) === '--' ? null : m[1].toLowerCase();
      if (!name || !KNOWN_PSEUDO.has(name)) return null;   // 未知伪类 → 跳过
      let inner = null;
      let adv = m[0].length;
      if (sel[i + adv] === '(') {
        let d = 0, j = i + adv;
        for (; j < sel.length; j++) {
          if (sel[j] === '(') d++;
          else if (sel[j] === ')') { d--; if (d === 0) break; }
        }
        inner = sel.slice(i + adv + 1, j);
        adv = j - i + 1;
      }
      tokens.push({ kind: 'pseudo', name, inner });
      i += adv;
    } else if (c === '.') {
      const m = /^\.([a-zA-Z0-9_-]+)/.exec(sel.slice(i));
      if (!m) return null;
      tokens.push({ kind: 'class', name: m[1] });
      i += m[0].length;
    } else if (c === '[') {
      const m = /^\[([^\]]*)\]/.exec(sel.slice(i));
      if (!m) return null;
      tokens.push({ kind: 'attr', name: m[1] });
      i += m[0].length;
    } else if (c === '*') {
      i += 1;
    } else if (/[a-zA-Z]/.test(c)) {
      const m = /^([a-zA-Z][a-zA-Z0-9-]*)/.exec(sel.slice(i));
      tokens.push({ kind: 'tag', name: m[1].toLowerCase() });
      i += m[0].length;
    } else { return null; }
  }
  return tokens;
}

function pseudoMatches(tok, el, state) {
  const { name, inner } = tok;
  if (name === 'not') {
    if (inner === null) return true;
    return !inner.split(',').some((s) => matchesCompound(s.trim(), el, state));
  }
  if (name === 'is' || name === 'where') {
    if (inner === null) return true;
    return inner.split(',').some((s) => matchesCompound(s.trim(), el, state));
  }
  if (name === 'hover') return !!state.hover;
  if (name === 'enabled') return !el.disabled;
  if (name === 'disabled') return !!el.disabled;
  if (name === 'checked') return !!el.checked;
  // active / focus / focus-visible / focus-within / first-child / last-child 等：
  // 测例元素处于"未激活、非首末子"的中性态 → 一律不匹配。
  return false;
}

function matchesCompound(sel, el, state) {
  const toks = parseCompound(sel);
  if (!toks) return false;
  for (const t of toks) {
    if (t.kind === 'class' && !el.classes.includes(t.name)) return false;
    if (t.kind === 'tag' && t.name !== el.tag) return false;
    if (t.kind === 'attr') return false;
    if (t.kind === 'pseudo' && !pseudoMatches(t, el, state)) return false;
  }
  return true;
}

/* ============================================================
 * 3. 令牌表（浅 / 暗）
 * ============================================================ */
function tokensFrom(selectorList, mediaMatch) {
  const map = new Map();
  for (const r of rules) {
    if (!selectorList.includes(r.selector)) continue;
    if (!mediaMatch(r.media)) continue;
    for (const [p, v] of r.decls) if (p.startsWith('--')) map.set(p, v);
  }
  return map;
}

const lightTokens = tokensFrom([':root'], (m) => !m);
const darkMediaTokens = tokensFrom([':root'], (m) => !!m && /prefers-color-scheme:\s*dark/.test(m));
const darkAttrTokens = tokensFrom([":root[data-theme='dark']"], () => true);
const lightAttrTokens = tokensFrom([":root[data-theme='light']"], () => true);

const darkTokens = new Map([...lightTokens, ...darkAttrTokens]);

function resolveVar(value, tokens, depth = 0) {
  if (depth > 8) return value;
  return value.replace(/var\(\s*(--[a-zA-Z0-9_-]+)\s*(?:,([^()]*))?\)/g, (_m, name, fb) => {
    const v = tokens.get(name);
    if (v === undefined) return (fb || '').trim() || 'transparent';
    return resolveVar(v, tokens, depth + 1);
  }).trim();
}

/* ============================================================
 * 4. 颜色解析 + 对比度
 * ============================================================ */
const NAMED = { transparent: [0, 0, 0, 0], white: [255, 255, 255, 1], black: [0, 0, 0, 1] };

function parseColor(str) {
  const s = String(str).trim().toLowerCase();
  if (NAMED[s]) return NAMED[s];
  let m = /^#([0-9a-f]{3})$/.exec(s);
  if (m) return [...m[1]].map((h) => parseInt(h + h, 16)).concat([1]);
  m = /^#([0-9a-f]{6})$/.exec(s);
  if (m) return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)).concat([1]);
  m = /^#([0-9a-f]{8})$/.exec(s);
  if (m) return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16)).concat([parseInt(m[1].slice(6, 8), 16) / 255]);
  m = /^rgba?\(([^)]+)\)$/.exec(s);
  if (m) {
    const p = m[1].split(/[,\s/]+/).filter(Boolean).map(Number);
    return [p[0] | 0, p[1] | 0, p[2] | 0, p.length > 3 ? p[3] : 1];
  }
  return null;
}

function over(fg, bg) {
  const a = fg[3];
  return [0, 1, 2].map((i) => Math.round(fg[i] * a + bg[i] * (1 - a))).concat([1]);
}

function luminance([r, g, b]) {
  const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

function contrast(fgStr, bgStr) {
  const fg0 = parseColor(fgStr), bg0 = parseColor(bgStr);
  if (!fg0 || !bg0) return null;
  const bgs = bg0[3] < 1 ? over(bg0, [255, 255, 255, 1]) : bg0;   // 半透明底：按白底合算
  const fg = over(fg0, bgs);
  const l1 = luminance(fg), l2 = luminance(bgs);
  return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
}

/* ============================================================
 * 5. 解算：给定元素 + 状态 + 主题，逐属性找胜出规则
 * ============================================================ */
const PROPS = ['background', 'border-color', 'color'];

function resolveProps(el, state, tokens) {
  const out = {};
  for (const prop of PROPS) out[prop] = null;
  for (const r of rules) {
    if (/prefers-color-scheme:\s*light/.test(r.media || '')) continue;   // 本文件无，保险
    if (r.media && /prefers-color-scheme/.test(r.media) && !/dark/.test(r.media)) continue;
    if (!matchesCompound(r.selector, el, state)) continue;
    const spec = specificity(r.selector);
    for (const [p, v] of r.decls) {
      if (!PROPS.includes(p)) continue;
      const cur = out[p];
      if (!cur) { out[p] = { value: v, spec, order: r.order, selector: r.selector, line: r.line }; continue; }
      const c = cmpSpec(spec, cur.spec);
      if (c > 0 || (c === 0 && r.order > cur.order)) {
        out[p] = { value: v, spec, order: r.order, selector: r.selector, line: r.line };
      }
    }
  }
  for (const p of PROPS) {
    if (!out[p]) { out[p] = { value: 'transparent', spec: [0, 0, 0], order: -1, selector: '(未声明)', line: 0 }; continue; }
    out[p] = { ...out[p], raw: out[p].value, value: resolveVar(out[p].value, tokens) };
  }
  return out;
}

/* ============================================================
 * 6. 用例矩阵
 * ============================================================ */
const EL = (classes, opts = {}) => ({ tag: 'button', classes, disabled: false, checked: false, ...opts });

const CASES = [
  { label: '普通按钮',        el: EL(['pms-btn']) },
  { label: '主按钮',          el: EL(['pms-btn', 'is-primary']) },
  { label: '主按钮·大·满宽',   el: EL(['pms-btn', 'is-primary', 'is-block', 'is-lg']) },
  { label: '主按钮·小',       el: EL(['pms-btn', 'is-primary', 'is-sm']) },   // Settings 主题三选一的选中态
  { label: '危险按钮',        el: EL(['pms-btn', 'is-danger']) },
  { label: '幽灵按钮',        el: EL(['pms-btn', 'is-ghost']) },
  { label: '幽灵·小',         el: EL(['pms-btn', 'is-ghost', 'is-sm']) },
  { label: '图标按钮',        el: EL(['pms-icon-btn']) },
  { label: '主按钮·禁用',      el: EL(['pms-btn', 'is-primary'], { disabled: true }) },
];

const THEMES = [
  { name: '浅色', tokens: lightTokens, backdrop: lightTokens.get('--pms-surface') },
  { name: '暗色', tokens: darkTokens,  backdrop: darkTokens.get('--pms-surface') },
];

const STATES = [{ name: '静止', hover: false }, { name: '悬停', hover: true }];

/* 覆盖断言：跳过的选择器里不允许出现我们关心的类 */
const skipped = rules.filter((r) => parseCompound(r.selector) === null);
const skipHits = skipped.filter((r) => /pms-btn|pms-icon-btn/.test(r.selector));

/* ============================================================
 * 7. 执行 + 判定
 * ============================================================ */
let fails = 0, warns = 0;
const warnLines = [];

const HEAD = '变体'.padEnd(20) + '状态'.padEnd(6) + '主题'.padEnd(6)
  + '背景'.padEnd(20) + '文字'.padEnd(20) + '对比度';
console.log('─'.repeat(96));
console.log(HEAD);
console.log('─'.repeat(96));

for (const c of CASES) {
  for (const st of STATES) {
    for (const th of THEMES) {
      const res = resolveProps(c.el, st, th.tokens);
      const isIcon = c.el.classes.includes('pms-icon-btn');
      const pageBg = resolveVar(th.tokens.get('--pms-bg') || '#ffffff', th.tokens);
      let bgSolid = res['background'].value;
      if (parseColor(bgSolid)?.[3] === 0) bgSolid = res['background'].selector === '(未声明)' ? pageBg : th.backdrop;
      const ratio = contrast(res['color'].value, bgSolid);
      const ratioTxt = ratio === null ? '解析失败' : ratio.toFixed(2) + ':1';

      let flag = '     ';
      if (ratio === null) { flag = ' FAIL'; fails++; }
      else if (ratio < 3.0) { flag = ' FAIL'; fails++; }
      else if (ratio < 4.5) {
        flag = ' WARN'; warns++;
        const why = isIcon ? '图标（非文本）：≥3 已达标，AA 正文线 4.5 未达' : '文本未达 AA 4.5';
        warnLines.push(`  · ${c.label} / ${st.name} / ${th.name}：${ratioTxt} —— ${why}`);
      }

      console.log(
        c.label.padEnd(20) + st.name.padEnd(6) + th.name.padEnd(6)
        + (res['background'].value + (res['background'].value !== bgSolid ? ` →${bgSolid}` : '')).padEnd(20)
        + res['color'].value.padEnd(20) + ratioTxt + flag
      );
    }
  }
}
console.log('─'.repeat(96));

/* INV-1 所有权：带**颜色变体类**的按钮，悬停三项必须由含该变体类的选择器胜出。
 * 注意：is-sm / is-block / is-lg 是尺寸修饰，不改配色，不参与所有权判定
 * （否则 `pms-btn is-lg` 这种写法会被误判）。 */
const COLOR_VARIANTS = ['is-primary', 'is-danger', 'is-ghost'];
const D27 = [];
for (const c of CASES) {
  if (c.el.disabled) continue;
  const variantClasses = c.el.classes.filter((x) => COLOR_VARIANTS.includes(x));
  const res = resolveProps(c.el, { hover: true }, lightTokens);
  for (const p of PROPS) {
    const w = res[p];
    if (w.selector === '(未声明)') continue;
    const owns = variantClasses.length === 0 || variantClasses.some((vc) => w.selector.includes('.' + vc));
    if (!owns) {
      D27.push({ case: c.label, prop: p, winner: w.selector, line: w.line, value: w.value, variants: variantClasses });
    }
  }
}

console.log('\n【INV-1 悬停态所有权】变体必须自己声明悬停的 background / border-color / color');
if (D27.length === 0) {
  console.log('  ✅ 通过：无任何变体在悬停态被通用规则抢走声明。');
} else {
  for (const d of D27) {
    console.log(`  ❌ ${d.case} 的 ${d.prop} 在悬停时由「${d.winner}」(第 ${d.line} 行) 胜出 = ${d.value}`);
    console.log(`     应为 ${d.variants.join(' / ')} 自己的悬停规则胜出 —— 该变体缺这条声明。`);
  }
  fails += D27.length;
}

/* INV-3 主题一致性
 * 暗色媒体块与暗色显式块：两者都是「全量覆盖」，令牌集必须完全一致。
 * 浅色显式块 vs :root 基色：显式块只覆盖**配色子集**（字体/圆角/尺寸仍由 :root 提供），
 * 因此只比对两处都声明过的键 —— 共有的键必须同值。 */
console.log('\n【INV-3 主题令牌一致性】data-theme 与 prefers-color-scheme 必须同值');
function diffMaps(a, b, label, mode) {
  const keysA = [...a.keys()], keysB = [...b.keys()];
  const keys = mode === 'intersect'
    ? keysA.filter((k) => b.has(k))
    : [...new Set([...keysA, ...keysB])];
  const bad = [];
  for (const k of keys) if (a.get(k) !== b.get(k)) bad.push(`${k}: ${a.get(k)} ≠ ${b.get(k)}`);
  if (mode === 'intersect') {
    const onlyA = keysA.filter((k) => !b.has(k));
    console.log(`  ℹ️  ${label}：共有键 ${keys.length} 个 / 显式块独有 ${onlyA.length} 个 / 基色块独有 ${keysB.filter((k) => !a.has(k)).length} 个`);
  }
  if (bad.length === 0) console.log(`  ✅ ${label} 共有键全部一致`);
  else { bad.forEach((x) => console.log(`  ❌ ${label} 不一致 → ${x}`)); fails += bad.length; }
}
diffMaps(darkAttrTokens, darkMediaTokens, '暗色 data-theme\'dark\' vs 媒体查询', 'union');
diffMaps(lightAttrTokens, lightTokens, '浅色 data-theme\'light\' vs :root 基色', 'intersect');

/* 覆盖断言 */
console.log('\n【覆盖】跳过的选择器（含组合器/未知伪类）中是否含按钮类');
if (skipHits.length === 0) {
  console.log(`  ✅ 通过：共跳过 ${skipped.length} 条无关选择器，其中不含 pms-btn / pms-icon-btn。`);
} else {
  for (const r of skipHits) console.log(`  ❌ 第 ${r.line} 行未纳入判定：${r.selector}`);
  fails += skipHits.length;
}

/* ============================================================
 * 8. 汇总
 * ============================================================ */
console.log('\n' + '═'.repeat(96));
if (warnLines.length) {
  console.log(`⚠️  WARN ${warns} 项（未达 AA 正文 4.5，但已过控件底线 3.0）：`);
  warnLines.forEach((l) => console.log(l));
  console.log('');
}
if (fails === 0) {
  console.log(`✅ PASS —— 0 FAIL / ${warns} WARN   文件：${CSS_PATH}`);
  process.exit(0);
} else {
  console.log(`❌ FAIL —— ${fails} FAIL / ${warns} WARN   文件：${CSS_PATH}`);
  process.exit(1);
}
