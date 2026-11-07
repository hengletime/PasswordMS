#!/usr/bin/env node
/**
 * check-e2e-quotes.mjs —— 走查单「引文」与四本字典的一致性检查（零依赖）
 *
 * ## 存在理由（2026-11-04，盘点时实测的事故雏形）
 *
 * `docs/07-e2e-checklist.md` 的判据大量**逐字引用界面文案**，例如
 * 「N 个旧代目录认不出或读不出，不能在这里删除…」「删除当前回滚点」。
 * 这些句子是当时截图抄下来的，但文案会被改成人话（1.5.31「清单化」、1.5.32「表格化」、
 * D-175「两格提示抽纯函数」），**改的人不知道走查单里抄了一份**。
 *
 * 后果比"描述旧 UI"严重：旧 UI 描述让人找不到按钮；**引错文案会让人把正确的界面判成缺陷**，
 * 然后登记一条不存在的 D-nn。而"缺陷号可信"恰恰是这个项目最不能松的一环。
 *
 * 实测基线（2026-11-04 盘点，qa 改写走查单之前的 HEAD 97f4759）：
 * 1.16–1.20 五行里 7 个引号片段，**5 个在四本字典里查不到**。
 *
 * ## 判定方式
 *
 * 把走查单里每一个引号片段（「…」/『…』，支持嵌套）拿去四本字典的**字符串值**里做子串匹配。
 * 三轮：
 *   1. **原样**（exact）——一字不差地出现在某条字典文案里；
 *   2. **归一化**（fuzzy）——把"必然随数据变化"的部分折成同一个通配符再看子串：
 *      `{count}` 这类占位符、数字、示例变量 N/M/G/X、加粗标记 `**`、全角/半角标点、
 *      嵌套引号里的示例值（「没有匹配「zzzz」的条目」→ 「没有匹配「{q}」的条目」）；
 *   3. **缩略**（abbrev）——片段带省略号时，按 `…` 切段，要求**每一段都落在同一条**字典文案里。
 *      没有这一轮，「导入失败，没有写入任何数据……不用重新选择文件」这种中间省略的写法
 *      会被报成查不到，而它其实完全成立。
 * 三轮都不中 → 记为「查不到」。
 *
 * 归一化折叠的**只有"数字从哪来"和"标点怎么写"，不折叠汉字**——汉字变了就是文案变了，
 * 这正是这条检查要抓的东西。
 *
 * ## 跳过规则（写在代码里的 SKIP_RULES，逐条给理由）
 *
 * 走查单里还有大量引号**不是界面文案**：测试者自造的分组名（「Commander 办公」）、
 * 操作指令（icacls 路径）、组合后的样子（「登录 · 工作」「分组: 未分组」）、
 * 叙述性比喻（「写漏了不会红」）。把它们一律报出来就是噪音，没人会看第二眼。
 *
 * 判定顺序是**先查命中、再判跳过**：短片段里既有自造数据、也有「全删」「未分组」这种真标签，
 * 先查字典能把真标签算作命中，只有查不到的才落到跳过规则里当"无法判定"。
 * 所以跳过清单里不会出现"明明存在却被跳过"的项。
 *
 * 六条规则 + 一条人工清单（NOT_COPY，逐条附理由）。**跳过的数量和样例照样打印**——
 * 跳过是判断，不是忽略。
 *
 * ## 用法
 *
 *   node scripts/check-e2e-quotes.mjs
 *   node scripts/check-e2e-quotes.mjs --verbose     # 连同命中项与全部跳过项一起打印
 *
 * ## 页眉读数口径
 *
 * `sha256 <8 位> · <字节数> B`：前者是**文件内容**的 sha256 前 8 位，**不是**
 * `git hash-object` 的 blob 哈希；后者是文件字节数。不印行数——`wc -l` 数换行符，
 * `split(/\r?\n/).length` 因末尾换行多一个空串，两套口径差 1，印出来只会制造歧义。
 * 两套哈希算法、两套行数口径，任一混用都会让两个人拿着"同一个文件"对不上数。
 *
 * ## 退出码
 *
 * **恒为 0（只报警，不判负）。** 理由：这条检查的目标是"提醒有人来读一遍"，
 * 不是"卡住一次提交"。走查单正处在改写期（qa 正在改 1.16–1.20 与 3.62–3.83），
 * 此刻判负会让总门无故变红。要不要升级成判负，等基线稳定后由主理人决定。
 *
 * ## 为什么它可以在"红着"的状态下诞生（与"检查要绿着诞生"的关系）
 *
 * 门（gates）必须绿着诞生：一个诞生就红的门会训练出"这个红是正常的"耐受，
 * 07-quality-gate §4 那 6 条常驻 CSS WARN 就是先例（红了很久，没有一条变成 todo）。
 *
 * 诊断工具相反：它的第一次红就是它的产出。分界线不是红绿，是**有没有人把第一次红
 * 消费掉**——本脚本第一次跑出 18 条"查不到"，当天就变成 qa 改写走查单的 todo 清单；
 * 没人消费的红才是噪声。所以：
 *
 *   - 已经消费过的红 → 允许存在，但要在报告里给出**总数**，当作下一次的基线；
 *   - 没人消费的红 → 别留着，要么修掉，要么删掉这条检查。
 *
 * 不进 `gate-all.sh` 的理由有三条：① 分母只增不减（每改一次人话文案就多一条引文漂移），
 * ② 它衡量的是"文档新不新"而不是"代码对不对"，混进门里会稀释"门全绿"的含义，
 * ③ 误报率随走查单作者波动。要机器化就只做**增量告警**（比上次基线多才提示），不设 0 阈值。
 */

import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, '..');

const CHECKLIST_PATH = 'docs/07-e2e-checklist.md';
const LOCALE_PATHS = [
  'src/lib/i18n/zh-CN.ts',
  'src/lib/i18n/zh-TW.ts',
  'src/lib/i18n/en-US.ts',
  'src/lib/i18n/th.ts',
];

const VERBOSE = process.argv.includes('--verbose');

// ===== 归一化 =====

/** 占位符折叠后的替身。用不可见字符，避免和真实文案撞车。 */
const PH = '\u0000';

/**
 * 把「同一条文案的两种写法」折叠成一种。
 *
 * 折叠掉的东西**只包括"必然随数据变化"的部分**：
 *   - `{count}` / `{size}` 这类 i18n 占位符；
 *   - 阿拉伯数字（走查单里写「30 秒」，字典里可能是 `{n} 秒`）；
 *   - 单独的示例变量 N / M / G / X（走查单习惯写「共 N 条」）；
 *   - 加粗标记 `**`；
 *   - 连续空白；
 *   - 片段末尾的省略号（截断引用）。
 *
 * 折叠掉的是"数字从哪来"，不是"这句话说什么"——所以文案真的被改写时，
 * 归一化救不了它，仍然会报出来。这一点是这条检查敢用的前提。
 */
