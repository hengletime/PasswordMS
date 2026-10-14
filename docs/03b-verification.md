# 03b · 编译与测试验证记录（权威）

> **本文档的性质**：`docs/03-impl.md` 的 §5「全局一致性审查 = IS_PASS: YES」是**在无编译器环境下做出的静态文本审查**，并经真实编译器证伪（详见 §3）。本文档是**唯一权威的编译/测试结论来源**——所有数字均来自**实际运行编译器与测试运行器**，含完整命令。
>
> **记录人**：主理人（小李）　**执行时间**：2026-10-13　**项目**：PasswordMS

---

## 1. 结论摘要（一句话）

**Rust 工作区已可真实编译（core + src-tauri 外壳 0 error / 0 warning），核心内核 37/37 测试全通；前端 svelte-check 0 错 0 警、vitest 34/34 全通、Vite 构建成功；Windows 双格式安装包实测 1.48 MB / 2.11 MB，远低于 10 MB 红线。** 但首轮静态自查（`03-impl.md` §5）**实为误判**：真实编译器暴露 **16 个编译错误**。

| 验证面 | 命令 | 真实结果 | 判定 |
|--------|------|---------|------|
| Rust 编译（内核） | `cargo check -p passwordms-core` | 首轮 **16 errors** → 修复后 **0 errors** | ✅ |
| Rust 编译（**外壳**） | `cargo check --workspace` | **0 errors / 0 warnings**（见 §3.4） | ✅ |
| Rust 测试 | `cargo test -p passwordms-core` | **37 passed / 0 failed**（9 个测试文件） | ✅ |
| Rust 静态检查 | `cargo clippy --workspace --target x86_64-pc-windows-gnullvm --all-targets` | **0 errors / 3 warnings** | ⚠️ 非阻塞 |
| 前端类型检查 | `npm run check`（svelte-check） | **0 errors / 0 warnings** | ✅ |
| 前端单测 | `npx vitest run` | **34 passed / 0 failed**（8 个文件） | ✅ |
| 前端构建 | `npm run build`（Vite 6.4.4） | 185 模块转换成功，产物 ≈ 298 KB | ✅ |
| Tauri 桌面打包 | `npx tauri build --target x86_64-pc-windows-gnullvm` | **成功**：`passwordms.exe` **3.93 MB** + NSIS 安装包 **1.48 MB** | ✅ |
| 体积红线（Windows ≤ 10 MB） | `scripts/check-size.sh desktop` | **1.48 MB（NSIS）+ 2.11 MB（MSI）≤ 10 MB → 均达标** | ✅ |
| MSI 打包 | `npx tauri build --bundles msi` | **成功**：`PasswordMS_0.1.0_x64_en-US.msi` **2.11 MB**（WiX 缓存预热后，见 §4b） | ✅ |
| Android 打包 | Gradle `:app:assembleUniversalRelease`（见 §4c） | **成功**：`app-universal-release-unsigned.apk` **6.29 MB**，且**原生库非空**（见 §4c.5 的 D-17） | ✅ |
| 体积红线（APK ≤ 12 MB） | `scripts/check-size.sh android` | **6.29 MB ≤ 12 MB → 达标（余量 48%）**，EXIT=0 | ✅ |
| 安全验收点 A-04 / A-03 | Phase 4b 对抗实验 → 修复后复验 | **已闭合**：`folders` 明文已擦、锁库改为「先清剪贴板再丢弃记忆值」、`pending_imports` 先擦后清；新增 2 个用例并经对抗实验证实可红（见 §10） | ✅ |

> **综合判定**：**CONDITIONAL PASS**。编译 / 测试 / 体积三项硬指标**全部实证通过**；QA 两轮独立复验发现的 3 处安全残留（D-1b / D-2b / D-3b）**已修复并经主理人对源码逐行复核 + 我自己复跑的对抗实验证实闭合**。**仍不能给出无条件通过**，但剩余阻塞项**已全部收敛为环境类**：真机局域网同步端到端、A-10 UI 交互走查、APK 签名与真机安装——本机不具备条件，非代码问题。安全面（D-1b / D-2b / D-3b / D-10）与验证面（A-05 时序、体积双红线）均已闭合。
>
> **⚠️ 另有一处必须点名的教训（D-17）**：Android 出包一度"体积达标、门禁全绿"，但 APK 的**原生库是 0 字节空壳**——装真机必崩。**"门禁绿灯"只证明体积没超标，不证明包能用**。该缺陷**已绕过但未根除**，详见 §4c.5。

---

## 2. 验证环境（真实测得）

| 项 | 值 |
|----|----|
| 主机 | Commander168 / Windows 11 / 非管理员账户 |
| 构建目录 | `D:\Software\PasswordMS` |
| rustc | `1.99.0 (b940084d7 2026-09-28)` |
| cargo | `1.99.0 (5f94df478 2026-08-27)` |
| host 三元组 | `x86_64-pc-windows-gnu`（rustup 安装，**无 Visual Studio Build Tools**） |
| 实际编译目标 | **`x86_64-pc-windows-gnullvm`**（LLVM ABI，绕开 mingw 依赖，见 §8） |
| clippy | `rustup component add clippy` 后可用 |
| Node / npm | `v22.22.2` / `10.9.7` |
| 代码规模 | Rust 46 文件 / 6169 行；前端 53 文件（`.svelte` + `.ts`） |

**可复现命令**（`.cargo/config.toml` 已固化 `linker` 与 `rustflags`，故只需）：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
cd "D:/Software/PasswordMS"
cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm
```

---

## 3. 首轮真实编译：16 个错误（撤回 `IS_PASS: YES` 的依据）

`03-impl.md` §5 的 8 项检查**全部是 grep/人工比对**，只能证明"引用与声明对齐"，**不能证明类型正确**。补齐工具链后首个 `cargo check` 即暴露：

### 3.1 生产代码 11 处

| # | 错误码 | 位置 | 根因 | 修复 |
|---|--------|------|------|------|
| 1–2 | `E0308` | `session.rs:82,83`（`create`） | `derive_kek/derive_verify_key` 形参已放宽为 `&[u8]`，但实参传的是 `&Zeroizing<[u8;32]>`，与 `&[u8]` 不匹配 | `&mk` → **`&mk[..]`** |
| 3–4 | `E0308` | `session.rs:125,126`（`unlock`） | 同上 | `&mk` → **`&mk[..]`** |
| 5–6 | `E0308` | `session.rs:177,178`（`change_master_password` 校验旧密码） | 同上 | `&mk_old` → **`&mk_old[..]`** |
| 7–8 | `E0308` | `session.rs:189,190`（`change_master_password` 重派生 KEK） | 同上 | `&mk_new` → **`&mk_new[..]`** |
| 9–10 | `E0308` | `store/repository.rs:945,946`（`verify_master`） | 同上（`crate::crypto::kdf::derive_kek(&mk)`） | `&mk` → **`&mk[..]`** |
| 11 | `E0603` | `store/container.rs:25` | `FRAME_HEADER_LEN` 经 `crate::store::frame` 私有转导入，外部不可见 | 改为直接 `use crate::constants::FRAME_HEADER_LEN;` |

另 1 处**警告**：`sync/server.rs:23` 未使用的 `Hello` 导入 → 已从 `use crate::sync::protocol::{...}` 中移除。

> 根因归纳：**AEAD/KDF 的密钥参数从 `&[u8;32]` 放宽到 `&[u8]` 时，调用点漏了切片转换**。`Zeroizing<[u8;32]>` 的 `Deref` 只解一层（到 `[u8;32]`），不会自动再解到 `[u8]`。

### 3.2 测试代码 5 处

| # | 错误码 | 位置 | 根因 | 修复 |
|---|--------|------|------|------|
| 12–15 | `E0308` | `tests/crypto_vectors.rs:57,58,68,69` | `&mk` / `&wrong_mk` 传给 `&[u8]` 形参 | → `&mk[..]` / `&wrong_mk[..]` |
| 16 | `E0425` | `tests/container_roundtrip.rs:59` | 调用了不存在的 `keys::derive_kek`；且第 6 行未导入 `kdf` | 导入改为 `use passwordms_core::crypto::{kdf, keys, rng, verify};`，调用改 `kdf::derive_kek(&mk[..])` |

> 这些错误**此前完全不可见**：`cargo test` 在链接阶段就死了，测试源码根本没被完整类型检查。这正说明"没跑过 = 不知道"。

### 3.3 与 `03-impl.md` 的关系

| 文件 | 原结论 | 现结论 |
|------|--------|--------|
| `03-impl.md` §5 | `### → IS_PASS: YES`（第 1 轮即通过） | **已划掉并撤回**，改判 **NO**，并保留误判记录 |
| `03-impl.md` §6 | "测试文件已就位，**预期可直接运行**" | **已证伪**（测试文件本身含 5 个编译错误） |
| `03-impl.md` 交付声明 | "所有未被验证的结论均已如实标注" | 该句本身不成立（把静态审查当成了验证），**已重写** |

