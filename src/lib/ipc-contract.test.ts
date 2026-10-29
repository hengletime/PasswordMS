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

  it('keeps the six #531 ordering commands wired on both sides', () => {
    // #531：排序模型 = 钉住块（pin_order）→ 未钉按频度。六条全链新命令，前端先落、
    // 后端（kodouma）并行注册 —— 后端落地前 `registered` 少这六条是**约定的红**
    // （报告里如实标注，等后端落地后复跑须全绿）。任一侧漏一条 = 运行期 `command not found`。
    for (const name of [
      'record_item_usage',
      'set_item_pin',
      'reorder_pinned_items',
      'set_folder_pin',
      'reorder_pinned_folders',
      'move_item_to_folder',
    ]) {
      expect(invoked, `前端 invoke 缺 ${name}`).toContain(name);
      expect(registered, `后端 generate_handler! 缺 ${name}`).toContain(name);
    }
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

  it('所有 tauri::command 都带 rename_all = "snake_case"（D-114 起为 63/63）', () => {
    const all = Object.values(rustTree).join('\n');
    const total = (all.match(/tauri::command/g) ?? []).length;
    // 不写成 `\(rename_all = "snake_case"\)` 精确匹配：§23 起有 5 个命令带 `, async`
    // （Android 侧 `run_mobile_plugin` 会阻塞，命令必须在同步线程池里跑，不能落在主线程）。
    // 这条不变量的本体是「**每个** command 都声明了 snake_case」，不是「参数列表只有一项」。
    const withAttr = (all.match(/tauri::command\([^)]*rename_all = "snake_case"[^)]*\)/g) ?? [])
      .length;
    // 防空扫描假绿：`rustTree` 的 glob 若漏了 `**`（只匹配 `src-tauri/src/*.rs`），
    // commands/ 子目录一个都扫不到，total 会是 0，而 0 === 0 会静默通过。
    expect(total).toBeGreaterThan(40);
    // 死数字是有意的：新增命令时必须来改这两个数，等于强制作者看一眼这条不变量。
    // #531：53 → 59（record_item_usage / set_item_pin / reorder_pinned_items /
    // set_folder_pin / reorder_pinned_folders / move_item_to_folder）。
    // D-108（豆包审查 3.5-7）：删除前后端占位死命令 sync_accept_pair，59 → 58。
    // D-113 P2（docs/08 生物识别）：+4（biometric_status / _enable / _disable / _unlock），58 → 62。
    // D-114（快速解锁引导）：+1（open_biometric_settings，打开系统「登录选项」），62 → 63。
    expect(total).toBe(63);
    expect(withAttr).toBe(63);
  });
});

