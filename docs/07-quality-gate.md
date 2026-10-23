# 07 · 质量门与出包门禁（权威）

> **本文档的性质**：PasswordMS 交付前的**唯一门禁权威**。§1 日常门 = 一条命令跑完的自动化总门；§2 出包门 = 产出安装包/APK 的**唯一正道**与出包后的人工验收；§3 安全验收点 A-01~A-10 与测试证据的映射；§4 已知边界（**未验证 ≠ 通过**）。
> 所有数字均来自 `bash scripts/gate-all.sh` **完整实跑**，不接受口头计数。**§1.2 / §1.3 已按 2026-10-22 22:35（v1.2.11 + Q-2 + #530-clippy清零，**8 门**）实跑刷新**（总判定 **PASS**，8 门全过 / 0 失败 / 0 跳过；Rust **148 例通过 + 1 例 `#[ignore]`**、前端 **26 文件 204 例**；各门耗时 13s / 14s / 4s / 2s / 400s / 4s / 30s / 15s；门禁期间 `find … -newermt 22:35:39` **空输出** = 零改动）；2026-10-22 16:19（v1.2.11 + Q-2，clippy 尚为 7 条，148 例 / 204 例，12s / 11s / 4s / 2s / 325s / 12s / 30s / 15s）；2026-10-22 14:10（v1.2.11，**7 门**——Q-2 尚未纳入，148 例 / 204 例，12s / 11s / 4s / 341s / 12s / 28s / 13s）；2026-10-22 12:51（v1.2.10，145 例 / 204 例）、2026-10-22 09:25（v1.2.9，134 例 / 204 例）、2026-10-21 23:11（v1.2.8，131 例 / 204 例）、2026-10-21 20:33（v1.2.7，5m55s / 164 例 / 24 文件）、2026-10-21 16:17（v1.2.6，6m26s / 124 例 / 21 文件 149 例）、2026-10-21 15:11（6m39s / 120 例 / 20 文件 138 例）、2026-10-18 的旧值（4m36s、80 例、clippy 11 条）与更早的 7m03s/109 例仅作历史对照，勿跨轮引用。
> **记录人**：工程师（寇豆码）　**最近一次执行**：2026-10-22

---

## 1. 日常门（`scripts/gate-all.sh`）

### 1.1 一条命令

```bash
bash scripts/gate-all.sh            # 默认 fail-fast：任一门 FAIL 立即停
bash scripts/gate-all.sh --keep-going  # 可选：失败也跑完剩余门再汇总
```

脚本自带铁律兜底：内部 `export PATH="$HOME/.cargo/bin:$PATH"`（本机 Bash 默认无 cargo，见 D-33/D-44 教训，**勿删该行**）；Rust 命令统一 `--target x86_64-pc-windows-gnullvm`（非管理员无法装 MSVC，gnullvm 是实测唯一可用目标）。不跑 `tauri build`（出包是显式动作，见 §2）；体积核验直接复用 `scripts/check-size.sh`，不重复造轮子。任一门 FAIL → 汇总后 `exit 1`。