**沉淀为团队规范**：凡未经真实编译器 / 运行时执行的结论，**一律不得标注 PASS**；只能写"未验证"。静态审查结论必须显式标注"未经编译器"。

### 3.4 补完工具链后：`src-tauri` 外壳首次通过真实编译器

首轮 `cargo check --workspace` 在本机**编译不过**，原因不在代码而在**工具链缺件**（详见 §8）。补齐后实测：

```
$ cargo check --workspace --target x86_64-pc-windows-gnullvm
    Checking passwordms-core v0.1.0 (D:\Software\PasswordMS\crates\passwordms-core)
    Checking passwordms v0.1.0 (D:\Software\PasswordMS\src-tauri)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.80s
EXIT=0        # 0 error / 0 warning
```

> **意义**：这证明 **D-2（剪贴板按归属清空）的 Rust 侧代码在类型层面是自洽的**。此前工程师登记"`src-tauri` 未经编译验证"，本节把该登记项从"未验证"修订为"已通过编译器"。
>
> ⚠️ **但"能编译"≠"需求满足"**。QA 在 Phase 4b 用行为推演证明：D-2 的**「锁定兼容」分句（`01-security-req.md:97`）并未满足**——`vault_cmd::lock` 丢弃记忆值却不写空白剪贴板，导致倒计时到点后静默不清理。故 D-2 的准确状态是：**编译已验证 / 需求部分未满足**（详见 `04-qa.md` §4 与 §8 的 D-2b）。本节此前把它笼统写成"已验证"属措辞夸大，已更正。

---

## 4. Rust 测试：37 / 37 全通（原始输出）

```
     Running tests\container_roundtrip.rs      running 3 tests ... ok. 3 passed; 0 failed
     Running tests\crypto_vectors.rs           running 4 tests ... ok. 4 passed; 0 failed
     Running tests\health_and_generator.rs     running 5 tests ... ok. 5 passed; 0 failed
     Running tests\import_bitwarden.rs         running 3 tests ... ok. 3 passed; 0 failed
     Running tests\sync_conflict.rs            running 5 tests ... ok. 5 passed; 0 failed
     Running tests\sync_replay.rs              running 8 tests ... ok. 8 passed; 0 failed
     Running tests\timing_sidechannel.rs       running 1 test  ... ok. 1 passed; 0 failed   (69.5s)
     Running tests\unlock_error_probe.rs       running 1 test  ... ok. 1 passed; 0 failed   (6.0s)
     Running tests\zeroize_check.rs            running 7 tests ... ok. 7 passed; 0 failed
                                                              ----------------------------
                                                              合计 37 passed / 0 failed
```

> 计数变化：27（首轮）→ 28（D-1 叶子测试）→ 33（QA Phase 4 指出测试强度不足后补强）→ 35（QA Phase 4b 指出 `folders` / `pending_imports` 未擦后补强）→ **37**（QA Phase 4c 补 A-05 时序测试 + 解锁错误码探针，见 §10 的 D-10）。**每一次增加都必须由主理人或 QA 独立复跑核对，不接受"我加了 N 个用例"的口头声明。**

### 用例 ↔ 安全验收点对照

| 测试文件 | 用例 | 覆盖 |
|---------|------|------|
| `crypto_vectors.rs` (4) | `kdf_params_match_spec` | Argon2id 参数 m=64MiB/t=3/p=1 |
| | `aead_roundtrip_and_tamper_detection` | XChaCha20-Poly1305 往返 + 篡改必失败 |
| | `vault_key_wrap_roundtrip` | VK 包裹/解包 |
| | `unlock_failure_is_unified_error` | **A-05** 统一解锁报错（错密码与损坏不可区分） |
| `container_roundtrip.rs` (3) | `container_roundtrip_and_minimal_rewrite` | **A-01 / A-02** 容器往返 + 墓碑/追加分支 |
| | `single_item_update_hits_in_place_branch_and_leaves_others_intact` | **A-02** 等长**原位写入**分支 + 无关帧字节级逐字节不变 |
| | `item_json_roundtrip` | 条目序列化往返 |
| `zeroize_check.rs` (5) | `secret32_zeroizes` / `wipe_zeroes_buffer` / `session_lock_transitions_and_cascades` | **A-04** 清零原语 + 锁定级联 |
| | `item_wipe_plaintext_clears_sensitive_fields` | **A-04** 条目明文字段擦除（叶子函数级） |
| | `repository_wipe_plaintext_clears_item_read_from_repo` | **A-04** 擦除**覆盖面**（Repository 级：经仓库接口读回的条目字段确已变空） |
| | `repository_wipe_plaintext_clears_folder_fields` | **A-04** 分组名/图标/颜色擦除（**对抗实验证实：禁用该擦除后本用例变红**，见下） |
| | `import_preview_wipe_plaintext_clears_items_and_folders` | **A-04** `ImportPreview` 条目 + 分组明文擦除 |
| `sync_replay.rs` (8) | `replay_rejected_on_non_monotonic_seq` / `replay_rejected_outside_time_window` / `replay_rejected_after_clock_rollback` | **A-08** 防重放（序号单调 + ±120s 时间窗 + **时钟回拨**） |
| | `envelope_aad_binds_session_and_seq` | **A-08** AAD 绑定 `session_id‖seq‖ts`（三者各一变体，均须失败） |
| | `pair_token_rejections_via_server` / `pair_token_matches_semantics` | **A-06** 一次性令牌：已用 / 过期 / 不匹配的**行为**拒绝 |
| | `resync_unpaired_is_rejected` | **A-07** 未配对拒绝（**行为**测试，取代原先的枚举同义反复） |
| | `key_confirmation_mismatch_is_detected` | **S3** 双向 HMAC 密钥确认（防中间人） |
| `sync_conflict.rs` (5) | `conflict_when_both_changed_differently` / `no_conflict_when_only_one_side_changed` / `no_conflict_when_both_changed_to_same_content` | 冲突盒三态判定（**绝不静默覆盖**） |
| | `conflict_choice_serde` / `hash_is_stable` | 冲突选择序列化 + 内容哈希稳定 |
| `health_and_generator.rs` (5) | `health_detects_weak_reused_and_stale` / `health_ignores_non_login_items` | 密码体检（弱/重复/过期） |
| | `generator_respects_charset_and_length` / `generator_guarantees_each_selected_class` / `generator_rejects_empty_charset_and_bad_length` | 密码生成器边界 |
| `import_bitwarden.rs` (3) | `json_mapping_and_skip_count` / `csv_mapping_and_skip_count` | Bitwarden JSON/CSV 导入映射 |
| | `encrypted_export_is_rejected` | 加密导出必须拒绝（无法解密则不导入） |

> **⚠️ 测试强度的诚实边界（QA Phase 4b 对抗实验结论）**
> QA 把 `Session::lock` 里的 `wipe_plaintext()` 调用**注释掉**后重跑全量测试（当时为 33 个用例）：**33/33 仍全绿，无用例变红**。即：现有测试**抓不住"`wipe_plaintext` 没被调用"**。
> 原因不是漏洞——真正的兜底是 `impl Drop for Repository`（`repository.rs:1130`），`Repository` 被丢弃时先擦再放。所以删掉显式调用不至于泄漏明文，只是**该行为在测试中无法被观测**（Drop 之后读即 UB）。
> 因此上文对 `repository_wipe_plaintext_clears_item_read_from_repo` 的定位已修正为"证明**擦除覆盖面**"，不再声称它能证明"接线被调用"。
> 另：该用例只能证明「字段长度归零」，**不能证明底层缓冲被覆写**（`zeroize` 对 `String`/`Vec` 会 `shrink_to_fit()` 释放缓冲）。在**不触发 UB** 的前提下，"证明内存被覆写"不可行——该断言已是安全可达的上限，属**回归护栏级**，非取证级。

---

## 4b. 打包与体积红线实测（Windows 桌面）

**这是本项目第一次拿到真实的体积数字**——此前 `02-arch.md` §1.3 全部是设计期估算。

```
$ export PATH="/c/Users/HUAWEI/.workbuddy/tools/mingw-bin:$HOME/.cargo/bin:$PATH"
$ npx tauri build --target x86_64-pc-windows-gnullvm
```

