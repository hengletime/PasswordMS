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
/** D-68：Rust 侧 DTO 定义文件（`PairingDto` / `PairResultDto` / `DeviceDto` 都在这里）。 */
const dtoRs = Object.entries(rustModules).find(([path]) => path.endsWith('dto.rs'))?.[1] ?? '';

const typeModules = import.meta.glob('../types.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** TS 侧 DTO 定义文件。 */
const typesTs = typeModules['../types.ts'] ?? '';

/**
 * 取出 `export interface <name> { ... }` 的字段名集合（只取顶层一行一个字段的形态）。
 *
 * 本仓的 DTO 都是扁平结构（字段一行一个，JSDoc 注释单独成行），
 * 因此「行首两空格 + 标识符 + 可选 `?` + `:`」就是字段声明。
 */
function interfaceFields(source: string, name: string): string[] {
  const start = source.indexOf(`interface ${name} {`);
  if (start < 0) {
    return [];
  }
  const end = source.indexOf('\n}', start);
  const body = end < 0 ? source.slice(start) : source.slice(start, end);
  return [...body.matchAll(/^\s{2}([a-z][a-z0-9_]*)\??\s*:/gm)].map((m) => m[1]).sort();
}

/**
 * 取出 `pub struct <name> { ... }` 的字段名集合（`#[serde(...)]` 属性行被忽略）。
 *
 * `b64url` / `serde(default)` 之类的属性不影响字段名本身，故只匹配
 * 「行首四空格 + `pub` + 标识符 + `:`」。
 */
function structFields(source: string, name: string): string[] {
  const start = source.indexOf(`pub struct ${name} {`);
  if (start < 0) {
    return [];
  }
  const end = source.indexOf('\n}', start);
  const body = end < 0 ? source.slice(start) : source.slice(start, end);
  return [...body.matchAll(/^\s{4}pub ([a-z][a-z0-9_]*)\s*:/gm)].map((m) => m[1]).sort();
}

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

  it('keeps rename_paired_device wired on both sides (D-66 设备改名回归锚点)', () => {
    // D-66：设备改名是全链新命令（ipc.ts → commands/sync_cmd.rs → generate_handler!）。
    // 只在其中一侧改动就会变成运行期 `command not found`，界面表现为「改名点了没反应」。
    expect(invoked).toContain('rename_paired_device');
    expect(registered).toContain('rename_paired_device');
  });
});

// D-67 / D-69：指纹与会话码字段的**镜像契约**。
//
// 背景：这几个字段是前后端逐字比对的安全提示值。两侧只要有一处拼错 / 漏加，TypeScript 侧
// 读到的就是 `undefined` → `fingerprintText()` 返回空串、`hasSessionCode()` 判假 →
// 核对块「安静地」不渲染。编译器不会报错，界面只是少了一行，没人会发现。
// `src-tauri/src/dto.rs`（serde 序列化源）与 `src/types.ts`（反序列化目标）必须同时带
// 同一个字段名，因此用源码结构断言把两端绑在一起。
//
// D-69 变更：`PairingDto.fingerprint` → `session_code`；`PairResultDto.fingerprint` →
// `device_fingerprint` + `session_code`。后者是「删一个加两个」的形状 —— 最容易只改一侧
// 还两边都绿，所以下面额外给 `PairResultDto` 补了「字段集合完全相等」的断言。
describe('指纹 / 会话码字段前后端镜像契约（D-67 / D-69）', () => {
  it('读取到两侧 DTO 源码', () => {
    expect(dtoRs.length).toBeGreaterThan(1000);
    expect(typesTs.length).toBeGreaterThan(1000);
  });

  it('PairingDto 两侧都有 session_code（D-69 本次会话码）', () => {
    expect(structFields(dtoRs, 'PairingDto')).toContain('session_code');
    expect(interfaceFields(typesTs, 'PairingDto')).toContain('session_code');
    // D-69 负向：`fingerprint` 不得回到 PairingDto —— 电脑屏要显示的是本次会话码，
    // 不是长期设备指纹（两者混用会让两屏永远对不上，那正是 D-68 失败的原因）。
    expect(structFields(dtoRs, 'PairingDto')).not.toContain('fingerprint');
    expect(interfaceFields(typesTs, 'PairingDto')).not.toContain('fingerprint');
  });

  it('PairResultDto 两侧都有 device_fingerprint 与 session_code（D-69）', () => {
    // device_fingerprint：Host 的长期设备指纹（取自二维码 fp），供设备列表辨识。
    expect(structFields(dtoRs, 'PairResultDto')).toContain('device_fingerprint');
    expect(interfaceFields(typesTs, 'PairResultDto')).toContain('device_fingerprint');
    // session_code：本次会话码，与电脑屏显示的必须是同一个值。
    expect(structFields(dtoRs, 'PairResultDto')).toContain('session_code');
    expect(interfaceFields(typesTs, 'PairResultDto')).toContain('session_code');
    // 旧的单字段 `fingerprint` 已删除。注意 `toContain` 是精确元素比对，
    // 所以 `device_fingerprint` 不会让这条负向假红。
    expect(structFields(dtoRs, 'PairResultDto')).not.toContain('fingerprint');
    expect(interfaceFields(typesTs, 'PairResultDto')).not.toContain('fingerprint');
  });

  it('DeviceDto 两侧都有 fingerprint（D-67 设备行指纹）', () => {
    expect(structFields(dtoRs, 'DeviceDto')).toContain('fingerprint');
    expect(interfaceFields(typesTs, 'DeviceDto')).toContain('fingerprint');
  });

  it('两侧 PairingDto 字段集合完全一致（防止只加 Rust 不加 TS 的半截改动）', () => {
    const rust = structFields(dtoRs, 'PairingDto');
    const ts = interfaceFields(typesTs, 'PairingDto');
    expect(rust.length).toBeGreaterThan(0);
    expect(ts).toEqual(rust);
  });

  it('两侧 PairResultDto 字段集合完全一致（D-69「删一个加两个」的形状最容易只改一侧）', () => {
    // `PairResultDto` 本轮是「删 fingerprint、加 device_fingerprint + session_code」。
    // 只改 Rust 或只改 TS 时，`toContain` 那两条仍可能因另一侧恰好有同名字段而绿，
    // 只有集合相等能同时锁住「加了没」和「删了没」。
    const rust = structFields(dtoRs, 'PairResultDto');
    const ts = interfaceFields(typesTs, 'PairResultDto');
    expect(rust.length).toBeGreaterThan(0);
    expect(ts).toEqual(rust);
  });
});
