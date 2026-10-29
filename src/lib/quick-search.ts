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
 * 过滤 + 排序。
 *
 * 空查询不是"没有结果"，而是**最常用的那几条**：用户按热键常常只是想说
 * "给我上次用的那个"。排序口径与主列表一致（频度 → 最近使用 → 最近更新），
 * 免得同一个条目在两个地方的位置对不上。
 *
 * @param items 全量条目（含非登录项，本函数内部会筛掉）
 * @param query 原始输入，内部做去空格 + 小写；纯空白视同空查询
 */
export function rankItems(items: ItemDto[], query: string, limit = RESULT_LIMIT): QuickHit[] {
  const q = query.trim().toLowerCase();

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

  const scored = q ? hits.filter((h) => (h.score = scoreOf(h, q)) > 0) : hits;

  scored.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    if (b.useCount !== a.useCount) return b.useCount - a.useCount;
    return a.title.localeCompare(b.title);
  });

  return scored.slice(0, limit);
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