| 产物 | 字节数 | 换算 | 红线 | 判定 |
|------|--------|------|------|------|
| 主程序 `passwordms.exe`（release, strip, LTO） | 4,122,112 | **3.93 MB** | — | — |
| **NSIS 安装包 `PasswordMS_0.1.0_x64-setup.exe`** | 1,549,027 | **1.48 MB** | ≤ 10 MB | ✅ **达标（余量 85%）** |
| **MSI 安装包 `PasswordMS_0.1.0_x64_en-US.msi`** | 2,215,936 | **2.11 MB** | ≤ 10 MB | ✅ **达标（余量 79%）** |
| **Android APK `app-universal-release-unsigned.apk`** | 6,595,076 | **6.29 MB** | ≤ 12 MB | ✅ **达标（余量 48%）**（详见 §4c） |

> 以上字节数均为 `stat -c%s` 直接读取，非文档转述。
> 两种 Windows 安装包格式**均已真实产出并通过门禁**（MSI 的离线绕行见下方代码块）。
>
> **⚠️ 产物新鲜度（本节数字在交付前经过一次专项审计，务必读完）**
>
> 首次打包后发现**产物与源码不同步**：MSI 构建于 10-13 20:00、APK 构建于 21:54，
> 而 `build.rs` 20:11、`src-tauri/Cargo.toml` 21:58（恢复 `cdylib`）、
> `crates/passwordms-core/src/session.rs` 21:59（D-10 修复）**都晚于它们**。
> 即：**这两个产物出自修复前的代码**。
> 判定方法不是看人怎么说，而是**逐产物比对 mtime 与源文件 mtime**
> （`stat -c '%s %y %n'`）——"构建成功"和"产物是新的"是两件事。
>
> 已全部重出，上表为**重出后**的实测值，四个产物的时间戳见 §4c.6。
> 结论：**任何早于其所依赖源码最后修改时间的产物，一律不得交付。**

门禁核验（`scripts/check-size.sh desktop` → exit 0）：

```
[size] 检查 Windows 安装包体积上限（≤ 10 MB）
[ OK ] 桌面安装包 体积达标：target/.../bundle/msi/PasswordMS_0.1.0_x64_en-US.msi（2.11 MB ≤ 10 MB）
[ OK ] 桌面安装包 体积达标：target/.../bundle/nsis/PasswordMS_0.1.0_x64-setup.exe（1.48 MB ≤ 10 MB）
[ OK ] 体积核验通过。
```

PowerShell 版（`scripts\check-size.ps1 -Target desktop` → PS_EXIT=0）输出同构，两个产物由**同一个门禁**一并核验（这是 B-2 修复后"合并门禁"语义的实证）。

> **与估算的对照**：`02-arch.md` 估算 Windows 安装包 3.5–5.2 MB；**实测 NSIS 仅 1.48 MB，优于估算下限**。原因是 `webviewInstallMode = downloadBootstrapper`（不内嵌 WebView2 运行时）+ release `opt-level="z"`/`lto`/`strip` 生效。

### 4b.1 打包过程中修掉的两个真实缺陷

| # | 现象 | 根因 | 处置 |
|---|------|------|------|
| B-1 | `rust-lld: error: too many exported symbols (got 69529, max 65535)` | `src-tauri/Cargo.toml` 声明 `crate-type = ["staticlib","cdylib","rlib"]`，而 **`cdylib` 只有移动端需要**；windows-gnu/gnullvm 下为它生成 dll 导出表时，GNU 版 `ld` **默认开启 `--export-all-symbols`**，会把整棵依赖树的全局符号全部导出，超出 PE 导出序号上限 | **中间方案（已废弃）**：先把 `crate-type` 降为 `["staticlib","rlib"]` 应急通过。**最终方案（当前代码）**：恢复 `["staticlib","cdylib","rlib"]`，改由 `src-tauri/build.rs` **按目标**注入 `cargo::rustc-link-arg-cdylib=--exclude-all-symbols`（仅 `x86_64-pc-windows-gnullvm`），见下 §4b.2 |
| B-2 | 体积门禁**假阴性** | 脚本只搜 `src-tauri/target/`，但 Cargo workspace 的实际产物目录是**根目录 `target/`** → 明明构建成功却被判"产物缺失" | 两个脚本都改为同时搜 `target/` 与 `src-tauri/target/`；并把门禁语义修正为"**该平台一个产物都没有 = 失败；有产物则逐个查体积**"（不再因缺少某一种打包格式而误判失败） |

### 4b.2 B-1 的最终解法：把"按目标生效"写进 `build.rs`，而不是靠切换 `crate-type`

B-1 的中间方案（砍掉 `cdylib`）能过桌面，但会**把 Android 打死**——移动端出 `.so` 必须靠 `cdylib`。于是产生一个"桌面 / 移动二选一"的伪两难：每次出 Android 包前手动加回 `cdylib`，出完再删掉。这种"打包前后手改 manifest"的做法极易漏改，且漏改的后果是**链接期报错**而非静默错误，属于必须消除的工艺缺陷。

最终解法把条件判断下沉到**随仓库提交**的构建脚本里：

```rust
// src-tauri/build.rs
fn main() {
    // 仅对 x86_64-pc-windows-gnullvm 生效：Android 目标用 NDK clang 链接器，
    // 该参数不会也不应该加进去（Android JNI 导出依赖默认的显式导出）。
    if std::env::var("TARGET").as_deref() == Ok("x86_64-pc-windows-gnullvm") {
        println!("cargo::rustc-link-arg-cdylib=--exclude-all-symbols");
    }
    tauri_build::build()
}
```

为什么**不能**放在 `.cargo/config.toml`：该文件含本机绝对路径（rustup 工具链位置、mingw 库目录），已按 §8 的策略排除出版本库（模板见 `.cargo/config.toml.example`）。把这条 target 相关链接参数放进 `build.rs` 之后，**新克隆的机器无需任何额外配置**即可同时编译桌面与移动端——这是 B-1 真正闭合的标志，而不是"本机现在能过"。

验证：桌面在**恢复 `cdylib` 之后**重新打包，`npx tauri build --target x86_64-pc-windows-gnullvm --bundles nsis` 返回 `EXIT=0`，说明 `--exclude-all-symbols` 确实只对 gnullvm 生效且足够（详见 §4b 表格下方的新鲜度说明）。

---

## 4c. Android 出包实测（首次真实产出 APK）

Android 侧**不是"设计通过"，而是真的装齐工具链、真的产出了 `.apk`**。

### 4c.1 环境（安装位置在本机 D 盘，避开系统盘）

| 组件 | 版本 | 路径 |
|------|------|------|
| JDK（Temurin） | `17.0.20.1+1` | `D:\AndroidDev\jdk17` |
| Android SDK（根） | — | `D:\AndroidDev\sdk` |
| ├ cmdline-tools | 12.0 | `D:\AndroidDev\sdk\cmdline-tools` |
| ├ platform-tools | 37.0.1 | `D:\AndroidDev\sdk\platform-tools` |
| ├ platforms | android-35 / 36 / 37.0 | `D:\AndroidDev\sdk\platforms` |
| ├ build-tools | 35.0.0 / 36.0.0 / 37.0.0 | `D:\AndroidDev\sdk\build-tools` |
| └ ndk | `27.2.12479018` | `D:\AndroidDev\sdk\ndk` |
| Gradle 家目录（缓存） | 9.6.1 | `D:\AndroidDev\.gradle` |

必需的 Rust 目标（`rustup target add`）：

```
aarch64-linux-android      # 主力 ABI（arm64-v8a）
armv7-linux-androideabi    # 32 位 ARM，可选
i686-linux-android         # 可选
x86_64-linux-android       # 可选
```

构建前需要的最小环境变量集（路径按上表）：

```bash
export JAVA_HOME="D:\\AndroidDev\\jdk17"
export ANDROID_HOME="D:\\AndroidDev\\sdk"
export NDK_HOME="$ANDROID_HOME\\ndk\\27.2.12479018"
export PATH="$JAVA_HOME\\bin:$PATH"
```

### 4c.2 命令序列

```bash
# ① 一次性生成 Android 工程（已执行；产物在 src-tauri/gen/android/ 且【已入库】，见 §4c.3）
npx tauri android init

# ② 出 APK（只出 arm64-v8a，控制体积）
npx tauri android build --apk --target aarch64
```

### 4c.3 ⚠️ 可复现性关键：`gen/android/` 为什么必须入库

`src-tauri/gen/android/` 是 `tauri android init` **生成**的目录，第一直觉是"生成物不入库"。这里**刻意反直觉地入库**（`.gitignore` 中显式写明），理由有三条，每条都踩过或将要踩：