> **doctest 的隐形依赖（2026-10-21 踩到，已修）**：`cargo test` 会连带跑 `///` 下的示例（doc-tests）。
> 本机 gnullvm 目标要靠 `.cargo/config.toml` 里 `target.x86_64-pc-windows-gnullvm.rustflags` 补两处
> `-L native=…`（CRT self-contained + `wsllibs` 的 Windows 导入库），**而 cargo 不会把 `rustflags`
> 转发给 rustdoc 发起的 doctest 编译**——那条命令只吃 `rustdocflags`。缺了它，只要有人往文档里写一个
> ``` 示例，Rust 门就会以「`lld: error: unable to find library -lkernel32` / `undefined symbol:
> mainCRTStartup`」这种**看起来像环境坏了**的方式失败（我曾因此误判为工具链问题）。
> 现已在同一 `[target.x86_64-pc-windows-gnullvm]` 段镜像了一份 `rustdocflags`，`cargo test
> -p passwordms-core --doc --target …` 1 passed。**新增/删除这两个列表必须同步**，否则 Rust 门会红得莫名其妙。
> 注意 `config.toml` 被 `.gitignore` 排除，所以同样的三条**也必须写进 `.cargo/config.toml.example`**——
> 只改本机私有文件等于没修，换机器/重新 clone 会原样复发（已补，详见 `docs/03b-verification.md` §8.3）。

### 1.2 门清单与本次实跑数字

| # | 门 | 命令 | 本次实测（2026-10-22 16:19 · v1.2.11 + Q-2） | 判定 |
|---|----|------|---------|------|
| 1 | 前端单测 | `npm run test`（vitest） | **26 文件 204/204 通过**（与 v1.2.10 同数——D-80 全是后端，`src/` 零改动） | ✅ 13s |
| 2 | 前端类型检查 | `npm run check`（svelte-check） | **0 errors / 0 warnings** | ✅ 14s |
| 3 | CSS 层叠契约 | `npm run check:css` | **0 FAIL / 6 WARN**（6 WARN 为既有浅色主/危险按钮对比度 3.74~4.38:1，过控件底线 3.0 未达 AA 4.5，已知项） | ✅ 4s |
| 4 | **Rust 格式基线**（**Q-2 新增**） | `cargo fmt --all -- --check` | **0 处差异**（拉平前基线为红：**12 处 / 3 文件**，见 §1.6） | ✅ 2s |
| 5 | Rust 全量测试 | `cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm` | **148 通过 / 0 失败 / 1 忽略**（**24 条** `test result` 行；忽略项为 `compaction.rs` 的 `real_vault_ledger`，`#[ignore]` 显式跳过——它要读真实库，不进日常门。较 v1.2.10 的 145 新增 **3 例**：D-80 已仲裁冲突停止保活，GC 出口 / 压缩出口 / `KeepBoth` 防过修各 1 例） | ✅ 400s |
| 6 | Rust 静态检查 | `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm` | **0 error / 0 条 lint**（#530 已清零，基线 0——**此后任何 warning 都是新引入**，门禁 `-gt 0` 会打 `⚠` 行如实登记；逐条修法见 §1.4） | ✅ 4s |
| 7 | 体积门-桌面 | `bash scripts/check-size.sh desktop --allow-missing` | MSI **2.42 MB**（2,539,520 B）+ NSIS **1.60 MB**（1,679,584 B），均 ≤ 10 MB；绿色版 zip **1.95 MB**（2,044,590 B，4 件含 `WebView2Loader.dll`，**zip 内 exe 与最终 exe 逐字节相同，sha256 `e79fec35…`**） | ✅ 30s |
| 8 | 体积门-Android | 扫 `target/PasswordMS-*-arm64.apk` 逐个核验 | **6.66 MB ≤ 12 MB**（`PasswordMS-1.2.11-arm64.apk`，6,983,315 B / sha256 `8f8c6f94…`；字节数与 v1.2.10 **恰好相同**——D-80 只多一个 `if` 分支且真身 .so 按页对齐，**hash 与 mtime（14:26）不同，非旧包**；V3 签名证书 SHA-256 `610ca6f0…3c7c` = 正式证书；`target/` 下**共 13 个** APK 逐个核验全达标） | ✅ 15s |

> **§1.2 的读数口径（重要）**：第 1~6 行来自**同一次** `bash scripts/gate-all.sh` 完整实跑（2026-10-22 22:35:39 前端起跑，**8 门全 PASS / 0 失败 / 0 跳过，EXIT=0**）；
> 第 7~8 行的字节数来自**同一棵树**的 v1.2.11 正式出包产物（`--allow-missing` 口径不变）。
> ⚠️ **门禁输出必须落日志再 grep**：`bash scripts/gate-all.sh | tail -N` 会把前端例数等关键读数截掉，且日志含 ANSI 色码 —— 用 `grep -a` 才读得到。
> **「门禁起跑后零改动」是这个读数成立的唯一凭据**（2026-10-21 当天因并行写入报废过两轮读数，见 §1.5）。固定收尾断言（**必须排除构建缓存**，否则 `.gradle` 里的 lock/bin 会把断言变成噪音）：
> ```bash
> find src src-tauri crates docs package.json -newermt "<门禁前端起跑时刻>" -type f \
>   | grep -vE "/target/|/gen/android/\.gradle/"
> # 输出为空 = 这轮覆盖的就是最终交付树；非空 = 读数作废，重跑
> ```
> 前端 204 例的构成（逐文件实测，`npx vitest run --reporter=json`）：26 个文件中，本轮新增 2 个 ——
> `pairing-state.test.ts` **16 例**（D-75）、`vault-compact-ui.test.ts` **19 例**（D-76）；
> 既有文件长大：`ipc-contract.test.ts` **18 例**（命令锚点 53）、`settings-row-type-guard.test.ts` **8 例**、`settings-page.test.ts` **7 例**。
> ⚠️ **本节不再写 `164 + Δ` 的算术式**：v1.2.7 的逐文件分解未留档，任何反推出来的 Δ 都是猜的（当天已因「算出来的数字进文档」被纠正过一次）。
> **Rust 403s 与上轮 287s 不可直接比较**：本轮改过 `crates/` 源码（D-76 压缩）→ 全量重编；上轮 Rust 零改动、全增量复用。
> **前端例数在交付窗口内是移动靶**：它是各分工的累加量。本节记的是**"最近一次实跑的定值 + 时间戳"**，不是冻结基线；
> 引用请连同时间一起引，别跨轮引用一个裸数字（已两次出现"旧数字被当现值"的事故）。

