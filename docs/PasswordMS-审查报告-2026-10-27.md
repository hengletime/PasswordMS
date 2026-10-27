# PasswordMS 全面审查报告（代码 / 逻辑 / 契约 / 多语言文案）

- **审查日期**：2026-10-27
- **审查对象**：PasswordMS 1.2.22（Tauri 2 + Svelte 5 + Rust workspace），commit `02dccb0`
- **审查方式**：6 路并行静态审查（Rust 核心 ×3、Tauri 后端、前端、i18n 文案）+ 独立门禁实测；**全程只读，未修改任何源码 / 测试 / 配置**（唯一例外：门禁代理为验证 vitest 退出码做过一次性探针用例，测后即删，结束时 `git status` 干净）
- **验证手段**：逐条给出 文件:行号 + 代码原文证据；运行了 `npm run test`、`npm run check`、`npm run check:css`、`cargo test -p passwordms-core`、`cargo test`（workspace）

---

## 一、总体结论

核心密码学设计与数据模型无 P0 级致命缺陷，六项既有「项目铁律」（blob 引用不变量、帧编码、snake_case 契约、同步三段式、锁中毒留痕、错误闭环）核对全部成立；**但存在 4 条 P1 级问题与 22 条 P2 级问题需要排期修复**，另确认 3 条历史遗留未修复项（S1/S6/C3）与 1 项环境性门禁断裂（workspace cargo test）。

### 严重级别统计（本次新发现，共 58 条）

| 级别 | 数量 | 说明 |
|---|---:|---|
| P0 致命 | 0 | 未发现数据丢失必现路径、可利用远程漏洞或解锁绕过 |
| P1 高 | 4 | 1 条安全清理旁路（锁中毒）；3 条文案/文档功能性问题 |
| P2 中 | 22 | 数据一致性、竞态、边界校验、错误处理、安全纵深 |
| P3 低 | 32 | 润色、可维护性、文档注释、格式 |

### 各模块分布

| 模块 | P1 | P2 | P3 | 合计 |
|---|---:|---:|---:|---:|
| Rust 核心 · 存储/数据模型（store/model） | 0 | 7 | 6 | 13 |
| Rust 核心 · 密码学/会话/辅助（crypto/session） | 0 | 1 | 7 | 8 |
| Rust 核心 · 同步/导入（sync/import） | 0 | 1 | 3 | 4 |
| Tauri 后端（src-tauri） | 1 | 5 | 7 | 13 |
| 前端（Svelte/TypeScript） | 0 | 6 | 4 | 10 |
| 多语言文案（i18n/README/legal） | 3 | 2 | 5 | 10 |

### 高信号结论（Top 5）

1. **P1：锁定时导入预览明文清理被锁中毒旁路**（`vault_cmd.rs:71-76`）——A-04「锁定后内存无明文残留」在 Mutex 中毒场景失效。
2. **P2：解锁错误统一化（A-05）存在逃逸口**（`session.rs:161`）——篡改 KDF 参数可毫秒级返回 `ERR_VALIDATION`，绕开 burn-KDF 耗时均衡，与「任何失败统一 UnlockFailed」注释承诺不符。
3. **P2：同步发送水位在 Joiner 配对路径被丢弃**（`client.rs:195-207`）——O1「大库免重发」在配对→首轮重同步之间失效（不丢数据，性能回退）。
4. **P2：存储写入仅 `flush()` 不 `sync_all()`**（`container.rs:200-217`）——掉电丢失最后几秒写入；`place_frame` 长度变更路径 IO 失败可致条目丢失（`repository.rs:443-450`）。
5. **P2：前端列表加载无请求序号守卫**（`vault.svelte.ts:42-59`）——快速切分组/搜索/复制后出现列表与筛选状态不一致。

---

## 二、审查范围与方法

| 范围 | 内容 |
|---|---|
| Rust 核心（crates/passwordms-core） | `store/`（repository/container/frame）、`model/`、`crypto/`、`session.rs`、`generator.rs`、`health.rs`、`totp.rs`、`url_safety.rs`、`error.rs`、`sync/`（15 文件）、`import/`（4 文件）、`paths.rs`、`constants.rs`、`attachment_name.rs`、`securemem.rs` |
| Tauri 后端（src-tauri） | `commands/`（13 命令模块）、`state.rs`、`dto.rs`、`error.rs`、`lib.rs`、`content_uri.rs`、`tauri.conf.json`、`capabilities/default.json`、`build.rs` |
| 前端（src） | 全部组件（components/ 含 common/、sync/）、`lib/` 全部（含 stores/）、`types.ts`、`App.svelte`、`main.ts` |
| 多语言文案 | `i18n/{en-US,zh-CN,zh-TW,th,index}.ts`、`legal-content.ts`、组件硬编码文案、`README.md`、`README.zh-CN.md` |
| 门禁实测 | `npm run test` / `check` / `check:css`、`cargo test -p passwordms-core`、`cargo test`、SHA256SUMS 核对 |
| 历史基线 | `docs/PasswordMS-security-audit-2026-10-25.md`、`deliverables/gstack/full-audit-passwordms-2026-10-24.md`（D-91）、`security-audit-triage-d96-2026-10-25.md`（D-96）、`proposal-o1o2-sync-redesign-2026-10-24.md` |

方法：静态阅读 + 机械化键 diff（Node 脚本）+ 全量测试实测；每条发现均在当前代码中核实并引用原文。**覆盖范围**：本次覆盖上述清单内文件的业务逻辑、边界、安全与文案；不覆盖（见第八节）：动态运行时行为、真机 Android、真实崩溃/掉电复现、Rust 依赖 CVE 库扫描。

---

## 三、按模块分组的详细发现清单

### 3.1 Rust 核心 · 存储与数据模型（P2 ×7，P3 ×6）

#### [P2] 3.1-1 业务写路径无 `sync_all()`，掉电丢失最后写入帧
- 位置：`crates/passwordms-core/src/store/container.rs:200-217`
- 描述：`append_frame`/`pwrite_frame` 仅 `flush()`（在 `std::fs::File` 上是 no-op，数据仍在 OS page cache）。改主密码路径（`container.rs:134`）已补 `sync_all`，但 create/update/delete/attach 全部业务写路径未补。
- 影响：掉电后重启 `rebuild_index` 从磁盘重扫，最后几秒的写入（条目/附件/回收站记录）静默丢失。
- 证据：`file.write_all(&bytes)?; file.flush()?;`（无 sync_all）。
- 修复建议：两函数在 `flush()` 后补 `file.sync_all()?`；性能敏感可批量写后统一 sync。

#### [P2] 3.1-2 `place_frame` 长度变更路径：旧帧墓碑化后新帧追加失败 → 条目丢失
- 位置：`crates/passwordms-core/src/store/repository.rs:436-451`
- 描述：更新帧长度变化时先 `pwrite_frame` 把旧帧原地墓碑化（磁盘已改），再 `append_frame` 追加新帧；追加失败（磁盘满/IO 错）则旧内容已毁、新内容未落，重启后条目从 `self.items` 消失且不可恢复。
- 影响：磁盘满/IO 错误下更新条目（改密码后 JSON 长度变化属高频）丢数据。
- 修复建议：先 append 新帧成功后再墓碑化旧帧（append 失败则旧帧未动，可重试）。