1. **AndroidManifest.xml / MainActivity.kt / 图标 / res 资源** —— `init` 只生成一次，之后对 manifest 和图标做的任何定制，若不入库，换机器重跑 `init` 会**全部丢失**。
2. **镜像配置** —— `settings.gradle` 与 `build.gradle.kts` 里配置了阿里云 / 腾讯云 / 华为云 Maven 镜像，`gradle-wrapper.properties` 指向阿里云的 Gradle 9.6.1 分发包。若不入库，新克隆的机器会回落到 Maven Central 与 services.gradle.org，实测速度约 18 KB/s，**首次构建基本不可用**。
3. **入库不等于把构建垃圾也入库** —— 靠**嵌套 `.gitignore`** 分层拦截，而不是靠"整个目录不入库"这种粗暴手段：
   - `gen/android/.gitignore` → 拦 `build/`、`.gradle/`、`local.properties`、`key.properties`、`.tauri`
   - `gen/android/app/.gitignore` → 拦 `src/main/**/generated`、`jniLibs/**/*.so`、`assets/tauri.conf.json`

   > 实测佐证：`app/build/intermediates/...` 下的上千个中间文件（含 Gradle 变换缓存 JSON）**均未被 `git add -A` 收进暂存区**，而 `AndroidManifest.xml`、`gradlew.bat`、`gradle-wrapper.jar` 等 41 个工程文件正常入库。

4. **`gradlew.bat` 必须保持 CRLF** —— `cmd.exe` 解析批处理的多行块与 `goto` 标签依赖 CRLF，转成 LF 会出现难以定位的诡异错误。已在 `.gitattributes` 中对 `*.bat` / `*.cmd` 单独钉死 `eol=crlf`（`gradlew` 无扩展名，保持 LF）。这是"全局 LF"策略的必要例外。

   > 附带一条实测观察：`gen/android/` 下另有 **22 个工程文件**（`*.gradle.kts` / `*.xml` / `*.kt` / `.gitignore` / `*.properties`）在**本机工作区是 CRLF**（Tauri 在 Windows 上生成时留下的），但**在索引里已被规范成 LF**。`git ls-files --eol` 显示它们为 `i/lf w/crlf`，且 `git status` **判定为干净**——因为 `text=auto eol=lf` 会在比对前先把工作区内容转成 LF。这不影响 Android 构建（Gradle/XML/Kotlin 对换行不敏感），只是提醒后续维护者：**"工作区看上去是 CRLF"不等于"入库是 CRLF"**，判断标准永远是 `git ls-files --eol` 的第二列。

**换行属性自检结果**（`.gitattributes` 生效证据）：

```
$ git ls-files --eol | awk '{print $1, $2}' | sort | uniq -c
     21 i/-text w/-text     # 二进制（图标 / .jar），明确按二进制处理
     23 i/lf    w/crlf      # 批处理 1 个 + 上段所述 22 个；入库均为 LF
    152 i/lf    w/lf        # 其余文本，入库与工作区同为 LF
```

### 4c.4 产物与体积

| 产物 | 字节数 | 换算 | 红线 | 判定 |
|------|--------|------|------|------|
| `app-universal-release-unsigned.apk` | 6,595,076 | **6.29 MB** | ≤ 12 MB | ✅ **达标（余量 48%）** |

> **⚠️ 这个数字改过一次，原因值得记住**：最初登记的"1.62 MB"量的是一个**残缺包**
> （原生库为空壳），见 §4c.5 的 D-17。**达标结论不变，但余量从 87% 降到 48%** ——
> 若按错误的 1.62 MB 去反推"还能再加多少功能"，会严重误判余量。

APK 内含 ABI（用 Python `zipfile` 独立复核，非文档转述）：**仅 `lib/arm64-v8a/libpasswordms_lib.so`**，总条目 688 个。输出目录名为 `universal`，但实际只含 arm64-v8a，符合 `--target aarch64` 的预期。

**原生库完整性硬证据**（这是"包是不是假的"的唯一判据）：

```
=== 2. 全量扫描 lib/ 下的 .so ===
  总条目数        : 688
  [OK ] lib/arm64-v8a/libpasswordms_lib.so
        压缩后 4,895,912 B / 解压后 4,895,912 B
=== 3. 从 APK 内读出 .so 并做哈希/magic 校验 ===
  读出的 .so 长度 : 4,895,912 B
  前 4 字节       : 7f454c46          ← ELF 魔数
  包内 .so sha256 : 6cc3ccbca1c839f349b12223630256fdaecd142098170b97c693ba3f0a5ac4a5
  构建产物 sha256 : 6cc3ccbca1c839f349b12223630256fdaecd142098170b97c693ba3f0a5ac4a5
  一致性          : ✅ 完全一致
=== 4. 签名状态 ===
  签名相关条目    : （无 → 确实未签名）
```

> **判据说明**：**只看 APK 体积是不可靠的**。空壳 APK 与正常 APK 都会报"体积达标"。
> 唯一可信的判据是**解出包内的 `.so`，断言其长度 > 0、头部为 ELF、且哈希与构建产物一致**。
> 这三条现在都已实测通过。

### 4c.5 🔴 缺陷 D-17：Tauri 产出的 APK 原生库为空（已闭合；附一次自我纠错）

> **编号说明**：本节缺陷编号为 **D-17**。仓库中曾一度写作 "D-11"，但 **D-11 已被 `03-impl.md` 占用**（`sync_now` 直连重同步，见 `03-impl.md:130`）。为避免两个不同议题共用一个编号，此处改用 **D-17**（全仓库此前已用到 D-16）。

**这是本轮最严重的一个发现，且它差点被"体积达标"这个绿灯掩盖过去。**

| 项 | 内容 |
|---|---|
| **现象** | `npx tauri android build` 产出的 APK 里，`lib/arm64-v8a/libpasswordms_lib.so` 的**压缩前后长度均为 0** —— 是个**没有原生代码的空壳包** |
| **危害等级** | **致命**。1.62 MB 的体积完全落在 12 MB 红线内，门禁**全绿通过**；但装到真机会因缺少原生库**直接崩溃**。若不做 §4c.4 那三层校验，这个包会被当成"Android 端已可用"交付出去 |
| **发现方式** | **不是自动化测出来的**，是复核时对 `.so` 条目"读出来 0 字节"起疑、手工深挖出来的 |
| **触发机制** | `cargo-mobile2`（Tauri 移动端构建的实际执行者，经 Gradle 的 `rustBuild*` 任务调用）要把编译好的 `.so` 放进 `jniLibs/<abi>/`，做法是**创建符号链接**。构建日志可直接看到该动作：`Info symlinking lib "…/libpasswordms_lib.so" in jniLibs dir "…/jniLibs/arm64-v8a"` |
| **根因（原生证据）** | **本机进程无法创建符号链接**。原生 PowerShell `New-Item -ItemType SymbolicLink` 在三个位置（仓库 `.git` 目录、其子目录、系统临时目录）**全部失败**：<br>`UnauthorizedAccessException / HRESULT=0x80070005 / "此操作需要管理员权限。"`<br>同时实测：`AppModelUnlock` 注册表键**不存在**（= Windows 开发者模式**未开启**），`IsAdmin=False`。<br>→ 进程不持有 `SeCreateSymbolicLinkPrivilege`。**符号链接创建被系统拒绝后，`jniLibs` 里留下一个 0 字节文件，AGP 把这个空文件打进 APK。** |
| **产物形态（现场原生取证）** | 在上游刚打印 `symlinking lib …` 的那一刻抓取该文件：<br>`Length=0`、`Attributes=Archive`、`LinkType=(空)`、`IsReparsePoint=False`、可打开、可读长度 0<br>→ 它是一个**货真价实的 0 字节普通文件**，**既不是符号链接，也不是断链**。（对照：真实库 `len=4895912`，`attr=Archive, NotContentIndexed`） |
| **处置** | 绕开该软链步骤：**复用上游已产出的真实库** `target/aarch64-linux-android/release/libpasswordms_lib.so` → **硬拷贝**进 `jniLibs/arm64-v8a/` → 直接调 Gradle 打包并 `-x` 掉全部 `rustBuild*` 任务。见 §4c.2 命令序列 |
| **⚠️ 修复路径的禁忌** | **不能**改成自己调 `cargo build --target aarch64-linux-android`。该目标的链接器由 **Tauri 在构建时以环境变量注入**，`.cargo/config.toml` 里并没有对应 `linker` 配置；自行调用会因找不到链接器而失败。所以只能**复用**上游那一次编译的产物 |
| **⚠️ 五个 `-x` 一个都不能漏** | `rustBuildUniversalRelease` / `rustBuildArm64Release` / `rustBuildArmRelease` / `rustBuildX86Release` / `rustBuildX86_64Release`。漏掉哪个，哪个任务就会重新执行软链步骤，把 `jniLibs` 再次覆盖成 0 字节 |

#### 4c.5.1 ⚠️ 自我纠错：本文档一度写下过一个**错误结论**

这一段保留下来，因为**过程本身是本次最值得记住的教训**。

调查途中，本文档曾写下："**『本机不能建符号链接』这个说法是错的**……`os.symlink` 在项目目录与系统临时目录均成功，故**开启开发者模式解决不了**"。**该结论是错误的，现已撤回。** 错在方法：

