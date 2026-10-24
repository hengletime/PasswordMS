# PasswordMS 全仓审查报告

**日期**：2026-10-24
**场景**：全仓审查（安全审计 + 正确性排查 + 测试门禁 + 代码评审，四成员并行，全程只读未改码）
**参与成员**：安全卫士（gstack-security-officer）+ 排障手（gstack-investigator）+ 质量门神（gstack-qa-lead）+ 评审官（gstack-product-reviewer）
**版本基线**：package.json 1.2.14

---

## 📌 TL;DR（执行摘要）

- 整体结论：**🟡 有条件通过** —— 架构纪律与密码学实现质量显著高于同类独立项目（零知识加密闭环核验全过、IPC 契约 0 例外、blob 引用不变量成立），但存在 **2 个 🔴**（同步 blob 路径穿越可任意写文件；回收站还原可静默覆盖同步数据）与 **3 个 🟠**（大库同步永久失败、同步持锁冻 UI、lockNow 谎报已锁），**不修复 🔴 不建议发版**。
- 阻塞项数量：🔴 2 / 🟠 5 / 🟡 约 14 / 🟢 若干
- 下一步：主人审批修复清单 → 按 P0（2🔴）→ P1（3🟠 + 门禁修复）→ P2（🟡 分批）顺序执行。

---

## 🎯 核心结论卡片

| 项目 | 内容 |
|------|------|
| Go / No-Go | 🟡 条件 Go：修完 2 个 🔴 + 1 个 🟠-1（同步分页）后可发版；🟠-2/3 可随版 |
| 严重度分布 | 🔴 2 / 🟠 5 / 🟡 14 / 🟢 8 |
| 关键行动项 | 8 条（见行动清单） |
| 建议负责人 | 主人拍板范围；实现按排箱团队分工（实现 → QA 回归 → 设计走查） |

---

## 1. 各成员核心结论

### 🛡️ 安全卫士（OWASP + STRIDE 审计）
- 核心判断：**🟡 有条件发布，综合评级 B**。密码学设计与实现质量显著高于同类独立项目：Argon2id（64MiB/t3）达 OWASP 推荐档、XChaCha20-Poly1305 逐条独立密钥 + AAD 绑定防密文移植、常量时间比较 + 解锁时序均衡、密钥全程 Zeroizing + 锁定时 wipe_plaintext 全覆盖、同步配对（一次性令牌 + 临时 X25519 + transcript 绑定 + 重放防护）与发现协议隐私设计全部到位。依赖树健康无高危 CVE。
- 关键建议：修复 **F-001**（blob spool 路径穿越，改动小收益大，P0）；**F-002**（flags 不在 AAD，破坏零知识完整性承诺）随下个格式版本修复；F-003~F-007 进 backlog。

### 🔧 排障手（正确性与逻辑闭环）
- 核心判断：六项项目铁律全部核验**通过**（blob 引用集合不变量 4 处消费方一致、同步跳 blob 帧链路成立、59 处 command 全带 snake_case 0 例外、Svelte 5 无 structuredClone 违例、被动同步 emit 对称、错误反馈基本闭环）。但发现 **1 个 🔴 丢数据路径**和 3 个 🟠（大库同步结构性缺口、同步持全局锁、lockNow 谎报），另有 6 个 🟡 边界缺陷。
- 关键建议：修复顺序 🟠-1（分页 + base 过滤）→ 🔴-1（restore 守卫）→ 🟠-2（锁粒度）→ 🟠-3 → 🟡-1/2。

