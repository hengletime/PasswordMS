// D-171 分组冲突（Kimi 审计 F-01）的接线守卫。
//
// 这一轮有三个「写错不会报错、单测也抓不到」的地方：
//
// 1. **命令层列哪张表**：`list_conflicts` 若仍调 `list_conflicts()`（只含条目），
//    core 里分组冲突收得再对，用户也永远看不到——界面显示「没有待处理冲突」。
// 2. **面板画几个按钮**：分组不支持「两者都留」。模板若写死三选一，分组卡片上会多出
//    一个按钮，点下去 back-end 直接返回校验错误（core 侧已拒），用户只会觉得坏了。
// 3. **DTO 字段名**：`record_type` / `record_id` / `remote_deleted` 三个名字跨
//    Rust ↔ TS，写错一个字母 TypeScript 因为 DTO 是「从 Rust 手抄」而不会报错。
//
// 这三处都靠本文件钉住；行为正确性由 core `tests/d171_folder_conflict.rs`（13 例）负责。

import { describe, expect, it } from 'vitest';

import {
  CONFLICT_CHOICES,
  FOLDER_CONFLICT_CHOICES,
  conflictChoices,
  isChoiceAllowed,
} from './conflict';

const raw = import.meta.glob(
  [
    '../../src-tauri/src/commands/sync_cmd.rs',
    '../../src-tauri/src/dto.rs',
    '../../crates/passwordms-core/src/constants.rs',
    '../../crates/passwordms-core/src/store/repository.rs',
    '../../crates/passwordms-core/src/sync/merge.rs',
    '../components/sync/ConflictPanel.svelte',
    './stores/sync.svelte.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function src(suffix: string): string {
  const hit = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1];
  if (hit === undefined || hit.length === 0) {
    throw new Error(`读不到源码：${suffix}（glob 空集会静默通过，故显式抛错）`);
  }
  return hit;
}

function fnBody(source: string, signature: string): string {
  const at = source.indexOf(signature);
  if (at < 0) {
    throw new Error(`找不到 ${signature}`);
  }
  let i = source.indexOf('{', at);
  if (i < 0) {
    throw new Error(`${signature} 之后没有函数体`);
  }
  let depth = 0;
  for (; i < source.length; i += 1) {
    if (source[i] === '{') {
      depth += 1;
    } else if (source[i] === '}') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(at, i + 1);
      }
    }
  }
  throw new Error(`${signature} 的函数体没有闭合`);
}

const panel = src('ConflictPanel.svelte');
const syncCmd = src('sync_cmd.rs');
const dto = src('dto.rs');
const constants = src('constants.rs');
const repository = src('repository.rs');
const merge = src('merge.rs');
const syncStore = src('stores/sync.svelte.ts');

describe('选项集合：分组没有「两者都留」', () => {
  it('条目三选一，分组只有两项', () => {
    expect(CONFLICT_CHOICES).toEqual(['keep_local', 'take_remote', 'keep_both']);
    expect(FOLDER_CONFLICT_CHOICES).toEqual(['keep_local', 'take_remote']);
    expect(conflictChoices('item')).toEqual(CONFLICT_CHOICES);
    expect(conflictChoices('folder')).toEqual(FOLDER_CONFLICT_CHOICES);
  });

  it('isChoiceAllowed 挡住分组 + keep_both（未选择也视为不允许）', () => {
    expect(isChoiceAllowed('folder', 'keep_both')).toBe(false);
    expect(isChoiceAllowed('folder', 'keep_local')).toBe(true);
    expect(isChoiceAllowed('item', 'keep_both')).toBe(true);
    expect(isChoiceAllowed('item', null)).toBe(false);
    expect(isChoiceAllowed('folder', null)).toBe(false);
  });
});