- 当时判定"成功"的依据，仅仅是 **`os.symlink` 没有抛异常**——**从未验证产物本身**。重跑同一脚本时，它在紧随其后的 `os.lstat()` 处直接崩 `FileNotFoundError`：**`os.symlink` 既不抛异常、也什么都没创建**，是**静默空操作**（本机 Bash 工具所处的沙箱 shell 带有文件系统垫片，`os.symlink` 被拦截）。用它来判断本机符号链接能力，得到的观测**不可信**。
- 同一错误还派生出一个次要错判：一度把产物描述为"**坏链接 / 断链**"，由此推出"AGP 顺着断链读到 0 字节"。原生取证表明产物是**普通文件**，不存在断链，该描述同样不准确。

**教训（两条，通用）**：
1. **"调用没有报错"不等于"操作成功"。** 判定成功必须**验证产物**——文件存不存在、长度对不对、内容能不能读出来。这与本项目的核心教训是同一条：**构建返回 0 ≠ 产物可用**。
2. **沙箱化的 shell 会静默吞掉部分系统调用。** 涉及权限、符号链接、注册表、进程等**系统级能力**的判断，必须用**原生进程**（如直接跑 `powershell.exe`）取证，不能拿沙箱 shell 里的 Python/Node 观测当结论。

> 这次纠错由 **QA 独立质疑**触发——QA 指出"Python 不抛异常但产出 0 字节"与"原生 PowerShell 报 `E_ACCESSDENIED`"相互矛盾，且质疑沙箱垫片干扰。该质疑成立。**这正是"验证者必须换角度攻击、而不是复跑实现者跑过的路径"的价值。**

**⚠️ 残留风险与兜底（现状）**：

> **上游缺陷本身没有根除**：只要再跑一次 `npx tauri android build`，`jniLibs` 下的真实 `.so` 就会被重新生成的那个 0 字节文件覆盖，包立刻变回空壳。
> 但**这条路径已被脚本封死**——`scripts/build-android.sh` 已改造为：
>
> **上游构建 → 解包校验 →（失败则）自动修复 → 二次校验 → 体积核验**
>
> 校验对**每一条**期望条目做四条断言，缺一不可：
> ① 条目存在；② 长度 > 0；③ 前 4 字节为 `7f454c46`（ELF）；
> ④ **sha256 与 `target/aarch64-linux-android/release/libpasswordms_lib.so` 一致**。
> 修复后**再校验一次**，仍失败则**非零退出**——`die`，绝不放过。
> 因此**谁跑这个脚本都不会再拿到空壳包**；而"构建命令返回 0"依旧**不是**判据。
>
> 第 ④ 条是 QA 复验时提出的补强：只查"非空 + ELF 头"挡不住"ELF 头正确但内容不对"的包（例如用别的库、或截断后仍是 ELF 的文件）。加上哈希一致性后，**包内原生库必须与本次构建产物逐字节相同**。
>
> **为什么体积门禁挡不住它**：空壳包因为少了 4.89 MB 原生库，体积**反而更小**（1.62 MB vs 正常 6.29 MB），12 MB 红线只会更轻松地放行。**门禁越绿越可疑**——这是 D-17 最反直觉的地方。

**实验记录**（证明这不是偶发，也证明修复是必要的）：手工先放入真实的 4.89 MB `.so`，再执行一次 `npx tauri android build` → `jniLibs` 里的文件**变回 0 字节**。即**每次构建都覆盖**，不是一次性事故。

**⛔ 尚未验证的替代解法**：**开启 Windows 开发者模式（或以管理员权限运行）后，上游 `npx tauri android build` 是否能直接产出正常包？** 若能，则本节的绕行脚本只是"不依赖系统设置"的兜底，而非唯一解。**本会话未做该实验**（需管理员权限，且会改动系统设置），故如实记为**未验证**。

### 4c.6 四个产物的时间戳台账（交付前的核验凭据）

```
      4122112  2026-10-14 07:05:32  target/x86_64-pc-windows-gnullvm/release/passwordms.exe
      1549027  2026-10-14 07:05:27  target/x86_64-pc-windows-gnullvm/release/bundle/nsis/PasswordMS_0.1.0_x64-setup.exe
      2215936  2026-10-14 07:05:30  target/x86_64-pc-windows-gnullvm/release/bundle/msi/PasswordMS_0.1.0_x64_en-US.msi
      6595076  2026-10-14 07:22:50  src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk
```

**四个产物全部晚于所属源文件的最后修改时间**（最晚的源码改动是 `session.rs` 21:59 / `Cargo.toml` 21:58，均为 10-13），因此**均包含 D-1b / D-2b / D-3b / D-10 全部修复**。

门禁复核（`scripts/check-size.sh all` → `SIZE_SH_EXIT=0`，主理人独立复跑）：

```
[size] 检查 Windows 安装包体积上限（≤ 10 MB）
[ OK ] 桌面安装包 体积达标：…msi（2.11 MB ≤ 10 MB）
[ OK ] 桌面安装包 体积达标：…nsis（1.48 MB ≤ 10 MB）
[size] 检查 Android APK 体积上限（≤ 12 MB）
[ OK ] APK 体积达标：…app-universal-release-unsigned.apk（6.29 MB ≤ 12 MB）
[ OK ] 体积核验通过。
```

PowerShell 版（`scripts\check-size.ps1 -Target all` → `PS_EXIT=0`）输出同构。

### 4c.7 ⛔ 如实登记的 Android 侧限制（未做，不得当已完成）

| # | 限制 | 影响 | 现状 |
|---|------|------|------|
| A-1 | **APK 未签名** | 无法直接装到真机；也无法上架 | 已复核：`META-INF/` 下**无任何签名条目**，文件名本身即 `-unsigned`。**未配置 keystore，未跑 `apksigner`**。装真机需先配签名，或改用 debug 构建（`npx tauri android build --apk -d`）—— debug 版的 debug keystore 由 Gradle 自动生成 |
| A-2 | **真机 / 模拟器未运行过** | "能编译出 APK" ≠ "App 能启动、能解锁、能同步" | 本机无可用设备，**启动与功能全部未验证** |
| A-3 | **局域网同步端到端未做** | P1 的核心卖点未被验证 | 需要第二台设备（另一部手机或 PC），见 §7 |
| A-4 | **仅 arm64-v8a** | 32 位 ARM 老机型装不上 | 本轮只出 aarch64；如需覆盖老机型再加 `armv7-linux-androideabi` 目标重出 |
| ~~A-5~~ | ~~`scripts/build-android.sh` 仍会产出空壳 APK~~ | **已闭合**：脚本改造为「上游构建 → 解包校验 → 失败自动修复 → 二次校验」，成品由**二次校验**把关，不过即非零退出 | 见 §4c.5 |
| A-6 | **上游 Tauri 的空壳包缺陷仍在** | 直接手敲 `npx tauri android build` 仍会拿到空壳包 | 绕行已固化进脚本，但上游缺陷未修；升级 Tauri 后应重新确认该分支是否还需要 |
| A-7 | **未验证：开启开发者模式后上游命令能否直接出正常包** | 若成立，则脚本的绕行只是"不依赖系统设置"的兜底，而非唯一解 | 需管理员权限且会改动系统设置，**本会话未做该实验** |

> **结论口径**：Android 侧当前可声明的成绩是"**工具链打通 + 工程可复现 + APK 产出，且原生库经 sha256 与构建产物逐字节比对**"；**仍不等于"Android 端可用"**。签名、真机启动、局域网同步三项均未验证。而 D-17 的存在提醒：**"构建返回 0"和"包能用"是两件事**。

### 4c.8 `scripts/build-android.sh` 端到端实测（主理人独立复跑 4 次）

脚本改造后由主理人**亲自跑了 4 次**，每次都走完「上游构建 → 校验失败 → 自动修复 → 二次校验通过」全程：

| 运行 | 目的 | 结果 |
|------|------|------|
| 第 1 次 | 只剩 `cargo` 不在 `PATH` | **`EXIT=1`**，前置检查给出明确 `[fatal]` 提示——**顺带验证了守卫子句有效、不会硬闯** |
| 第 2 次 | 验证 `verify_apk` + 自动修复 + `locate_apk` 改动 | **`EXIT=0`**，APK 6,595,076 B |
| 第 3 次 | 验证新增的 sha256 断言（第 ④ 条） | **`EXIT=0`**，APK 6,595,076 B |
| 第 4 次 | 验证 **F3 修复**（降级路径改为 fail-closed、后缀只在四条全过时打印） | **`EXIT=0`**，APK 6,595,076 B；降级 warn 出现 **0** 次、`（4/4）` 标记出现 **1** 次 |

第 4 次运行的关键输出（原文摘录）：