#### [P2] 3.1-3 `restore_item` 不校验 folder_id 存在性，还原后进入悬空分组
- 位置：`crates/passwordms-core/src/store/repository.rs:1582-1610`
- 描述：还原直接复用 `item_snapshot` 的 `folder_id`，不检查分组是否仍存在（回收站期间分组被删）。`create/update/move` 均有 `validate_draft_folder`，唯还原路径缺。
- 影响：条目在对应分组视图不可见（仅在全部/未分组出现），用户误以为丢失。
- 修复建议：还原时若 folder_id 不存在则置 `None`（移入未分组）或提示选择目标分组。

#### [P2] 3.1-4 `read_blob` 返回非 Zeroizing 明文，附件内容内存残留
- 位置：`crates/passwordms-core/src/store/repository.rs:1281-1285`
- 描述：`open_frame` 返回 `Zeroizing<Vec<u8>>`，但 `read_blob` 用 `.to_vec()` 克隆为普通 `Vec<u8>` 返回（Drop 不覆写）。`blob_chunks`（:1326）同受影响。
- 影响：A-04 要求锁定时无明文残留；附件明文（≤20MB）在导出/同步后仍可能残留在堆中。
- 修复建议：返回类型改 `Zeroizing<Vec<u8>>`，或调用方用后显式 zeroize。

#### [P2] 3.1-5 `write_blob_frame` 不校验 blob 大小，同步端可推送超大 blob
- 位置：`crates/passwordms-core/src/store/repository.rs:1536-1561`
- 描述：接收同步 blob 帧时仅校验 `is_blob()` 与 record_type，不检查帧大小（20MB 上限）与库总容量（200MB）。本地路径 `insert_attachment`（:1222-1230）有完整检查。
- 影响：被入侵/异常的配对端可推送超大 blob 导致本地库膨胀（信任边界内 DoS，纵深防御缺失）。
- 修复建议：append 前校验 `plaintext_len <= MAX_ATTACHMENT_BYTES` 且 `file_size + encoded_len <= MAX_VAULT_BYTES`。

#### [P2] 3.1-6 384KiB 明文上限未贯穿同步写入路径
- 位置：`crates/passwordms-core/src/store/repository.rs:1812-1821`（put_item_direct）、`:1718-1797`（resolve_conflict TakeRemote/KeepBoth）
- 描述：`validate_item_kind` 只在 create/update 调用；`put_item_direct`/`resolve_conflict` 直接 `write_record` 不校验大小。
- 影响：异常对端可绕过 384KiB 上限，本端照写不误，可能库膨胀/后续同步异常。
- 修复建议：同步写入前同样校验序列化长度上限。

#### [P2] 3.1-7 `tombstone()` 先删内存索引再写磁盘，IO 失败内存/磁盘不一致
- 位置：`crates/passwordms-core/src/store/repository.rs:454-462`
- 描述：先 `self.live.remove(...)` 再 `pwrite_frame` 写墓碑；写盘失败时内存已删、磁盘仍 live，重启后条目「复活」，与用户删除预期冲突。
- 修复建议：写盘成功后再删内存索引；失败保留 live 下次重试。

#### [P3] 3.1-8 打开库仅交叉核对 item_count，未核对 folder_count
- 位置：`repository.rs:220-226`。建议补充 folder_count 核对。

#### [P3] 3.1-9 `read_frames` 静默容忍尾部 <124B 残料
- 位置：`container.rs:188-196`。建议循环结束 `cursor < data.len()` 时补 open_warning（备份恢复路径 `verify_frame_stream` 已严格校验，正常打开路径未检查）。

#### [P3] 3.1-10 错误码语义不准确
- 位置：`repository.rs:1269`（remove_attachment 附件未找到返回 `ItemNotFound`）、`:1282`（read_blob 返回 `ItemNotFound`）。建议改用 `AttachmentNotFound`/`BlobMissing` 等精确变体。

#### [P3] 3.1-11 repository.rs 上帝对象仍成立
- 本次实测 **2569 行**（门禁核对代理曾报 2388，经复核 2388 为计数误差，以 2569 为准），聚合条目/分组/回收站/冲突/附件/设备/同步/压缩/导出等约 50 个 pub 方法。可维护性项（历史 O4，设计取舍不修）。

#### [P3] 3.1-12 `delete_folder` 多条目操作非事务
- 位置：`repository.rs:657-700`。循环中第 N 条失败则前 N-1 条已落盘，半完成状态无回滚。

#### [P3] 3.1-13 `empty_trash` 逐条 purge 中途失败不可撤销
- 位置：`repository.rs:1635-1641`。已 purge 的条目/附件已墓碑化不可恢复；建议收集错误尽量清空所有能清的。

**边界核对表（节选）**：空标题/超长 SecureNote/非法 folder_id（create/update）/附件 20MB/16 个/文件名 255B/非法 UUID/重复分组名/回收站还原同 id live（RestoreConflict 已守卫）均通过；风险项集中在：同步路径大小校验（3.1-5/6）、restore 悬空分组（3.1-3）、<124B 尾料静默（3.1-9）、掉电与 IO 失败路径（3.1-1/2/7/12/13）、附件明文擦除（3.1-4）。`now_ms` 无单调时钟（时钟回拨未覆盖）、folder 名无长度上限（未覆盖）。

### 3.2 Rust 核心 · 密码学 / 会话 / 辅助（P2 ×1，P3 ×7）

#### [P2] 3.2-1 解锁路径 A-05 错误统一化存在三个逃逸口，且夹取失败路径毫秒级返回
- 位置：`crates/passwordms-core/src/session.rs:161`、`:169`、`:170`（夹取见 `crypto/kdf.rs:21-32`）
- 描述：session.rs:124-133 注释承诺「除 NoVault 外任何失败统一 UnlockFailed」，且 burn-KDF 补偿只挂在 `Container::open` 失败分支（:145-148）。其后三个 `?` 原样上抛：①KDF 参数超限（篡改库头）返回 `ERR_VALIDATION` 且发生在 Argon2 执行**之前**（毫秒级 vs 正常秒级）；②`.device` 文件 IO 失败返回 ERR_IO；③`Repository::open` 二次 open/rebuild 结构性损坏返回 ERR_INTERNAL/ERR_CRYPTO。
- 影响：本地篡改者可借错误码+耗时差区分「头解析失败/参数非法/密码错误」，与 A-05 设计目标相悖；burn-KDF 留下唯一没补偿的快速通道。
- 修复建议：:161-170 非 NoVault 失败统一折叠为 `UnlockFailed`，夹取/Argon2 失败前补一次 `burn_unlock_kdf`；构造假库文件补回归测试。

#### [P3] 3.2-2 `burn_unlock_kdf` 硬编码默认 KDF 参数
- 位置：`session.rs:322-330`。当前 create 硬编码默认参数故等价；未来开放自定义派生强度后耗时均衡失真。建议注释标注或改造为「头部能读参数就按参数 burn」。