### ✅ 质量门神（测试与门禁实测）
- 核心判断：**🟡 有瑕疵，不可直接放行**。svelte-check 0/0、CSS 契约 0 FAIL、passwordms-core 169 测试全绿（含 zeroize/timing/sync_replay 等对抗套件）均 ✅；但前端 **2 个 UI-guard 用例失败**（D-88/D-89）、**vitest 失败时退出码仍为 0**（门禁失效隐患）、workspace 级 cargo test 因 mingw 链接 `export ordinal too large` 无法执行（src-tauri 侧 Rust 门禁形同虚设）、1.2.7 清单与磁盘 exe 不一致、7 个核心前端模块无测试（含 totp.ts / session.svelte.ts 两处安全敏感面）。
- 关键建议：P0 修 D-88/D-89 + vitest fail-on-error；P1 解决 src-tauri 链接问题；P2 补安全敏感模块测试、统一交付物归档路径。

### 🔍 评审官（架构与代码评审）
- 核心判断：**纪律性极强的代码库，无 🔴 级评审问题**。分层严格成立（core 零 Tauri 依赖已逐条核对）、src-tauri 全层 0 处 unwrap/expect/panic、docs 与代码同步度高（决策日志至 D-87、门禁数字与 Cargo.toml 一致）、主流程走查每步均有实现与错误反馈闭环。
- 关键建议：拆解 repository.rs（2570 行上帝对象，最大可维护性杠杆）；建立统一错误码落盘日志消灭静默 catch；3 处 `unreachable!` 换显式错误返回（release panic=abort 下不变量漂移 = 整个 app 直接死）。

---

## 2. 综合审查发现（去重合并，按严重度排序）

### 🔴 阻塞级（发版前必须修）