> **门禁读数因并行写入而报废一次（2026-10-21，v1.2.7 本轮）**：本轮第一遍门禁于 **20:24:11** 启动、报 `Test Files 23 passed` / PASS 7 门 / EXIT=0，
> **但读数作废**——前端测试阶段只跑 12s，20:24:2x 就结束了，而 `src/lib/settings-row-type-guard.test.ts` 到 **20:25:30** 才被创建，
> 于是那轮**少了 8 例守卫**却依然全绿：**门禁不会告诉你"它没看到的东西"**。
> 判据不是看总判定（两轮都是 PASS），而是看**文件时点 vs 门禁阶段时点**。重跑后 24 文件 164 例才对上账（21+3 / 149+15）。
> **教训**：跑门禁前必须确认文件树静止；派工给并行工程师后，要么等他明确停手，要么对比 sha256 基线，不能靠"我觉得他改完了"。
> 配套做法：让工程师交付**静止树 sha256 基线**（`Settings.svelte 05514fb2…` / `settings-row-type-guard.test.ts 4d034b83…` / `06-ui-design.md d61974ad…` / `settings-page.ts 95c6452a…`），跑前跑后各校一次。

> **测量口径事故（2026-10-21 修，值得记住形状）**：`timing_sidechannel` 在本轮门禁里红了两次（比值 1.671 / 1.502，越界 `[0.7,1.4]`），
> 而同一棵代码树 20 分钟前刚通过。查下去不是安全缺陷，是**采样相位缺陷**：原实现「一条路径连续跑满 N 次再换下一条」，
> 相位顺序固定为 错误密码 → 内容损坏 → 结构性损坏，于是**第一相位独自承担机器上升期**（CPU 升频收敛、Argon2id 64MB 内存页首次触碰、杀软扫新产物的尾巴）。
> 原始数据完全吻合：内容损坏三次稳定 1726/1744/1787ms，错误密码却在 1750 与 2684/2885 两档跳。
> 修法 = **三路径轮转采样**（每轮各跑一次），**阈值 `[0.7,1.4]` / `[0.6,1.6]` 与「每条路径必须跑满 ≥50% KDF 基准」的结构性断言一字未动**。
> 验证：连跑 3 次比值 **0.974 / 0.990 / 1.005**；第 3 次整机抖动（三路径中位数全冲到 2500、p10 全在 1700）比值仍 1.005 —— 因为负载对三条路径等量施加。
> **判据**：一条时序测试若把「整机负载」记成「路径差异」，它就是在随机报红；改采样口径不算放宽阈值，改阈值才算。

### 1.3 Rust 全量 149 例逐 target 累加（2026-10-22 14:10 实跑，148 通过 + 1 `#[ignore]`）

> 口径：`cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm -- --list` 逐 target 读 `N tests, 0 benchmarks`，**不接受口头计数**。`--list` 与真实跑测的目标/用例一一对应，且不会因 `timing_sidechannel` 耗时 100s+ 而拖慢复核。

