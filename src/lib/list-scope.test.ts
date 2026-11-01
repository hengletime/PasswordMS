// D-157：移动端顶栏标题的判定。测的是「读出来是什么」，不是「有没有调用某个函数」。
import { describe, expect, it } from 'vitest';
import { listScopeLabel, shouldShowRowFolder, type ListScope } from './list-scope';
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

// D-160：列表副行的分组名 —— 只在「列表里会出现不同分组条目」时显示。
//
// 主人反馈（2026-11-01，附超长组名截图）：「只有为全部（全部条目）这里才显示分组名，
// 如果已经在某分组里了，这里就没有必要显示分组名了。」
//
// 这一组用例里最重要的一条是「收藏仍显示」：按「有没有选分组」写判据（folderId === null）
// 会把收藏一起关掉，而收藏恰恰是跨组的 —— 那才是唯一「看着像全部、其实更需要组名」的视图。
describe('D-160：副行分组名只在跨组视图显示', () => {
  it('全部：显示（全库混合，组名是唯一归属线索）', () => {
    expect(shouldShowRowFolder(ALL)).toBe(true);
  });

  it('某个分组内：隐藏（每条都属于这个组，重复 20 遍只是噪音）', () => {
    expect(shouldShowRowFolder({ ...ALL, folderId: 'f1' })).toBe(false);
  });

  it('收藏：显示（收藏是跨组的，条目来自各个组）', () => {
    expect(shouldShowRowFolder({ ...ALL, favoriteOnly: true })).toBe(true);
  });

  it('未分组视图：隐藏（每行都写「未分组」= 同一句话念 20 遍）', () => {
    expect(shouldShowRowFolder({ ...ALL, ungroupedOnly: true })).toBe(false);
  });

  it('空串 folder_id 视为「没有分组」→ 显示（与顶栏标题同口径）', () => {
    expect(shouldShowRowFolder({ ...ALL, folderId: '' })).toBe(true);
  });

  it('状态位异常叠加时读法仍只有一个：收藏优先（与 listScopeLabel 同序）', () => {
    const messy: ListScope = { folderId: 'f1', favoriteOnly: true, ungroupedOnly: true };
    expect(shouldShowRowFolder(messy)).toBe(true);
  });
});
