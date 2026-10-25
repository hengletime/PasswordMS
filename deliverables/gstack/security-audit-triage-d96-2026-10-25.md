# Kimi 安全审计复核与修复（D-96）

**日期**：2026-10-25
**场景**：安全审计复核 + 全流程修复交付
**参与成员**：安全卫士（逐项对码复核）+ 实施工程师×2（Rust 侧 / Android+依赖侧）+ 主理人（独立复跑门禁）
**输入**：`docs/PasswordMS-security-audit-2026-10-25.md`（Kimi 静态审计，整体评级中）

---

## 📌 TL;DR

- 整体结论：🟢 Kimi 报告质量高，**无 outright 误报**；12 项成立修复已全部实施并验证
- 判定分布：成立 12 / 部分成立 3 / 设计取舍不改 3
- 门禁：cargo check 0 错 0 警 · cargo test 205/0 · npm test 300/300 · svelte-check 0/0 · npm audit 0 漏洞
- 下一步：主人 git 提交 → 出包 v1.2.19 拍板

---

## 🎯 核心结论卡片

| 项目 | 内容 |
|------|------|
| Go / No-Go | 🟢 Go（已验证全绿） |
| 严重度分布 | 修复：中×6 / 低-中×4 / 低×2；不改：设计取舍×3 |
| 关键行动项 | 12 项已全部落地 |
| 遗留拍板 | 无（四项拍板主人已全选 A） |

---

## ✅ 已修复（12 项）

### Rust 侧（10 项）

| # | 问题 | 修法 | 位置 |
|---|---|---|---|
| 1 | 🔴身份私钥 Base64 明文落盘 | VK 包裹 XChaCha20-Poly1305（magic `PMSIDENT1`+AAD `b"identity"`+原子落盘+Unix 600+旧格式自动迁移）；删死字段 host_identity | `sync/identity.rs`（新）、`sync_cmd.rs`、`state.rs` |
| 2 | 导入无大小/条目数上限 | 64MB metadata 预检 + 10000 条上限 + Android `read_to_bytes_limited`（附件 20MB 语义不变） | `import/mod.rs`、`import_cmd.rs`、`content_uri.rs` |
| 3 | 同步接收无总页数/字节上限 | 4096 页 / 200MB 累计夹取（协议兼容） | `paging.rs` |
| 4 | KDF 参数无上限（本地 DoS） | 夹取 1GiB / 16 次 / 8 并行（存量库零影响） | `kdf.rs`、`constants.rs` |
| 5 | 剪贴板清空依赖前端计时器+退出不清理 | Rust 兜底线程 30s + `RunEvent::Exit` 清理（前端倒计时保留） | `system_cmd.rs`、`lib.rs` |
| 6 | 条目无统一大小上限（本地能建同步不了） | 明文 ≤384KiB（< paging 单条阈值 ~496KiB，换算注释已写） | `repository.rs` |
| 7 | 库文件帧边界截断不可检测 | open 时头部计数交叉核对 → maintenance_warnings（不阻断） | `repository.rs`、`session.rs` |
| 8 | 改主密码头部原地重写仅 flush（掉电=库损坏） | 原地分支补 `sync_all` | `container.rs` |
| 9 | 导入去重集合不随循环回填 | 导入成功后回填 key（同文件重复只导首条） | `import/mod.rs` |
| 10 | create/update 不校验 folder_id 存在性 | 对齐 move 流程校验 | `repository.rs` |

### Android + 依赖侧（2 项）

| # | 问题 | 修法 |
|---|---|---|
| 11 | 无备份/截屏防护 | `allowBackup=false` + data_extraction_rules/full_backup_content 全禁 + MainActivity FLAG_SECURE（super.onCreate 前） |
| 12 | vitest dev 链 2 critical + 1 moderate | vitest 3.2.7→5.0.5（breaking 零适配），npm audit 3→0 |

**测试**：新增 `tests/d96_guards.rs` 8 例集成 + 模块内单测 13 例（重点：双格式迁移幂等、接收超限拒绝、KDF 边界值、条目超限报错、同文件去重、无效 folder 拒绝）。
**错误码**：0 新增（全复用 `ERR_VALIDATION`，消息负载区分），前端零改动、IPC 契约 59/59 不变。
**文档**：`docs/02-arch.md` 新增 §3.20.1~3.20.10。

---

## ❌ 判「不改」（3 项，含理由）

1. **墓碑密文残留**（Kimi 定性「编码隐患」→ 复核改判「设计取舍」）：墓碑是删除语义跨端传播的唯一机制，移除会导致已删数据被对端陈旧 live 变更复活；残留为 AEAD 密文，物理清除由压缩（D-76 tmp→sync_all→rename）兜底。
2. **解锁期明文内存**：密码要显示在屏幕上是产品固有限制；Rust 侧已有 lock 时 wipe、Zeroizing、级联 Drop，JS 引擎无法保证擦除。
3. **文件权限全面收紧（Windows ACL）**：`%APPDATA%` 默认 ACL 已限 用户+SYSTEM+Admins，便携 FAT/exFAT 无 ACL 可设；identity 的真实加固已并入修复 #1（Unix 600）。

## ⚠️ 复核对 Kimi 报告的三处定性修正

1. §二-2 墓碑残留不是编码隐患（见上）。
2. §四 明文内存「并非完全没有清理逻辑」——Kimi 自己已承认，属固有限制非可修复项。
3. §十一-1 「静默吞尾部」窗口比描述小：帧体截断本就硬报错（`frame.rs truncated frame`），真实残余风险仅 <124B 尾料与帧边界精确截断两种，且已由修复 #7 的计数交叉核对覆盖报警。

## 📚 成员产出索引

- gstack-security-officer：逐项复核（17 项判定 + 汇总表，本报告结论的事实基础）
- gstack-fixer（Rust 侧实施）：10 项修复 + cargo 门禁
- pms-android-vitest（Android+依赖实施）：2 项修复 + vitest 迁移核查
- 主理人独立复核：cargo check/test、npm test/check、注册面与 identity.rs 抽查

---

> 本报告由软件工坊 AI 协作生成，关键决策已由工程负责人（主人）拍板。