function normalize(text) {
  return (
    text
      .replace(/\*\*/g, '')
      // 全角/半角标点等价：`导入完成: N 条` 与 `导入完成：{n} 条` 是同一条文案的两种排版。
      // 只折叠标点，不折叠汉字——汉字变了才算文案变了。
      .replace(/[：]/g, ':')
      .replace(/[（]/g, '(')
      .replace(/[）]/g, ')')
      .replace(/[，、]/g, ',')
      .replace(/\{[a-zA-Z_][a-zA-Z0-9_]*\}/g, PH)
      // 嵌套引号里的内容是**示例数据**（「没有匹配「zzzz」的条目」里的 zzzz），
      // 字典那边是占位符（`没有匹配「{q}」的条目`）。两边一起折叠，否则每条
      // 带示例值的判据都会误报——这类引用是成立的，只是把变量填成了样例。
      .replace(/「[^「」]*」/g, `「${PH}」`)
      .replace(/[0-9]+/g, PH)
      .replace(/(^|[^A-Za-z])([NMGX])(?![A-Za-z])/g, `$1${PH}`)
      .replace(/\s+/g, ' ')
      .replace(/(…|\.\.\.)+\s*$/, '')
      .trim()
  );
}

/** 只留"这句话有没有中文"这个判断用。 */
function cjkCount(text) {
  const m = text.match(/[\u4e00-\u9fff\u3400-\u4dbf]/g);
  return m ? m.length : 0;
}

// ===== 读字典 =====

/**
 * 从 .ts 文件里抠出所有字符串字面量的值（跳过 // 与 /* *\/ 注释）。
 *
 * 为什么不用 import：这些是 .ts，node 直接 import 需要 loader；而这条检查只关心
 * "这段话在不在字典里"，把值抠出来做子串匹配已经足够，且不引入任何依赖。
 * 注释必须跳过——否则一段被注释掉的旧文案会让检查误判为"还在"。
 * 模块路径（`from './zh-CN'`）也必须跳过——否则字典条数会虚高一格。
 */
function extractStringValues(src) {
  const values = [];
  let i = 0;
  // 最近一个标识符词，以及它结束的位置——只用来识别 `import ... from './x'` 这类
  // **模块路径**，它们不是文案。判据要求"词与引号之间只有空白"，否则 `from: '从'`
  // 这种真键会被一起吃掉（键名也叫 from 是很常见的）。
  let lastWord = '';
  let lastWordEnd = -1;
  while (i < src.length) {
    const c = src[i];
    const next = src[i + 1];

    if ((c === '/' && next === '/') || (c === '/' && next === '*')) {
      // 注释里也可能出现 from/import 字样，先走注释分支，别污染 lastWord
      if (next === '/') {
        const nl = src.indexOf('\n', i);
        i = nl === -1 ? src.length : nl + 1;
        continue;
      }
      const end = src.indexOf('*/', i + 2);
      i = end === -1 ? src.length : end + 2;
      continue;
    }
    if (/[A-Za-z_$]/.test(c)) {
      let j = i;
      while (j < src.length && /[A-Za-z0-9_$]/.test(src[j])) j += 1;
      lastWord = src.slice(i, j);
      lastWordEnd = j;
      i = j;
      continue;
    }
    if (c === "'" || c === '"' || c === '`') {
      const quote = c;
      let j = i + 1;
      let buf = '';
      while (j < src.length) {
        const d = src[j];
        if (d === '\\') {
          buf += src[j + 1] ?? '';
          j += 2;
          continue;
        }
        if (d === quote) break;
        buf += d;
        j += 1;
      }
      // `import type { Messages } from './zh-CN'` 里的路径不是文案。不剔掉的话，它会被
      // 当成"字典里的一条"，而那四行 `xxx 628 条` 就会变成 `en-US 629 条`——一个只在
      // 英文那行多出来的数字，看的人只会以为英文多了一条文案，实际多的是一个 import。
      // 判据（词与引号之间只有空白）保证 `from: '从'` 这种真键不被误杀。
      const isModuleSpecifier =
        (lastWord === 'from' || lastWord === 'import') &&
        src.slice(lastWordEnd, i).trim() === '';
      if (!isModuleSpecifier) values.push(buf);
      i = j + 1;
      continue;
    }
    i += 1;
  }
  return values;
}

function loadDictionary() {
  /** @type {{exact: Set<string>, normalized: string[], files: Record<string, number>, droppedEmpty: number, sha: string, bytes: number}} */
  const dict = { exact: new Set(), normalized: [], files: {}, droppedEmpty: 0, sha: '', bytes: 0 };
  const dh = createHash('sha256');
  for (const rel of LOCALE_PATHS) {
    const raw = readFileSync(resolve(ROOT, rel));
    dict.bytes += raw.length;
    // 字典侧同样留指纹：四本字典在工作区里被改过（D-191/D-192 那两批），
    // 只报走查单哈希时，两个人可能在"同一份走查单 + 不同字典"上得出不同结论。
    dh.update(raw);
    const values = extractStringValues(raw.toString('utf8'));
    dict.files[rel] = values.length;
    for (const v of values) {
      if (!v) continue;
      dict.exact.add(v);
      // 归一化后为空的条目（只有标点/省略号，如 `sep: '…'`）从**归一化语料**里剔除。
      // 注意方向：判据是 `字典条目.includes(片段)`，所以空条目**不会**让片段误命中
      // （`''.includes('abc')` 是 false，反过来才恒真）。剔除只是让语料干净、
      // 让"命中"这件事不依赖一条本身没有信息的条目——**不是**在防误命中。
      // （2026-11-04 产品评审指出：原先这里写的理由是错的，方向搞反了。）
      const norm = normalize(v);
      if (norm) dict.normalized.push(norm);
      else dict.droppedEmpty += 1;
    }
  }
  dict.sha = dh.digest('hex').slice(0, 8);
  return dict;
}

// ===== 读走查单 =====

/**
 * 抠出一行里所有引号片段（支持嵌套：「…再点「粘贴截图」…」）。
 *
 * 朴素的 `[^」]*` 正则在嵌套引号上会截断在第一个 `」`，把外层句子切两半，
 * 于是既报一条假的"查不到"，又漏掉真正那句话。所以按深度扫描。
 */
function parseQuotedFragments(line) {
  const out = [];
  const PAIRS = { '「': '」', '『': '』' };
  let i = 0;
  while (i < line.length) {
    const open = line[i];
    if (!PAIRS[open]) {
      i += 1;
      continue;
    }
    const close = PAIRS[open];
    let depth = 1;
    let j = i + 1;
    let buf = '';
    while (j < line.length && depth > 0) {
      const ch = line[j];
      if (ch === open) {
        depth += 1;
        buf += ch;
      } else if (ch === close) {
        depth -= 1;
        if (depth > 0) buf += ch;
      } else {
        buf += ch;
      }
      j += 1;
    }
    if (depth === 0 && buf.length > 0) out.push(buf);
    i = j;
  }
  return out;
}

/** 行号 + 行首的条目标号（如 `| 3.62 |` → `3.62`），用于报告定位。 */
function parseRowId(line) {
  const m = line.match(/^\|\s*~~?([0-9]+\.[0-9]+[a-z]?)~~?\s*\|/);
  if (m) return m[1];
  const m2 = line.match(/^\|\s*([0-9]+\.[0-9]+[a-z]?)\s*\|/);
  return m2 ? m2[1] : '';
}

/**
 * 一行里 `「」`/`『』` 的净深度。0 = 配平。
 *
 * 为什么要单独算这个、而不是看扫描器自己的 `depth`：**扫描器看不见"被它丢掉的东西"**。
 * 一个落单的 `「` 会让 `parseQuotedFragments` 从那里一直吃到行尾仍不闭合，
 * 于是 `depth === 0` 不成立、整段不 push —— **这一行后半段的所有引文被静默丢掉，且不报任何东西**。
 * 报告照样打出"查不到 0 条"，因为它压根没去看。
 *
 * 这不是假想：qa-lead 的 1.18 末尾少了一个 `」`，`「全删」` 那条引文因此从来没被扫过，
 * 而我上一版报告的"全绿"里它是瞎的。独立性是这个函数存在的全部理由——
 * 它与扫描器分别数一遍，配不平就当场指出来，而不是指望扫描器自证清白。
 */
