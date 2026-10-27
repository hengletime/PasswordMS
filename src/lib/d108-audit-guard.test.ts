// D-108：豆包外部审查余项修复守门（源码断言，走 import.meta.glob('?raw')，
// svelte-check 不认 node:fs）。锁死本轮结构性修复，防止回归：
// auto-lock 触点、锁定级联复用、autolock 事件链路、capabilities/CSP 收窄、
// 死代码删除、DTO 收敛、五路加载合并提示。

import { describe, expect, it } from 'vitest';

// import.meta.glob 只接受字面量 pattern（不能经辅助函数转发）；范围必须收窄到
// 源码目录——宽 pattern（如 ../../src-tauri/**）会把 target/ 下的构建产物抓进来，
// 读到陈旧副本导致假红。
const srcLib = import.meta.glob('./*.{ts,svelte}', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const srcStores = import.meta.glob('./stores/*.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const srcI18n = import.meta.glob('./i18n/*.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const srcComponents = import.meta.glob(['../App.svelte', '../types.ts', '../components/*.svelte'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const tauriSrc = import.meta.glob('../../src-tauri/src/**/*.rs', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;
const tauriConf = import.meta.glob(
  ['../../src-tauri/tauri.conf.json', '../../src-tauri/capabilities/*.json'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const srcFiles: Record<string, string> = { ...srcLib, ...srcStores, ...srcI18n, ...srcComponents };
const tauriFiles: Record<string, string> = { ...tauriSrc, ...tauriConf };

const read = (files: Record<string, string>, suffix: string): string => {
  const hit = Object.entries(files).find(([p]) => p.endsWith(suffix));
  expect(hit, `源文件缺失: ${suffix}`).toBeDefined();
  return hit?.[1] ?? '';
};

describe('D-108 3.4-6 auto-lock：lock_session 触点全覆盖', () => {
  const commandFiles = [
    'commands/folder_cmd.rs',
    'commands/health_cmd.rs',
    'commands/import_cmd.rs',
    'commands/item_cmd.rs',
    'commands/keydoc_cmd.rs',
    'commands/ordering_cmd.rs',
    'commands/settings_cmd.rs',
    'commands/sync_cmd.rs',
    'commands/system_cmd.rs',
    'commands/trash_cmd.rs',
    'commands/vault_cmd.rs',
  ];

  it('全部会话锁调用统一走 lock_session()（自动刷新活动时间戳）', () => {
    for (const f of commandFiles) {
      const src = read(tauriFiles, f);
      expect(src, `${f} 应使用 lock_session`).toContain('.lock_session()');
      expect(src, `${f} 不得再直接 state.session.lock().map_err`).not.toContain(
        'state.session.lock().map_err(lock_err)?',
      );
    }
  });

  it('worker 自身轮询不刷新活动时间戳（state.rs 直用 session.lock()）', () => {
    const state = read(tauriFiles, 'src/state.rs');
    expect(state).toContain('spawn_auto_lock_worker');
    expect(state).toContain('last_activity_ms');
  });

  it('锁定级联唯一实现：lock 命令与 worker 共用 perform_lock', () => {
    const vaultCmd = read(tauriFiles, 'commands/vault_cmd.rs');
    expect(vaultCmd).toContain('pub fn perform_lock');
    const lib = read(tauriFiles, 'src/lib.rs');
    expect(lib).toContain('spawn_auto_lock_worker');
  });

  it('前端监听 autolock://locked 并复位 store（App.svelte 常驻注册）', () => {
    const app = read(srcFiles, 'App.svelte');
    expect(app).toContain('AUTOLOCK_LOCKED_EVENT');
    const evt = read(srcFiles, 'auto-lock-event.ts');
    expect(evt).toContain("export const AUTOLOCK_LOCKED_EVENT = 'autolock://locked'");
  });
});

describe('D-108 3.4-11/12 capabilities 与 CSP 收窄', () => {
  it('capabilities 不再放行 opener（open_url 白名单链路是唯一出口）', () => {
    const caps = read(tauriFiles, 'capabilities/default.json');
    expect(caps).not.toContain('opener:');
  });

  it('CSP 补纵深指令并放行 GitHub API（D-107 更新检查）', () => {
    const conf = read(tauriFiles, 'tauri.conf.json');
    expect(conf).toContain("object-src 'none'");
    expect(conf).toContain("base-uri 'none'");
    expect(conf).toContain("form-action 'none'");
    expect(conf).toContain('https://api.github.com');
  });
});

describe('D-108 3.5-7 死代码 sync_accept_pair 双侧删除', () => {
  it('前端 ipc.ts 与后端 sync_cmd.rs / lib.rs 均无残留（注释提及不算）', () => {
    expect(read(srcFiles, 'ipc.ts')).not.toContain("invoke<void>('sync_accept_pair'");
    expect(read(tauriFiles, 'commands/sync_cmd.rs')).not.toContain('pub fn sync_accept_pair');
    expect(read(tauriFiles, 'src/lib.rs')).not.toContain('sync_cmd::sync_accept_pair');
  });
});

describe('D-108 3.4-8 vault_file_info 只回传文件名', () => {
  it('DTO 字段更名 file_name，前端同步', () => {
    const dto = read(tauriFiles, 'dto.rs');
    expect(dto).not.toContain('pub path: String');
    expect(dto).toContain('pub file_name: String');
    expect(read(srcFiles, 'types.ts')).toContain('file_name: string');
    expect(read(srcFiles, 'Settings.svelte')).toContain('fileInfo.file_name');
  });
});

describe('D-108 3.5-10 五路加载失败合并提示', () => {
  it('MainLayout 走 loadAfterUnlock + allSettled，stores 提供 silent 通道', () => {
    const layout = read(srcFiles, 'MainLayout.svelte');
    expect(layout).toContain('loadAfterUnlock');
    expect(layout).toContain('Promise.allSettled');
    expect(layout).toContain("ui.toast('error', 'err.load_partial')");
    for (const store of ['stores/vault.svelte.ts', 'stores/settings.svelte.ts']) {
      expect(read(srcFiles, store)).toContain('silent = false');
    }
  });
});

describe('D-107 更新检查 UI 与 i18n', () => {
  it('Settings 关于卡片含开关 + 手动按钮 + 下载跳转', () => {
    const settings = read(srcFiles, 'Settings.svelte');
    expect(settings).toContain('update_check_now');
    expect(settings).toContain('update_download');
    expect(settings).toContain('UPDATE_RELEASES_URL');
  });

  it('四语言 update_* 键与 err.load_partial / err.attachment_not_found 全部落位', () => {
    for (const lang of ['zh-CN', 'zh-TW', 'en-US', 'th']) {
      const src = read(srcFiles, `i18n/${lang}.ts`);
      expect(src, `${lang} update_auto`).toContain('update_auto:');
      expect(src, `${lang} update_check_now`).toContain('update_check_now:');
      expect(src, `${lang} update_available`).toContain('update_available:');
      expect(src, `${lang} err.load_partial`).toContain('load_partial:');
      expect(src, `${lang} err.attachment_not_found`).toContain('attachment_not_found:');
      expect(src, `${lang} import.header_read_failed`).toContain('header_read_failed:');
    }
  });

  it('四语言用户协议含更新检查披露（请求不含用户数据）', () => {
    const legal = read(srcFiles, 'legal-content.ts');
    expect(legal).toContain('该请求不携带任何用户数据');
    expect(legal).toContain('該請求不攜帶任何使用者資料');
    expect(legal).toContain('that request carries no user data');
    // 泰文残留注音清干净（5.2-7 收尾）。
    expect(legal).not.toContain('(crash log)');
  });
});
