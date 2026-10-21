# 07 · 质量门与出包门禁（权威）

> **本文档的性质**：PasswordMS 交付前的**唯一门禁权威**。§1 日常门 = 一条命令跑完的自动化总门；§2 出包门 = 产出安装包/APK 的**唯一正道**与出包后的人工验收；§3 安全验收点 A-01~A-10 与测试证据的映射；§4 已知边界（**未验证 ≠ 通过**）。
> 所有数字均来自 `bash scripts/gate-all.sh` **完整实跑**，不接受口头计数。**§1.2 / §1.3 已按 2026-10-21 15:11 实跑刷新**（总耗时 **6m39s**，总判定 **PASS**，7 门全过 / 0 失败 / 0 跳过；Rust **120 例**、前端 **20 文件 138 例**）；2026-10-18 的旧值（4m36s、80 例、clippy 11 条）与更早的 7m03s/109 例仅作历史对照，勿跨轮引用。
> **记录人**：工程师（寇豆码）　**最近一次执行**：2026-10-21

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

| # | 门 | 命令 | 本次实测（2026-10-21 v1.2.6） | 判定 |
|---|----|------|---------|------|
| 1 | 前端单测 | `npm run test`（vitest） | **21 文件 149/149 通过**（2026-10-21 16:17:18 实测；含 D-72 新增 `save-dialog` 5、`ipc-contract` 12→15、`import` 5→7、`vault-export` 5→6） | ✅ 12s |
| 2 | 前端类型检查 | `npm run check`（svelte-check） | **0 errors / 0 warnings** | ✅ 14s |
| 3 | CSS 层叠契约 | `npm run check:css` | **0 FAIL / 6 WARN**（6 WARN 为既有浅色主/危险按钮对比度 3.74~4.38:1，过控件底线 3.0 未达 AA 4.5，已知项） | ✅ 4s |
| 4 | Rust 全量测试 | `cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm` | **124/124 通过**（21 个编译目标 + 1 个 doc-test，明细见 §1.3） | ✅ 343s |
| 5 | Rust 静态检查 | `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm` | **0 error / 7 条 lint**（基线 ≤7，明细见 §1.4），rc=0 | ✅ 13s |
| 6 | 体积门-桌面 | `bash scripts/check-size.sh desktop --allow-missing` | MSI **2.40 MB**（2,519,040 B）+ NSIS **1.59 MB**（1,664,935 B），均 ≤ 10 MB；绿色版 zip **1.93 MB**（2,026,617 B） | ✅ 15s |
| 7 | 体积门-Android | 扫 `target/PasswordMS-*-arm64.apk` 逐个核验 | **6.61 MB ≤ 12 MB**（`PasswordMS-1.2.6-arm64.apk`，6,934,163 B） | ✅ 8s |

> **§1.2 的读数口径（重要）**：第 1~5 行来自**同一次** `bash scripts/gate-all.sh` 完整实跑（2026-10-21 16:17:18→16:23:44，6m26s，**7 门全 PASS / 0 失败 / 0 跳过，EXIT=0**）；
> 第 6~7 行的字节数来自该次实跑之后的 v1.2.6 正式出包产物（`--allow-missing` 口径不变）。
> ⚠️ **门禁输出必须落日志再 grep**：`bash scripts/gate-all.sh | tail -N` 会把前端例数等关键读数截掉，且日志含 ANSI 色码 —— 用 `grep -a` 才读得到。
> 前端 149 例的构成（逐文件实测，非估算）：`138 + 5(save-dialog 新增) + 1(vault-export) + 3(ipc-contract) + 2(import) = 149`，与 `vitest` 汇总行吻合。
> **前端例数在交付窗口内是移动靶**：它是各分工的累加量。本节记的是**"最近一次实跑的定值 + 时间戳"**，不是冻结基线；
> 引用请连同时间一起引，别跨轮引用一个裸数字（本轮已两次出现"旧数字被当现值"的事故）。