function quoteBalance(line) {
  let d = 0;
  for (const ch of line) {
    if (ch === '「' || ch === '『') d += 1;
    else if (ch === '」' || ch === '』') d -= 1;
  }
  return d;
}

/**
 * 逐行括号统计：左端取扫描器的结果，右端**只扫括号深度、绝不经过扫描器**。
 *
 * 配平且没丢东西的行上有恒等式：`开括号数 − 顶层片段数 == 嵌套内层数`。
 * 每个顶层片段恰好消耗一个在 depth=0 处遇到的开括号；其余开括号都在 depth>0
 * 处遇到，就是"嵌套内层"。所以两边必然相等，与嵌套有几层无关。
 *
 * 一旦有片段被静默丢掉（落单的 `「` 让从那里到行尾配不上对、整段不 push），
 * 左端少算一个或多个片段，右端仍按括号原样计数 —— 等式立刻失衡。
 *
 * 这是「开 − 片段 ≥ 2」做不到的地方：`docs/07-e2e-checklist.md:175` 的合法双重嵌套
 * （外层一句套「导出保险库」「从备份恢复」两对）本来就是 开8 / 片段6 / 内层2，
 * 差恰好等于 2。拿 ≥2 当阈值必然把"嵌套"和"丢失"混成同一维，当场误报。
 * qa-lead 2026-11-06 复算给出 `lhs=2 eq=true`，这里让它成为判据本身。
 */
function bracketStats(line) {
  let opens = 0;
  let inner = 0;
  let depth = 0;
  for (const ch of line) {
    if (ch === '「' || ch === '『') {
      opens += 1;
      if (depth > 0) inner += 1;
      depth += 1;
    } else if (ch === '」' || ch === '』') {
      depth -= 1;
      // 多余的右括号不该让后续开括号被算成"外层"；夹到 0 后照常继续数。
      if (depth < 0) depth = 0;
    }
  }
  const frags = parseQuotedFragments(line).length;
  const lhs = opens - frags;
  return { opens, frags, inner, lhs, eq: lhs === inner };
}

/**
 * 唯一的"疑似丢失"判据。`loadChecklist()` 与 P9 探针**共用这一个函数**：
 * 判据只许有一份实现，否则"文档上跑的那套"和"探针里验的那套"会各自漂移，
 * 探针绿着却什么都没守住。把这里改回 `st.lhs >= 2`，P9 的 `:175` 形状 case 立刻变红。
 */
function isLostFragment(st) {
  return st.lhs !== st.inner;
}

function loadChecklist() {
  // 把输入文件的**内容指纹**一起带出去。理由不是洁癖：这一轮里我和 qa-lead 各自跑出
  // `347/280` 和 `354/288` 两组数，差 10 条，光看数字没法判断是"口径不同"还是"看的不是
  // 同一份文件"——最后是靠"差值正好落在命中类"反推出来的。一个 8 位短哈希把这类争论
  // 一次解决：哈希不同就是两份文件，先对齐文件再谈结论。
  const raw = readFileSync(resolve(ROOT, CHECKLIST_PATH));
  const lines = raw.toString('utf8').split(/\r?\n/);
  /** @type {{line: number, row: string, text: string}[]} */
  const items = [];
  /** @type {{line: number, row: string, balance: number}[]} */
  const unbalanced = [];
  // 非表格行里也有引文（章节导语、修订记录、书写规范本身），这里既不算命中也不算跳过。
  // 不报出来的后果是"分母看起来很大、盲区没人知道"——qa-lead 实测这些行里有 1 行
  // 引着一条代码里 0 命中的死文案，已经存在很久，没有任何东西提示过它。
  let proseLines = 0;
  let proseOpens = 0;
  /** @type {{line: number, row: string, opens: number, frags: number, inner: number, lhs: number}[]} */
  const suspicious = [];
  const countOpens = (s) => (s.match(/[「『]/g) ?? []).length;
  lines.forEach((line, idx) => {
    if (!line.trimStart().startsWith('|')) {
      const n = countOpens(line);
      if (n > 0) {
        proseLines += 1;
        proseOpens += n;
      }
      return;
    }
    const row = parseRowId(line);
    const balance = quoteBalance(line);
    if (balance !== 0) unbalanced.push({ line: idx + 1, row, balance });
    const frags = parseQuotedFragments(line);
    // 逐行恒等式：开 − 顶层片段 == 嵌套内层。右端只扫括号深度、不经过扫描器，
    // 所以扫描器静默丢片段时它仍会失衡。判据在 isLostFragment()，与 P9 探针共用一份实现。
    const st = bracketStats(line);
    if (isLostFragment(st)) suspicious.push({ line: idx + 1, row, ...st });
    for (const text of frags) {
      items.push({ line: idx + 1, row, text });
    }
  });
  // 页眉只报 sha256 + 字节数，**不再报行数**：`wc -l` 数换行符（当前 237），
  // `split(/\r?\n/).length` 因末尾换行多一个空串元素（当前 238）。两套口径印在同一行，
  // 等于亲手把"报数带锚"要消灭的歧义挪到另一列。字节数 + 内容哈希是单一事实来源。
  // 另：这里的 sha256 是**文件内容**哈希，不是 `git hash-object` 的 blob 哈希（后者为 d7843f19），
  // 两套算法不要拿来对数。
  return {
    items,
    unbalanced,
    suspicious,
    prose: { lines: proseLines, opens: proseOpens },
    sha: createHash('sha256').update(raw).digest('hex').slice(0, 8),
    bytes: raw.length,
  };
}

// ===== 匹配 =====

/**
 * 这段归一化片段**本身有没有信息量**。
 *
 * `normalize()` 会把 `{var}`、数字、示例变量 N/M/G 全折成同一个占位符，所以一条
 * 只由它们组成的引文（「N」「共 5 份」「30 秒」）归一化后就是 `\0` 或 `\0 秒` / `共 \0 份`——
 * 这些是**大多数带占位符的文案的子串**，于是必然"命中"。
 *
 * 那种命中不构成证据：它只证明"字典里有一条带占位符的文案"，不证明"界面这么说"。
 * 而它的后果是把片段从"查不到"挪进"命中"——**让报告看起来更干净**，与本工具的价值相反。
 *
 * 所以归一化路径要求片段至少有 2 个汉字或 4 个连续拉丁字母；达不到就单独归为
 * 「无法核对」，**既不算命中也不算查不到**。原样命中（`exact`）不受此限——
 * 一字不差对上一条字典文案本身就是强证据，哪怕只有两个字（如「全删」）。
 */
function hasSubstance(norm) {
  return cjkCount(norm) >= 2 || /[A-Za-z]{4}/.test(norm);
}

