// D-175：备份卡片那两格危险提示的**正向**绊线。
//
// 既有的 `i18n.test.ts` 是黑名单：那两句文案里**不许**出现"占用空间""disk space"这类词。
// 黑名单只在我们想得到词的时候红，它挡不住更常见的一种垮法——**整块没渲染出来**：
// 组件里少一个 `{#if}`、或者把那两格并进外层 `old_files > 0` 里面，界面一声不吭，
// 而所有文案测试全绿。
//
// 所以这组测试问的是反方向的问题：**给定这些输入，界面必须说一句话。**
//
// | `unreadable_batches` | 必须说什么 | 不许说什么 |
// |---|---|---|
// | `null`（数不清） | `uncounted` 那格 | **任何数字** |
// | `> 0`（数得清） | 带数字那格 | —— |
// | `0`（确认没有） | 不出现 | 任何提示 |
//
// `null` 与 `0` 必须分开：前者是"不知道"，后者是"确认没有"。
// 把 `null` 当 `0` 处理，界面就会在"盘上有、我删不掉、我还不知道有几个"时保持沉默。

import { describe, expect, it } from 'vitest';
import { RESTORE_POINT_KIND_KEY, backupHints, restorePointKindKey } from './backup-hints';
import { LOCALES, hasKey } from './i18n/index';

/** 只取函数关心的那个字段，其余留空（它不读别的，也不许读别的）。 */
function status(unreadable_batches: number | null): { unreadable_batches: number | null } {
  return { unreadable_batches };
}

describe('D-175 正向绊线：该说话的时候必须说', () => {
  it('数不清（null）→ 有且只有一条 uncounted 提示', () => {
    const hints = backupHints(status(null));
    expect(hints).toHaveLength(1);
    expect(hints[0].id).toBe('old_unreadable_uncounted');
  });

  it('数得清（1 / 2 / 99）→ 有且只有一条带数字的提示', () => {
    for (const n of [1, 2, 99]) {
      const hints = backupHints(status(n));
      expect(hints, `unreadable_batches=${n}`).toHaveLength(1);
      expect(hints[0].id).toBe('old_unreadable');
      expect(hints[0].count).toBe(n);
    }
  });

  it('恒等式：非 0 必有一句，为 0 必无一句（没有第三种可能）', () => {
    for (const n of [null, -1, 0, 1, 7, 1000]) {
      const expected = n === 0 ? 0 : 1;
      expect(backupHints(status(n)).length, `n=${n}`).toBe(expected);
    }
  });
});

describe('D-175 反向断言：不该说的时候一句都不许说', () => {
  it('确认没有（0）→ 空数组，不是"一条空提示"', () => {
    // 返回 `[{ id: ... }]` 而让组件自己去判空，等于把"不说"又变回布局层的约定。
    expect(backupHints(status(0))).toEqual([]);
  });

  it('数不清那格**不带数字**（count 为 null，组件据此不渲染占位符）', () => {
    const [hint] = backupHints(status(null));
    expect(hint.count).toBeNull();
  });
});

