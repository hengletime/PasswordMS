// IPC 契约一致性（全局审查项 ①）：
// `src/lib/ipc.ts` 中所有 `invoke('<name>')` 的命令名，必须与 Rust 侧
// `src-tauri/src/lib.rs` 的 `generate_handler![...]` 注册列表**完全一致**。
// 任何一个名字不符（拼写 / 漏注册 / 多余）都会导致运行期 `command not found`。
//
// 说明：用 Vite 的 `import.meta.glob`（`?raw`）读取源码，避免依赖 Node 类型。

import { describe, expect, it } from 'vitest';

/** 从文本中按捕获组提取去重并排序后的名称集合。 */
function extract(text: string, pattern: RegExp): string[] {
  const out = new Set<string>();
  for (const match of text.matchAll(pattern)) {
    out.add(match[1]);
  }
  return [...out].sort();
}

const rustModules = import.meta.glob('../../src-tauri/src/*.rs', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const ipcModules = import.meta.glob('./ipc.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const libRs = Object.entries(rustModules).find(([path]) => path.endsWith('lib.rs'))?.[1] ?? '';
const ipcTs = ipcModules['./ipc.ts'] ?? '';

describe('ipc command contract', () => {
  // Rust：`commands::<module>::<fn>`。
  const registered = extract(libRs, /commands::[a-z_]+::([a-z0-9_]+)/g);
  // 前端：`invoke<...>('<name>'`。
  const invoked = extract(ipcTs, /invoke<[^>]*>\(\s*'([a-z0-9_]+)'/g);

  it('extracts the raw sources via import.meta.glob', () => {
    expect(libRs.length).toBeGreaterThan(0);
    expect(ipcTs.length).toBeGreaterThan(0);
  });

  it('extracts a non-trivial command set from both sides', () => {
    expect(registered.length).toBeGreaterThan(40);
    expect(invoked.length).toBe(registered.length);
  });

  it('frontend invoke names exactly match backend generate_handler! registrations', () => {
    expect(invoked).toEqual(registered);
  });

  it('keeps add_attachment / remove_attachment wired on both sides (D-60 前端入口回归锚点)', () => {
    // D-59 的教训（后端就绪但前端零调用 → 附件不可达）；D-60 更名后锚定新命令双侧在册。
    expect(invoked).toContain('add_attachment');
    expect(registered).toContain('add_attachment');
    expect(invoked).toContain('remove_attachment');
    expect(registered).toContain('remove_attachment');
  });

  it('keeps get_auto_lock / set_auto_lock wired on both sides (D-61 自动锁定回归锚点)', () => {
    // D-61：档位读写通道补齐；任何一侧被移除（前端忘调用 / 后端忘注册）都会红。
    expect(invoked).toContain('get_auto_lock');
    expect(registered).toContain('get_auto_lock');
    expect(invoked).toContain('set_auto_lock');
    expect(registered).toContain('set_auto_lock');
  });
});