#### [P3] 3.2-3 url_safety 误拒 `host:port` 形态 URL
- 位置：`crates/passwordms-core/src/url_safety.rs:17-21`。补 `https://` 条件为「不含 `://` 且不含任何 `:`」，`mysite.com:8443` 被误判 UnsafeUrl。**未发现 `javascript:`/`file:` 白名单绕过**。修复：判定改为「不含 `://`」即可。

#### [P3] 3.2-4 TOTP base32 密钥无最小长度校验
- 位置：`totp.rs:66-77`。`"A"` 类弱密钥可被接受生成可枚举口令；RFC 6238 建议 ≥160bit（16 base32 字符）。建议解码后 `key.len() < 10` 拒绝。

#### [P3] 3.2-5 健康检查重复密码为精确字节匹配
- 位置：`health.rs:58-63`。`"pw"` 与 `"pw "` 判为不同而漏报重复；建议 trim + 去零宽字符后比较。

#### [P3] 3.2-6 session.rs 全模块零单元测试
- A-05 统一化 / burn-KDF / 锁态清零 / 改密流程均无 core 级回归网（本次 P2 逃逸口即因此长期存在）。建议补假库 unlock、耗时上下界、lock 后状态、错误旧密码改密 4 类测试。

#### [P3] 3.2-7 unlock 并发创建 `.device` 文件无害写竞争
- 位置：`session.rs:333-345`。内容为随机 UUID，后写胜，结果等价；可选 `create_new(true)`。

#### [P3] 3.2-8 unlock 两次 `Container::open` 间 TOCTOU 窗口
- 位置：`session.rs:139` / `repository.rs:198`。窗口毫秒级，文件被替换时错误码逃逸统一化（与 3.2-1 同源）；建议复用第一次 open 的 Container。

**核对通过的正面结论**：AEAD 每条记录 CSPRNG 新取 32B IK + 24B nonce（无复用路径）；AAD = record_type‖record_id 绑定完整；HKDF 双 info 域分离；`subtle::ConstantTimeEq` 常量时间比较；MK 用完即 drop 清零；lock() 先 wipe_plaintext 再级联 Drop；KDF 夹取（1GiB/16/8）真实生效且对存量库零影响；错误码 31 变体全部有映射；`From<io::Error>` 不留路径；主密码最小 8 字符；生成器拒绝采样无模偏置；解锁期维护错误已收集进 `maintenance_warnings`（D-91 修复在位）。

### 3.3 Rust 核心 · 同步 / 导入（P2 ×1，P3 ×3）

#### [P2] 3.3-1 Joiner 配对路径 O1 发送水位被段②重读丢弃
- 位置：`crates/passwordms-core/src/sync/client.rs:195-207`（重读点 `:200`），对照正确实现 `resync.rs:410-420`、`server.rs:375-393`
- 描述：`push_and_sync_access` 段①锁内导出后，:178-181 在**内存 state** 上推进水位 W'；段②闭包**重新从 repo 读取** `st = repo.sync_state(&host_id)`（:200，仍是旧水位 W），:207 将 state 重新绑定为该 fresh-read → W' 整体丢弃、未落盘。
- 影响：**不丢数据**（merge 幂等），但配对→首轮重同步整库全量重发，O1「大库免重发」收益在该段失效（1500+ 条目约等于再做一次全量同步）。
- 修复建议：段②闭包改 `move` 捕获外层 `state` 直接修改（逐行对齐 resync.rs），删除 fresh-read；补集成测试「配对后立即 resync，第二轮 `export_changes` 返回空」。

#### [P3] 3.3-2 identity.rs 注释文件长度算错（81 vs 实际 82 字节）
- 位置：`sync/identity.rs:23`。magic `PMSIDENT1\0` 实为 10 字节（`constants.rs:76`），`IDENTITY_FILE_LEN = 82`；代码与测试正确，仅注释误导。

#### [P3] 3.3-3 import apply 克隆整个 preview.items
- 位置：`import/mod.rs:198`。万条导入峰值内存约双倍；改 `into_iter()`/`drain(..)` 即可。

#### [P3] 3.3-4 discovery 请求侧仅挂 1 个 tag，与模块文档「多 tag」不符
- 位置：`discovery.rs:184` vs 模块文档 `:28`。功能正确（响应方遍历配对表），仅文档-实现不一致。

**边界核对表（节选，均通过）**：blob_ref 路径穿越已封堵（`blob_transfer.rs:387-388` UUID 规范化 + `:228` spool 纵深校验，测试覆盖绝对路径/`..`/反斜杠）；`chunk_total=0` underflow 已拒；帧长 u32 前缀超限拒绝；段②期间锁定/换库整段丢弃（`SyncAborted`）；发送中途断连水位不推进（宁多勿丢）；接收 4096 页/200MB 上限生效；导入 64MB/10000 条上限生效；导入去重回填生效（`import/mod.rs:213`）；GBK/UTF-16 编码容错生效；C1（pair 记录实际连通 IP）、S5（discovery 限速 per-source 5/s + global 50/s）、S6（令牌失败仅 REJECT）、D-98（purge 墓碑跨端清理回收站快照，rev 条件正确）、D-96（VK 包裹身份零化/迁移幂等）均验证通过；merge `remote_deleted` 不制造重复回收站条目（C2 修复在位）。

### 3.4 Tauri 后端（P1 ×1，P2 ×5，P3 ×7）

#### [P1] 3.4-1 锁定时 `pending_imports` 锁中毒导致导入预览明文残留内存（A-04 旁路）
- 位置：`src-tauri/src/commands/vault_cmd.rs:71-76`
- 描述：`lock` 命令清理导入预览缓存用 `if let Ok(mut guard) = state.pending_imports.lock()`，中毒时静默跳过 `wipe_plaintext()` 与 `clear()` —— 完整导入预览明文（标题/用户名/URL/分组名）残留在内存。同函数第 61 行 `session` 锁用的是 `map_err(lock_err)?`（中毒即报错），两条路径口径不一致。
- 影响：A-04「锁定后无残留明文」在锁中毒场景失效；内存取证可看到上一会话导入的明文。
- 修复建议：与 session 锁统一口径——`PoisonError::into_inner()` 拿到 guard 照常清理；最差也要 `append_log` 留痕（当前完全静默）。
- **交叉印证**：门禁核对 M4 项确认残留 `if let Ok` 均为 best-effort 清理路径（`vault_cmd.rs:71`、`sync_cmd.rs:461/472`），其中本条因处于 A-04 清理路径升为 P1。