describe('D-175 组件接线：判定必须来自函数，不许写回布局', () => {
  const raw = import.meta.glob(['../components/Settings.svelte'], {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>;
  const source = (
    Object.entries(raw).find(([p]) => p.endsWith('/Settings.svelte'))?.[1] ?? ''
  ).replace(/\r\n/g, '\n');

  it('先钉住守卫本身没空跑（读不到源码时后面的 not.toContain 会假绿）', () => {
    expect(source.length).toBeGreaterThan(1000);
    expect(source).toContain('backupHints(backupStatus)');
  });

  it('两格 `{#if}` 不许写回模板（写回去就等于把逻辑藏进布局）', () => {
    expect(source).not.toContain('{#if backupStatus.unreadable_batches');
    expect(source).not.toContain('{:else if backupStatus.unreadable_batches');
  });

  it('两个键都还在模板里（抽了函数却只渲染一句 = 另一句被静默丢掉）', () => {
    expect(source).toContain("ui.t('settings.restore_point_old_unreadable_uncounted')");
    expect(source).toContain("ui.t('settings.restore_point_old_unreadable',");
  });
});

// D-178：「用这份退回去」从"最近一份 pre-restore"改成"盘上有几份就列几份"。
//
// 上一版的错是**条件几乎不可能成立**（`pre-restore` 只有做过一次恢复之后才存在），
// 而这一类错没有任何文案测试看得出来：键都在、字典都对、组件也确实渲染了——
// 只是那一行永远不进入分支。所以这里钉住的是"清单存在时必须列出来"这个方向。
describe('D-178 退回去的清单：每种备份类型都得有一句人话', () => {
  it('七种类型全覆盖，且互不重复（复制粘贴换掉一个值，两条类型就共用一个键）', () => {
    const keys = Object.values(RESTORE_POINT_KIND_KEY);
    expect(keys).toHaveLength(7);
    expect(new Set(keys).size).toBe(7);
  });

  it('每个键在四种语言里都是真存在的（少一本字典，界面就把 key 原样显示给用户）', () => {
    const keys = [...Object.values(RESTORE_POINT_KIND_KEY), 'settings.restore_point_kind_other'];
    for (const locale of LOCALES) {
      for (const key of keys) {
        expect(hasKey(locale, key), `${locale}:${key}`).toBe(true);
      }
    }
  });

  it('认不出的类型给一句通用话，不把 key 裸奔出去', () => {
    // 版本不一致（前端旧、后端新）真会出现一个前端没听说过的 kind。
    // 那一刻的正确表现是"还能退回去，只是说不清是哪一步"。
    expect(restorePointKindKey('未来新增的类型')).toBe('settings.restore_point_kind_other');
    expect(restorePointKindKey('')).toBe('settings.restore_point_kind_other');
    // 正向：认得出的必须走自己的键，不能一律落到兜底（那样上面那条也会假绿）。
    expect(restorePointKindKey('password_before')).toBe(
      'settings.restore_point_kind_password_before',
    );
  });
});

describe('D-178 组件接线：清单来自后端，不许写死只认一种', () => {
  const raw = import.meta.glob(['../components/Settings.svelte'], {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>;
  const source = (
    Object.entries(raw).find(([p]) => p.endsWith('/Settings.svelte'))?.[1] ?? ''
  ).replace(/\r\n/g, '\n');

  it('先钉住守卫本身没空跑', () => {
    expect(source.length).toBeGreaterThan(1000);
    expect(source).toContain('backupStatus.points');
  });

  it('旧判据 `latest_point_ms` 一句都不许留（留着就是两个入口两套判据）', () => {
    expect(source).not.toContain('latest_point_ms');
    expect(source).not.toContain('restoreLatestPoint');
  });

  it('"要旧密码"这句在组头说明里说，不再逐行重复（表格化之后行里只有时间/哪一步/按钮）', () => {
    // 1.5.31 是逐行写「要用改密码之前的那个密码」；1.5.32 按主人的要求改成
    // 「改密前的归一起、改密后的归一起，附一句简短说明」——那句说明就是 `old_note`。
    // 判据仍然是 `needs_old_password`（后端判的），只是从"行级"提到"组级"。
    // 组级也不能省：两个表看起来只差一个标题，而"要用旧密码"是它们唯一实质区别。
    expect(source).toContain("'settings.restore_point_old_note'");
    expect(source).toContain('splitPoints');
  });

  it('确认框的文案与密码说明都按 `needs_old_password` 分叉', () => {
    // 只改一半是最坏的情形：文案说"这是旧密码的备份"，密码框却写着
    // "输入主密码确认（只是确认是你本人）"——那一刻用户会去试现在的密码。
    expect(source).toContain("'settings.restore_point_restore_body_old_pwd'");
    expect(source).toContain("'settings.restore_point_restore_pwd_old'");
    expect(source).toContain("'settings.restore_point_restored_old_pwd'");
  });
});

describe('D-180 钉住与自动清理：开关必须真的接到后端', () => {
  const raw = import.meta.glob(['../components/Settings.svelte'], {
    query: '?raw',
    import: 'default',
    eager: true,
  }) as Record<string, string>;
  const source = (
    Object.entries(raw).find(([p]) => p.endsWith('/Settings.svelte'))?.[1] ?? ''
  ).replace(/\r\n/g, '\n');

  it('守卫本身没空跑', () => {
    expect(source.length).toBeGreaterThan(1000);
  });

  it('钉住按钮存在且状态来自后端字段 `pinned`（不是前端自己记的）', () => {
    expect(source).toContain('setPinned');
    expect(source).toContain('point.pinned');
    // 钉住必须是**行级**的：整组钉住会把"我只想保住这一份"变成"全保住"。
    expect(source).not.toContain('setPinnedGroup');
  });

  it('自动清理只有一个开关，天数由后端给（前端不许写死档位、不许自带第二套上限）', () => {
    expect(source).toContain('setRetention');
    expect(source).toContain('retention_on');
    expect(source).toContain('retention_days');
    // 档位下拉（0/7/30/90/180）是主人 2026-11-03 明确否掉的做法，不许回流。
    expect(source).not.toContain('RETENTION_OPTIONS');
    // 份数由 `prune()` 按类别管（日备份 5 份、事件每类 1 份）。前端再拿一个"总数"
    // 去说事会跟分类轮转打架——2026-11-03 试过一版，撤销了。
    expect(source).not.toContain('retention_max');
  });
});
