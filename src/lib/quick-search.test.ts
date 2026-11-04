// 快速搜索小窗的纯逻辑（v1.4.11）：窗口判定 / 排序 / 键盘导航 / 回执文案。
//
// 这几条都是"错了不会报错、只会让用户拿到错的东西"的类型：
// 排序错了会先复制到别人的密码，窗口判定错了会把主界面渲染进 440px 的小窗。
import { describe, expect, it } from 'vitest';

import { RESULT_LIMIT, copiedLabel, isQuickWindow, moveIndex, rankCandidates, rankItems } from './quick-search';
import type { RankableCandidate } from './quick-search';
import type { ItemDto, ItemKind } from '../types';

/** 造一条条目：只写本组测试关心的字段，其余给稳定的默认值。 */
function item(id: string, kind: ItemKind, extra: Partial<ItemDto> = {}): ItemDto {
  return {
    item_id: id,
    kind,
    attachments: [],
    favorite: false,
    created_at: 0,
    updated_at: 0,
    rev: 1,
    modified_by: 'test',
    pinned: false,
    pin_order: 0,
    use_count: 0,
    last_used_at: null,
    ...extra,
  };
}

function login(title: string, username: string | null, url: string | null = null): ItemKind {
  return {
    type: 'login',
    title,
    username,
    url,
    password: 'p@ss',
    notes: null,
    totp_secret: null,
    custom_fields: [],
  };
}

describe('isQuickWindow：URL 判定', () => {
  it('带 w=quick 才是小窗', () => {
    expect(isQuickWindow('?w=quick')).toBe(true);
    expect(isQuickWindow('?x=1&w=quick')).toBe(true);
  });

  it('空 query 与近似写法都不算（防止把主界面渲染进小窗）', () => {
    expect(isQuickWindow('')).toBe(false);
    expect(isQuickWindow('?w=quick2')).toBe(false);
    expect(isQuickWindow('?w=Quick')).toBe(false);
    expect(isQuickWindow('?quick=1')).toBe(false);
  });
});

describe('rankItems：只收登录条目', () => {
  it('笔记与附件被排除', () => {
    const items = [
      item('a', login('GitHub', 'octocat')),
      item('b', { type: 'secure_note', title: 'GitHub 恢复码', body: 'xxx' }),
      item('c', {
        type: 'attachment',
        title: 'GitHub 备份.zip',
        original_filename: 'x.zip',
        mime_type: 'application/zip',
        size_bytes: 1,
        blob_ref: 'r',
        blob_size_bytes: 1,
      }),
    ];
    const hits = rankItems(items, 'github');
    expect(hits.map((h) => h.itemId)).toEqual(['a']);
  });
});

describe('rankItems：排序', () => {
  // D-196：前端**不许**自己排空查询。后端 `list_items` 的顺序就是答案
  // （钉住块 pin_order → 未钉按频度），再补一层 use_count 会把钉住整块抹掉。
  it('空查询原样保留传入顺序（钉住块由后端排，前端重排会把它挤下去）', () => {
    // 刻意让 use_count 与数组顺序**相反**：钉住的条目通常用得少。
    // 旧实现按 use_count 降序 → 钉住的 pinnedLow 被甩到最后一条，正是真机症状。
    const items = [
      item('pinnedLow', login('Alpha', 'u1'), { pinned: true, pin_order: 1, use_count: 1 }),
      item('often', login('Beta', 'u2'), { use_count: 9 }),
      item('rare', login('Gamma', 'u3'), { use_count: 5 }),
    ];
    expect(rankItems(items, '').map((h) => h.itemId)).toEqual(['pinnedLow', 'often', 'rare']);
    // 纯空白与空串等价：用户手滑敲了空格不该得到"没有结果"。
    expect(rankItems(items, '   ').map((h) => h.itemId)).toEqual(['pinnedLow', 'often', 'rare']);
  });

  it('标题前缀 > 标题包含 > 用户名前缀 > 用户名包含 > 网址包含', () => {
    const items = [
      item('url', login('Something else', 'zzz', 'https://github.com/login')),
      item('userpart', login('Something else', 'my-github-name')),
      item('userpre', login('Something else', 'github-user')),
      item('titlepart', login('My GitHub account', 'zzz')),
      item('titlepre', login('GitHub', 'zzz')),
    ];
    expect(rankItems(items, 'github').map((h) => h.itemId)).toEqual([
      'titlepre',
      'titlepart',
      'userpre',
      'userpart',
      'url',
    ]);
  });

  it('大小写与首尾空格不影响结果', () => {
    const items = [item('a', login('GitHub', 'OctoCat'))];
    expect(rankItems(items, '  GITHUB  ').length).toBe(1);
    expect(rankItems(items, 'octocat').length).toBe(1);
  });

  it('不匹配的条目被过滤掉', () => {
    const items = [item('a', login('GitHub', 'octocat'))];
    expect(rankItems(items, 'gitlab')).toEqual([]);
  });

  it('同分保持传入顺序（稳定排序，不按 use_count 二次重排）', () => {
    // 钉住的条目 use_count 更低，仍须排在前面 —— 靠的是保序，不是比较函数。
    const items = [
      item('pinned', login('Mail', 'u'), { pinned: true, pin_order: 1, use_count: 2 }),
      item('often', login('Mail', 'u'), { use_count: 7 }),
    ];
    expect(rankItems(items, 'mail').map((h) => h.itemId)).toEqual(['pinned', 'often']);
  });

  it('结果条数受 RESULT_LIMIT 约束', () => {
    const items = Array.from({ length: RESULT_LIMIT + 5 }, (_, i) =>
      item(`i${i}`, login(`Item ${i}`, 'u')),
    );
    expect(rankItems(items, '').length).toBe(RESULT_LIMIT);
  });
});

