# PasswordMS · 系统架构设计与任务分解

- **项目代号**：PasswordMS
- **文档版本**：v1.0
- **上游依据**：`docs/00-scope.md`（需求冻结书）、`docs/01-prd.md`（PRD）、`docs/01-security-req.md`（安全需求）
- **负责人**：高见远（架构师）
- **状态**：待评审
- **修订记录**：§2.6 帧表曾长期滞后于实现（D-1 的 `wrapped_key` 未回写文档），2026-10-22 已与 `frame.rs` 对齐；各节修订以正文内的 `D-nn` 标注为准。
- **设计基线**：四项冻结决策（D1 Tauri 2 / D2 二维码配对直连 / D3 P0+P1 / D4 附件+笔记都要）**只细化、不推翻**

> **裁定说明**：PRD §8 五项待确认问题（Q1~Q5）已由主理人代为裁定，本文按裁定结论设计，并在对应章节标注 **【已裁定】**。

---

## 目录

- [1. 实现方案与框架选型](#1-实现方案与框架选型)
- [2. 加密方案](#2-加密方案)
- [3. 局域网同步协议](#3-局域网同步协议)（含 §3.12 附件 blob 同步·缺陷 D-53 增量设计）
- [4. 文件列表及相对路径](#4-文件列表及相对路径)
- [5. 数据结构与接口](#5-数据结构与接口)
- [6. 程序调用流程](#6-程序调用流程)
- [7. 任务列表](#7-任务列表)
- [8. 依赖包列表](#8-依赖包列表)
- [9. 共享知识](#9-共享知识)
- [10. 待明确事项](#10-待明确事项)
- [附录 A：安全验收点（A-01~A-10）对照表](#附录-a安全验收点对照表)

---

## 1. 实现方案与框架选型

### 1.1 核心技术难点

| # | 难点 | 本质 | 应对策略 |
|---|------|------|---------|
| N1 | **一体双端** | Windows 与 Android 共用一套逻辑与 UI | Rust workspace：`passwordms-core`（纯逻辑，零 Tauri 依赖）+ Tauri 外壳；前端 Svelte 单套代码，平台差异用 `platform()` 运行时判定 |
| N2 | **体积硬约束** | Win ≤ 10 MB / APK ≤ 12 MB | 复用系统 WebView（不打包内核）；Rust 依赖白名单；拒绝重型 UI 库 / 图标字体 / TLS 栈 / 数据库引擎 |
| N3 | **零自研密码学** | 算法与参数可审计、可留档 | 全量使用 RustCrypto 成熟 crate（argon2 / chacha20poly1305 / hkdf / sha2 / hmac / x25519-dalek） |
| N4 | **逐条加密 + 少重写** | 改一条不重写全库 | 记录帧化容器（见 §2.6）：每条独立 nonce/IPK；未变记录复用密文，仅重写变更帧 |
| N5 | **零知识局域网同步** | 嗅探/重放/中间人/未配对接入防护 | 应用层 E2E（XChaCha20-Poly1305）+ X25519 密钥协商 + 指纹与密钥确认 + 序号/时间窗防重放 |
| N6 | **冲突可见可仲裁** | 不得静默覆盖 | 条目级 LWW 探测「双改」，命中即入「冲突箱」，UI 三选一 |

### 1.2 选型结论（含版本）

| 层 | 选型 | 版本 | 理由 |
|----|------|------|------|
| 桌面/移动外壳 | **Tauri** | **2.x**（基线 `2.1`） | 冻结 D1；复用系统 WebView2（Win）/ Android System WebView，天然满足体积约束；官方支持 Android 目标 |
| 前端框架 | **Svelte** | **5.x**（`^5.0`，启用 runes） | 编译期产出、无运行时框架开销，产物体积远小于 React/Vue；响应式 store 契合本地状态管理 |
| 构建工具 | **Vite** | **6.x** | Svelte 官方配套；ES 构建产物小、可 tree-shake |
| 语言（前端） | **TypeScript** | **5.x** | 与 Rust DTO 对齐，编译期校验 IPC 契约 |
| 语言（后端） | **Rust** | **edition 2021** | 内存安全 + 单文件静态链接 + 跨端复用；`edition 2021` 稳定且无额外成本 |
| 异步运行时 | **tokio**（经 `tauri::async_runtime`） | 随 Tauri | 复用 Tauri 内置运行时，不额外引入 async-std/smol |
| 同步传输 | **原生 TCP + 自研长度前缀帧** | `std`/`tokio` | LAN 直连 + 应用层 E2E，**无需 TLS/HTTP 栈**（省 1~2 MB，也未必要） |

> **不使用清单（会导致体积超标）**：Electron（禁用）；Ant Design / MUI / Element 等重型 UI 库（禁用）；图标字体包 Font Awesome/Material Icons（改用内联 SVG）；`image` / `opencv`（改为无图像编解码依赖，QR 生成用 `qrcode` 纯矢量、QR 识别放前端）；`rustls`/`native-tls`/`tokio-tungstenite`（LAN 直连不需要 TLS）；`sqlx`/`rusqlite`/`diesel`（改用自研记录帧容器，零数据库引擎）；`chrono`/`time`（时间戳直接用 `SystemTime` 转 `i64` UTC 毫秒）。

### 1.3 体积核算表

> **⚠️ 本节全部数字均为设计期「预估/估算」，不是实测值。**
> 表中 ✅ 仅表示"估算值落在红线内"，**不等于体积红线已通过**。真实体积须由 `scripts/build.*` 产出安装包后，用 `scripts/check-size.*` 实测（该门禁已修为"产物缺失即失败"）。
> **截至 2026-10-13：体积红线 `未验证`** —— Tauri 打包在本机尚未跑通（见 `docs/03b-verification.md` §7、`docs/04-qa.md`）。

#### Windows 安装包（NSIS/MSI，目标 ≤ 10 MB）

| 构成项 | 预估体积 | 说明 |
|--------|---------|------|
| Tauri 运行时（`tauri`+`tao`+`wry`） | 2.4 – 3.2 MB | 静态链接进 exe |
| WebView2 引导/加载器 | 0 – 1.5 MB | 采用 **evergreen 系统运行时**（Win10/11 预装）可趋近 0；兜底 bootstrapper ≈1.5 MB |
| Rust 核心 + 加密依赖（静态） | 0.9 – 1.4 MB | argon2/chacha20poly1305/hkdf/sha2/hmac/x25519-dalek/uuid/base64/qrcode |
| 前端产物（gzip 后内嵌） | 0.15 – 0.3 MB | Svelte 编译产物 + jsQR |
| 图标/资源 | < 0.1 MB | 内联 SVG，ICO/PNG 各一张 |
| **合计** | **≈ 3.5 – 5.2 MB** | **预估 ≤ 10 MB（估算，未实测）** |

#### Android APK（目标 ≤ 12 MB）

| 构成项 | 预估体积 | 说明 |
|--------|---------|------|
| Rust `.so`（arm64-v8a，strip 后） | 2.8 – 4.0 MB | 仅 arm64 + armeabi-v7a 双 ABI，按需裁剪 |
| AndroidX / Tauri Android glue | 2.0 – 3.0 MB | WebView 容器、Activity、权限 glue |
| 前端产物（assets 内嵌） | 0.2 – 0.4 MB | 同上 |
| 资源/图标/清单 | 0.4 – 0.8 MB | 多密度图标 + manifest |
| **合计** | **≈ 5.4 – 8.2 MB** | **预估 ≤ 12 MB（估算，未实测）** |

> **结论（估算口径）**：按设计期估算，选型组合在两端均有 ≥ 45% 体积余量。**该结论未经实测**，须以 `scripts/check-size.*` 对真实产物的核验结果为准。**唯一风险点是 WebView2 兜底引导器**——交付时默认依赖系统 evergreen 运行时，估算安装包 < 6 MB。

### 1.4 双端代码组织策略

```
PasswordMS/                       # Cargo workspace 根
├── Cargo.toml                    # [workspace] members = ["crates/passwordms-core", "src-tauri"]
├── package.json                  # 前端（Svelte/Vite）—— 两端共用同一份
├── vite.config.ts
├── index.html
├── src/                          # 前端源码（两端共用，平台差异运行时判定）
├── crates/
│   └── passwordms-core/          # ★ 纯 Rust 逻辑核心：加密/容器/同步/导入/体检/TOTP
│                                 #   零 Tauri 依赖，可单测、可被任意外壳复用
└── src-tauri/                    # Tauri 外壳（桌面 + Android 共用）
    ├── src/                      # Rust 命令层：把 core 暴露为 #[tauri::command]
    ├── tauri.conf.json           # 含 bundle 目标（nsis/msi + android）
    └── gen/android/              # Tauri 生成的 Android 工程（apk/aab）
```

**Rust 核心复用**：`passwordms-core` 是纯库 crate，不依赖 `tauri`；桌面端与 Android 端由**同一个 `src-tauri`** 编译（Tauri 2 支持同一工程 `tauri android build` / `tauri build`）。所有业务逻辑下沉到 core，`src-tauri` 只做「参数解包 → 调 core → 错误映射」的薄命令层。

**前端复用**：`src/` 单套代码；布局差异通过 CSS 断点 + `platform()`（`@tauri-apps/api/os` 或 Tauri 2 `platform()`）运行时判定（桌面三栏 / 手机单栏+抽屉）；平台专属能力（如 Android 相机扫码）通过能力探测降级（无摄像头 → 手动录入）。

---

## 2. 加密方案

> 全程落实安全需求 §2 与冻结书 §5；对应验收点 **A-01 / A-02 / A-04 / A-05**。

### 2.1 密钥层级（不可跳级）

```mermaid
flowchart TD
  MP["主密码 MP<br/>仅存内存·绝不落盘"] -->|Argon2id| MK["主密钥 MK (32B)"]
  MK -->|HKDF-SHA256 派生 KEK| KEK["密钥加密密钥 KEK (32B)"]
  KEK -->|XChaCha20-Poly1305 包裹| VKW["wrapped_vault_key (存于文件头)"]
  VKW -->|解包| VK["保险库密钥 VK (32B)"]
  VK -->|逐条包裹| IK["条目密钥 IK (32B·每条独立)"]
  IK -->|AEAD| CT["条目密文 / 附件密文 blob"]
  VK -->|AEAD| BLOB["附件密文 blob"]
```

| 层级 | 作用 | 存储形式 |
|------|------|---------|
| **MP 主密码** | 唯一用户输入 | **仅内存**；不写盘、不写日志、不写崩溃转储 |
| **MK 主密钥** | KDF 输出，输入到 HKDF | **仅内存**；解锁后立刻经 HKDF 派生 KEK，MK 不再留存 |
| **KEK 密钥加密密钥** | 专门用于包裹/解包 VK，便于改主密码时只重包裹 | **仅内存**；`Zeroizing<[u8;32]>` |
| **VK 保险库密钥** | 包裹全部 IK，是数据加密的根 | 以 `wrapped_vault_key`（KEK 加密）形式**存于文件头明文区** |
| **IK 条目密钥** | 每条目独立，包裹在该条目记录内 | 以 `wrapped_item_key`（VK 加密）随条目记录存储 |

**改主密码（O-08）**：仅用新 KEK 重新包裹**同一个 VK**，不重写任何条目密文。

### 2.2 KDF 算法与具体参数

| 项 | 取值 | 说明 |
|----|------|------|
| 算法 | **Argon2id**（v1.3，`argon2` crate） | 内存硬，抗 GPU/ASIC |
| 内存开销 `m` | **64 MiB**（65536 KiB） | 权衡手机端可接受性 |
| 迭代次数 `t` | **3** | 单次派生约 3 趟内存填充 |
| 并行度 `p` | **1** | 低端机稳定、跨端可复现（参数随库存储） |
| 盐长度 | **16 字节**（每库唯一，CSPRNG） | 独立于任何其它派生盐 |
| 输出长度 | **32 字节**（→ MK） | 直接作为 HKDF 输入 |
| 参数存储 | 写入 `header_json.kdf` | 便于未来演进；解锁**按文件内参数**执行 |

**低端 Android 预估**：以 4×Cortex-A53 @1.4 GHz 为参照，Argon2id(64 MiB, t=3, p=1) 单次 ≈ **0.8 – 1.6 s**，满足「解锁 ≤ 3 s」（G2）且显著抬高离线爆破成本。**iOS/中端机** ≈ 0.4 – 0.7 s。

**HKDF 派生标签（`info` 常量）**：

| 派生 | info 字符串 | 输出 |
|------|------------|------|
| KEK | `"passwordms/v1/kek"` | 32B，包裹 VK |
| 密码校验键 | `"passwordms/v1/verify"` | 32B，见 §2.5 |

### 2.3 对称加密

| 项 | 取值 |
|----|------|
| 算法 | **XChaCha20-Poly1305**（AEAD，`chacha20poly1305` crate） |
| 密钥长度 | **256 bit（32 字节）** |
| Nonce | **24 字节（192 bit）随机**，每次加密由 CSPRNG 生成，与密文同存、不保密；192-bit 随机空间使 nonce 复用概率可忽略 |
| Tag | Poly1305，**16 字节**，附于密文尾 |
| AAD | 绑定记录身份，防密文移植/替换。**条目/记录 AAD = `record_type(1B) ‖ record_id(16B)`**；根层 VK 包裹 AAD = 常量 `"passwordms/v1/vault-key"`；同步会话内层 AAD = `session_id ‖ seq(8B) ‖ ts(8B)` |
| 完整性 | 解密失败（tag 校验失败）一律映射为统一错误（§2.5） |

**包裹 VK/IK 的实现**：复用同一 AEAD（XChaCha20-Poly1305）做密钥包裹（key wrap）：`wrapped = AEAD(key=KEK/VK, nonce=rand24, pt=rawkey32, aad=...)`。

### 2.4 随机数来源

- **统一使用 `getrandom` crate**（`getrandom = "0.2"`），底层为 OS CSPRNG：Windows `BCryptGenRandom`、Android `getrandom(2)` 系统调用。**禁止**在密码学用途使用任何用户态可预测 PRNG。
- 覆盖范围：KDF 盐、VK/IK、AEAD nonce、配对令牌、X25519 临时私钥、会话 ID、`sync_state` 随机盐。

### 2.5 密钥验证方式（不泄露信息）

- 解锁 = **用 KEK 解包 `wrapped_vault_key`**。AEAD tag 校验通过 ⇒ 主密码正确且数据完整；失败 ⇒ 抛**统一错误 `ERR_UNLOCK_FAILED`**。
- **不区分**「主密码错误」与「数据损坏/被篡改」，文案与错误码完全一致（落实 A-05、冻结书 §5.6）。
- 另存 `verify_mac = HMAC-SHA256(derive(verify_key), "passwordms/v1/verify")` 用于**快速负向判定**：MAC 不匹配直接判失败。为避免侧信道，`verify_mac` 校验与 VK 解包**均返回同一错误码**；实现上用 `subtle::ConstantTimeEq` 做常量时间比较。
- 主密钥/密码在验证后立即 `zeroize`，不留存。

### 2.6 保险库文件格式（单文件容器）

**文件名**：`passwordms.vault`（逻辑单文件；扩展名自定义）。

#### 顶层二进制布局

| 偏移 | 长度 | 字段 | 类型/说明 |
|------|------|------|----------|
| `0x00` | 8 | `magic` | ASCII 固定 `"PMSVAULT"` |
| `0x08` | 2 | `format_version` | `u16` 小端，当前 = **1** |
| `0x0A` | 2 | `header_len` | `u16` 小端，**头部区容量**（= `header_json` 尾部空格补齐后的字节数，**不是** JSON 实长） |
| `0x0C` | 4 | `reserved` | 全 0（对齐/扩展） |
| `0x10` | `header_len` | `header_json` | UTF-8 JSON + 尾部空格填充，**明文元数据**（不含任何凭据） |
| `0x10+header_len` | 变长 | `record_area` | 记录帧序列（每条独立 AEAD 密文） |

- **头部区容量语义**：头部区**固定容量、尾部空格填充**（A-02 要求计数更新可原位移位写入），`header_len` 记的是容量，`Container::open` 直接把读到的值当 `header_capacity`。新建 = `HEADER_RESERVED_BYTES`(**8192**)；写不下时整文件重写并**翻倍**（8192 → 16384 → 32768 → 65535）。
- **容量硬上限 65535（D-78）**：`header_len` 是 u16，翻倍必须夹在 `MAX_HEADER_BYTES`(= `u16::MAX`) 内；夹不住**返回 `ERR_VAULT_CAPACITY_EXCEEDED` 且磁盘一字不动**，绝不用夹住后的容量继续写。历史实现此处无夹取：`65536 as u16 == 0` → `header_len` 写成 0 → `open` 判 `zero header` → **库永久打不开**；且进程内 `header_capacity` 仍是正确的 65536，故**本次运行毫无异常，重启/重新解锁才暴露**。实测主人库头部 JSON 实长 808 B / 容量 8192，到达危险窗口需连踩四次区间（JSON 8193..16384 → 16385..32768 → 32769..65536），一次性跳跃会被 `serialize_header_padded` 的 `?` 挡在 Err（安全），故概率极低、后果不可回退。

#### `header_json`（明文，禁止含凭据）

| 字段 | 类型 | 说明 |
|------|------|------|
| `vault_id` | string(uuid) | 库唯一标识 |
| `format_version` | int | 冗余校验，=1 |
| `kdf` | object | `{algorithm:"argon2id", memory_kib, iterations, parallelism, salt_len}` |
| `kdf_salt` | string(base64) | 16B 盐 |
| `wrapped_vault_key` | object | `{nonce(base64 24B), ct(base64 48B=32+16)}` |
| `verify_mac` | string(base64) | 32B HMAC，含 `hmac_salt` |
| `encryption_scheme` | string | `"xchacha20poly1305"` |
| `item_count` / `folder_count` | int | 计数（不含回收站） |
| `created_at` / `updated_at` | int | UTC 毫秒 |
| `auto_lock_minutes` | int | 1/5/15，默认 5 |

#### 记录帧 `record`（`record_area` 内逐帧）

| 偏移 | 长度 | 字段 | 说明 |
|------|------|------|------|
| `0` | 1 | `record_type` | `0x01` Folder / `0x02` Item / `0x03` Trash / `0x04` Conflict / `0x05` Settings / `0x06` Device / `0x07` SyncState |
| `1` | 1 | `flags` | bit0 = tombstone（墓碑，已删除占位）／ bit1 = blob（附件密文帧） |
| `2` | 2 | `reserved` | 全 0 |
| `4` | 16 | `record_id` | UUID 原始 16 字节（条目/分组 ID） |
| `20` | 24 | `wrapped_key.nonce` | 该条目 IK 的包裹 nonce |
| `44` | 48 | `wrapped_key.ct` | 32B IK + 16B tag |
| `92` | 4 | `payload_len` | `u32` 小端 = `24 + ct_len`（**含** `[100..124)` 那段 nonce） |
| `96` | 4 | `plaintext_len` | `u32` 小端（容量统计用，不含 tag） |
| `100` | 24 | `nonce` | XChaCha20-Poly1305 nonce |
| `124` | `ct_len` | `ct` | 密文 + 16B tag，`ct_len = plaintext_len + 16` |

- **固定帧头 `FRAME_HEADER_LEN` = 124 B**。
- **单帧总长 = `124 + ct_len` = `100 + payload_len`**。`payload_len` 已含 24 字节 nonce（nonce 在帧头 `[100..124)`，不在 `ct` 里），所以**不能**用 `124 + payload_len` 算帧长——那会把 nonce 算两次。
- **表已与实现对齐（关闭 D-1）**：原表的帧头只有 52 B、未含 `wrapped_key`，与「IK 由 VK 包裹、随条目记录存储」（§2.1／§3.6）自相矛盾。实现从一开始就把 `wrapped_key`（nonce 24B + ct 48B）纳入帧头，并在 `frame.rs` 顶部标注「与架构文档的偏差（D-1）」；自本版起本节即为真值，偏差关闭。
- **AAD** = `record_type(1B) ‖ record_id(16B)`。
- **更新语义（满足 A-02）**：修改单条 → 仅重写该帧；若新密文长度不变则**原地 pwrite**，否则将旧帧置墓碑（`flags.bit0=1`）并在尾部追加新帧。**未变记录帧的 nonce 与密文完全复用，字节级零差异**，故「改 1 条 → 文件字节变更量 ≈ 单条」。提供「压缩保险库」维护操作清理墓碑与空洞。
- **记录内容**：条目/分组/回收站/冲突记录的**明文头**仅含 `record_id`（在帧头里）与极少量元信息；**全部敏感字段（标题、网址、用户名、密码、备注、TOTP、自定义字段、正文、附件元信息、分组名/图标/颜色、回收站快照）均在 `ct` 内**。
- **附件 blob**：作为 `0x02 Item`（`attachment` 型）的密文负载之一，或独立大 blob 帧（`plaintext_len` 达 20 MB 级），由条目记录通过 `record_id` 引用。

### 2.7 敏感内存处理（Rust zeroize）

| 载体 | 类型 | 处理 |
|------|------|------|
| MP / MK / KEK / VK / IK / TOTP 密钥 | `zeroize::Zeroizing<[u8;32]>` 或 `#[derive(Zeroize, ZeroizeOnDrop)]` 结构 | 离开作用域或显式 `.zeroize()` 时**覆写为 0** |
| 解密后的明文字段（密码、正文） | `Zeroizing<String>` / `SecretString` | 界面回传后 Rust 侧立即清零；前端 DTO 短生命周期 |
| 会话容器 | `Session` 持有上述全部，`Drop` 时级联清零 | 锁定时 `*state = Session::Locked`，触发 Drop |
| 剪贴板 | 由 `tauri-plugin-clipboard-manager` 写；Rust 侧不常驻 | 前端 30s 清空（§9） |

- **锁定清理（A-04）**：`lock` 命令将 `AppState.session` 替换为 `Locked`，旧 `Session` 立即 Drop → 全部 `Zeroizing` 缓冲区清零；同时清空前端 store 中的明文，丢弃未保存编辑（提示）。
- **禁止**：依赖「作用域结束即回收」；崩溃转储/日志/临时文件不得出现敏感数据（A-09）。

---

## 3. 局域网同步协议

> 落实安全需求 §4（S1~S8）与验收点 **A-06 / A-07 / A-08**；对应冻结决策 D2。

### 3.1 角色与总览

- **Host（监听端）**：生成一次性令牌 + 展示二维码，绑定 `0.0.0.0:<port>` 监听。
- **Joiner（扫码端）**：扫描二维码获得 `IP+端口+令牌+指纹`，主动发起 TCP 连接。
- 两端角色**可互换**（Windows 或手机都能当 Host）。

### 3.2 配对流程时序（含密钥协商与确认）

```mermaid
sequenceDiagram
  autonumber
  participant H as Host(监听端)
  participant J as Joiner(扫码端)
  H->>H: 生成会话: 临时X25519密钥对 + 一次性令牌 + session_salt
  H->>H: 绑定端口并展示二维码(IP/端口/令牌/指纹/exp)
  J->>J: 扫码解析 QRPayload
  J->>J: 校验 exp 未过期, 生成临时X25519密钥对
  J->>H: HELLO{token, joiner_id, joiner_pub, v}
  H->>H: 校验令牌: 未使用 & 未过期 & 匹配
  alt 校验通过
    H-->>J: ACCEPT{host_id, host_pub, session_id, session_salt, v}
    Note over H,J: 双方 shared = X25519(priv_eph, peer_pub)<br/>prk = HKDF-Extract(salt, shared)<br/>sync_key/confirm_key = HKDF-Expand(...)
    H->>H: mac_h = HMAC(confirm_key, "server"||transcript)
    J->>J: mac_j = HMAC(confirm_key, "client"||transcript)
    J->>H: CONFIRM{mac_j}
    H->>J: CONFIRM{mac_h}
    H->>H: 校验 mac_j -> 一致则配对成立
    J->>J: 校验 mac_h -> 一致则配对成立
    Note over H,J: 双向持久化 设备ID+指纹+sync_key(session确认); 标记令牌已用
  else 令牌已用/过期/不匹配
    H-->>J: REJECT{code: ERR_PAIR_TOKEN_*}
  end
```

### 3.3 二维码内容结构（字段级）

**字符串形式**：`PMS1:` + `base64url(UTF-8(JSON))`（前缀便于版本识别）。

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `v` | int | 是 | 协议版本，=1 |
| `ip` | string | 是 | Host 局域网 IPv4 点分十进制 |
| `port` | int | 是 | Host 监听端口（49152–65535 内随机空闲端口） |
| `token` | string(base64url) | 是 | **一次性配对令牌**，32B CSPRNG |
| `host_id` | string(uuid) | 是 | Host 设备稳定 ID |
| `host_name` | string | 是 | 展示用设备名（可含设备型号，不含隐私） |
| `fp` | string(hex) | 是 | Host 静态身份公钥指纹 = `sha256(pubkey)` 前 8 字节 hex，共 16 字符 |
| `exp` | int | 是 | 令牌过期时间 UTC 毫秒（默认 **签发 + 120 s**） |

**令牌有效期**：默认 **120 秒**；UI 提供倒计时与「刷新」（旧令牌立即作废）。**一次性**：Host 校验成功后立即 `used=true`；重复提交同一令牌 → `ERR_PAIR_TOKEN_USED`。

**指纹来源**：Host 首次运行生成**长期身份密钥对**（X25519，存于应用数据目录），其 `fp` 随二维码下发，Joiner 可（可选）向用户展示并与 Host 界面比对。

### 3.4 密钥协商与确认

- **ECDH**：双方各自生成**临时 X25519** 密钥对；`shared = X25519(my_priv_eph, peer_pub_eph)`。
- **派生**：`prk = HKDF-Extract(salt=session_salt, ikm=shared)`；
  - `sync_key = HKDF-Expand(prk, "passwordms/sync/v1/key", 32)`
  - `confirm_key = HKDF-Expand(prk, "passwordms/sync/v1/confirm", 32)`
- **密钥确认（防 MITM）**：`transcript = sha256(HELLO ‖ ACCEPT)`；双方交换 `HMAC(confirm_key, role ‖ transcript)` 并常量时间比对，任一方失败即中止并报 `ERR_FINGERPRINT_MISMATCH`。
- **共享密钥持久化**：`sync_key` 以「由 VK 包裹」的形式存入本库 `paired_devices`，用于后续会话（见 §3.5 会话引导）。

### 3.5 会话引导（已配对设备的后续同步）

已配对设备再次同步时，重放完整 ECDH 握手成本高；采用**基于共享密钥的一次性挑战**：

1. 双方各自生成一次性 `nonce_a`（Joiner）/`nonce_b`（Host）随握手交换；
2. 会话密钥 `sk = HKDF(sync_key, salt=nonce_a‖nonce_b, info="passwordms/sync/v1/session")`；
3. 双方用 `sk` 完成密钥确认（同上）；未配对设备因无 `sync_key` 无法推导 `sk` → **拒绝（A-07）**。
4. 每次同步 `session_id` 随机、`sk` 唯一，构成会话隔离。

**v1.2.4 增量（对端地址自愈 + 被动同步通知，缺陷见 §3.15）**

5. `ResyncHello` 增加 `#[serde(default)] sync_port: u16`：发起方通告**自己的**持久重同步监听端口。
   老客户端不发该字段 → 反序列化为 `0` → 响应方**保留**其原有 `peer_port`（不得被 0 清空）。
6. 响应方在**校验通过且同步成功之后**用「accept 出的真实来源 IP + `hello.sync_port`」刷新配对记录
   （`Repository::update_paired_addr`，只改 `peer_ip`/`peer_port`，不动 `sync_key_wrapped` 等）。
   失败/被拒的连接不得写入地址（防投毒）。
7. 被动同步（对端发起）成功后，壳层 `spawn_resync_worker` 向前端 emit `sync://applied`
   （载荷只有计数 `applied`/`blobs`/`conflicts`），前端据此 reload——否则本机 UI 会停留在同步前的快照。
8. **v1.2.10（D-74）**：上述自愈**只在连接成功后**才生效，换网络时是死锁。故发起侧在静态地址
   连接失败后增加一次**按需局域网发现**兜底，见 §3.17。

### 3.6 同步报文格式（字段级 schema）

传输：TCP + 长度前缀帧（`u32` 小端长度 + 帧体）。握手阶段帧体为**明文 JSON**；数据阶段帧体为**加密信封**。

#### 信封 `SyncEnvelope`（明文外壳 + 密文内层）

| 字段 | 类型 | 说明 |
|------|------|------|
| `session_id` | string(base64url 16B) | 会话 ID，每会话随机 |
| `seq` | int(u64) | **单调递增序号**，双向各自独立计数，从 1 起 |
| `ts` | int(i64) | 发送时刻 UTC 毫秒 |
| `nonce` | string(base64 24B) | AEAD nonce |
| `ct` | string(base64) | `AEAD(sk, nonce, SyncPayload, aad=session_id‖seq‖ts)` |

#### 内层 `SyncPayload`（被加密）

| 字段 | 类型 | 说明 |
|------|------|------|
| `kind` | enum | `push` / `pull` / `ack` / `conflict_notice` / `done` |
| `device_id` | string(uuid) | 发送方设备 ID |
| `cursor` | int(u64) | 变更游标（对端已确认的 `seq` 水位），`pull` 用 |
| `changes` | array | `push` 时的变更集，元素为 `ChangeRecord` |
| `conflicts` | array | `conflict_notice` 时的冲突清单（仅 ID + 时间，不含明文） |

#### `ChangeRecord`（变更集元素）

| 字段 | 类型 | 说明 |
|------|------|------|
| `record_type` | int | 0x01 Folder / 0x02 Item / 0x03 Trash / 0x04 Conflict |
| `record_id` | string(uuid) | 记录 ID |
| `rev` | int(u64) | 条目/记录修订号，本地每次编辑 +1 |
| `updated_at` | int(i64) | UTC 毫秒 |
| `modified_by` | string(uuid) | 最后修改设备 ID |
| `deleted` | bool | 墓碑标记 |
| `wrapped_key` | string(base64) | 由 **VK** 包裹的 IK（对端可用同一 VK 解包，故要求双端 VK 一致） |
| `ciphertext` | string(base64) | `AEAD(IK, nonce, payload, aad=record_type‖record_id)`，**端到端密文**，Host 无法解密对端明文 |

> **零知识保证**：`ciphertext` 由 IK 加密，IK 由 VK 包裹。双端 VK 相同（配对时由 Host 的 VK 通过 `sk` 加密下发 `VK` 给 Joiner，实现「同一保险库密钥」），故任一端都可解密，而链路嗅探者只有密文。**VK 下发**：配对成功后 Host 发送 `sync_key` 加密的 `VK`（`AEAD(sk, VK)`）给 Joiner，Joiner 用自己的 KEK 重新包裹存入自库。

> **#531 增量字段过线说明**：`Item.pinned/pin_order/use_count/last_used_at` 与 `Folder.pinned/pin_order` 全部 `#[serde(default)]` 内嵌于记录明文 payload，随密文原样过线——**协议零改动、报文零新增**；旧客户端 serde 忽略未知字段可读（语义不识别，双端同发惯例不变）。`use_count`/钉住变更推 `rev` 走**正常 LWW**（计频节流 60s 已把流量限制在每条目每分钟 ≤1 次；`record_item_usage`/`move_item_to_folder` 不动 `updated_at`，避免元数据变更污染用户可见的「最近更新」）。`FORMAT_VERSION` 不动。实测回归见 `tests/ordering_usage.rs`（8 例：排序四段/节流/钉住递增/重排校验/归组/旧库兼容/分组聚合/收藏不置顶）。

### 3.7 防重放（S4 / A-08）

| 机制 | 设计 |
|------|------|
| **单调序号** | 每一方向维护 `seq`，从 1 递增；接收端维护 `last_recv_seq`，**拒绝任何 `seq ≤ last_recv_seq`** 的报文 |
| **时间窗** | `ts` 必须落在 `[now-120s, now+120s]`；越界拒绝 |
| **会话绑定** | AAD 含 `session_id‖seq‖ts`，跨会话重放必然 tag 失败 |
| **一次性令牌** | 配对令牌用后即焚（`used=true`），重放旧令牌 → `ERR_PAIR_TOKEN_USED`（A-06） |
| **会话 ID** | 每会话随机，旧会话信封在新会话下无法通过 AEAD |
| **错误码** | 命中任一 → `ERR_REPLAY_DETECTED`，断开连接、记录**不含凭据**的审计计数 |

### 3.8 防中间人（S3 / 密钥确认）

- Apple/Google 通用做法：**ECDH + 指纹 + 密钥确认三件套**。
- `fp`（Host 身份指纹）随二维码下发，Joiner 可选展示给用户比对（防二维码被偷拍替换）。
- `HMAC(confirm_key, transcript)` 双向确认，阻断任何未持有 `shared` 的中间人。
- 二维码**不含密钥材料**，仅含路由信息与一次性令牌。

### 3.9 冲突检测与「冲突箱」

**裁定 Q3【已裁定】**：**不静默覆盖**；自动同步按条目级 LWW 推进，但**双方自上次同步后都改过同一条目**时，保留冲突副本进「冲突箱」，UI 三选一。

**基准（base）追踪**：`sync_state` 为每个已配对设备记录每条记录的 `last_synced_rev` 与 `last_synced_hash`。

| 情形 | 判定 | 处理 |
|------|------|------|
| 仅本端改 | `local.rev > base.rev` 且 `remote.rev == base.rev` | 推送本地版本 |
| 仅远端改 | 对侧反之 | 应用远端版本，更新 base |
| 双端都改且内容相同 | `local.hash == remote.hash` | 视为无冲突，更新 base |
| **双端都改且内容不同** | 两者 `rev > base.rev` 且 `hash` 不同 | **产生冲突**：本地版本保留为正式条目，远端版本快照存入 `ConflictEntry`，条目置 `has_conflict=true`，UI 三选一 |

**三选一语义**：

| 选项 | 动作 |
|------|------|
| 保留本机 | 丢弃远端快照，`has_conflict=false` |
| 采用远端 | 用远端快照替换本机条目，`rev = max(rev)+1` |
| 两者都留 | 本机保留；远端快照生成**新条目**（标题追加「(冲突副本)」，新 `item_id`） |

**`ConflictEntry`** 见 §5.1。

### 3.10 触发与重试

| 触发方式 | 说明 | 默认 |
|---------|------|------|
| **手动** | 「立即同步」按钮 | 始终可用 |
| **启动时** | 解锁后自动尝试一次 | 可开关，默认**关** |
| **定时/变更** | 数据变更后 debounce 3 s 触发；节流 ≥ 60 s | 可开关，默认**关** |

**失败重试**：指数退避 `1s → 2s → 4s`，最多 **3 次**，含 ±20% 抖动；仅针对「网络超时/连接拒绝」；协议类错误（令牌/重放/指纹）**不重试**，直接提示。

### 3.11 异常路径与返回码

| 场景 | 触发条件 | 返回码 |
|------|---------|--------|
| 未配对设备接入 | 无有效 `sync_key` 或握手确认失败 | `ERR_NOT_PAIRED`（A-07） |
| 令牌已过期 | `now > exp` | `ERR_PAIR_TOKEN_EXPIRED` |
| 令牌已使用 | `used == true` | `ERR_PAIR_TOKEN_USED`（A-06） |
| 令牌不匹配 | 提交值与签发值不等 | `ERR_PAIR_TOKEN_INVALID` |
| 指纹/密钥确认失败 | `HMAC` 比对失败 | `ERR_FINGERPRINT_MISMATCH` |
| 重放 | `seq ≤ last` 或 `ts` 越窗 | `ERR_REPLAY_DETECTED`（A-08） |
| 会话被中断 | 锁定/网络断开 | `ERR_SYNC_ABORTED` |

### 3.12 附件 blob 同步（增量设计 · 缺陷 D-53）

> **缺陷**：同步只搬运条目/分组，附件的密文 blob 帧不随行——对端有附件条目元数据却无内容，导出报「条目不存在」。本节在 §3.6/§3.9 之上做**纯增量**设计：不改 `SYNC_MAX_FRAME_BYTES`（U4 推论，冻结）、不引入明文过网、不改动既有 `Push → Push → Done` 交换循环的语义。

#### 3.12.1 代码现状裁定依据（工程师必读）

| 事实 | 出处 | 设计影响 |
|------|------|---------|
| `ChangeRecord` 无 `flags` 字段，wire 无法区分 blob 帧与条目帧 | `sync/protocol.rs` | 分片报文**不复用** `ChangeRecord`，另立 `BlobMessage` |
| blob 帧以 `record_type=ITEM + FLAG_BLOB` 存储，`record_id == blob_ref == item_id`；AAD = `record_type ‖ record_id` | `store/repository.rs` `write_record`/`import_attachment` | 分片传输可直接切片 blob 帧**密文**，对端按原字节还原整帧 |
| blob 帧头无 rev/updated_at（rev 属于 item 明文层）；`update_item` 不重写 blob 帧 → **blob 内容写后不可变** | `store/repository.rs` | blob 不参与 LWW，规则退化为「按 `blob_ref` 幂等补齐」 |
| 传输单帧上限 `SYNC_MAX_FRAME_BYTES = 1MB + 64KB`；1MB 明文经 AEAD(+16B)+Base64(×4/3) 膨胀 ≈ 1.37MB **必超限** | `constants.rs` / `sync/transport.rs` | 线上分片明文预算必须 < 1MB，见 §3.12.2 |
| 三条同步路径（配对 `server.rs`/`client.rs`、重同步 `resync.rs`）共用 `export_changes` + `merge::apply_changes` | 复核确认 | blob 阶段做成**公共模块**供三处复用，一处实现三处生效 |
| `export_changes` 只导出 live 记录且 `deleted` 恒为 `false`（墓碑不在 `live` 表） | `store/repository.rs` | **发现独立缺陷**：删除（条目与 blob 皆然）当前根本不会过线传播，见 §3.12.7 |

#### 3.12.2 分片传输协议（裁定：方案 B 改良——独立 blob 阶段 + 密文切片）

**结论**：采用 **B（条目交换完成后按需拉取的独立阶段）**，但分片**不是**「每块一条 `ChangeRecord`」，而是对 blob 帧密文的**透明切片**（新报文 `BlobMessage::Data`）。

**为什么不选 A（每块一条 ChangeRecord）**：
1. `ChangeRecord` 的契约是「一条密文对应一个完整 AEAD 帧」——密文切片不是合法 AEAD 输出，塞进去要为每片伪造 `plaintext_len` 语义，破坏 §3.6 的字段不变式；
2. 每片独立 IK 意味着 20MB 附件产生 20 组 `wrapped_key`，且对端收齐后仍须「解密→重组→重新 AEAD 封装」成单个本地 blob 帧（本地存储是单帧，U4），Android 端多付一轮 20MB 级 CPU+内存；
3. 三条同步路径的交换循环都要改造成多信封推送，动面大。

**B+切片的收益**：
- 发送端**零解密**：切片直接取自内存 `slots` 中 blob 帧的 `ct`（`Frame.ct` 常驻），不触碰明文；
- 接收端重组密文后**一次 `open_frame`**——AEAD tag 免费充当「组帧完整性+真实性」校验，无需额外哈希；
- 本地 blob 帧字节与对端**完全一致**（可原样 `append_frame`），对 A-02 式去重与压缩友好；
- 每信封仅 ~835KB 密文，Android 内存受控。

**分片尺寸推导（红线：`SYNC_MAX_FRAME_BYTES` 不放宽）**：

```
SYNC_BLOB_CHUNK_BYTES := 768 KiB = 786,432 B   （新增常量，线上分片尺寸）
Base64(786,432)            = 1,048,576 B（恰为 1 MiB，×4/3 精确整除）
信封 JSON 开销（会话ID/seq/ts/nonce/字段名）≈ 600 B
合计 ≈ 1,049,200 B < SYNC_MAX_FRAME_BYTES = 1,114,112 B ✓（余量 ≈ 64KB）
```

> 注意：`ATTACHMENT_CHUNK_BYTES = 1MB`（U4）是**本地**分片常量，继续用于本地；线上分片用 `SYNC_BLOB_CHUNK_BYTES`，二者**分离命名、分离取值**，互不牵动。

#### 3.12.3 协议时序（push 侧分片 → 传输 → 对端组帧 → apply → 条目可用）

```mermaid
sequenceDiagram
  autonumber
  participant A as 设备A(有附件)
  participant B as 设备B(缺blob)
  Note over A,B: 前置：既有 Push → Push → Done 交换已完成<br/>（attachment 条目 ChangeRecord 已到 B，B 解出 blob_ref）
  Note over A,B: 能力协商：双方 SyncPayload.blob_support=true 才进入本阶段<br/>（serde default=false，老端不触发、不收新报文）
  B->>A: BlobPull{ want:[blob_ref…] }  ← seq 单调延续，同信封加密
  A->>A: 逐 ref：取 slot 中 blob 帧的 wrapped_key/nonce/plaintext_len/ct（零解密）
  A->>B: BlobMeta{ blob_ref, item_id, wrapped_key, wrapped_key_nonce, nonce, plaintext_len, chunk_total, chunk_size }
  loop chunk_total 次（每次一个独立加密信封）
    A->>B: BlobData{ blob_ref, index, ct_slice(≤768KiB) }
    B->>B: 落 spool：idx.part（密文切片）+ manifest 记账
  end
  B->>B: 组帧：ct = part₀‖…‖partₙ → Frame{ITEM, FLAG_BLOB, record_id, …} → open_frame 验 tag
  alt 验签通过且容量允许
    B->>B: append_frame 入库 → blobs 索引可见 → 条目立即可导出
    A->>B: BlobPhaseEnd{ completed:[ref], missing:[] }
  else tag 失败/超容量
    B->>B: 丢弃 spool，不入索引（read_blob 永不见半成品）
    A->>B: BlobPhaseEnd{ completed:[], missing:[ref] }（UI 呈现部分同步）
  end
  Note over A,B: 角色对调：A→B 发 BlobPull(A 侧缺失清单)，对称重复②~⑤<br/>（双向各拉各缺，互不阻塞）
```

**方向与编排**：`Done` 之后，先 A 拉 B 缺的，再 B 拉 A 缺的（串行，避免半双工交织）；两条路径共用 `sync/blob_transfer.rs` 的 `pull_missing()` / `serve_pull()`。

#### 3.12.4 新增 / 变更的数据结构

**`sync/protocol.rs`**：

```rust
// SyncPayload 增量（serde default → 老端反序列化兼容）
#[serde(default)] pub blob_support: bool,

// 新报文族（与 HandshakeMessage 平行的明文外壳 + 信封加密，serde tag="t"）
pub enum BlobMessage {
    Pull  { want: Vec<String> },                       // 拉取方 → 对端：缺失清单
    Meta  { blob_ref: String, item_id: String,         // 帧元信息（一次）
            #[serde(with = "b64")] wrapped_key: Vec<u8>,
            #[serde(with = "b64")] wrapped_key_nonce: Vec<u8>,
            #[serde(with = "b64")] nonce: Vec<u8>,
            plaintext_len: u32, chunk_total: u32, chunk_size: u32 },
    Data  { blob_ref: String, index: u32,              // 一片一信封
            #[serde(with = "b64")] ct_slice: Vec<u8> },
    End   { completed: Vec<String>, missing: Vec<String> },  // 阶段收尾
}
```

**`constants.rs`**：`SYNC_BLOB_CHUNK_BYTES: usize = 768 * 1024`（推导见 §3.12.2）。

**`store/repository.rs` 新增原语**（只加不改既有签名）：

| 方法 | 职责 |
|------|------|
| `has_blob(blob_ref) -> bool` | `blobs` 表查询（补齐判定的依据） |
| `attachment_blob_refs() -> Vec<String>` | 收集**全部可能恢复该 blob 的引用**：活跃条目 ∪ 回收站快照 ∪ 冲突快照（两侧），见 §3.14.4 与 D-79 修订注记 |
| `blob_frame(blob_ref) -> Option<&Frame>` | 暴露完整 blob 帧（含 `wrapped_key`/`nonce`/`plaintext_len`/`ct`），供发送端零解密切片 |
| `write_blob_frame(frame: Frame) -> BlobWriteOutcome` | 接收端入库：帧已存在且字节一致 → `Skipped`；存在但不同 → 保留本地 `KeptLocal`；不存在 → `Appended`（blob 帧内容写后不可变，不做 LWW 覆盖） |
| `remove_item_direct`（变更） | 删除 attachment 条目时**连带 `tombstone_blob`**（先查 kind 再墓碑） |

**新模块 `sync/blob_transfer.rs`**：`pull_missing()` / `serve_pull()` / `BlobPhaseReport{ completed, missing }`；内含 spool 管理（§3.12.6）。

**`SyncResultDto` / `ResyncOutcome`（IPC）**：`#[serde(default)] blobs: usize`（本阶段实际入库 blob 数，UI 呈现）。

#### 3.12.5 blob 的 rev/updated_at 语义与收敛（裁定）

- blob 帧头**无** rev/updated_at（帧布局 §2.6 既有事实），rev 语义属于 item 明文层 → **blob 不参与 LWW**；
- 当前实现中 blob 内容**写后不可变**（`update_item` 仅重写 item 帧；无替换 blob 的 API）→ 规则收敛为幂等补齐：
  - 本地缺 `blob_ref` → 拉取；本地已有且字节一致 → 跳过（不重传不重写）；
  - 本地已有但字节不同 → **保留本地**、计入 `missing` 上报（异常路径，记审计日志，不得覆盖本地）；
- **次序无关收敛**：
  - 先 item 后 blob → item 在列表可见，导出/查看返回 `ERR_BLOB_MISSING`，UI 显示「附件内容待同步」；blob 补齐后立即可用；
  - 先 blob 后 item → blob 静默入库（无 item 引用，无 UI 入口，无泄露面）；item 到达后零成本可用。

#### 3.12.6 不完整 blob 的中间态与断点续传（裁定）

- **spool 目录**：`<保险库文件路径>.blobspool/<blob_ref>/`，内含 `idx.part`（**密文切片**）+ `manifest.json`（`{blob_ref, chunk_total, chunk_size, received:[…]}`）。**A-01 安全**：spool 只存密文，泄漏不产生明文暴露面；
- **`read_blob` 永不见半成品**：`blobs` 索引仅在「组帧 → `open_frame` tag 验证通过 → `append_frame` 入库」成功后写入；spool 与正式索引物理隔离；
- **断连重试续传**：manifest 持久化已收分片，重同步时该 `blob_ref` 仍在缺失清单 → 已收分片跳过（按 `idx` 与 `part` 文件长度双重校验），只补差额；
- **孤儿清理**：锁定时 / 下次同步开始时，清理「无对应 attachment 条目」的 spool 目录（条目中途被删的场景）；
- **上限校验前置**：接收端在 `Meta` 阶段做 `MAX_VAULT_BYTES` 容量预检，超限直接拒绝（回 `missing`），不落半个分片；
- **内存峰值备案（D-56 轮补注）**：finalize 的「拼接 → `open_frame` 验 tag」在内存中持有完整密文一次（≤ 20MB + 16B tag）；MVP 可接受，如需优化可改为流式拼接 + 分块校验（不改协议）。

#### 3.12.7 删除传播（裁定 + 独立缺陷上报）

- **merge 侧连带规则（本次就位）**：`apply_changes` 的 ITEM `deleted=true` 分支，若被删条目为 attachment → 连带 `tombstone_blob`（与 `delete_item(permanent=true)` 本地语义对齐）；本地**软删（进回收站）不清 blob**（还原需要），仅远端删除与 purge 清；
- **🔴 独立缺陷（建议另立 D-5x，不并入 D-53）**：`export_changes` 只遍历 `live` 表且 `deleted` 恒为 `false` → **墓碑记录从不导出，当前任何删除（条目/blob/分组）都不会传播到对端**。blob 删除传播的触发前提缺失。本设计已把 merge 侧连带逻辑与 blob 幂等规则就位，待墓碑导出补齐后自动生效；D-53 验收范围限定为「blob 可用性同步」。

#### 3.12.8 安全复核（A-01 / A-06 / A-07 / A-08 / A-09）

| 项 | 复核结论 |
|----|---------|
| 明文过网 | **无**。线上载荷 = 会话密钥 AEAD 信封内的 blob 帧**密文切片**（双重加密：会话层 + blob IK 层），发送端全程不解密 |
| 同步层是否需解密 blob | **不需要**。对端持 VK（U1/U2 裁定共享）即可解开 `wrapped_key` 入库；唯一解密发生在接收端 finalize 的一次 `open_frame`（兼作组帧校验） |
| 防重放 | **不受影响**。blob 阶段全部信封走同一 `ReplayGuard`，`seq` 从 `Done` 之后单调延续，AAD 仍绑 `session_id‖seq‖ts`（A-06/A-08） |
| 未配对/越权 | 阶段仅在既有握手+密钥确认成功后进入，无新增认证面（A-07） |
| 日志/落盘 | spool 仅密文；日志不记 `blob_ref` 以外任何内容，不记凭据（A-09） |

#### 3.12.9 新增错误码

| code | 含义 | 触发 |
|------|------|------|
| `ERR_BLOB_MISSING` | 附件条目存在但本地无密文 blob | 导出/查看附件时 blob 未同步 |
| `ERR_BLOB_INCOMPLETE` | 组帧失败（分片缺失/错序/tag 不符） | finalize 校验失败（内部，转 `missing` 上报） |
| `ERR_BLOB_CAPACITY` | 接收端容量预检超限 | `Meta` 阶段 `MAX_VAULT_BYTES` 预检 |

#### 3.12.10 兼容性（裁定）

MVP 两端同时发版；协议层以 `SyncPayload.blob_support`（`serde(default)=false`）做**能力协商**：老端不带该字段 → 新端**不发起** blob 阶段，行为与现状逐位一致（老端永远不会收到无法解析的新报文）；两端同版则全功能。混跑期间 D-53 缺陷在老端依旧存在——可接受，文档留痕。

> **🔴 修订注记（D-60 附件依附父条目）**：本节「blob 帧的 `record_id == blob_ref == item_id`」假设**自 D-60 起作废**——blob 帧 `record_id` 改为附件自身的 `attachment_id`（UUID，≠ 父 item_id），附件元数据内嵌于父条目（§3.14）。`blob_transfer.rs` 按 `blob_ref 16B` 传输与 spool 按 `blob_ref` 隔离的机制**不变**；变化仅在 blob 清单来源（`attachment_blob_refs` 遍历父条目 `attachments` 数组，**含回收站快照**）与孤儿清理判定（可达性 GC，§3.14.4）。`blob_support` 协商机制不变。

### 3.13 删除传播（增量设计 · 缺陷 D-56）

> **缺陷**：`export_changes` 只遍历 `live` 表且 `deleted` 恒为 `false`，墓碑帧从不导出 → 任何删除（条目/分组/blob）不过线（§3.12.7 已上报）。本节补齐「墓碑导出 + 远端删除语义」，**零帧格式变更、零 wire 结构变更**，与 §3.12 blob 阶段正交。

#### 3.13.1 裁定一：墓碑枚举——解密原 payload 取 rev，零格式变更

**事实**（复核确认）：帧头无 rev/updated_at；`tombstone()` 仅 `flags |= FLAG_TOMBSTONE` 原地 pwrite，**payload（删除前的原记录密文）未被重写**。

**裁定**：**不改 `tombstone()`、不改帧格式**。导出时遍历 `slots` 中满足「`is_tombstone && !is_blob && record_type ∈ {ITEM, FOLDER}`」的帧，用本端 VK 经既有 `open_frame` 解密原 payload → 反序列化 `Item`/`Folder` → 取 `rev`/`updated_at` → 构造 `ChangeRecord{ deleted: true, rev, updated_at, wrapped_key/nonce/ciphertext 原样 }`。

- **老库兼容**：旧墓碑帧 payload 同样未重写，天然可提取；解密失败（损坏）→ **跳过该墓碑并计数，不致命**（统一错误语义，A-05）；
- `deleted_at` 不入 wire：merge 的墓碑分支只消费 `rev`（base 推进），不参与 LWW 比较；回收站展示的删除时间由对端**本端**生成（软删时刻）；
- blob 墓碑**不独立导出**：item 墓碑已完整表达删除意图（对端软删保留 blob，见裁定二），blob 帧墓碑仅本地语义。

#### 3.13.2 裁定二：远端删除 = 软删（进回收站），修正 §3.12.7 连带规则

**结论**：远端 item 墓碑到达时，对端**走软删**（新 `Repository::trash_item_direct`，等价 `move_item_to_trash`：新 `trash_id` 由对端本地生成、`deleted_at` = 对端当前时间），**不是** `remove_item_direct` 永久删。

- **理由**：(a) 安全需求 O-05 要求删除默认走回收站而非物理删除——对端用户同样享有误删保护；(b) 两端语义对称，A 端软删 ⟺ B 端软删；(c) 若永久删，单端误删/缺陷即瞬时摧毁对端数据且不可恢复；
- **🔴 修正 D-53 §3.12.7**：远端删除改软删后，`remove_item_direct` 的「连带 `tombstone_blob`」**不再由 merge 触发**——软删后 blob 必须保留（回收站还原需要）；blob 墓碑仅随**本端** purge（`purge_item`/`empty_trash`/30 天到期）发生。`remove_item_direct` 降级为内部保留方法（merge 不再调用）。

**merge `deleted` 分支新逻辑（幂等 + 收敛）**：

```text
base = state.base.get(id)
若 base.rev >= change.rev  → report.skipped += 1; continue     // 幂等判重：墓碑不重放（收敛）
match repo.get_item_opt(id):
  Some(local) if local.rev > change.rev:                        // 删除-修改拉锯：本端更新
      remote_snapshot = repo.decrypt_change(change)?           // 墓碑 ct = 死亡前快照（零 wire 变更！）
      add_conflict(ConflictEntry{ local, remote_snapshot })    // 入冲突箱，UI 三选一（Q3）
  Some(_)  => repo.trash_item_direct(&local)?                   // 软删：tombstone ITEM + 写本端 trash
  None     => {}                                                // 本端从未见过，仅推进 base
state.base.insert(id, { rev: change.rev, hash: [0u8;32] })      // 无论何分支，base 推进到墓碑 rev
```

- **删除-复活拉锯收敛确认**：A 删（rev=R）、B 未同步前改（rev=R+1）→ B 收墓碑 rev=R，`local.rev(R+1) > R` → **冲突箱而非复活/静默删**；仲裁「保留本机」→ B 条目存活、后续 item 变更正常过线 → A 端复活（A 本地无条目 + 非删除变更 → 正常 put）✓；仲裁「采用远端」→ B 软删 + base 推进，A 端 trash 快照本地私有保留 ✓。两向皆收敛，无拉锯震荡；
- **FOLDER 墓碑 = 仅删分组壳，无级联**：A 端 `delete_folder(ToTrash)` 的条目级联**已经**通过逐条 item 墓碑过线（`delete_folder` 内部对每条 `move_item_to_trash`）；`ToUngrouped` 的条目转未分组也已通过 item 更新（`folder_id=None, rev+1`）过线。对端只需应用分组墓碑 + 各 item 变更，**不得**再自行级联（会与对端用户意图冲突）。对端条目 `folder_id` 悬空的瞬态（同步中断）由前端兜底「分组不存在 → 渲染为未分组」。

#### 3.13.3 裁定三：墓碑保留与收敛

| 项 | 裁定 |
|----|------|
| 墓碑帧物理累积 | **MVP 可接受，本轮不物理清除**（留 TODO：compaction 时剔除「全部已配对设备 base.rev 均已推进至该墓碑 rev」的帧） |
| 体积量化 | 条目/分组墓碑 ≈ 124B 帧头 + 300–600B 密文/条；按 1000 次删除估算 ≤ **0.75 MB**（< 200MB 上限的 0.4%）。**blob 墓碑是唯一大头**：`tombstone_blob` 原地标记，≤20MB 密文仍占文件——`file_size()` 容量预检已计入（现状即含），不产生超卖；blob 墓碑回收依赖未来 compaction（TODO） |
| 免重发 | `export_changes` 增加 **peer base 过滤**：base 已推进（`base.rev >= 墓碑 rev`）的墓碑**不导出** → 连续两次同步第二次墓碑零重发、对端零重复应用（带宽 + 幂等双收益） |

#### 3.13.4 裁定四：trash 记录本身不过线（确认建议）

**确认**：`RECORD_TYPE_TRASH` 的 live 记录与墓碑**均不导出**。理由：回收站是本地私有视图（30 天本地保留），`trash_id` 由各端独立生成，同步会造成 ID 漂移与重复记录；对端删除感知完全由 item 墓碑承载。顺带修正既有浪费：`export_changes` 现遍历整个 `live` 表，TRASH/CONFLICT/DEVICE/SYNC_STATE 的 live 记录也会被发出去在对端 `skipped`——本次**白名单化**：live 仅导出 ITEM/FOLDER，墓碑仅导出 ITEM/FOLDER，其余一律不发。

#### 3.13.5 裁定五：与 blob 阶段的交互（确认）

- 远端 item 墓碑 → 对端**软删**（blob 保留）→ 条目进对端回收站，`attachment_blob_refs()`（基于活跃条目）不再含该 `blob_ref` → §3.12.6 的孤儿清理**只清 spool 目录，不清正式 blob 索引**（现状设计即如此，确认无需改）→ 回收站内附件可正常还原导出；
- 对端 30 天到期 `purge_expired_trash` / 手动彻底删 → 本端 `tombstone_blob` → blob 不可读、密文留待 compaction（TODO）；~~对端 purge 事件不需要额外过线报文~~ **D-98（C 案，2026-10-25）修订：purge 意图过线**，见 §3.13.7；
- `blob_transfer.rs` 拉取判定基于活跃条目 → 已删条目的 blob 既不会被拉取也不会被误清 ✓。

#### 3.13.7 裁定七：purge 意图过线（D-98，C 案）

**背景**：裁定四使 TRASH 记录永不过线，本端「彻底删除」对端不可感知——实测（主人 2026-10-25）手机彻底删除后同步，电脑端回收站纹丝不动，只能等 30 天过期。与 e2e 走查单 2.6 的期望（彻底删除后对端消失）天生矛盾。**修订**：purge 意图借道 ITEM 墓碑过线，**零 wire 格式变更**：

1. **发送侧**（`Repository::purge_item`）：以回收站死亡快照（`TrashEntry.item_snapshot`）为载荷、`rev+1` 重封一枚 ITEM 墓碑**追加**落盘（`append_tombstone_frame`，不进 live 索引；`empty_trash`/`purge_expired_trash` 逐条走同一路径）。同 `(type,id)` 多枚墓碑帧并存：导出（`tombstone_changes`）/ 复活抑制（`tombstone_rev`）取 rev 最高者，compaction 每死记录只留最高一枚——旧帧无需清理。死亡快照密文仍可对端解出（冲突箱/审计依赖不变）。
2. **接收侧**（`merge::apply_changes` ITEM deleted 分支的 None 臂）：本端无活跃条目且持有 `snapshot.rev ≤ 墓碑 rev` 的回收站快照 → `purge_trash_snapshots` 顺带彻底删除（TRASH 墓碑照旧仅本地，blob 引用收窄交孤儿 GC）。快照 rev 更高（本端删了更新版本）→ 保留，待自身墓碑过线收敛。幂等/免重发由既有 base 机制天然承担（对端 note_sent 后 `(rev+1, hash=0)`，第二轮零重发、零回声）。
3. **边界（文档化）**：对端在 purge 到达前**还原**（live rev == 墓碑 rev）→ 沿用既有「等 rev 删除胜」语义软删（可从回收站再恢复，不丢数据）；live rev 更高 → 删除-修改拉锯进冲突箱（不变）。对端从未见过该条目 → 不复活、仅推进 base（复活抑制不变）。

守门：`tests/d98_purge_sync.rs`（传播收敛/rev 保护/等 rev 竞速/单机语义/empty_trash 批量/从未见过六例）+ `tests/sync_deletion.rs` case5（重写：软删 blob 保留 → 还原补齐 → purge 传播 → blob 孤儿 GC → 收敛无回声）。

#### 3.13.6 函数签名变更（前后端契约不变，仅 Rust 内部）

| 签名 | 变更 |
|------|------|
| `Repository::export_changes(&self, peer_base: &DeviceSyncState) -> Vec<ChangeRecord>` | **签名变更**：增加 peer base 参数做免重发过滤；内部并入墓碑枚举与类型白名单。4 个调用点（`server.rs:349`、`client.rs:114`、`resync.rs:197/383`）各传在手 `state` |
| `Repository::tombstone_changes(&self, peer_base) -> CoreResult<Vec<ChangeRecord>>`（**新**） | 遍历墓碑帧 → 解密取 rev → `ChangeRecord{deleted:true}`；解密失败跳过计数 |
| `Repository::trash_item_direct(&mut self, item: &Item) -> CoreResult<()>`（**新**） | merge 专用软删：写本端 trash（新 trash_id、deleted_at=now）+ tombstone ITEM（**不**清 blob） |
| `merge::apply_changes` ITEM/FOLDER deleted 分支 | 重写为 §3.13.2 逻辑（幂等判重 / 冲突箱 / 软删 / base 推进） |
| `Repository::remove_item_direct` | 降级为内部保留（merge 不再调用）；文档标注 |

**时序要点**（并入既有 `Push → Push → Done` + blob 阶段，无新阶段）：Push 载荷的 `changes` 由「live ITEM/FOLDER + 墓碑 ITEM/FOLDER（peer 过滤后）」构成 → 对端 merge 按上文分支应用 → 墓碑在两端 base 推进后自然停发。wire 结构、传输上限、加密层**零变更**。

#### 3.13.7 兼容性

老端（D-56 前）收到新端墓碑 ChangeRecord：merge 既有 deleted 分支会执行 `remove_item_direct`（永久删）+ base 推进——**行为正确但语义偏狠**（对端无回收站痕迹），且老端不导出墓碑故删除单向。MVP 两端同版发版（与 D-53 同一约束），混跑期间可接受，文档留痕。

#### 3.13.8 实现补注（D-56 落地时的两处细化与一处补充，用例实证必要）

1. **幂等判重 / 导出过滤按 base.hash 细化**：base 与墓碑同 rev 但 `hash != [0;32]` 表示本端持有**同 rev 活跃条目**，该墓碑是**首次到达**，不得判重跳过——常规流「同步 rev1 → 删 rev1」中 `base.rev == 墓碑 rev` 恒成立，伪码字面的 `>=` 会使删除永远不过线。修正为：`base.rev > rev`，或 `base.rev == rev && base.hash == [0;32]`（墓碑标记）才判重；导出侧 peer 过滤同理。
2. **发送侧墓碑基准推进（`merge::note_sent_tombstones`）**：协议无「已应用」回执，接收侧 base 无法回传；各发送点在**发送成功后**把已导出墓碑记入本端对该对端的 base（`hash=[0;32]`），下一轮导出过滤即零重发（§3.13.3 的落点）。失败路径不落盘（`save_sync_state` 仅在整轮成功后执行），中断自动重发，宁多勿丢。
3. **复活抑制**：merge 的 live 分支在本端无活跃条目但持有 `rev >= 变更 rev` 的墓碑时跳过该 live 变更——否则「采用远端」删除仲裁后的收敛会被对端陈旧 live 变更回滚（拉锯震荡）。

> **🔴 修订注记（D-60 附件依附父条目）**：本节的「条目删除连带 blob」规则按 §3.14.4 改写为**可达性 GC**——父条目墓碑（附件元数据随父帧密文一同死亡）到达对端走软删、blob 保留（回收站还原需要），blob 墓碑仅在**孤儿回收**时发生（扫描全部活跃条目 + 回收站快照的 `attachments[].attachment_id`，不在引用集合中的 blob 帧才 `tombstone_blob`）。原「`remove_item_direct` 连带 `tombstone_blob`」1:1 规则作废（KeepBoth 仲裁下两条目可共享同一 blob_ref，1:1 会误杀）。merge 的墓碑判重/冲突箱/复活抑制逻辑**不变**（全部作用于父条目层，附件无独立 rev）。

> **架构复核追认（高见远，2026-10-18）**：上三点全部追认——1 是对 §3.13.2 伪码 `>=` 缺陷的正确修正（主干路径阻断级，反例独立复推成立）；2 是 §3.13.3 欠定处的必要补全；3 是收敛正确性必需。**残余风险备案**：若本端完整走完一轮（send 成功→base 推进→落盘）而接收端在读取 push 帧之后、apply 落盘之前崩溃，该墓碑两端均不再重发，收敛为「对端存活、本端已删」的静默分歧（数据无损、不复发不震荡，窗口毫秒~秒级）——MVP 接受为已知残余风险。远期修正方向（P2 backlog）：(a) Lamport/HLC 时钟统一删除-vs-live 新旧比较（推荐，零 wire 变更）；(b) Done 载荷 base 回执反熵。到达序不对称（墓碑先到→冲突箱；live 先到→静默复活）同属此本质冲突，一并入 P2。

#### 3.13.9 实现补注（D-57 · 重同步连接超时与网络文案归一化）

1. **连接统一短超时**：`SYNC_CONNECT_TIMEOUT_SECS = 4` 为配对与重同步共用常量；`transport::connect`（无超时版）已删除，全部连接走 `connect_with_timeout`。
2. **connect 阶段文案统一**：`stage == "connect"` 的 Io 失败无论 kind 一律报 `err.network_unreachable`（「无法连接到对端设备：请确认两台设备在同一 Wi‑Fi 同一网段…」）——连接失败对用户只有一种可行动含义。**v1.2.4 起该文案方向中性**（原文写死「无法连接到电脑…手机与电脑」，电脑端触发时方向反了）。配对路径 client.rs 的 connect 失败**不带** connect 阶段标注，仍按 kind 归一化结果报 `network_refused`（「对方可能已离开配对页」），两场景可行动含义区分保留（QA 实证）。
3. **跨平台 kind 形态根因（D-44 遗留缺陷修正）**：`format!("io:{}", err.kind())` 的 Debug 输出在 Windows 实测为小写带空格（`io:connection refused`/`io:timed out`，QA harness 本机实证），D-44 的 PascalCase 字面映射表（`io:ConnectionRefused`）**整体落空坠入 `err.io`**——用户网络错误全部显示「文件读写出错」。修复：message_key 对 kind 归一化（去 `io:` 前缀 → 小写 → 仅留字母数字）后匹配，双形态通吃；`operation would block` 等未映射 kind 仍走 `err.io` 兜底（WouldBlock 在 D-46 轮询中是正常控制流不上屏，无回归）。

### 3.14 附件产品语义修正（增量设计 · D-60：附件依附父条目）

> **需求**（主人 #530 走查裁定）：附件不是第三种条目类型，而是**登录/安全笔记条目的附属内容**——详情页内添加/查看/导出/删除，文件随父条目走（父删除 → 附件级联；同步随父条目变更传播）。不再有独立 attachment 条目与列表页「添加附件」按钮（D-59 入口回退）。

#### 3.14.1 数据模型裁定：案 a —— 父条目内嵌 `attachments` 数组

**两案权衡**：

| 维度 | 案 a：条目内嵌数组 | 案 b：独立记录 + `parent_item_id` 外键 |
|------|------------------|--------------------------------------|
| 实体/白名单 | 零新增（复用 ITEM 帧与全部既有链路） | 新 record_type(0x08)：export 白名单、墓碑导出、peer 过滤、merge 分支、冲突箱拼装全链路加类 |
| 同步帧 | 元数据随父 `ChangeRecord` 加密帧走，自动获得 E2E/rev/快照 | 父子两帧独立同步，冲突检测粒度碎，仲裁需拼装 |
| 删除语义 | 父 tombstone → 附件级联天然成立 | 级联需外键扫描 + 子墓碑传播 |
| 冲突箱 | Item 快照完整含附件 | 快照须拼父子两帧 |
| blob 引用 | 需孤儿 GC 替代 1:1 引用（见 §3.14.4） | 天然 1:1 |
| IPC/UI | 父条目详情一跳可达 | 双实体查询拼装 |

**裁定：案 a**。决定性理由：同步链路（白名单/墓碑/判重/冲突箱）**零新增类型**，删除级联天然成立；代价（孤儿 GC）与回收站生命周期天然对齐（§3.14.4）。附件数上限（常量 `MAX_ATTACHMENTS_PER_ITEM = 16`，禁硬编码）下单条目元数据最坏 ≈ 16 × ~200B ≈ 3.2KB，相对 1MB 传输帧上限无压力。

**结构定义（serde，wire snake_case；`#[serde(default)]` 向后兼容）**：

```rust
pub struct AttachmentMeta {
    pub attachment_id: String,      // UUID；同时是 blob 帧 record_id（16B）
    pub filename: String,           // 原始文件名（wipe_plaintext 覆盖）
    pub mime_type: String,
    pub size_bytes: u64,
    pub blob_size_bytes: u64,       // 密文占用（容量统计）
}
pub struct Item {
    // … 既有字段不变 …
    #[serde(default)]
    pub attachments: Vec<AttachmentMeta>,
}
```

- **`ItemKind::Attachment` 变体废弃**（保留反序列化仅供迁移读取，§3.14.3）；
- **`attachment_id == blob 帧 record_id`**：旧模型「`blob_ref == item_id`」的强耦合解耦——同一 blob 帧可被多条目引用（KeepBoth 仲裁场景），附件删除**不**立即 tombstone blob，交孤儿 GC；
- `wipe_plaintext`：`filename` 纳入擦除（与原 `AttachmentData.original_filename` 同级私密）；
- 搜索（`item_matches`）与健康检查**不含**附件文件名（最小改动：不扩大搜索面，文件名随父条目密文天然受 A-01 保护）。

#### 3.14.2 同步协议影响：元数据随父帧走，blob 阶段机制不变

| 项 | 影响与裁定 |
|----|-----------|
| 元数据传输 | `attachments` 在父条目明文 payload 内 → 随父 `ChangeRecord` 加密过线：**自动获得**端到端加密（A-01）、rev 版本化、LWW、冲突快照、D-56 判重/墓碑——**wire 结构零变更** |
| blob 拉取清单 | 对端 merge 应用父条目后，`attachment_blob_refs()` 收集**全部活跃条目 + 回收站快照 + 冲突快照（两侧）**的 `attachments[].attachment_id`（D-79）；缺失者即拉取清单（父帧数组即清单，无需新报文） |
| blob 阶段机制 | `BlobPull/Meta/Data/End`、`SYNC_BLOB_CHUNK_BYTES`、spool 按 `attachment_id`（原 blob_ref）目录隔离、manifest 续传、`blob_support` 协商——**全部不变**；仅 spool 键由「父 item_id」变为「attachment_id」（多附件互不干扰） |
| D-56 级联 | 父墓碑到达 → 对端软删父（trash 快照含附件元数据）→ blob 保留；对端 purge 父 → 孤儿 GC tombstone blob。**无新增墓碑类型**，判重/复活抑制/冲突箱逻辑全部保持在父条目层 |
| 冲突仲裁 | 「两者都留」：新条目复制 `attachments` 元数据、`attachment_id` 原样 → 两父条目**共享**同一 blob 帧（内容写后不可变，双份导出均正确）；删除其一不清 blob（GC 兜底） |
| 老端兼容 | 父帧多出 `attachments` 字段：老端 serde 忽略未知字段可读，但**语义不识别**（老端把附件当独立条目）；本项目双端同发惯例不变——**混跑不支持附件新语义**，`blob_support` 协商照旧，文档留痕 |

#### 3.14.3 迁移：启动时一次性自动迁移（独立附件条目 → SecureNote 挂附件）

**裁定：自动迁移（migrate on unlock）**，废弃重导不采纳。依据：blob 帧可零重写复用（迁移仅改父帧元数据，blob 帧原样保留）；迁移逻辑幂等可测；「主人库只有走查样例」不构成废弃重导的充分理由——自动迁移对任意老库都正确。

规则（解锁重建索引后、对 `kind == Attachment` 的存量条目一次性执行）：

1. 生成新条目 `kind = SecureNote{ title: 原附件 title, body: "" }`——**选 SecureNote 而非 Login**：假 Login（无密码）会污染 F-16 体检「弱口令」统计，笔记无此副作用；
2. `attachments = [AttachmentMeta{ attachment_id: 原条目 item_id（= 原 blob_ref → **blob 帧零重写**）, filename: 原 original_filename, … }]`；
3. 新条目 `rev = 原 rev + 1`、`folder_id/favorite/created_at` 继承 → 正常随同步传播；
4. 旧 ITEM 帧置墓碑（D-56 墓碑链路自动传播删除）；
5. `persist_header` + 计数修正；迁移在 `wipe_plaintext` 之前、幂等（无 `kind == Attachment` 即空转）。

#### 3.14.4 blob 生命周期：孤儿可达性 GC

取代 D-53/D-56 的 1:1「条目删 → blob 墓碑」规则：

- **引用集合 = 活跃条目 ∪ 回收站快照 ∪ 冲突快照（local_snapshot + remote_snapshot 两侧）**：
  - 活跃条目 `attachments[].attachment_id`；
  - 回收站快照的 `attachments[].attachment_id`（还原必须可用）；
  - **冲突快照的 `local_snapshot` 与 `remote_snapshot` 各自的 `attachments[].attachment_id`**
    ——**仅限尚未仲裁（`resolved == false`）的冲突**（D-79 引入，D-80 收窄，见下方两条修订注记）；
- **GC 触发时机**：`purge_item` / `empty_trash` / `purge_expired_trash` / 解锁时（幂等扫描）；不在引用集合中的 blob 帧 → `tombstone_blob`；
- **正确定性**：删除单个附件（父条目仍存活）→ 该 blob 立即脱离引用集合 → 下一次 GC 时 tombstone（密文滞留待未来 compaction，§3.13.3 量化结论不变）；KeepBoth 共享引用 → 双方任一存在即保活 ✓；
- 容量预检（`MAX_VAULT_BYTES`）语义不变（`file_size()` 已含全部滞留密文）。

> **🔴 修订注记（D-79 冲突快照必须计入引用集合，v1.2.9）**：
> 上述引用集合**从 D-60 落地起就漏了 `conflicts`** —— `ConflictEntry` 持有 `local_snapshot: Item` 与
> `remote_snapshot: Item`，两者都带 `attachments`，但 `attachment_blob_refs()` 只遍历 `items` 与 `trash`。
> **一个函数四张嘴，四处一起错**：`gc_orphan_blobs()`（tombstone 掉）、`plan_compaction()`（D-76 压缩重写时丢弃，
> 最彻底——帧从文件里消失）、`cleanup_orphan_spools()`（spool 被删）、`send_blob` 的 `want` 清单（对端缺该 blob 时不去拉）。
> **触发路径**：同一 item 双端并发修改，一版删了附件 A、另一版仍含 A → 冲突快照引用 A、活条目不再引用 A
> → 用户**尚未仲裁**时跑 GC/压缩 → 选「采用含 A 的那版」⇒ `BlobMissing`，附件永久丢失。
> **修法**：`attachment_blob_refs()` 追加 `self.conflicts` 遍历，两个快照都收；**`resolved` 这一版不过滤、一律计入**
> （代价不对称：多保活只浪费空间，少保活是永久丢数据）。
> **不变量**：引用集合必须覆盖**所有可能恢复该 blob 的快照来源**；新增任何「暂存一份 `Item` 等用户决定」的结构，
> 必须同步加进这里。
> 守门测试：`tests/compaction.rs` 的 `compact_keeps_blob_referenced_by_conflict_snapshot` /
> `gc_orphan_blobs_keeps_blob_referenced_by_conflict_snapshot` / **`compact_drops_blob_referenced_by_nobody`（防过修）**。

> **🟢 二次修订（D-80 收窄：已仲裁冲突停止保活，v1.2.11）**：
> D-79 的「一律计入」是**保守兜底**——当时没核清「已仲裁快照是否还会被读」。核实后结论是**不会**，于是收窄：
> `attachment_blob_refs()` 遇 `resolved == true` 直接 `continue`，两端快照的引用不再计入。
>
> **三条依据（已核实，不是「看起来没人读」的推测）**：
> 1. **读不到**：`list_conflicts()` 过滤 `!resolved` ⇒ 已仲裁冲突前端不可见，不存在读取 / 回滚 / 再仲裁路径；
> 2. **不需要**：`resolve_conflict()` 的写入顺序是「先写活条目/副本（`KeepBoth` 新建 `item_id` 副本；
>    `TakeRemote` 且 `remote_deleted` 时 `trash_item_direct` 软删），**最后**才写 `resolved = true`」。
>    所以磁盘上 `resolved=true` 成立的那一刻，被继承的附件引用已落在活条目或回收站快照里，
>    而这两处本来就由本函数保活；逐级 `?` 上抛保证「写 conflict 失败 ⇒ `resolved` 仍为 false ⇒ 保活照旧」；
> 3. **留着有害**：已仲裁快照是**永久 dead weight**（每冲突最多 2×16 个引用、单 blob 20MB），
>    与 D-76「用户删掉的东西真的没了」直接冲突。
>
> **`KeepLocal` / `KeepBoth` 的分野正是「停的是 dead weight、不是活引用」的证明**：
> 前者用户明确丢弃远端快照 ⇒ 其引用应回收；后者把远端快照变成**新 `item_id` 的副本条目**、引用被继承 ⇒ 必须保活。
>
> **副作用（预期，非 bug）**：仲裁后 `reclaimable_bytes` 会**增大**——原来被已仲裁快照钉住的 blob 转入可回收。
> 设置页「保险库文件」卡片里变大的是**「可回收」**这一行；**「已删附件占」不受影响**（那是墓碑帧字节），
> 故 D-80 **不需要任何前端改动**（文案语义核对见 `06-ui-design.md` §6.11）。
>
> **新增守门测试**（`tests/compaction.rs`，3 例）：
> `gc_orphan_blobs_reclaims_blob_of_resolved_conflict`（GC 出口）/ `compact_reclaims_blob_of_resolved_conflict`（压缩出口）
> / **`resolve_conflict_keep_both_keeps_inherited_blob_alive`（防过修：`KeepBoth` 继承的 blob 必须仍活着且可读）**。
> 变异验证：把 `if entry.resolved { continue; }` 删掉 → 前两例红、第三例**仍绿**（证明停的是 dead weight）。
>
> **仍未释放**：已仲裁的 **conflict 记录本身**（约 1.2KB × N）仍在库里，与 backlog「已仲裁冲突永不删除」同一件事，本轮不做。

#### 3.14.5 IPC 契约变更（前后端）

| 命令 | 变更 |
|------|------|
| `add_attachment`（替代 `import_attachment`） | `{item_id, source_path, title?}` → 返回**父条目** `ItemDto`（`rev+1`）；校验：父存在且非 attachment 遗留、附件数 ≤ 16（`ERR_ATTACHMENT_LIMIT`，新增错误码）、单文件 ≤ 20MB、库容量 ≤ 200MB |
| `remove_attachment`（新） | `{item_id, attachment_id}` → 父 `ItemDto`（`rev+1`）；blob 交 GC |
| `export_attachment` | `{item_id, attachment_id, dest_path}`（原按条目定位改按附件定位） |
| `ItemDto` | 增加 `attachments: AttachmentMetaDto[]` |

`attachment-import.ts` 守卫逻辑**原样复用**：依赖注入签名仅 `importAttachment` → `addAttachment(item_id, …)` 一处替换，取消/成功/失败/守卫四分支契约不变。

#### 3.14.6 UI 改动清单

| 改动 | 说明 |
|------|------|
| ItemList：**回退 D-59 第三按钮** | 「添加附件」列表入口移除（`attachment-import.ts` 逻辑保留供详情页复用） |
| ItemDetail：新增附件区 | 登录/笔记条目通用：区头「添加附件」+ 行列表（文件名 / 大小 / MIME + 导出 + 删除）；空态隐藏 |
| AttachmentView → **AttachmentRow** | 由整页详情组件改造为附件行组件（导出前保留明文风险二次确认 + `ERR_BLOB_MISSING` 待同步态） |
| ItemEditor | 不含附件编辑（附件只在详情页操作，避免编辑-删除竞态） |
| Trash | 父条目行显示附件数徽标；还原/彻底删除均整父操作 |
| i18n | `item.new_attachment` 撤销；新增 `attachments.add/export/remove/empty/limit` 等键（中英双份） |

`docs/06-ui-design.md` 增加一行「待实现」注记（D-60 附件区布局）。

#### 3.14.7 任务分解（T-D60-xx）

| 编号 | 任务 | 涉及文件 | 前置 | 验收要点 |
|------|------|---------|------|---------|
| T-D60-01 | core 数据模型 + 迁移 + GC | `model/item.rs`（AttachmentMeta/attachments/wipe）、`store/repository.rs`（add/remove_attachment、export_attachment 重定位、孤儿 GC、migrate_legacy_attachments）、`constants.rs`（MAX_ATTACHMENTS_PER_ITEM=16）、`error.rs`（ERR_ATTACHMENT_LIMIT） | — | serde 往返含 attachments（default 兼容旧帧）；迁移后 blob 帧字节不变且导出一致；GC 不误杀 trash 快照引用与 KeepBoth 共享引用；附件数上限拒绝 |
| T-D60-02 | core 同步 + 删除适配 | `repository.rs`（attachment_blob_refs 扩展含 trash 快照）、`sync/blob_transfer.rs`（spool 键确认）、`sync/merge.rs`（复核父层逻辑不变） | T-D60-01 | blob 拉取清单来自父数组；父软删 blob 保留；KeepBoth 共享 blob 同步后双端可导出；多附件（≥3）逐个续传 |
| T-D60-03 | 前端 UI | `ItemList.svelte`（回退第三按钮）、`ItemDetail.svelte`（附件区）、`AttachmentView→AttachmentRow`、`attachment-import.ts`（注入改造+测试适配）、i18n 双份、`docs/06-ui-design.md` 注记 | T-D60-01 | 列表无独立附件入口；详情页增/导出/删全链路；守卫四分支测试照过；明文风险二次确认保留 |
| T-D60-04 | 回归测试 | `tests/sync_blob.rs`（9 例改：blob_ref≠item_id、多附件）、`tests/sync_deletion.rs`（8 例改：父删级联 GC）、`sync_conflict`（快照含附件）、`ipc-contract.test.ts`（锚点更新）、新增 attachment-in-item 用例 | T-D60-02, T-D60-03 | 全部旧用例修订后通过 + 新增：添加附件→父 rev+1→同步→对端详情可见→导出字节一致→删附件→GC；迁移用例（老库 attachment 条目→SecureNote 挂附件） |
| T-D60-05 | 文档收尾 | `docs/03-impl.md`（实现注记） | T-D60-04 | 与 §3.14 一致；标注混跑限制 |

#### 3.14.8 风险与边界

- **元数据膨胀**：附件数上限 16（常量），最坏 ~3.2KB/父帧，远低于 1MB 传输上限；文件名长度校验 ≤ 255B（导入时拒绝超长，`ERR_VALIDATION`）；
- **多附件顺序与续传**：blob 阶段逐 `attachment_id` 串行传输，spool 按其目录隔离，manifest 续传机制复用，互不干扰；
- **KeepBoth 共享 blob**：元数据复制、blob 共享，GC 引用集合保证不误删（T-D60-01/T-D60-04 显式用例）；
- **回收站**：整父呈现（附件数徽标），不提供单附件粒度还原——附件操作只在活跃父条目上；
- **迁移触发时机**：解锁后同步前执行，避免迁移产物与远端旧语义帧竞态；两端各自幂等迁移，收敛一致。

#### 3.14.9 v1.2.5 增量（D-66：Android `content://` 附件 + 已配对设备可辨识）

> 真机反馈两条：① Android 上**永远加不进附件**；② 两台手机在设备列表里都叫 `Android-3dd4390a`，无法分辨。前者是根因级缺陷，后者是可用性缺陷。

**A. Android 文件选择返回 `content://`，不能当路径读（缺陷根因）**

- **根因**：`@tauri-apps/plugin-dialog` 的 Android 实现在 `open()` 里返回的是 ContentProvider URI 字符串（`tauri-plugin-dialog` 的 `DialogPlugin.kt`：`uris.add(uri?.toString())`），而 §3.14.5 的 `add_attachment` 把它当文件系统路径直接交给 `Repository::add_attachment(&item_id, Path::new(&source_path), &title)`；core 第一步 `std::fs::metadata(source_path)?` 对 `content://…` 必然失败 → `CoreError::Io` → `ERR_IO` → 前端 toast「文件读写出错。」。
- **修法（分层，IPC 形状零变更）**：

| 层 | 变更 |
|----|------|
| `passwordms-core` | 新增 `attachment_name::filename_from_source(&str) -> String`：`content://` → 取最后一个 `/` 后的段 → 百分号解码（手写 `%XX`，不新增依赖）→ 再取 `/` 后段 → 剥 provider 前缀（`document/`、`primary:`、`raw:`、`home:`）；其它按路径取最后一段；空结果兜底 `attachment`。provider 不给真名时**原样保留**（如 `image:1000000123`），不伪造扩展名。 |
| `passwordms-core` | `Repository` 抽出私有 `insert_attachment(item_id, filename, data: &[u8])` 承载**全部**校验与落库逻辑；公开入口两个：`add_attachment(item_id, source_path, title)`（保留 `metadata` 预检避免超大文件进内存）与新增 `add_attachment_bytes(item_id, filename, data)`。校验**顺序与错误码逐条不变**（legacy → `AttachmentLimit` → `AttachmentTooLarge` → `VaultCapacityExceeded` → `filename_too_long`），大小判定由 `meta.len()` 改为 `data.len()`。 |
| `src-tauri` | `add_attachment` 新增注入参数 `app: tauri::AppHandle`；`#[cfg(target_os = "android")]` 且 `source_path.starts_with("content://")` 时，用 `tauri_plugin_fs::{FilePath, FsExt, OpenOptions}` 打开该 URI（插件 Android 侧走 `getFileDescriptor` 解析 content URI）读字节 → `add_attachment_bytes`；其余情况（桌面全部路径、Android 普通路径）原样走 `add_attachment`。Rust 侧直接调插件 API 不经 IPC 权限，**capabilities 无需改动**；`tauri::Url` 即 `url::Url` 的再导出，`Cargo.toml` 无新增依赖。 |

- **错误映射**：fs 插件 `open` / `read_to_end` 失败 → `std::io::Error` → `CoreError::Io` → 仍是 `ERR_IO`，**不新增错误码**。虚拟文件（云盘类 provider 不返回 fd）同样落 `ERR_IO`，用户可换用「下载到本地再添加」。
- **兼容性**：桌面行为零变化；Android 普通路径（极少数第三方选择器）仍走原路径分支；`tests/attachment_sync.rs` 三例语义不变。

**B. 已配对设备默认名加平台前缀 + 支持重命名**

- **默认名**：`resync::device_display_name` 由固定 `PasswordMS-<8 位短 ID>` 改为 `<平台>-<8 位短 ID>`（`Android-3dd4390a`、`Windows-a1b2c3d4`），平台由 `cfg!(target_os = …)` 判定（`Android` / `iOS` / `Windows` / `macOS` / 其它 `Linux`）。纯函数、无隐私字段；wire 校验（`server_id` / 指纹 / MAC）**一律不动**——展示名只作为 `client_name` / 配对通告名传输。外壳侧 `host_display_name` 改为委托同一函数，保证「配对时通告的名字」与「重同步 HELLO 里的 `client_name`」同源。
- **手动改名**：core 新增 `Repository::rename_paired_device(device_id, name) -> CoreResult<PairedDevice>`：trim 后空 → `Validation("device_name_empty")`；trim 后 > 64 **字节** → `Validation("device_name_too_long")`；设备不存在 → `NotPaired`；**只改 `device_name`**（`peer_ip` / `peer_port` / `sync_key_wrapped` / `fingerprint` / `paired_at` / `last_sync_at` 一字不动）；trim 后与现值相同不重写记录（与 `update_paired_addr` 的「值未变化不重写」一致）。
- **IPC 契约**：新增命令 `rename_paired_device { device_id, name } → void`（`DeviceDto` 结构不变，前端自行显示 `paired_at`）。

#### 3.14.10 v1.2.5 修订（D-69：配对核对改「会话码」口径，修正 D-68）

> 出包前的门禁抓到的**真红**（不是环境问题）：`bytes_entry_keeps_validation_order_of_path_entry`
> 失败 → 顺藤查出 `repository.rs` 里残留着 QA 变异实验的注入代码（文件名校验被提到大小校验之前，
> 与同函数文档承诺相反）。回滚后转绿。**同一轮还查出 D-68 首版实现是负收益**，一并修正。

**A. 残留变异实验代码（真红根因）**

- 现象：`insert_attachment` 里有一句 `// [QA125 变异实验 e] …` 的注入块，把 ⑥ 文件名校验搬到 ④ 大小校验之前。
- 为何是 defect 而不是测试错：同函数的 doc 明确承诺「④ 大小 → ⑤ 容量 → ⑥ 文件名」，测试守的正是文档；
  **代码与文档不一致时，按 §0 约定以文档为准 → 回滚代码**。回滚后 6/6 绿。
- 纪律：变异实验的注入块**必须带可检索标记**（本轮靠 `grep 变异实验|QA125|MUTATION` 唯一命中定位），
  且**回滚后才能提交**；`docs/03-impl.md` §13.5 已把「落盘前对抗实验」写成流程，需补「回滚自检」一步。

**B. D-68 → D-69：会话码必须是**同一次握手**绑定的量**

| 项 | 首版（D-68，错误） | 修正（D-69） |
|----|----|----|
| 电脑屏显示 | `identity.fingerprint()`＝`sha256(长期身份公钥)` | `SyncServer::session_code()`＝`sha256(host_eph 公钥)` |
| 手机屏显示 | `fingerprint_of(&Accept.host_pub)`＝`sha256(临时公钥)` | 同一把 `host_eph` 的公钥 → **同一个值** |
| 结果 | 两把不同密钥 → **必然不等**，每台设备都被报成中间人 | 构造上必然相等；中继式中间人两侧公钥不同 → 两码不等，**能抓出** |
| 临时密钥生成点 | `handle_stream` 每条连接新生成 | `SyncServer::start()` 一次生成（否则电脑屏在手机连上前无值可显示） |
| 命名 | 两处都叫 `fingerprint` | 拆成 `session_code`（每次变）与 `device_fingerprint`（稳定），DTO 字段同步改名 |

- **为什么不在 `Accept` 里带长期身份公钥**：那是 wire 变更 + 双方必须同时升级（老板已裁定本轮不做协议变更）。
  会话码方案不动 wire，且安全性上真正成立——它绑的是 ECDH 实际使用的公钥。
- **电脑端不再写对端指纹**：`HostSyncResult.fingerprint` 由 `fingerprint_of(&joiner_pub)`（临时公钥）改为**空串**。
  HELLO 只带 Joiner 的临时公钥，拿它算「设备指纹」就是 D-67 同类错误的重演。
  要恢复「电脑端也能看到手机长期指纹」需扩展 HELLO 携带 `joiner_identity_pub`（加法式、向后兼容）→ **backlog D-70**。
- **手机端设备指纹改真**：`JoinerSession.device_fingerprint` 改取**二维码 `QRPayload.fp`**（长期身份指纹，
  与电脑「本机身份」同源），不再取临时公钥指纹；写进 `PairedDevice.fingerprint` 后跨重配对稳定。
- **测试**：`crates/passwordms-core/tests/pairing_session_code.rs` 三例（替换原 `pairing_fingerprint_consistency.rs`）——
  ① 两屏同值且为 16 位小写 hex；② 会话码跨配对必变、设备指纹跨配对不变、两者不相等；③ 电脑端对端指纹留空。

#### 3.14.11 v1.2.6 增量（D-71 / D-72：Android `content://` 写盘 + 「取消不是错误」口径 + 导入模板）

> 客户真机反馈（2026-10-21）：「手机端想把附件下载保存时，提示：文件读写错误。」
> 「在设置有个导入功能，对导入内容的格式有什么要求？是否需要给用户一个模板？」
> 拍板：**导出＋导入一起修**；**加说明＋可下载 CSV 模板**，不做通用列映射向导。

**A. D-71：`content://` 是 ContentProvider URI，不是内核路径**

Android 的保存对话框（`ACTION_CREATE_DOCUMENT`）回传的是 `content://com.android.providers.…`，
内核里不存在对应路径 → `std::fs::write` 必返 `EINVAL` → `ERR_IO` →「文件读写出错。」。
**这不是权限问题，也不是分隔符问题**——SAF URI 根本不该被交给文件系统 API。

- **收敛点**：`src-tauri/src/content_uri.rs`（`read_to_bytes` / `write_bytes`），经
  `tauri-plugin-fs` 的 Android 实现 `resolve_content_uri` → `run_mobile_plugin("getFileDescriptor")`
  → `FsPlugin.kt::openAssetFileDescriptor(uri, mode)` 拿到 provider fd。
- **写模式 = `"wt"`**：`OpenOptions` read=false + write=true + **truncate=true** → `android_mode()` 映射 `"wt"`。
  必须 truncate，因为 `ACTION_CREATE_DOCUMENT` **已经建好**目标文件，新内容比旧的短时会留尾部残字节；
  不用 `"rwt"`（多要一个读权限，部分 provider 直接拒）；不设 `create`（文件由系统创建）。
- **读有上限**：最多读 `MAX_ATTACHMENT_BYTES` + 1 字节，超限即 `AttachmentTooLarge`——
  SAF 允许选中几百 MB 视频，先整份读进内存再让 core 拒掉是 OOM 级尖峰。
- **两个函数不加 `cfg` 门控**：桌面侧 `Fs::open` / `FilePath` / `OpenOptions` 与 Android 同名同签名，
  不门控才能在桌面目标下被 `cargo check` / clippy 覆盖（**没编过等于没写过**）；
  只有调用点的 `content://` 分支在 `cfg(target_os = "android")` 内。
- **capabilities 零改动**：Rust 侧直调插件 API，不经 IPC 权限层，`default.json` 不动。
- **打包侧证据**（插件源码）：`DialogPlugin.kt:199-235` 的 `saveFileDialog` 用 `ACTION_CREATE_DOCUMENT`
  且 `callResult.put("file", uri.toString())`；`FsPlugin.kt:60` 的 `openAssetFileDescriptor(uri, mode)` 支持写模式。
- **同源缺陷 D-66 的读路径**：`import_preview` 的 `content://` 也走 `read_to_bytes`（读字节后按 UTF-8 解析，
  非法编码复用 `ERR_IMPORT_PARSE`，**不新增错误码**）。

**B. D-72：把「取消归一」从约定升级为代码**

原约定（`src/lib/attachment-import.ts` 的 doc）是「Android 取消是 reject，由**注入方自行** try/catch 归一成 null」。
把归一动作外包给每个调用点，就必然漂移——实测三处、三种错法：

| 调用点 | 错法 | 用户可见后果 |
|---|---|---|
| `vault-export.ts` `pickDestination()` 在 try 外 | reject 逃出函数（`Settings.svelte` 无 try/catch） | **未捕获 rejection**，点取消毫无反应 |
| `AttachmentView.exportAttachment` try 包住 save + IPC | 取消被当失败 | **点取消弹红「内部错误」** |
| `ImportWizard.downloadTemplate` 空 `catch {}` | 真失败被吞 | 写盘真失败 → **静默无反应** |

- **唯一口径**：`src/lib/save-dialog.ts::runSaveToDestination(deps)` →
  `SaveOutcome = cancelled | saved | failed{messageKey}`。三处全部改走它。
- **判定矩阵（结构判据，不赌插件行为）**：

  | 抛点 | 语义 | 结果 |
  |---|---|---|
  | `pickDestination()` 抛（Android reject） | 用户取消 | `cancelled`，**`write` 不被调用** |
  | `pickDestination()` 返回非字符串/空串 | 用户取消（桌面 null） | `cancelled`，`write` 不被调用 |
  | `write(dest)` 抛 | 真实失败 | `failed`，`messageKey = toAppError(err).messageKey` |
  | `write(dest)` 成功 | 成功 | `saved` |

  这样**不需要知道**插件取消时走 reject 还是 resolve(null)，两条都判成取消。
- **IPC 形状**：新增命令 `save_import_template(app, dest_path, texts)`（D-112 起 `texts` 为本地化示例文本，此前仅 `dest_path`），`rename_all = "snake_case"`，
  wire 键 `dest_path`（见 `docs/03-impl.md` §14 不变量：全仓 50 → **51** 处 `tauri::command`，51 处带该属性，0 例外）。
  **既有命令形状一律未变**（`export_attachment` 仍是 `item_id` / `attachment_id` / `dest_path`）。

#### 3.14.11.1 v1.2.16 增量（D-94：Android「从备份恢复」读 `content://`——读方向补齐）

- **症状**（主人真机实测 2026-10-24）：手机上锁屏页「从备份恢复」选电脑导出的 `.vault`（33 KB，magic `PMSVAULT` 正常，导出侧无问题）→ toast「文件读写错误」。
- **根因**：D-55 写该功能时只考虑了桌面——桌面 `open()` 返回真实内核路径；Android 的 SAF `open()` 返回 `content://…`，`restore_backup` 直接 `Path::new(&source_path)` 交给 core `restore_backup_from`（`fs::metadata` / `File::open`）必失败 → `ERR_IO`。**全仓 4 个选文件入口最后一个漏网点**：`import_cmd`（D-87）、`keydoc_cmd`（D-66/D-71）都已接 `content://` 分支，唯独它没有。
- **修法（A 案，主人拍板）**：
  - `content_uri` 新增 `copy_to_file(app, uri, dest)`：经 provider fd **`io::copy` 流式**拷到本地路径——**不用 `read_to_bytes`**，因为库文件没有 20 MB 附件上限可依（实测存在 24 MB 未压缩库，会被 `ERR_ATTACHMENT_TOO_LARGE` 误拒），且整库进内存是无谓尖峰；流式拷贝内存占用与文件大小无关、无大小上限。
  - `restore_backup` 检测 `source_path.starts_with("content://")`：拷到 vault 同目录 `passwordms.restore-<unix_ms>.tmp` → 走既有 `restore_backup_from` 的**尺寸 / magic / 整库帧流**全套校验落位（校验逻辑零重复）→ **成败都删临时文件**；桌面该分支恒假，行为零变化。
  - 覆盖保护（`.bak-<unix_ms>`）与锁态校验在分支之前，语义不变。
- **错误码 / IPC 形状**：零新增——`ERR_IO` / `ERR_VALIDATION` 语义不变，`restore_backup(source_path)` 契约不变（全仓仍 51 处 `tauri::command`）。
- **验证口径**：`cargo check --workspace --all-targets` 0 错 0 警；真机装新 APK 重复主人原操作（选备份 → 确认 → 解锁）为最终判据——**「每个交付渠道都要端到端走过」**（D-82 教训在读方向上的再次适用）。
- **模板一致性靠测试锁，不靠人眼**：`passwordms_core::import::template::CSV_TEMPLATE` 的表头
  必须与 `import::bitwarden_csv::KNOWN_COLUMNS` **逐字同序**，且模板自身导入 0 跳过、示例行不溢出到 `custom_fields`
  ——`crates/passwordms-core/tests/import_template.rs` 逐条断言。**改模板必须同步改测试**，
  否则「模板改了、解析器没跟上」会静默把示例值映射到别的列。
- **前端不拼模板**：内容只从 core 常量取，避免两份文本各自演化。

#### 3.14.12 v1.2.7 增量（D-73：设置页旁路数据的失败隔离 + 行类型分离）

> 客户装 1.2.6 真机反馈（2026-10-21，三条截图）：「关于」组窄屏排版散乱 / 设置里「保险库文件」没看到文件、
> 版本号也没出来 / 导入面板里按钮写着「选择导出文件」且「预览」不可用。
> 本轮查出**两处独立根因、五个症状**——修好第一处不会让第五个自动消失，这是本条要留给后人的关键区别。

**A. 根因一：`structuredClone` 遇 `$state` 代理必抛，而它站在 `$effect` 第一行**

`Settings.svelte` 的 `$effect` 首行原为 `draft = structuredClone(settings.value)`。
`settings.value` 是 Svelte 5 的**深层响应式 Proxy**，而 HTML 结构化克隆算法遇 Proxy 必抛 `DataCloneError`
（实测报文 `#<Object> could not be cloned.`）。异常抛在首行 ⇒ **该 `$effect` 后续全部逻辑静默不执行**，
且**不冒到界面**（effect 内的异常不会渲染成错误页）。于是同一个 bug 长出四个症状：

| 症状 | 表现 | 为什么看起来像四个 bug |
|---|---|---|
| ① | 「关于 → 版本」显示 `—` | 版本读取根本没跑 |
| ② | 「保险库文件」卡片只有标题、下面全空 | 库文件信息根本没跑 |
| ③ | 自动锁定下拉**恒为默认档**（设 1 或 15 分钟都显示 5 分钟） | 档位读取根本没跑；**客户尚未发现这一条** |
| ④ | 「读失败」与「本来就没有」长得一模一样（都是空白） | 每块的 `.catch(() => null)` 把失败吞成空状态 |

- **修法一**：深拷改 `$state.snapshot(settings.value)`。
- **修法二（更重要）**：三块旁路数据抽到 `src/lib/settings-page.ts::loadSettingsPageSide`，
  **各自 try/catch、失败显式建模**（`fileInfoFailed` / `autoLockFailed`），并明确**串行**而非 `Promise.all`
  ——`Promise.all` 的语义就是「一个 reject 全丢」，正是本缺陷要消灭的形状。
- **为什么不能用浅拷贝 `{ ...settings.value }`**：内层 `generator` / `health` 仍是代理，
  下面 `bind:value` 改草稿会**直接写穿到 store**，变成「取消不掉」的隐藏 bug。
- **`NaN` / `Infinity` 判失败**：后端若回非有限数，`<select>` 里没有任何 option 与之相等，
  会渲染成一个**没选中项的下拉**——又一种「看起来正常、其实不对」。

**B. 根因二：裸 `structuredClone` 会被人再写一次 → 静态守卫**

`src/lib/state-clone-guard.test.ts` 两例：① 扫描面有效性（`> 60` 个文件、锚定到被守对象）；② 全仓
`src/**/*.{svelte,ts}` 零命中（正则刻意避开注释与正则字面量自身）。
**配套约束**：注释里也不要写出调用形态，否则会命中守卫自己的正则（本项目实测踩过）。

**C. 症状⑤：窄屏 `.set-row` 是「一刀切」，但它只为控件行而写**

`Settings.svelte` 的 `@media (max-width: 767px)` 里 `.set-row { flex-direction: column }` 的**真实理由**
是控件行里的 `.set-ctl`（160px 固定宽）在 390px 会溢出，必须堆叠。但选择器没限定范围，
把**纯信息行**（标签 / 值）与**按钮行**一起扫了 ⇒ 信息行被拆成上下两行左对齐、按钮被拉成三条满宽竖排。

- **修法**：行类型**显式标注** —— 信息行 `.is-info`（`row` + `baseline` + `space-between`，值右对齐可收缩）、
  按钮行 `.is-actions`（`row` + `wrap` + `flex-start`），覆盖规则写在**同一个 `@media` 块内、`.set-row` 之后**。
- **不许用 `:has()`**：WebView 版本差异，别赌。
- **桌面 ≥768px 逐像素不变**，且有**结构性证明**：媒体块**外**零 `.is-*` 规则
  （`src/lib/settings-row-type-guard.test.ts` 8 例，含「覆盖规则在同一媒体块内且在 `.set-row` 之后」的花括号配平判据）。
- **特异度而非顺序才是主保险**：`.set-row.is-info` (0,2,0) > `.set-row` (0,1,0)，
  即使将来有人调换源码顺序也依然生效；测试里硬绑顺序只是第二道保险。
- **负向断言必须先剥注释**：`expect(SRC).not.toContain(':has(')` 会命中解释「不许用 `:has()`」的注释本身，
  所以守卫先 `stripComments()` 再扫代码（**不是**删注释让它变绿——注释是留给下一个人的，删掉等于把陷阱复原）。

**D. 旁带：两处误导性 UI**

- `import.choose_file` 文案原为「选择导出文件」——本应用导出的是 `.vault` 加密备份，**该入口不导入 `.vault`**，
  文案与实际功能相反。改为「选择要导入的文件」。
- 「预览」原先 `disabled={sourcePath.length === 0}`：**置灰且无任何解释**，用户不知道要先选文件。
  改为可点 + 未选文件时 `import.choose_first` 提示。
- **`loading` 态刻意保留**：去掉它，`fileInfo === null && !fileInfoFailed` 只能渲染空白，
  恰好复现症状② 的观感。**已知无害的闪烁 < 会与 bug 混淆的空白**。

**E. 门禁读数报废事故（方法论，值得记形状）**

本轮第一遍门禁 20:24:11 启动、报 **PASS 7 门 / EXIT=0**，但**读数作废**：
前端阶段只跑 12s（20:24:2x 结束），而 `settings-row-type-guard.test.ts` 到 **20:25:30** 才创建，
那轮**少跑 8 例却依然全绿**。**门禁不会告诉你它没看到的东西**——判据不是总判定（两轮都 PASS），
而是**文件时点 vs 门禁各阶段时点**。重跑后 24 文件 164 例才对上账（21+3 / 149+15）。
配套做法：并行工程师交付**静止树 sha256 基线**，跑门禁前后各校一次。



### 3.15 v1.2.4 增量：对端地址自愈 + 被动同步通知 + 网络错误文案方向中性（客户「根因 A/B/C」复盘）

> 客户实测（2026-10-21）：手机删某条目一个附件并在手机上点「立即同步」（该方向成功）→
> 电脑上该附件仍在；随后在电脑点「立即同步」→ 报「无法连接到电脑…」且确实连不上。
> 本文按客户的「根因 A/B/C」口径记录（正式缺陷编号待主理人编号）。

#### 3.15.1 根因 A 附件删除传播：**已被证伪**

`Repository::remove_attachment` / `export_changes` / `merge::apply_changes` 三者组合是**正确的**，
双端 loopback 已证明删除能过线（`crates/passwordms-core/tests/attachment_sync.rs`，3 例）：
B 删附件 → B 发起重同步 → A 侧 `attachments.len()==1`、剩的是第二个、`attachment_blob_refs()`
不含被删 id；反向（A 删 → B 消失）同样成立；连续两轮同步不复活。

因此客户看到的现象**不是数据没过去，而是数据过去了但本机 UI 没刷新**——见 §3.15.3。

#### 3.15.2 根因 B 对端地址自愈

`peer_ip` / `peer_port` 此前只在配对那一刻写一次。对端换 IP（DHCP 续租）或监听端口 rebind
（`resync.port` 被占用后 fallback）后，本机记录永久失效 → `sync_now` 永远连不上，只能重配对。

修法见 §3.5 增量 5/6：`ResyncHello.sync_port` + 成功后 `update_paired_addr`。
测试：`tests/resync_addr_heal.rs`（① 记录被改坏 → 一次成功同步自愈；② `sync_port==0` 老客户端
保留原端口；③ `update_paired_addr` 三契约）。

#### 3.15.3 根因 A 的真实落点：被动同步后的 UI 陈旧

`src-tauri/src/state.rs::spawn_resync_worker` 在 `poll_once` 成功后只写日志，**不通知前端**；
而本机自发起的那条路径（`sync.svelte.ts::syncNow`，D-47）是会 `vault.load()` 的。
于是「对端推过来的变更」永远停留在库里、不上屏，表现为「附件还在」。

修法：worker 在成功轮询后 emit `sync://applied`（载荷仅计数），前端
`src/lib/sync-applied.ts::shouldReloadAfterPeerSync` 判定后 reload 条目/设备/冲突。
三项全零（对端只是取走本机数据）不刷新，避免白刷列表打断编辑。

#### 3.15.4 根因 C 网络错误文案方向中性

`err.network_unreachable` 原文写死「无法连接到**电脑**…请确认**手机与电脑**…」，电脑端触发时
方向完全反了。改为「无法连接到对端设备…若刚刚在另一台设备上同步成功过，请再试一次」。
`network_timeout` / `network_refused` / `network_broken` 复核为方向中性，未改。

---

### 3.16 配对的「成功态」与核对时机（增量设计 · D-75）

> 客户真机实测：手机扫码成功后，手机端弹出「请核对电脑上的会话码」，**而电脑上的配对面板在
> 这一瞬间已经关闭**。重新生成二维码又是一个新会话、新码 —— 永远差一步。
> ⇒ D-69（两屏显示同一次会话的会话码，防中间人）**从未真正可用**。

#### 3.16.1 根因：不是显示 bug，是「完成事件与核对时机互斥」

协议层没有任何问题（两端算的是同一把 `host_eph` 公钥的哈希，见 §3.5 / D-69）。缺陷在**前端状态机**：

```text
后端 sync_cmd.rs（Host worker 的两种结局，同一个事件、不同载荷）
  成功： *state.pairing = None;  emit("pairing://ended", "completed");  break;
  失败： *state.pairing = None;  emit("pairing://ended", "aborted");    break;

前端 sync.svelte.ts（旧实现，只有「结束」一个语义）
  listen("pairing://ended", (ev) => {
    sync.pairing = null;                 // ← 两种结局都清空
    if (ev.payload === "aborted") sync.pairEnded = true;
  });
```

会话码的唯一载体是 `PairingDto.session_code`，而它挂在 `sync.pairing` 上。于是：

- 手机端要核对的提示，出现在 `join()` 成功**之后**（即 `completed` 事件之后）；
- 电脑端能核对的码，在 `completed` 事件**到达的那一帧**就被删掉。

**「会话结束」这个事件把「核对会话码」这个动作的前提一起删掉了** —— 功能写着，用不上。
这类缺陷的特征是：每一段代码单独看都对，**错在两者的时间关系**。

#### 3.16.2 修法：按载荷分流，成功时保留 `pairing`

判定收敛进纯函数（`src/lib/pairing-state.ts::nextPairingState`），store 只负责套用：

| 载荷 | `clearPairing` | `completed` | `ended` | 面板 |
|---|---|---|---|---|
| `"completed"` | **false** | true | false | 成功态（保留会话码待核对） |
| `"aborted"` | true | false | true | 失败态（提示重新生成二维码） |
| 空串 / `null` / 未知 | true | false | true | 同上（**保守**，不留假成功态） |

前端状态机（四态）：

```text
         startPairing()                 completed（保留 pairing）
  [空] ───────────────▶ [已生成] ───────────────────────▶ [配对成功·待核对]
   ▲                     │  aborted / 未知                 │
   │                     ▼                                │ endPairing()（「完成」按钮）
   └──────────────── [失败/结束] ◀────────────────────────┘
```

#### 3.16.3 为什么不把会话码「另存一份」

最省事的写法是事后 `sync.lastPairSessionCode = sync.pairing?.session_code` 再清屏。**不采用**，
理由是 D-31 的教训：**同一件事只能有一个状态源**。多存一份就有两份可能不同步的真值
（例如重开一轮时清了 `pairing` 忘了清副本 → 旧码挂在新二维码旁边，比不显示更糟）。

保留 `pairing` 本身则天然满足：会话码与「这一轮会话」同生命周期，清它一次就干净。
`pairCompleted` 只是**这一个对象的状态标注**，不是第二份数据。

#### 3.16.4 不变量（改动时守住）

1. `pairCompleted === true` ⇒ `pairing !== null`（成功态必须还有码可看）。
2. `completed` 与 `ended` **不可能同时为真**（两种结局互斥）。
3. 退出成功态的四条路（点「完成」/ 重新生成 / 取消 / 锁定）都必须把 `pairCompleted` 复位为 `false`。
4. `endPairing()` **不发 IPC**：后端在发 `completed` 之前已 `*state.pairing = None` 并 `break`，
   会话本就不存在；从前端再调 `cancel_pairing` 只会制造一次无意义的错误路径。
5. 成功态**不渲染二维码**（已扫过；留着会让人以为还能再扫一次）。
6. 未知载荷按失败处理 —— 与「读失败不得伪装成正常值」（D-27 / D-73 同族）同一条纪律。

#### 3.16.5 测试锚点

`src/lib/pairing-state.test.ts`（16 例，三层）：

| 层 | 断言 |
|---|---|
| 纯函数 | 三种载荷的完整返回值；未知载荷逐个（`''` / `null` / `undefined` / `'COMPLETED'` / `'ok'` / `0` / `{}`）必须落失败；三字段互斥 |
| store 结构 | `listen(PAIRING_ENDED_EVENT, …)` 的回调里 `nextPairingState(` 的下标**早于**任何 `sync.pairing = null`，且清空必须在 `if (next.clearPairing)` 内；不得退回 `=== 'aborted'` 字面量比较；`pairCompleted` 有 ≥3 处复位 |
| 组件结构 | `PairShow.svelte` 的成功态分支存在、判定含 `pairing !== null`、**位于** `qr_svg` 分支之前、块内含 `session_code` / `pair_completed_hint` / `endPairing()` / `pair_done`、块内**不含** `qr_svg` 与 `sync-qr` |

> 组件层用 `?raw` 扫源码而不是渲染快照：本项目没有 DOM 测试环境，且要守的是
> 「分支存在且顺序对」，不是「像素长什么样」。
> ⚠️ 块内负向断言（不含 `qr_svg`）必须配一条 `block.length > 200` 的前置断言 ——
> 否则整块被删时切片为空，负向断言会恒绿（假绿三形态之二）。

---

### 3.17 局域网自动发现（增量设计 · 缺陷 D-74）

#### 3.17.1 要解决的结构性死锁

`resync_with_peer` 用**配对那一刻静态记录的** `peer_ip` + `peer_port` 直连；而地址自愈
（§3.15.2）发生在**接收方**，且必须 HELLO 已到、即**连接已成功**才执行。

⇒ 换网络后双方 IP 全变：A 用旧 IP 连 B 失败、B 用旧 IP 连 A 失败 ⇒ **双方都连不上，
自愈永远触发不了** ⇒ 唯一出路是重新扫码配对。2026-10-21 主人实测确认（家里 WiFi 从公司
网段切到 `192.168.31.0/24` 后 6 次 `sync_now` 全失败、零成功）。

发现机制的作用不是"扫描设备"，是**打破这个死锁**：让发起方在连接失败后自己找到对端新地址。

#### 3.17.2 三条设计取舍（不可按直觉改）

1. **不做被动宣告。** 只在用户触发同步、且静态地址连接失败时广播**一次**。周期性广播
   「我在线 + 我的 id」会让同网段的人持续追踪"这台机器装着密码管理器、此刻在活动"。
2. **报文里不放 `device_id` / 设备名 / 指纹。** 线上只有 32B 随机数 `q` 与 32B **不可区分
   tag**（`HMAC-SHA256(sync_key_i, ...)`）。陌生设备算不出 tag ⇒ **不会应答**，既不能确认
   本机装了 PasswordMS，也不能把应答当放大器。**发现不是匿名的，但对未配对者不可见。**
3. **不用 mDNS。** 需新增 crate（违反 §8.1 的依赖白名单硬限），Android 还要 NSD/`MulticastLock`
   与权限；而 `if-addrs`（已是直接依赖）的 `Ifv4Addr.broadcast` 现成可用。

#### 3.17.3 常量（`constants.rs`）

| 常量 | 值 | 说明 |
|---|---|---|
| `DISCOVERY_PORT` | `47823` | UDP 固定端口，双端必须一致。**刻意避开** `SYNC_PORT_MIN..=SYNC_PORT_MAX`（49152-65535）——那是可漂移的 TCP 重同步口，这是编译期常量 |
| `DISCOVERY_WAIT_MS` | `1500` | 广播后等应答时长；200ms 轮询节拍下最多错过 7 个周期 |
| `DISCOVERY_MAX_TAGS` | `8` | 单请求携带 tag 上限，超出截断不报错 |
| `DISCOVERY_VERSION` | `1` | 报文 `v` 字段 |

#### 3.17.4 报文（`sync/discovery.rs`）

| 方向 | 字段 | 类型 | 说明 |
|---|---|---|---|
| 请求（广播） | `v` | u16 | `DISCOVERY_VERSION` |
| | `q` | base64url(32B) | 本次等待窗口内唯一，**逐次随机**，不持久化 |
| | `tags` | `string[]` | 每项 = `b64url(HMAC-SHA256(sync_key_i, "pmsd1/req" ‖ q))` |
| 应答（单播） | `v` | u16 | 同上 |
| | `q` | base64url(32B) | 原样回显 |
| | `tag` | string | `b64url(HMAC-SHA256(sync_key_i, "pmsd1/resp" ‖ q))` |
| | `port` | u16 | 应答方 resync TCP 口（`ResyncServer::port()`） |

- 两个 label（`pmsd1/req` / `pmsd1/resp`）**必须不同**：防把应答当请求回灌。
- tag 比较一律**常量时间**（`subtle` / `handshake::verify_mac`），不做普通 `==`。
- 应答的源 IP 就是请求方要的新地址，`port` 就是要的新端口。

#### 3.17.5 发起侧

```text
静态地址 connect ──成功──▶ 原流程（HELLO 起一字不改）
      │失败（stage_io "connect"）
      ▼
discover_peer(&sync_key, 1500ms)      # 复用第 2 步已解出的那把 sync_key，不重复 unwrap
      ├─ Some((ip, port)) ──▶ 用新地址再 connect
      │                          成功 ──▶ 原流程
      │                          失败 ──▶ stage_io(第二次的 err, "connect")
      └─ None ──▶ 返回**静态那次的原样错误**（不包装、不新造）
```

- 发现无果时**返回原始 connect 错误**，而不是造一条"请重新配对"：发现为空恰恰说明广播
  出不了本网段（不同网段 / AP 隔离 / 热点），此时指引用户重配对是**错误方向**（重配对后
  照样不通）；而 `err.network_unreachable` 的文案是「确认同一个 Wi-Fi」+「防火墙放行」+
  「仍失败请重新配对」，三条逐级可行动。
- **阶段名必须保持 `"connect"`**：`error.rs` 对 `stage == Some("connect")` 是精确匹配，
  换成别的名字会掉进 kind 归一化分支、可能落到 `err.io`（就是 D-57 修掉的那个坑）。
- 成功后（`resync.rs` 收尾）若用到的地址与记录不同，调 `update_paired_addr` 写回——
  规则与 §3.15.2 完全一致：**只在校验通过且同步成功之后**写，失败路径不得篡改地址（防投毒）。

#### 3.17.6 应答侧（与重同步共用 200ms 轮询）

- `ResyncServer` 新增 `discovery: Option<DiscoveryResponder>`：**UDP bind 失败不致命**，
  置 `None` 并由上层记一行 `[discovery] udp bind failed, auto-discovery disabled`。
  发现功能**不允许**把解锁/建库流程拖死。
- `poll_discovery_once(&Repository) -> usize`：非阻塞收最多 8 个包 —— 解析失败 / `v` 不符 /
  `q.len() != 32` / tag 全不匹配 → 丢弃；**一个都不匹配就静默不回包**。
- 逐条解 `sync_key_wrapped` 比对 tag；单条解包失败**跳过该条继续**，不整轮失败。
- 限速：每源 IP 5 次/秒、全局 50 次/秒；统计的是**被允许处理的请求**（未命中 tag 的也占额），
  偏保守。
- **自应答过滤（不可删）**：本机 responder 与 requester 同进程同网卡，广播包自己的 socket
  也收得到；且两端持**同一把 `sync_key`** ⇒ tag 校验**天然拦不住自己**。必须以
  「`resp.port == 本机监听口` **且** `is_local_ip(源 IP)`」复合条件丢弃，否则客户端会把自己
  当对端、连自己、握手必挂。
- 调用点是 `state.rs` 的重同步轮询线程，在 `poll_once` **之前**一行，返回值**不参与**
  外层 `match`（发现出错绝不能让轮询循环 `break`）。

#### 3.17.7 平台与权限

UDP 广播收发**不需要**额外权限，`AndroidManifest.xml` 保持只有 `INTERNET` + `CAMERA`。
部分厂商 ROM 会过滤广播；若真机收不到，行为退化为「和没有发现功能一样」（静态地址 + 报
`err.network_unreachable`），**不比现状更差**。不预先加 `MulticastLock`。

> **真机实测（2026-10-22，v1.2.10）：通过。** 主人换网络后**未重新扫码**即同步成功，
> Android 侧广播收发未被 ROM 过滤，未触发 `[discovery] udp bind failed`，无需补任何权限。


#### 3.17.8 已知不覆盖

- 「只判端口、不判 IP」这条错误改法在回环上**构造不出**可分辨用例（本机来源必然是本机 IP），
  故只能测「整段删除」与「弱化成只判 IP」两条错路。
- 无签名报文 ⇒ 同网段攻击者可**丢弃/篡改**应答（DoS），但不能伪造有效应答（无 key）；
  也不影响握手（认证仍在 §3.4/§3.8 的通道内完成）。

### 3.18 D-91 全仓审查修复（2026-10-24，软件工坊四角色并行审查驱动）

> 背景：主人委托全仓审查（安全审计 + 正确性排查 + 测试门禁 + 代码评审），报告见
> `deliverables/gstack/full-audit-passwordms-2026-10-24.md`。本轮修 P0 + 小修包，
> O1（同步分页 + peer-base 过滤）与 O2（同步锁粒度）两个大项**先出方案、下轮实施**。

#### 3.18.1 🔴 R1：blob spool 路径穿越（安全审计 F-001）

- **根因**：`BlobMeta::from_message` 对对端 Meta 报文的 `blob_ref`（自由 String）只校验
  wrapped_key/nonce 形状与分片数学，`Spool::open_or_create` 直接 `root.join(&blob_ref)` ——
  绝对路径整体替换 root、`..\..\` 穿越，恶意配对对端可在宿主机任意路径 create_dir_all +
  落盘受控密文切片（≤768 KiB/片）。
- **修法（两层）**：① `from_message` 强制 `blob_ref` 过 UUID 解析（`record_id_to_bytes`，
  失败即 `Validation("uuid")`）并回写规范化小写连字符形式；② `Spool::open_or_create`
  纵深校验 `dir.parent() == Some(root)`（`Validation("blob_ref_path")`），即便上游被绕过
  也一票拦截。`cleanup_orphan_spools` 本就只按 root 下一级目录名清理，无穿越面。
- **守门**：`blob_transfer.rs` 单测 4 例（七种穿越形态全拒 / UUID 规范化 / 纵深兜底 /
  规范形式放行）。

#### 3.18.2 🔴 R2：回收站还原可静默覆盖同步数据（丢数据）

- **根因**：对端「删除后又编辑」（rev 超墓碑）→ 本端同步复活，但旧 `TrashEntry` 不清；
  用户「还原」取旧快照 rev+1 + updated_at=now 直写 live 帧 → 顶掉同步来的新版本，
  且 LWW 把覆盖传播回对端（两端丢）。
- **修法（根因 + 兜底）**：① merge 复活路径（`put_item_direct` 后）调新增的
  `Repository::drop_trash_snapshots(item_id)` 清陈旧快照（幂等，blob 引用集合随之收窄、
  孤儿交既有 GC）；② `restore_item` 前置守卫：`items.contains_key(item_id)` 即拒，
  返回新错误变体 `CoreError::RestoreConflict`（`ERR_RESTORE_CONFLICT` / `err.restore_conflict`）。
  配套：`resolve_conflict`「采用远端（删除）」在 live 不在且回收站已有同 item_id 快照时
  **跳过软删**（防重复回收站条目，审查 C2）。
- **守门**：`tests/trash_restore_guard.rs` 4 例（守卫拒还原且 live 原样 / 清快照幂等 /
  不产重复条目 / live 在场软删行为不变）。**blob 引用集合不变量不受影响**：新增的
  drop_trash_snapshots 只**收窄**引用集合，收窄后孤儿由 gc_orphan_blobs 兜底，4 处
  消费方口径不变。

#### 3.18.3 小修包（O3 / C1 / C4 / C5）

- **O3 谎报已锁**：`session.lock()` 返回布尔；`MainLayout.lockNow` 失败时**不** reset
  store、**不**切锁屏（后端仍 Unlocked、明文在内存时，界面必须保持主界面而非假锁屏）。
- **C1 记错对端 IP（D-44 回归）**：`client::pair` 记录**实际连通**的候选 IP 持久化
  （此前恒记主选 `qr.ip`，多网卡 Host 在 fallback 候选握手成功时首次 sync_now 必败一次）。
- **C4 解锁维护静默吞错**：`Session::unlock` 的 migrate/purge/gc 三项失败收集为
  `maintenance_warnings`，由 `vault_cmd::unlock` 写文件日志（`[maintenance] …`，仅错误码）。
- **C5 锁中毒静默退出**：resync worker 与 pairing worker 的锁中毒分支一律
  `append_log` 留痕后再退出（此前本机会从此不接受被动同步且无线索可查）。

#### 3.18.4 已备案待办（下轮）

- ~~O1~~ / ~~O2~~：已于 §3.19 / §3.19.3 全量实施（O1 全量；O2 两步——被动 resync
  与主动 sync_now/配对链路均已三段式）。

---

### 3.19 O1 同步分页 + 发送水位；O2 第一步被动 resync 三段式（2026-10-24）

#### 3.19.1 O1：发送水位 + cursor 分页 + 帧预算（修复 🟠-1「大库同步永久失败」）

- **发送水位**：`DeviceSyncState.sent_watermark: HashMap<record_id, rev>`（serde default
  兼容老状态帧）。`export_changes` 只导出 `rev > 水位` 的 live 记录（墓碑走 base 过滤
  不变）；发送成功后由 `merge::note_sent_changes` 推进——与 `note_sent_tombstones`
  同一失败安全语义（中途失败 `save_sync_state` 不执行，随会话丢弃，下轮重发，宁多勿丢；
  接收方 merge 幂等跳过）。首次配对（水位为空）= 全量首推，行为不变。
- **cursor 分页**（发送方驱动，接收方零新语义）：`sync/paging.rs` 把确定性排序
  （`(record_type, record_id)`）的变更清单按帧预算切块；每页一个 Push 信封（seq 单调
  递增），`cursor` = 本页之后剩余条数（0 = 最后一页）；接收方 `recv_push_pages` 循环读
  到 cursor==0，聚合后**一次** merge（页面间记录互不重叠，等价单帧）。老端单页 Push
  cursor 恒 0 → 一帧即出，天然兼容；大库在新旧混布下与旧版同样失败（`frame_too_large`），
  无回归。4 处交换点（client / resync 发起方 / server / resync 响应方）统一走
  `send_push_pages` / `recv_push_pages` / `recv_done`；Done 的 cursor 语义归零（旧值
  `last_recv_seq` 从未被消费）。
- **帧预算预检**：`PAGE_BUDGET_BYTES = SYNC_MAX_FRAME_BYTES − 96 KiB`；记录上线估算
  `ciphertext×2 + 512`（双重 b64 膨胀 16/9 按 2 倍保守计）。单条超预算 →
  `Validation("change_too_large:<id>")` 指名报错（正常 256 KiB 笔记估算 ~516 KiB，
  到不了这一步）。
- **守门**：`tests/sync_pagination.rs` 5 例（1500 条分页全到位且配对后导出为空 /
  增量 50 条只发增量 + 连接失败水位不推进 + 修复后收敛 / 老格式无 cursor 字段互通
  且恰好停在 cursor=0 / 分页收发循环停在最后一帧 / paging 单测 4 例）。

#### 3.19.2 O2 第一步：被动 resync 三段式（修复 🟠-2「同步期间 UI 冻结」）

- **`sync/access.rs`**：`RepoAccess::with_repo(RepoJob)` —— 锁内段入口（毫秒级），
  `DirectAccess` 供主动路径/测试（repo 已持锁）直连。
- **`ResyncServer` 拆分**：`poll_accept`（不持任何会话锁）+ `handle_connection`（三段式）：
  锁内段①配对校验+解 sync_key+状态初值 → 网络段握手/收 Push → 锁内段②vault 未变校验
  （device_id 比对，锁定后换库即弃）+ 合并 + 导出回包页 → 网络段回 Push/收 Done/blob →
  锁内段③持久化游标+地址自愈。`poll_once` 保留为兼容包装（DirectAccess）。
- **blob 阶段去持锁化**：`pull_missing` 锁内段①取缺失清单+容量快照 → 网络收片落
  spool（不需要 repo）→ 锁内段②批量 finalize；`serve_pull` 按 `SERVE_BATCH_BYTES=32MB`
  分批「锁内读帧出锁 → 锁外发送」。
- **自动锁定竞态裁定**：段中会话被锁 ⇒ `with_repo` 返回 `Err(NotUnlocked)` ⇒ 整段丢弃、
  对端下轮重发（宁多勿丢）；密文暂存随连接丢弃，**无明文残留**（解密只发生在锁内段②）。
- **同步互斥**：`AppState.sync_busy: AtomicBool`——被动 worker 与主动 `sync_now` 互斥
  （三段式后两者可真并发，各自推进同一份 DeviceSyncState 会产生 seq/base 竞态）；
  抢不到的一侧快速失败 `ERR_SYNC_BUSY`（`err.sync_busy` 双语文案）。
- **O2 第二步（待办）**：主动路径 `sync_now` 与 pairing worker / `sync_join_impl`
  的三段化（当前仍整场持 session 锁；`resync_with_peer` 客户端侧未拆段）。
- **已知取舍**：pull_missing 容量预检用锁内段①的文件大小快照（偏差=段②合并增量，
  以字节计）；断连遗留的待 finalize spool 由下轮 `cleanup_orphan_spools` 清理或续传。

#### 3.19.3 O2 第二步：主动 sync_now 与配对链路三段式（2026-10-24）

- **`resync_with_peer_access`**（`sync/resync.rs`）：客户端侧三段式——锁内段①（配对信息+
  解 sync_key+状态初值+导出待发页，产出 `PreparedResync`：密文页+密钥+对端地址元信息，
  均可安全带出锁外）→ 网络段（连接/按需发现/握手/发 Push 页/收对端 Push 页）→
  锁内段②（device_id 比对 → 合并，**沿用段①载入的状态**——发送侧水位推进随状态带入，
  合并不重载，否则每轮全量重发）→ 网络段（Done/blob）→ 锁内段③（持久化游标+地址自愈）。
  `resync_with_peer` / `resync_with_peer_opts` 保留为兼容包装（DirectAccess）。
- **`JoinerSession::push_and_sync_access`**（`sync/client.rs`）：配对交换客户端三段式，
  结构同上；`push_and_sync` 保留兼容包装。
- **`SyncServer::handle_connection`**（`sync/server.rs`）：Host 配对侧三段式——锁内段①
  （取本端 VK，ACCEPT 信封需要）→ 网络段（HELLO/令牌/ECDH/ACCEPT/双向确认）→
  锁内段②（令牌用后即焚+persist_device+状态初值 D-62 重置）→ 网络段收 Push 页 →
  锁内段③（合并+导出回包页）→ 网络段（回 Push/收 Done/blob）→ 锁内段④（持久化游标）。
  `accept_and_sync` / `poll_accept_and_sync` 保留兼容包装；新增 `poll_accept`（不持锁）；
  原 `run_exchange` 内联删除。
- **`sync_join_impl` 三段化**：锁内段①取本端设备身份 → 网络段 `client::pair`（握手全程
  锁外）→ 锁内段②（VK 采纳 U1/U2/D-47/D-62 + persist_host_device）→ 三段式交换。
  **行为变更**：握手期间自动锁定触发 ⇒ 配对整体放弃（旧实现持锁横跨全程无此窗口；
  换来配对全程 UI 可响应）。
- **`spawn_pairing_worker` 三段化**：`poll_accept` 不持 session 锁；连接后经 `SessionAccess`
  处理（`sync_busy` 互斥覆盖配对交换）；会话中途被锁 ⇒ 配对作废并广播
  `pairing://ended`，消除旧「二维码还挂着但 worker 已退出」悬死态。
- **并发安全论证**：三段式后同一对端的 `DeviceSyncState` 仅由本场交换读写
  （`sync_busy` 互斥被动/主动/配对三路）；库文件写入全部经 session 锁内段序列化；
  段间窗口内的用户编辑不进本场导出（下轮补发，最终一致）。

---

### 3.20 D-96 全仓复核修复批次（2026-10-25，只读复核 → 主人全 A 批准后实施）

> 背景：D-91 后的第二次全仓只读复核提出 10 项建议，主人逐项拍板批准（全 A），
> 本节为实施登记。错误码全部**复用既有 `ERR_VALIDATION`**（无新增错误码/无新增 i18n 键）；
> 前端 `src/lib/constants.ts` 不涉及（新增常量均为 Rust 内部防御上限，不上线、不进 wire）。

#### 3.20.1 身份私钥 VK 包裹（同步安全）

- **修法**：Host 长期身份私钥不再以 Base64 裸明文落盘，改用**保险库主密钥（VK）**
  XChaCha20-Poly1305 包裹。新格式 = `PMSIDENT1\x00` 魔数(10B) ‖ nonce(24B) ‖ ct(48B)，
  AAD 绑 `AAD_IDENTITY`（`b"identity"`）；旧格式（Base64 裸私钥）读取时识别后**立即
  改写为新格式**（自动迁移，幂等，老库无感知）。落盘走「同目录临时文件 + rename」
  原子替换，Unix 侧临时文件与目标文件 chmod 600（`#[cfg(unix)]`）。
- **落点**：core 新模块 `sync/identity.rs`（`load_or_create_identity_file`，纯逻辑可测）；
  外壳 `sync_cmd.rs` 在 `sync_start_pairing`（唯一调用点，必然已 `session.repo()?` 解锁）
  从会话取 VK 传入；顺带删除 `state.rs` 死字段 `host_identity` 及 `HostIdentity` 导入。
- **守门**：`sync/identity.rs` 单测 7 例（封/解回环与魔数 / 错 VK 拒绝 / 密文篡改拒绝 /
  非魔数拒绝 / 旧格式迁移且私钥不变 / 新建即新格式 / Unix 600 权限）。

#### 3.20.2 导入两级上限（资源防护）

- **修法**：`import::preview_from_file` 在 `fs::read` 前 `metadata` 预检文件
  ≤ `MAX_IMPORT_FILE_BYTES`（64 MB，新常量）；解析后条目数 ≤ `MAX_IMPORT_ITEMS`
  （10 000，新常量）。超限 `ERR_VALIDATION`（信息带实际大小/数量）。Android SAF 入口
  `content_uri::read_to_bytes_limited`（`read_to_bytes` 的限量化变体，附件入口 20 MB
  语义不变）按 64 MB 读取，拿到字节后按同一常量复查。
- **守门**：`tests/d96_guards.rs` 3 例（超限文件拒绝 / 超限条目数拒绝 / 9999 条放行对照组）。

#### 3.20.3 同步接收侧资源夹取（协议防护）

- **修法**：`paging::recv_push_pages` 接收循环累计 `estimate_record_size` 总和
  ≤ `SYNC_MAX_RECV_TOTAL_BYTES`（= `MAX_VAULT_BYTES` 200 MB）且页数
  ≤ `SYNC_MAX_RECV_PAGES`（4096，新常量），超限 `Validation("sync_recv_limit_exceeded:
  pages=…, bytes=…")`。协议兼容：正常分页远低于阈值（200 MB 库最多几十页）。
  内核 `recv_push_pages_with_limits` 限额注入可测。
- **守门**：`paging.rs` 单测 3 例（页数超限拒绝 / 字节超限拒绝 / 限额充足时 3 页全收对照）。

#### 3.20.4 KDF 参数夹取（解锁 DoS 防护）

- **修法**：`kdf::derive_master_key` 入口校验头部 KDF 参数（**不可信输入**）：
  `memory_kib ≤ KDF_MAX_MEMORY_KIB`（1 GiB）、`iterations ≤ KDF_MAX_ITERATIONS`（16）、
  `parallelism ≤ KDF_MAX_PARALLELISM`（8），零值同样拒绝。超限
  `Validation("kdf_params_out_of_range")`，绝不以超限参数执行 Argon2id。
  默认 64 MiB/3/1 远低于上限，存量库零影响。
- **守门**：`kdf.rs` 单测 3 例（默认放行 / 边界值 1 GiB·16·8 与下界放行 / 6 组越界拒绝）。

#### 3.20.5 剪贴板 Rust 兜底（A-03 加固）

- **修法**：`copy_secret` 成功后 spawn **detached 线程**等 `CLIPBOARD_TTL_SECS`（30s）
  再调 `clear_owned_clipboard`（take+比对语义复用，与前端倒计时幂等协作——先到者清理、
  后到者空转）；`lib.rs` 由 `.run(ctx)` 改 `.build(ctx)` + `app.run(callback)`，
  在 `RunEvent::Exit` 同步调 `clear_owned_clipboard`（进程退出是最后一个清理窗口）。
  前端 `clipboard.ts` 不动。
- **守门**：外壳层逻辑（无 core 单测；依赖剪贴板系统状态，纯集成域）。

#### 3.20.6 条目统一明文上限（同步卡死防护）

- **修法**：`validate_item_kind` 新增统一上限：`serde_json::to_vec(kind)` 字节数
  ≤ `MAX_ITEM_PLAINTEXT_BYTES`（384 KiB，新常量）。此前 Login.notes 无上限，一条巨型
  笔记可把单条变更顶过同步分页阈值（`change_too_large` 每轮重发、永不收敛）。
  换算关系：明文 384 KiB → 线上估算 ≈ 769 KiB < `PAGE_BUDGET_BYTES`（≈992 KiB，
  单条拒绝线 ct ≈ 496 KiB），384 KiB 条目永远到不了 `change_too_large`。超限错误带
  标题（截 64 字符）与上限值。`MAX_NOTE_BYTES`（256 KiB）既有语义不变。
- **守门**：`tests/d96_guards.rs` 2 例（create/update 超限拒绝且报错带标题与上限 /
  256 KiB 满额 SecureNote 正常创建回归锚点）。

#### 3.20.7 头部计数交叉核对（静默丢数据预警）

- **修法**：`Repository::open` 在 `rebuild_index` 后比对头部 `item_count`（
  `persist_header` 维护的活条目数）与实际解密扫描计数；不一致**不阻断解锁**
  （帧密文是真相源），警告 `header_item_count_mismatch: header=N, actual=M` 存入
  `Repository::open_warnings`，由 `Session::unlock` 并入既有 `maintenance_warnings`
  通道，`vault_cmd::unlock` 落文件日志（与 D-91 C4 同路）。
- **守门**：`tests/d96_guards.rs` 1 例（等长改写头部计数 → 解锁出警告、业务数据完好）。

#### 3.20.8 `update_header` 原地分支真刷盘（掉电防护）

- **修法**：`Container::update_header` 原地写分支在 `flush` 后补 `sync_all`——
  `flush` 对 `File` 是 no-op，头部仍在 page cache；本分支承载 `change_master_password`
  改 `wrapped_vault_key` 的写入，掉电即「文件在、VK 没落盘」= 库锁死。压缩路径
  （`rewrite_with_frames` 已有 `sync_all`）与记录分支（append/pwrite，flush 语义维持）
  不动。
- **守门**：随 `container_roundtrip.rs` 既有回归覆盖（行为增强无语义变更）。

#### 3.20.9 导入去重回填（D-90 补漏）

- **修法**：`import::apply` 的 `put_item_direct` 成功后把 `(title, type_name)` 回填
  `existing_keys`——同一预览文件内的重复条目此前会双双落库（首条写入后集合未更新）。
- **守门**：`tests/d96_guards.rs` 1 例（同文件两条同名同型 → 只导首条、
  `duplicate_skipped=1`、库内唯一）。

#### 3.20.10 条目 `folder_id` 存在性校验（数据完整性）

- **修法**：`create_item` / `update_item` 对 `draft.folder_id = Some` 时
  `folders.contains_key` 校验（对齐 `move_item_to_folder` 既有写法），不存在报
  `Validation("folder <id> does not exist")`。此前引用不存在的分组后条目在列表中「隐身」。
- **守门**：`tests/d96_guards.rs` 1 例（create/update 引用幽灵分组拒绝且报错带 id /
  真实分组放行 / update 校验早退不半途改动原条目）。

---

### 3.21 D-129：同步自动化——两个「假开关」终于有消费点（1.4.13，2026-10-29；**1.4.16 已回退，见 §3.23**）

> **本节已作废（v1.4.16 / D-133）。** 自动同步整条链路（`src/lib/auto-sync.ts`、`syncQuiet()`、
> 两个设置开关、状态页回执）已随 D-133 一并删除，同步回到「点『立即同步』才发生」。
> 保留本节是为了留下**为什么当初做、又为什么撤**的完整链条，回退理由见 §3.23。

#### 3.21.1 缺陷：开关能存能读，全仓没有人读它做事

`VaultSettings` 里 `auto_sync`（`model/vault.rs:125`）与 `sync_on_start`（`:127`）自 v1.0 就在设置页有
复选框，勾选后 `update_settings` 整结构落盘成功、读回来也是 `true`——但**没有任何一处代码读它来做事**。
grep 全仓，两个字段的全部命中只有四类：结构体定义、`Default`、i18n 文案、`Settings.svelte` 的
`bind:checked`。零消费点。

后果不是「少一个功能」，而是**UI 替不存在的功能做了承诺**：用户勾上「自动同步」，保存成功、界面显示
已勾选、此后再不手动点「立即同步」，两端数据静默不同步。对密码管理器这是数据一致性风险。

对照组证明判据成立：同一结构里的 `clipboard_clear_secs` 有 `settings_cmd.rs:22` 的后端钳制与前端倒计时，
`auto_lock_minutes` 有 D-61 的全局闲置计时——**真开关都有消费点**。

PRD 侧口径本就留了口子：`01-prd.md:144` 写「自动同步为可选增强，须有开关与节流策略」，
`P-09`/`P-10` 只要求「有开关」。于是「先摆开关、功能后补」成了默认，补的那一半一直没来。

#### 3.21.2 落点：调度器进 `src/lib/auto-sync.ts`（与 auto-lock 同形）

照 `auto-lock.ts` 的先例，逻辑与宿主分离：`createAutoSync(deps)` 只依赖 `now` / `schedule` /
`getSettings` / `getDeviceCount` / `isBusy` / `runOnce` 六个注入项，不碰 `window`、不碰 Tauri，
vitest 注入假时钟即可覆盖全部分支。组件层 `MainLayout.svelte` 只负责注入真实实现。

| 常量 | 值 | 理由 |
|---|---|---|
| `AUTO_SYNC_TICK_MS` | 60 s | **心跳只做判定、不承担计时精度** |
| `AUTO_SYNC_INTERVAL_MS` | 5 min | PRD 只要求「有节流策略」，取口令管理器常见量级 |
| `AUTO_SYNC_FAIL_BACKOFF_MS` | 15 min | 对端离线是常态（手机不在同一局域网），别每 5 分钟敲门 |
| `AUTO_SYNC_MIN_GAP_MS` | 60 s | 防 `$effect` 重跑导致连击 |

**为什么不直接排一个 5 分钟的定时器**：隐藏窗口/后台标签页里 JS 定时器会被浏览器节流到分钟级甚至
冻结，长定时器会累积漂移（每一次都晚一点，几次之后偏差到小时级）。改「60 秒心跳 + 用真实时钟
`now >= dueAt` 判到期」后，节流的最坏后果只是「晚一分钟」，不会越跑越偏。

**到期判定的四条否决理由必须分开写**（`isAutoSyncDue`，纯函数）：开关没开 / 无配对设备 /
手动同步在飞（不插队）/ 未到下次允许时刻。合成一个布尔表达式会让「为什么没同步」在日志与测试里
都消失——而「不知道为什么没同步」正是本次缺陷的成因（连一个消费点都没有，自然无处可说）。

#### 3.21.3 `syncQuiet()`：与 `syncNow()` 的三点刻意差异

| 维度 | `syncNow`（用户点的那台） | `syncQuiet`（自动） |
|---|---|---|
| 提示 | 成功/失败都弹 toast | **一律不弹**（每 5 分钟一条等于骚扰；对端离线不是错误） |
| 范围 | 只有 `device_id` 那一台 | 遍历**全部**已配对设备（自动同步没有「用户点的那台」） |
| 占用态 | `syncingDeviceId`（驱动按钮转圈） | 独立 `autoBusy`（后台跑不该让按钮转起来），且**给手动同步让路** |

两者互相让路是必要的：一个后台同步与一个手动同步同时推进同一份 `DeviceSyncState` 会产生
seq/base 竞态（后端 `sync_busy` 也会挡，前端先让路能少一次注定失败的 IPC）。
`vault.load` 走静默模式——自动同步期间不该因为一次读库失败弹红字。
编辑器草稿是独立 `$state` 拷贝（`ItemEditor.svelte:28-34`），reload 不会冲掉用户正在输入的内容。

#### 3.21.4 启动时机：必须等五路加载完成

`start()` 一进来就要读 `settings.value.sync_on_start` 与 `sync.devices.length`。若在挂载时立刻调用，
读到的是 store 默认值（开关 `false`、设备 0 台），「启动时同步」永远不会发生——**缺陷换了个地方重演**。
因此 `MainLayout` 把 `start()` 挂在 `loadAfterUnlock()` 的 `then` 上，与 D-108 的五路加载串行。
`$effect` 的 cleanup 里 `stop()`，锁定即停心跳。

#### 3.21.5 守门（18 例，四条变异验证全部真红）

`src/lib/auto-sync.test.ts`：`isAutoSyncDue` 四条否决逐条 + 「启动时同步」命中/开关关/无设备/占用中/
幂等/重开会话 + 定时按心跳触发 + 成功 5 分钟、失败 15 分钟 + 无设备空转 + 改设置下一拍生效 + `stop()` 后
不再触发 + 三个常量值钉死。

变异验证（注入后必须红，还原后复绿、字节一致）：删 `busy` 否决 ⇒ 红；删 `start()` 幂等守卫 ⇒ 红
（实测报 `expected 3 to be 1`）；失败退避压成成功间隔 ⇒ 红（`expected 2 to be 1`）；删 `deviceCount`
否决 ⇒ 红。

#### 3.21.6 已知限制（如实登记）

* **后台/隐藏窗口的定时器会被节流**：关窗到托盘后 WebView 仍在跑，但 Chromium 会把定时器降频，
  实际间隔可能长于 5 分钟。心跳 + 真实时钟的设计保证「晚但不漂」。
* **Android 进后台可能冻结 WebView**：切后台期间自动同步不跑（接收侧仍由 Rust 被动 worker 负责），
  回前台后按真实时钟判断是否到期。这是移动端后台限制，不是缺陷。
* **不解决「两端同时改同一条目」**：自动同步只是把同步跑得更勤，冲突仍走既有 `conflict` 链路。

### 3.22 D-130/D-131：自动同步「跑了吗」的三层真相（1.4.14，2026-10-29）

#### 3.22.1 症状（主人真机实测，v1.4.13）

> 「点击 自动同步和启动时同步 后还要点击保存按钮才能保存，用户易忽视保存按钮。」
> 「没有实现启动时同步，电脑和手机一起试了几次没有启动时同步。等了很久也没有自动同步。
> 上次同步时间没有变，更改内容也没有同步上。」
> 后续补充：「刚才我发现不在一个网段，现在改成一个网段，启动时也没有同步。」

**关键**：主人给出的结论是「没实现」。而日志证明**实现了、跑了、失败了、没人告诉他**。
这三种现实在界面上完全同形，是本轮最核心的缺陷。

#### 3.22.2 实测证据（`D:\Tools\PasswordMS-Portable\passwordms.log`）

```
22:30:17 [sync_now] failed: ERR_IO / err.network_unreachable
22:30:23 [sync_now] failed: ERR_IO / err.network_unreachable   ← 5.5s 后，第二台设备
22:38:01 [sync_now] failed: ERR_IO / err.network_unreachable
22:38:07 [sync_now] failed: ERR_IO / err.network_unreachable
```

**成对、间隔 5.5 秒** = `syncQuiet` 遍历两个已配对设备、各超时一次。**调度器确实在跑**。
配套的端口实测（同网段修正后）：

| 检查项 | 结果 |
|---|---|
| `Get-NetUDPEndpoint -LocalPort 47823` | `0.0.0.0:47823` **已绑定**（发现应答器在跑） |
| `Get-NetTCPConnection -LocalPort 38931` | `0.0.0.0:38931` **LISTENING**（重同步服务在跑） |
| 本机 IPv4 | `192.168.31.122`（WLAN），另有 `192.168.137.1` 热点 / `172.20.64.1` WSL |

即：**电脑这侧一切正常，连不上的是对端**。

#### 3.22.3 三层根因（必须分开修，压成一条就会漏）

**① 失败完全静默（可观测性为零）**

`syncQuiet` 把每台设备的异常吞进空 `catch`，`autoLast` 这种状态根本不存在。
界面上唯一的痕迹是「上次同步时间没变」——而「没变」同时对应：

* 没跑（开关没开 / 没配对）
* 跑了但失败（对端不可达）
* 跑了也成功，但对端本来就没有新变更

用户无法区分，只能猜。**这是「功能没实现」这个错误结论的来源**。

**② 退避策略与真实场景不匹配**

原实现失败后固定等 15 分钟。而对端（手机）**锁屏时会 `stop_resync_listener`**，
「电脑解锁那一刻手机恰好锁着」是**常态**不是异常。主人解锁手机、改完条目、回电脑，
下一次重试还排在十几分钟后 —— 于是永远看不到结果。

**③ 地址自愈在 Android 上根本不可能生效（D-131，最深的那个）**

`resync.rs` 的兜底是「静态地址连不上 → `discover_addr` 广播找一次」。
但 Android 默认**丢弃**收到的广播/多播包，除非持有 `WifiManager.MulticastLock`，
而拿这个锁需要 `CHANGE_WIFI_MULTICAST_STATE` 权限。**两者此前都没有**（`AndroidManifest.xml`
只有 `INTERNET` + `CAMERA`）。

后果：只要对端换过 IP（DHCP 续租、换 Wi‑Fi、跨网段），电脑的广播手机**收不到**，
发现永远返回 `None`，`sync_now` 只能报 `err.network_unreachable`。
主人这次「不在一个网段 → 改成一个网段」正好踩在这条上：**改了网段，存的旧地址作废，
而唯一能自愈的通道是哑的**。

#### 3.22.4 修法

| 编号 | 改动 | 位置 |
|---|---|---|
| D-130 | 两个开关**勾选即保存**（`saveSyncPrefs`，只写这两个字段，失败回滚 checkbox） | `Settings.svelte` |
| D-130 | `AutoSyncOutcome` 记录每次结果（时间/成败/台数/首个错误 key），状态页如实展示 | `sync.svelte.ts` / `SyncStatus.svelte` |
| D-130 | 退避改渐进：`failBackoffMs` = 2 → 4 → 5 分钟封顶，一次成功清零 | `auto-sync.ts` |
| D-131 | Android 补 `CHANGE_WIFI_MULTICAST_STATE` + `onResume` 持有 `MulticastLock` | `AndroidManifest.xml` / `MainActivity.kt` |

**为什么「勾选即存」只写这两个字段**：若要保存 `$state.snapshot(draft)`，
用户改了别的字段没保存、顺手勾一下开关，那半截改动会被一起落盘——那是另一种「界面骗人」。

**为什么 MulticastLock 放 `onResume`/`onPause`**：锁会让无线芯片不休眠接收多播，
后台常驻是白耗电；而同步只在解锁使用时才有意义。

#### 3.22.5 判据与守门

`auto-sync.test.ts` 20 例，其中三条变异各自钉住一个分支：

| 变异 | 期望红在哪条 |
|---|---|
| `failBackoffMs` 改回固定 15 分钟 | 「失败后 2 分钟就重试」 |
| 成功分支去掉 `failures = 0` | 「一次成功即把失败计数清零」 |
| 失败分支去掉 `failures += 1` | 「连续失败渐进退避 2→4→5 分钟封顶」 |

三条已实测全红、还原后字节一致并复绿。

> **踩过的坑**：第一版「清零」用例只断言「成功后按 5 分钟跑」，而成功分支的间隔恒为
> `AUTO_SYNC_INTERVAL_MS`、与 `failures` 无关 —— 删掉 `failures = 0` 它照样绿。
> 改写成「成功之后**再失败**，下次间隔必须是 2 分钟而不是 5 分钟」才真正守住。

#### 3.22.6 仍然没有解决的（如实登记）

* **对端锁屏时拉不到数据**，这是 `stop_resync_listener` 的设计取舍（锁定时关监听），
  不是本轮能绕过的。可见的缓解是与它配套的另外两件：**手机端也打开「启动时同步」**
  让它解锁时主动推；以及本轮的短退避让电脑在手机解锁后最多 5 分钟内跟上。
* **MulticastLock 只解决「手机收得到广播」**。若 APP 被系统冻结或手机锁屏，
  responder 本身不存在，广播照样无人应答。
* **未端到端复现「跨网段 → 同网段 → 同步成功」**：需要主人装 1.4.14 后实测。
  判据是状态页「上次自动同步」显示成功，或日志出现 `[resync] completed`。

### 3.23 D-132：局域网发现的可观测性 + 多轮重播（1.4.15，2026-10-30）

**症状**（主人实测，v1.4.14 双端）：两端都打开、同一条 Wi-Fi（路由器 `commander`）、
两端版本都是 1.4.14，但自动同步与手动同步**全部失败**，`上次同步` 停在换网前的
`2026-10-29 14:36`，`待同步变更 28` 一直不动。

**排查矩阵（全部有实据，非推断）**：

| 假设 | 实测 | 结论 |
|---|---|---|
| 防火墙拦 | `D:\tools\passwordms-portable\passwordms.exe` 的 TCP/UDP 入站规则：`LocalPort=Any`、`Profile=Public`、`Allow`、启用 | 排除 |
| 路由/网卡不对 | 默认路由与 `192.168.1.0/24` 都走 WLAN（`192.168.1.1`） | 排除 |
| 监听没起 | `0.0.0.0:38931` 有监听、`0.0.0.0:47823` 已绑定（同一 pid） | 排除 |
| 设备列表含本机（同步自己） | 本机 `passwordms.device` 是 UUID；列表两台是真实对端 | 排除 |
| 会话锁饿死发现轮询 | `sync_now` 已三段式，网络 I/O 全程锁外（O2） | 排除 |
| 网络层不通 | 手机浏览器访问 `http://192.168.1.103:38931` 返回 **`ERR_CONNECTION_REFUSED`**（收到 RST，不是超时） | **网络层通**，且该次为电脑锁定态（锁定时本就关监听，属设计取舍） |

**关键证据：失败耗时 5.5 秒**。`SYNC_CONNECT_TIMEOUT_SECS=4` + `DISCOVERY_WAIT_MS=1500`
正好 5.5s，与日志里成对出现的失败间隔严丝合缝 —— 说明「静态地址 connect 失败 → 按需
广播发现」这条路径**确实执行了，且一个应答都没收到**，而不是发现没被触发。

**根因未端到端确认（如实标注）**。已能确定的是：配对记录里的地址是换网前的旧网段，
静态连接必然失败，唯一兜底是广播发现；而发现在真机上一轮下来零应答。至于「手机没收到
广播」还是「手机回了但应答到不了」，v1.4.14 的日志一个字都没有，无法判定。

**这次不做猜测式修复**，只做两件事：

1. **补可观测性（必做）**。`discovery` 模块加线程内轨迹（`trace`/`drain_trace`/
   `clear_trace`），`sync_now` 结束后把轨迹合并成一行落盘 `[sync] 轨迹: …`，内容含：
   配对记录地址、静态 connect 结果、发现模式、广播目标数、**每轮每个目标的 send 成功/
   失败数**、收到包数、`bad_json`/`bad_q`/`bad_tag`/`self` 四个丢弃计数、最终结果。
   `[sync_now] failed` 日志同时补上 `device=`（两台对端连续失败时原先无法分辨是哪台）。
2. **多轮重播**。原来一个窗口只广播**一轮**，丢一次就彻底救不回来（WiFi 省电、AP 缓冲
   溢出、手机刚出 Doze 射频未就绪都会整体丢失）。改为窗口内每 `400ms` 重播一轮、
   最多 5 轮，`DISCOVERY_WAIT_MS` 由 `1500` 提到 `2500`。代价是「发现失败 → 报错」
   升到约 6.5s，且只发生在静态地址已失败的兜底路径上。

**单轮/多轮必须分成两个入口（踩坑记录）**。第一版直接把 `discover_peer_with` 改成多轮，
`discovery_ignores_self_reply` 立刻打红，而且红的是**对照组**——这不是测试挑剔，是真实
时序缺陷：多轮会把上一轮的遗留请求留在应答方的 socket 缓冲里，下一次 `poll` 先消费到
旧 `q`、回了旧包，新那轮的应答反被顶掉。因此「发几轮」属于**调用方策略**，不是发现单元
的语义：`discover_peer_with`（单轮，测试与单发语义用）与 `discover_peer_retrying`
（多轮，生产用）并存，共用内部 `discover_rounds(..., max_rounds)`。

**守门测试**：`discovery_rebroadcast_repeats_within_window` 用 `self_port` 自过滤让请求方
「收不到应答但继续重播」，从而用应答方收到的请求数直接量出轮数（单轮必须正好 1 次、
多轮必须 ≥2 次）。变异验证：把 `discover_peer_retrying` 改回单轮 → 只有这一条红，其余
7 条仍绿，说明守门精准无误伤。

> 待下次真机复现验证：`[sync] 轨迹:` 那行里的 `bad_tag` 与 `recv` 计数直接决定方向——
> `recv=0` 是「广播没到 / 对方不应答」；`bad_tag>0` 是「对方持有的 `sync_key` 与本机不一致」
> （旧配对残留或重配对未同步），两者修法完全不同。

### 3.24 D-133 / D-134：自动同步回退 + 手动同步的网络信息上屏（1.4.16，2026-10-30）

#### 3.24.1 真机取证（v1.4.15 双端，全部实测，不是推断）

主人回传的轨迹（10:36:03 与 10:37:20 两次，间隔 37s）：

```
[sync] 轨迹: peer: 配对记录地址 192.168.1.102:46879 | connect: 静态地址 192.168.1.102:46879 失败（4s 超时）
→ 尝试局域网发现 | discovery: start local=0.0.0.0:51434 targets=5 wait=2500ms self_port=Some(38931)
| discovery: 第1轮 广播 sent=5 err=0
| discovery: timeout rounds=5 recv=25 bad_json=0 bad_q=0 bad_tag=0 self=25 → 无应答
```

`recv=25` 与 `self=25` **完全相等**，且 `bad_json=0`：这 25 个包全部解析成了合法
`DiscoveryResponse`、通过 `q` 与 `tag` 校验、再被「自己应答自己」规则丢弃
（`resp.port == 38931` 且源 IP 是本机）。即 **5 个广播目标 × 5 轮 = 25 次发送，25 次都由本机
自己的 responder 应答**，对端 ip 一个包都没回来（连畸形包都没有）。广播链路在本机侧完全通畅。

同一时刻对那台手机做的网络探测（在本机执行，脚本与结论都可复现）：

| 探测 | 结果 | 含义 |
|---|---|---|
| 本机 IPv4 | `192.168.1.103/24`（WLAN） | 与手机 `192.168.1.102` **同网段** |
| `ping 192.168.1.102` × 10 | 10/10 通，RTT 17–111ms | 手机在线；抖动幅度 = 射频在省电唤醒 |
| TCP → `192.168.1.102:9`（对照组） | 2.4s **收到 RST** | 手机内核活着，会回拒绝 |
| TCP → `192.168.1.102:46879`（同步口） | 3.5s **超时，连 RST 都没有** | SYN 被**静默丢弃**，不是「端口没开」 |

对照组是关键：**同一台设备**对没监听的端口会立刻回 RST，对同步口却连 RST 都不给。
没监听只会得到 RST，`TcpListener` 的 backlog 是 std 默认的 128（不可能被几次 SYN 塞满），
所以只剩一种解释：**这个 App 的 socket 上的包被系统按 UID 丢掉了**——Android 的
Doze / 应用待机桶 / 后台数据限制就是这个形态（静默丢包，不回 RST，也不回 ICMP 错误）。
UDP 发现同理由此归零，两个症状是**同一个原因**。

据此排除的假设：跨网段（同 /24）、密钥不一致（`bad_tag=0`）、防火墙/路由（本机 25 个自答全通）、
「手机没装 1.4.15 缺 `MulticastLock`」（APK 内 `CHANGE_WIFI_MULTICAST_STATE`、
`ACCESS_WIFI_STATE`、dex 里的 `createMulticastLock` 与 `passwordms-lan-discovery` 均已核实进包；
且组播锁只管收广播帧，管不了 TCP SYN 被丢——TCP 也是死的）。

#### 3.24.2 结论与决策：自动同步回退（D-133）

要让手机在息屏状态下仍可被局域网找到，Android 上只有一条正经路子：**前台服务 +
`WifiLock(HIGH_PERF)` + `MulticastLock` + 电池优化白名单**。代价是一条常驻通知与持续耗电。

主人裁定：**「不行的话就不要自动同步了，回退到手动同步，收益不大。」**

于是 v1.4.16 做了**完整回退**，删掉的是功能，不是「留个开关不接线」：

| 位置 | 删除内容 |
|---|---|
| `src/lib/auto-sync.ts` + 单测 | 调度器本体（`createAutoSync` / `isAutoSyncDue` / `failBackoffMs` / 全部常量） |
| `MainLayout.svelte` | `createAutoSync` 实例与 `$effect` 接线（保留 `loadAfterUnlock()`） |
| `stores/sync.svelte.ts` | `autoBusy` / `autoLast` / `AutoSyncOutcome` / `syncQuiet()` |
| `Settings.svelte` | 「自动同步 / 启动时同步」两个复选框、`saveSyncPrefs()`、勾选即存提示 |
| `SyncStatus.svelte` | 开关状态行 + 上次自动同步回执行 |
| `types.ts` / `stores/settings.svelte.ts` / `crates/.../model/vault.rs` | `auto_sync` / `sync_on_start` 字段 |
| i18n × 4 | `settings.auto_sync` / `sync_on_start` / `sync_instant_hint` / `save_instant_failed` / `sync.auto_*` 共 14 键 |

**兼容性**：serde 默认忽略未知字段，旧库里的两个键在反序列化时被丢弃、下次保存即消失，无需迁移。

设置页保留一句事实说明（`settings.sync_manual_only`「同步只在你点『立即同步』时发生」）——
回退之后不能让用户只看到「开关不见了」。

#### 3.24.3 手动同步必须可读（D-134）

回退之后，手动同步就是唯一路径，而它现在**失败时只给一句「网络不可达」**。用户手上没有任何
可以自己判断的信息。本轮把两个「对照量」上屏：

1. **本机网络**（`sync_net_info`，只读）：本机全部 IPv4 + 当前重同步监听端口 +
   UDP 发现应答是否可用。`listen_port === null` 单独显示「未在监听」，不与「端口是 0」混同。
2. **每台设备的配对记录地址**：`DeviceDto` 增加 `peer_ip` / `peer_port`（`PairedDevice` 里本来就有，
   只是从没上过屏），设备行显示 `192.168.1.102:46879`；缺地址时显示「未记录地址」而不是留白。

两端各看一眼就能定性：地址/端口对不上 = 记录过期（重新配对或等发现自愈）；
对得上但连不上 = 对端被系统掐了网络（页面上给出「解锁对端并保持前台」的行动指引）。

`sync_net_info` **刻意不取 session 锁、也不调用 `ensure_resync_listener`**：诊断命令绝不能有
「顺手把监听器拉起来」的副作用。它读的是 `state.resync` 里既有的 handle。

#### 3.24.4 守门

`src/lib/d133-d134-sync-guard.test.ts` 8 例，正负成对（负向断言单独存在会在「整块被删」时假绿）：

| 断言 | 变异验证 |
|---|---|
| 设置页不得再出现 `auto_sync` / `sync_on_start`，且必须含 `settings.sync_manual_only` | 重新加回 `pub auto_sync` → 红 |
| `MainLayout` 不得再出现 `auto-sync` / `createAutoSync`，且 `loadAfterUnlock()` 仍在 | — |
| `VaultSettings` 不得再有这两个字段（正向锚点：`clipboard_clear_secs` 仍在） | 同上一条红 |
| `SyncStatus` 必须渲染 `peer_ip` / `peer_port` / `local_net` / `listen_port` | 把地址换回 `device.device_name` → 红 |

两条变异已实测真红，还原后字节一致并复绿。

## 4. 文件列表及相对路径

> 交付工程师寇豆码的实现清单。所有路径相对项目根 `D:/Software/PasswordMS`。

### 4.1 工作区 / 构建配置

| 路径 | 职责 |
|------|------|
| `Cargo.toml` | Cargo workspace 定义（members: core + src-tauri） |
| `package.json` | 前端依赖与脚本（`dev`/`build`/`tauri`） |
| `vite.config.ts` | Vite + Svelte 插件配置，产物体积优化 |
| `svelte.config.js` | Svelte 编译配置 |
| `tsconfig.json` | TS 编译配置（strict） |
| `index.html` | 前端入口 HTML |
| `.gitignore` | 忽略 `target/`、`node_modules/`、`dist/` |

### 4.2 前端（`src/`）

| 路径 | 职责 |
|------|------|
| `src/main.ts` | 应用引导入口，挂载 `App.svelte` |
| `src/App.svelte` | 根组件：锁定/解锁 ↔ 主界面路由切换 |
| `src/types.ts` | 与 Rust DTO 对齐的 TS 类型定义（IPC 契约） |
| `src/lib/ipc.ts` | Tauri `invoke` 类型化包装层（所有命令前端入口） |
| `src/lib/errors.ts` | 错误码 → 用户可读文案映射 |
| `src/lib/constants.ts` | 与 Rust `constants.rs` 对齐的常量（上限、默认值） |
| `src/lib/stores/session.ts` | 会话状态（锁定态/解锁态、当前保险库元信息） |
| `src/lib/stores/vault.ts` | 条目/分组/回收站/冲突的内存状态与派生（过滤、搜索、收藏置顶） |
| `src/lib/stores/settings.ts` | 设置状态（自动锁、体检阈值、生成器偏好、同步开关） |
| `src/lib/stores/ui.ts` | UI 状态（路由、toast、模态、抽屉、当前选中条目） |
| `src/lib/clipboard.ts` | 复制 + 30s 清空逻辑（调用 Rust 剪贴板命令，含「仅清自有内容」判定） |
| `src/lib/qr.ts` | QR 展示（取 Rust SVG）与扫描（`jsqr` 解析摄像头帧） |
| `src/lib/url-safety.ts` | F-05 URL 协议白名单校验（仅 http/https，其余拒绝） |
| `src/lib/i18n/index.ts` | 极简 i18n：`t(key)` + 语言切换（默认 zh-CN） |
| `src/lib/i18n/zh-CN.ts` | 简体中文文案 |
| `src/lib/i18n/en-US.ts` | 英文文案（占位，MVP 不启用） |
| `src/styles/global.css` | 全局样式、CSS 变量、响应式断点 |
| `src/components/common/Toast.svelte` | 轻提示（复制倒计时、保存状态） |
| `src/components/common/ConfirmDialog.svelte` | 二次确认对话框（O-03/04/05/06/07 复用） |
| `src/components/common/Modal.svelte` | 通用模态容器 |
| `src/components/common/Icon.svelte` | 内联 SVG 图标集（**不使用图标字体**） |
| `src/components/common/StrengthMeter.svelte` | 主密码/生成器强度条 |
| `src/components/common/Spinner.svelte` | 加载指示 |
| `src/components/LockScreen.svelte` | P-01 解锁页 |
| `src/components/CreateVaultWizard.svelte` | P-01 首启创建保险库向导 |
| `src/components/MainLayout.svelte` | P-02 主框架（桌面三栏 / 移动单栏自适应） |
| `src/components/FolderSidebar.svelte` | P-02 分组栏（全部/收藏/分组/回收站/体检入口 + 新建/编辑/排序） |
| `src/components/ItemList.svelte` | P-02 条目列表（搜索、收藏置顶、空状态） |
| `src/components/ItemDetail.svelte` | P-02 详情面板（字段、复制、打开网址、TOTP 展示） |
| `src/components/ItemEditor.svelte` | P-03 条目编辑（类型切换、动态字段、自定义键值对） |
| `src/components/PasswordGenerator.svelte` | P-04 密码生成器 |
| `src/components/SecureNoteView.svelte` | P-05 安全笔记查看（等宽、折叠敏感） |
| `src/components/AttachmentView.svelte` | P-06 附件查看与导出（明文风险提示） |
| `src/components/ImportWizard.svelte` | P-07 Bitwarden 导入向导（解析预览 + 映射表 + 结果） |
| `src/components/sync/PairShow.svelte` | P-08 展示二维码（倒计时/刷新） |
| `src/components/sync/PairScan.svelte` | P-08 扫码配对（摄像头 + 手动录入兜底） |
| `src/components/sync/DeviceList.svelte` | P-08 已配对设备列表与解除配对 |
| `src/components/sync/SyncStatus.svelte` | P-09 同步状态/进度/结果 |
| `src/components/sync/ConflictPanel.svelte` | P-09 冲突提示与三选一仲裁 |
| `src/components/Settings.svelte` | P-10 设置页（安全/数据/同步/体检/关于） |
| `src/components/HealthCheck.svelte` | P-11 密码体检页 |
| `src/components/Trash.svelte` | P-12 回收站 |
| `src/tests/*.test.ts` | 前端单测（stores、clipboard 逻辑、url-safety，vitest） |

### 4.3 Rust 核心（`crates/passwordms-core/src/`）

| 路径 | 职责 |
|------|------|
| `lib.rs` | crate 根，导出公共 API 与模块 |
| `error.rs` | `CoreError` 枚举与错误码（§9.2） |
| `constants.rs` | 全部常量（KDF 参数、大小上限、超时、重试、协议版本） |
| `securemem.rs` | `Secret`/`Zeroizing` 类型别名与清零辅助 |
| `model/mod.rs` | 模型模块导出 |
| `model/vault.rs` | `Vault`、`KdfParams`、`WrappedKey`、`VaultSettings` |
| `model/folder.rs` | `Folder` |
| `model/item.rs` | `Item`、`ItemKind`、`LoginData`、`SecureNoteData`、`AttachmentData`、`CustomField` |
| `model/trash.rs` | `TrashEntry` |
| `model/conflict.rs` | `ConflictEntry`、`ConflictChoice` |
| `model/sync_state.rs` | `PairedDevice`、`DeviceSyncState`、`SyncCursor` |
| `crypto/mod.rs` | 加密模块导出 |
| `crypto/kdf.rs` | Argon2id 派生 MK，HKDF 派生 KEK/verify |
| `crypto/aead.rs` | XChaCha20-Poly1305 加解密 + 密钥包裹/解包 |
| `crypto/keys.rs` | 密钥层级类型（KEK/VK/IK）与包裹逻辑 |
| `crypto/rng.rs` | CSPRNG 封装（`getrandom`）与随机 nonce/密钥/令牌生成 |
| `crypto/verify.rs` | 主密码验证（常量时间比较） |
| `store/mod.rs` | 存储模块导出 |
| `store/container.rs` | 容器二进制布局读写（header + 记录帧） |
| `store/frame.rs` | 记录帧编解码、墓碑与原地更新逻辑 |
| `store/repository.rs` | 保险库 CRUD（条目/分组/回收站/冲突/设置） |
| `session.rs` | `Session` 生命周期：创建/解锁/锁定/改主密码 |
| `generator.rs` | 密码生成器（CSPRNG 采样、字符集、排除易混） |
| `health.rs` | 密码体检（弱口令/重复/超期） |
| `totp.rs` | TOTP 生成与剩余时间（RFC 6238） |
| `import/mod.rs` | 导入模块导出与统一预览/结果结构 |
| `import/bitwarden_json.rs` | Bitwarden JSON 解析与字段映射 |
| `import/bitwarden_csv.rs` | Bitwarden CSV 解析与字段映射 |
| `sync/mod.rs` | 同步模块导出 |
| `sync/protocol.rs` | 报文 schema（`QRPayload`/`Hello`/`Accept`/`Confirm`/`SyncEnvelope`/`SyncPayload`/`ChangeRecord`） |
| `sync/handshake.rs` | 配对握手、X25519 协商、密钥确认、令牌校验 |
| `sync/transport.rs` | TCP 监听/连接 + 长度前缀帧收发 |
| `sync/server.rs` | Host 侧会话状态机（监听、接受、推送/拉取） |
| `sync/client.rs` | Joiner 侧会话状态机 |
| `sync/merge.rs` | 冲突检测、`sync_state` 推进、冲突箱写入 |
| `sync/replay.rs` | 序号/时间窗校验（防重放） |

### 4.4 Rust 测试（`crates/passwordms-core/tests/`）

| 路径 | 职责 |
|------|------|
| `crypto_vectors.rs` | 加解密/KDF 测试向量（A-01、A-05） |
| `container_roundtrip.rs` | 容器读写往返 + 单条更新字节差异（A-01、A-02） |
| `sync_conflict.rs` | 冲突检测与三选一仲裁（Q3 回归） |
| `sync_replay.rs` | 重放/未配对拒绝（A-07、A-08） |
| `import_bitwarden.rs` | Bitwarden JSON/CSV 映射与跳过计数 |
| `health_and_generator.rs` | 体检规则与生成器字符集 |
| `zeroize_check.rs` | 锁定后会话内存清零（A-04 辅助） |

### 4.5 Tauri 外壳（`src-tauri/`）

| 路径 | 职责 |
|------|------|
| `Cargo.toml` | 外壳 crate 依赖与 workspace 成员 |
| `build.rs` | Tauri 构建脚本 |
| `tauri.conf.json` | 应用标识、窗口、bundle（nsis/msi/android）、权限 |
| `capabilities/default.json` | Tauri 2 能力授权（dialog/clipboard/opener 权限） |
| `icons/icon.ico` / `icons/*.png` | 应用图标（Win + Android 多密度） |
| `src/main.rs` | 可执行入口 |
| `src/lib.rs` | Tauri Builder：注册插件、命令、托管 `AppState` |
| `src/state.rs` | `AppState`（`Mutex<Session>` + 同步任务句柄） |
| `src/dto.rs` | IPC 传输 DTO（请求/响应结构，serde） |
| `src/error.rs` | `CoreError` → IPC 错误 JSON 映射 |
| `src/commands/mod.rs` | 命令模块汇总与注册列表 |
| `src/commands/system_cmd.rs` | 状态、剪贴板、打开网址、文件信息 |
| `src/commands/vault_cmd.rs` | 创建/解锁/锁定/改主密码 |
| `src/commands/item_cmd.rs` | 条目 CRUD、搜索、收藏 |
| `src/commands/folder_cmd.rs` | 分组 CRUD、排序 |
| `src/commands/keydoc_cmd.rs` | 附件导入/导出、安全笔记 |
| `src/commands/generator_cmd.rs` | 密码生成、TOTP |
| `src/commands/import_cmd.rs` | Bitwarden 预览与执行 |
| `src/commands/trash_cmd.rs` | 回收站操作 |
| `src/commands/health_cmd.rs` | 体检执行 |
| `src/commands/sync_cmd.rs` | 配对、设备、同步、冲突仲裁 |
| `src/commands/settings_cmd.rs` | 设置读写 |

### 4.6 脚本（`scripts/`）

| 路径 | 职责 |
|------|------|
| `scripts/build.sh` | Linux/macOS 一键构建（前端 + 桌面 + Android） |
| `scripts/build.ps1` | Windows PowerShell 一键构建 |
| `scripts/build-android.sh` | Android APK 构建与体积核验 |
| `scripts/check-size.sh` | 校验 Win/APK 体积是否达标（CI 门禁） |

---

## 5. 数据结构与接口

### 5.1 Rust 侧核心数据结构

> 序列化统一 `serde`，**wire 格式 = snake_case**（见 §9.5）。`🔒` 表示落盘时必须处于 AEAD 密文内。

#### `Vault`（落盘头部，明文元数据）

| 字段 | 类型 | 序列化 | 说明 |
|------|------|--------|------|
| `vault_id` | `String` (uuid) | string | 库唯一 ID |
| `format_version` | `u16` | number | =1 |
| `kdf` | `KdfParams` | object | 见下 |
| `kdf_salt` | `Vec<u8>` | base64 | 16B |
| `wrapped_vault_key` | `WrappedKey` | object | VK 密文 |
| `verify_mac` | `Vec<u8>` | base64 | 32B |
| `encryption_scheme` | `String` | string | `"xchacha20poly1305"` |
| `item_count` / `folder_count` | `u32` | number | 计数 |
| `created_at` / `updated_at` | `i64` | number | UTC 毫秒 |
| `auto_lock_minutes` | `u16` | number | 1/5/15 |
| `settings` | `VaultSettings` | object | 偏好（加密存储） |

```rust
pub struct KdfParams { pub algorithm: String, pub memory_kib: u32, pub iterations: u32, pub parallelism: u32, pub salt_len: u16 }
pub struct WrappedKey { pub nonce: Vec<u8>, pub ct: Vec<u8> }   // serde → base64
pub struct VaultSettings {
  pub generator: GeneratorOptions, pub health: HealthConfig,
  pub clipboard_clear_secs: u16,
  // D-133（v1.4.16）：auto_sync / sync_on_start 已随自动同步回退删除。
}
```

#### `Folder`（🔒 内容加密）

| 字段 | 类型 | 说明 |
|------|------|------|
| `folder_id` | `String` (uuid) | 主键（帧头明文） |
| `name` | `String` | 🔒 非空、库内唯一 |
| `icon` | `Option<String>` | 🔒 内联 SVG 标识 |
| `color` | `Option<String>` | 🔒 `#RRGGBB` |
| `sort_order` | `i32` | 🔒 **遗留**：#531 起排序不再消费（字段与 `reorder_folders` 保留仅为兼容） |
| `rev` | `u64` | 同步修订号 |
| `updated_at` | `i64` | UTC 毫秒 |
| `pinned` | `bool` | #531，`#[serde(default)]`：钉住标记（拖拽置顶即钉住） |
| `pin_order` | `i64` | #531，`#[serde(default)]`：钉住块内顺序（小在前；未钉住恒 0） |

> **#531 排序模型（拖拽排序与频度置顶，客户已拍板）**：
> - **列表排序 = 钉住块在顶（`pin_order` ASC）→ 未钉住按使用频度**（`use_count` DESC → `last_used_at` DESC（`None`=0）→ `updated_at` DESC → `item_id` ASC 兜底）。两个排序段是互斥块：钉住条目完全不参与频度比较。**D-84 交互语义（v2.17）**：未钉区也可手动排序——前端拖 A 到 B 上把 B 一并收编进钉住块（`set_*_pin(true)` ×2 按需 + `reorder_pinned_*`，await 串行，先钉后排满足「id 必须已钉住」的整体校验）；本节后端排序模型与 IPC 契约零改动，变化只在前端何时发 `set_item_pin`/`set_folder_pin`。
> - **`favorite` 不再置顶**——保留为纯标记，排序完全不参与（`toggle_favorite` 命令与字段原样保留）。
> - **「使用」= 仅复制动作**：前端在复制成功后调 `record_item_usage` 埋点（`copy_secret` 不感知 item_id）；同条目 **60 秒节流**（`USAGE_THROTTLE_MS`，`record_item_usage_at` 注入时钟可测）——窗口内调用零状态变更（不写帧、不推 rev）。计频 `use_count += 1; last_used_at = Some(now); rev += 1`，**`updated_at` 不变**（不把「复制过」伪装成「修改过」）。
> - **分组频度 = 组内条目 `use_count` 之和**（`last_used_at` 取组内最大），`list_folders` 调用时现算（`Repository::folder_usage`，与 `FolderDto` 的聚合字段同源——UI 显示数字与排序依据必然一致）。
> - 钉住/重排/归组的写入语义：`rev += 1` 走正常 LWW；`set_item_pin`/`set_folder_pin` 同步推进 `updated_at`（用户主动操作，同 `toggle_favorite` 先例）；`reorder_pinned_items`/`reorder_pinned_folders` 先**整体校验**（id 必须存在且已钉住，未钉住报 `ERR_VALIDATION`）再落笔、变更者才推 rev（同序重放零写入）；`move_item_to_folder` 目标分组必须存在（否则 `ERR_VALIDATION`）、`updated_at` 不变。

#### `Item` / `ItemKind`（🔒 全字段加密）

```rust
pub struct Item {
  pub item_id: String,           // 帧头明文
  pub kind: ItemKind,
  pub folder_id: Option<String>, // 单分组归属【已裁定 Q2】
  pub favorite: bool,
  pub created_at: i64, pub updated_at: i64,
  pub rev: u64, pub modified_by: String,
  // ↓ ↓ 以下为增量字段（全部 #[serde(default)]，旧库缺字段读回默认值）
  pub attachments: Vec<AttachmentMeta>,  // D-60
  pub pinned: bool,                      // #531 钉住标记
  pub pin_order: i64,                    // #531 钉住块内顺序（未钉住恒 0）
  pub use_count: u64,                    // #531 使用频度（仅复制动作计数，60s 节流）
  pub last_used_at: Option<i64>,         // #531 最近计频时间（节流判定 + 次级排序键）
}

#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
  Login(LoginData),
  SecureNote(SecureNoteData),
  Attachment(AttachmentData),
}

pub struct LoginData {
  pub title: String, pub url: Option<String>, pub username: Option<String>,
  pub password: Option<String>, pub notes: Option<String>,
  pub totp_secret: Option<String>,           // Base32
  pub custom_fields: Vec<CustomField>,
}
pub struct SecureNoteData { pub title: String, pub body: String }
pub struct AttachmentData {
  pub title: String, pub original_filename: String, pub mime_type: String,
  pub size_bytes: u64, pub blob_ref: String, pub blob_size_bytes: u64,
}
pub struct CustomField { pub key: String, pub value: String, pub hidden: bool }
```

#### `TrashEntry`（🔒 快照加密）

| 字段 | 类型 | 说明 |
|------|------|------|
| `trash_id` | `String` (uuid) | 主键 |
| `item_snapshot` | `Item` | 🔒 被删条目完整快照 |
| `original_folder_id` | `Option<String>` | 还原目标分组 |
| `deleted_at` | `i64` | UTC 毫秒 |
| `expire_at` | `i64` | `deleted_at + 30 天` |

#### `ConflictEntry`（🔒 快照加密）

| 字段 | 类型 | 说明 |
|------|------|------|
| `conflict_id` | `String` (uuid) | 主键 |
| `item_id` | `String` (uuid) | 冲突对的本机条目 ID |
| `local_snapshot` | `Item` | 🔒 本机版本 |
| `remote_snapshot` | `Item` | 🔒 远端版本 |
| `local_updated_at` / `remote_updated_at` | `i64` | 供 UI 展示 |
| `detected_at` | `i64` | 检出时间 |
| `resolved` | `bool` | 是否已仲裁 |

```rust
pub enum ConflictChoice { KeepLocal, TakeRemote, KeepBoth }
```

#### 同步状态

```rust
pub struct PairedDevice {
  pub device_id: String, pub device_name: String, pub fingerprint: String,
  pub sync_key_wrapped: WrappedKey,   // 由 VK 包裹
  pub paired_at: i64, pub last_sync_at: Option<i64>,
}
pub struct DeviceSyncState {
  pub device_id: String,
  pub base: HashMap<String, BaseRev>, // record_id -> 上次同步基准
  pub last_recv_seq: u64, pub last_sent_seq: u64,
}
pub struct BaseRev { pub rev: u64, pub hash: [u8; 32] }
```

### 5.2 Tauri 命令清单（前后端契约）

> 统一返回 `Result<T, IpcError>`；`IpcError = { code: string, message_key: string }`（见 §9.2/§9.3）。

| 命令 | 入参 | 返回 | 说明 |
|------|------|------|------|
| `get_status` | — | `StatusDto{has_vault,locked}` | 启动判定 |
| `create_vault` | `{master_password, strength_confirmed}` | `StatusDto` | F-01 建库 |
| `unlock` | `{master_password}` | `StatusDto` | F-01 解锁（失败统一码） |
| `lock` | — | `()` | F-02 锁定并清零 |
| `change_master_password` | `{old_password,new_password}` | `()` | O-08 |
| `list_folders` | — | `Vec<FolderDto>` | F-03 |
| `create_folder` | `{name,icon,color}` | `FolderDto` | F-03 |
| `update_folder` | `{folder_id,name,icon,color}` | `FolderDto` | F-03 |
| `delete_folder` | `{folder_id,strategy}` | `()` | 二次确认由前端；`strategy: "to_ungrouped"｜"to_trash"` |
| `reorder_folders` | `{ordered_ids}` | `()` | F-03 排序 |
| `list_items` | `{folder_id?,favorite_only?,query?}` | `Vec<ItemDto>` | F-04/F-08 |
| `get_item` | `{item_id}` | `ItemDto` | F-04 |
| `create_item` | `{draft}` | `ItemDto` | F-04 |
| `update_item` | `{item_id,draft}` | `ItemDto` | F-04 |
| `delete_item` | `{item_id,permanent}` | `()` | 走回收站（F-15） |
| `toggle_favorite` | `{item_id,favorite}` | `ItemDto` | F-08（#531 起仅标记，不再置顶） |
| `record_item_usage` | `{item_id}` | `()` | #531 复制埋点（60s 节流，窗口内 no-op） |
| `set_item_pin` | `{item_id,pinned}` | `()` | #531 钉住/取消（拖拽置顶） |
| `reorder_pinned_items` | `{ordered_ids}` | `()` | #531 钉住块内重排（id 须已钉住） |
| `set_folder_pin` | `{folder_id,pinned}` | `()` | #531 分组钉住/取消 |
| `reorder_pinned_folders` | `{ordered_ids}` | `()` | #531 分组钉住块重排 |
| `move_item_to_folder` | `{item_id,folder_id?}` | `()` | #531 归组/移出未分组（目标须存在） |
| `import_attachment` | `{source_path,title}` | `ItemDto` | F-12（超限报错） |
| `export_attachment` | `{item_id,dest_path}` | `()` | F-12（明文风险由前端提示） |
| `export_vault` | `{dest_path,master_password}` | `()` | O-03 备份 |
| `get_auto_lock` | — | `u16` | D-61 读自动锁定档位（`Vault.auto_lock_minutes` 明文头部） |
| `set_auto_lock` | `{minutes}` | `()` | D-61 设档位（仅 1/5/15，非法 `ERR_VALIDATION`；幂等） |
| `generate_password` | `GeneratorOptions` | `{password}` | F-07 |
| `copy_secret` | `{text,ttl_secs}` | `()` | F-06（含 30s 清空调度） |
| `open_url` | `{url}` | `()` | F-05（协议白名单） |
| `totp_now` | `{secret}` | `{code,period,remaining}` | F-11 |
| `import_preview` | `{source_path,format}` | `ImportPreviewDto` | F-14 预览（D-87：源编码容错 UTF-8 BOM/UTF-16/GBK） |
| `import_apply` | `{source_path,format,preview_token}` | `ImportResultDto` | F-14 执行（D-90：同名文件夹合并 + 同名同型条目跳过，绝不覆盖现有数据；`duplicate_skipped`/`merged_folders` 计数） |
| `list_trash` | — | `Vec<TrashDto>` | F-15 |
| `restore_item` | `{trash_id}` | `ItemDto` | F-15 |
| `purge_item` | `{trash_id}` | `()` | F-15 |
| `empty_trash` | — | `()` | F-15（二次确认前端） |
| `health_check` | `HealthConfig?` | `HealthReportDto` | F-16 |
| `sync_start_pairing` | — | `PairingDto{qr_svg,ip,port,expires_at}` | F-09 Host |
| `sync_cancel_pairing` | — | `()` | 取消并作废令牌 |
| `sync_join` | `{qr_payload}` | `PairResultDto` | F-09 Joiner |
| `list_paired_devices` | — | `Vec<DeviceDto>` | F-09（D-134：含 `peer_ip` / `peer_port`） |
| `unpair` | `{device_id}` | `()` | O-07 废弃共享密钥 |
| `sync_now` | `{device_id}` | `SyncResultDto` | 手动同步 |
| `sync_status` | — | `SyncStatusDto` | 状态查询（库里的事实） |
| `sync_net_info` | — | `SyncNetInfoDto` | D-134：本机 IPv4 / 监听端口 / 发现应答是否可用（只读诊断，不取 session 锁、不拉起监听器） |
| `list_conflicts` | — | `Vec<ConflictDto>` | 冲突箱 |
| `resolve_conflict` | `{conflict_id,choice,copy_suffix}` | `()` | 三选一（D-112：copy_suffix=KeepBoth 副本标题后缀，前端 i18n 传入） |
| `get_settings` / `update_settings` | `VaultSettings` | `VaultSettings` | 设置 |
| `vault_file_info` | — | `{path,size_bytes}` | P-10 展示 |

**`GeneratorOptions`**：`{length:u8, uppercase:bool, lowercase:bool, digits:bool, symbols:bool, exclude_ambiguous:bool}`（默认 length=16）。
**`HealthConfig`**：`{weak_max_len:u8=8, stale_days:u32=180, ...}`。
**`IpcError`**：`{code,message_key}`。

### 5.3 前端 store 与 API 包装层

```ts
// src/lib/ipc.ts —— 唯一 invoke 入口（类型安全）
export const api = {
  getStatus: () => invoke<StatusDto>('get_status'),
  unlock: (master_password: string) => invoke<StatusDto>('unlock', { master_password }),
  listItems: (f?: ItemFilter) => invoke<ItemDto[]>('list_items', { folderId: f?.folderId, favoriteOnly: f?.favoriteOnly, query: f?.query }),
  // ... 其余命令一一对应，参数键名与 Rust 命令签名严格一致
};
```

| Store | 结构 | 职责 |
|-------|------|------|
| `session` | `{hasVault,locked,vaultMeta}` | 锁定态、库元信息；`lock()` 清空并跳转 |
| `vault` | `{items,folderMap,trash,conflicts,filter}` + 派生 `visibleItems/search` | 业务数据与视图过滤 |
| `settings` | `VaultSettings` 镜像 | 设置读写 |
| `ui` | `{route,toast,modal,confirm,drawerOpen,selectedId}` | 视图与交互状态 |

> 规则：**组件不直接调用 `invoke`**，一律经 `api.*`；`api.*` 出错统一 `throw IpcError`，由 `errors.ts` 映射文案。

---

## 6. 程序调用流程

### ① 首启创建保险库

```mermaid
sequenceDiagram
  autonumber
  participant U as 用户
  participant UI as CreateVaultWizard(Svelte)
  participant API as ipc.api
  participant ST as AppState
  participant CORE as passwordms-core
  U->>UI: 两次输入主密码
  UI->>UI: 校验一致 & 长度>=8 & 强度提示
  UI->>API: create_vault(master_password)
  API->>ST: invoke#[tauri::command] create_vault
  ST->>CORE: Session::create(master_password)
  CORE->>CORE: getrandom 生成 salt/VK
  CORE->>CORE: Argon2id -> MK -> HKDF -> KEK
  CORE->>CORE: KEK 包裹 VK -> wrapped_vault_key; 计算 verify_mac
  CORE->>CORE: container 初始化(header 明文 + 空记录区)
  CORE->>CORE: 落盘 passwordms.vault
  CORE-->>ST: Session(Unlocked, VK in Zeroizing)
  ST-->>API: StatusDto{has_vault:true,locked:false}
  API-->>UI: 成功
  UI-->>U: 进入主列表页 P-02
```

### ② 解锁

```mermaid
sequenceDiagram
  autonumber
  participant U as 用户
  participant UI as LockScreen
  participant API as ipc.api
  participant ST as AppState
  participant CORE as passwordms-core
  U->>UI: 输入主密码
  UI->>API: unlock(master_password)
  API->>ST: create_vault command? no -> unlock
  ST->>CORE: Session::unlock(master_password)
  CORE->>CORE: 读取 header(kdf params/salt) 
  CORE->>CORE: Argon2id -> MK -> KEK
  CORE->>CORE: 常量时间校验 verify_mac
  alt 校验失败
    CORE-->>ST: Err(ERR_UNLOCK_FAILED)  // 统一错误
    ST-->>API: IpcError{code:ERR_UNLOCK_FAILED}
    API-->>UI: 统一失败提示
  else 校验通过
    CORE->>CORE: KEK 解包 VK; 扫描 record_area 建立索引; 解密条目
    CORE-->>ST: Session(Unlocked, 解密数据)
    ST-->>API: StatusDto{locked:false}
    API-->>UI: 进入主列表页
  end
```

### ③ 新增条目并保存（逐条加密落盘）

```mermaid
sequenceDiagram
  autonumber
  participant U as 用户
  participant ED as ItemEditor
  participant API as ipc.api
  participant CORE as passwordms-core
  participant FS as 磁盘(passwordms.vault)
  U->>ED: 填写字段(可唤起生成器)并点保存
  ED->>API: create_item(draft)
  API->>CORE: repository::create_item(draft)
  CORE->>CORE: getrandom 生成 item_id + IK(32B)
  CORE->>CORE: AEAD(IK, nonce, payload, aad=0x02||item_id)
  CORE->>CORE: VK 包裹 IK -> wrapped_item_key
  CORE->>CORE: 追加记录帧(type=0x02 Item)
  CORE->>FS: pwrite 新增帧(仅该帧)
  CORE-->>API: ItemDto
  API-->>ED: 成功
  ED-->>U: 列表出现新条目
```

### ④ 二维码配对与首次同步（含冲突产生与三选一）

```mermaid
sequenceDiagram
  autonumber
  participant PC as Windows(Host)
  participant PH as Android(Joiner)
  participant U as 用户
  PC->>PC: 生成一次性令牌+临时X25519, 绑定端口, 展示二维码
  PH->>PH: 扫码得到 IP/端口/令牌/指纹; 生成临时X25519
  PH->>PC: HELLO{token, joiner_pub}
  PC->>PC: 校验令牌(未用&未过期&匹配)
  alt 校验失败
    PC-->>PH: REJECT{ERR_PAIR_TOKEN_*}
  else 校验通过
    PC-->>PH: ACCEPT{host_pub, session_id, session_salt}
    PC->>PC: shared=X25519 -> sync_key/confirm_key
    PH->>PH: shared=X25519 -> sync_key/confirm_key
    PH->>PC: CONFIRM{mac_j};  PC->>PH: CONFIRM{mac_h}
    PC->>PC: 校验通过 -> 配对成立, 下发 VK(用 sk 加密)
    PH->>PH: 用同一 VK, 持久化配对设备
    PH->>PC: PUSH changes(seq=1, 密文 ChangeRecord)
    PC->>PC: merge: 检测双改
    alt 存在冲突
      PC-->>PH: conflict_notice{冲突 ID 清单}
      PC->>PC: 写入 ConflictEntry(Local保留, Remote入箱)
      PC-->>U: UI 三选一提示
      U->>PC: 选择(保留本机/采用远端/两者都留)
      PC->>PC: resolve_conflict(choice)
    else 无冲突
      PC->>PC: 应用变更, 推进 sync_state
    end
    PC-->>PH: done
    PH-->>U: 同步完成提示
  end
```

> 时序图源文件另存于 `docs/sequence-diagram.mermaid`；类图另存于 `docs/class-diagram.mermaid`。

---

## 7. 任务列表

> 共 **16 项**，按依赖顺序排列。每项含：编号 / 标题 / 涉及文件 / 前置依赖 / 验收要点。

| 编号 | 标题 | 涉及文件 | 前置依赖 | 验收要点 |
|------|------|---------|---------|---------|
| **T01** | **项目基础设施与工作区** | `Cargo.toml`、`package.json`、`vite.config.ts`、`svelte.config.js`、`tsconfig.json`、`index.html`、`src/main.ts`、`src/App.svelte`、`src-tauri/Cargo.toml`、`src-tauri/build.rs`、`src-tauri/tauri.conf.json`、`src-tauri/capabilities/default.json`、`src-tauri/src/main.rs`、`src-tauri/src/lib.rs`（空壳）、`src-tauri/icons/*`、`.gitignore` | — | 前端 `vite build` 通过；`cargo build --workspace` 通过；两端均可空跑启动一个「Hello」窗口/Activity |
| **T02** | **Rust 核心 · 模型与错误基线** | `crates/passwordms-core/src/{lib.rs,error.rs,constants.rs,securemem.rs}`、`crates/passwordms-core/src/model/{mod,vault,folder,item,trash,conflict,sync_state}.rs`、`crates/passwordms-core/Cargo.toml` | T01 | 全部模型可 serde 往返；`CoreError` 覆盖 §9.2 全部错误码；`Secret` 类型具备 `ZeroizeOnDrop` |
| **T03** | **Rust 核心 · 加密层** | `crates/passwordms-core/src/crypto/{mod,kdf,aead,keys,rng,verify}.rs` | T02 | Argon2id 参数=§2.2；XChaCha20-Poly1305 加解密；KEK 包裹/解包 VK 往返；`crypto_vectors.rs` 通过；解密失败返回 `ERR_UNLOCK_FAILED` |
| **T04** | **Rust 核心 · 容器与仓储** | `crates/passwordms-core/src/store/{mod,container,frame,repository}.rs` | T02, T03 | 容器按 §2.6 布局读写；单条更新字节差异 ≈ 单条（A-02）；`container_roundtrip.rs` 通过 |
| **T05** | **Rust 核心 · 生成器 / 体检 / TOTP** | `crates/passwordms-core/src/{generator,health,totp}.rs` | T02 | 生成器遵守字符集与排除规则；体检三规则（弱口令/重复/超期 180 天）；TOTP 6 位 + 周期 30s；`health_and_generator.rs` 通过 |
| **T06** | **Rust 核心 · Bitwarden 导入** | `crates/passwordms-core/src/import/{mod,bitwarden_json,bitwarden_csv}.rs` | T02, T04 | JSON/CSV 映射正确；type=3/4 跳过并计数；多 URI 取第一条（Q2）；加密导出文件报错；`import_bitwarden.rs` 通过 |
| **T07** | **Rust 核心 · 同步协议** | `crates/passwordms-core/src/sync/{mod,protocol,handshake,transport,server,client,merge,replay}.rs`、`crates/passwordms-core/src/model/sync_state.rs` | T03, T04 | 配对握手 + X25519 + 密钥确认；防重放（seq/时间窗）；冲突检测与冲突箱；`sync_conflict.rs` / `sync_replay.rs` 通过（A-06/07/08） |
| **T08** | **Tauri IPC 命令层** | `src-tauri/src/{lib.rs,state.rs,dto.rs,error.rs}`、`src-tauri/src/commands/{mod,system_cmd,vault_cmd,item_cmd,folder_cmd,keydoc_cmd,generator_cmd,import_cmd,trash_cmd,health_cmd,sync_cmd,settings_cmd}.rs` | T03, T04, T05, T06, T07 | §5.2 命令全部注册；`CoreError`→`IpcError{code,message_key}` 映射；`get_status/create_vault/unlock/lock` 冒烟通过 |
| **T09** | **前端基础设施 · IPC/类型/状态/通用件** | `src/types.ts`、`src/lib/{ipc,errors,constants,clipboard,qr,url-safety}.ts`、`src/lib/stores/{session,vault,settings,ui}.ts`、`src/lib/i18n/{index,zh-CN,en-US}.ts`、`src/styles/global.css`、`src/components/common/{Toast,ConfirmDialog,Modal,Icon,StrengthMeter,Spinner}.svelte` | T01 | `api.*` 覆盖 §5.2 全部命令；`Icon` 为内联 SVG（无字体）；`clipboard.ts` 实现「仅清自有内容」；`url-safety.ts` 仅放行 http/https |
| **T10** | **前端 · 解锁与创建保险库** | `src/components/{LockScreen,CreateVaultWizard}.svelte`、`src/App.svelte`（路由接入） | T09 | 首启向导（密码+确认+强度）；解锁统一失败提示；进入主列表 |
| **T11** | **前端 · 主界面与分组** | `src/components/{MainLayout,FolderSidebar,ItemList,ItemDetail}.svelte` | T09 | 桌面三栏/手机单栏自适应；搜索模糊命中；收藏置顶；一键复制 + 一键直达（协议校验）；TOTP 倒计时展示 |
| **T12** | **前端 · 条目编辑与生成器** | `src/components/{ItemEditor,PasswordGenerator}.svelte` | T09, T11 | 类型切换动态字段；自定义键值对增行/隐藏；生成器联动密码字段 |
| **T13** | **前端 · 密钥文档（笔记 + 附件）** | `src/components/{SecureNoteView,AttachmentView}.svelte` | T09, T11 | 笔记等宽展示 + 折叠；附件元信息 + 导出（明文风险提示 + 二次确认） |
| **T14** | **前端 · 导入 / 体检 / 回收站 / 设置** | `src/components/{ImportWizard,HealthCheck,Trash,Settings}.svelte` | T09 | 导入预览/映射表/跳过计数；体检三维度卡；回收站还原/彻底删除/清空（二次确认）；设置含自动锁档位与体检阈值 |
| **T15** | **前端 · 同步配对与冲突仲裁** | `src/components/sync/{PairShow,PairScan,DeviceList,SyncStatus,ConflictPanel}.svelte`、`src/lib/qr.ts`（扫码接入）、`src/lib/totp.ts` | T09, T11 | 二维码展示 + 倒计时/刷新；扫码 + 手动兜底；设备列表与解除配对（二次确认）；冲突三选一 UI 接入 `resolve_conflict` |
| **T16** | **脚本 · 端到端集成 · 体积核验** | `scripts/{build.sh,build.ps1,build-android.sh,check-size.sh}`、`src-tauri/tauri.conf.json`（bundle 定稿）、README 片段 | T08, T10, T11, T12, T13, T14, T15 | 一键构建双端产物；`check-size.sh` 断言 Win ≤ 10 MB / APK ≤ 12 MB；全链路冒烟（建库→解锁→增改→配对→同步→冲突→导入→体检） |

**任务依赖图**

```mermaid
graph TD
  T01[T01 基础设施] --> T02[T02 模型与错误]
  T01 --> T09[T09 前端基础设施]
  T02 --> T03[T03 加密层]
  T02 --> T05[T05 生成器/体检/TOTP]
  T03 --> T04[T04 容器与仓储]
  T02 --> T04
  T04 --> T06[T06 Bitwarden 导入]
  T03 --> T07[T07 同步协议]
  T04 --> T07
  T03 --> T08[T08 Tauri IPC]
  T04 --> T08
  T05 --> T08
  T06 --> T08
  T07 --> T08
  T09 --> T10[T10 解锁/建库]
  T09 --> T11[T11 主界面/分组]
  T09 --> T12[T12 编辑/生成器]
  T11 --> T12
  T09 --> T13[T13 密钥文档]
  T11 --> T13
  T09 --> T14[T14 导入/体检/回收站/设置]
  T09 --> T15[T15 同步/冲突 UI]
  T11 --> T15
  T08 --> T16[T16 脚本/体积核验]
  T10 --> T16
  T11 --> T16
  T12 --> T16
  T13 --> T16
  T14 --> T16
  T15 --> T16
```

---

## 8. 依赖包列表

### 8.1 Rust crates（`passwordms-core` + `src-tauri`）

| crate | 版本 | 用途 | 影响体积 |
|-------|------|------|---------|
| `tauri` | `2` (2.1) | 应用外壳（仅 src-tauri） | 是（运行时主体，必要） |
| `tauri-plugin-dialog` | `2` | 文件选择/另存（附件、导入、导出） | 轻微 |
| `tauri-plugin-clipboard-manager` | `2` | 剪贴板读写（F-06） | 轻微 |
| `tauri-plugin-opener` | `2` | 调用系统浏览器打开 URL（F-05） | 轻微 |
| `tauri-plugin-fs` | `2` | 前端受限文件访问（导出路径） | 轻微 |
| `serde` | `1` | 序列化 | 轻 |
| `serde_json` | `1` | JSON 编解码（头部、同步报文） | 轻 |
| `thiserror` | `2` | 错误类型派生 | 轻 |
| `argon2` | `0.5` | Argon2id KDF | 轻 |
| `chacha20poly1305` | `0.10` | XChaCha20-Poly1305 AEAD | 轻 |
| `hkdf` | `0.12` | HKDF-SHA256 派生 | 轻 |
| `sha2` | `0.10` | SHA-256（指纹、哈希） | 轻 |
| `hmac` | `0.12` | HMAC（密钥确认、verify_mac） | 轻 |
| `sha1` | `0.10` | TOTP（HMAC-SHA1，RFC 6238） | 轻 |
| `base32` | `0.5` | TOTP 密钥 Base32 编解码 | 轻 |
| `x25519-dalek` | `2` | 配对 ECDH | 轻 |
| `getrandom` | `0.2` | OS CSPRNG | 轻 |
| `zeroize` | `1.8` | 敏感内存清零 | 轻 |
| `subtle` | `2.6` | 常量时间比较 | 轻 |
| `uuid` | `1` (`v4`,`serde`) | ID 生成 | 轻 |
| `base64` | `0.22` | 二进制↔文本 | 轻 |
| `qrcode` | `0.14`（`default-features=false`，仅 SVG） | 二维码生成 | 轻（**不启用 image/png 特性**） |
| `url` | `2` | URL 协议校验（F-05） | 轻 |
| `if-addrs` | `0.13` | 多网卡枚举候选局域网 IPv4（D-44，入表补录） | 轻 |
| `encoding_rs` | `0.8` | 导入源编码容错：UTF-16 BOM / GBK → UTF-8（D-87；Firefox 同款实现，非自研） | 轻（纯数据表，无系统依赖） |

> **TOTP 说明**：不引入独立 `totp-rs`，改用 `hmac`+`sha1`+`base32` 组合实现 RFC 6238（HMAC-SHA1 为 RustCrypto 审计实现，非自研密码学），进一步压体积。

### 8.2 前端 npm 包

| 包 | 版本 | 用途 |
|----|------|------|
| `svelte` | `^5.0` | UI 框架（编译期，产物小） |
| `vite` | `^6.0` | 构建 |
| `@sveltejs/vite-plugin-svelte` | `^4.0` | Svelte × Vite 集成 |
| `typescript` | `^5.6` | 类型系统 |
| `svelte-check` | `^4.0` | Svelte TS 检查（dev） |
| `@tauri-apps/api` | `^2.0` | `invoke`/`platform` 前端 API |
| `@tauri-apps/plugin-dialog` | `^2.0` | 文件选择对话框 |
| `@tauri-apps/plugin-clipboard-manager` | `^2.0` | 剪贴板 |
| `@tauri-apps/plugin-opener` | `^2.0` | 打开 URL |
| `@tauri-apps/plugin-fs` | `^2.0` | 受限文件访问 |
| `jsqr` | `^1.4` | 二维码扫描（摄像头帧解析，~数十 KB） |
| `vitest` | `^2.0` | 前端单测（dev） |

> **禁止列入**（体积红线）：`electron`、`react`/`antd`/`@mui/*`、`bootstrap`、`@zxing/*`（改 `jsqr`）、任何图标字体包（`@fortawesome/*`、`material-icons`）、`echarts`/`d3`。

---

## 9. 共享知识（跨文件约定）

### 9.1 命名约定

| 领域 | 规则 |
|------|------|
| Rust 标识符 | `snake_case`（类型 `PascalCase`） |
| **wire / IPC JSON** | **`snake_case`**（Rust `serde` 默认；前端 TS 接口**直接沿用 snake_case**，不做自动转换） |
| 前端局部变量 | 可 `camelCase`，但**凡对应 DTO 的字段一律用 snake_case** |
| 文件名 | Rust/Tauri：`snake_case.rs`；Svelte 组件：`PascalCase.svelte`；工具：`kebab-case.ts` |
| 常量 | Rust `SCREAMING_SNAKE_CASE`；TS `SCREAMING_SNAKE_CASE`，同名同值 |
| Mermaid/文档 | 中文优先 |

### 9.2 错误码表（Rust `CoreError` ↔ TS `ErrorCode`）

| code | 含义 | 用户可见文案 key |
|------|------|-----------------|
| `ERR_UNLOCK_FAILED` | 解锁失败（**统一**，不区分密码错/数据损坏） | `err.unlock_failed` |
| `ERR_WRONG_MASTER_PASSWORD` | **已解锁会话内**复核主密码失败（D-115：`verify_master_password` / `change_master_password` 旧密码 / `export_vault`）。与上一行**分工按调用位置**：本码只可能出现于已认证会话，故可以点名「主密码」 | `err.wrong_master_password` |
| `ERR_NO_VAULT` | 本地无保险库 | `err.no_vault` |
| `ERR_VAULT_EXISTS` | 保险库已存在 | `err.vault_exists` |
| `ERR_WEAK_MASTER_PASSWORD` | 主密码过弱（< 8 位） | `err.weak_password` |
| `ERR_NOT_UNLOCKED` | 未解锁调用 | `err.not_unlocked` |
| `ERR_ITEM_NOT_FOUND` | 条目不存在 | `err.item_not_found` |
| `ERR_FOLDER_NOT_FOUND` | 分组不存在 | `err.folder_not_found` |
| `ERR_FOLDER_NAME_INVALID` | 分组名空/重复 | `err.folder_name` |
| `ERR_VALIDATION` | 字段校验失败 | `err.validation` |
| `ERR_ATTACHMENT_TOO_LARGE` | 单附件超 20 MB | `err.attachment_too_large` |
| `ERR_VAULT_CAPACITY_EXCEEDED` | 库总量超 200 MB | `err.vault_capacity` |
| `ERR_UNSAFE_URL` | URL 协议不安全 | `err.unsafe_url` |
| `ERR_IMPORT_PARSE` | 导入文件解析失败 | `err.import_parse` |
| `ERR_IMPORT_ENCRYPTED_EXPORT` | 导入的是加密导出文件 | `err.import_encrypted` |
| `ERR_PAIR_TOKEN_INVALID` | 令牌不匹配 | `err.pair_token_invalid` |
| `ERR_PAIR_TOKEN_EXPIRED` | 令牌过期 | `err.pair_token_expired` |
| `ERR_PAIR_TOKEN_USED` | 令牌已使用 | `err.pair_token_used` |
| `ERR_NOT_PAIRED` | 未配对设备 | `err.not_paired` |
| `ERR_REPLAY_DETECTED` | 检测到重放 | `err.replay` |
| `ERR_FINGERPRINT_MISMATCH` | 指纹/密钥确认失败 | `err.fingerprint` |
| `ERR_SYNC_ABORTED` | 同步被中断 | `err.sync_aborted` |
| `ERR_CONFLICT_EXISTS` | 存在未仲裁冲突 | `err.conflict` |
| `ERR_RESTORE_CONFLICT` | 【D-91】回收站还原被拒（同 item_id 已有活跃条目） | `err.restore_conflict` |
| `ERR_IO` | 文件 IO 错误 | `err.io` |
| `ERR_CRYPTO` | 密码学操作失败 | `err.crypto` |
| `ERR_INTERNAL` | 内部错误 | `err.internal` |

> **错误回传格式（IPC）**：`{ "code": "ERR_...", "message_key": "err.xxx" }`。**日志中只记录 code，不记录任何参数值。**

### 9.3 常量定义位置

| 常量 | 值 | Rust 位置 | TS 位置 |
|------|-----|-----------|---------|
| Argon2 m/t/p/salt_len | 65536 KiB / 3 / 1 / 16 | `constants.rs::KDF_*` | `constants.ts::KDF_*` |
| AEAD 算法/nonce | `xchacha20poly1305` / 24B | `constants.rs` | `constants.ts` |
| 单附件上限 | **20 MB** | `constants.rs::MAX_ATTACHMENT_BYTES` | `constants.ts::MAX_ATTACHMENT_BYTES` |
| 库总容量上限 | **200 MB** | `constants.rs::MAX_VAULT_BYTES` | `constants.ts::MAX_VAULT_BYTES` |
| 安全笔记正文上限 | **256 KiB** | `constants.rs::MAX_NOTE_BYTES` | `constants.ts::MAX_NOTE_BYTES` |
| 剪贴板清空 | 30 s | `constants.rs::CLIPBOARD_TTL_SECS` | 同 |
| 自动锁默认 | 5 分钟（1/5/15） | `constants.rs::AUTO_LOCK_MINUTES` | 同 |
| 配对令牌有效期 | 120 s | `constants.rs::PAIR_TOKEN_TTL_SECS` | 同 |
| 同步时间窗 | ±120 s | `constants.rs::SYNC_TIME_WINDOW_SECS` | 同 |
| 同步重试 | 3 次，退避 1/2/4s | `constants.rs::SYNC_RETRY_*` | 同 |
| 体检默认阈值 | 弱口令<8 / 超期 180 天 | `constants.rs::HEALTH_*` | 同 |
| 回收站保留 | 30 天 | `constants.rs::TRASH_RETENTION_DAYS` | 同 |
| 协议版本 | QR `v=1` / 容器 `format_version=1` | `constants.rs` | `constants.ts` |

### 9.4 时间戳与编码

- **时间戳**：一律 **UTC Unix 毫秒（`i64`）**，展示时按本地时区格式化；不使用本地时区参与存储。
- **二进制**：JSON 内一律 **标准 Base64（带 `=` 填充）**。
- **二维码 / 令牌 / URL-safe**：使用 **base64url（无填充）**。
- **文本**：UTF-8。
- **UUID**：小写带连字符（帧头二进制为 16 字节原始形式）。

### 9.5 字段命名风格转换规则（前端 ↔ Rust）

- **不转换**：wire 层统一 `snake_case`；TS 接口与 Rust `serde` 输出**逐字段同名**。
- 前端组件内部若使用局部驼峰变量，**必须在调用 `api.*` 时映射回 snake_case**；`api.*` 参数键名与 Rust 命令签名严格一致。
- 唯一例外：Mermaid/文档中的中文说明不受此约束。

### 9.6 i18n 约定（最小实现）

- **本期只交付简体中文（zh-CN）**，但采用 **key-based** 结构，便于二期扩展。
- `t(key, vars?)` 极简实现：查表 + `{name}` 占位替换，无第三方 i18n 库（控体积）。
- 文案集中在 `src/lib/i18n/zh-CN.ts`；`en-US.ts` 提供占位与语言切换骨架。
- 错误码 → 文案经 `errors.ts` 映射到 `err.*` key。
- 日期/数字格式化用原生 `Intl`，按系统语言。

---

### 3.14.29 系统集成：托盘 / 全局热键 / 开机自启（1.4.10，2026-10-29）

**新增三个模块 + 一个新命令组**（`src-tauri/src/`）：

| 模块 | 职责 | 关键约束 |
|---|---|---|
| `prefs.rs` | 应用级配置 `AppPrefs{hotkey, autostart, tray_hint_shown}`，存 `data_dir/prefs.json` | 纯逻辑（路径由调用方给），可单测；**永不因坏配置失败**——解析失败/字段非法都回落默认值并返回告警串给调用方记日志；原子写（`.tmp` + rename）；`#[serde(default)]` 向前兼容 |
| `tray.rs` | 托盘图标 + 菜单 + `CloseRequested` 拦截 + 首次教育提示 | 托盘缺失时**放过** `CloseRequested`（恢复旧行为）；首次点 X 只提示不隐藏 |
| `hotkey.rs` | 注册/改键/回滚/启动装载 | 改键 = 先 `unregister_all` 再注册，失败按旧值回滚；`Pressed` 状态才响应（否则窗口闪两下） |
| `commands/prefs_cmd.rs` | `prefs_get` / `prefs_set_hotkey` / `prefs_set_autostart` / `tray_set_labels` | 顺序恒为「**先让系统状态成立，再落盘**」——注册不上就不写配置，避免"设置里写着却没生效" |
| `app_events.rs` | 后端→前端事件名常量（3 个） | 与前端 `tray-events.ts` / `auto-lock-event.ts` 字面值跨语言契约，`ipc-contract.test.ts` 逐字比对 + 检查**常量真的被 emit**（只比值挡不住"定义了从没发"） |

**为什么配置不进 `VaultSettings`**：那份设置在保险库内、随库加密，未解锁根本读不到。而托盘菜单、关窗行为、开机自启、热键键位这几件事恰恰发生在锁屏状态——甚至发生在一台从没解锁过的新机器上（开机自启要在启动瞬间就知道开没开）。放库里等于「保险库锁着的时候热键注册不出来」，而那正是最需要它的时刻。

**路径用 `data_dir` 而非 `app_config_dir`**：`data_dir` 已实现便携模式（exe 旁存在 `passwordms.portable` 时全部数据随 exe 走）。配置留系统盘会让绿色版"换台机器拷走目录"时热键/自启设置莫名丢失。

**热键键名的实测事实**（推翻了设计时的假设）：`Shortcut`（global-hotkey 0.8 / keyboard-types 0.7）对 `P` 与 `KeyP` **两种写法都接受**。因此不能靠"解析失败"来拒绝裸字母——必须在 `canonical_hotkey` 里主动归一（单字母 → `KeyX`、单数字 → `DigitX`），否则同一个物理键会存出两种规范化结果，"改个写法"在配置里看起来像换了键，去重与回滚判断都会漂。

**校验器与注册器同一个**：`canonical_hotkey` 末尾直接 `canonical.parse::<Shortcut>()`，杜绝"校验通过、注册时才报错"的两套标准。

**新增依赖**（均为官方插件，且**只在 Rust 侧调用**，前端不装 JS 包 → `capabilities/default.json` 零新增权限）：`tauri` 的 `tray-icon` + `image-png` feature、`tauri-plugin-global-shortcut 2.4.1`、`tauri-plugin-autostart 2.7.0`。

**这四个命令必须分平台写两份实现**（v1.4.10 的实测教训，不是风格选择）：`tauri::menu` / `tauri::tray` 在移动端**根本不存在**，`global-shortcut` / `autostart` 两个插件也**没有移动端实现**。第一版把它们放进公共 `[dependencies]`，`cargo check --target aarch64-linux-android` 当场炸出 17 个错误。正确形态是三层同时收口：

1. `src-tauri/Cargo.toml` 用 `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]` 挂两个插件——**不能放公共 `[dependencies]`**，否则移动端连依赖解析都过不去；
2. `lib.rs` 里 `hotkey` / `tray` 两个 `pub mod` 与 builder 上的 `.plugin()` / `.on_window_event()` 全部 `#[cfg(desktop)]`；
3. `commands/prefs_cmd.rs` 给同名同签名的**两套** `#[tauri::command]`（`cfg(desktop)` 真实现 + `cfg(not(desktop))` 桩）。**不要**在函数体内塞 `cfg` 分支——那样桌面 API 的引用仍留在移动端编译单元里，错误照旧。

移动端桩的语义要显式：`prefs_get` 返回 `PrefsDto{supported:false}`（不是报错——前端需要"这个平台没有这功能"这个**可读事实**，而不是一个只能 catch 的失败），三个写命令返回 `ERR_NOT_SUPPORTED`。`supported` 由后端自报、前端不做 UA 嗅探（D-116 的同一条纪律）。

**代价与守门**：命令名仍是 67 个，但 `#[tauri::command]` 属性出现 71 次（这 4 个命令各两份）。`ipc-contract.test.ts` 因此钉两个数：属性 71/71（每个都带 `rename_all`）+ **去重后的名字 = 67**。第二个数才是真守门——若移动端桩被写成 `prefs_get_mobile`，71/71 照样绿，而 Android 上 invoke `prefs_get` 直接 `command not found`。

### 3.14.30 快速搜索小窗（1.4.11）

**新增一个桌面专属模块** `src-tauri/src/quick_search.rs`（`#[cfg(desktop)]`，移动端没有第二个窗口这回事），**零新增 IPC 命令**。

| 关注点 | 做法 | 为什么只能这么做 |
|---|---|---|
| 窗口创建 | `setup` 里 `ensure` 一个隐藏的 `WebviewWindowBuilder`，`WebviewUrl::App("index.html?w=quick")` | 热键最需要立刻回应的时刻是刚开机、主窗口缩在托盘时；首次建 WebView 要几百毫秒，那时按下去就是"没反应" |
| 窗口销毁 | 不主动销毁，只 `hide`；`ensure` 发现窗口没了（Alt+F4）就重建 | 隐藏窗口复用 = 零等待；重建覆盖"用户把它关掉了"这条真实路径 |
| 位置 | `cursor_position` → `monitor_from_point` → 该屏居中 | `center()` 只认主显示器，多屏场景会跳到另一块屏 |
| 尺寸换算 | 逻辑像素 × `monitor.scale_factor()` 后再算物理坐标 | 不换算会在 125%/150% 缩放的机器上偏移 |
| 热键路由 | `AppState::session_unlocked()` 决定唤小窗还是主窗口 | 锁屏态小窗没有明文可取，此时用户唯一要做的是输主密码 |
| 失焦 | `on_window_event` 里 `Focused(false)` → `hide` | 复制完要切到目标程序，小窗必须让开 |

**它是第二个 WebView，这句话的全部后果**：

1. **JS 上下文独立**：主窗口的 `vault` / `session` store 在小窗里是空的，也不会互相同步。所以小窗数据一律现取：`get_status`（判锁）→ `list_items`（列表）→ `get_item`（取明文）→ `copy_secret`（写剪贴板 + TTL）→ `record_usage`（频度埋点）。**列表接口本身不带明文密码**（`wipe_plaintext` 的既定行为），想省一次 IPC 直接拿列表里的 `password` 只会拿到 `null`。
2. **`localStorage` 共享**（同 origin）：`passwordms.locale` / `passwordms.theme` 两个键直接读，语言与主题自动继承，不需要宿主传参。
3. **前端入口要分流**：`main.ts` 按 `isQuickWindow(location.search)` 动态 `import`，小窗不解析主界面那 500KB 组件。分流必须发生在 `mount` 之前（窗口 label 要异步取，来不及）。
4. **权限要单独给**：`capabilities/default.json` 的 `windows` 只含 `"main"`，小窗拿不到任何窗口权限。新增 `capabilities/quick.json`，`windows: ["quick"]`，权限只有 `core:default` + `core:window:allow-hide`——它只需"把自己藏起来"。

**事件复用而非新增**：`app://quick-search` 在 1.4.10 是发给主窗口（唤窗 + 聚焦搜索框），1.4.11 接收方变成小窗自己（清空旧词 + 聚焦输入框），主窗口那条监听删除。理由：**新增一条事件等于新增一处跨语言字面值契约**，而这里语义本来就相同（"窗口刚被唤出来，把光标放好"）。

**实测代价**（2026-10-29，本机 Win11，另有主人在运行的 1.4.10 实例）：

| 版本 | 新增 `msedgewebview2` 进程 | 新增进程合计 WorkingSet | 应用本体 |
|---|---|---|---|
| 1.4.10（单窗口） | 2 | 71.0 MB | 32.2 MB |
| 1.4.11（主窗 + 预建小窗） | 3 | 134.9 MB | 33.1 MB |

⇒ **小窗 ≈ +1 个 WebView2 渲染进程、约 +64 MB 常驻内存**。这就是"预建隐藏窗口"的真实代价：用 64MB 换掉首次唤出的数百毫秒。

测法：启动前记录全部 `msedgewebview2` 的 pid，启动后只统计**新出现的 pid** 的 WorkingSet 之和。**不能直接做前后总量差**——本机其它应用的 webview 进程自身在 GC，实测两次基线相差 55MB，差值法会得出"1.4.11 比 1.4.10 更省内存"这种荒谬结论。

**已知边界**：小窗不显示 TOTP 动态码；不支持拖动（无需拖动，也省一项权限）；热键只在桌面存在，Android 上整个模块不参与编译（`cargo check --target aarch64-linux-android` 零错误）。

**开机自启以系统为准**：`prefs.autostart` 只是"上次我们设置的值"；界面读的是 `autolaunch().is_enabled()`，用户在任务管理器里禁用后界面如实显示关闭。

### 3.14.31 单实例守卫（D-128，1.4.12）

**为什么非做不可**：v1.4.10 起关闭窗口只是**隐藏到托盘**，进程常驻。用户以为退出了，再点一次图标就
成了第二个实例。两个实例共用同一个 `passwordms.vault`、同一个 `biometric.json`，并且**同时抢 TPM 上
那把持久密钥**（RSA 解密在 TPM 内串行，`retry_transient` 重试耗尽后返回 `Platform`——它会被归类成
「密文被篡改」，正是 D-127 那条误导文案的诱因之一）。文件层面的并发写同样没有互斥保护。

**做法**：`tauri-plugin-single-instance`，注册在 builder **最前面**。

| 决定 | 取值 | 理由 |
|---|---|---|
| 插件位置 | `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]` | 移动端没有「第二个实例」这个概念，插件也没有移动端实现；放公共依赖会重演 §3.14.29 那 17 个错误 |
| 注册顺序 | **第一个** `.plugin(...)` | 插件必须早于其它插件初始化，否则第二个实例可能已经建好自己的窗口/托盘，先闪一下再退 |
| 第二个实例做什么 | `quick_search::hide` + `tray::show_main` + 留日志，然后由插件退出 | 不跑任何业务：不打开库、不读 `biometric.json`、不碰 TPM。它存在的唯一意义是"把已有窗口叫到前面" |

**边界（如实登记）**：

* 插件按「同名互斥体 + 窗口消息」实现，同一台机器上**不同用户会话**各自独立——那是系统的正确行为，
  不是缺陷；
* 便携版与安装版**共用同一个 identifier**，因此「安装版在跑 + 便携版启动」也会被判定为重复实例。
  这与它们共用同一个数据目录（`%APPDATA%` vs exe 旁）无关，是有意的：同一时刻只该有一个进程持有库；
* 安装程序在覆盖前若强杀旧进程，属安装器行为，本守卫管不到。

## 10. 待明确事项

| # | 事项 | 风险 | 建议处理 |
|---|------|------|---------|
| U1 | **VK 下发时机**：配对时 Host 是否直接下发 VK 给 Joiner（首次配对场景） | 若两端此前各自建过库，`vault_id` 不同、VK 不同，合并语义未定义 | **建议**：首次配对定义「主库（Host）为准」，Joiner 若已有本库则**先备份为空库**再接收；或提供「合并」选项（二期）。本期按「Host 为准 + Joiner 空库接入」处理，UI 明确提示 |
| U2 | **同步时的解密对端**：`ciphertext` 由 IK 加密，需双端 VK 一致才能解 | 与 U1 强相关 | 同上；文档已假设双端共享 VK |
| U3 | **Android 摄像头权限与 WebView `getUserMedia` 支持度** | 个别 WebView 版本可能不支持，导致扫码不可用 | **建议**：扫码为主 + **手动粘贴二维码文本/令牌**为兜底（已在 P-08 设计兜底入口）；T15 实现时做能力探测降级 |
| U4 | **超 20 MB 附件的多帧/分片** | 单帧 20 MB 需一次性读写内存 | **建议**：附件 blob 分片加密（每片 ≤ 1 MB）作为独立帧，条目记录保存片索引；本期先实现单片，分片列为 T04 的可选增强 |
| U5 | **「压缩保险库」触发策略** | 墓碑累积导致文件膨胀 | **建议**：墓碑占比 > 20% 或解锁时检测超阈值触发压缩（后台、可取消）；本期可在 T16 附带简单阈值压缩 |
| U6 | **主密码强度「提示」与「拦截」边界** | 仅 < 8 位才拦截，其余仅提示 | **已裁定 Q1**：< 8 位拦截；≥8 位放行 + 强度条提示，不强制混合类型 |
| U7 | **DB/索引持久化** | 搜索需明文索引，不得落盘 | 已定：索引仅内存构建，锁定时清空（A-04） |
| U8 | **CSV 导入的分组/自定义字段列** | Bitwarden CSV 无 `fields` 嵌套 | **建议**：CSV 仅映射同名基础列 + `folder`；其余列归入 `custom_fields`；跳过计数逻辑与 JSON 一致 |

---

## 附录 A：安全验收点对照表

| 验收点 | 设计落点 | 验证方式 |
|--------|---------|---------|
| **A-01** 库文件无明文凭据 | §2.6 记录帧全字段密文 + §2.3 AEAD | `crypto_vectors.rs` + 文件十六进制/字符串扫描 |
| **A-02** 改 1 条不重写全库 | §2.6 帧级原地更新 + 未变帧密文复用 | `container_roundtrip.rs` 字节差异断言 |
| **A-03** 复制 30s 清空 | §9.3 常量 + `clipboard.ts`「仅清自有内容」 | 前端单测 + 手动 |
| **A-04** 锁定后无残留明文 | §2.7 `Zeroizing` + `Session::Drop` | `zeroize_check.rs` + 会话复查 |
| **A-05** 解锁失败提示统一 | §2.5 统一 `ERR_UNLOCK_FAILED`。**边界（D-115）**：该统一只约束**锁屏入口**（未认证调用者）；已解锁会话内的密码复核用 `ERR_WRONG_MASTER_PASSWORD`——调用者已认证且手里就有库文件，折叠成「解锁失败」只是把用户带偏 | `crypto_vectors.rs` 断言锁屏错误码一致；`biometric_session_api.rs` / `restore_backup.rs` 断言会话内错误码 |
| **A-06** 一次性/过期令牌不可复用 | §3.3 `used`/`exp` + 校验 | `sync_replay.rs` |
| **A-07** 未配对设备被拒 | §3.5 会话需 `sync_key` | `sync_replay.rs` |
| **A-08** 重放载荷被拒 | §3.7 seq + 时间窗 + AAD | `sync_replay.rs` |
| **A-09** 日志无凭据 | §9.2 日志仅记 code | 关键字扫描 |
| **A-10** 导出/清空/解除配对二次确认 | `ConfirmDialog.svelte` + 各 UI 流程 | 交互验证 |

---

> **交付说明**：本文严格遵循冻结书四项决策，加密参数、协议报文、容器格式、模块划分均在本文定稿。设计与实现分离——本文只定义结构/签名/schema，不含实现函数体。工程师寇豆码可依据 §7 任务表按序实现。

#### 3.14.13 外部审查修复（D-106，2026-10-27，v1.2.23 待发布）

豆包六路静态审查（`docs/PasswordMS-审查报告-2026-10-27.md`）逐条核实后落码的 Rust 侧修复（前端部分见 06 文档 §6.20）：

| # | 审查编号 | 修复 | 文件 |
|---|---|---|---|
| 1 | 3.4-1 P1 | `lock` 清理 `pending_imports` 锁中毒不再跳过——`into_inner()` 兜底照常 `wipe_plaintext()+clear()` 并留痕（A-04 旁路封堵） | `vault_cmd.rs` |
| 2 | 3.1-1 P2 | `append_frame`/`pwrite_frame` 补 `sync_all()`（`flush()` 在 std File 上是 no-op，掉电丢尾帧）；写频为用户交互级，可靠性优先 | `container.rs` |
| 3 | 3.1-2 P2 | `place_frame` 长度变更路径改**先 append 新帧、后墓碑旧帧**——原顺序磁盘满时旧毁新未落＝条目丢失；改后追加失败可重试，墓碑失败时 rebuild 后写胜（新帧生效） | `repository.rs` |
| 4 | 3.1-7 P2 | `tombstone` 改先写盘后删内存索引——原顺序 IO 失败时条目重启「复活」 | `repository.rs` |
| 5 | 3.1-3 P2 | `restore_item` 还原时校验 `folder_id` 存在性，悬空则置 `None`（移入未分组），与 create/update 守卫口径一致 | `repository.rs` |
| 6 | 3.1-4 P2 | `read_blob`/`export_attachment_bytes` 返回 `Zeroizing<Vec<u8>>`，附件明文不再以普通 Vec 残留堆 | `repository.rs` |
| 7 | 3.1-5 P2 | `write_blob_frame`（同步接收）补明文 ≤20MB + 库容量 ≤200MB 守卫（与本地 `insert_attachment` 同口径，防异常配对端 DoS） | `repository.rs` |
| 8 | 3.1-6 P2 | `put_item_direct` / `resolve_conflict`（TakeRemote/KeepBoth）补 384KiB 序列化上限（同步路径贯穿 D-96 统一明文上限） | `repository.rs` |
| 9 | 3.2-1 P2 | `Session::unlock` 三处 `?` 逃逸口折叠：KDF 参数校验失败（篡改头）补 burn-KDF 后统一 `UnlockFailed`；device/Repository::open 失败统一折叠（Argon2 已跑，无需再 burn） | `session.rs` |
| 10 | 3.3-1 P2 | Joiner 段②删除 fresh-read，直接复用段①内存 state（W' 不再被丢弃）——配对 → 首轮重同步免整库重发（O1 恢复），逐行对齐 `resync.rs` 正确实现 | `client.rs` |
| 11 | 3.4-2 P2 | resync 锁不再跨网络 I/O：stop 信号提升为 `AppState.resync_stop`，worker 锁内 `take` handle、锁外整场 `handle_connection`、结束放回（`is_none` 防覆盖） | `state.rs` |
| 12 | 3.4-3 P2 | `stop_resync_listener` 先无条件置 stop（不依赖锁），锁中毒 `into_inner()` 兜底清 handle 并留痕 | `state.rs` |
| 13 | 3.4-5 P2 | `copy_secret` 的 `ttl_secs` 生效——兜底清理按用户配置 TTL 睡眠（原恒 30s，用户配 60s 时密码被提前清） | `system_cmd.rs` |
| 14 | 5.2-4 P2 | 泰文 `auto_lock_failed`「ไลบรารี」（library/图书馆）→「ตู้เซฟ」（保险库） | `th.ts` |

**核实后不修项**：IPC 路径白名单（3.4-4 / 历史 S2：本地单用户威胁模型，前端被投毒属更大面失守，接受风险、报告已确认设计取舍）；Rust 侧 auto-lock 兜底线程（3.4-6：跨 session/事件层改动，另行排期）；错误码细化（3.1-10：新增 `AttachmentNotFound` 变体牵动 31 变体错误码表与四语言 messageKey 映射，P3 分批）；C3/S1/S6/Q3 历史遗留项维持 D-91 核对结论（C3 单测靠同时间戳掩盖属实，留待 item_hash 重构时一并处理；Q3 为 windres 缺 DLL 环境项）。

守门：core `tests/d106_external_audit.rs` 3 例（还原悬空分组 / blob 帧超限拒收 / 同步条目超限拒收）。全量验证：core 215 例全绿（212+3）、前端 44 文件 341 例、clippy 0、fmt 过。

#### 3.14.14 外部审查余项批 + 更新检查（D-107/D-108，2026-10-27，v1.2.23 待发布）

D-106 汇报中「另行排期」的 auto-lock 兜底与豆包报告 P3 余项一并落地；同批新增 **D-107 更新提醒**（方案 B，主理人裁定）。

**D-107 更新检查（方案 B：自动检查默认关 + 手动按钮，只提醒不下载）**

| 项 | 实现 |
|---|---|
| 数据源 | GitHub Releases API `repos/hengletime/PasswordMS/releases/latest`，WebView `fetch` 直连（**零新增 Rust 依赖**——不加 HTTP crate，守住 §8.1 依赖上限与 10MB 体积门禁）；CSP `connect-src` 显式放行 `https://api.github.com` |
| 隐私 | 单向 GET，不携带任何本机信息（连本机版本号都不发送）；自动检查**默认关闭**，关闭时零网络请求；四语言用户协议已补披露句 |
| 节流 | 自动检查 24h 一次（localStorage 时间戳）；手动按钮不限次；失败静默（自动）/ 显式提示（手动） |
| 语义 | 只提醒 + 跳转 Releases 页手动下载——不引入自动更新的签名/供应链复杂度 |

**D-108 豆包余项（P2×1 + P3 批）**

| # | 审查编号 | 修复 |
|---|---|---|
| 1 | 3.4-6 P2 | **auto-lock Rust 兜底**：`AppState.last_activity_ms` 活动时间戳；全部触碰会话的 IPC 经 `AppState::lock_session()` 统一刷新（52 处调用点），不走会话锁的用户命令（复制/生成器/TOTP/导入预览）显式 `touch_activity`；`spawn_auto_lock_worker` 每 15s 判定「解锁中 + 闲置达档位」即执行与 `lock` 命令**同一套**级联（抽出唯一实现 `perform_lock`）+ emit `autolock://locked`，前端 App.svelte 常驻监听复位 store 切锁屏。worker 轮询不刷新时间戳（否则永不闲置） |
| 2 | 3.2-8 | `Repository::open_with_container`：unlock 复用头部阶段打开的容器，消除二次 open 的 TOCTOU 窗口（错误码逃逸统一化） |
| 3 | 3.2-7 | `.device` 文件创建改 `create_new(true)`，并发解锁的写竞争消除（回读既有值） |
| 4 | 3.2-3 | `url_safety` 补全判定只认 `://`——`mysite.com:8443`（host:port）不再误拒；scheme 白名单不变（`javascript:`/`file:`/`data:` 实测仍拒） |
| 5 | 3.2-4 | TOTP 解码后强制 ≥10 字节（RFC 6238 建议 ≥160bit），`"A"` 类弱密钥拒绝 |
| 6 | 3.2-5 | 健康检查「重复密码」判定改 trim + 去零宽字符的归一化键（`"pw"` vs `"pw "` 漏报）；弱口令判定仍用原串 |
| 7 | 3.1-8 | 打开库 `folder_count` 与 `item_count` 同口径交叉核对 → open_warning |
| 8 | 3.1-9 | `read_frames_with_tail`：尾部 <一帧头残料不再静默，计数字节进 open_warnings（与备份恢复路径同口径） |
| 9 | 3.1-12 | `delete_folder` 语义定为快停：条目搬移失败即停、分组保留可整体重试；分组 tombstone 只在全量成功后发生 |
| 10 | 3.1-13 | `empty_trash` 尽力而为：单条 purge 失败不中断，返回首个错误 |
| 11 | 3.1-10 | 新增 `CoreError::AttachmentNotFound`（`ERR_ATTACHMENT_NOT_FOUND` / `err.attachment_not_found` 四语言）：`remove_attachment`/`export_attachment_bytes` 附件定位失败不再借用 ItemNotFound |
| 12 | 3.4-7 | 新增 `CoreError::SyncBusy` 入错误码表（`ERR_SYNC_BUSY`），`sync_now` 不再手工拼错误 |
| 13 | 3.4-8 | `vault_file_info` DTO 字段 `path` → `file_name`，只回传文件名不暴露绝对路径（types.ts/Settings.svelte 同步） |
| 14 | 3.4-9 | 配对会话清理锁中毒 `into_inner()` 兜底 + 留痕（原静默跳过 → 僵尸监听） |
| 15 | 3.4-10 | `resync.port` 端口文件在本 server 确认被采纳后才落盘（原先写后查，竞态败者写坏端口文件） |
| 16 | 3.4-11 | capabilities 移除 `opener:default`（前端从不直接调 opener 插件，open_url 白名单链路是唯一出口）；`fs:default` 保留（ImportWizard `readTextFile` 依赖，收窄需真机回归，列为后续项） |
| 17 | 3.4-12 | CSP 补 `object-src 'none'; base-uri 'none'; form-action 'none'` |
| 18 | 3.4-13 | `random_port` RNG 失败直接报错（原退化到可预测的 49152） |
| 19 | 历史 S6 | 配对 worker 收到错令牌/过期/已用/指纹不符不再作废会话（REJECT 已由 server 发出，二维码保留，正确用户无须重扫） |
| 20 | 3.5-7 | 前后端占位死命令 `sync_accept_pair` 双侧删除（命令面 59 → 58） |
| 21 | 3.5-8 | `settings.saveQuiet` 独立 `quietBusy` 标志（原复用 busy 会误灰按钮） |
| 22 | 3.5-9 | 导入向导 CSV 表头读取失败 toast（原静默重置） |
| 23 | 3.5-10 | 解锁后五路加载改 silent + `allSettled`，失败立即整组重试一次，仍失败合并一条 `err.load_partial`（原 toast 堆叠） |
| 24 | 3.2-2 / 3.3-2 / 3.3-3 / 3.3-4 | 注释与实现一致性：burn-KDF 未来参数警示、identity 文件长度注释 81→82、import apply 按引用遍历（只克隆落库条目，万条导入峰值内存减半）、discovery 模块文档说明「协议多 tag / 请求侧单 tag」 |
| 25 | 5.2-7 收尾 | 泰文用户协议 "(crash log)" 英文注音残留清除（D-104 已修 telemetry 误译的收尾） |

守门：core `tests/d108_session_unlock.rs` 5 例（错误密码统一化 / NoVault / 截断与垃圾头部折叠 / 锁态复位 / 改密全流程，均真实走 Argon2）；url_safety/totp 内联单测 4 例；前端 `d107-update-check.test.ts`（版本比较/应答解析/节流 9 例）+ `d108-audit-guard.test.ts` 12 例（lock_session 全覆盖 / perform_lock 复用 / autolock 事件链路 / capabilities/CSP / 死代码删除 / DTO 收敛 / 合并提示 / 四语言键与披露句）。历史项 S1/C3/Q3 维持原结论（格式 v2 / item_hash 重构 / 环境 DLL）；3.4-4（S2 IPC 白名单）与 3.1-11（O4 上帝对象）维持设计取舍。

#### 3.14.15 冲突仲裁死循环修复（D-109，2026-10-27，v1.2.23 后续）

**实测现象**：LAN 同步双端同时修改同一条目 → 一端出现 3 个待仲裁冲突；仲裁「采用远端」后再同步，对端又出现同样 3 个；如此往复永不收敛。定位出**四层叠加缺陷**：

| # | 缺陷 | 根因 | 修复 |
|---|---|---|---|
| 1 | 仲裁产物必然互异 | `item_hash` 是**整 Item 结构 JSON 哈希**——`rev`/`updated_at`/`modified_by`（及本地统计 `use_count`/`last_used_at`）全部参与。仲裁「采用远端」重写这三项元数据 → 两端产物业务内容一致也 hash 异 → `detect_conflict` 每轮判「双改且内容不同」 | `item_hash`/`folder_hash` 改为**只哈希业务内容**（clone 后剥离 5 个元数据/统计字段再序列化）。`BaseRev.hash` 唯一消费点是墓碑判重（`== [0;32]`），口径变化对旧库完全兼容 |
| 2 | 双端仲裁振荡 | 旧 TakeRemote 产物 `rev = max+1` + `updated_at = now`：双端各自仲裁后产物**永远互为「对方上一版」**（内容互换、差半拍），数学上不可收敛 | TakeRemote 改为**原样落地**对方快照（`put_item_direct`）——「采用远端」的语义就是接受对方版本；双端互采 = 各自获得想要的版本，内容此后不变，振荡自然消除。语义注记：互采后两端内容不同（各持对方版本）是该仲裁选项的准确含义，非数据丢失（原版本仍在冲突箱 resolved 快照中） |
| 3 | 同 rev 双改单向丢改 | O1 回声消除（`note_received_changes`）无条件推发送水位：双端同时各自推进到**同 rev** 时，接收方把自己的独立修改误判为「对端已持有」，**本端修改永远发不出去**（冲突只在一侧出现、另一侧静默丢改）——正是「只有电脑端出现 3 个」的成因 | 回声消除前以 `apply_changes` 刚推进的 base 对照判同源：本端条目 rev 与 hash **均与 base 一致**才推水位；否则（独立修改）不推，本端版本照常导出 |
| 4 | 无基准静默覆盖 / 陈旧变更 | base 仅在接收侧推进：发送方对自己已发出的版本无基准记录 → 下轮双改判定缺基准 → 无基准 LWW 把后改一端**静默覆盖**先改一端（Q3 禁止） | 新增 `merge::note_sent_base`（4 个交换点：client/resync 发起方段②、server/resync 响应方导出后）：发送成功即把 base 推进到已发出 rev（hash 取发出版本明文摘要，仅 `rev > base.rev` 时推进不倒退）。另 `detect_conflict` 无基准分支：同 rev 异内容直接判冲突（无基准下同 rev 异内容必为双端独立修改） |

**水位回落配套**：TakeRemote 原样落地后条目 rev 可能低于本端曾发出的版本——高挂的发送水位会永久吞掉本端后续修改的导出。`resolve_conflict` 返回 `Vec<WatermarkReset>`（`model/conflict.rs`），命令层（`sync_cmd::resolve_conflict`）对每个对端 state 做 `min(水位, new_rev)` 应用；失败安全沿用「`save_sync_state` 不执行即随会话丢弃」语义。

**语义裁定（互采）**：双端都点「采用远端」时两端各持对方版本——这不是缺陷：该选项的意图就是「以对端为准」，双方意图各自达成；「双方都想保留自己的版本」请用 KeepLocal（两端各持己见、不再互扰）；需要同时保留两版用 KeepBoth（新副本）。

守门：`tests/d109_conflict_hash.rs` 6 例——①item_hash 元数据无关性（业务内容敏感）②folder_hash 同口径③双端 TakeRemote 仲裁后 6 轮零新冲突 + 互采断言（**主守门，旧实现永不收敛**）④TakeRemote 水位回落后本端新修改可达对端⑤KeepLocal 双端第 2 轮零冲突⑥同 rev 双改导出不被回声消除吞掉。core 全量 231 例绿（225+6），clippy 0 告警，fmt 过。历史项 C3（item_hash 重构时机）随本缺陷提前落地了 hash 口径修复，S1（格式 v2）不变。

#### 3.14.16 KeepLocal 传播语义修复（D-111，2026-10-27 晚）

**症状**（主人实测）：冲突仲裁选「保留本机」确认后，对端不变；再同步两端两样、不报冲突，永久不一致。

**根因（双重卡死）**：KeepLocal 旧实现条目原样不动——①本机版本 rev 早就在发送水位线上（该版本此前已发出，`rev ≤ 水位` 免重发），对端永远收不到胜出版本；②对端 base 在冲突发生时已推进到本机 rev——即使重发，对端 `detect_conflict` 判「远端 rev ≤ base.rev 未改」静默 skip。

**修复**（镜像 D-109 TakeRemote 的收敛思路）：

| 项 | 修复 |
|---|---|
| 胜出版本 | KeepLocal 改为抬 rev 落地：`rev = max(local, remote) + 1`，业务内容不变（仅元数据推进），`put_item_direct`（自带上限校验）——rev 超过所有对端水位与 base，下一轮同步自然重发 |
| base 推进 | `WatermarkReset` 扩展 `new_hash` 字段（TakeRemote 填落地 hash / KeepLocal 填胜出 hash），命令层对每个对端 `base.insert({new_rev, new_hash})`——否则双 KeepLocal 场景两端互为「同 rev 异内容」且各自偏离陈旧 base，每轮重新报冲突（新形态振荡）。TakeRemote 的 base 冲突时已推进为同值，insert 幂等 |
| 对端接受 | 对端无本地修改 → 单侧变更走 LWW 接受 → 全网收敛到胜出版本；删除型冲突（对端删除、本机保留）同理，抬 rev 后对端墓碑判定失效走 D-56 复活路径 |
| 过时冲突过期 | 新增 `Repository::expire_conflicts_for(item_id)`：本端应用某条目的更新版本（LWW / 复活）时，挂着的旧冲突快照已不代表现状（继续仲裁只会把过时版本写回去）→ 自动标记 resolved（事实解决收口）。merge.rs 两处 put（LWW 应用、None 复活）接线 |
| 双 KeepLocal | 两端意图矛盾时各自抬 rev，交换后 LWW 按 `updated_at` 确定性收敛（后仲裁者胜）——比永久不一致可预期 |

**语义升级**：「保留本机」从「两端各持己见、永不一致」升级为「以本机为准，同步到对方」（与「采用远端」的互采语义对称互补）。

**测试盲区教训**：测试环境 `Side.state` 是模拟同步会话的内存副本，repo 内直改 state（首次实现走的 `list_paired_devices` 遍历）对副本不可见且测试双端未注册配对（遍历空转）；正确模式是**与 TakeRemote 同构的返回值上交**——repo 层不持有权威副本，仲裁对状态的影响一律返回值上交、调用方应用。

守门：`tests/d109_conflict_hash.rs` 扩至 8 例——⑦KeepLocal 传播胜出版本 + 对端挂起冲突自动过期 + 两端 rev/hash 收敛（主人实测场景）⑧删除冲突 KeepLocal 后对端复活为保留版本；KeepLocal 双端用例改断言 LWW 收敛（后仲裁者胜）。`sync_deletion.rs` case2 断言随新语义更新（rev 3）。core 全量 233 例绿（231+2），clippy 0 告警，fmt 过。前端零改动（IPC 命令面不变，`resolve_conflict` 签名不变）。

#### 3.14.17 core 层文案本地化（D-112，2026-10-27 晚）

- **症状**（泰国朋友真机实测）：泰文 UI 下健康检查「建议」显示中文（「密码健康状况良好，继续保持。」）。主人指示「其它地方也检查一下」→ 全仓排查出**同一病根三处**：Rust core 硬编码中文用户文案。
  ① `health.rs`：4 条整改建议句子硬编码中文（四语言用户全看到中文；前端 i18n 早备有 `health.good` 键但从未被用上——设计意图本是前端出文案）；
  ② `repository.rs`：仲裁「两者都留」生成的冲突副本标题硬编码追加 ` (冲突副本)`；
  ③ `import/template.rs`：CSV 导入模板示例行硬编码中文。
- **架构裁定（方案 A，主人拍板「三处全修」）**：**core 只出数据，前端出文案**——与 D-102「zh-CN 唯一事实来源、四语言字典同加」一致，不在 Rust 里再造一套翻译。src-tauri 命令层与前端四语言字典本身无泄漏（全仓 grep 复核）。
- **修法**：
  - ① `HealthReport` 移除 `suggestions: Vec<String>` 字段；前端 `HealthCheck.svelte` 以 `$derived.by` 按 weak / reused_groups / stale 计数组装 i18n 文案（`health.sug_weak` / `sug_reused` / `sug_stale`，三项皆空用既有 `health.good`），四语言字典同加。
  - ② `resolve_conflict` 全链加第三参数 `copy_suffix`（命令层 snake_case wire 键 `copy_suffix`）：前端 `stores/sync.svelte.ts` 传 `ui.t('conflict.copy_suffix')`（如「 (冲突副本)」/「 (สำเนาข้อขัดแย้ง)」），core KeepBoth 用其拼副本标题。**存量副本标题不动**（是数据，不是 UI）。
  - ③ `CSV_TEMPLATE` 常量改 `csv_template(&TemplateTexts)` 函数：表头与转义示范结构留 core（契约不变），示例文本（`import.tpl_folder` / `tpl_login_name` / `tpl_login_notes` / `tpl_note_name` / `tpl_note_body`）由前端 i18n 传入；`TemplateTexts::new` 校验文本不得含 `"` / `\n` / `\r`（防结构性损坏；serde 反序列化绕过构造器，命令层重建强制校验）；`tpl_login_notes` 各语言**必须自带半角逗号**（引号转义示范的组成部份，zh-CN 字典为保证源头）。
- **守门**：`tests/import_template.rs` 参数化重构——中文（逐字回归基线）+ 泰文两组文本各跑全量契约（0 跳过 / 表头 == KNOWN_COLUMNS / 不溢出 custom_fields / 无 BOM + 尾换行）；`template.rs` 单测锁引号/换行注入拒绝 + 泰文 UTF-8 渲染。`resolve_conflict` 9 处测试调用点补后缀参数（`" (copy)"`）。core 34 个测试组全绿（236 例），clippy 0 告警（`ptr_arg` 一处顺手修），fmt 过；前端三门绿（vitest 364 / svelte-check 0 错 / CSS 契约 PASS）。
- **注意**：D-112 需两端同版本升级（`resolve_conflict` 前后端签名联动）；旧版本前端调用新后端会因缺 `copy_suffix` 参数报 invalid args。

#### 3.14.18 生物识别快速解锁 P1（core 旁路密封）+ D-113 审计整改（2026-10-28）

- **功能定位**：1.3.0 首个功能。**旁路密封**——不改密码学模型，把**同一个 VK** 用平台密钥设施另存一份到独立文件 `biometric.json`（不进容器、不进同步）。主密码链路、容器格式、同步协议、条目格式**零改动**；删掉该文件即回到只认主密码。
- **绑定熵**（`S5` 安全性质的全部依据）：
  `bind = SHA-256("passwordms/v1/biometric" ‖ 0x00 ‖ vault_id ‖ 0x00 ‖ verify_mac ‖ 0x00 ‖ len(nonce) ‖ nonce ‖ len(ct) ‖ ct)`，其中 `nonce/ct` 取自 `header.wrapped_vault_key`。平台侧把它当 DPAPI `pOptionalEntropy` / AES-GCM `AAD`。
  **必须含 `wrapped_vault_key`**：`verify_mac` 只随主密码变，而 `adopt_vault_key`（配对加入）**换 VK 不换 `verify_mac`**；只用前者会让封印在配对后仍判「可用」却封着旧 VK（D-113 的根因）。
- **状态机**：`SealState` 四态 `Absent / Corrupt / Ready / Stale(InvalidatedReason)`；`InvalidatedReason` 四值 `MasterPasswordChanged / VaultKeyChanged / BiometricEnrollmentChanged / Tampered`。分类靠两个域分离的 8 字节校验 `mac_check`（只覆盖 `verify_mac`）与 `wkv_check`（只覆盖 `wrapped_vault_key`）——先判 mac 再判 wkv，保证「改密」优先于「换 VK」。
- **会话形状**：`Unlocked.kek` → `Option<Kek>`（生物路径无 KEK，不跑 Argon2）。新增 `Session::unlock_with_vault_key()`、`has_kek()`，两条路径共用 `finish_unlock()`；密码路径仍 `.map_err(|_| UnlockFailed)` 保住 A-05，生物路径错误**不折叠**。
- **空库一律拒绝**（`NeedMasterPassword`）：空库没有任何活帧可解密，`rebuild_index` 对错 VK 是空操作 ⇒ VK 对不对无从检验。实测修复前一个全 `0xAB` 的假 VK 能「成功」打开刚建的空库。
- **D-113 BLOCKER（已实测复现并修复）**：`adopt_vault_key` 换 VK → 旧 bind 不变 → `inspect` 仍 `Ready` → 解封交出**旧 VK** → 空库接受 → 用户新建条目用错密钥加密 → 主密码解锁时 `rebuild_index` 撞上这些帧 → `?` 失败 ⇒ **整库永久不可解锁**。复现输出：`after_adopt_inspect=Ready / unlock_succeeded old_vk=true / write_with_stale_vk=OK / password_unlock_after=FAILED ERR_UNLOCK_FAILED`。修复 = 上述 bind 纳入 `wrapped_vault_key` + 空库拒绝两条，缺一不可。
- **如实告知的降级（审计 H-1/H-2）**：
  ① Windows 侧开启后，保险库保护下限从「主密码」降到「Windows 账户口令」——拿到磁盘副本**且**知道账户口令者可离机解出 VK、完全绕过 Argon2（DPAPI 用户主密钥在 `%APPDATA%\Microsoft\Protect\<SID>\`，只是被口令派生的密钥加密）。Android Keystore 在 TEE，无此问题。
  ② 「改主密码即失效」只对**应用层**成立：`change_master_password` 只重包裹不轮换 VK，旧 `biometric.json` + 旧头部仍能解出**当前** VK。**撤销的唯一动作是删除 `biometric.json`**。
- **平台强度不对称**：Android Keystore `AUTH_BIOMETRIC_STRONG` + `InvalidatedByBiometricEnrollment` 是硬绑定（新增指纹即永久失效，PIN 顶替不了）；Windows `UserConsentVerifier` 不暴露认证方式（PIN 也算），DPAPI 只绑用户登录态 ⇒ 文案只能叫「Windows Hello 快速解锁」。
- **守门**：`tests/biometric_wrap.rs` **27 例**（含真跑 `change_master_password` / 真跑 `adopt_vault_key` / 空库拒绝 / 损坏 / 超限 / 跨平台 / 适配器短 VK）+ `biometric.rs` 2 单测（bind 三项输入各自生效、两个校验域分离）。core **35 组 / 265 例全绿**，clippy 0（仅 D-33 windres 提示），fmt 过；前端零改动（P1 不接 UI），vitest 364 / svelte-check 0 错 / CSS 契约 PASS。
- **P2/P3 硬约束**：①适配器必须真把 `bind` 用作 entropy/AAD（Mock 会掩盖这一点，须补适配器级 `bind1 封 → bind2 解必须失败`）；②`adopt_vault_key` / `change_master_password` 成功后命令层必须 `store.remove()`；③`DeviceBusy`/RDP/未配置**绝不**映射成 `Invalidated`（会引导删封印）；④`CRYPTPROTECT_UI_FORBIDDEN`、禁用 `CRYPTPROTECT_LOCAL_MACHINE`；⑤`CryptUnprotectData` 的 `LocalAlloc` 缓冲拷完必须 `SecureZeroMemory` + `LocalFree`；⑥IPC 返回 DTO 不得含 VK 或其派生值。
- **文档**：方案全文 `docs/08-biometric-unlock.md`（§18 为审计逐条处置）；延后项 `docs/03-impl.md §13.10`。

### 3.14.19 生物识别 P2：Windows 适配器 + 命令层 + 前端（D-113，2026-10-28）

- **适配器两段式**（`src-tauri/src/platform/`）：DPAPI 用户域加密（`bind` 作 `pOptionalEntropy`）+ `UserConsentVerifier` 在场闸门。平台部分全部关在 `cfg(windows)` 里；非 Windows 用 `UnsupportedSealer`（恒 `Unsupported`），**Android 可正常编译**，P3 再接 Keystore。`Cargo.toml` 只在 `cfg(windows)` 下打开 `windows` 特性（版本对齐既有依赖树的 0.62.2，**不引新包**）+ `windows-future`。
- **WinRT 线程模型**：专用 STA 线程（`CoInitializeEx(APARTMENTTHREADED)`）+ `PeekMessage` 消息泵等完成回调；Tauri 命令线程不碰 WinRT。超时 / `DeviceBusy` / 未配置一律 `Unavailable`，**绝不** `Invalidated`。
- **核心抽象**：`platform::PresenceGate`（`availability` + `request(reason)`）把「弹框」与「加解密」拆开——真机弹框无法自动化，而「`bind` 是否真的参与加密」只有适配器自己知道。于是测试可注入「永远通过」的闸门，对**真实 DPAPI** 断言「换 `bind` 必解不开」（§18.5 约束 1）。
- **命令层 5 条**（`commands/biometric_cmd.rs`，全部 `rename_all="snake_case"`）：`biometric_status`（锁定态可调，只读）/ `biometric_enable`（解锁态 + **主密码** + 非空库）/ `biometric_disable`（幂等删除，撤销唯一动作）/ `biometric_unlock`（`Container::open` 只一次）/ `open_biometric_settings`（**无参数**，唤起系统「登录选项」，D-114）。命令面 58 → **63**。
- **会话层新增两个 API**（`session.rs`）：
  ① `verify_master_password(&self, pw)` —— 开启快速解锁的「证明你知道主密码」，**不改变会话状态**，生物会话（`kek=None`）同样可用（只用到头部 + KDF）；
  ② `unlock_with_opened_container(path, container, vk)` —— 与 `unlock_with_vault_key` 行为等价，差别只在复用调用方已打开的容器，消掉「算 `bind` 用旧头部、建会话用新头部」的 TOCTOU 窗口。
- **撤销点前移到命令层**：`change_master_password` 与配对 `adopt_vault_key` 成功后都必须 `SealStore::remove()`——`Session` 刻意不持有 `SealStore`，core 侧无法完成（§18.5 约束 2）。
- **并发**：`AppState::biometric_busy`（CAS）+ RAII `BiometricGuard`；抢不到的一侧返回 `ERR_VALIDATION`，前端静默（用户正在等前一次的 Hello 弹框）。
- **守门**：适配器 7 例（真 DPAPI 往返 / **换熵必失败** / 输出长度落在 core 白名单 48..=1024 / 取消与不可用不弹框 / subtype / 结果映射）+ 会话级 5 例（`verify_master_password` 正确-错误-锁定态-生物会话；`unlock_with_opened_container` 与路径入口等价、空库仍拒、错 VK 仍拒）。core **270 例全绿**（265 + 5），适配器 **7 例全绿**，clippy 0，fmt 过；前端 vitest **364** / svelte-check 0 错 / CSS 契约 PASS。
- **文档**：`docs/08-biometric-unlock.md §19`（P2 落地记录与十条约束落点表）、`docs/06-ui-design.md §6.24`（解锁页按钮 + 设置页卡片 + 行类型纪律）。
### 3.14.20 D-114：快速解锁的三处 UX 缺陷修复

主人真机走查 P2 报出三条，全部是**流程/文案**层，不改密码学模型：

| # | 现象 | 根因 | 修法 |
|---|---|---|---|
| 1 | 改主密码后只在右下角闪过一句「确定」 | `ui.toast('success', 'common.ok')` —— `common.ok` 是按钮标签「确定」，等于什么都没说；成功 toast 只有 3 s | 改密文案点名「主密码已修改」，并在卡片内**常驻** `.pms-alert.is-success + role="status"`；三个输入框任一改动即清除旧回执 |
| 2 | 改密后快速解锁悄悄没了，无任何解释 | 命令层按 §18.5 约束 2 删封印，前端既不重查也不说明 | 改密**前后对比** `bioEnabled`（改密后必为 false，不能事后取）→ 成功时 `refreshBiometric()` 并区分 `plain` / `bio_off` 两档文案 |
| 3 | 没开 Windows Hello 时红字说「本机没有可用的硬件」 | `UserConsentVerifier` 在「Hello 从未配置」的机器上回报 **`DeviceNotPresent` 而不是 `NotConfiguredForUser`** | 文案只陈述「未检测到」并点明「多半是尚未设置，不一定缺硬件」；补一句操作指引 + `open_biometric_settings` 无参数命令跳「设置 → 账户 → 登录选项」；窗口重新获焦自动重查（开通后切回来即刻生效） |

**为什么不复用 `open_url` 打开 `ms-settings:`**：那条链路的前后端白名单都只放行 http/https（PRD F-05 不变量），为它开口子等于把「打开链接」变成「打开任意系统协议」。新增的 `open_biometric_settings` **不带参数**，目标 URI 是 Rust 里的编译期常量，`ShellExecuteW` 唤起（`Win32_UI_Shell` 特性，仍不引新包）。

守门：`src/lib/d114-feedback-guard.test.ts` 7 例（禁回退到 `common.ok` / 改密后必须重查 / `bioWasOn` 必须在改密前取 / 常驻结果条结构 / 三个 input 都要清回执 / `not_present` 文案禁再断言缺硬件 / 四语言 5 键齐全）。前端 vitest 371（364+7）/ svelte-check 0 错 / CSS 契约 PASS；core 270 例不变、clippy 0、fmt 过。

附带修正：`--pms-success-soft` 在 4 个主题块（`:root` / 媒体查询暗色 / `data-theme='dark'` / `data-theme='light'`）**必须齐全**——CSS 契约脚本会逐键比对暗色两块的共有键，漏一个即 FAIL。

### 3.14.21 D-115：错误密码文案与「已开启但不可用」的状态矛盾

主人 v1.3.1 真机走查报两条，前一条是**错误码的分工错误**，后一条是**状态模型少一维**。

**① 已解锁会话里的密码复核不该说「解锁失败」**

| 项 | 旧 | 新 |
|---|---|---|
| 错误码 | `ERR_UNLOCK_FAILED` → `err.unlock_failed`「解锁失败，请重试。」 | `ERR_WRONG_MASTER_PASSWORD` → `err.wrong_master_password`「主密码错误，请重新输入。」 |
| 覆盖点 | `Session::verify_master_password`、`Session::change_master_password`、`Repository::export_vault` | 同上三处全部改 |
| 显示方式 | `ui.toast('error', …)`（右下角 3 秒） | 就地常驻：开启卡片显示在输入框下方（`role="alert"`）、改密卡片显示在底部结果条 |

**A-05 的边界至此明确**（`§9.2` 错误码表与 A-05 行同步）：`ERR_UNLOCK_FAILED` 是**锁屏入口**（未认证调用者）的答案，要求「密码错 / 头部坏」不可区分；而这三个调用点都只在**已解锁会话**里可达——调用者已通过认证、手里就有容器文件（可自行离线逐条试），「密码对不对」对它不是新信息。继续折叠只会让用户在设置页里读到一句自己解释不通的话。

**② `SealState::Ready` ≠ 平台现在可用**

`biometric_status` 返回两个**独立**事实：`state`（`biometric.json` 结构完好 + bind 自洽）与 `availability`（平台现在能否弹 Hello）。Hello 被关掉时二者组合为 `ready + not_present`，旧 UI 状态行照 `state` 渲染「已开启（日期）」，与下方红字「未检测到可用的 Windows Hello」**同屏自相矛盾**（主人截图）。

修法：前端派生第五种**显示态** `bioDegraded = ready && !available`，状态行改「已开启，但当前不可用（{date}）」（warning 色，`.bio-state.is-warn`），并补一句 `bio_degraded_hint` 说明「关闭期间一次也用不了 / 重新开通自动恢复 / 撤销请点关闭」。**后端 DTO 不变**（不加字段、不加枚举值），因为这是纯展示层的组合推导，不该在 wire 上多一份真相。

**安全结论（回答主人提问「这里是否有安全风险」）**：无新增可利用面。

- Hello 关闭期间 `biometric::unlock` **先查 `availability`** 即返回 `BiometricUnavailable`（`biometric.rs:527`），一次都进不去 —— 不存在「退回只靠 DPAPI」的窗口。
- 闸门每次都重新验证，**不缓存**；重新开通 Hello 后能用，是因为 DPAPI 主密钥与 `bind` 都没变，封印本就完好，而不是绕过了什么。
- 能开关 Hello 的人必须掌握该 Windows 账户凭据，而按 §7 T1b，DPAPI 用户域主密钥的磁盘副本正是被账户凭据派生的密钥加密 —— 该攻击者本来就能直接解开 `biometric.json`。**不是能力提升**。
- 真实缺口是**检测**：Windows 侧没有 Android `KeyPermanentlyInvalidatedException` 那样的「生物特征集变更」信号，PIN 重置 / 重新录入不会让我们判 `Stale`。登记为 P3 候选（`KeyCredentialManager` NGC 绑定），见 `docs/08` §21.4。

守门：`src/lib/d115-bio-state-guard.test.ts` 7 例；core `biometric_session_api.rs`（2 处断言改 `WrongMasterPassword`）、`d108_session_unlock.rs`（改密旧密码）、`restore_backup.rs` 新增 1 例（导出错密码 + 不落半个备份文件）。前端 vitest 371 → **378**，core 270 → **271**。

### 3.14.22 D-116：`Availability` 拆出 `NotImplemented`（未实现 ≠ 不支持）

安卓 v1.3.1 的设置页出现「Windows Hello 快速解锁 · 未开启 · 当前平台不支持」。根因是 **`Availability::Unsupported` 一词双义**：

| 语义 | 谁产生 | 用户该看到什么 |
|---|---|---|
| 有适配器、但平台此刻做不了（WinRT 激活失败 / 系统被裁剪 / RDP 中断） | Windows 适配器 | **必须可见地**说清楚，否则等于藏故障 |
| 本平台**根本没有适配器**（Android 的 Keystore 属 P3） | `UnsupportedSealer` | **整块不渲染**——为不存在的功能做广告是错的，理由也不成立 |

修法：`Availability` 增加 `NotImplemented`；`UnsupportedSealer` 三个方法全部改报它；`availability_name()` 增加 `"not_implemented"`；前端在有该值时**整卡不渲染**（`bio === null` 同样不渲染）。

影响面：`enable` / `unlock` 的判定是 `availability != Available`，两个变体的**行为完全相同**，因此这是纯分类/展示改动，密码学零风险。`Unsupported` 的 wire 值 `"unsupported"` **保持不变**（老分支仍能命中），只是文案改口径为「Windows Hello 组件在当前系统上无法调用」。

守门：`src/lib/d116-platform-gate.test.ts` 4 例（含「禁止改成显示但置灰」的反向断言）。前端 vitest 378 → **382**。

### 3.14.23 生物识别 P3：Android Keystore 适配器（D-113，2026-10-28）

`platform/mod.rs` 的 `platform_sealer()` 增加第三分支：`cfg(target_os="android")` → `android_keystore::KeystoreSealer`。**`VaultSealer` trait 一字未改**——Android 的密钥绑定语义恰好落在 `seal(vk, bind)` / `unseal(sealed, bind)` 这两个方法的表达能力之内，不需要给公共 API 加参数。

**三条实现决定**

| 决定 | 做法 | 为什么 |
|---|---|---|
| 密钥别名 | `hex(bind)` 全长（64 hex） | `bind` 已含 `vault_id` / `verify_mac` / `wkv nonce`，换库换密码自动换别名；截断会让两个库以极小概率共用一把密钥 |
| `bind` 的角色 | GCM 的 **AAD**（`updateAAD`，加解密各一次） | 与 Windows 用 `bind` 作 DPAPI `pOptionalEntropy` 语义对齐：换 bind 必解不开 |
| 在场验证 | `BiometricPrompt.CryptoObject(cipher)` | **认证与解密是同一个对象**，中间没有可插针的缝隙；不需要 Windows 那道独立闸门，也不会弹两次 |

**线程模型（最容易写错的一处）**

Tauri 的 `run_command` 经 `run_on_android_context` 把 JNI 调用**投递到主线程**执行 ⇒ Kotlin 的 `@Command` 跑在主线程。而 Rust 侧 `run_mobile_plugin` 是 `mpsc::recv()` 阻塞等回复。两条叠加，若任何一端占住主线程就是**静默死锁**（对话框永不出现，也没有错误码）。因此：

1. Kotlin `@Command` 只「发起」——挂上 `BiometricPrompt` 后**立即返回，不 resolve**，把 `Invoke` 存进回调，认证结束才 `resolve`/`reject`（`AtomicBoolean` 保证只回一次）；
2. 五个命令全部标 `#[tauri::command(rename_all = "snake_case", async)]` ⇒ 同步函数体落 `sync_threadpool`，不在主线程。

> 契约测试的正则随之放宽为 `[^)]*rename_all = "snake_case"[^)]*`——不变量的本体是「每个 command 都声明 snake_case」，不是「参数列表只有一项」。63 → 58 的假红就是这么来的。

**错误映射**：`canceled` → `Canceled`（前端静默）；`invalidated`（`KeyPermanentlyInvalidatedException`）→ `Invalidated`，最终呈现 `stale + enrollment_changed`；长度越界/解密失败 → `tampered`；插件不可达 → `Unavailable(Unsupported)`。`onAuthenticationFailed`（这一下没认出指纹）**不回复**——系统会让用户继续试，此时 reject 会把「没对准」变成一次失败解锁。

**撤销**：`biometric_disable` 先读头部算 `bind`（删完就读不到）→ 删 `biometric.json` → 调插件 `remove` 删 Keystore 别名。`purge_platform_key()` 在 Windows 是空操作（DPAPI 主密钥由系统账户派生，我们没有自己的密钥句柄）。

**平台自述**：`BiometricStatusDto` 新增 `platform`（`windows`/`android`/`other`），前端据此取词（见 `06-ui-design.md` §6.28）。用后端说而不嗅探 UA——UA 随 WebView 与 ROM 漂移。

**撤销/失效的边界**：`minSdk 24`，API 30+ 用 `setUserAuthenticationParameters(0, AUTH_BIOMETRIC_STRONG)`（auth-per-use + 限强生物识别），24–29 退 `setUserAuthenticationValidityDurationSeconds(-1)`；两档都设 `setInvalidatedByBiometricEnrollment(true)`。**不关掉它**——那正是 Android 唯一能告给我们、Windows 侧至今拿不到的失效信号（§21.4 候选 C 的等价物）。

守门：`src/lib/bio-android-guard.test.ts` 21 例 + `platform/keystore_rules.rs` 5 例（纯逻辑抽到平台无关模块，桌面 `cargo test` 就能验；`android_keystore.rs` 整文件被 `cfg(target_os="android")` 门控，写在里面的测试在桌面永远跑不到）。适配器测试 8 → **13**。

### 3.14.24 D-128 / D-124：回前台重查——接到**窗口事件**才成立（2026-10-28，两轮）

**问题**：设置页本来就挂了 `window.addEventListener('focus', …)` 去重查快速解锁状态（D-114）。它在 Windows 上成立——窗口真的丢焦点（切到系统设置）又拿回来。**Android 不成立**：Activity 进后台再回前台时，WebView 里的 `document` 从未失去焦点，`focus` 事件不触发，`refreshBiometric()` 不跑，卡片停在旧值。

**修法**：移动端的生命周期只有宿主知道，所以在事件循环回调里加一个分支。**注意是窗口事件，不是 `RunEvent::Resumed`**（D-124 修正，原因见下）：

```rust
app.run(|app_handle, event| match event {
    tauri::RunEvent::Exit => { /* D-96 剪贴板兜底 */ }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    tauri::RunEvent::WindowEvent {
        event: tauri::WindowEvent::Resumed,
        ..
    } => {
        use tauri::Emitter;
        let _ = app_handle.emit(APP_RESUMED_EVENT, ());
    }
    _ => {}
});
```

> **D-124（主人真机第二次）**：初版接的是 `tauri::RunEvent::Resumed`，那条**在移动端一次都不发**——`tauri-runtime-wry` 全局只有一处 `callback(RunEvent::Resumed)`，条件是 `Event::NewEvents(StartCause::Poll)`（桌面事件循环）；安卓的 `onResume` 经 tao/ndk-glue 落地为 `Event::WindowEvent { event: WindowEvent::Resumed }`。错的是**层**，不是字面量，所以编译通过、旧守门全绿、真机毫无反应。

四条实现约束：

| 约束 | 原因 |
|---|---|
| 分支必须 `#[cfg(any(target_os = "android", target_os = "ios"))]` | `WindowEvent::Resumed` 在上游是 `#[cfg(mobile)]`（tauri 2.12.3 `app.rs`），桌面目标不加 cfg 直接编译失败 |
| 必须匹配 `RunEvent::WindowEvent { event: WindowEvent::Resumed, .. }` | 见上面的 D-124 注；`RunEvent::Resumed` 在移动端是死分支 |
| 事件名走常量 `APP_RESUMED_EVENT` | 与前端 `src/lib/app-resume.ts` 是**跨语言字面值**，手写字符串就失去逐字比对保护（D-118 同类坑） |
| 前端注册在**根组件**而非设置页 | 锁屏按钮的可见性同样依赖这份状态；用户在系统设置里加了指纹回来，锁屏那个按钮就该立刻变 |

**上游转发**：tauri 在 `manager/window.rs` 收到同一个窗口事件时，还会自动给 JS 发 `tauri://resumed`。前端**两条都听**（`RESUME_EVENTS`），互为兜底；同秒到达由 `createResumeGate()` 做在途合并（查询未回则丢弃第二次触发，回来后可再触发），避免两次回复乱序覆盖。

**前端**：`registerResumeListener()` 在 `App.svelte` 的 `onMount` 里注册，`hasTauriHost()` 判据保证 Node 测试环境不调 `listen()`；收到事件后过一遍纯函数 `shouldRefreshOnResume({ busy, route })`——正在解锁/正在开关（`busy`）时不插一脚，还在「创建保险库」向导（没有库）时不发噪音查询，其余情况 `session.refreshBiometric()`。

**真正修好的场景**：①设备还没录指纹/人脸（`not_configured` / `weak_only`）→ 去系统设置录完 → 回来卡片立刻可用；②把指纹全删了 → 回来立刻显示降级。这两件正是「去系统设置」这段引导所指向的事，之前只靠重启或重进设置页才能看到变化。

守门：`bio-android-guard.test.ts` 6 例——事件名逐字一致、**`lib.rs` 不许再出现 `RunEvent::Resumed`**（负向钉死死分支）、必须出现 `WindowEvent::Resumed` 且其前文有 `RunEvent::WindowEvent`（钉住「层」）、cfg 门、`RESUME_EVENTS` 含两条链路、`createResumeGate` 在途合并行为。**变异验证**：把 `WindowEvent::Resumed` 改回 `RunEvent::Resumed` → 两条同时变红；删掉 `tauri://resumed` → 第四条形红。

---

### 3.14.25 D-125：`weak_only`——只录了便捷级人脸，不许说「未设置」（2026-10-28）

**问题**：主人在 Android 上看到「本机尚未设置 指纹 / 人脸。」，而手机的人脸解锁一直可用。真因在 Android 的取值粒度：`BiometricManager.canAuthenticate(BIOMETRIC_STRONG)` 对「一档都没录」与「只录了便捷级（Class 2，多为 2D 人脸，部分虹膜同理）」返回**同一个** `BIOMETRIC_ERROR_NONE_ENROLLED`，旧代码把它一律映射成 `not_configured`，前端照模板出「尚未设置」。

**能力不变**：密钥用 `AUTH_BIOMETRIC_STRONG`（Class 3）保护，Class 2 凭据解不开 —— `weak_only` 仍 ≠ `Available`，`enable` / `unlock` 照旧拒绝（core 的判据只看「是不是 `Available`」，无需改动）。改的只是分类与文案。

**改动面**（缺一处就会退化成 `Unsupported`，卡片变成更糊涂的「组件无法调用」）：

| 层 | 改动 |
|---|---|
| Kotlin `BiometricPlugin.availabilityName()` | 两段判定：STRONG 成功 → `available`；**仅在 STRONG 报 `NONE_ENROLLED` 时**才追问 WEAK，WEAK 成功 → `weak_only`；无硬件 / 策略禁用 / 硬件占用原样上报，不被便捷档掩盖 |
| core `Availability` | 新增 `WeakOnly`（文档写明「不许并进 `NotConfigured`」的理由） |
| Rust `keystore_rules::availability_from` | `"weak_only"` → `WeakOnly`，**未知取值仍落 `Unsupported`** |
| Rust `biometric_cmd::availability_name` | `WeakOnly` → `"weak_only"` |
| 前端 | `bioShouldOfferSettings()` 纳入 `weak_only`（该去补一枚指纹）；文案 `settings.bio_avail_weak_only` 四语言 |

**文案**（zh-CN，D-126 起的版本）：`本机只录入了「便捷级」生物识别：它能解开手机屏幕锁，但 Android 不允许用它保护加密密钥，所以本功能用不了。请先在系统设置里录入一枚指纹。`

守门：Kotlin 里 `BIOMETRIC_WEAK` 必须在 `BIOMETRIC_STRONG` **之后**且上下文含 `NONE_ENROLLED`、全文件只出现一次（不许泄进 seal / unlock）；`AUTH_BIOMETRIC_STRONG` 与 `setAllowedAuthenticators(…STRONG)` 必须仍在；wire 与 core 枚举两端补齐；四语言键齐全且**禁「尚未设置 / not set up」**。Rust 单测 `weak_only_is_never_folded_into_not_configured`。

### 3.14.26 D-126：Android 文案去「人脸」+ 便捷级不可用于绑密钥的实证（2026-10-29）

主人拍板：绝大多数手机是 2D 便捷级人脸、本功能用不了，**文案不提「人脸」**，免得被当 bug 反复解释。

- 四语言字典：`bio_noun_android` / `bio_title_android` / `biometric_unlock_android` / `biometric_waiting_android` / `bio_desc_android` / `bio_risk_android` / `bio_degraded_hint_android` / `bio_manage_hint_android` / `bio_avail_weak_only` / `bio_state_stale_enrollment_android` / `bio_invalidated_enrollment_android` / `biometric_canceled_android` / `password_changed_bio_off_android` 全面去字（英文按词边界替换，`fingerprint / face` → `fingerprint`）。
- 「记录已变更」统一改说**生物识别记录**（准确性：任何生物特征变化都会废掉密钥）。
- **平台限制实证**：本地核查 `android.jar`（api 35/36/37）的 `android.security.keystore.KeyProperties`，`AUTH_BIOMETRIC_STRONG` 存在、**`AUTH_BIOMETRIC_WEAK` 三个版本都不存在** ⇒ Keystore 从未提供「便捷级生物识别绑密钥」的取值。放宽只能走时长型认证（`validityDuration`，安全等级下降）或纯界面锁（不可信），主人裁定维持 A 案。
- 守门：`bio-android-guard.test.ts` 新增 `d126` 四例（字面量零命中 / 取值逐字 / weak_only 仍给出「录入指纹」这一步 / `bio_manage_hint_android` 禁自动承诺），变异验证三条全红。

### 3.14.27 Windows 适配器变成「TPM2 优先 + DPAPI 兜底」（1.4.8，2026-10-29）

架构上只有一处变化，但它是**语义级**的：`VaultSealer::subtype()` 原来同时承担两件事，Windows 两条腿并存后必然错一件。

| 语义 | 方法 | 谁读它 |
|---|---|---|
| 新封印**写**什么 subtype | `subtype()` | 只有 `enable` |
| 这条老封印我**解不解得了** | `accepts(subtype)`（新增，带默认实现） | `unlock` |

默认实现 `subtype == self.subtype()` 让 Android / Mock / Unsupported 三个适配器**零改动**、行为不变。`unlock` 里的等值比较改成 `!sealer.accepts(...)`。

适配器结构（`platform/mod.rs` → `PlatformSealer = WinSealer<HelloGate>`）：

```
platform_sealer() ── WinSealer::new()
                      ├── tpm_ready()? → Some(TpmSealer)   subtype()=WindowsTpm2Seal
                      └── 否          → None               subtype()=WindowsDpapiUser
                     seal()   : 有 TPM 走 TPM，否则 DPAPI
                     unseal() : 按密文魔数 PMST 分派（**不**按当前 subtype）
                     accepts(): DPAPI 恒 true / TPM2 看本机有无 TPM / Android false
                     purge()  : 删 TPM 持久密钥（DPAPI 无可删物）
```

两条设计约束值得记住：

1. **`unseal` 按密文自识别，不按「当前首选」**。否则「本机 TPM 今晚读不到」会连累老 DPAPI 封印一起解不开。
2. **升级不搬移**。老封印保持 DPAPI 不动，用户下次「重新开启」才迁到 TPM——迁移动作永远由用户的一次显式操作触发，后台不偷偷重写密钥文件。

威胁模型、实测数据、PKCS#1 取舍、回滚行为见 `docs/08-biometric-unlock.md` §33。

### 3.14.28 Hello 闸门改用 interop 版 API（1.4.9，2026-10-29）

`platform/windows_dpapi.rs` 的闸门实现有一处调用变更，架构含义只有一条：**桌面应用不再调用 UWP 专属 API**。

| 项 | 改前 | 改后 |
|---|---|---|
| 调用 | `UserConsentVerifier::RequestVerificationAsync` | `IUserConsentVerifierInterop::RequestVerificationForWindowAsync(hwnd, msg)` |
| owner 窗口 | 无 | 主窗口 HWND |
| 线程模型 | `run_on_sta`（STA + `CoInitializeEx`）+ `pump`（`PeekMessage`） | **不变**（§34.2 实测证明必需） |
| 回退 | — | Win10 / 无窗口 → 原 UWP 版 |

新增一个进程级寄存器 `platform::main_hwnd`（`AtomicIsize`，启动时由 `note_main_window()` 写一次），因为闸门从命令线程进入，那条路径上没有 `AppHandle`。句柄每次现查 `IsWindow`，不缓存有效性判断。

可用性查询（`CheckAvailabilityAsync`）仍走 UWP 版：interop 只提供窗口版请求方法，没有窗口版可用性查询。

为什么不能借这次改动删掉 `run_on_sta` / `pump`：见 `docs/08-biometric-unlock.md` §34.2 的四条实测（尤其 `GetResults()` 在无消息泵的 STA 上返回 `E_ILLEGAL_METHOD_CALL`）。