#### [P2] 3.4-2 resync 锁跨整个网络连接持有，与「网络 I/O 全程锁外」注释不符
- 位置：`src-tauri/src/state.rs:260-283`
- 描述：`spawn_resync_worker` 第二次取 `resync` 锁后在 guard 存活期间调用 `handle_connection`（整场被动同步网络 I/O + 加解密 + 合并）。`session` 锁确实已按 O2 三段式下沉（`SessionAccess::with_repo` 短暂取锁），但 `resync` Mutex 本身跨网络持有 → `stop_resync_listener`（:191）与 `sync_now`/`sync_start_pairing`/`unlock` 的 `ensure_resync_listener` 在被动同步期间全部阻塞。
- 影响：锁定操作在被动同步进行期间卡死 IPC；与设计注释矛盾（O2 改造漏改的层级）。
- 修复建议：锁内只 `clone()` 出 `ResyncHandle`（stop + server Arc）即释放，锁外 `handle_connection`；或 `try_lock` 跳过。

#### [P2] 3.4-3 `stop_resync_listener` 锁中毒静默，锁定后重同步监听不停止
- 位置：`state.rs:191-198`
- 描述：`if let Ok(...)` 中毒即返回，不置 `stop=true` 不清空 handle；worker 继续 200ms 节奏 `poll_accept`（虽 `with_repo` 会因 NotUnlocked 拒绝业务，但 socket 仍被接受、线程不退出）。无线索日志。
- 修复建议：`into_inner()` 兜底执行 stop 置位与清空；至少 append_log 留痕。

#### [P2] 3.4-4 IPC 路径原语无 Rust 侧白名单（S2 记录）
- 位置：`vault_cmd.rs:96-105`（export_vault）、`import_cmd.rs:98-112`（save_import_template）、`vault_cmd.rs:121-151`（restore_backup）、`keydoc_cmd.rs:94-119`（export_attachment）
- 描述：`dest_path`/`source_path` 直接交给 `std::fs`，无目录白名单。威胁模型：前端 JS 被 XSS/供应链投毒（CSP `script-src 'self'` 挡不住被篡改的 self bundle）时可任意写小文件（模板内容固定）/ 任意读（经 import_preview 加密入库，不直接回显）。
- 修复建议：纵深防御——Rust 侧校验扩展名/所在盘，或要求路径带 dialog 插件签发的 token；至少在 docs 明确记录该接受风险。

#### [P2] 3.4-5 `copy_secret` 的 `ttl_secs` 被丢弃，Rust 兜底恒 30s，与用户可配置 TTL 不一致
- 位置：`system_cmd.rs:38-55`（`let _ = ttl_secs;` :46，兜底线程恒 `CLIPBOARD_TTL_SECS=30` :62）
- 描述：`VaultSettings.clipboard_clear_secs` 用户可配置（如 60s），前端 60s 倒计时到点才清，但 Rust 兜底线程 30s 先清——密码在用户第二次粘贴前被提前清掉。方向偏安全侧，但违背用户配置预期。
- 修复建议：兜底线程按 `ttl_secs`（或读取设置）睡眠；或删除该参数并注释说明恒 30s。

#### [P2] 3.4-6 auto-lock 完全由前端 JS 定时器驱动，无 Rust 兜底
- 位置：`vault_cmd.rs:57-80`（lock 命令本身）；全仓 grep 无 Rust 侧定时器
- 描述：WebView 后台/休眠节流定时器时计时暂停，保险库保持解锁态。剪贴板有 Rust 兜底（D-96），自动锁定没有对应兜底。
- 影响：用户离开后预期自动锁定实际不触发；安全模型里唯一只有前端守的环节。
- 修复建议：Rust 侧低优先级线程按 `auto_lock_minutes` 检查「距上次 IPC 活动时长」，超时主动 lock + emit 事件；或至少窗口失焦超时锁定。

#### [P3] 3.4-7 `ERR_SYNC_BUSY` 不在 core codes 表，靠 sync_cmd 硬编码构造
- 位置：`sync_cmd.rs:239-243`。建议补 `CoreError::SyncBusy` 变体或提为常量，消除手工同步点。

#### [P3] 3.4-8 `vault_file_info` 回传保险库绝对路径给前端 JS
- 位置：`system_cmd.rs:136-139`。建议只回传文件名或相对 data_dir 路径。

#### [P3] 3.4-9 pairing worker 清空 pairing 状态时锁中毒静默，SyncServer 变僵尸监听
- 位置：`sync_cmd.rs:461-463`、`472-474`。`into_inner()` 兜底 + append_log。

#### [P3] 3.4-10 `ensure_resync_listener` 竞态：先写 port_file 后发现端口被占，回退端口未同步
- 位置：`state.rs:145-185`。port_file 写入应移到确认本 server 被采纳后的第二个锁段内。

#### [P3] 3.4-11 capabilities 权限偏宽（dialog/opener/fs 均 :default）
- 位置：`capabilities/default.json:6-19`。建议显式细粒度权限；`opener:default` 允许 JS 绕过 Rust `open_url` 的 http/https 白名单打开任意 file:///mailto: 等。

#### [P3] 3.4-12 CSP 缺纵深指令
- 位置：`tauri.conf.json:28`。`script-src 'self'` 良好；建议补 `object-src 'none'; base-uri 'none'; form-action 'none'`。

#### [P3] 3.4-13 `random_port` RNG 失败退化到固定端口 49152
- 位置：`sync_cmd.rs:357-366`。RNG 失败应直接报错，不要退化到可预测端口。

**核对通过的正面结论**：O2 三段式整体到位（session 锁不跨网络 I/O，sync_busy CAS 三方互斥，锁中毒 worker 留痕）；`IpcError::from(CoreError)` 完整透传；`CoreError` 手写 Debug 只打印错误码 + io kind，不打印载荷（日志脱敏成立）；剪贴板兜底线程 + `RunEvent::Exit` 钩子双保险；锁定级联顺序（session.lock → clear_owned_clipboard → pending_imports 先擦后清 → stop_resync_listener）注释明确；restore_backup 要求锁定态 + magic 校验 + 临时文件成败都删；content:// 读取按 20MB/64MB 上限 take。

### 3.5 前端（P2 ×6，P3 ×4）

#### [P2] 3.5-1 `vault.load()` 无请求序号守卫，快速切换筛选时乱序响应覆盖最新结果
- 位置：`src/lib/stores/vault.svelte.ts:42-59`（load）、`:75-78`（setQuery）、`ItemList.svelte:49-63`（220ms 防抖）
- 描述：多个 `list_items` 在途时后返回的覆盖先返回的；如「搜索 ab 未返回就点仅收藏」，ab 结果后到会覆盖收藏结果，UI 状态（favoriteOnly/query）与列表数据不一致。
- 修复建议：`load()` 内加单调递增 `loadSeq`，赋值前 `if (seq !== this.loadSeq) return;`。

#### [P2] 3.5-2 每次复制都触发全量 `vault.load()`，与用户在途操作形成同源竞态
- 位置：`vault.svelte.ts:198-205`（recordItemUsage）；调用点 `ItemDetail.svelte:100-102`、`SecureNoteView.svelte`
- 修复建议：与 3.5-1 同加序号守卫；或让 `record_item_usage` 返回新 use_count 本地 patch，避免全量 reload。

#### [P2] 3.5-3 剪贴板自动清空失败被静默吞掉
- 位置：`src/lib/clipboard.ts:62-70`（`:67` `void api.system.clearSecretIfOwn();` 无 catch）
- 描述：倒计时到点 IPC reject（后端不可达/锁中毒）时密码残留剪贴板且用户无感知；前端倒计时 UI 已停，只靠后端 30s 兜底。
- 修复建议：`.catch(() => ui.toast('error', 'err.clipboard_clear_failed'))`。