describe('moveIndex：键盘导航', () => {
  it('上下移动并在两端回卷', () => {
    expect(moveIndex(0, 1, 3)).toBe(1);
    expect(moveIndex(2, 1, 3)).toBe(0);
    expect(moveIndex(0, -1, 3)).toBe(2);
    expect(moveIndex(1, -1, 3)).toBe(0);
  });

  it('空结果返回 -1（无选中项）', () => {
    expect(moveIndex(0, 1, 0)).toBe(-1);
    expect(moveIndex(5, -1, 0)).toBe(-1);
  });

  it('单条结果时原地不动', () => {
    expect(moveIndex(0, 1, 1)).toBe(0);
    expect(moveIndex(0, -1, 1)).toBe(0);
  });
});

describe('copiedLabel：回执文案', () => {
  it('复制密码 / 用户名各自有专用文案', () => {
    expect(copiedLabel('password', true)).toBe('quick.copied_password');
    expect(copiedLabel('username', true)).toBe('quick.copied_username');
  });

  it('条目没有用户名时如实说"没有用户名"，不能假装复制成功', () => {
    expect(copiedLabel('username', false)).toBe('quick.no_username');
  });
});

// 1.5.39：自动输入浮层的候选行入口（`rankCandidates`）。
//
// 它与 `rankItems` 共用私有 `rankHits`（同一套打分、两窗位置一致），但收窄到
// 5 字段扁平行、**不判别不过滤**——登录项过滤已在 Rust `autotype_candidates` 做完。
// 这个入口是浮层唯一的排序入口，不能零覆盖：少 url → 站点名搜永不命中；
// 空查询不保序 → 钉住与手动顺序整块丢失（D-196，两种退化都不会有测试变红）。
describe('rankCandidates：自动输入浮层候选行', () => {
  /** 造一条候选：5 字段，与 Rust `AutotypeCandidateDto` 同名。 */
  function candidate(
    id: string,
    title: string,
    url: string | null = null,
    useCount = 0,
  ): RankableCandidate {
    return { item_id: id, title, username: null, url, use_count: useCount };
  }

  it('只有网址命中关键词的候选也会入选（url 档是站点名搜索的唯一途径）', () => {
    const rows = [
      candidate('site', '工作账号', 'https://github.com'),
      candidate('other', '个人邮箱', 'https://mail.example.com'),
    ];
    expect(rankCandidates(rows, 'github').map((h) => h.itemId)).toEqual(['site']);
  });

  it('空查询原样保留候选顺序（钉住在最前由后端保证，前端不许重排）', () => {
    // 主人 2026-11-04 真机：钉住的没排前面。旧实现按 use_count 降序，
    // 这里刻意让钉住的那条 use_count 最低 —— 它必须仍在第一位。
    const rows = [candidate('pinned', 'Z 站', null, 1), candidate('often', 'A 站', null, 9)];
    expect(rankCandidates(rows, '').map((h) => h.itemId)).toEqual(['pinned', 'often']);
  });

  it('有查询时仍按得分排序（保序不等于不排）', () => {
    // 防"干脆不排序"的退化：输入顺序与得分顺序相反，输出必须按得分。
    const rows = [candidate('urlOnly', '别的东西', 'https://github.com'), candidate('titlePre', 'GitHub')];
    expect(rankCandidates(rows, 'github').map((h) => h.itemId)).toEqual(['titlePre', 'urlOnly']);
  });
});