```
[FAIL] 条目缺失或为空：lib/arm64-v8a/libpasswordms_lib.so（unzip/python/tar 均未解出内容）
[warn ] APK 完整性校验失败：上游产出空壳包 → 进入自动修复分支（D-17）。
[android] 真实库：target/aarch64-linux-android/release/libpasswordms_lib.so（4895912 字节）→ 硬拷贝进 .../jniLibs/arm64-v8a/
[android] Gradle 重新打包（-x 跳过全部 rustBuild* 任务，避免其把 jniLibs 再次覆盖为 0 字节）
BUILD SUCCESSFUL in 9s
[android] 修复后 APK 路径：.../app-universal-release-unsigned.apk
[ OK ] lib/arm64-v8a/libpasswordms_lib.so  字节=4895912  magic=7f454c46  工具=unzip  sha256=6cc3ccbc…a5f4  已比对构建产物（4/4）
[android] 修复后二次校验通过。（提示：此分支因上游 symlink 缺陷而存在，若 Tauri 修复后可移除。）
[android] APK：...（6.29 MB / 6595076 字节）
[ OK ] 体积核验通过（≤ 12 MB）。
SCRIPT_EXIT=0
```

> `已比对构建产物（4/4）` 这个后缀是**第 ④ 条 sha256 断言真正执行并通过对**的凭据。
> **它只在四条断言全部执行且通过的那一行打印**——降级情形（构建产物缺失 / 哈希工具不可用）
> 一律 `fail` + 非零返回，**不会**出现该后缀（详见下方 F3）。

#### 4c.8.1 F3：一个由 QA 发现的「假绿通道」（已修复）

校验逻辑第一版里，`已比对构建产物` 后缀是**无条件硬编码打印**的：当 `$REAL_SO` 不存在或 `sha256sum` 不可用时，脚本只打一行 `[warn]` 说明"仅完成三条断言"，**却仍然打印后缀并返回 0**。后果是：**同一条目同时声称"没比对"和"已比对"，并把一个"ELF 头正确、内容任意"的坏包放行**——门禁静默从四条降级成三条，还宣称做了四条。

**这与 D-17 本身是同一类缺陷："声称做了、其实没做、然后放行"。**

修复：
1. **删掉整条降级 `warn` 路径**；构建产物缺失 / 哈希算不出来 → 一律 `fail` + `rc=1`，走不到 `ok` 行。
2. 后缀改为只在四条全过时打印，并显式标注 `（4/4）`。
3. 哈希工具缺失 → **前置检查直接 `die`**，不允许静默降级。
4. 新增 `sha256_of()` 回退链 `sha256sum → shasum → python hashlib`（原先只有 `sha256sum` 一条，缺了即退化）。

**回退链已实测**（把 `sha256sum` 与 `shasum` 同时从 `PATH` 中屏蔽，只留 python）：

```
sha256sum -> 无
shasum    -> 无
python    -> .../3.13.12/python
sha256_of rc=0
sha256_of 结果 = 6cc3ccbca1c839f349b12223630256fdaecd142098170b97c693ba3f0a5ac4a5
对照（同一文件用 python 直接算）= 6cc3ccbca1c839f349b12223630256fdaecd142098170b97c693ba3f0a5ac4a5
```

→ 回退分支确实能算出正确哈希，**第 ④ 条断言不会因为缺 `sha256sum` 而失效**。

> 这次的教训再次印证：**"门禁通过"本身也需要被证伪。** 一个门禁只要存在"降级但仍报通过"的路径，它就等于不存在——而且比不存在更危险，因为它给人虚假的安全感。
>
> 追加一点：**这条 F3 反例是 QA 主动构造出来的**（`$REAL_SO` 指向不存在的路径），并做了「修复前 vs 修复后」对照。修复前它成功复现了"`rc=0` + 矛盾两行 + 带后缀"，修复后变为"`rc=1` + 无后缀 + 命中指定报错文本"。**若只让验证者确认"正常路径能过"，这条假绿通道会一直潜伏到某天 `target/` 被 `cargo clean` 之后。**

独立解包复核（Python `zipfile`，与脚本所用工具链无关）：APK 6,595,076 B、包内 `.so` 4,895,912 B、前 4 字节 `7f454c46`、sha256 `6cc3ccbca1c839f349b12223630256fdaecd142098170b97c693ba3f0a5ac4a5`，与 `target/aarch64-linux-android/release/libpasswordms_lib.so` **完全一致**；`META-INF/` 下**无签名条目**。

> **⚠️ 这批证据的边界**：以上证明的是「**脚本能稳定产出原生库正确的包，并且能抓住空壳包**」。
> **不**证明 APK 能在真机上跑起来——签名、安装、启动、解锁、局域网同步**全部未验证**（见 §4c.7）。

---

## 5. Rust 静态检查：clippy 0 errors / 3 warnings

```
cargo clippy -p passwordms-core --target x86_64-pc-windows-gnullvm --all-targets
cargo clippy --workspace    --target x86_64-pc-windows-gnullvm      # 含 src-tauri 外壳
```

| # | 级别 | 位置 | 内容 | 处置 |
|---|------|------|------|------|
| 1 | warning | `src/session.rs:23` | `large_enum_variant`：`SessionState` 变体体积差异大 | 待评估（`Unlocked` 内含 `Repository` 属必然），**不阻塞** |
| 2–3 | warning | `src/store/repository.rs:704,764` | `unnecessary_sort_by`：建议改 `sort_by_key` | 可一键改，**不阻塞** |

**0 errors。** 均为风格类告警，不影响正确性与安全。

> 计数变化：首轮 4 条 → 现 **3 条**。减少的那条是 `tests/sync_replay.rs:61` 的 `assertions_on_constants`——QA 指出它是**恒真断言**后，工程师已将该用例替换为真实的 TCP 行为测试，告警随之消失。**这正说明"加强测试"与"消除静态告警"可以是同一件事。**


**0 errors**。这 4 条均为风格类告警，不影响正确性与安全。是否清理交由 Phase 4 QA 裁定。

---

## 6. 前端验证（三项全通）

| 检查 | 命令 | 原始输出（摘要） | 判定 |
|------|------|-----------------|------|
| 类型检查 | `npm run check` | `svelte-check found 0 errors and 0 warnings` | ✅ |
| 单测 | `npx vitest run` | `Test Files 8 passed (8)` / `Tests 34 passed (34)` | ✅ |
| 构建 | `npm run build` | `✓ 185 modules transformed` / `✓ built in 3.26s` | ✅ |

**vitest 8 个测试文件**：`i18n-usage`(2)、`i18n`(4)、`clipboard`(5)、`url-safety`(5)、`import`(5)、`conflict`(6)、`stores/sync`(4)、`ipc-contract`(3) = **34 用例**。

**Vite 构建产物**：

```
dist/index.html                0.54 kB │ gzip:  0.40 kB
dist/assets/style-*.css       26.43 kB │ gzip:  4.63 kB
dist/assets/index-*.js       271.08 kB │ gzip: 95.47 kB
```

> 前端 JS 271 KB（gzip 95 KB）为 Svelte 5 运行时 + 全部页面逻辑，**无 UI 组件库、无图标包**，符合"足够小"的目标。

---

## 7. ⛔ 未执行的验证（如实登记，未做任何推断）