function isHit(fragment, dict) {
  if (dict.exact.has(fragment)) return 'exact';
  const norm = normalize(fragment);
  if (!norm) return false;
  if (dict.normalized.some((d) => d.includes(norm))) {
    return hasSubstance(norm) ? 'fuzzy' : 'weak';
  }

  // 缩略引用（带省略号）：把片段按 … 切成若干段，要求**每一段都落在同一条字典文案里**。
  // 例：「导入失败，没有写入任何数据……不用重新选择文件」是 `nothing_written` 的中间省略写法，
  // 首尾两段都在那条文案里 → 这条引用成立，不该报。而「…请升级 PasswordMS 后再恢复」
  // 的第二段与字典（`请升级 PasswordMS 后重试`）不符 → 仍然报出来，因为那是真的改过文案。
  const hasEllipsis = /…|\.\.\./.test(fragment);
  const segs = fragment
    .split(/…|\.\.\./)
    .map((s) => normalize(s))
    .filter((s) => s && cjkCount(s) >= 4);
  if (segs.length >= 1) {
    const ok = dict.normalized.some((d) => segs.every((s) => d.includes(s)));
    // 开头的省略号（「…接下来请用…解锁。」）只剩一段，也是合法的截断引用。
    if (ok && (hasEllipsis || segs.length >= 2)) return 'abbrev';
  }
  return false;
}

// ===== 跳过规则 =====

/**
 * 每条规则 = { id, reason, test(fragment, dict) }。
 * 顺序有意义：先跳"根本不是文案"的，再跳"是文案但没法机检"的。
 *
 * 关于"跳过的正当性"（产品评审的标准）：跳过率**不由比例保证**，由"每一条跳过都有一句
 * 能查的理由"保证。所以这里分两种规则：
 *   - **可判型**（R1/R2/R3a/R3b/R4/R5）：理由就是那条正则/那个集合，逐条成立，能当场复核；
 *   - **判定型**（R6/R3c）：没有可判形态，只能人写理由，逐条列在 `SKIP_ITEM_REASONS` 里。
 * 原先 R3 把「设置 → 语言」和「加密副本 + 恢复入口 + mtime 说明」混成一个 23 条的大桶，
 * 理由是"含拼接符"——那句话对两类都成立，但说服力不同：前者每一段都是字典里真有的标签
 * （可判、甚至能顺手验），后者是作者对屏幕的概括（只能人写）。混在一个桶里，
 * 桶涨了就没人分得清是"路径变多了"还是"概括变多了"。拆开后每个桶的理由都是它自己的。
 */
const SKIP_RULES = [
  {
    id: 'R1/no-cjk',
    reason: '不含中文：路径、命令、英文标识、代码片段，不是字典文案',
    test: (t) => cjkCount(t) === 0,
  },
  {
    id: 'R2/path-cmd',
    reason: '含路径或命令特征（/ \\ < > :// % -- 或扩展名），是操作指令不是界面文案',
    test: (t) => /[/\\<>]|:\/\/|%[A-Z_]+%|--|\.(svg|vault|json|bak|md|exe|apk)\b/.test(t),
  },
  {
    id: 'R3a/nav-path',
    reason:
      '导航路径：整句严格形如 ^A → B( → C)*$，每段都是字典里真有的菜单/分区标签，段内不含其它标点——屏幕上是两级菜单，不存在这一整句',
    // 可判型：不但"理由成立"，而且**顺手验了一遍**——每段都得在字典里原样存在。
    // 形态判据不能省：只看"含 → 且每段在字典里"，`「密码 → 用户名」` 这种对注入顺序的
    // 描述也会被当成菜单路径收走；而 `A → → B` / `→ A → B` 里的空段会被 filter(Boolean)
    // 悄悄抹平，等于规则自己放宽了自己。所以先钉 `^段(\s*→\s*段)+$`（段内不许出现 →
    // 与常见分隔符/标点，首尾都必须是段），再逐段查字典。哪天某个菜单改名，这条会掉回
    // "查不到"，正是我们要的。反向验证在 P10/r3a-shape 探针里。
    test: (t, dict) => {
      const s = t.replace(/\*\*/g, '').trim();
      if (
        !/^[^\s→·+｜|：:，,。！？、（）()]+(?:\s*→\s*[^\s→·+｜|：:，,。！？、（）()]+)+$/.test(s)
      ) {
        return false;
      }
      const segs = s.split('→').map((x) => x.trim());
      return segs.length >= 2 && segs.every((x) => dict.exact.has(x));
    },
  },
  {
    id: 'R3b/list-row',
    reason:
      '列表行样式：类型标签 + 用户数据（分组名/条数），中间用 · 拼——只有标签那一段在字典里，整行是拼出来的',
    test: (t, dict) => {
      if (!/[·｜|]/.test(t) || t.includes('→')) return false;
      const segs = t
        .replace(/\*\*/g, '')
        .split(/[·｜|]/)
        .map((s) => s.trim())
        .filter(Boolean);
      if (segs.length < 2) return false;
      if (segs.some((s) => cjkCount(s) > 14)) return false; // 长句不是"行样式"
      return segs.some((s) => dict.exact.has(s));
    },
  },
  {
    id: 'R3c/composed',
    reason: '作者对屏幕的概括（"几条文案拼起来的样子"），没有单条字典文案可对照——理由逐条写在下面',
    test: (t) => /[→←·+｜|]/.test(t),
  },
  {
    id: 'R3e/label-pair',
    reason: '形如「甲: 乙」的两段短标签（如「分组: 未分组」「导入完成: 3 条」）：由两条文案拼出来，没有单条字典文案可对照',
    test: (t) => {
      const s = t.replace(/\*\*/g, '').replace(/：/g, ':');
      if (!s.includes(':')) return false;
      // 有句读就不算标签对——那是句子（例：「删除失败：备份还在这台电脑上」）。
      if (/[。！？,；]/.test(s)) return false;
      const [left, right] = s.split(':', 2);
      return cjkCount(left) <= 10 && cjkCount(right) <= 10;
    },
  },
  {
    id: 'R4/too-short',
    reason: '短于 4 个汉字：多为示例数据（分组名、条目名）或通用词，命中与否不指示文案漂移',
    test: (t) => cjkCount(t.replace(/\s/g, '')) < 4,
  },
  {
    id: 'R5/numeric',
    reason: '纯数字 + 单位（秒/条/份/个）：数值随配置与数据变化，不是固定文案',
    test: (t) => {
      const rest = t
        .replace(/\*\*/g, '')
        .replace(/[0-9NMGPX\s]/g, '')
        .replace(/[秒条份个组位次天年月]/g, '')
        .replace(/[：:，,。.、（）()]/g, '');
      return rest.length === 0;
    },
  },
  {
    id: 'R6/not-copy',
    reason: '人工判定为叙述性引文（比喻/复述别人的话/形状说明），不是界面文案；新增条目请补理由',
    // 逐条附理由——不做模式匹配。宁可手写清单，也不要再发明一条会误杀真实文案的正则。
    test: (t) => NOT_COPY.some(([frag]) => frag === t),
  },
];

/**
 * 人工清单：经逐条读上下文，确认「这不是界面文案」的引文。
 * 每条都必须能被下一个人复核，所以带理由。
 */
