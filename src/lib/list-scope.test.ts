// D-157：移动端顶栏标题的判定。测的是「读出来是什么」，不是「有没有调用某个函数」。
import { describe, expect, it } from 'vitest';
import { listScopeLabel, type ListScope } from './list-scope';
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

/** 假翻译：直接把 key 打回，断言就不必依赖真实文案。 */
const t = (key: string): string => key;

const ALL: ListScope = { folderId: null, favoriteOnly: false, ungroupedOnly: false };

describe('D-157：移动端顶栏标题说清当前作用域', () => {
  it('全部 → 段落名「全部条目」', () => {
    expect(listScopeLabel(ALL, [], t)).toBe('nav.items');
  });

  it('选中分组 → 直接是组名（不带任何前缀）', () => {
    expect(listScopeLabel({ ...ALL, folderId: 'f1' }, [folder('f1', '工作')], t)).toBe('工作');
  });

  it('仅收藏 → 「收藏」，不是组名也不是「全部条目」', () => {
    expect(listScopeLabel({ ...ALL, favoriteOnly: true }, [folder('f1', '工作')], t)).toBe(
      'nav.favorites',
    );
  });

  it('未分组视图 → 「未分组」（与侧栏行同一文案源）', () => {
    expect(listScopeLabel({ ...ALL, ungroupedOnly: true }, [], t)).toBe('folder.ungrouped');
  });

  it('folder_id 指向不存在的分组 → 「未知分组」，不冒充未分组', () => {
    expect(listScopeLabel({ ...ALL, folderId: 'gone' }, [folder('f1', '工作')], t)).toBe(
      'folder.unknown',
    );
  });

  it('分组列表还没加载回来（空数组）时不退回「全部条目」', () => {
    // 解锁瞬间 folders 与 items 同批拉回，理论上同步到；但手机上点开抽屉的那一刻
    // 若 folders 为空还显示「全部条目」，就等于对「我在工作组里」这件事撒谎。
    expect(listScopeLabel({ ...ALL, folderId: 'f1' }, [], t)).toBe('folder.unknown');
  });

  it('空串 folder_id 视为没有分组（交给 ungroupedOnly 或段落名），不当未知', () => {
    expect(listScopeLabel({ ...ALL, folderId: '' }, [folder('f1', '工作')], t)).toBe('nav.items');
  });

  it('三态优先级固定：收藏 / 未分组先于 folderId', () => {
    // 正常路径下 store 保证互斥；这里钉住「即便数据异常，读法也只有一个」。
    const messy: ListScope = { folderId: 'f1', favoriteOnly: true, ungroupedOnly: true };
    expect(listScopeLabel(messy, [folder('f1', '工作')], t)).toBe('nav.favorites');
  });
});