#### [P2] 3.5-4 设置页剪贴板 TTL 输入未钳制，0/负值立即清空、极大值长期残留
- 位置：`Settings.svelte:327-330`（`<input type="number" min="5" max="120">`）；消费点 `ItemDetail.svelte:89`；后端 `settings_cmd.rs:16-23` 直接透传不校验
- 描述：HTML min/max 不阻止键入；TTL=0/负 → 复制即清；极大值 → 密码长期残留剪贴板。
- 修复建议：保存前 `Math.min(120, Math.max(5, Math.floor(Number(v) || 30)))`；后端 `update_settings` 加校验。

#### [P2] 3.5-5 启动期 `get_status` 失败被归一为「无库」，误导向创建向导
- 位置：`src/lib/stores/session.svelte.ts:18-25`；路由决策 `App.svelte:12-16`
- 描述：`refresh()` catch 把状态写成 `{ has_vault: false, locked: true }`，App 据此路由到 create。启动瞬态 IPC 故障被伪装成「还没建库」，用户可能误建新库（向导有写库路径）。
- 修复建议：增加 `statusError` 状态；刷新失败显示「启动失败，重试」而非路由到 create。

#### [P2] 3.5-6 拖拽后 `clickSuppressed` 在异常路径下永不复位，吞掉下一次真实点击
- 位置：`ItemList.svelte:93`（声明）、`:136-138`（onDrop 置 true）、`:274-282`（唯一复位点 onclick）
- 描述：指针落在行外/窗口失焦时不派发 click，flag 永真，下一次点任意行被吞（「点列表无反应」）。
- 修复建议：在 dnd 的 `finish()` 或 `setTimeout(() => clickSuppressed = false, 0)` 兜底复位。

#### [P3] 3.5-7 `sync_accept_pair` 前后端均为占位死代码
- 位置：前端 `src/lib/ipc.ts:216-217`；后端 `sync_cmd.rs:173-177`（`_session_id` 未使用，恒 `Ok(())`）；全仓无调用方。建议删除或补全。

#### [P3] 3.5-8 `settings.saveQuiet` 复用 `busy` 标志
- 位置：`settings.svelte.ts:67-77`。当前无 UI 影响；未来按钮绑 `disabled={busy}` 会偶发置灰。建议独立 `quietBusy`。

#### [P3] 3.5-9 导入向导 CSV 表头读取失败被静默重置
- 位置：`ImportWizard.svelte:63-72`。建议 catch 内 toast。

#### [P3] 3.5-10 解锁后 5 路并行加载失败 toast 堆叠
- 位置：`MainLayout.svelte:67-75`。建议首次加载失败合并为一条提示 + 重试入口。

**IPC 契约核对结论（重要正面结果）**：前端 `ipc.ts` vs 后端 `commands/*.rs` 共核对 **60 条命令**——命令名、snake_case 参数键、DTO 字段集（types.ts vs dto.rs）**0 处不一致**；唯一异常为 `sync_accept_pair` 参数名一致但后端为未实现 stub（3.5-7）。`ipc-contract.test.ts` 仅覆盖命令名注册一致性，参数名逐字比对与 DTO 字段集由本次人工补齐。历史修复点（lockNow 失败不 reset/不切路由、D-88 登录转笔记、D-89 备注 textarea、D-101 搜索清空、D-102 首启跟随系统语言）逐条证据确认在位，无回归。

### 3.6 多语言文案模块

详细问题清单见第五节（语言文案问题清单）。本模块共 10 条：P1 ×3、P2 ×2、P3 ×5。**键完整性无问题**：四语言包各 376 个叶子键完全对齐，0 缺失 / 0 多余；占位符 `{n}/{name}/{size}/{ip}` 四语言 0 处不一致；扫描到的 243 个字面量 `ui.t()` 键全部存在；th.ts 体积偏大（36KB）系泰文句长所致，非结构冗余。

---

## 四、历史已知问题修复状态核对表

### 4.1 D-91 全仓审查（2026-10-24，26 项）

结论口径：**已修复 / 未修复 / 部分修复 / 设计取舍不修 / 观察记录**。关键修复提交：`c684cb7`（P0 七项）、`5d1c99c`/`58222cf`（O1/O2）、`8c02416`（D-96 十项）、`6dca4d6`（Android+vitest）、`0b96f93`（D-98）。