| 测试目标 | 用例数 | 备注 |
|---------|-------|------|
| lib 单元测试（`src/lib.rs`） | 25 | 含 `attachment_name::tests` 7（D-66 `content://` 文件名解析）+ `error::tests` 5（D-62/P0 文案键）+ `paths::tests` 4 + `sync::` 9（`protocol` 4 + `resync::tests` 3 D-66 平台前缀 + `handshake::tests` 2 D-69 指纹口径） |
| `add_attachment_bytes` | 6 | **D-66 新增**：`content://` 字节入口与 `source_path` 入口的校验链一致性（含 255B 文件名边界、20MB 上限、**校验顺序 ④/⑥ 不得漂移**——本轮正是它抓到残留变异注入） |
| `attachment_sync` | 3 | **P0 T1 新增**：附件删除跨端传播（双端 loopback，双向 + 幂等） |
| `auto_lock` | 3 | 前端 `auto-lock.ts` 的 Rust 侧对偶（假时钟） |
| `container_roundtrip` | 7 | A-01/A-02 证据（含 Phase 4b 字节级原位分支用例）。**D-78 新增 4 例**：`header_grows_to_next_capacity_and_reopens`（8192→16384 常规翻倍不被新夹取误伤）、`header_growth_past_u16_ceiling_fails_without_corruption`、`header_at_exact_u16_max_roundtrips`（65535 是闭区间）、`header_growth_at_max_capacity_fails_fast`。四例共同判据：**失败后 `Container::open` 仍成功 + 容量字段与 `record_start()` 未变 + 帧数不变**（只断言 `is_err()` 对"先写坏再报错"无感） |
| `crypto_vectors` | 4 | A-05 错误码维度 |
| `discovery` | 7 | **D-74 新增**（UDP 认证发现）：① `discovery_roundtrip_on_loopback`；② `discovery_no_reply_for_unpaired`（未配对**静默不回包**）；③ `discovery_drops_mismatched_q`；④ `discovery_rejects_bad_version_and_oversized_tags`；⑤ `discovery_rate_limited_per_source`；⑥ **`resync_heals_stale_addr_via_discovery`（验收核心：把 `peer_port` 写错仍能同步成功并写回新端口）**；⑦ **`discovery_ignores_self_reply`（自应答过滤，三组断言：过滤组 / 对照组 / 弱化成只判 IP 的探针组）** |
| `health_and_generator` | 8 | |
| `import_bitwarden` | 3 | |
| `import_template` | 4 | **D-72 新增**：① 模板自身导入 **0 跳过**（`total==2`/`items.len()==2`/`skipped==0`/1 个分组）；② 表头 10 列与 `KNOWN_COLUMNS` **逐字同序**；③ 示例行**不溢出** `custom_fields`；④ 无 BOM + 尾换行 + `lines()==3`。另在 ① 内钉死「notes 必须含**半角**逗号」——旧模板误用全角「，」，导致英文引号形同虚设、转义路径从未被验证 |
| `pairing_loopback` | 4 | D-44 配对回环 |
| `pairing_session_code` | 3 | **D-69 新增**（替换 `pairing_fingerprint_consistency` 的 2 条 `#[ignore]`）：① 两屏会话码逐字相同且为 16 位小写 hex；② 会话码跨配对必变、设备指纹跨配对不变、两者不相等；③ 电脑端对端指纹**留空**（不拿临时公钥冒充） |
| `rename_paired_device` | 5 | D-66 设备昵称（含 64 字节边界：65 字节必拒 / 22 汉字必拒 / 21 汉字必过） |
| `restore_backup` | 5 | |
| `resync_addr_heal` | 3 | **P0 T2 新增**：`sync_port` 上报 + `update_paired_addr` 契约 |
| `sync_blob` | 12 | D-53 附件同步，1 例不得回退 |
| `sync_conflict` | 5 | |
| `sync_deletion` | 8 | D-56 删除传播 |
| `sync_replay` | 10 | A-06/A-07/A-08 + D-57 新增 2 例（`ResyncHello` 字面量需带 `sync_port: 0` 表示老客户端） |
| `timing_sidechannel` | 1 | **本次实跑 104.38s（历史 103~169s，波动属正常）**，KDF 烧时序设计使然，勿当卡死。采样口径已改为**三路径轮转**（原为逐路径连续采样，会把整机负载记成路径差异 → 间歇假红，详见 §1.2「测量口径事故」） |
| `unlock_error_probe` | 1 | D-10 护栏（`assert_ne!(code, "ERR_UNLOCK_FAILED")`——D-10 已修，此护栏的存在性见 04-qa.md §Phase 4c） |
| `zeroize_check` | 7 | A-04 证据（含 D-1b folders 擦除） |
| `compaction` | 14 | **D-76 新增 7 + D-79 新增 3 + D-80 新增 3，共 13 通过 + 1 `#[ignore]`**：D-76 —— ① `stats_separate_live_and_dead_attachment_bytes`；② `compact_reclaims_deleted_attachment_bytes`；③ **`compact_keeps_item_tombstone_for_sync`（防回魂）**；④ **`compact_keeps_blob_referenced_by_trash_snapshot`（回收站保活）**；⑤ `compact_drops_churn_tombstones_of_live_records`；⑥ **`compact_preserves_header_counts_and_master_password`**；⑦ `compact_is_noop_when_nothing_to_reclaim`。D-79 —— ⑧ **`compact_keeps_blob_referenced_by_conflict_snapshot`（冲突快照保活，撤除修复即红）**；⑨ **`gc_orphan_blobs_keeps_blob_referenced_by_conflict_snapshot`（同根因的另一条出口）**；⑩ **`compact_drops_blob_referenced_by_nobody`（防过修：谁都不引用的仍必须丢）**。D-80 —— ⑪ **`gc_orphan_blobs_reclaims_blob_of_resolved_conflict`（GC 出口：已仲裁冲突钉住的附件必须被回收）**；⑫ **`compact_reclaims_blob_of_resolved_conflict`（压缩出口：必须真从文件里抹掉，不是只置墓碑）**；⑬ **`resolve_conflict_keep_both_keeps_inherited_blob_alive`（防过修：`KeepBoth` 继承进副本条目的 blob 必须仍活着、字节可读、压缩后仍在）**；⑭ `#[ignore]` `real_vault_ledger`（要读真实库，不进日常门） |
| doc-tests | 1 | `attachment_name.rs` 的 ``` 示例（**必须靠 `.cargo/config.toml` 的 `rustdocflags` 才能链接**，见 §1.1） |
| **合计** | **149** | 148 passed / 0 failed / 1 ignored；**23 个编译目标（lib + 22 集成测试）+ 1 个 doc-test 目标 = 24 条 `test result` 行**（与 `gate-all.sh` 日志逐行累加吻合） |

### 1.4 clippy 基线（2026-10-18 实跑 11 条 → Q-1 清 4 条剩 7 条 → **2026-10-22 #530 清零至 0**）

`--all-targets` 会把**测试代码**一并纳入 lint，故总数高于历史「lib-only ≤3」口径。2026-10-18 实测 **0 error / 11 条 lint**；其中**测试代码 4 条属低价值噪音，已在 Q-1 工单顺手清零**（`sync_deletion.rs` doc 懒续行 ×2 + `manual_contains` ×1、`pairing_loopback.rs` doc 懒续行 ×1）。剩余 **7 条已于 2026-10-22 随 #530 质量门专项清零**——全部为 lint 修整，**零行为变更、零公开签名变更**（2026-10-22 复跑 `cargo clippy --workspace --all-targets` = **0 error / 0 warning**）：

| lint | 原位置（D-80 后行号） | 修法（一句话） | 涉签名/行为？ |
|------|------|------|------|
| consider using `sort_by_key` ×2 | `src/store/repository.rs:1232,1306` | `sort_by(\|a, b\| b.x.cmp(&a.x))` → `sort_by_key(\|e\| std::cmp::Reverse(e.x))`（同为稳定排序，结果序列逐位不变） | 否 |
| items after a test module | `src/error.rs`（tests 模块后置 3 个 `impl From` + `CoreResult` 别名） | 把文件尾的 3 个 `impl From<io/serde_json/uuid>` 与 `pub type CoreResult` 整体**挪到 `mod tests` 之前**（Rust 项声明与顺序无关，纯挪位） | 否 |
| large size difference between variants | `src/session.rs:23` | `#[allow(clippy::large_enum_variant)]` + 注释说明：Box 化需改公开类型 `Unlocked`，按「不改公开签名」红线改用 allow；锁定态本就不持敏感数据，A-04 清零语义不变 | 否（allow，**未 Box**；若裁定必须真 Box 需改公开签名，另行立项） |
| borrowed expression implements traits | `src/sync/protocol.rs:372`（测试内） | `to_value(&sample(...))` 去掉 `&`（`sample` 返回值直接按值传入即可） | 否 |
| map_or can be simplified | `src/store/repository.rs:2118` | `best.map_or(true, \|b\| rev > b)` → `best.is_none_or(\|b\| rev > b)`（语义等价改写） | 否 |
| owned instance just for comparison | `src-tauri/src/commands/sync_cmd.rs:137` | `joiner.host_id().to_string()` → `joiner.host_id()`（`host_id()` 返回 `&str`，`String == &str` 本就成立，少一次分配） | 否 |