const NOT_COPY = [
  ['写漏了不会红', '6.§0 导语里的比喻，说的是"漏写测试也不报错"这件事'],
  ['读不出、你自己看着办', '1.17 判据里的反例（"不许只说这句"），引的是被否定的说法'],
  ['一条都没进', '3.82 判据的通俗复述，界面真文案在「导入失败，没有写入任何数据……」那条'],
  ['从哪个分组进来', '3.44 的提问式复述，不是界面元素'],
  ['一共有几批', '1.17 描述"程序数不出来"的对象，不是界面文案'],
  ['purge 不过线', '2.6 备注里的内部术语引用（协议层说法）'],
  ['两者都留', '3.79 判据里指"缺少的那个按钮"，界面没有这个词'],
  ['扫进来不行、粘进来能存', '3.49 判据的反例描述（"不许出现"），不是界面文案'],
  ['复制完口令就空了、等几十秒才回来', '3.74 判据的反例描述（"不该出现"），不是界面文案'],
  ['框里 9999、实际 120', '3.76 判据的反例描述（"不该出现"），不是界面文案'],
  ['标题被压成两三个字 + 分组徽标占满头部', '3.31 判据的反例描述（"不得出现"），不是界面文案'],
  ['空搜索框 + 筛过的列表', '3.37 判据的反例描述（"不再是"），不是界面文案'],
  ['两者皆无 → 原生读图', '3.56 描述判定顺序的内部逻辑，不是界面文案'],
  ['指纹: 16 位小写 hex', '3.15 描述"这一行长什么样"，真值随设备指纹变化，无固定文案可对'],
  ['加密副本（非明文）+ 恢复入口 + mtime 说明', '1.12 对确认文案"要包含哪三件事"的概括，不是逐字引用'],
  ['指纹', '3.15/3.16 的字段名简称（整行形如「指纹: …」）'],
  ['关于', '3.19 指设置页里的那个分区标题，两字通用词'],
  ['全部条目', '3.34/3.45 顶栏标题的说明性引用，短且与字典键 title 不同源'],
  ['附近的设备', '3.17 引的是 **Android 系统权限名**（系统弹窗里的字），不在本仓字典里，改不了也不该改'],
  ['复制文件地址', '1.20 引的是 **Windows 资源管理器右键菜单项**，系统文案，不是本应用文案'],
  ['这组没数据', '3.40 的判据是"不能读成「这组没数据」"——引的是被否定的错误理解'],
  ['拖了没反应', '3.63 的判据是"不许出现「拖了没反应」"——引的是被否定的症状描述'],
  ['截图在剪贴板但没有填充上去', '3.52 引的是**主人报缺陷时的原话**，不是界面文案'],
  ['不是备份文件', '3.77 的判据是"不能报「不是备份文件」"——引的是被否定的错误提示'],
  ['共 5 份（21.4 MB）· 其中 2 份要用保存时的密码', '1.16 描述摘要行"三件事挤在一行"的样子；份数与体积随数据和单位换算变化，无固定文案可对'],
  ['· Commander 办公', '3.43 的判据是"**不再出现**「· Commander 办公」"——引的是被否定的旧样式（组名重复），不是界面文案'],
  ['导入完成: 0 条 · 重复跳过 3 条 · 合并分组 1 个', '3.83 描述结果行的三段拼接；三个数字随文件内容变化，没有单条字典文案'],
  ['重复跳过 n 条 · 合并同名分组 n 个', '6.3 同一结果行的省略写法（n 是变量），同上'],
  ['最近使用 → 最近修改', '7.10 描述未钉区**兜底排序规则的内部顺序**（不是屏幕上的一行文案，字典里没有、也不该有）'],
];

/**
 * 跳过项的逐条理由（判定型规则必须有，可判型不需要）。
 * 只从 NOT_COPY 派生，不另建一份——两份清单必然漂移，而漂移的那一份不会有任何告警。
 */
const SKIP_ITEM_REASONS = new Map(NOT_COPY);

// ===== 自检（探针） =====

/**
 * 为什么要在每次运行时自检：这条检查最大的失效方式**不是报错，而是悄悄全绿**。
 *
 * 只要 `normalize()` 被放松一点（比如为了少几条误报，把汉字也折了），所有片段都会
 * "命中"，于是报告变成"0 条查不到"——而它看起来完全正常。那时没有任何东西会告诉你
 * 检查已经废了。所以探针不测文案，**测匹配器本身**：
 *
 *   P1 命中：随便取一条字典文案原样放进去，必须命中；
 *   P2 归一化：把该文案里的 `{占位符}` 换成数字，必须仍然命中（证明占位符折叠在工作）；
 *   P3 汉字不折：把同一条文案的两个相邻汉字对调，必须**查不到**（证明折叠没放松到汉字）；
 *   P4 缩略：把一条长文案拦腰截断加 `…`，必须命中（证明缩略匹配在工作）。
 *
 * 探针的输入**从字典现取**，不写死任何一条文案——否则文案一改，探针自己先红，
 * 那就成了"又一个需要维护的假警报"。取不到合适样本时如实报 skip，不假装通过。
 */