| # | 位置 | 问题 | 建议 |
|---|------|------|------|
| R1 | `sync/blob_transfer.rs` Spool::open_or_create(~L223) / write_chunk(~L275)，入口 pull_missing(~L538) | **blob spool 路径穿越**：对端 Meta 报文的 `blob_ref` 为自由 String 无格式校验，直接 `root.join(&blob_ref)` 后 create_dir_all + 落盘对端可控字节（≤768KiB/片）。绝对路径整体替换 root、`..\..\` 穿越。已配对设备被攻陷或 sync_key 泄露的 LAN 攻击者可在宿主机任意路径写文件（潜在 RCE） | 在 `BlobMeta::from_message` 强制 blob_ref 为规范 UUID（复用 record_id_to_bytes 校验后回写规范化形式），或 join 后校验 `starts_with(spool_root)` + 拒绝绝对路径/`..` 分量；`cleanup_orphan_spools` 同口径。改动小、收益大 |
| R2 | `store/repository.rs:1500-1521` restore_item + `merge.rs:156-176` 复活路径 + `Trash.svelte:30` | **回收站还原可静默覆盖同步进来的新数据（丢数据）**：对端删除后又编辑（rev 超墓碑 rev）时本端复活条目，但旧 TrashEntry 不清除 → 同 id 同时存在 live 新版本 + trash 旧快照。用户点「还原」：旧快照 rev+1 + updated_at=now 直接覆盖 live 帧，LWW 赢家，覆盖还传播回对端，**两端都丢最新内容** | restore_item 前检查 `self.items.contains_key(&item_id)`——已存在则走冲突箱（或拒绝+提示）；或还原时对同 id live 条目做 detect_conflict |

### 🟠 高优先级

| # | 位置 | 问题 | 建议 |
|---|------|------|------|
| O1 | `repository.rs:2229-2268` export_changes + transport.rs:46 | **大库同步必然永久失败**：每轮把全部 live 条目密文塞单帧（≈1.06MB 上限，内层 JSON+b64 预算仅 ~800KB），2~3 条 256KB 大笔记或上千条常规条目即超限；live 无 peer-base 过滤、cursor 恒 0 → base 无法推进、payload 永不变小，同步从此每轮失败且用户只见笼统错误。200MB 库容量 vs 1MB 帧上限差 200 倍 | 按对端 base 过滤 live 变更 + 实现 cursor 分页（协议已留字段）+ 发送前按帧上限预算切批 |
| O2 | `state.rs:211-235` resync worker、`sync_cmd.rs:350-375/116-147` + D-48 主线程执行 | **同步期间 UI 整体冻结**：网络 I/O（握手 30s 超时/片、blob 分钟级）全程持全局 session Mutex，同步命令又在主线程执行 → 被动同步/配对传 20MB 附件时，电脑端一切 IPC（含自动锁定）阻塞数十秒到分钟级（安全官 F-005 同源，合并） | 「锁内取快照 → 锁外传输 → 锁内合并」三段式；至少把 recv/send 粒度拆锁；同步命令移出主线程 |
| O3 | `MainLayout.svelte:158-164` + `stores/session.svelte.ts:68-80` | **lockNow 谎报已锁**：后端 lock 失败（如锁中毒）只 toast，前端无条件 reset store + 切锁屏 → 界面显示锁屏但后端会话仍 Unlocked、明文仍在内存。自动锁定路径同样中招 | session.lock 返回布尔；失败不 reset 不切路由，给可见错误并保持主界面 |
| O4 | `store/repository.rs`（2570 行） | **上帝对象**：条目/分组/附件/blob/回收站/冲突箱/同步直写/压缩/迁移全在一个类型，数据完整性风险集中于最难安全修改的文件 | 按领域拆子模块（items/folders/attachments/trash/conflicts/blob_store）保留 facade |
| O5 | append_log 仅 sync_cmd/system_cmd/state 使用；`MainLayout.svelte:111` 空 catch；vault_cmd/state.rs 多处 `let _ =` | **同步路径之外错误零留痕**：IPC 错误只回传前端且存在静默吞错点，零知识约束下错误码日志安全但目前缺失 | 统一命令包装层记 `err.code`；清理静默 catch 点 |

### 🟡 中优先级（按主题归并）

**安全面**
| # | 位置 | 问题 | 建议 |
|---|------|------|------|
| S1 | `store/frame.rs` encode/decode + repository.rs tombstone_changes(~L2281) | 帧的 flags（含 FLAG_TOMBSTONE）不在 AEAD AAD 内 → 持 vault 文件写权限者可无密钥翻转成墓碑，删除经 merge 传播到全部对端，破坏零知识完整性承诺 | flags 纳入 AAD（格式版本 v2 迁移），或墓碑导出时校验密文内嵌 rev 与本地记账一致 |
| S2 | `sync_cmd.rs` add_attachment/export_attachment、import_cmd.rs save_import_template | IPC 命令面暴露无约束路径原语（纵深防御；正常流程路径来自系统对话框，CSP 严格，实际风险低） | 对话框路径白名单/禁系统目录；save_import_template 加解锁校验 |
| S3 | `repository.rs plan_compaction` | 「彻底删除」的死亡快照密文以墓碑形式无限期留存（取证残留） | UI 明示语义或提供解绑后清墓碑 + 压缩的擦除入口 |
| S4 | `sync_cmd.rs:356-362`、`state.rs` | 配对/重同步期间持 session 锁做阻塞网络 IO（与 O2 同源，修 O2 一并解决） | 同 O2 |
| S5 | `discovery.rs` | 发现请求可重放 → 存在性与端口探测（已有限速缓解） | 应答方对 q 加 LRU 去重 |
| S6 | `sync_cmd.rs spawn_pairing_worker` | 任意 LAN 连接一次垃圾握手即终止整个配对会话（DoS，可用性） | 令牌校验失败仅 REJECT 不作废会话 |
| S7 | `vault_cmd.rs` master_password: String | 主密码明文跨 IPC、Rust 侧不 zeroize（Tauri 架构固有，session.rs 内已正确处理；建议文档记录为已接受风险） | 安全文档 A-04 补充记录 |

**正确性/数据面**
| # | 位置 | 问题 | 建议 |
|---|------|------|------|
| C1 | `client.rs:218-242 vs :311` | pair() 记录 qr.ip 而非实际连通的候选 IP（D-44 回归）→ 多网卡配对后首次 sync_now 必失败一次 | 记录 connect 成功的那个 ip |
| C2 | `repository.rs:1589-1596` | resolve_conflict「采用远端(删除)」可制造重复回收站条目，还原其一即触发 R2 | remote_deleted 且回收站已有同 id 条目时跳过软删只标记 resolved |
| C3 | `repository.rs:2486-2488` | item_hash 含 updated_at/modified_by → 同内容双端修改产生伪冲突 | hash 排除时序字段 |
| C4 | `session.rs:166-170` | 解锁期 migrate/purge/gc 三项维护错误全静默吞掉（连 append_log 都没调） | 至少落 append_log |
| C5 | `state.rs:212-233`、`sync_cmd.rs:342-360` | 锁中毒使被动同步 worker 永久静默退出，直至重启，无用户提示 | 中毒时重建锁或日志/UI 告警 |
| C6 | `vault_cmd.rs:35-49` | 双重并发 unlock 的文件级竞态（KDF 期间不持 session 锁做文件写入，帧交错损坏风险） | unlock 的文件维护移入 session 锁内或加文件锁 |
| C7 | `repository.rs:914/976/1370` | 3 处 `unreachable!`（release panic=abort 下不变量漂移 = app 直接死，与命令层零 panic 纪律矛盾） | 改返回 CoreError::Internal |

**质量门禁面**
| # | 位置 | 问题 | 建议 |
|---|------|------|------|
| Q1 | `src/lib/d88-d90-ui-guard.test.ts` | 前端 2 用例失败：D-89 登录项 ed-notes textarea 未绑定 notes；D-88 convertToNote 按钮位置断言不满足 | 修 UI 或修断言（先确认哪个是本意） |
| Q2 | vitest 配置 | 失败时退出码仍为 0 → 门禁失效隐患 | 开 fail-on-error |
| Q3 | workspace cargo test | src-tauri mingw 链接失败（export ordinal too large: 124147）→ src-tauri 侧 Rust 测试永远无法执行 | 换 MSVC 工具链或链接器参数调优 |
| Q4 | `SHA256SUMS-1.2.7.txt` | 磁盘 exe（10-24 10:11 重建）与清单不一致；且清单指向 target/ 而 release/ 无 1.2.7 交付物，归档路径不统一 | 重建 1.2.7 产物 + 重签清单 + 归档到 release/ |
| Q5 | src/lib | 7 个核心模块无测试：constants/errors/qr/**totp**/dnd.svelte/**stores/session**/stores/settings（后两处安全敏感）；src-tauri 无 Rust 测试（IPC 仅前端契约 19 用例） | P2 优先补 totp.ts 与 session.svelte.ts |

**可维护性面**
| # | 位置 | 问题 | 建议 |
|---|------|------|------|
| M1 | MainLayout 788 行 / Settings 705 / FolderSidebar 544 / ItemList 528 | 主组件过胖（lib/ 抽取文化好，但主组件在堆积） | 比照 sync/ 子目录做法拆分 |
| M2 | i18n-usage.test.ts | 只校验字面量键，动态键与裸中文不在网内（当前实测无硬编码中文漏网） | 扩展守卫范围 |
| M3 | `Repository::blob_chunks()` 等 | 疑似死代码/仅测试消费 | 核实后删除或标注 |
| M4 | `sync_cmd.rs:391/405`、`vault_cmd.rs:66` | `if let Ok(...)` 静默跳过锁失败（debug 与 release 行为不一致） | 统一 map_err(lock_err) |

### 🟢 低优先级
- empty_trash O(n²)（purge_item 每条跑一次 gc）；pending_imports 明文预览放弃导入后驻留至下次锁定；remove_attachment 错误码语义失真；浅色主题 6 处按钮对比度 3.74~4.38:1（过 3.0 控件底线未达 AA 4.5）；`let _ = ensure_resync_listener` 冗余。

---

## 3. 六项项目铁律核验（排障手实测）

| 铁律 | 结果 |
|------|------|
| blob 引用集合不变量（4 处消费方） | ✅ 成立（含冲突箱双快照、resolved==false 口径自洽） |
| 同步跳 blob 帧 / 压缩后大小差异不影响同步 | ✅ 成立 |
| 59 处 tauri::command 全带 rename_all="snake_case" | ✅ 0 例外，ipc.ts wire 键逐字一致 |
| Svelte 5 禁 structuredClone 于 $state | ✅ 无违例（均已 $state.snapshot） |
| 被动同步落库后 emit 通知对称 | ✅ state.rs:254 → sync.svelte.ts:318 |
| 失败不伪装空状态 | ✅ 基本闭环（后端静默点见 C4/C5/O5） |

## 4. 测试与门禁实测数据（质量门神）

- `npm run test`：280 用例，278 过 / **2 失败**（D-88/D-89）❌
- `npm run check`：0 error / 0 warning ✅
- `npm run check:css`：0 FAIL / 6 WARN（浅色对比度）✅
- `cargo test -p passwordms-core`：**169 passed / 0 failed** / 1 ignored ✅（27 套件）
- `cargo test`（workspace）：❌ src-tauri 链接失败，0 个 src-tauri 测试被执行
- SHA256SUMS-1.2.7.txt：4 OK / **1 FAILED**（exe 磁盘文件晚于清单重建）

---

## ✅ 行动清单（待主人审批，未动一行代码）

| # | 行动 | 对应发现 | 紧急度 | 说明 |
|---|------|---------|--------|------|
| 1 | blob_ref 规范化 UUID 校验（from_message + cleanup_orphan_spools 双口径） | R1 | **P0** | 改动小收益大，防任意路径写 |
| 2 | restore_item 加 live 存在性守卫 → 走冲突箱 | R2 + C2 | **P0** | 防丢数据 |
| 3 | export_changes 按 peer-base 过滤 + cursor 分页 + 帧预算切批 | O1 | P1 | 大库同步结构性修复，工作量最大 |
| 4 | lockNow 失败不 reset 不切路由 | O3 | P1 | 小改动，安全语义修正 |
| 5 | 同步网络 I/O 移出 session 锁（三段式） | O2/S4 | P1 | UI 冻结根治 |
| 6 | 修 D-88/D-89 + vitest fail-on-error | Q1/Q2 | P1 | 恢复门禁可信 |
| 7 | pair 记录实际连通 IP + 解锁维护错误落日志 + 锁中毒告警 | C1/C4/C5 | P2 | 小修打包一批 |
| 8 | flags 纳入 AAD（格式 v2）/ unreachable! 换错误 / repository.rs 拆分 / 补 totp+session 测试 | S1/C7/O4/Q5 | P2/Backlog | 大项，逐个排期 |

---

## ⚠️ 待完善 / 已知局限

- 本轮为纯静态审查 + 测试实测，**未做真机动态复现**（R1 攻击链为代码级完整推演，置信度 8/10）。
- 锁序与死代码结论基于静态分析，未经运行时验证。
- 工作树冻结说明：QA 门禁期间无并行落盘，读数可信（278+2=280 用例为全量）。
- D-88/D-89 的「修 UI 还是修断言」需主人拍板本意。

## 📚 成员产出索引

- gstack-security-officer：完整审计报告（14 阶段口径，Critical 1 / High 0 / Medium 1 / Low 4 / Info 6，评级 B）
- gstack-investigator：六项铁律核验 + 🔴1/🟠3/🟡6/🟢4 分级清单 + gc/压缩/同步时序专项
- gstack-qa-lead：四道门禁实测 + 产物校验 + 覆盖面统计（17m23s）
- gstack-product-reviewer：全仓评审（🔴0/🟠2/🟡6/🟢3）+ 主流程闭环走查 + Top 3 建议

---

> 本报告由软件工坊 AI 协作生成，关键决策请由工程负责人复核。