> 历史口径（已作废，留档）：2026-10-21 复核为 7 条「登记不动」，行号旧值 786/846/258/361/1226/125 → 1097/1171/265/361/1680/131 → D-80 后 1232/1306/265/23/2118/137。当时不切 `-D warnings` 的理由（存量含 `error.rs` impl 挪位、`session.rs` Box 化等结构性改动）随 #530 清零而消失——**此后任何 warning 都应视为新引入**，`gate-all.sh` 的基线判定已同步从 `-gt 7` 改为 `-gt 0`（见下）。
>
> `gate-all.sh` 同步改四处（原「基线 ≤7」→「基线 0」）：头注释、clippy 段注释、pass 文案、`-gt` 判定。**注意 `-gt 0` 的语义变化**：≤7 时代码含义是"warning 超基线才提示（不阻塞）"，0 时代码含义是"**任何 warning 都该提示**"——pass 文案同步改为 `warning 0 条（基线 0）`，一旦出现 warning 行即打印 `⚠ warning 数超过已知基线 0`，仍不阻塞但如实登记。（门禁脚本改动的完整实跑读数由主理人复核后刷新 §1.2，此处不预填。）
>
> 脚本对 warning 只**如实计数不阻塞**（0 error 是唯一硬约束）；计数已剔除 cargo 汇总行与 build-script（windres 兜底）提示，避免重复统计。

### 1.5 门禁读数报废事故（2026-10-21，同日两次）

**现象**：同一天连续两轮 `gate-all.sh` 都报 `PASS / 7 门 / EXIT=0`，但读数**不覆盖最终交付树**：

| 轮次 | 前端起跑 | 报的例数 | 为什么作废 |
|---|---|---|---|
| 第 1 轮 | 22:28:05 | 26 文件 201 例 | 工程师的改动 22:31 才落盘 —— **前端阶段比改动早 3 分钟** |
| 第 2 轮 | 22:36:37 | 26 文件 201 例 | 改动 22:36–22:41 仍在进行 —— **门禁与编辑并发** |
| 第 3 轮 | 22:45:08 | **26 文件 204 例** | `find … -newermt 22:45:08` **空输出** → 覆盖最终树，此轮才算数 |

**根因不是门禁写错了，是「门禁跑到一半有人在改同一个文件」** —— 具体到本轮，是主理人与前端工程师各改半个 `Settings.svelte`。
**`PASS` 只说明「它跑到的那一瞬间是绿的」，不说明「它跑完了你以为它跑的东西」。**

**判据（固定收尾，已写进 §1.2）**：比对**文件改动时刻** vs **门禁各阶段时刻**；`find -newermt <前端起跑时刻>` 必须为空。
**不要靠「总判定 PASS」判断读数有效性** —— 那正是这两个案例里唯一一直为真的信号。