function runProbes(dict, ctx) {
  const probes = [];
  const values = [...dict.exact].filter((v) => v.length >= 8 && cjkCount(v) >= 6);

  // P1：原样命中
  const base = values[0];
  probes.push({
    id: 'P1/exact-hit',
    expect: 'hit',
    frag: base,
    note: base === undefined ? '字典里没有足够长的中文文案，取不到样本' : '',
  });

  // P2：占位符折叠
  const withPh = values.find((v) => /\{[a-zA-Z_]+\}/.test(v));
  probes.push({
    id: 'P2/placeholder-hit',
    expect: 'hit',
    frag: withPh === undefined ? undefined : withPh.replace(/\{[a-zA-Z_]+\}/g, '7'),
    note: withPh === undefined ? '字典里没有带 {占位符} 的文案，取不到样本' : '',
  });

  // P3：汉字不折（相邻两字对调后必须查不到）
  let swapped;
  if (base !== undefined) {
    const chars = [...base];
    const i = chars.findIndex((c, k) => k > 0 && cjkCount(c) === 1 && cjkCount(chars[k - 1]) === 1);
    if (i > 0) {
      [chars[i - 1], chars[i]] = [chars[i], chars[i - 1]];
      swapped = chars.join('');
    }
  }
  probes.push({
    id: 'P3/hanzi-not-folded',
    expect: 'miss',
    frag: swapped,
    note: swapped === undefined ? '找不到可对调的相邻汉字，取不到样本' : '',
  });

  // P4：缩略匹配
  let abbr;
  const long = values.find((v) => cjkCount(v) >= 16);
  if (long !== undefined) {
    const chars = [...long];
    const head = chars.slice(0, 8).join('');
    const tail = chars.slice(-8).join('');
    abbr = `${head}……${tail}`;
  }
  probes.push({
    id: 'P4/abbrev-hit',
    expect: 'hit',
    frag: abbr,
    note: abbr === undefined ? '字典里没有 ≥16 汉字的文案，取不到样本' : '',
  });

  // P5：无意义输入不得命中。
  // `''` 是 `includes` 的单位元，任何"折叠过头"的实现都会在这里露出马脚：
  // 空串/纯空白/纯省略号一旦被判为命中，说明匹配器已经能把任意片段匹配上。
  probes.push({ id: 'P5/empty-miss', expect: 'miss', frag: '', note: '' });
  probes.push({ id: 'P5b/ellipsis-miss', expect: 'miss', frag: '…', note: '' });
  probes.push({ id: 'P5c/space-miss', expect: 'miss', frag: '   ', note: '' });

  // P6：语料里不许出现模块路径。
  // 这是一条**不变量**探针：它不测匹配器，测的是"喂进来的语料是不是文案"。
  // 之前 `en-US.ts` 的 `from './zh-CN'` 被当成一条文案，于是四行字典计数变成
  // `628 / 628 / 629 / 628`——一个只在英文那行多出来的数字，读的人只会以为
  // 英文多了一条文案。数字不对，报告就不可信。
  probes.push({
    id: 'P6/no-module-path',
    expect: '无模块路径混入语料',
    check: () => {
      const leaked = [...dict.exact].filter((v) => /^\.{0,2}\/[^\s]*$/.test(v));
      return leaked.length === 0 ? null : `混入 ${leaked.join(' , ')}`;
    },
  });

  // P8：四本字典的条数必须相等。
  // P6 是"针对已知污染源"的规则（模块路径），抓得住这一次，抓不住下一次的形态
  // （注释里带引号的句子、模板字面量、`as const` 里多出来的值……每一种都要再补一条）。
  // 而"四个数相等"是**不变量**：不需要预知污染长什么样，任何形态的混入都会让它红。
  // 依据是 `src/lib/i18n.test.ts` 已经断言了四语言键集合与 zh-CN 完全一致——所以
  // 条数不等只可能是"抠出来当文案的东西"变了，不是"字典合法地多了一条"。
  probes.push({
    id: 'P8/four-counts-equal',
    expect: '四本条数相等',
    check: () => {
      const pairs = LOCALE_PATHS.map((p) => [p.split('/').pop(), dict.files[p]]);
      const vals = new Set(pairs.map(([, n]) => n));
      if (vals.size === 1) return null;
      return pairs.map(([f, n]) => `${f} ${n}`).join(' / ');
    },
  });

  // P9：逐行等式 `开 − 顶层片段 == 嵌套内层`。
  // 合成三类输入，全部走**与文档同一份判据** isLostFragment(bracketStats(line))：
  //   ① :175 形状（外层一句套多个内层）→ 合法，绝不能算丢失；
  //   ② 落单在最前 → 从第一个「 起整段不 push，0 片段，必须算丢失；
  //   ③ 落单在中间 → 之前闭合的对子照常入账、之后全丢，必须算丢失。
  // ③ 是 qa-lead 用 1.18 修前 13 条 / 修后 15 条实测纠正过的失效模式：不是"整行 0 条"，
  // 而是"落单处之后全丢"。① 同样不能少——否则下一个人为了让探针绿会去动解析器。
  // 把判据改回 `st.lhs >= 2`，①（lhs=2）会立刻变红，②③ 也会被这份 case 表点出来。
  probes.push({
    id: 'P9/no-lost-fragment',
    expect: '等式判据能区分合法嵌套与静默丢失',
    check: () => {
      const cases = [
        {
          name: ':175 形状（合法双重嵌套）',
          line: '| x | 提示「请在那台「导出保险库」，到另一台「从备份恢复」，然后再重新配对」 |',
          lost: false,
        },
        { name: '落单在最前', line: '| x | 「甲「乙」 |', lost: true },
        { name: '落单在中间', line: '| x | 先「甲」「乙」「丙 |', lost: true },
      ];
      const bad = [];
      for (const c of cases) {
        const st = bracketStats(c.line);
        const got = isLostFragment(st);
        if (got !== c.lost) {
          bad.push(
            `${c.name}: 期望 lost=${c.lost} 实得 ${got}（开${st.opens}/片段${st.frags}/内层${st.inner}/差${st.lhs}）`,
          );
        }
      }
      return bad.length === 0 ? null : bad.join(' ; ');
    },
  });

  // P9b：当前走查单本身也不许有等式失衡的行。P9 验判据、P9b 验输入，分开报——
  // 把判据改回 ≥2 时，P9 先红（:175 形状），P9b 也会把 :175 那一行点出来。
  probes.push({
    id: 'P9b/doc-clean',
    expect: '当前走查单没有等式失衡的行',
    check: () => {
      const bad = ctx?.suspicious ?? [];
      if (bad.length === 0) return null;
      return bad
        .map((b) => `:${b.line} 开${b.opens}/片段${b.frags}/内层${b.inner}/差${b.lhs}`)
        .join(' ; ');
    },
  });

  // P7：括号配平检测本身（输入是合成串，故意写坏）。
  // 这条测的不是"文档没错"，而是"文档错了我会不会发现"。落单括号是我这套实现里
  // 唯一一个**静默**丢数据的入口：配不上对就整段不 push，报告照样全绿。
  //
  // 三个 case 是 qa-lead 用 1.18 修前 13 条 / 修后 15 条实测定下来的形状：
  //   ① 落单在最前 → 0 片段（第一个「 一直吃到行尾仍不闭合）；
  //   ② 落单在中间 → 2 片段（之前闭合的对子照常入账，之后全丢）——**不是整行 0 条**，
  //      上一版把这里写成"整行 0 条"是错的，已撤回；
  //   ③ 外层套多个内层（:175 形状）→ 1 片段且等式仍成立，这是合法形状、不许被改坏。
  // 另留「正常两对」「多一个右括号」两个基础 case 作形状回归。③ 在这里再钉一次的理由：
  // 它是合法形状，下一个人为了让探针绿最容易去动解析器，而不是去改判据。
  probes.push({
    id: 'P7/unbalanced-detected',
    expect: '三种落单/嵌套形状都按实测条数解析',
    check: () => {
      const cases = [
        { name: '正常两对', line: '| x | 点「复制截图」再点「粘贴」 |', frags: 2, bal: 0 },
        { name: '落单在最前', line: '| x | 「甲「乙」 |', frags: 0, bal: 1 },
        { name: '落单在中间', line: '| x | 先「甲」「乙」「丙 |', frags: 2, bal: 1 },
        {
          name: ':175 形状（外层套两个内层）',
          line: '| x | 提示「请在那台「导出保险库」，到另一台「从备份恢复」」 |',
          frags: 1,
          bal: 0,
          eq: true,
        },
        { name: '多一个右括号', line: '| x | 多了一个「复制」」 |', frags: 1, bal: -1 },
      ];
      const bad = [];
      for (const c of cases) {
        const got = parseQuotedFragments(c.line).length;
        const bal = quoteBalance(c.line);
        const eq = bracketStats(c.line).eq;
        if (got !== c.frags || bal !== c.bal || (c.eq !== undefined && eq !== c.eq)) {
          bad.push(
            `${c.name}: 片段 ${got}/${c.frags} 配平 ${bal}/${c.bal}` +
              (c.eq !== undefined ? ` 等式 ${eq}/${c.eq}` : ''),
          );
        }
      }
      return bad.length === 0 ? null : bad.join(' ; ');
    },
  });

  // P10：R3a 的形态判据本身也要被验（可判型规则不许只靠"写了理由"就算数）。
  // 正向：从字典现取两个真标签串成 `A → B` / `A → B → A`，必须被 R3a 收走；
  // 反向：带 `→` 但不是导航路径的描述（夹了别的标点 / 空段）不许被 R3a 收走，
  //       且必须被某条跳过规则接住 —— 否则它会从"跳过"掉进"查不到"，凭空多一条噪声。
  // 产品评审 2026-11-06 指出：`「密码 → 用户名」` 两段都可能在字典里，只看"每段命中"
  // 会把它误收成菜单路径；形态判据 + 反向验证就是为这一侧补的。
  probes.push({
    id: 'P10/r3a-shape',
    expect: 'R3a 只收 ^A → B( → C)*$，其余 → 描述仍被跳过',
    check: () => {
      const r3a = SKIP_RULES.find((r) => r.id === 'R3a/nav-path');
      const r3c = SKIP_RULES.find((r) => r.id === 'R3c/composed');
      const labels = [...dict.exact].filter(
        (v) => cjkCount(v) >= 2 && /^[^\s→·+｜|：:，,。！？、（）()\/\\<>.%\-]+$/.test(v),
      );
      if (labels.length < 2 || !r3a || !r3c) return '取不到两个可拼的字典标签，或规则缺失';
      const [a, b] = labels;
      const cases = [
        { name: '正向 A → B', frag: `${a} → ${b}`, byR3a: true },
        { name: '正向 A → B → A', frag: `${a} → ${b} → ${a}`, byR3a: true },
        { name: '反向 夹了别的标点', frag: `${a} → ${b}，再确认`, byR3a: false },
        { name: '反向 中间空段', frag: `${a} → → ${b}`, byR3a: false },
        // 这两条专门钉住旧实现的 `filter(Boolean)`：前导/尾随的 → 会被它悄悄抹平，
        // 于是"形态不对"的串照样被 R3a 收走。去掉形态判据，P10 立刻红。
        { name: '反向 前导空段', frag: `→ ${a} → ${b}`, byR3a: false },
        { name: '反向 尾随空段', frag: `${a} → ${b} →`, byR3a: false },
      ];
      const bad = [];
      for (const c of cases) {
        const gotA = r3a.test(c.frag, dict);
        const first = SKIP_RULES.find((r) => r.test(c.frag, dict));
        const landed = first?.id ?? 'none（会掉进查不到）';
        const stillSkipped =
          c.byR3a || (landed !== 'none（会掉进查不到）' && landed !== 'R3a/nav-path' && r3c.test(c.frag, dict));
        if (gotA !== c.byR3a || !stillSkipped) {
          bad.push(`${c.name}: R3a=${gotA}/${c.byR3a} 首命中=${landed}`);
        }
      }
      return bad.length === 0 ? null : bad.join(' ; ');
    },
  });

  return probes.map((p) => {
    if (p.check) {
      const err = p.check();
      return { ...p, ok: err === null, got: err === null ? '干净' : err };
    }
    if (p.frag === undefined) return { ...p, ok: null };
    const got = isHit(p.frag, dict) ? 'hit' : 'miss';
    return { ...p, ok: got === p.expect, got };
  });
}