> **测量口径事故（2026-10-21 修，值得记住形状）**：`timing_sidechannel` 在本轮门禁里红了两次（比值 1.671 / 1.502，越界 `[0.7,1.4]`），
> 而同一棵代码树 20 分钟前刚通过。查下去不是安全缺陷，是**采样相位缺陷**：原实现「一条路径连续跑满 N 次再换下一条」，
> 相位顺序固定为 错误密码 → 内容损坏 → 结构性损坏，于是**第一相位独自承担机器上升期**（CPU 升频收敛、Argon2id 64MB 内存页首次触碰、杀软扫新产物的尾巴）。
> 原始数据完全吻合：内容损坏三次稳定 1726/1744/1787ms，错误密码却在 1750 与 2684/2885 两档跳。
> 修法 = **三路径轮转采样**（每轮各跑一次），**阈值 `[0.7,1.4]` / `[0.6,1.6]` 与「每条路径必须跑满 ≥50% KDF 基准」的结构性断言一字未动**。
> 验证：连跑 3 次比值 **0.974 / 0.990 / 1.005**；第 3 次整机抖动（三路径中位数全冲到 2500、p10 全在 1700）比值仍 1.005 —— 因为负载对三条路径等量施加。
> **判据**：一条时序测试若把「整机负载」记成「路径差异」，它就是在随机报红；改采样口径不算放宽阈值，改阈值才算。

### 1.3 Rust 全量 124 例逐 target 累加（2026-10-21 16:17 实跑）

> 口径：`cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm -- --list` 逐 target 读 `N tests, 0 benchmarks`，**不接受口头计数**。`--list` 与真实跑测的目标/用例一一对应，且不会因 `timing_sidechannel` 耗时 100s+ 而拖慢复核。