**纪律**：
1. 派工时就约定「门禁期间冻 `src/` `crates/` `src-tauri/`」，改动排队；
2. 同一文件**只许一个人改**（本轮两人各改半个 `Settings.svelte`，是读数报废的直接成因）；
3. 出包前的最后一轮门禁，跑完立刻做那条 `find`；非空就重跑，不解释、不豁免。

### 1.6 fmt 基线（Q-2，2026-10-22 拉平）

**拉平前状态**：`cargo fmt --all -- --check` 在本机**长期为红**（`EXIT=1`），v1.2.11 实测 **12 处 / 3 个文件**：
`crates/passwordms-core/tests/compaction.rs` **10 处**、`crates/passwordms-core/src/store/repository.rs` **1 处**（`precompact_backup_path` 的
`let mut name = path.file_name()…` 单行超宽）、`src-tauri/src/commands/system_cmd.rs` **1 处**。
**全部是换行重排**（rustfmt 把超宽调用拆成多行），**没有一处语义改动**，也不是某一轮引入的——是长期累积。

**为什么没顺手做**：其中两个文件（`compaction.rs` / `repository.rs`）正是功能改动的热点文件。
在功能分支上跑 `cargo fmt --all`，格式噪音会混进同一笔 diff，评审时「哪几行是真的」就看不出来了。
故按 **Q-2 单开一单**处理：功能提交落地后再格式化，`git log` 里一眼能看出「这笔只动空白」。

**拉平后验证**（三项，缺一不可）：

| 检查 | 拉平前 | 拉平后 |
|------|--------|--------|
| `cargo fmt --all -- --check` | **EXIT=1（12 处）** | **EXIT=0** |
| `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm` | 0 error / 7 条 | **0 error / 7 条**（不变） |
| `cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm` | 148 / 0 / 1 | **148 / 0 / 1**（24 条 `test result` 行，逐数一致） |

> **已按 A 案纳入 `gate-all.sh`（2026-10-22，主人拍板）**：`cargo fmt --all -- --check` 成为**第 4 门**（执行序；总门数 **7 → 8**），
> 位置在前端三门之后、Rust 全量之前 —— 格式红是最容易修的一类，不该让人先等 5 分钟跑完测试才发现。
> 实现见 `scripts/gate-all.sh` 的 `fmt_gate()`：用 `grep -c '^Diff in'` 计数（**`grep -c` 在 0 匹配时返回 1**，已用 `|| echo 0` 兜底，
> 否则差别数会变成空串）；判红时只打印前 20 行坐标不刷屏，完整 diff 落 `/tmp/pms-gate-fmt.log`。

> **这个门被验证过「真的会红」**（没红过的门等于没设）——负向探针（2026-10-22）：
> 把 `src-tauri/src/commands/system_cmd.rs` 换回格式化前的版本（sha256 `cd35b305…`）后跑 `gate-all.sh`，
> 得到 `NEG_EXIT=1`、`[FAIL] fmt 失败（2s，1 处差异）`、坐标精确打在 `src-tauri\src\commands\system_cmd.rs:125`、
> 汇总 `[FAIL] 总判定：FAIL（通过 3 / 失败 1 / 跳过 0）`，且 **fail-fast 使 Rust 全量门根本没启动**（省掉约 5 分钟）。
> 还原后 `--check` 立刻回到 `EXIT=0`，8 门重新全 PASS。

---

## 2. 出包门（产物是显式动作，不在日常门内）

### 2.1 桌面（Windows）

```bash
# 前置：前端资源烤进 exe（dist/ 烤死在二进制里——只改前端不重编 exe 无效）
npx tauri build --target x86_64-pc-windows-gnullvm
bash scripts/check-size.sh desktop          # EXIT=0 才算过（≤10 MB）
bash scripts/make-portable.sh               # 如需便携版；前置 tauri build 产物
```

产物落点：`target/x86_64-pc-windows-gnullvm/release/bundle/{msi,nsis}/`。

### 2.2 Android：唯一正道 = `scripts/build-android.sh`

**禁止直跑 `npx tauri android build`**，两条实测证伪的岔路（引 `.workbuddy/memory/MEMORY.md` 与 `docs/04-qa.md` Phase 4d / D-17）：

1. **symlink 空壳包（D-17，装真机必崩）**：本机非管理员、无 `SeCreateSymbolicLinkPrivilege`，cargo-mobile2 放置原生库依赖符号链接 → `jniLibs/<abi>/` 留下**真实的 0 字节文件**被 AGP 打进 APK。空壳包因少了 4.89 MB `.so` 体积反而更小（≈1.62 MB），**门禁越绿越可疑**。
2. **universal 超红线**：直跑默认产 4-ABI universal 包，超 12 MB 体积红线。

`build-android.sh` 的四道断言（缺一即 fail-closed）：① 包内 `.so` ELF magic 非空壳；② APK ≤ 12 MB；③ 关键条目存在；④ 包内 `.so` sha256 **与构建产物一致**（防垃圾 ELF 蒙混，QA Phase 4d 已对该断言做过对抗证伪）。产物：`target/PasswordMS-<ver>-arm64.apk`（signed）。

