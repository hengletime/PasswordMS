// D-144 / D-145 的源码结构守卫。
//
// 这两条的成因都是「**某个平台上少了一条分支**」，而单测跑在 Node 上：
// Android 专属代码在这里既不编译也不执行，行为级测试**永远不可能**发现它缺失。
// 所以守门只能钉接线，把「这条分支必须存在」写成断言。
//
// 事故原貌（2026-10-31 真机）：
//   手机「导出保险库」→ 盘上出现 **0 字节** 文件 → 界面只有一句「确定」→
//   用户以为备份成功 → 卸载（`allowBackup=false` 使 /data/data 整体消失）→
//   拿那份空备份恢复 → 「输入无效」→ 数据彻底没了。
//
// 三处断裂，各钉一条：
//   ① `vault_cmd::export_vault` 只走 `std::fs::copy`，没有 `content://` 分支
//      → SAF 建好的占位文件永远是 0 字节；
//   ② `content_uri::copy_from_file` 若只写不验，「写失败仍报成功」会复活；
//   ③ 前端若不把 0 字节判失败，用户仍会看到一句成功。
//
// 读文件走 `import.meta.glob(?raw)`：tsconfig 无 node 类型，`node:fs` 会让
// `npm run check` 直接红（2026-10-31 轮次已踩过）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../../src-tauri/src/commands/vault_cmd.rs',
    '../../src-tauri/src/content_uri.rs',
    './vault-export.ts',
    './ipc.ts',
    '../components/Settings.svelte',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

function src(suffix: string): string {
  return Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
}

const vaultCmd = src('vault_cmd.rs');
const contentUri = src('content_uri.rs');
const vaultExport = src('vault-export.ts');
const ipc = src('/ipc.ts');
const settings = src('Settings.svelte');

/**
 * 截出某个成员的源码片段（从签名到下一个同级成员/注释）。
 *
 * Rust 用 `///` 文档注释分成员，TS 用 2 空格缩进的 `//` 或下一个 `export`——
 * 两种都要认，否则 TS 侧会一路截到文件尾，把别人的 `Promise<void>` 算到这条头上。
 */
function fnBody(source: string, signature: string): string {
  const start = source.indexOf(signature);
  if (start < 0) {
    return '';
  }
  const rest = source.slice(start + signature.length);
  const stops = ['\n///', '\n  //', '\n  export'].map((m) => {
    const i = rest.indexOf(m);
    return i < 0 ? Number.POSITIVE_INFINITY : i;
  });
  const end = Math.min(...stops);
  return Number.isFinite(end) ? rest.slice(0, end) : rest;
}

describe('D-144 导出保险库必须有 Android content:// 分支', () => {
  it('五个源文件都读到了（守门本身别空跑）', () => {
    expect(vaultCmd.length).toBeGreaterThan(2000);
    expect(contentUri.length).toBeGreaterThan(2000);
    expect(vaultExport.length).toBeGreaterThan(1000);
    expect(ipc.length).toBeGreaterThan(2000);
    expect(settings.length).toBeGreaterThan(10000);
  });

  it('export_vault 认得 content:// 并走流式拷贝（缺失则 SAF 只留 0 字节占位）', () => {
    const body = fnBody(vaultCmd, 'pub fn export_vault(');
    expect(body.length).toBeGreaterThan(300);
    expect(body).toContain('starts_with("content://")');
    expect(body).toContain('copy_from_file');
    // 临时文件必须是本机库副本，迟早要被删掉 —— 不许留在数据目录里。
    expect(body).toContain('remove_file');
  });

  it('export_vault 回传实际字节数（前端据此把 0 B 摆给用户看）', () => {
    expect(vaultCmd).toContain('pub fn export_vault(');
    expect(fnBody(vaultCmd, 'pub fn export_vault(')).toContain('IpcResult<u64>');
  });

  it('copy_from_file 写完必回读长度（「写失败仍报成功」的唯一硬防线）', () => {
    const body = fnBody(contentUri, 'pub(crate) fn copy_from_file(');
    expect(body.length).toBeGreaterThan(500);
    // 关闭 fd（SAF 于 close 提交）之后再比对长度，顺序不能反。
    expect(body).toContain('drop(out)');
    expect(body).toContain('content_write_size_mismatch');
    expect(body).toContain('meta.len()');
    // 0 字节源文件直接拒绝：不制造「看起来成功」的空备份。
    expect(body).toContain('empty_source');
  });
});

describe('D-144 前端必须把 0 字节判失败', () => {
  it('0 字节走失败分支而不是成功文案', () => {
    expect(vaultExport).toContain('EXPORT_EMPTY_KEY');
    expect(vaultExport).toContain('written === 0');
  });

  it('成功文案带实际体积插值（0 B 无处可藏）', () => {
    expect(vaultExport).toContain('EXPORT_OK_WITH_SIZE_KEY');
    expect(vaultExport).toContain('formatBytes');
  });

  it('IPC 契约返回 number（不是 void），否则前端拿不到字节数', () => {
    const body = fnBody(ipc, 'exportVault:');
    expect(body).toContain('Promise<number>');
    expect(body).not.toContain('Promise<void>');
  });

  it('设置页把「上次成功导出」记下来并显示（用户唯一的复查凭据）', () => {
    expect(settings).toContain('recordBackup');
    expect(settings).toContain('lastBackupRecord');
    expect(settings).toContain('settings.backup_last_ok');
  });
});

describe('D-145 备份恢复的失败文案必须分成因', () => {
  it('后端按「空 / 不是备份 / 损坏」三种成因给不同文案键', () => {
    expect(vaultCmd).toContain('err.backup_empty');
    expect(vaultCmd).toContain('err.backup_not_vault');
    expect(vaultCmd).toContain('err.backup_corrupt');
    // 分叉函数必须真的被两条恢复路径用到（桌面真实路径 + Android 临时文件）。
    const uses = vaultCmd.split('backup_reject(').length - 1;
    expect(uses).toBeGreaterThanOrEqual(3);
  });
});
