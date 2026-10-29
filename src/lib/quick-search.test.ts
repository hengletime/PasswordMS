// 快速搜索小窗的纯逻辑（v1.4.11）：窗口判定 / 排序 / 键盘导航 / 回执文案。
//
// 这几条都是"错了不会报错、只会让用户拿到错的东西"的类型：
// 排序错了会先复制到别人的密码，窗口判定错了会把主界面渲染进 440px 的小窗。
import { describe, expect, it } from 'vitest';

import { RESULT_LIMIT, copiedLabel, isQuickWindow, moveIndex, rankItems } from './quick-search';
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
  it('空查询 = 最常用的几条（按 use_count 降序）', () => {
    const items = [
      item('a', login('Alpha', 'u1'), { use_count: 1 }),
      item('b', login('Beta', 'u2'), { use_count: 9 }),
      item('c', login('Gamma', 'u3'), { use_count: 5 }),
    ];
    expect(rankItems(items, '').map((h) => h.itemId)).toEqual(['b', 'c', 'a']);
    // 纯空白与空串等价：用户手滑敲了空格不该得到"没有结果"。
    expect(rankItems(items, '   ').map((h) => h.itemId)).toEqual(['b', 'c', 'a']);
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

  it('同分按 use_count 降序', () => {
    const items = [
      item('a', login('Mail', 'u'), { use_count: 2 }),
      item('b', login('Mail', 'u'), { use_count: 7 }),
    ];
    expect(rankItems(items, 'mail').map((h) => h.itemId)).toEqual(['b', 'a']);
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
