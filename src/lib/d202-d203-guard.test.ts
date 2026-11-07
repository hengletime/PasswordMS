// D-202 / D-203 守门：确认框说了什么，界面就得能做到什么。
//
// ## D-202（删分组的确认框）
//
// `folder.delete_body` 原文写着"…或一并移入回收站，**请选择处理方式**"，而界面上的
// `ConfirmDialog` 只有「取消 / 确定」两颗按钮，代码固定调 `deleteFolder(id,'to_ungrouped')`。
// 也就是说：正文承诺了一个界面没给的选择，用户按「确定」时并不知道按下去会发生什么。
// 这两个键（`delete_to_ungrouped` / `delete_to_trash`）在字典里躺了好几版，
// 直到 D-194 的反扫才被翻出来——**走查单却一直把它们写成可点选项**，去看的人永远找不到。
//
// 主人拍板走 C 案（2026-11-07）：不补第二个选项，改为**说实话**——
// 正文改成"条目会被移出分组，不会被删除"，主按钮从「确定」改叫「移出分组」。
// 「移入回收站」这个键四本同删（后端 `FolderDeleteStrategy::ToTrash` 仍在且有用例，
// 只是界面不再有入口；哪天要做 A 案再把它加回来，本文件第 3 条会立刻提示）。
//
// ## D-203（同步页的行动指引）
//
// docs/06 §5.6 写明「状态页列表下方常驻 `sync.manual_sync_hint` 行动指引」，D-134 那一版
// 只落了「本机网络」与「记录地址」两行，这句一直没渲染。它不是遗留键，是**漏的渲染点**：
// 键里写的正是"为什么连不上"的三条出路（对端锁屏要解锁并保持前台 / 两端地址应同网段 /
// 地址长期对不上就重新扫码配对）。

import { describe, expect, it } from 'vitest';

import { hasKey, LOCALES, translate } from './i18n/index';

const DICT_SOURCES = import.meta.glob(['../**/*.svelte'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const modules = import.meta.glob(
  [
    '../components/FolderSidebar.svelte',
    '../components/common/ConfirmDialog.svelte',
    '../components/sync/SyncStatus.svelte',
    './stores/ui.svelte.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const sidebar = modules['../components/FolderSidebar.svelte'] ?? '';
const dialog = modules['../components/common/ConfirmDialog.svelte'] ?? '';
const syncStatus = modules['../components/sync/SyncStatus.svelte'] ?? '';
const uiStore = modules['./stores/ui.svelte.ts'] ?? '';

/** 从 `from` 切到锚点 `to`；切不出来就让用例红，别静默返回空串。 */
function slice(src: string, from: string, to: string, label: string): string {
  const a = src.indexOf(from);
  expect(a, `${label}：找不到起点 ${from}`).toBeGreaterThan(-1);
  const b = src.indexOf(to, a + from.length);
  expect(b, `${label}：找不到终点 ${to}（切不出函数体）`).toBeGreaterThan(a);
  return src.slice(a, b);
}

/** 四语言里"正文不该再承诺一个界面没有的选择"的禁词。 */
const PROMISE_WORDS: Record<string, string[]> = {
  'zh-CN': ['请选择处理方式', '移入回收站'],
  'zh-TW': ['請選擇處理方式', '資源回收筒'],
  'en-US': ['Choose an action', 'sent to trash'],
  th: ['เลือกวิธีจัดการ', 'ถังขยะ'],
};

/** 四语言里"后果必须说清"的关键词（条目不会被删）。 */
const CONSEQUENCE: Record<string, string> = {
  'zh-CN': '不会被删除',
  'zh-TW': '不會被刪除',
  'en-US': 'Nothing is deleted',
  th: 'ไม่มีข้อมูลถูกลบ',
};

describe('D-202：删分组的确认框不再承诺一个不存在的选择', () => {
  it('守门别空跑：三个组件与 store 都真的读到了', () => {
    expect(sidebar.length, 'FolderSidebar.svelte 读成空串，下面的断言全是假绿').toBeGreaterThan(
      1000,
    );
    expect(dialog.length, 'ConfirmDialog.svelte 读成空串').toBeGreaterThan(500);
    expect(syncStatus.length, 'SyncStatus.svelte 读成空串').toBeGreaterThan(1000);
    expect(uiStore.length, 'ui.svelte.ts 读成空串').toBeGreaterThan(1000);
    expect(Object.keys(DICT_SOURCES).length, '组件扫描面为空').toBeGreaterThan(10);
  });

  it('四语言正文都不许再说"请选择处理方式"，且必须说清条目不会被删', () => {
    for (const locale of LOCALES) {
      expect(hasKey(locale, 'folder.delete_body'), `${locale} 缺 folder.delete_body`).toBe(true);
      const text = translate(locale, 'folder.delete_body');
      for (const word of PROMISE_WORDS[locale] ?? []) {
        expect(text, `${locale} 的 folder.delete_body 仍在承诺"${word}"`).not.toContain(word);
      }
      expect(
        text,
        `${locale} 的 folder.delete_body 没写清后果（用户最怕的是"条目也没了"）`,
      ).toContain(CONSEQUENCE[locale]);
    }
  });

  it('「移入回收站」这个键四本都不许长回来（界面没有第二个选项）', () => {
    for (const locale of LOCALES) {
      expect(
        hasKey(locale, 'folder.delete_to_trash'),
        `${locale} 里又有了 folder.delete_to_trash —— 加回来就必须同时给它一个按钮`,
      ).toBe(false);
    }
  });

  it('按钮词由调用方指定：removeFolder 传「移出分组」，组件按请求渲染', () => {
    const body = slice(sidebar, 'async function removeFolder(', '/** 选中分组', 'removeFolder');
    expect(body.length, 'removeFolder 函数体切错了').toBeGreaterThan(100);
    expect(body, 'removeFolder 没有把动作名传给确认框').toContain(
      "'folder.delete_to_ungrouped'",
    );
    // 确认框渲染层：默认仍是「确定」，只有请求里带了 confirmKey 才换词。
    expect(dialog, 'ConfirmDialog 没有按 request.confirmKey 渲染主按钮').toContain(
      "request.confirmKey ?? 'common.confirm'",
    );
    // store 契约：confirmKey 必须真的存进请求，否则组件永远读到 undefined。
    expect(uiStore, 'confirm() 的 confirmKey 没有写进 confirmRequest').toContain(
      'this.confirmRequest = { titleKey, messageKey, params, confirmKey, resolve }',
    );
  });
});

describe('D-203：同步页必须真的渲染那句行动指引', () => {
  it('SyncStatus 渲染了 sync.manual_sync_hint', () => {
    expect(syncStatus, '同步状态页没有渲染 sync.manual_sync_hint（docs/06 §5.6 要求常驻）').toContain(
      "ui.t('sync.manual_sync_hint')",
    );
  });

  it('四语言都有这句，且它在界面上说得出口（不是键名、不是空串）', () => {
    for (const locale of LOCALES) {
      expect(hasKey(locale, 'sync.manual_sync_hint'), `${locale} 缺 sync.manual_sync_hint`).toBe(
        true,
      );
      const text = translate(locale, 'sync.manual_sync_hint');
      expect(text, `${locale} 的 sync.manual_sync_hint 原样返回了键名`).not.toBe(
        'sync.manual_sync_hint',
      );
      expect(text.length, `${locale} 的 sync.manual_sync_hint 太短，说不了三条出路`).toBeGreaterThan(
        40,
      );
    }
  });
});
