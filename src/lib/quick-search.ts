// 快速搜索小窗的纯逻辑（v1.4.11）。
//
// 抽出来的理由和本仓其它 `*-*.ts` 一样：**排序与导航是这套界面里唯一会出错又难
// 肉眼验证的部分**。组件里剩下的只有 DOM 接线，那部分靠源码断言守门。
//
// 一条边界写在这里而不是藏进组件：**小窗只搜登录条目**。
// 它的用途是"拿一条密码去填别人的登录框"，笔记/附件根本填不进去；把它们混进结果
// 只会让"回车之后什么都没复制"变成一种新的困惑。空结果时界面要说清是"没有匹配的
// 登录条目"，不能笼统说"没找到"——后者会让用户以为自己的笔记丢了。

import type { ItemDto } from '../types';

/** 结果条目（只保留小窗要显示与要用的字段）。 */
export interface QuickHit {
  itemId: string;
  title: string;
  username: string | null;
  url: string | null;
  useCount: number;
  /** 匹配得分，仅供排序；界面不显示。 */
  score: number;
}

/** 结果条数上限。小窗可视区约 6~7 行，再多也翻不过来。 */
export const RESULT_LIMIT = 40;

/**
 * 当前页面是不是小窗。
 *
 * 入口由 Rust 侧以 `index.html?w=quick` 打开（见 `src-tauri/src/quick_search.rs`），
 * 所以判定只依赖 URL，不依赖窗口 label（前端拿 label 要额外异步调用，
 * 而分流必须发生在 `mount` 之前）。
 */
export function isQuickWindow(search: string): boolean {
  return new URLSearchParams(search).get('w') === 'quick';
}

/**
 * 单条打分。数字只用于相对排序，绝对值没有含义。
 *
 * 优先级：标题前缀 > 标题包含 > 用户名前缀 > 用户名包含 > 网址包含。
 * 标题排最前是因为用户记住的通常是"那条叫什么"，而不是用户名。
 */
function scoreOf(hit: QuickHit, query: string): number {
  const title = hit.title.toLowerCase();
  const username = (hit.username ?? '').toLowerCase();
  const url = (hit.url ?? '').toLowerCase();
  if (title.startsWith(query)) return 100;
  if (title.includes(query)) return 80;
  if (username.startsWith(query)) return 60;
  if (username.includes(query)) return 40;
  if (url.includes(query)) return 20;
  return 0;
}

/**
 * 自动输入候选行（`autotype_candidates` 的 5 字段 wire 形状）。
 *
 * 结构型、不绑定 `ItemDto`。两个窗口共用**同一套打分**（私有 `rankHits`），
 * 产品承诺是「同一个条目在两个地方的位置对得上」——共用的是打分，不是同一个入口：
 * 快速搜索吃完整 `ItemDto`（登录数据在 `kind` 里、要按 `kind.type` 过滤），
 * 浮层吃这份扁平行（登录项过滤已在 Rust 侧完成）。
 *
 * ⚠️ 刻意**不**让 `rankItems` 吃联合类型：一个函数吃两种形状就需要判别守卫，
 * 而守卫（`'kind' in item`）依赖"另一侧恰好没有某个字段"——将来谁给候选行加个
 * `kind`，扁平行会被当下完整条目、整列被滤空，且 TypeScript 一声不响。
 * 两个入口各自收窄，判别代码根本不存在。
 */
export interface RankableCandidate {
  item_id: string;
  title: string;
  username: string | null;
  url: string | null;
  use_count: number;
}

/**
 * 对**已经是 QuickHit 的列表**做过滤 + 排序（内部实现，两个入口共用同一把尺子）。
 *
 * `url` 为 `null` 时网址那两档自然拿不到分，排序退化到
 * 「标题前缀 > 标题包含 > 用户名前缀 > 用户名包含」。
 *
 * ## 为什么空查询**不排序**、同分也不设次键（D-196，主人 2026-11-04 真机）
 *
 * 顺序不是前端能自己决定的东西。后端 `list_items` 已按
 * 「钉住块 `pin_order` ASC → 未钉住 `use_count` DESC → `last_used_at` DESC
 * → `updated_at` DESC → `item_id` ASC」排好，两个入口拿到的数组都带着这个顺序
 * （`login_candidates` 的 `filter_map` 与 DTO 的 `map` 都保序）。
 *
 * 前端再补一层 `use_count` + 标题序就会把整块顺序**覆盖**掉：钉住条目的 `use_count`
 * 通常不高，一按频度重排就沉到未钉住的常用条目下面——1.5.41 真机反馈的
 * 「钉住和手动排序在呼出的弹窗里不管用、钉住的没排前面」就是这么来的
 * （旧排序目标里根本没有「钉住」这一维，`pin_order` 更是从未被读取）。
 *
 * 所以这里只做一件事：**按匹配得分做稳定排序**。得分是前端独有的信息（后端不知道
 * 用户在搜什么），其余维度一律继承后端顺序。`Array.prototype.sort` 自 ES2019 起
 * 保证稳定，同分条目保持原序 = 免费继承「钉住 + 手动顺序 + 频度」三层语义，
 * 且后端哪天改了排序口径，这里自动跟随，不会长出第二套事实。
 *
 * `QuickHit.useCount` 因此**不再参与排序**（保留在结构里只为 wire 稳定，
 * 不许为了"加点兜底"把它重新塞回比较函数）。
 */
