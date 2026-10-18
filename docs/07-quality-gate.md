# 07 · 质量门与出包门禁（权威）

> **本文档的性质**：PasswordMS 交付前的**唯一门禁权威**。§1 日常门 = 一条命令跑完的自动化总门；§2 出包门 = 产出安装包/APK 的**唯一正道**与出包后的人工验收；§3 安全验收点 A-01~A-10 与测试证据的映射；§4 已知边界（**未验证 ≠ 通过**）。
> 所有数字均来自 2026-10-18 `bash scripts/gate-all.sh` **完整实跑**（总耗时 4m36s，总判定 **PASS**，7 门全过 / 0 失败 / 0 跳过），复跑口径与 `docs/03b-verification.md` 一致：逐 target `test result:` 行累加，不接受口头计数。
> **记录人**：工程师（寇豆码）　**执行时间**：2026-10-18

---

## 1. 日常门（`scripts/gate-all.sh`）

### 1.1 一条命令

```bash
bash scripts/gate-all.sh            # 默认 fail-fast：任一门 FAIL 立即停
bash scripts/gate-all.sh --keep-going  # 可选：失败也跑完剩余门再汇总
```

脚本自带铁律兜底：内部 `export PATH="$HOME/.cargo/bin:$PATH"`（本机 Bash 默认无 cargo，见 D-33/D-44 教训，**勿删该行**）；Rust 命令统一 `--target x86_64-pc-windows-gnullvm`（非管理员无法装 MSVC，gnullvm 是实测唯一可用目标）。不跑 `tauri build`（出包是显式动作，见 §2）；体积核验直接复用 `scripts/check-size.sh`，不重复造轮子。任一门 FAIL → 汇总后 `exit 1`。

### 1.2 门清单与本次实跑数字

| # | 门 | 命令 | 本次实测 | 判定 |
|---|----|------|---------|------|
| 1 | 前端单测 | `npm run test`（vitest） | **12 文件 58/58 通过** | ✅ 10s |
| 2 | 前端类型检查 | `npm run check`（svelte-check） | **0 errors / 0 warnings** | ✅ 11s |
| 3 | CSS 层叠契约 | `npm run check:css` | **0 FAIL / 6 WARN**（6 WARN 为既有浅色主/危险按钮对比度 3.74~4.38:1，过控件底线 3.0 未达 AA 4.5，已知项） | ✅ 4s |
| 4 | Rust 全量测试 | `cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm` | **15 目标 80/80 通过**（明细见 §1.3） | ✅ 226s |
| 5 | Rust 静态检查 | `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm` | **0 error / 11 条 lint**（明细见 §1.4），rc=0 | ✅ 15s |
| 6 | 体积门-桌面 | `bash scripts/check-size.sh desktop --allow-missing` | MSI **2.39 MB** + NSIS **1.55 MB** ≤ 10 MB | ✅ 5s |
| 7 | 体积门-Android | 扫 `target/PasswordMS-*-arm64.apk` 逐个核验 | **6.58 MB ≤ 12 MB**（1 个产物）；无产物则 SKIP 不判负 | ✅ |

### 1.3 Rust 全量 80 例逐 target 累加（本次实跑）

| 测试目标 | 用例数 | 备注 |
|---------|-------|------|
| lib 单元测试（`src/lib.rs`） | 13 | 含 D-57 新增 `error::tests` 4 例 |
| `container_roundtrip` | 3 | A-01/A-02 证据（含 Phase 4b 字节级原位分支用例） |
| `crypto_vectors` | 4 | A-05 错误码维度 |
| `health_and_generator` | 8 | |
| `import_bitwarden` | 3 | |
| `pairing_loopback` | 3 | D-44 配对回环 |
| `restore_backup` | 5 | |
| `sync_blob` | 9 | D-53 附件同步，1 例不得回退 |
| `sync_conflict` | 5 | |
| `sync_deletion` | 8 | D-56 删除传播 |
| `sync_replay` | 10 | A-06/A-07/A-08 + D-57 新增 2 例 |
| `timing_sidechannel` | 1 | **本次实跑 109.21s，属正常耗时**（KDF 烧时序设计使然），勿当卡死 |
| `unlock_error_probe` | 1 | D-10 护栏（`assert_ne!(code, "ERR_UNLOCK_FAILED")`——D-10 已修，此护栏的存在性见 04-qa.md §Phase 4c） |
| `zeroize_check` | 7 | A-04 证据（含 D-1b folders 擦除） |
| doc-tests | 0 | |
| **合计** | **80** | 0 failed / 0 ignored |

### 1.4 clippy 基线（2026-10-18 实跑 11 条 → Q-1 修订后 7 条）

`--all-targets` 会把**测试代码**一并纳入 lint，故总数高于历史「lib-only ≤3」口径。2026-10-18 实测 **0 error / 11 条 lint**；其中**测试代码 4 条属低价值噪音，已在 Q-1 工单顺手清零**（`sync_deletion.rs` doc 懒续行 ×2 + `manual_contains` ×1、`pairing_loopback.rs` doc 懒续行 ×1，清零后复跑 clippy 确认消失，受影响测试目标 `sync_deletion` 8/8、`pairing_loopback` 3/3 无回退）。现存 **7 条**（均涉及结构性改动，不在 Q-1 范围）：

| lint | 位置 | 状态 |
|------|------|------|
| consider using `sort_by_key` ×2 | `src/store/repository.rs:786,846` | 登记不动 |
| items after a test module | `src/error.rs:258`（tests 模块后置 impl） | 登记不动 |
| large size difference between variants | `src/session.rs:23` | 登记不动 |
| borrowed expression implements traits | `src/sync/protocol.rs:361` | 登记不动 |
| map_or can be simplified | `src/store/repository.rs:1226` | 登记不动 |
| owned instance just for comparison | `src-tauri/src/commands/sync_cmd.rs:125` | 登记不动 |

> 脚本对 warning 只**如实计数不阻塞**（0 error 是唯一硬约束）；计数已剔除 cargo 汇总行与 build-script（windres 兜底）提示，避免重复统计。

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

### 2.3 出包后人工验收四门（MEMORY.md 铁律，缺一不可）

```bash
# 1. 产物新鲜度：源码不得晚于产物（输出为空才过）
find src src-tauri -newer <产物路径>

# 2. 新前端真进包：新哈希名在 exe 内 + 旧哈希名消失
#    ⚠️ 不可对 exe 做明文内容级 grep——Tauri 压缩资源，必假阴性（D-17 姊妹教训）
#    用资源哈希名（assets/index-<hash>.js）的新旧对比判断

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

> 引用 `docs/03b-verification.md` 综合判定：CONDITIONAL PASS，剩余阻塞项已收敛为环境类。**任何一项上表内容的验证完成，须回写本文档并更新对应判定。**