| # | 历史问题 | 当前证据 | 结论 |
|---|---|---|---|
| R1 | blob spool 路径穿越 | `blob_transfer.rs:387-388` UUID 规范化 + `:228` spool 纵深校验 + 回归测试 | ✅ 已修复 |
| R2 | 回收站还原覆盖同步新数据 | `repository.rs:1592` 同 id live 存在则 `RestoreConflict`；测试 `restore_is_rejected_when_live_item_exists` | ✅ 已修复 |
| O1 | 大库同步永久失败 | `paging.rs:44-66` cursor 分页 + `repository.rs:2412` sent_watermark 过滤 + 测试 | ✅ 已修复（残留缺陷见 3.3-1） |
| O2 | 同步持锁冻结 UI | `sync_cmd.rs:114-158`/`246-257` 三段式 + `state.rs:205` | ✅ 已修复（残留缺陷见 3.4-2） |
| O3 | lockNow 谎报已锁 | `session.svelte.ts:71-85` + `MainLayout.svelte:192-195` 失败不切路由 | ✅ 已修复 |
| O4 | repository.rs 上帝对象 | 本次实测 **2569 行** | 设计取舍不修（可维护性项） |
| O5 | 同步路径外错误零留痕 | `state.rs:99 append_log`，调用点遍布各命令 | ✅ 已修复 |
| S1 | 帧 flags 不在 AEAD AAD | `keys.rs:22-27` AAD=record_type‖record_id；`frame.rs:41` flags 为裸头部字节 | ❌ **未修复**（本地篡改面，低危） |
| S2 | IPC 暴露无约束路径原语 | `keydoc_cmd.rs:70/117`、`import_cmd.rs:110` | 设计取舍不修（本地单用户；本次仍列为 P2 纵深建议，见 3.4-4） |
| S5 | 发现请求重放 | `discovery.rs:254/332-347` per-source+global 限速 | ✅ 已修复 |
| S6 | 配对会话 DoS | `server.rs:249-251` 已发 REJECT，但 `sync_cmd.rs:468-477` worker 收任意 Err 即作废会话 | ❌ **未修复**（低危；一次错令牌连接令用户重开二维码） |
| S7 | 主密码跨 IPC 不 zeroize | 审计报告已记录为中风险残余 | ✅ 已记录为接受风险 |
| C1 | pair 记录 qr.ip 而非连通 IP | `client.rs:385-387` 持久化真正 connect 成功的 IP | ✅ 已修复 |
| C2 | remote_deleted 制造重复回收站条目 | `repository.rs:1740-1751` 先查 trash 已有快照 | ✅ 已修复 |
| C3 | item_hash 含时序字段 | `repository.rs:2653-2655` 对整个 Item serde 后 hash（含 updated_at/modified_by/rev/use_count/pin_order） | ❌ **未修复**（两设备独立同改到相同内容仍误报冲突；单测靠同一对象同时间戳掩盖） |
| C4 | 解锁维护错误静默 | `session.rs:179-189` 收集进 maintenance_warnings + `vault_cmd.rs:44-46` 落日志 | ✅ 已修复 |
| C5 | 锁中毒 worker 静默退出 | `sync_cmd.rs:386-393/424-431`、`state.rs:220/265` append_log 后 break | ✅ 已修复 |
| C6 | 双重并发 unlock 文件级竞态 | unlock 只读库文件、互斥换入内存会话 | 设计取舍不修（不可利用） |
| C7 | unreachable! ×3 | `repository.rs:975/1037/1452`，均前置校验兜底 | 观察记录（3 处，无裸 panic 面） |
| Q1 | D-88/D-89 测试失败 | `d88-d90-ui-guard.test.ts` 7 例全绿 | ✅ 已修复 |
| Q2 | vitest 失败退出码 0 | 探针实测失败用例退出码=1 | ✅ 已修复 |
| Q3 | workspace cargo test 链接失败 | 实测仍 **exit 101**，但根因已变为 tauri-winres 的 `windres` 以 `0xc0000135`（缺 DLL）崩溃 | ❌ **未修复（仍失败，根因已变化）** |
| Q4 | SHA256SUMS 不一致 | 4/5 一致；散放 `release/passwordms.exe` 哈希不符（被后续构建覆盖） | 部分修复（仍 1 项不符；分发包全部一致） |
| Q5 | 核心模块缺测试 | `totp.ts` 仅 d97-guard 间接覆盖、`stores/session.svelte.ts` 无配套；另 strength/qr/constants/errors/settings store 无专测 | 部分修复 |
| M4 | `if let Ok` 吞锁错 | 关键路径已 `.map_err(lock_err)?`；残留仅 best-effort 清理路径（`vault_cmd.rs:71`、`sync_cmd.rs:461/472`） | 部分修复（其中 vault_cmd.rs:71 已单列为本次 P1，见 3.4-1） |
| 低优先级批 | empty_trash O(n²) / pending_imports 明文驻留 / remove_attachment 错误码 / ensure_resync_listener | ①`repository.rs:1635` 每条 purge 跑一次 gc_orphan_blobs，O(n²) 未优化；②放弃导入未锁定时预览驻留内存；③错误码透传 core；④`let _ = ensure_resync_listener` 实为 `?` 已传播未吞 | 按现状记录 |

### 4.2 D-96 Kimi 审计 12 项（2026-10-25）

| # | 问题 | 当前证据 | 结论 |
|---|---|---|---|
| 1 | 身份私钥 Base64 明文落盘 | `identity.rs:27-55` VK 包裹（magic `PMSIDENT1\0` + AAD + XChaCha20-Poly1305）+ 旧格式迁移 + Unix 0600 | ✅ 已修复 |
| 2 | 导入无大小/条数上限 | `import/mod.rs:83-89` 64MB 预检 + `:152-157` 10000 条 + `content_uri.rs:54-72` 限量读取 | ✅ 已修复 |
| 3 | 同步接收无总页/字节上限 | `paging.rs:116-132` 4096 页 / 200MB + 测试 | ✅ 已修复 |
| 4 | KDF 参数无上限 | `kdf.rs:21-32` 夹取 1GiB/16/8 + 边界测试 | ✅ 已修复 |
| 5 | 剪贴板无 Rust 兜底 | `system_cmd.rs:60-67` 30s 兜底线程 + `lib.rs:105-114` Exit 清理 | ✅ 已修复 |
| 6 | 条目明文无统一上限 | `repository.rs:1134-1143` 384KiB + 测试 | ✅ 已修复（同步路径未贯穿，见 3.1-6） |
| 7 | 帧截断不可检测 | `repository.rs:220-224` 头部计数交叉核对 → open_warning + `session.rs:176` | ✅ 已修复（仅 item_count，见 3.1-8） |
| 8 | 改主密码仅 flush | `container.rs:129-134` 原地分支补 `sync_all()` | ✅ 已修复 |
| 9 | 导入去重不随循环回填 | `import/mod.rs:213` 写入后回填 key + 测试 | ✅ 已修复 |
| 10 | create/update 不校验 folder_id | `repository.rs:823-832` validate_draft_folder（create/update 均调用） | ✅ 已修复（restore 路径未覆盖，见 3.1-3） |
| 11 | Android 无备份/截屏防护 | `AndroidManifest.xml:12-13` allowBackup=false + dataExtractionRules；`MainActivity.kt:14-17` FLAG_SECURE 于 super.onCreate 前 | ✅ 已修复 |
| 12 | vitest dev 链 CVE | `package.json:31` `vitest ^5.0.5`，实跑 `RUN v5.0.5` | ✅ 已修复 |

**历史核对汇总**：D-91 26 项——已修复 14 / 未修复 4（S1、S6、C3、Q3）/ 部分修复 3（Q4、Q5、M4）/ 设计取舍不修 3（O4、S2、C6）/ 观察记录 2。D-96 12 项——**全部已修复**，均有代码/配置 + 回归测试实证。

---

## 五、语言文案问题清单（含建议译文）

### 5.1 键完整性与占位符（机械化核对结果）

| 语言 | 叶子键数 | 相对 zh-CN 缺失 | 相对 zh-CN 多余 |
|---|---:|---:|---:|
| zh-CN | 376 | — | — |
| zh-TW | 376 | 0 | 0 |
| en-US | 376 | 0 | 0 |
| th | 376 | 0 | 0 |

- 占位符一致性：四语言间同一键的 `{n}/{name}/{size}/{ip}` 集合 **0 处不一致**。
- 调用点：243 个字面量 `ui.t()` 键全部存在；动态键（confirm/toast/message_key）人工核对均有对应键。
- 结论：**未发现「用户看到键名原样」或「英文回退」类功能性崩溃**。

### 5.2 问题清单

#### [P1] 5.2-1 硬编码 · WindowControls 窗口控制钮 aria/title 写死中文
- 位置：`src/components/common/WindowControls.svelte:35,40,45`
- 原文：`aria-label="最小化" title="最小化"` / `"最大化 / 还原"` / `"关闭"`
- 问题：未入语言包；en-US/th 用户悬停窗口按钮仍见中文 tooltip，屏幕阅读器读中文。现有 `i18n-usage.test.ts` 只扫 `ui.t('...')` 字面量，裸字符串不在守卫内。
- 建议译文：新增 `common.minimize` / `common.maximize_restore` / `common.close_window`：
  - zh-CN：`最小化` / `最大化 / 还原` / `关闭窗口`
  - zh-TW：`最小化` / `最大化 / 還原` / `關閉視窗`
  - en-US：`Minimize` / `Maximize / Restore` / `Close window`
  - th：`ย่อหน้าต่าง` / `ขยาย/คืนขนาดหน้าต่าง` / `ปิดหน้าต่าง`