function rankHits(hits: QuickHit[], query: string, limit: number): QuickHit[] {
  const q = query.trim().toLowerCase();
  if (!q) {
    // 空查询 = "给我最该看的几条"：后端顺序本身就是答案，切前 limit 条即可。
    // 这个早退还切断了一个隐性依赖：下面那句 `scoreOf(h, '') > 0` 之所以不会
    // 把全部条目滤光，全靠 `''.startsWith('')` 是 true（返回 100 分）。哪天
    // `scoreOf` 改成空串不匹配，空查询就会白屏 —— 早退让这种事只影响不到这里。
    return hits.slice(0, limit);
  }
  const scored = hits.filter((h) => (h.score = scoreOf(h, q)) > 0);
  scored.sort((a, b) => b.score - a.score);
  return scored.slice(0, limit);
}

/**
 * 快速搜索窗口：过滤 + 排序。
 *
 * 空查询不是"没有结果"，而是**最该看的几条**：用户按热键常常只是想说
 * "给我上次用的那个"。顺序**完全继承主列表**（钉住块 `pin_order` → 未钉按频度），
 * 所以同一个条目在两个地方的位置必然对得上——前提是调用方传进来的是
 * 后端 `list_items` 的原序（别先自己 `sort()` 一遍再传）。
 *
 * @param items 全量条目（含非登录项，本函数内部会筛掉）
 * @param query 原始输入，内部做去空格 + 小写；纯空白视同空查询
 */
export function rankItems(items: readonly ItemDto[], query: string, limit = RESULT_LIMIT): QuickHit[] {
  const hits: QuickHit[] = [];
  for (const item of items) {
    // 只收登录项：笔记与附件没有可复制的密码，混进结果只会让回车"什么也没发生"。
    if (item.kind.type !== 'login') {
      continue;
    }
    hits.push({
      itemId: item.item_id,
      title: item.kind.title,
      username: item.kind.username ?? null,
      url: item.kind.url ?? null,
      useCount: item.use_count ?? 0,
      score: 0,
    });
  }
  return rankHits(hits, query, limit);
}

/**
 * 自动输入浮层：候选行打分排序。
 *
 * **不判别、不过滤**：候选只含登录项（Rust `autotype_candidates` 已筛过），
 * 这里再把关只会多一处需要测试的分支。打分与 `rankItems` 共用 `rankHits`——
 * 同一个条目在两个窗口的位置因此必然一致。
 *
 * 空查询保序这一条对浮层尤其要紧：`autotype_candidates` 走的就是
 * `repo.list_items()`，候选顺序 = 主列表顺序 = 钉住块在前（D-196）。
 */
export function rankCandidates(
  rows: readonly RankableCandidate[],
  query: string,
  limit = RESULT_LIMIT,
): QuickHit[] {
  const hits: QuickHit[] = rows.map((row) => ({
    itemId: row.item_id,
    title: row.title,
    username: row.username ?? null,
    url: row.url ?? null,
    useCount: row.use_count ?? 0,
    score: 0,
  }));
  return rankHits(hits, query, limit);
}

/**
 * 键盘导航：上下移动选中项，**到头回卷**。
 *
 * 回卷而不是夹紧：结果通常只有几条，用户按三下下键想回到第一条时，
 * 夹在最后一条不动会让人以为键盘失灵。
 *
 * @returns 新下标；`length === 0` 时返回 -1（无选中）
 */
export function moveIndex(index: number, delta: number, length: number): number {
  if (length <= 0) {
    return -1;
  }
  return (((index + delta) % length) + length) % length;
}

/**
 * 复制之后该说什么。
 *
 * 单独抽出来是因为"复制了但没说清复制的是哪一项"会让用户去粘贴一个错的东西：
 * 复制用户名与复制密码在小窗里用的是同一个键区，文案必须点明。
 */
export function copiedLabel(kind: 'password' | 'username', hasUsername: boolean): string {
  if (kind === 'username' && !hasUsername) {
    return 'quick.no_username';
  }
  return kind === 'password' ? 'quick.copied_password' : 'quick.copied_username';
}