// ── VK 不跨 JS 边界（`docs/08` §18.5 约束 6 · `docs/03` §13.10 第 3 条） ─────────────
//
// 2026-10-29 核实的事实，先写清楚**什么不成立**：
//
//   `Repository::vault_key()` 是 `pub`，命令层**实际调用**了三处（`biometric_cmd` 取 VK
//   去封印、`sync_cmd` 取 VK 包身份私钥 / 比对 `adopt`）。所以「命令层根本拿不到 VK」
//   **不是事实**，不能拿它当不变量——写成断言只会是一条假绿。
//
// 真正成立的是**类型层面不可序列化**：`VaultKey = Zeroizing<[u8; 32]>`，而 zeroize 里
// `impl<Z> Serialize for Zeroizing<Z>` 带 `#[cfg(feature = "serde")]` 门控；那个 feature
// 来自 zeroize 的 **optional dependency**，隐式生成、默认不开。本项目只开 `derive`
// ⇒ VK 物理上过不了 serde，也就过不了 Tauri IPC。
//
// 但这层屏障只需在 Cargo.toml 里加一个词就消失，且消失时**全仓一片绿、功能毫无变化**
// —— 正是必须机器守门的形态。三条断言对应三种真实翻车路径：
//
//   ① 有人为了「顺手打个日志」给 zeroize 开 serde（物理屏障失效）；
//   ② 有人新建一个带 `vk` 字段的 DTO（类型换成 String，屏障不适用）；
//   ③ 有人让命令直接回 `Vec<u8>` / `Zeroizing`（绕过 DTO 层）。
//
// 已知局限（如实登记）：本条只看 Cargo.toml 里的 `zeroize` 声明行。若有人把 features
// 拆成多行 TOML 写法，行匹配会漏——所以文档里同时要求 code review 关注 Cargo.toml 改动，
// 不能只靠这条测试。
describe('VK 不跨 JS 边界（§18.5 约束 6 · §13.10 第 3 条）', () => {
  const cargos = import.meta.glob(
    ['../../Cargo.toml', '../../crates/passwordms-core/Cargo.toml'],
    { query: '?raw', import: 'default', eager: true },
  ) as Record<string, string>;

  /** 密钥材料的字段名（精确匹配，不做子串——否则 `device_fingerprint` 之类会误伤）。 */
  const FORBIDDEN_FIELDS = [
    'vk',
    'sealed_vk',
    'vault_key',
    'master_key',
    'master_password',
    'kek',
    'verify_key',
    'unwrapped_key',
    'bind',
  ];

  it('读取到根与 core 的 Cargo.toml（glob 写错会让下面的断言空转）', () => {
    expect(Object.keys(cargos).length).toBe(2);
    expect((cargos['../../Cargo.toml'] ?? '').length).toBeGreaterThan(500);
    expect((cargos['../../crates/passwordms-core/Cargo.toml'] ?? '').length).toBeGreaterThan(100);
  });

  it('zeroize 不得启用 serde feature（否则 `Zeroizing` 可序列化，VK 失去物理屏障）', () => {
    const decls = Object.entries(cargos).flatMap(([path, text]) =>
      text
        .split('\n')
        .filter((l) => /^\s*zeroize\s*[.=]/.test(l) && !l.trim().startsWith('#'))
        .map((l) => `${path} → ${l.trim()}`),
    );
    // 防空扫：一处都没扫到 = 正则写错了，后面的循环零次执行会静默通过。
    expect(decls.length).toBeGreaterThan(0);
    for (const decl of decls) {
      expect(decl, `zeroize 声明不得含 serde：${decl}`).not.toMatch(/\bserde\b/);
    }
  });

  it('Rust DTO 里没有密钥材料字段（`BiometricStatusDto` 的设计前提）', () => {
    // `    pub <name>:` 在本仓只出现在 struct 定义内（impl 块不会这么写）。
    const fields = [...dtoRs.matchAll(/^\s{4}pub ([a-z][a-z0-9_]*)\s*:/gm)].map((m) => m[1]);
    // 防空扫 + 保证扫描面确实覆盖了 DTO 文件。
    expect(fields.length).toBeGreaterThan(40);
    for (const bad of FORBIDDEN_FIELDS) {
      expect(fields, `dto.rs 出现了密钥字段 ${bad}`).not.toContain(bad);
    }
  });

  it('TS 侧 DTO 里同样没有密钥材料字段（前端拿到也等于已经泄漏）', () => {
    const fields = [...typesTs.matchAll(/^\s{2}([a-z][a-z0-9_]*)\??\s*:/gm)].map((m) => m[1]);
    expect(fields.length).toBeGreaterThan(40);
    for (const bad of FORBIDDEN_FIELDS) {
      expect(fields, `types.ts 出现了密钥字段 ${bad}`).not.toContain(bad);
    }
  });

  it('`BiometricStatusDto` 字段数恒为 5（它是最容易长出密钥字段的那个 DTO）', () => {
    // 这个 DTO 直接围绕封印/VK 打转，加字段的诱因最强；字段数一变就该有人来解释。
    expect(structFields(dtoRs, 'BiometricStatusDto')).toHaveLength(5);
    expect(interfaceFields(typesTs, 'BiometricStatusDto')).toHaveLength(5);
  });

  it('没有任何命令把裸二进制或密钥容器交给 IPC', () => {
    const all = Object.values(rustTree).join('\n');
    // VK 是 32 字节数组，最可能的泄露形态就是「序列化成字节数组直接回」。
    expect(all).not.toMatch(/IpcResult<\s*Vec<u8>\s*>/);
    expect(all).not.toMatch(/IpcResult<\s*\[u8/);
    expect(all).not.toMatch(/IpcResult<\s*Zeroizing/);
    expect(all).not.toMatch(/IpcResult<\s*VaultKey/);
  });

  it('命令层直接取 VK 的调用点恒为 2 处（新增第 3 处必须有人来解释）', () => {
    // 完整收窄（把 `vault_key()` 降为 `pub(crate)`）的成本核算在 `docs/08` §35.5：
    // 32 处改动（3 处生产 + 29 处集成测试）+ 要把身份私钥的读包逻辑搬进 core，
    // 而收益只是「防未来误用」（VK 本来就在进程内存里，拦不住真攻击者）⇒ 不排期。
    //
    // 这条白名单用近乎零的成本拿主要收益：新增第 3 处调用必须显式改这个数组，
    // 等于强制作者当场回答「这里为什么也需要裸 VK」。
    const sites = Object.entries(rustTree)
      .filter(([p]) => p.includes('/commands/'))
      .flatMap(([path, text]) =>
        text
          .split('\n')
          // 注释里的示例不算调用点（`///` 与 `//` 都排除）。
          .filter((l) => !l.trim().startsWith('//'))
          .filter((l) => /\.vault_key\(\)/.test(l))
          .map(() => path.split(/[\\/]/).pop() ?? path),
      )
      .sort();
    expect(sites).toEqual(['biometric_cmd.rs', 'sync_cmd.rs']);
  });
});