#### [P1] 5.2-2 硬编码 · ItemEditor TOTP placeholder 写死中文
- 位置：`src/components/ItemEditor.svelte:209`
- 原文：`placeholder="BASE32 或 otpauth://"`
- 问题：en-US 用户看到中英混排「或」，th 用户同样看到中文字符。
- 建议译文：新增 `editor.totp_placeholder`：zh-CN `BASE32 或 otpauth://` / zh-TW `BASE32 或 otpauth://` / en-US `BASE32 or otpauth://` / th `BASE32 หรือ otpauth://`

#### [P1] 5.2-3 README 单附件上限写错（2 MB vs 实际 20 MB）
- 位置：`README.md:17`、`README.zh-CN.md:17`
- 原文：`(up to 16 per entry, ≤ 2 MB each)` / `（单条目最多 16 个，单附件 ≤ 2MB）`
- 问题：与代码不符——`src/lib/constants.ts:20` `MAX_ATTACHMENT_BYTES = 20 * 1024 * 1024` = **20 MB**；i18n `attach.too_large` 也是 `{n} MB`（n=20）。10 倍数值错误会误导用户拒收大附件。
- 建议译文：`(up to 16 per entry, ≤ 20 MB each)` / `（单条目最多 16 个，单附件 ≤ 20MB）`

#### [P2] 5.2-4 th · settings.auto_lock_failed 把「库里」误译为「ไลบรารี」
- 位置：`src/lib/i18n/th.ts:287`
- 原文：`อ่านไม่สำเร็จ: ที่แสดงอยู่คือค่าสำรอง ไม่สามารถอ่านค่าจริงจากไลบรารีได้`
- 问题：zh-CN「库里」指保险库内；泰文 `ไลบรารี` 指 library（代码库）/图书馆。同文件 `vault_file` 已统一译作 `ไฟล์ตู้เซฟ`。
- 建议译文：`อ่านไม่สำเร็จ: ค่าที่แสดงอยู่เป็นค่าสำรอง ไม่สามารถอ่านค่าจริงจากตู้เซฟได้`

#### [P2] 5.2-5 README-en 用 "Recycle bin" 与应用内 "Trash" 术语不一致
- 位置：`README.md:18`
- 原文：`**Recycle bin**: soft delete → restore → permanent purge; ...`
- 问题：应用内 en-US `trash` = `Trash`、`purge` = `Delete permanently`；README 用 Windows 用语 "Recycle bin" + "permanent purge"，用户从 README 进应用找不到对应项。
- 建议译文：`**Trash**: soft delete → restore → delete permanently; attachments are cleaned up alongside entries`

#### [P3] 5.2-6 zh-TW · 对象字面量格式（sync 闭花括号与 conflict 同行）
- 位置：`src/lib/i18n/zh-TW.ts:369`（`  },  conflict: {`）；其它三语言同位置均换行。Prettier 跑一次即可修。

#### [P3] 5.2-7 th · legal 文本夹未翻译英文 "telemetry" / "crash log"
- 位置：`src/lib/legal-content.ts:433`（TERMS_TH）
- 原文：`ซอฟต์แวร์ไม่เก็บข้อมูล telemetry บันทึกข้อผิดพลาด (crash log) หรือสถิติการใช้งาน`
- 建议译文：`ซอฟต์แวร์ไม่เก็บข้อมูลเทเลเมทรี บันทึกข้อขัดข้อง หรือสถิติการใช้งาน`

#### [P3] 5.2-8 th · legal "ผ่าน" 后多一个空格
- 位置：`legal-content.ts:310`（DISCLAIMER_TH）：`ผ่าน การตั้งค่า` → `ผ่านการตั้งค่า`

#### [P3] 5.2-9 th · 日期格式化未切佛历
- 位置：`src/lib/constants.ts:81`（`formatDateTime` 非 zh-CN 一律 MM/DD/YYYY 西元）
- 问题：th 下回收站/同步时间显示 2026 而非佛历 2569。建议 th 年份 +543，或至少用 DD/MM/YYYY。

#### [P3] 5.2-10 README-zh 用「永久清除」与应用内「彻底删除」不一致
- 位置：`README.zh-CN.md:18`；应用内 `zh-CN.ts:272-273` `purge: '彻底删除'`。
- 建议译文：`**回收站**：软删除 → 还原 → 彻底删除，附件随条目级联清理`

### 5.3 术语对照表（四语言一致性核查）

| 概念 | zh-CN | zh-TW | en-US | th | 备注 |
|---|---|---|---|---|---|
| 保险库 | 保险库 | 保險庫 | vault | ตู้เซฟ | ✅ |
| 分组 | 分组 | 群組 | folder | กลุ่ม | ✅ |
| 附件 | 附件 | 附件 | attachment | ไฟล์แนบ | ✅ |
| 回收站 | 回收站 | 資源回收筒 | Trash | ถังขยะ | ⚠️ README-en 用 "Recycle bin"（5.2-5） |
| 导出 / 导入 | 导出/导入 | 匯出/匯入 | export/import | ส่งออก/นำเข้า | ✅ |
| 配对 / 同步 | 配对/同步 | 配對/同步 | pairing/sync | จับคู่/ซิงค์ | ✅ |
| 密码体检 | 密码体检 | 密碼健檢 | Health check | ตรวจสอบรหัสผ่าน | ✅ th 已避开 สุขภาพ |
| 主密码 | 主密码 | 主密碼 | master password | รหัสผ่านหลัก | ✅ |
| 安全笔记 | 安全笔记 | 安全筆記 | secure note | โน้ตความปลอดภัย | ✅ |
| 剪贴板 | 剪贴板 | 剪貼簿 | clipboard | คลิปบอร์ด | ✅ |
| 自动锁定 | 自动锁定 | 自動鎖定 | auto-lock | ล็อกอัตโนมัติ | ✅ |
| 收藏 | 收藏 | 我的最愛 | favorite | รายการโปรด | ✅ |
| 条目 | 条目 | 項目 | item | รายการ | ✅ |
| 彻底删除 | 彻底删除 | 永久刪除 | Delete permanently | ลบถาวร | ⚠️ README-zh 用「永久清除」（5.2-10） |
| 会话码 | 核对码 | 核對碼 | pairing code | รหัสยืนยันการจับคู่ | ✅ |

敏感文案核查：「已复制到剪贴板」「密码已显示」「解锁失败」「主密码错误」「剪贴板已清空」等在四语言中均准确、专业，未发现泄露安全细节或语气不当（D-104/D-105 两轮审校覆盖扎实，本次未复现问题）。

### 5.4 语言维度小结

最该立刻修的是 **5.2-1 / 5.2-2 两处硬编码中文**（en-US/th 用户最直接可见的中文残留）与 **5.2-3 README 数值错误**。建议给 `i18n-usage.test.ts` 增加一条「组件源码不得出现 CJK 字面量」的守卫，覆盖裸字符串（现有守卫只扫 `ui.t('...')`）。

---

## 六、前后端契约一致性结论

