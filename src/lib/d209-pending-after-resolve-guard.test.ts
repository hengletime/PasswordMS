// D-209 守门：冲突裁决成功后必须刷新「待同步」读数。
//
// 事故（主人真机 1.6.7 走查 3.81/3.84）：
//   在 A 端对「本端已删除 ↔ 远端改名」选「保留本机」（= 确认删除），卡片当场消失，
//   但 B 端的分组还在——**必须再同步一次** B 才会跟着删。
//
// 前半句是同步模型的必然：裁决只是本端库里的一次写，协议里没有「裁决后主动推送」，
// 对端只能等下一轮同步。**不是缺陷。**
// 后半句才是缺陷：`resolve()` 只刷了业务视图与冲突箱（D-164 补的），没刷同步状态，
// 于是「待同步」还停在 0 —— 用户看到卡片消失就以为完事了，只能靠「对面怎么还没删」
// 反推还有一次同步欠着（D-186 的老账：不自动消失的状态必须让人看得见）。
//
// 判据刻意取「位置」而不是「存在」：`loadStatus` 在别处（`syncNow` / 解锁加载）早就有了，
// 只查 `toContain('loadStatus')` 会被那两处喂饱，等于永远绿。必须落在 `resolve` 切片里、
// 且在 `loadConflicts()` 之后。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(['./stores/sync.svelte.ts'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

function storeSource(): string {
  const text = Object.values(raw)[0] ?? '';
  return text.replace(/\r\n/g, '\n');
}

function resolveBody(): string {
  const text = storeSource();
  const start = text.indexOf('async resolve(');
  if (start < 0) {
    return '';
  }
  const end = text.indexOf('\n  }', start);
  return end < 0 ? '' : text.slice(start, end);
}

describe('D-209：裁决后「待同步」必须跟着动', () => {
  const body = resolveBody();

  it('① resolve 切片取得成功', () => {
    expect(body, 'store 里找不到 async resolve( —— 切片锚点失效，后面的断言会全成假绿').not.toBe(
      '',
    );
    expect(body).toContain('api.sync.resolveConflict(');
  });

  it('② resolve 内部必须刷新同步状态（不是别处的 loadStatus）', () => {
    expect(
      body.includes('this.loadStatus('),
      '裁决后没刷「待同步」：卡片消失但数字还是 0，用户看不出还欠一次同步（D-209）',
    ).toBe(true);
  });

  it('③ 刷新必须晚于裁决提交、且在冲突箱刷新之后', () => {
    const resolvedAt = body.indexOf('await api.sync.resolveConflict(');
    const conflictsAt = body.indexOf('await this.loadConflicts();');
    const statusAt = body.indexOf('this.loadStatus(');
    expect(resolvedAt).toBeGreaterThan(-1);
    expect(conflictsAt).toBeGreaterThan(-1);
    expect(statusAt, 'loadStatus 必须晚于仲裁提交').toBeGreaterThan(resolvedAt);
    expect(statusAt, '状态刷新应排在冲突箱刷新之后（先让卡片消失，再更新数字）').toBeGreaterThan(
      conflictsAt,
    );
  });

  it('④ 状态刷新失败不得把成功的裁决变成报错', () => {
    // `loadStatus(true)` 是**静默**模式：静默分支会 throw，所以必须自己吞掉；
    // 换成非静默会弹一条「读取失败」，用户会以为仲裁没生效。
    const slice = body.slice(body.indexOf('this.loadStatus('), body.indexOf('return true;'));
    expect(slice).toContain('loadStatus(true)');
    expect(slice, '必须吞掉状态刷新失败').toContain('.catch(');
  });
});