| 测试目标 | 用例数 | 备注 |
|---------|-------|------|
| lib 单元测试（`src/lib.rs`） | 25 | 含 `attachment_name::tests` 7（D-66 `content://` 文件名解析）+ `error::tests` 5（D-62/P0 文案键）+ `paths::tests` 4 + `sync::` 9（`protocol` 4 + `resync::tests` 3 D-66 平台前缀 + `handshake::tests` 2 D-69 指纹口径） |
| `add_attachment_bytes` | 6 | **D-66 新增**：`content://` 字节入口与 `source_path` 入口的校验链一致性（含 255B 文件名边界、20MB 上限、**校验顺序 ④/⑥ 不得漂移**——本轮正是它抓到残留变异注入） |
| `attachment_sync` | 3 | **P0 T1 新增**：附件删除跨端传播（双端 loopback，双向 + 幂等） |
| `auto_lock` | 3 | 前端 `auto-lock.ts` 的 Rust 侧对偶（假时钟） |
| `container_roundtrip` | 3 | A-01/A-02 证据（含 Phase 4b 字节级原位分支用例） |
| `crypto_vectors` | 4 | A-05 错误码维度 |
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
| `timing_sidechannel` | 1 | **本次实跑 108.37s（历史 103~169s，波动属正常）**，KDF 烧时序设计使然，勿当卡死。采样口径已改为**三路径轮转**（原为逐路径连续采样，会把整机负载记成路径差异 → 间歇假红，详见 §1.2「测量口径事故」） |
| `unlock_error_probe` | 1 | D-10 护栏（`assert_ne!(code, "ERR_UNLOCK_FAILED")`——D-10 已修，此护栏的存在性见 04-qa.md §Phase 4c） |
| `zeroize_check` | 7 | A-04 证据（含 D-1b folders 擦除） |
| doc-tests | 1 | `attachment_name.rs` 的 ``` 示例（**必须靠 `.cargo/config.toml` 的 `rustdocflags` 才能链接**，见 §1.1） |
| **合计** | **124** | 0 failed / 0 ignored；**21 个编译目标（lib + 20 集成测试）+ 1 个 doc-test 目标** |

### 1.4 clippy 基线（2026-10-18 实跑 11 条 → Q-1 修订后 7 条 → 2026-10-21 复核仍 7 条）

`--all-targets` 会把**测试代码**一并纳入 lint，故总数高于历史「lib-only ≤3」口径。2026-10-18 实测 **0 error / 11 条 lint**；其中**测试代码 4 条属低价值噪音，已在 Q-1 工单顺手清零**（`sync_deletion.rs` doc 懒续行 ×2 + `manual_contains` ×1、`pairing_loopback.rs` doc 懒续行 ×1，清零后复跑 clippy 确认消失，受影响测试目标 `sync_deletion` 8/8、`pairing_loopback` 3/3 无回退）。现存 **7 条**（均涉及结构性改动，不在 Q-1 范围）：

| lint | 位置（2026-10-21 复核行号） | 状态 |
|------|------|------|
| consider using `sort_by_key` ×2 | `src/store/repository.rs:1097,1171` | 登记不动 |
| items after a test module | `src/error.rs:265`（tests 模块后置 impl） | 登记不动 |
| large size difference between variants | `src/session.rs:23` | 登记不动 |
| borrowed expression implements traits | `src/sync/protocol.rs:372` | 登记不动 |
| map_or can be simplified | `src/store/repository.rs:1680` | 登记不动 |
| owned instance just for comparison | `src-tauri/src/commands/sync_cmd.rs:131` | 登记不动（D-62 存量） |

> **2026-10-21 复核**：仍是这 **7 条**，一条没多、一条没少；**仅行号因 D-60/D-61/D-66 的插入整体下移**（旧值 786/846/258/361/1226/125 → 新值见上表，`session.rs:23` 未动）。复核时两种计数口径互相印证：脚本现有口径（`grep '^warning: '` 剔除 cargo 汇总行与 windres 兜底）= **7**，按唯一源码位置去重（`--> file:line:col` 去重）= **7**，两者一致，说明脚本的计数没有虚高。
>
> 门禁口径**维持**「0 error 阻塞 + warning 非阻塞、如实登记」，不切 `-D warnings`：存量 7 条里有 `error.rs` 的 impl 挪位、`session.rs` 的大枚举变体（需 `Box`）等**结构性改动**，不该混进功能修复的交付。**清零 7 条列入 backlog**（随 #530 质量门专项一起做）。
>
> 顺带订正一处历史口径：`gate-all.sh` 的注释与提示文案长期写着「基线 ≤3」，那是**lib-only 时代**的数字；`--all-targets` 落地后真实基线是 7，导致门禁每轮都打印「超过基线 3」的误报（狼来了，真引入时反而没人看）。现已统一为 **≤7**（`:14` 头注释、`:91` 段注释、`:111` pass 文案、`:113` `-gt` 判定**四处同改**），改后完整重跑 7 门全 PASS，`[ OK ] clippy 0 error（warning 7 条，非阻塞；基线 ≤7）`，**不再出现任何 `⚠ warning 数超过已知基线` 误报行**。

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
| `cargo fmt` 代码格式统一 | **已执行（未提交）**：v1.2.5 交付窗口已跑过 `cargo fmt --all`，工作树里 58 个文件是纯格式化改动（换行/空行/缩进，零语义），由主理人以**独立 `style:` commit** 提交，**不含任何功能改动**。<br>**因 §1.4 的 7 个行号系 fmt 之后实测的定值，该提交不会造成行号漂移**——下次不必重测。<br>提交时建议 `git diff --stat -w`（忽略空白）自证零语义变化。同批应把 `cargo fmt --all -- --check` 加入 `gate-all.sh` 成为第 8 门 | 独立 `style:` commit 提交 |

> 引用 `docs/03b-verification.md` 综合判定：CONDITIONAL PASS，剩余阻塞项已收敛为环境类。**任何一项上表内容的验证完成，须回写本文档并更新对应判定。**