- **60 条命令**逐条核对（`ipc.ts` vs `commands/*.rs`，含 snake_case 参数键）：**0 处不一致**。
- **DTO 结构**（`types.ts` vs `dto.rs`）逐字段核对：字段名与 snake_case 全部一致，`ItemDto.pinned/pin_order/use_count/last_used_at` 与后端在途字段对齐。
- **错误码映射**：`IpcError::from(CoreError)` 完整透传 code/message_key；`From<io::Error>` 走 kind 归一化；`From<serde_json::Error>` 归 Internal。
- **已知漂移点**：`ERR_SYNC_BUSY` 未进 core codes 表，由 `sync_cmd.rs:239-243` 硬编码构造（3.4-7，P3）——契约能跑通，但缺少编译期约束。
- **死代码**：`sync_accept_pair` 前后端均为 stub 且无调用方（3.5-7，P3）。
- **测试覆盖**：`ipc-contract.test.ts` 只校验命令名注册一致性；参数名逐字比对与 DTO 字段集为本次人工补齐，建议把这两项也纳入测试。

---

## 七、门禁实测结果

| 命令 | 结果 | 退出码 | 数字 |
|---|---|---:|---|
| `npm run test`（vitest） | ✅ PASS | 0 | **44 文件 / 341 用例全绿**（历史 D-88/D-89 用例现全绿）；Q2 探针验证失败用例退出码=1（历史「失败仍退 0」已修） |
| `npm run check`（svelte-check） | ✅ PASS | 0 | 0 error / 0 warning |
| `npm run check:css` | ✅ PASS | 0 | 0 FAIL / 6 WARN（6 条 WARN 为主按钮浅色 3.74:1 / 危险按钮 4.38:1，过控件底线 3.0、未达 AA 正文 4.5，已知接受项） |
| `cargo test -p passwordms-core` | ✅ PASS | 0 | **212 passed + 1 ignored**（历史 205 → 212；ignored 为需真实库的 `real_vault_ledger`） |
| `cargo test`（workspace） | ❌ **FAIL** | **101** | src-tauri build-script 阶段 tauri-winres 调 `windres` 以 `0xc0000135`（STATUS_DLL_NOT_FOUND，缺 DLL）崩溃；**根因已不是历史的「export ordinal too large」链接错误**，但 src-tauri 测试仍无法执行 |
| SHA256SUMS-1.2.7.txt | ⚠️ 4/5 | — | setup/msi/apk/zip 四个分发包与清单逐字节一致；唯散放 `release/passwordms.exe` 哈希不符（已被后续构建覆盖，别拿它当 1.2.7 真值） |

---

## 八、未覆盖项与验证说明

1. **动态/运行时验证**：本轮为静态审查 + 测试实测；`place_frame` 丢条目、掉电丢帧、锁中毒旁路等 IO 错误路径为代码级推演（证据充分但未做故障注入复现）。
2. **真机验证**：未做 Android 实机、Windows 真机 UI 走查；配对/同步端到端依赖既有 212 例 core 测试与集成测试覆盖。
3. **workspace cargo test**：本机因 windres 缺 DLL 无法执行，src-tauri 一层 Rust 测试等于不跑（Q3 历史项仍成立，根因已变）；建议 CI/前置环境补齐 mingw 运行时 DLL，或把门禁明确拆分为 `cargo test -p passwordms-core`（可信）+ src-tauri 白名单。
4. **Rust 依赖 CVE 库扫描**：本机无离线漏洞库，未完成 Rust 依赖公开漏洞扫描（与历史审计同口径）。
5. **覆盖范围**：本次覆盖 `第三节` 所列文件清单的代码与文案；`docs/` 全部文档、`scripts/`、`src-tauri/gen/android` 其余文件、release 构建链未逐行审查（仅抽查）；`App.svelte` 以外的 Svelte 样式/可访问性仅记录影响功能的项。
6. **与历史结论的关系**：本次独立复核了 D-91/D-96 全部历史项（见第四节），未发现「已修复项回归」；新增发现集中在近期新代码（O1 分页/三段式/identity.rs）、前端竞态与 i18n 硬编码层。

---

## 九、优先修复建议（行动清单）

| 优先级 | 项 | 对应发现 | 工作量 |
|---|---|---|---|
| P1-1 | `pending_imports` 锁中毒兜底清理（`into_inner()`/append_log），恢复 A-04 保证 | 3.4-1 | 小 |
| P1-2 | WindowControls 三按钮 + ItemEditor TOTP placeholder 入语言包（四语言） | 5.2-1 / 5.2-2 | 小 |
| P1-3 | README 双语附件上限 2MB → 20MB 更正 | 5.2-3 | 极小 |
| P2-1 | `vault.load()` 加请求序号守卫（含 recordItemUsage 路径），根治前端列表竞态 | 3.5-1 / 3.5-2 | 小 |
| P2-2 | session.rs 解锁失败统一折叠 UnlockFailed + 夹取失败补 burn-KDF + 假库回归测试 | 3.2-1 | 中 |
| P2-3 | client.rs 配对段②改 move 捕获 state，水位落盘 | 3.3-1 | 小 |
| P2-4 | `append_frame`/`pwrite_frame` 补 `sync_all`；`place_frame` 先 append 后墓碑 | 3.1-1 / 3.1-2 | 小 |
| P2-5 | 同步写入路径补大小校验（blob/条目）+ restore folder_id 校验 + tombstone 顺序 | 3.1-3 / 3.1-5 / 3.1-6 / 3.1-7 | 小-中 |
| P2-6 | 剪贴板清空失败 toast + TTL 钳制 + `get_status` 失败不误入创建向导 | 3.5-3 / 3.5-4 / 3.5-5 | 小 |
| P2-7 | Rust 侧 auto-lock 兜底线程；`copy_secret` ttl_secs 生效；resync 锁不再跨网络持有 | 3.4-2 / 3.4-5 / 3.4-6 | 中 |
| P2-8 | th `auto_lock_failed` 误译修正 + README 术语对齐（Trash / 彻底删除） | 5.2-4 / 5.2-5 / 5.2-10 | 极小 |
| P3 批 | 错误码细化（3.1-10）、`ERR_SYNC_BUSY` 入 core、`sync_accept_pair` 清理、read_blob 返回 Zeroizing、CSP/capabilities 收窄、random_port 失败报错、identity 注释、import clone、端口文件竞态、C3 item_hash 排除时序字段（历史遗留）、S6 错令牌不作废会话（历史遗留）、S1 flags 进 AAD（格式 v2） | 见各节 | 分批 |
| 环境 | 修复 workspace `cargo test`（windres 缺 DLL）或拆分门禁口径 | Q3 | 环境 |

---

> 本报告由 6 路并行审查（Rust 核心 ×3 / Tauri 后端 / 前端 / i18n 文案）+ 独立门禁实测汇总而成，全部结论均有 文件:行号 + 代码原文 证据；关键数字经二次复核（如 repository.rs 行数实测 2569、MAX_ATTACHMENT_BYTES=20MB）。本报告仅审查，未改动任何代码。
