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
 * docs §14：**全树** Rust 源码（含 `commands/` 子目录）。
 *
 * ⚠️ 必须用 `**` 递归：命令都在 `src-tauri/src/commands/*.rs`，只写 `*.rs` 会漏掉全部命令，
 * 得到一个 `0 === 0` 的假绿 —— 这正是 §13 讨论过的「跑了个空」。
 */
const rustTree = import.meta.glob('../../src-tauri/src/**/*.rs', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

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

  it('keeps save_import_template wired on both sides (D-72 CSV 模板下载回归锚点)', () => {
    // D-72：模板下载是同一形状的全链新命令（ipc.ts → commands/import_cmd.rs →
    // generate_handler!）。两侧缺一侧 → 运行期 `command not found`，界面表现为
    // 「点下载没反应」；而 Android 上还叠加一层「取消走 reject」，会被 catch 吞掉，
    // 于是连错误提示都不会出现 —— 这正是必须用结构锚点钉死、不能只靠人点一次的原因。
    expect(invoked).toContain('save_import_template');
    expect(registered).toContain('save_import_template');
  });

  it('keeps vault_compaction_stats / compact_vault wired on both sides (D-76 体积构成与压缩回归锚点)', () => {
    // D-76：这两条命令由 Rust 侧先落（`system_cmd.rs`），前端接线晚一步 —— 那一步的窗口期里
    // 「命令已注册但零调用」正是 D-59 的形态（后端就绪、前端不可达）。两侧都断言，
    // 任一方向漏掉都会红：只注册不调用 = 功能不可达；只调用不注册 = 运行期 `command not found`。
    expect(invoked).toContain('vault_compaction_stats');
    expect(registered).toContain('vault_compaction_stats');
    expect(invoked).toContain('compact_vault');
    expect(registered).toContain('compact_vault');
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

// D-76：体积构成 / 压缩两个 DTO 的镜像契约。
//
// 与 D-67 / D-69 那组同一个理由：字段名拼错时 TypeScript 读到 `undefined` → 界面显示
// 「0 B」或「可回收 0 B」，**看起来正常、其实是错的**（比崩溃更难查）。
// `CompactionStatsDto` 有 11 个字段、其中 4 个是「相近但不相同」的字节数
// （`tombstone_bytes` / `kept_tombstone_bytes` / `attachment_dead_bytes` / `attachment_live_bytes`），
// 正是最容易只改一侧、且两侧仍各自能编译的形状 → 用**字段集合完全相等**锁死。
describe('体积构成 / 压缩 DTO 的前后端镜像契约（D-76）', () => {
  it('CompactionStatsDto 两侧字段集合完全一致（11 个）', () => {
    const rust = structFields(dtoRs, 'CompactionStatsDto');
    const ts = interfaceFields(typesTs, 'CompactionStatsDto');
    expect(rust.length).toBe(11);
    expect(ts).toEqual(rust);
  });

  it('CompactionResultDto 两侧字段集合完全一致（4 个）', () => {
    const rust = structFields(dtoRs, 'CompactionResultDto');
    const ts = interfaceFields(typesTs, 'CompactionResultDto');
    expect(rust.length).toBe(4);
    expect(ts).toEqual(rust);
  });
});

// docs §14：`rename_all = "snake_case"` 必须 **100% 覆盖** —— 这是一条不变量，不是约定。
//
// 为什么需要机器守门：本仓 `tauri::command` 全部带该属性（当前 53/53），所以任何人 30 秒
// 就能判定「新命令该怎么写」，不必逐条读。一旦有人漏写一个，这条判断就退化为「必须逐条核」
// —— **不变量被打破的成本不是那一处，而是所有人的每次判断**。
//
// 漏写的失败形态极难查（§14.3）：wire 键变回 camelCase → 前端 `invalid args` → 若调用点
// 外层还包了取消归一（`save-dialog.ts` 就是），用户看到的是**点了没反应、无堆栈、无日志**。
// 名称集合锚点（上面的 `invoked` / `registered`）抓不到这个：命令名两侧都有，只是参数对不上。
describe('docs §14：`rename_all = "snake_case"` 必须 100% 覆盖', () => {
  it('扫描面覆盖 commands/ 子目录（glob 写错会退化成假绿）', () => {
    const paths = Object.keys(rustTree);
    expect(paths.length).toBeGreaterThan(10);
    expect(paths.some((p) => p.includes('/commands/'))).toBe(true);
    expect(paths.some((p) => p.endsWith('/lib.rs'))).toBe(true);
  });

  it('所有 tauri::command 都带 rename_all = "snake_case"（53/53）', () => {
    const all = Object.values(rustTree).join('\n');
    const total = (all.match(/tauri::command/g) ?? []).length;
    const withAttr = (all.match(/tauri::command\(rename_all = "snake_case"\)/g) ?? []).length;
    // 防空扫描假绿：`rustTree` 的 glob 若漏了 `**`（只匹配 `src-tauri/src/*.rs`），
    // commands/ 子目录一个都扫不到，total 会是 0，而 0 === 0 会静默通过。
    expect(total).toBeGreaterThan(40);
    // 死数字是有意的：新增命令时必须来改这两个数，等于强制作者看一眼这条不变量。
    expect(total).toBe(53);
    expect(withAttr).toBe(53);
  });
});