| 未执行项 | 阻塞原因 | 影响 | 建议 |
|---------|---------|------|------|
| ~~`tauri build`（Windows 桌面）~~ | **已完成**（见 §4b） | ~~体积红线未实测~~ → **已实测：1.48 MB ≤ 10 MB** | — |
| ~~MSI 打包（`--bundles msi`）~~ | **已完成**（见 §4b）：经 GitHub 加速代理取包并手工预热 `%LOCALAPPDATA%\tauri\WixTools314\`，Tauri 检测到缓存即跳过下载 | ~~MSI 未产出~~ → **已产出：2.11 MB，门禁通过** | — |
| ~~Android 打包~~ | **已完成**（见 §4c） | ~~无法产出 APK~~ → **已产出 6.29 MB（原生库经哈希校验非空）** | — |
| ~~体积红线（APK）~~ | **已完成**：6.29 MB ≤ 12 MB | — | — |
| Android APK 签名 / 真机安装 | 无 keystore、无可用设备 | **未验证**：产物为 `-unsigned`，真机装不上、启动未跑过 | 见 §4c.7 |
| 真机局域网同步联调 | 需 Windows + Android 双端产物 | **端到端同步未实测**（协议层单测已过） | 待双端可运行后补 |
| `cargo fmt --check` | 未安装 `rustfmt` 组件 | 格式规范未机器校验 | 可选 |
| 内存取证级清零（A-04） | 需外部工具（如 gdb/WinDbg）验证真实内存页 | 现有测试验证的是**擦除覆盖面（字段清空）**，非页级取证；且 QA 已证实它**抓不到"wipe 未被调用"**（靠 `Drop` 兜底） | 安全审计阶段可补 |
| ~~A-05 时序侧信道~~ | **已完成**（见 §10 / `04-qa.md` Phase 4c） | ~~未验证~~ → **已实测**：错误密码 median 1868.4ms、数据篡改 median 1877.6ms，比值 0.995（两次独立运行 0.960 / 0.995），两条路径均 ≈ 99% KDF 基准 → **无提前返回** | — |
| A-10 二次确认（UI）交互 | 未做人工 UI 走查 | **未验证** | 待双端可运行后补 |

> **重要**：以上均为"**未验证**"，既非"通过"也非"失败"。任何把它当作"通过"的表述都是错误的（这正是 §3 所纠正的错误类型）。

---

## 8. 附：非管理员 Windows 上的 Rust 工具链搭建（踩坑记录）

本机**无 Visual Studio Build Tools、无 mingw**，且沙箱对部分操作有限制。为让"代码能被真实编译"成为可能，主理人完成如下引导，**结果已固化进 `.cargo/config.toml`**，后续任何人 clone 后可直接 `cargo test`：

1. **rustup 代理 0 字节**：在沙箱内运行 `rustup-init.exe` 导致 `~/.cargo/bin/{cargo,rustc,...}.exe` 全部为 0 字节（硬链接被拦）。→ 修复：`cp -f rustup.exe cargo.exe` 等，逐个替换为同一份二进制（rustup 多路复用）。
2. **`dlltool.exe: program not found`**：`x86_64-pc-windows-gnu` 目标的 `getrandom` 走 raw-dylib，需要 dlltool；自带的是 LLVM 版且不认 rustc 的 `--64` 参数。→ **改用 `x86_64-pc-windows-gnullvm` 目标**（LLVM ABI，无需 mingw）。
3. **链接器缺失**：目标默认找 `x86_64-w64-mingw32-clang`。→ 指定 `linker = "rust-lld"`。
4. **`unable to find library -lkernel32`**：→ 追加 `-L native=<rustlib>/x86_64-pc-windows-gnu/lib/self-contained`。
5. **`unable to find library -lunwind` / 未定义 `_Unwind_*`、`_GCC_specific_handler`**：→ 以 `libgcc_eh.a` 内容构造 `libunwind.a`，置于 `C:\Users\HUAWEI\.workbuddy\tools\wsllibs\`。
6. **未定义 `__p___initenv` / `_crt_atexit`**（工具链内不存在）：→ 手写 LLVM IR shim（`shim.ll`）→ `llvm-as` / `llc` → `llvm-ar` 合并进 `libunwind.a`。
7. **`cargo test` 卡死 9 分钟**：增量编译锁文件删除被拒（`os error 5`）。→ `incremental = false`（已写入 `.cargo/config.toml`）。

最终 `.cargo/config.toml`：

```toml
[target.x86_64-pc-windows-gnullvm]
linker = "rust-lld"
rustflags = [
    "-L", "native=C:\\Users\\HUAWEI\\.rustup\\toolchains\\stable-x86_64-pc-windows-gnu\\lib\\rustlib\\x86_64-pc-windows-gnu\\lib\\self-contained",
    "-L", "native=C:\\Users\\HUAWEI\\.workbuddy\\tools\\wsllibs",
    "-C", "link-self-contained=yes",
]
[build]
incremental = false
```

### 8.1 补记（2026-10-13 晚）：让**主机侧**也能编译 —— `src-tauri` 的最后一道坎

上面 1–7 只解决了**目标侧**（`x86_64-pc-windows-gnullvm`）的链接。但 `cargo check --workspace` 依然失败，因为 **`tauri-build` 是构建依赖**，它连同 `windows-sys` / `getrandom` / `parking_lot_core` 必须按**主机三元组**（`x86_64-pc-windows-gnu`）编译，而 gnu 目标下的 `raw-dylib` 需要 `dlltool`。逐个攻破：

| # | 现象 | 根因 | 处置 |
|---|------|------|------|
| 8 | `dlltool.exe: program not found` | `dlltool` 不在 PATH（rustc 只按 PATH 找它，不会自动搜 self-contained） | 把工具目录加入 PATH |
| 9 | `dlltool could not create import library ... CreateProcess` | **`dlltool` 会去自己的目录找 `as` 汇编器，而 Rust 自带的 `rust-mingw` 里根本没有 `as.exe`**；其 `x86_64-w64-mingw32-gcc.exe` 是**纯链接器残桩**（`GCC-WARNING.txt` 自认"不能编译 C 文件"），也调不动 `as` | 从 MSYS2 取来 **静态链接、零 DLL 依赖**的 `as.exe` + `dlltool.exe`，放 `C:\Users\HUAWEI\.workbuddy\tools\mingw-bin\`；并把 `as.exe` 补进 self-contained（那正是 dlltool 硬编码要找的位置） |
| 10 | `tauri-winres` panic：`NotAttempted("windres")` | Tauri 要用资源编译器嵌入图标与 manifest，本机没有 `windres` | 同包内的 `windres.exe` 一并放入 `mingw-bin` |
| 11 | `windres: preprocessing failed` + `'gcc' 不是内部或外部命令` | **GNU windres 总会先跑 C 预处理器**（默认 `gcc -E -xc-header -DRC_INVOKED`），而本机没有 C 编译器 | 见下 |

**关于 11 的处置（不伪造 `gcc`）**：先读了 tauri 生成的 `resource.rc`，确认它**只有 `#pragma code_page(65001)`，没有任何 `#include`/`#define`**，即预处理只需**逐字节透传**。也确认了 windres **没有**可跳过预处理的环境变量。于是自建了一层 `windres` 包装器（源码 `C:\Users\HUAWEI\.workbuddy\tools\mingw-bin\src\windres_wrapper.rs`，用 rustc 编译）：

- 真 windres 改名为 `windres-real.exe`；
- `windres.exe` 是我们的包装器：注入 `--preprocessor=<自身> --preprocessor-arg=--rc-passthrough` 后转发；
- 带 `--rc-passthrough` 标记时，自身扮演透传预处理器（文件 → stdout 原样拷贝）。

**刻意不提供名为 `gcc.exe` 的假编译器**——那会让其他构建系统误判"C 编译器可用"。另外踩到一个 Windows 坑：`std::env::current_exe().canonicalize()` 会返回 `\\?\C:\...` 扩展路径前缀，windres 用字符串拼接命令行时会因此报"系统找不到指定的路径"，故**必须去掉 `canonicalize()`**。

**最终可用环境**（写进任何构建脚本即可复现）：

```bash
export PATH="/c/Users/HUAWEI/.workbuddy/tools/mingw-bin:$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0
cd "D:/Software/PasswordMS"
cargo check --workspace --target x86_64-pc-windows-gnullvm   # EXIT=0
```

工具目录 `mingw-bin` 仅含 4 个文件：`as.exe`、`dlltool.exe`、`windres-real.exe`、`windres.exe`（+ `src/`）。

---

## 9. 移交 Phase 4（QA 严过关）的清单

请 QA 在**独立复现**（不看本文件结论、自行运行）的前提下，重点核验：

1. `cargo test -p passwordms-core` 是否同样 27/27 —— **禁止只信本文档**。（QA 已复现；后经两轮补强，现为 **37/37**）
2. 上述 §4 安全验收点，是否**真的被断言覆盖**（逐条读测试代码，判断断言强度，而非只看用例名）。
3. §7 的未验证项是否被误当作已验证 —— 检查 `03-impl.md` / `03b-verification.md` / 交付说明中是否存在"伪 PASS"残留。
4. 体积红线：在拿到 Tauri 产物后实测 Windows 安装包与 APK 体积。
5. 对抗性测试：A-05（统一报错）能否被计时/错误码侧信道区分；A-08（防重放）在时钟回拨下的行为。

**§9 执行结果**：QA 已按本清单完成两轮独立复验，全部结论落在 **`docs/04-qa.md`**（含 Phase 4 与 Phase 4b 两章）。两轮结论均为 **CONDITIONAL PASS**。

---

## 10. Phase 4b 复验发现的问题与闭合状态（权威清单在 `04-qa.md`）

> 本节只做**索引**，不作为判定依据；逐条证据见 `04-qa.md` 的 Phase 4b 章节。以下三项已由主理人**对着源码逐行复核确认属实**。