**⚠️ 三个环境变量是硬前置（2026-10-21 实测：缺一个就在 11 秒内 die，不产生任何产物）**：

```bash
export JAVA_HOME="D:/AndroidDev/jdk17"
export ANDROID_HOME="D:/AndroidDev/sdk"
export NDK_HOME="D:/AndroidDev/sdk/ndk/27.2.12479018"
```

- **必须用正斜杠**。写成 `D:\AndroidDev\jdk17` 拼进 `PATH` 后，Git Bash 里 `java` 仍然找不到（`java: command not found`），而脚本因为 `java -version | head` 的退出码被 `head` 吃掉而继续往下跑 —— **看起来像"环境配好了"**。
- 脚本 `:77` / `:80` 的守卫是 `die`（fail-safe，不会打出空壳包），但 `:394` 自己又写了 `ANDROID_HOME:-D:/AndroidDev/sdk` 默认值 —— **守卫比实际需求严，同文件内自相矛盾**。修法应照 `src-tauri/build.rs` 的 `ensure_windres_on_path()` 先例：**探路径→自己前置进 PATH**，而不是让操作者去 export（"文档里写了" ≠ "生效"）。
- 正式签名默认生效（`D:/AndroidDev/keystores/passwordms-release.jks`）；测试证书须显式 `PMS_KEYSTORE` / `PMS_KEY_PASS`，**永不可发布**。出包后核对：`apksigner verify --print-certs` → SHA-256 应为 `610ca6f0…3c7c`。

### 2.3 出包后人工验收四门（MEMORY.md 铁律，缺一不可）

```bash
# 1. 产物新鲜度：源码不得晚于产物（输出为空才过）
find src src-tauri -newer <产物路径>

# 2. 新前端真进包：新哈希名在 exe 内 + 旧哈希名消失
#    ⚠️ 不可对 exe 做**内容级** grep——Tauri 把前端资源压缩后烤进二进制，
#       搜 'pms-btn' 之类必然搜不到（假阴性）。
#    ✅ 可靠判据 = **资源文件名**（assets/index-<hash>.js）作为 asset map 的键是明文存的，
#       可以 grep；再与 dist/ 实际文件名逐字对比。可直接用：
grep -a -oE "assets/(index|style)-[A-Za-z0-9_-]{6,12}\.(js|css)" <exe 或 包内 .so> | sort -u
ls dist/assets/ | sort
#    两行输出必须逐字相同

# 2b. 双端同源（2026-10-21 新增，比「重新构建过」更强）
#     桌面 exe 与 APK 内 lib/arm64-v8a/libpasswordms_lib.so 的资源名必须逐字相同
#     —— 它证明的是**内容同一份 dist**，而不是**动作跑过一遍**
TMP=$(mktemp -d) && unzip -o -q target/PasswordMS-<ver>-arm64.apk "lib/*/libpasswordms_lib.so" -d "$TMP"
grep -a -oE "assets/(index|style)-[A-Za-z0-9_-]{6,12}\.(js|css)" "$TMP"/lib/*/libpasswordms_lib.so | sort -u

# 2c. 绿色版一致性：zip 内 exe 必须与构建产物逐字节一致（zip 会滞后于最终构建）
unzip -p <zip> "PasswordMS-Portable/passwordms.exe" | sha256sum
sha256sum target/<target>/release/passwordms.exe

# 3. 体积双红线
bash scripts/check-size.sh desktop   # ≤10 MB
bash scripts/check-size.sh android   # ≤12 MB（扫 src-tauri/gen 与 gen）

# 4. Android 原生库换代：包内 .so sha256 与构建产物一致（build-android.sh 断言④的独立复核）
```

> **门禁绿灯 ≠ 包能用**（D-17 一句话教训）。四门全过只证明「包是新的、没超标、原生库是真的」，真机行为仍归 §4。

---

## 3. 安全验收点 A-01~A-10 → 测试证据映射

> 证据均引 `docs/04-qa.md` Phase 4b/4c 的 file:line 与对抗实验结论；「强断言」指 QA 用对抗实验（临时改坏生产代码）证实过**失败时会真的红**。

