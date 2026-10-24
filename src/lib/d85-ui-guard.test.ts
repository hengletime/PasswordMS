// D-85 UI 守卫：点钉取消钉住 + 分组改名。
//
// 无 DOM 测试环境，组件行为用**源码结构断言**钉（与 d81-ui-guard / usage-tracking 同族）。
// 契约关键点：`update_folder(folder_id, name, icon, color)` 对 icon/color 是**整字段覆盖**
// （repository.rs：`folder.icon = icon; folder.color = color`），传 None = 清除——
// 所以改名必须原样回传 icon/color，测试钉死这一点（防止未来改回 `vault.updateFolder`
// 的 null/null 写法把分组图标/颜色抹掉）。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob('../components/*.svelte', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('D-85 需求 1：点钉图标 = 直接取消钉住', () => {
  it('ItemList：钉住按钮是行按钮的兄弟节点（不嵌套），click 直接 setItemPin(id, false)', () => {
    const src = pickSource('ItemList.svelte');
    const pinAt = src.indexOf('class="list-pin"');
    expect(pinAt, '应有 list-pin 钉住按钮').toBeGreaterThan(-1);
    // 是 button 而非纯展示 span，且带可访问名（title 用「取消钉住」文案）。
    const tagAt = src.lastIndexOf('<button', pinAt);
    expect(src.slice(tagAt, pinAt + 60)).toContain('type="button"');
    // 兄弟节点而非嵌套：pin 之前最近一次 </button> 必须在行按钮开始之后（行已闭合）。
    const rowOpenAt = src.indexOf('class="list-row"');
    const closeBeforePin = src.lastIndexOf('</button>', pinAt);
    expect(closeBeforePin, '钉住按钮必须不在行按钮内部（嵌套 button 无效 HTML）').toBeGreaterThan(rowOpenAt);
    expect(src.slice(pinAt, src.indexOf('</button>', pinAt))).toContain("ui.t('menu.unpin')");
    const unpinAt = src.indexOf('vault.setItemPin(item.item_id, false)', pinAt);
    expect(unpinAt, '点钉必须直接 setItemPin(false)').toBeGreaterThan(pinAt);
  });

  it('FolderSidebar：分组钉住按钮同款兄弟结构 —— setFolderPin(id, false)', () => {
    const src = pickSource('FolderSidebar.svelte');
    const pinAt = src.indexOf('class="side-pin"');
    expect(pinAt, '应有 side-pin 钉住按钮').toBeGreaterThan(-1);
    const tagAt = src.lastIndexOf('<button', pinAt);
    expect(src.slice(tagAt, pinAt + 60)).toContain('type="button"');
    const rowOpenAt = src.indexOf('class="side-row"');
    const closeBeforePin = src.lastIndexOf('</button>', pinAt);
    expect(closeBeforePin, '钉住按钮必须不在行按钮内部（嵌套 button 无效 HTML）').toBeGreaterThan(rowOpenAt);
    const unpinAt = src.indexOf('vault.setFolderPin(folder.folder_id, false)', pinAt);
    expect(unpinAt, '点钉必须直接 setFolderPin(false)').toBeGreaterThan(pinAt);
  });
});

describe('D-85 需求 2：分组改名（菜单 → Modal 提交）', () => {
  it('菜单含「重命名」项，弹层复用 Modal + folder.rename_title', () => {
    const src = pickSource('FolderSidebar.svelte');
    expect(src).toContain("{ key: 'rename', label: ui.t('menu.rename') }");
    expect(src).toContain('titleKey="folder.rename_title"');
  });

  it('改名走 api.folders.update 且 icon/color 原样回传（整字段覆盖契约，防 null 抹除）', () => {
    const src = pickSource('FolderSidebar.svelte');
    expect(src).toContain('api.folders.update(target.folder_id, name, target.icon ?? null, target.color ?? null)');
    // 成功后刷新列表。
    expect(src).toContain('await vault.load()');
    // 空名/纯空白前端先挡（不发 IPC）：renameFolder 有空名早退。
    const fnAt = src.indexOf('async function renameFolder');
    expect(fnAt).toBeGreaterThan(-1);
    const guardAt = src.indexOf('name.length === 0', fnAt);
    expect(guardAt, '空名必须在前端挡下').toBeGreaterThan(fnAt);
  });

  it('负向：FolderSidebar 不再调用 vault.updateFolder（它传 null/null，会抹掉 icon/color）', () => {
    const src = pickSource('FolderSidebar.svelte');
    // 断言**调用点**（`vault.updateFolder(`）而非裸名 —— 契约注释里提到了这个名字，剥注释断言才是准的。
    expect(src, '双击与菜单改名都已收敛到 renameFolder').not.toContain('vault.updateFolder(');
  });
});
