// D-73 同轮（v2.13）：设置页「控件行 / 信息行 / 按钮行」三分 —— 结构守门。
//
// 起因（主人 1.2.6 真机 390px 截图）：「关于」卡片变成
//   PasswordMS / 版本: — / 作者 ⏎ hengletime / 许可证 ⏎ MIT License / 三个按钮各占一行满宽
// 根因不是「规则写错了」，而是「规则的**适用范围**没写」——窄屏这条
//   `.set-row { flex-direction: column; align-items: stretch }`
// 本是**为控件行写的**（`.set-ctl` 那 160px 固定宽在 390px 会溢出），
// 但它只判了 `.set-row`，于是把纯信息行（左右拆成两行，读成两条独立信息）
// 与按钮行（`align-items: stretch` 把三个按钮拉成满宽竖排）一起误伤。
//
// 本项目没法在本机跑真浏览器（没有 jsdom / playwright），
// 所以这里守的是**可判定的结构事实**：
//   1. 哪些行标了 `is-info` / `is-actions` / `is-check`（标注错了，窄屏一定错）；
//   2. 覆盖规则**在同一个 `@media (max-width: 767px)` 块内**、且**位于 `.set-row` 之后**
//      （不许新开第二个 `@media`，否则层叠关系会被搅乱）；
//   3. 桌面（≥768px）**没有任何** `.set-row.is-info` / `.set-row.is-actions` /
//      `.set-row.is-check` 规则 ⇒ 桌面逐像素不变（媒体查询外的样式与改造前完全一致）。
//
// 层叠为什么成立：`.set-row.is-info` 特异度 (0,2,0) > `.set-row` (0,1,0)，
// 即便将来有人调换两条规则的顺序也仍然生效；源码顺序是第二道保险。
// D-110（v2.34）新增第三种行类型 is-check：复选框行窄屏也必须保持横排。

import { describe, expect, it } from 'vitest';

/** 读取 `Settings.svelte` 原文。键相对本文件（`src/lib/`），故跨目录写 `../components/...`。 */
const sources = import.meta.glob('../components/Settings.svelte', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

const SRC: string = sources['../components/Settings.svelte'] ?? '';

/**
 * 取出**第一个** `@media (max-width: 767px) { ... }` 块的内部文本（花括号配平）。
 *
 * @param source 组件全文
 * @returns 块内文本；找不到时返回 `null`
 */
function extractMobileMediaBlock(source: string): string | null {
  const head = '@media (max-width: 767px)';
  const at = source.indexOf(head);
  if (at < 0) {
    return null;
  }
  let depth = 0;
  let started = false;
  for (let i = at + head.length; i < source.length; i += 1) {
    const ch = source[i];
    if (ch === '{') {
      depth += 1;
      started = true;
    } else if (ch === '}') {
      depth -= 1;
      if (started && depth === 0) {
        return source.slice(at + head.length, i);
      }
    }
  }
  return null;
}

/** 组件里所有 `@media (max-width: 767px)` 的出现次数（多于 1 个就说明有人另开了一块）。 */
function countMobileMediaBlocks(source: string): number {
  return source.split('@media (max-width: 767px)').length - 1;
}

/**
 * 去掉注释后再做「不许出现某选择器」这类负向断言。
 *
 * 理由：注释里 Explain 规则时会**写出被禁止的写法本身**（本文件的注释就写着
 * 「不许用 `:has()`」），直接扫全文会命中注释 → 恒红；而把负向断言改成扫
 * 「只扫代码」才既守住真实用法、又不误报。
 *
 * @param source 组件全文
 * @returns 挖掉 `/* … *\/` 与 `<!-- … -->` 之后的文本
 */
function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/<!--[\s\S]*?-->/g, '');
}