| # | 验收点 | 测试证据（file:line） | 强度 | 判定 |
|---|--------|----------------------|------|------|
| A-01 | 库文件无明文凭据 | `tests/container_roundtrip.rs:56-99`（`:92-96` 对文件字节搜明文 needle） | 强（真实字节检查） | ✅ |
| A-02 | 最小重写不整库重写 | `single_item_update_hits_in_place_branch_and_leaves_others_intact`（`container_roundtrip.rs:199-212`）：等长密文更新、总长/帧数不变、无关帧字节级一致；**对抗实验 B** 证实注释掉 `place_frame` 原位分支会红 | 强（字节级，Phase 4b §5） | ✅ |
| A-03 | 剪贴板 30s 清空、替换则不清 | 前端计时逻辑单测（`src/lib/clipboard.test.ts`）+ Phase 4b D-2 修复（分支语义：只清自有内容；锁库「先清剪贴板再丢弃记忆值」） | 逻辑面闭合；真机项见 §4 | ✅* |
| A-04 | 锁定后内存无明文残留 | `tests/zeroize_check.rs` 7 例（`wipe_zeroes_buffer`、`repository_wipe_plaintext_clears_folder_fields` 等）+ `session.rs:146-150` Drop 路径；D-1b folders 残留已闭合（Phase 4b） | 强（代码审查 + 单测） | ✅* |
| A-05 | 解锁失败文案/耗时不可区分 | 错误码：`tests/crypto_vectors.rs:52-91` + `unlock_error_probe.rs`（D-10 修复后 5 种结构性损坏全 → `ERR_UNLOCK_FAILED`）；时序：`tests/timing_sidechannel.rs`（两路径比值 0.960/0.995，QA 独立复跑 1.031/1.137，均 ≈ KDF 基准无早退） | 强（Phase 4c + QA 复跑） | ✅ |
| A-06 | 一次性配对令牌重放被拒 | `pair_token_rejections_via_server`（`sync_replay.rs:225-246`）双侧断言 `ERR_PAIR_TOKEN_USED/EXPIRED/INVALID`；**对抗实验 A** 证实注释 `server.rs` 的 used 检查会红 | 强（Phase 4b §5） | ✅ |
| A-07 | 未配对设备请求被拒 | `resync_unpaired_is_rejected`（`sync_replay.rs:262-290`）真起 ResyncServer 走真实拒绝路径（`resync.rs:292-297`） | 强（行为级） | ✅* |
| A-08 | 重放同一载荷被拒 | `replay_rejected_on_non_monotonic_seq` / `replay_rejected_outside_time_window` / `replay_rejected_after_clock_rollback`（`sync_replay.rs:168-183`）/ `envelope_aad_binds_session_and_seq`（`:205-219`，分别篡改 seq 与 ts） | 强（Phase 4b 补齐回拨与 AAD） | ✅ |
| A-09 | 日志无凭据 | 静态扫描（04-qa.md:122）：`println!/eprintln!/log::` 仅 `src-tauri/src/lib.rs:81` 一处启动期错误信息（非凭据）；前端 console.log 0 命中 | 静态检查 | ✅ |
| A-10 | 危险操作二次确认 | 无自动化证据（UI 交互） | — | ⬜ §4 |

> ✅* = 逻辑/单测面闭合，但标注 * 者仍有真机环境项未验（见 §4），不影响门禁判定但**不得对外宣称「真机已验」**。

---

## 4. 已知边界（未验证 ≠ 通过）

以下项目**本机不具备验证条件**，未纳入任何门禁；门禁全绿**不覆盖**它们，交付话术必须区分「单测闭合」与「真机已验」：

| 项 | 现状 | 所需条件 |
|----|------|---------|
| A-03 真机剪贴板（30s 实清、锁定期间清空仍生效） | 仅前端计时单测 | 真机/桌面运行环境手动验证 |
| A-07 真机未配对拒绝 / **双端局域网端到端同步** | 仅协议层单测（回环） | 两台真机 + 同一局域网 |
| A-10 导出/清空/解除配对二次确认 | 无自动化证据 | UI 人工走查 |
| A-04 内存取证级清零 | 单测 + Drop 路径审查 | 外部内存取证工具 |
| A-05 release 构建时序分布 | timing_sidechannel 为 **debug** 构建，绝对值不可外推 release | release profile 复测 |
| APK 签名与真机安装 | signed APK 已产出（断言①~④过） | 真机安装走查 |
| check:css 6 WARN（浅色主/危险按钮对比度） | 已知既有项，过控件底线未达 AA 正文 | 设计决策（改色 or 维持） |
| `cargo fmt` 代码格式统一 | **已闭合（Q-2，2026-10-22）**：基线拉平并**纳入门禁**。<br>① v1.2.5 窗口的 58 文件纯格式化改动已由主理人以独立 `style:` commit 提交；② v1.2.11 后残留的 **12 处 / 3 文件**差异（`tests/compaction.rs` 10、`store/repository.rs` 1、`src-tauri/commands/system_cmd.rs` 1，全是换行重排）已再次拉平，`--check` EXIT=1 → **0**，clippy 7 条与 Rust 148/0/1 均不变；③ `cargo fmt --all -- --check` 已作为**第 4 门**加入 `gate-all.sh`（总门数 7 → 8），并做过**负向探针**（换回未格式化版本 → 门确实红、fail-fast 生效）。底账与验证矩阵见 **§1.6** | 已闭合，无需再排期 |

> 引用 `docs/03b-verification.md` 综合判定：CONDITIONAL PASS，剩余阻塞项已收敛为环境类。**任何一项上表内容的验证完成，须回写本文档并更新对应判定。**
