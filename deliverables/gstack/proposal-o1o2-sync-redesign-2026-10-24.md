# O1 / O2 同步链路改造方案（下轮实施审批稿）

**日期**：2026-10-24
**背景**：D-91 全仓审查 🟠 级发现 O1（大库同步永久失败）与 O2（同步期间 UI 冻结）。本轮只出方案，经主人审批后实施。
**关联代码**：`crates/passwordms-core/src/store/repository.rs`（export_changes）、`sync/protocol.rs`、`sync/client.rs` / `server.rs` / `resync.rs`、`sync_cmd.rs`、`src-tauri/src/state.rs`

---

## O1：同步载荷分页 + peer-base 过滤

### 问题回顾

`export_changes`（repository.rs ~L2229）遍历**全部** live 条目塞进单个 SyncPayload，`cursor` 恒为 0；transport 单帧上限 `SYNC_MAX_FRAME_BYTES ≈ 1.06MB`（内层 JSON+b64 后明文预算 ~800KB）。2~3 条 256KB 大笔记或上千条常规条目即超限；且因每轮全量导出，`base` 永远无法推进 → **同步从此每轮失败，永久不可恢复**。200MB 库容量与 1MB 帧上限差 200 倍，无分页是结构性缺口。

### 方案（三件套，全部向后兼容）

1. **发送侧水位过滤（砍掉 95% 载荷）**：
   - `DeviceSyncState` 增加**发送水位**：`sent_watermark: HashMap<record_id, rev>`（每对端一份，复用现有 `RECORD_TYPE_SYNC_STATE` 持久化通道，新增字段 serde default 兼容老文件）。
   - `export_changes(peer)` 只导出 `rev > sent_watermark[record]` 的条目。发送**成功后**推进水位（与 `note_sent_tombstones` 同一失败安全语义：中途失败不落盘，下轮重发，宁多勿丢）。
   - 首次配对（水位为空）= 现状的「全量首次推送」，行为不变。
2. **cursor 分页（单轮装不下的部分分批）**：
   - `SyncPayload.cursor`（协议已留字段，当前恒 0）启用：导出时按 `(record_type, record_id)` 排序后按序填充，装到预算上限即截断，`cursor` 记录断点。
   - 交换循环改造：Done 判定改为「`cursor == 0`（无更多页）才 Done」，否则继续下一轮 Push（对端 **merge 语义零改动**——每页都是幂等的 ChangeRecord 流）。
   - 兼容性：老对端不认 cursor？——**不需要认**。分页由发送方驱动（发送方把多页当多次 Push 发），接收方 merge 每页独立幂等，报文形状不变；`blob_support` 同款 serde default 兼容模式复用。
3. **帧预算预检（杜绝超限）**：
   - 导出时对每条 ChangeRecord 估算序列化大小（明文长度 × 4/3 + 固定开销），累计逼近 `SYNC_MAX_FRAME_BYTES - 64KB` 安全余量即截断。不再「装完才发现超限」。
   - 单条即超预算的条目（极端大笔记）：直接报 `Validation("change_too_large")` 并在 UI 给出**指名道姓**的错误（哪一条装不下），而不是笼统同步失败——256KB 笔记 × b64 ≈ 340KB < 800KB 预算，正常内容永远到不了这一步。

### 验收标准

- 既有 `sync_deletion.rs` / `sync_conflict.rs` / `sync_blob.rs` / `sync_replay.rs` 全绿（零回归）。
- 新增 `tests/sync_pagination.rs`：① 1500 条小条目分多页全部到位 ② 页间中断重同步不丢不重 ③ 老格式 payload（cursor=0 单页）与新版互通 ④ 水位推进后第二轮导出为空 ⑤ 发送失败水位不推进。
- 实测主人库（24MB 级）连续三轮同步，第二轮起导出条数为 0。

## O2：同步「三段式」去持锁化

### 问题回顾

resync worker（state.rs ~L211）与 pairing worker（sync_cmd.rs ~L350）**持全局 session Mutex 横跨整个网络交换**（握手 30s/片读超时 + blob 分钟级），而同步命令又在主线程执行（D-48 注释自证）→ 任何拿到 session 锁的 IPC（列表/复制/自动锁定）在同步期间排队数十秒到分钟级，UI 整体冻结。

### 方案：锁内取快照 → 锁外传输 → 锁内合并

1. **段一（锁内，毫秒级）**：取 session 锁，校验已解锁；导出待发变更（O1 的分页导出）+ 快照对端需要的元数据；克隆必要上下文（vault 路径、device_id、sync_key 引用计数或密文材料）；**立即释放锁**。
2. **段二（锁外，秒~分钟级）**：全部网络 I/O（握手、Push/Pull、blob 传输）在锁外完成。数据落到内存暂存（接收的 ChangeRecord 流）与既有 spool（blob 密文，本就落盘）。
3. **段三（锁内，毫秒级）**：重新取锁；**会话仍解锁且 vault 未变**（校验 vault 指纹/世代号，不匹配则整段丢弃、下轮重来——与现状「失败安全重发」同语义）→ `apply_changes` 合并 + `pull_missing` 的 spool finalize 入库 + 推进水位/base + `save_sync_state` + emit `sync://applied`。

### 必须解决的两个交互

- **自动锁定竞态**：段二期间用户锁定 → 段三发现会话 Locked → **静默放弃本次结果**（密文暂存随线程丢弃，无明文残留），对端下轮重发。宁可白传，不可在锁定态写库。
- **本地编辑并发**：段二期间用户改了条目 → 段三 `apply_changes` 本就是逐条 LWW + detect_conflict，天然处理并发（比现状更优：现状是锁死用户不让改）。

### 迁移顺序（降低风险）

1. 先做 O1（纯增量，零锁语义改动，独立可交付可回滚）。
2. O2 第一步只动 **resync worker**（被动路径，风险面小）；pairing worker 与 `sync_join_impl` 第二步跟进。
3. 全程保留既有锁序铁律（resync → session）注释与断言。

### 验收标准

- 同步进行中（传 20MB 附件），电脑端列表/复制/锁定 IPC 响应 < 100ms（现状：冻结数十秒）。
- 段二期间锁定 → 无明文残留（`zeroize_check` 套件扩展一例）。
- 段二期间本地编辑同条目 → 冲突箱正确出件。
- 既有全部同步测试 + `timing_sidechannel` 全绿。

---

## 工作量与风险

| 项 | 规模 | 主要风险 | 缓解 |
|----|------|---------|------|
| O1 | 中（core 导出/交换循环 + 水位，1 个新测试文件） | 水位语义与既有 note_sent_tombstones 的一致性 | 失败安全同款语义；互通性用例锁死 |
| O2 | 大（worker 结构重排） | 段三会话态校验遗漏导致在错误 vault 上写库 | vault 世代号校验 + 失败即弃 + zeroize 回归 |

**建议排期**：O1 先行单独交付回归 → O2 分两步。两步都完成后，Review 🟡-5（`empty_trash` O(n²)）等顺带批次再排。