describe('Settings.svelte —— 行类型三分（D-73 同轮 / v2.13）', () => {
  it('扫描面有效：能读到 Settings.svelte 且能切出移动端媒体块（防 glob 写错退化成空断言假绿）', () => {
    expect(SRC.length).toBeGreaterThan(1000);
    expect(SRC).toContain('settings.group_about');

    const block = extractMobileMediaBlock(SRC);
    expect(block).not.toBeNull();
    expect((block ?? '').length).toBeGreaterThan(80);
    // 切出来的确实是那块：里面必须有原始的一刀切规则，否则说明切错位置了。
    expect(block ?? '').toContain('flex-direction: column');
  });

  it('纯信息行共 6 条 is-info（关于组 3 + 压缩体积 2 + 快速解锁状态 1）、按钮行 2 条 is-actions', () => {
    // 标记在 markup 里是空格分隔的 class 串（`class="set-row is-info"`），与 CSS 的 `.set-row.is-info` 不同形。
    //
    // 计数从 3 改成 5 是 **D-76 的有意变更**（不是为了让测试变绿而放宽）：
    // 「保险库文件」卡片里新增的两行体积构成同样是**纯信息行**，窄屏必须避开
    // `.set-row { flex-direction: column }` 的堆叠规则 —— 否则「已删附件占 24.0 MB」
    // 会被拆成两行（一行「已删附件占」一行「24.0 MB」），读起来像两条无关信息（D-73 症状⑤ 同型）。
    // 所以新行**必须**带 is-info；把这个数字改大，正是让守卫继续「锁死实际条数」而不是失效。
    //
    // 5 → 6 是 **D-113 P2 的有意变更**：快速解锁卡片的「Windows Hello 快速解锁 / 已开启（日期）」
    // 是纯信息行（右侧状态文字要能自然换行），窄屏不能拆成两行。
    const infoRows = SRC.match(/class="set-row is-info"/g) ?? [];
    const actionRows = SRC.match(/class="set-row is-actions"/g) ?? [];
    expect(infoRows).toHaveLength(6);
    // 计数 1 → 2 是 **D-107 的有意变更**：「关于」卡片新增更新检查区块，
    // 「前往下载」按钮行同样是按钮行，窄屏必须横向排布（避开窄屏堆叠）。
    expect(actionRows).toHaveLength(2);
  });

  it('复选框行共 2 条 is-check（D-110：更新检查；v1.4.10：开机自启）', () => {
    // 计数 0 → 3 是 **D-110 的有意变更**（主人手机实测）：窄屏堆叠把复选框
    // 堆到文字下一行——复选框行是第三种行类型，必须像 is-info / is-actions
    // 一样登记标记、在窄屏媒体块里覆盖回横排。
    // v1.4.10 系统集成：+1（开机自启），3 → 4。
    // **v1.4.16 回退自动同步（D-133）**：4 → 2，「自动同步 / 启动时同步」两个开关
    // 随功能删除。这个数字变小是**有意的**：守卫锁的是实际条数，功能回退就该跟着改，
    // 否则它会变成「一条永远修不掉的假红」。
    const checkRows = SRC.match(/class="set-row is-check"/g) ?? [];
    expect(checkRows).toHaveLength(2);
  });

  it('别的行一个都不许标 is-*：控件行必须继续走窄屏堆叠', () => {
    // 全文件 `class="set-row"`（裸）的行数 = 总 set-row 行数 - 带标记的行数。
    const plainRows = SRC.match(/class="set-row"/g) ?? [];
    const totalRows = SRC.match(/class="set-row[ "]/g) ?? [];
    // 主题/语言/自动锁定/剪贴板/两个体检阈值/导出密码/三个改密码 = 10 条裸行（D-110 起
    // 两个同步开关移入 is-check，不再是裸行）。D-113 P2 的快速解锁开启密码输入框也是裸行
    // （控件行，窄屏就该堆叠），裸行 10 → 11。
    // **D-133（v1.4.16）**：两个同步开关随自动同步一起删除，标记行 12 → 10
    // （6 is-info + 2 is-actions + 2 is-check）。
    // 这里不写死裸行具体条数（将来加控件行会合法变动），只断言「裸行是多数、且标记行
    // 共 10 条」。
    expect(plainRows.length).toBeGreaterThanOrEqual(8);
    expect(plainRows.length + 10).toBe(totalRows.length);
    // 白色名单外不许出现别的 `is-*` 变体（例如 `is-buttons` 这种手滑写法）。
    const anyVariant = SRC.match(/class="set-row is-[a-z-]+"/g) ?? [];
    expect(anyVariant).toHaveLength(10);
  });

  it('覆盖规则在同一个移动端媒体块内，且都在一刀切 `.set-row` 之后（层叠顺序）', () => {
    expect(countMobileMediaBlocks(SRC)).toBe(1);

    const block = extractMobileMediaBlock(SRC) ?? '';
    const plainRuleAt = block.indexOf('.set-row {');
    const infoAt = block.indexOf('.set-row.is-info {');
    const infoMetaAt = block.indexOf('.set-row.is-info .pms-meta {');
    const actionsAt = block.indexOf('.set-row.is-actions {');
    const checkAt = block.indexOf('.set-row.is-check {');

    expect(plainRuleAt).toBeGreaterThanOrEqual(0);
    expect(infoAt).toBeGreaterThan(plainRuleAt);
    expect(infoMetaAt).toBeGreaterThan(infoAt);
    expect(actionsAt).toBeGreaterThan(plainRuleAt);
    // D-110：is-check 覆盖同样必须在同一块内、位于一刀切规则之后。
    expect(checkAt).toBeGreaterThan(plainRuleAt);
  });

  it('信息行：窄屏改回 row + 两端对齐 + 值右对齐可收缩（不是继续堆叠）', () => {
    const block = extractMobileMediaBlock(SRC) ?? '';
    const infoRule = block.slice(block.indexOf('.set-row.is-info {'), block.indexOf('.set-row.is-info .pms-meta {'));
    expect(infoRule).toContain('flex-direction: row');
    expect(infoRule).toContain('justify-content: space-between');
    expect(infoRule).toContain('baseline');

    const metaRule = block.slice(block.indexOf('.set-row.is-info .pms-meta {'), block.indexOf('.set-row.is-actions {'));
    expect(metaRule).toContain('text-align: right');
    // 长文本必须能收缩，否则 390px 下会顶出卡片。
    expect(metaRule).toContain('min-width: 0');
  });

  it('按钮行：窄屏横排 + 允许换行（不整宽、不竖排）', () => {
    const block = extractMobileMediaBlock(SRC) ?? '';
    const actionsRule = block.slice(block.indexOf('.set-row.is-actions {'));
    expect(actionsRule).toContain('flex-direction: row');
    expect(actionsRule).toContain('flex-wrap: wrap');
    expect(actionsRule).toContain('align-items: center');
    // `stretch` 会把按钮拉成满宽 —— 按钮行里绝不允许出现。
    expect(actionsRule).not.toContain('stretch');
  });

  it('复选框行（D-110）：窄屏改回 row + 两端对齐（文字左 / 复选框右，同桌面）', () => {
    const block = extractMobileMediaBlock(SRC) ?? '';
    const checkRule = block.slice(block.indexOf('.set-row.is-check {'));
    expect(checkRule).toContain('flex-direction: row');
    expect(checkRule).toContain('justify-content: space-between');
    expect(checkRule).toContain('align-items: center');
    // 复选框不许被 stretch 拉宽，也不许再被堆叠。
    expect(checkRule).not.toContain('stretch');
    expect(checkRule).not.toContain('column');
  });

  it('桌面（≥768px）一个像素都不许变：媒体块外没有任何 is-info / is-actions / is-check 规则', () => {
    const block = extractMobileMediaBlock(SRC) ?? '';
    // 先剥注释再判：markup 里的说明会写出选择器本身（例如「两行都走 `.set-row.is-info`」），
    // 直接扫全文会把**注释**当成规则（同一类假红，见下面 `:has(` 那条）。
    const outside = stripComments(SRC.replace(block, ''));
    expect(outside).not.toContain('.set-row.is-info');
    expect(outside).not.toContain('.set-row.is-actions');
    expect(outside).not.toContain('.set-row.is-check');
    // 桌面仍然是原始的 `space-between` 横排（这条一直没被改过）。
    expect(outside).toContain('justify-content: space-between');
  });

  it('不许用 :has() 区分行类型（WebView 版本差异，别赌）', () => {
    // 只扫代码、不扫注释 —— 否则本文件里的「不许用 `:has()`」这句注释会把自己判红。
    expect(stripComments(SRC)).not.toContain(':has(');
  });
});