| 编号 | 级别 | 事实（file:line 已核实） | 状态 |
|------|------|------------------------|------|
| **D-1b** | 🔴 中高 | `Repository::wipe_plaintext`（`store/repository.rs:1031-1042`）只遍历 `items`/`trash`/`conflicts`，**未擦 `self.folders`**；而 `01-security-req.md:70` 明确要求"分组名称、图标、颜色"应加密，`model/folder.rs:1` 亦自述"整条内容均加密" → 锁库后分组名仍是裸 `String` | ✅ **已修复**：新增 `Folder::wipe_plaintext()`（`model/folder.rs`）+ 在 `Repository::wipe_plaintext` 中补遍历 `folders`；新增用例 `repository_wipe_plaintext_clears_folder_fields`，**经对抗实验证实（禁用该擦除 → 用例 `zeroize_check.rs:244` panic 变红）** |
| **D-2b** | 🔴 高 | `vault_cmd::lock`（`commands/vault_cmd.rs:56-59`）把 `state.clipboard` 置 `None` 却**不写系统剪贴板**；前端唯一调用点在倒计时 `onDone`（`src/lib/clipboard.ts:68`）→ 复制后 10s 锁库、30s 到点时因记忆值缺失而 `Ok(false)`，**密码永久残留在系统剪贴板**。违反 `01-security-req.md:97`「锁定兼容」 | ✅ **已修复**：抽出 `system_cmd::clear_owned_clipboard(app, &AppState)`（`clear_secret_if_own` 改为薄壳，语义不变），`vault_cmd::lock` 在**丢弃记忆值之前**调用它——先清剪贴板、再清记忆值，且用户已替换内容时仍保持不动 |
| **D-3b** | 🟠 中 | `AppState.pending_imports`（`state.rs:37`）缓存 `ImportPreview`＝完整条目明文（含 password），锁定路径不清、放弃导入的预览永久驻留（且无上限增长） | ✅ **已修复**：新增 `ImportPreview::wipe_plaintext()`（`import/mod.rs`），`vault_cmd::lock` **先逐条 wipe 再 `clear()`**（顺序关键——`clear()` 只 drop 不覆写）；新增用例 `import_preview_wipe_plaintext_clears_items_and_folders` |
| **D-7** | 🟡 低 | A-05 时序侧信道未验证（代码抗时序，但无耗时证据） | ✅ **已闭合**：`tests/timing_sidechannel.rs` 实测两条失败路径 median 1868.4ms / 1877.6ms，比值 0.995（两次独立运行 0.960 / 0.995），均 ≈ 99% KDF 基准 → **无提前返回**。<br>**第三次独立运行**（主理人补测，含 D-10 新增的结构性损坏路径）：错误密码 1921.0 / 内容损坏 1944.7 / **结构性损坏 1932.6** ms，基准 2038.1，比值 0.988 / 0.994。<br>**第四次独立运行（QA 复跑，绝不与文档对齐）**：错误密码 1831.6 / 内容损坏 1776.8 / **结构性损坏 2021.0** ms，基准 2052.2，比值 1.031 / 1.137 —— **结构性损坏是三条中最接近基准的一条（98.5%）**，比前两组更能证明 `burn_unlock_kdf` 确实接上。绝对值波动属机器负载正常，**四组的结论一致**：三条失败路径均无毫秒级提前返回 |
| **D-10** | 🟠 中低 | **解锁错误的错误码会泄漏"结构性损坏"**：`session.rs:112` 的 `Container::open(path)?` 把结构性错误原样上抛，**走不到** `session.rs:130` 那个统一映射。实测：错误密码 / verify_mac 篡改 → `ERR_UNLOCK_FAILED`；**magic / version / header_len 损坏 → `ERR_INTERNAL`；文件截断 → `ERR_IO`**。攻击者改一个头字节即可从错误码区分，违反 A-05「文案一致、不区分」 | ✅ **已闭合**（两条都做了）：`session.rs` 改为 `match Container::open`——`NoVault` 原样返回，其余折叠为 `UnlockFailed`，且**先调 `burn_unlock_kdf()` 烧掉一次等价 KDF 再返回**。<br>**错误码实测**：magic / version / header_len / 截断 / JSON 反序列化 **5 种结构性损坏全部 → `ERR_UNLOCK_FAILED`**；不存在的路径仍 → `ERR_NO_VAULT`（符合 §10.1 裁定）。<br>**时序实测**：结构性损坏 median **1932.6 ms**（≈95% KDF 基准 2038.1 ms），与错误密码 1921.0 ms、内容损坏 1944.7 ms 两两比值 **0.988 / 0.994** → 不再提前返回。 |
| **D-8** | 🟡 低 | `03-impl.md` 多处仍称"未执行 `cargo check`/打包"（`:27/:209/:211/:212/:414/:431`），在补齐工具链后已不成立 | ✅ **已同步**：上述各处已改为实测结果（外壳编译通过、打包通过、37/37） |
| **D-17** | 🔴 **致命（出包）** | **`npx tauri android build` 产出的 APK 原生库为空壳**：`lib/arm64-v8a/libpasswordms_lib.so` 压缩前后均为 0 字节，是个没有原生代码的包。体积 1.62 MB 完全落在 12 MB 红线内、**门禁全绿**，但装真机必崩。触发机制：`cargo-mobile2` 要用**符号链接**把 `.so` 放进 `jniLibs`（构建日志可见 `Info symlinking lib … in jniLibs dir …`），而**本机进程无法创建符号链接**——原生 PowerShell `New-Item -ItemType SymbolicLink` 在三个位置全部 `UnauthorizedAccessException / HRESULT=0x80070005 "此操作需要管理员权限"`，且开发者模式未开启（`AppModelUnlock` 键不存在）、`IsAdmin=False`。符号链接失败后 `jniLibs` 留下一个 **0 字节普通文件**（原生取证：`Attributes=Archive`、`IsReparsePoint=False`、`LinkType` 为空），AGP 把它打进 APK | ✅ **已闭合（脚本层封死）**：`scripts/build-android.sh` 改造为「上游构建 → 解包校验 → 失败自动修复 → 二次校验 → 体积核验」。校验对每条 `.so` 断言 **存在 / 长度>0 / ELF magic `7f454c46` / sha256 与构建产物一致**，修复后**再校验一次**，仍失败即**非零退出**。<br>**产出 6.29 MB**，包内 `.so` 4,895,912 B、ELF 头、sha256 与构建产物**完全一致**（主理人独立复核）。<br>**残留**：上游 Tauri 缺陷本身未根除（再跑上游命令仍产空壳），但已无法通过本仓库脚本产出坏包 → 见 §4c.5。<br>**附**：本节曾一度把根因写成"本机可以建符号链接/开开发者模式无效"，**该结论已被原生证据推翻并撤回**，纠正过程见 §4c.5.1 |

| **D-17 编号说明** | — | 仓库中曾一度把本缺陷记为 "D-11"，但 **D-11 已被 `03-impl.md:130` 占用**（`sync_now` 直连重同步），故本缺陷统一为 **D-17** | — |
| — | ⛔ 环境 | 真机局域网同步、A-10 UI 二次确认、APK 签名后真机安装 | 本机不可验证 |

> **已闭合**（经 QA 对抗实验证实真闭合）：D-1（条目明文擦除，靠 `Drop` 兜底）、D-3、D-4、D-5、D-6。
>
> **修复后主理人独立复跑（2026-10-13 晚）**：`cargo test -p passwordms-core` **37/37**（补入 A-05 时序与解锁错误码两个新测试文件后的最终数字）；`cargo check --workspace` **EXIT=0（0 error / 0 warning）**；`cargo clippy --workspace --all-targets` **0 error / 3 warning**；命令数 `#[tauri::command]` **44** == `generate_handler!` **44**；前端 svelte-check **0/0**、vitest **34/34**。新测试中**无**裸指针 / `transmute` / `unsafe` 等 UB 手法（已 grep 核实）。
>
> **仍未验证（环境所限，非代码问题）**：真机局域网同步端到端、A-10 UI 二次确认、APK 签名与真机安装。
> （Android APK 已产出：6.29 MB，原生库经 ELF 头 + sha256 双重校验非空；但为 **unsigned** 产物，装机前需签名，且**真机启动与局域网同步仍未验证**。）

### 10.1 主理人裁定：`NoVault` **不**折叠进 `UnlockFailed`

修 D-10 时，`Container::open` 的失败要折叠成 `UnlockFailed`，但 **`CoreError::NoVault`（保险库文件不存在）必须保持独立**，理由：
1. 它是**本地 UI 状态**，不是机密——文件在不在，能访问到该路径的人都已知晓，折叠它不增加任何安全性；
2. 前端要靠它区分「首次使用 → 显示创建向导」与「已有库 → 显示解锁页」（`CreateVaultWizard` vs `LockScreen`），折叠会导致首次启动逻辑错乱；
3. A-05 的原文针对的是**「密码错」与「数据损坏」不可区分**，`NoVault` 不属于这两者中的任何一个。

即：**只折叠"库存在但头部结构性损坏"这一类**（magic / version / header_len / JSON 反序列化 / 读截断），`NoVault` 原样返回。
