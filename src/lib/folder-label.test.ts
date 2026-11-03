// D-154：条目所属分组的标签判定。
//
// 四类输入（覆盖「未分组」的所有书写方式与悬空引用）+ 一条「非全量 folders 会误判」的边界。

import { describe, expect, it } from 'vitest';
import { folderLabelOf, folderLabelText, resolvableFolderId } from './folder-label';
import type { FolderDto } from '../types';

function folder(folder_id: string, name: string): FolderDto {
  return {
    folder_id,
    name,
    sort_order: 0,
    rev: 1,
    updated_at: 1,
    pinned: false,
    pin_order: 0,
    use_count: 0,
    last_used_at: null,
  };
}

describe('D-154 folderLabelOf', () => {
  it('null / undefined / 空串一律「未分组」（空串不是合法 id，别让它变成 unknown）', () => {
    for (const v of [null, undefined, '']) {
      expect(folderLabelOf(v, [folder('f1', '工作')])).toEqual({ kind: 'ungrouped' });
    }
  });

  it('命中分组 → 返回该分组名（不是 id）', () => {
    const folders = [folder('f1', '工作'), folder('f2', '财务')];
    expect(folderLabelOf('f2', folders)).toEqual({ kind: 'folder', name: '财务' });
  });

  it('悬空引用 → `unknown`，**不许冒充 ungrouped**（否则与侧栏未分组读数矛盾）', () => {
    expect(folderLabelOf('gone', [folder('f1', '工作')])).toEqual({ kind: 'unknown' });
  });

  it('空分组列表 + 有 folder_id → unknown（不是 ungrouped）', () => {
    expect(folderLabelOf('f1', [])).toEqual({ kind: 'unknown' });
  });
});

describe('D-154 folderLabelText', () => {
  const t = (key: string): string => `t:${key}`;

  it('正常归属直接给组名（不走 i18n）', () => {
    expect(folderLabelText({ kind: 'folder', name: '工作' }, t)).toBe('工作');
  });

  it('未分组 / 未知分组各取各的键（不许共用一个兜底文案）', () => {
    expect(folderLabelText({ kind: 'ungrouped' }, t)).toBe('t:folder.ungrouped');
    expect(folderLabelText({ kind: 'unknown' }, t)).toBe('t:folder.unknown');
  });
});

describe('D-174 resolvableFolderId', () => {
  const folders = [folder('f1', '工作')];

  it('能解析的分组原样返回（编辑时不许把它清空）', () => {
    expect(resolvableFolderId('f1', folders)).toBe('f1');
  });

  it('悬空 id 收敛成 null —— 下拉框显示未分组，保存也就存成未分组', () => {
    // 这一条是「改不动的条目」的根：悬空 id 塞进 bind:value 时，下拉框显示第一个选项
    // （未分组），但绑定的仍是那个 id，保存会被后端 validate_draft_folder 拒绝。
    expect(resolvableFolderId('gone', folders)).toBeNull();
    expect(resolvableFolderId('f1', [])).toBeNull();
  });

  it('空值保持 null（未分组不是错误状态）', () => {
    for (const v of [null, undefined, '']) {
      expect(resolvableFolderId(v, folders)).toBeNull();
    }
  });
});