// ===== 主流程 =====

function main() {
  const dict = loadDictionary();
  const { items, unbalanced, suspicious, prose, sha, bytes } = loadChecklist();

  const hits = [];
  const misses = [];
  const weaks = [];
  const skips = new Map();

  for (const item of items) {
    // 先判命中、再判跳过：短片段（≤3 字）里既有"用户自造的分组名"，也有「全删」「未分组」
    // 这种真标签。先查字典能把真标签算作命中（不产生噪音），只有查不到的才落到跳过规则里
    // 去当"无法判定"——这样跳过清单里不会有"明明存在却被跳过"的项。
    const hit = isHit(item.text, dict);
    if (hit === 'weak') {
      weaks.push(item);
      continue;
    }
    if (hit) {
      hits.push({ ...item, kind: hit });
      continue;
    }
    const rule = SKIP_RULES.find((r) => r.test(item.text, dict));
    if (rule) {
      if (!skips.has(rule.id)) skips.set(rule.id, { rule, items: [] });
      skips.get(rule.id).items.push(item);
      continue;
    }
    misses.push(item);
  }

  const line = '─'.repeat(96);
  const skippedTotal = [...skips.values()].reduce((n, s) => n + s.items.length, 0);
  const kindCount = (kind) => hits.filter((h) => h.kind === kind).length;
  // 判定型规则（理由必须人写）里还没写理由的条目：跳过本身照旧成立，但必须在**总行**上
  // 出现——写在桶里的 ⚠️ 太容易被跳过段落本身淹没，而"没写理由的跳过"正是这个工具
  // 唯一会让盲区悄悄变大的路径。
  const unreasoned = [...skips.values()]
    .filter((b) => b.rule.id === 'R3c/composed' || b.rule.id === 'R6/not-copy')
    .flatMap((b) => b.items.filter((s) => !SKIP_ITEM_REASONS.get(s.text)));
  console.log('');
  console.log('=== E2E 引文校验（只报警，不判负）===');
  console.log(`走查单：${CHECKLIST_PATH}  [sha256 ${sha} · ${bytes} B]`);
  console.log(
    `字典  ：${LOCALE_PATHS.map((p) => `${p.split('/').pop()} ${dict.files[p]} 条`).join(' · ')}  [sha256 ${dict.sha} · ${dict.bytes} B]`,
  );
  console.log(
    '  （页眉是文件内容 sha256 前 8 位 + 字节数；不是 `git hash-object` 的 blob 哈希，也不再印行数——避免 wc -l / split 两套口径。）',
  );
  console.log(line);
  console.log(
    `引号片段 ${items.length} 条 ｜ 参与检查 ${items.length - skippedTotal} 条 ｜ ` +
      `命中 ${hits.length} 条（原样 ${kindCount('exact')} / 占位符归一 ${kindCount('fuzzy')} / 缩略 ${kindCount('abbrev')}） ｜ ` +
      `**查不到 ${misses.length} 条** ｜ 跳过 ${skippedTotal} 条（无法判定，规则见下）` +
      (unbalanced.length > 0 ? ` ｜ **落单括号 ${unbalanced.length} 行（后半段没被扫）**` : '') +
      (unreasoned.length > 0 ? ` ｜ **跳过未写理由 ${unreasoned.length} 条**` : ''),
  );

  // ---- 落单括号（先报这个：它会让上面的数字本身失真）----
  if (unbalanced.length > 0) {
    console.log('');
    console.log(`【落单括号】${unbalanced.length} 行（「」/『』没配平，这些行**从这个括号起到行尾都没被扫**）`);
    for (const u of unbalanced) {
      const where = u.row ? u.row.padEnd(6) : '      ';
      const dir = u.balance > 0 ? '少一个 」' : '多一个 」';
      console.log(`  :${String(u.line).padEnd(4)} ${where}${dir}`);
    }
    console.log('  这不是"格式问题"：扫描器配不上对就整段不 push，于是那些引文既不算命中也不算查不到，');
    console.log('  报告照样是"查不到 0 条"——因为它压根没去看。补上括号再跑，数字会变。');
  }

  // ---- 未纳入（既不进分子也不进分母）----
  // 这一段不参与任何判定，但必须打出来：**"没查"和"查了没问题"在报告里长得一样**，
  // 这是所有诊断工具最常见的失信方式。qa-lead 是靠人工挖才发现在这些行里
  // （走查单 :19）已经引着一句代码里 0 命中的死文案——它不是未来的风险，是已经发生过、
  // 没人看见。不把它印在报告里，就等于继续假装分母是全部。
  if (prose.lines > 0) {
    console.log('');
    console.log(
      `【未纳入】非表格行含开括号 ${prose.lines} 行 ${prose.opens} 处（章节导语 / 修订记录 / 书写规范本身）`,
    );
    console.log('  这些引文**既不进分子也不进分母**：本节只校表格行（判据所在处）。');
    console.log('  非表格行里的引文请人工过一眼——它们不会被本脚本报出来，但同样会误导走查的人。');
  }

  // ---- 疑似整段丢失 ----
  if (suspicious.length > 0) {
    console.log('');
    console.log(`【疑似丢失】${suspicious.length} 行（逐行等式 开 − 顶层片段 == 嵌套内层 失衡）`);
    for (const s of suspicious) {
      console.log(
        `  :${String(s.line).padEnd(4)} ${(s.row || '').padEnd(6)}开 ${s.opens} / 片段 ${s.frags} / 内层 ${s.inner} / 差 ${s.lhs}`,
      );
    }
    console.log('  右端只扫括号深度、不经过扫描器：合法嵌套（差恰好等于内层数）不会报；');
    console.log('  失衡才说明扫描器静默丢过片段。落单括号行也会同时出现在这里（两个独立信号）。');
  }

  // ---- 查不到 ----
  console.log('');
  console.log(`【查不到】${misses.length} 条（按行号）`);
  if (misses.length === 0) {
    console.log('  （无）');
  } else {
    for (const m of misses) {
      const where = m.row ? m.row.padEnd(6) : '      ';
      console.log(`  :${String(m.line).padEnd(4)} ${where}「${m.text}」`);
    }
    console.log('');
    console.log('  处理方式（二选一，都不改代码）：');
    console.log('    a) 引文改成字典里现在的那句话（改完重跑，本行会移到"命中"）；');
    console.log('    b) 判据确实要的是"含义"而不是原文 —— 那就去掉引号，写成"含义是……"，');
    console.log('       并说明为什么不该逐字比对（例如文案本身允许随用户数据变化）。');
  }

  // ---- 无法核对 ----
  if (weaks.length > 0) {
    console.log('');
    console.log(`【无法核对】${weaks.length} 条（片段只由占位符/数字/单位组成，命中不构成证据）`);
    for (const w of weaks.slice(0, VERBOSE ? weaks.length : 8)) {
      const where = w.row ? w.row.padEnd(6) : '      ';
      console.log(`  :${String(w.line).padEnd(4)} ${where}「${w.text}」`);
    }
    if (!VERBOSE && weaks.length > 8) console.log(`  …另有 ${weaks.length - 8} 条（--verbose 看全部）`);
    console.log('  这类引文请改写成带汉字的样子（如「共 N 份」→「共 3 份备份」），否则机器无法判断。');
  }

  // ---- 跳过 ----
  console.log('');
  console.log(`【跳过】${skippedTotal} 条（按规则，跳过是判断不是忽略）`);
  const ruleOrder = new Map(SKIP_RULES.map((r, i) => [r.id, i]));
  const buckets = [...skips.values()].sort(
    (a, b) => (ruleOrder.get(a.rule.id) ?? 99) - (ruleOrder.get(b.rule.id) ?? 99),
  );
  for (const { rule, items: list } of buckets) {
    console.log(`  ${rule.id.padEnd(14)} ${String(list.length).padStart(3)} 条 — ${rule.reason}`);
    const sample = VERBOSE ? list : list.slice(0, 3);
    for (const s of sample) {
      const why = SKIP_ITEM_REASONS.get(s.text);
      console.log(`      :${String(s.line).padEnd(4)} 「${s.text}」${why ? `  ← ${why}` : ''}`);
    }
    if (!VERBOSE && list.length > 3) console.log(`      …另有 ${list.length - 3} 条（--verbose 看全部）`);
    // 判定型规则（理由逐条写）必须每条都写全；缺一条就当场说，别让"没写理由"混成"没理由"。
    if (rule.id === 'R3c/composed' || rule.id === 'R6/not-copy') {
      const missing = list.filter((s) => !SKIP_ITEM_REASONS.get(s.text));
      if (missing.length > 0) {
        console.log(`      ⚠️  这 ${missing.length} 条还没写理由：${missing.map((m) => m.text).join(' , ')}`);
      }
    }
  }

  // ---- 命中（可选） ----
  if (VERBOSE) {
    console.log('');
    console.log(`【命中】${hits.length} 条`);
    for (const h of hits) {
      const where = h.row ? h.row.padEnd(6) : '      ';
      console.log(`  :${String(h.line).padEnd(4)} ${where}（${h.kind}）「${h.text}」`);
    }
  }

  // ---- 自检 ----
  const probes = runProbes(dict, { suspicious });
  const failed = probes.filter((p) => p.ok === false);
  const skippedProbes = probes.filter((p) => p.ok === null);
  console.log('');
  console.log(`【自检】匹配器本身（探针 ${probes.length - skippedProbes.length} 条，输入取自字典/语料）`);
  for (const p of probes) {
    const mark = p.ok === null ? '—' : p.ok ? '✓' : '✗';
    const tail = p.ok === null ? `未跑（${p.note}）` : p.ok ? '' : `期望 ${p.expect} 实得 ${p.got}`;
    console.log(`  ${mark} ${p.id.padEnd(22)}${tail}`);
    if (p.ok === null && VERBOSE) console.log(`      样本：${p.note}`);
  }
  if (failed.length > 0) {
    console.log('');
    console.log('  ⚠️  自检失败：**上面的"查不到"结论不可信**，先修好再读结果。');
    console.log('      P1–P5：匹配器失效。最常见是有人放松了 normalize()（把汉字也折了），');
    console.log('             于是所有片段都能"命中"——这种失效不报错，只让报告变成一片绿。');
    console.log('      P6/P8：语料混进了非文案（如 import 路径），字典条数会虚高，数字不再可信。');
    console.log('      P7/P9/P9b：括号形状或逐行等式判据失效，报告的分母可能被静默截断。');
    console.log('      P10  ：R3a 形态判据失效，导航路径桶会误收或漏收 → 描述。');
  }
  if (skippedProbes.length > 0) {
    console.log('');
    console.log(
      `  ⚠️  自检**不完整**：${skippedProbes.length} 条探针取不到样本（某个文案被删或改名了？）。`,
    );
    console.log('      "没跑"不等于"通过"——探针样本取自字典，样本没了说明字典动过，先补样本再信这份报告。');
  }
  if (dict.droppedEmpty > 0) {
    console.log('');
    console.log(
      `  ⚠️  字典里有 ${dict.droppedEmpty} 条文案归一化后为空（只有标点/省略号，如 \`sep: '…'\`），已从语料中剔除。`,
    );
    console.log('      剔掉是为了语料干净（"628 条"里不该混着不是话的条目），**不是**在防误命中：');
    console.log('      判据是"字典条目.includes(片段)"，空条目让 `\'\'.includes("abc")` 为 false，并不会误命中。');
  }

  console.log('');
  console.log(line);
  console.log('退出码恒为 0：本脚本只报警，不判负。是否升级成总门的一门，等基线稳定后由主理人定。');
  // `--strict` 只对**缺陷类**判负：查不到 = 走查单抄了字典里不存在的话；落单括号 = 报告本身
  // 不可信（那一行后半段根本没扫，"0 条查不到"不是事实）。
  // 覆盖率类（无法核对 / 规则跳过 / 跳过未写理由）**一律不判负**：给它们设阈值，
  // 第一个反应一定是把难判的引文改短、改糊、拆碎，而不是改清楚。告警照报，门不开。
  const fatal = misses.length + (unbalanced.length > 0 ? 1 : 0);
  if (process.argv.includes('--strict') && fatal > 0) {
    console.log(
      `[--strict] 查不到 ${misses.length} 条 + 落单括号 ${unbalanced.length} 行 → 退出码 1（默认不带 --strict 时恒为 0）。`,
    );
    process.exit(1);
  }
  console.log('');
}

main();