describe('面板渲染必须跟着记录类型走', () => {
  it('选项由 conflictChoices(record_type) 给出，模板里没有写死的三选一', () => {
    expect(panel).toContain('conflictChoices(conflict.record_type)');
    // 回归口子：`{#each CONFLICT_CHOICES as choice}` 会让分组卡片多一个必失败按钮。
    expect(panel).not.toContain('each CONFLICT_CHOICES as choice');
  });

  it('「已选择」判定用 isChoiceAllowed，而不是只判 null', () => {
    expect(panel).toContain('isChoiceAllowed(conflict.record_type, sync.choiceOf(');
  });

  it('远端是删除墓碑时卡片上要标出来', () => {
    expect(panel).toContain('conflict.remote_deleted');
    expect(panel).toContain("'conflict.remote_deleted'");
    expect(panel).toContain('{#if conflict.remote_deleted}');
  });

  it('卡片标明是条目还是分组', () => {
    expect(panel).toContain("'conflict.kind_folder'");
    expect(panel).toContain("'conflict.kind_item'");
  });
});

describe('提交路径：分组 keep_both 在客户端就被挡住', () => {
  it('store.resolve 用 isChoiceAllowed 复核后再调后端', () => {
    const body = fnBody(syncStore, 'async resolve(conflictId: string)');
    expect(body).toContain('isChoiceAllowed(target.record_type, choice)');
    expect(body).toContain('this.conflicts.find(');
  });

  it('core 侧对分组 keep_both 返回校验错误且不改状态', () => {
    const body = fnBody(repository, 'fn resolve_folder_conflict(');
    expect(body).toContain('folder_conflict_no_keep_both');
    // 必须在写任何东西之前就返回：否则「点了没反应但冲突被标成已解决」。
    const rejectAt = body.indexOf('folder_conflict_no_keep_both');
    expect(rejectAt).toBeGreaterThan(0);
    expect(body.indexOf('put_folder_direct')).toBeGreaterThan(rejectAt);
    expect(body.indexOf('self.persist_header()')).toBeGreaterThan(rejectAt);
  });
});

describe('命令层与 DTO 接线', () => {
  it('list_conflicts 命令列的是两类合一的表', () => {
    const body = fnBody(syncCmd, 'pub fn list_conflicts(');
    expect(body).toContain('list_all_conflicts()');
    // 回归口子：只列条目 = 分组冲突永远不上屏。
    expect(body).not.toContain('.list_conflicts()');
  });

  it('DTO 三个新字段名与 TS 侧一致', () => {
    for (const field of ['record_type', 'record_id', 'remote_deleted']) {
      expect(dto).toContain(`pub ${field}:`);
    }
    expect(dto).toContain('ConflictView::Folder(c)');
    expect(dto).toContain('c.local_snapshot.name');
  });

  it('分组冲突用独立记录类型，不能复用 0x04', () => {
    expect(constants).toContain('pub const RECORD_TYPE_FOLDER_CONFLICT: u8 = 0x08;');
    expect(constants).toContain('pub const RECORD_TYPE_CONFLICT: u8 = 0x04;');
    expect(repository).toContain('RECORD_TYPE_FOLDER_CONFLICT => {');
  });
});

describe('合并链路：分组双改不再静默覆盖', () => {
  it('FOLDER 分支调用 detect_folder_conflict 并在命中时入冲突箱', () => {
    expect(merge).toContain('detect_folder_conflict(&local, &remote, base.as_ref())');
    expect(merge).toContain('repo.add_folder_conflict(entry)?;');
    // 回归口子：无条件直落 = 谁后同步谁赢，另一边零提示。
    expect(merge).not.toContain('} else {\n                    repo.put_folder_direct(remote.clone())?;\n                    report.applied += 1;\n                }');
  });

  it('分组删除也走「本端改过 → 冲突」而不是直接删', () => {
    const body = fnBody(merge, 'pub fn apply_changes(');
    expect(body).toContain('Some(local) if local.rev > change.rev => {');
    expect(body).toContain('remote_deleted: true');
  });

  it('条目与分组共用同一份「双改」判定', () => {
    const detect = fnBody(merge, 'pub fn detect_conflict(');
    const detectFolder = fnBody(merge, 'pub fn detect_folder_conflict(');
    expect(detect).toContain('detect_change_conflict(');
    expect(detectFolder).toContain('detect_change_conflict(');
    expect(merge).toContain('fn detect_change_conflict(');
  });
});
