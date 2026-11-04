# 07 · 质量门与出包门禁（权威）

> **本文档的性质**：PasswordMS 交付前的**唯一门禁权威**。§1 日常门 = 一条命令跑完的自动化总门；§2 出包门 = 产出安装包/APK 的**唯一正道**与出包后的人工验收；§3 安全验收点 A-01~A-10 与测试证据的映射；§4 已知边界（**未验证 ≠ 通过**）。
> 所有数字均来自 `bash scripts/gate-all.sh` **完整实跑**，不接受口头计数。**§1.2 已按 2026-10-25 18:29（v1.2.19 + D-96 + #530 lint 清零，**8 门**）实跑刷新**（总判定 **PASS**，8 门全过 / 0 失败 / 0 跳过；Rust **205 例通过 + 1 例 `#[ignore]`**、前端 **37 文件 300 例**；各门耗时 5s / 7s / 1s / 1s / 355s / 3s / 16s / 7s；门禁期间 `find … -newermt 18:29:00` **空输出** = 零改动）；其前一次为 2026-10-23 22:03（v1.2.14 + D-83，**8 门**）实跑（总判定 **PASS**，8 门全过 / 0 失败 / 0 跳过；Rust **156 例通过 + 1 例 `#[ignore]`**、前端 **32 文件 256 例**；各门耗时 15s / 13s / 5s / 3s / 353s / 14s / 39s / 19s；门禁期间 `find … -newermt 22:03:59` **空输出** = 零改动）；2026-10-23 21:15（v1.2.13 + #531，8 门，156 例 / 253 例，12s / 13s / 5s / 2s / 354s / 18s / 35s / 17s）；2026-10-23 10:07（v1.2.12 + D-81，8 门，148 例 / 223 例，13s / 15s / 4s / 3s / 334s / 17s / 32s / 16s）；2026-10-22 22:35（v1.2.11 + Q-2 + #530-clippy清零，8 门，148 例 / 204 例，13s / 14s / 4s / 2s / 400s / 4s / 30s / 15s）；2026-10-22 16:19（v1.2.11 + Q-2，clippy 尚为 7 条，148 例 / 204 例，12s / 11s / 4s / 2s / 325s / 12s / 30s / 15s）；2026-10-22 14:10（v1.2.11，**7 门**——Q-2 尚未纳入，148 例 / 204 例，12s / 11s / 4s / 341s / 12s / 28s / 13s）；2026-10-22 12:51（v1.2.10，145 例 / 204 例）、2026-10-22 09:25（v1.2.9，134 例 / 204 例）、2026-10-21 23:11（v1.2.8，131 例 / 204 例）、2026-10-21 20:33（v1.2.7，5m55s / 164 例 / 24 文件）、2026-10-21 16:17（v1.2.6，6m26s / 124 例 / 21 文件 149 例）、2026-10-21 15:11（6m39s / 120 例 / 20 文件 138 例）、2026-10-18 的旧值（4m36s、80 例、clippy 11 条）与更早的 7m03s/109 例仅作历史对照，勿跨轮引用。
> **记录人**：工程师（寇豆码）　**最近一次执行**：2026-10-23

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

| # | 门 | 命令 | 本次实测（2026-10-25 18:29 · v1.2.19 + D-96） | 判定 |
|---|----|------|---------|------|
| 1 | 前端单测 | `npm run test`（vitest 5） | **37 文件 300/300 通过**（较 v1.2.14 的 32 文件 256 例新增 5 文件 44 例——D-95/D-96 新增守卫；本轮前端零改动，例数较上一轮稳定） | ✅ 5s |
| 2 | 前端类型检查 | `npm run check`（svelte-check） | **0 errors / 0 warnings** | ✅ 7s |
| 3 | CSS 层叠契约 | `npm run check:css` | **0 FAIL / 6 WARN**（6 WARN 为既有浅色主/危险按钮对比度 3.74~4.38:1，过控件底线 3.0 未达 AA 4.5，已知项；本轮 CSS 零改动） | ✅ 1s |
| 4 | **Rust 格式基线**（**Q-2 新增**） | `cargo fmt --all -- --check` | **0 处差异**（D-96 落码遗留 50 处已于本轮 #530 独立拉平，见 §1.6 补记） | ✅ 1s |
| 5 | Rust 全量测试 | `cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm` | **205 通过 / 0 失败 / 1 忽略**（较 v1.2.14 的 156 净增 49 例——D-95/D-96 新增守卫与上限校验用例；30 条 `test result` 行；忽略项仍为 `compaction.rs` 的 `real_vault_ledger`） | ✅ 355s |
| 6 | Rust 静态检查 | `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnullvm` | **0 error / 0 条 lint**（D-96 落码曾带入 10 条，本轮 #530 已全部清零回基线——修法见表后补记；此后任何 warning 仍视为新引入） | ✅ 3s |
| 7 | 体积门-桌面 | `bash scripts/check-size.sh desktop --allow-missing` | **target 下全部历史 MSI/NSIS/portable 逐个达标**；v1.2.19 正式产物：MSI **2.58 MB** + NSIS **1.80 MB**（含 WebView2Loader.dll——D-82 installerHooks 入线延续）+ 绿色版 zip **2.10 MB**，均 ≤ 10 MB | ✅ 16s |
| 8 | 体积门-Android | 扫 `target/PasswordMS-*-arm64.apk` 逐个核验 | **21 个产物逐个核验全达标**；v1.2.19 APK **6.87 MB**（7,204,611 B）≤ 12 MB，包内 .so 四断言 4/4（sha `ee06b790…`），V3 签名证书 SHA-256 `610ca6f0…3c7c` = 正式证书 | ✅ 7s |

> **§1.2 的读数口径（重要）**：第 1~6 行来自**同一次** `bash scripts/gate-all.sh` 完整实跑（2026-10-23 22:03:59 前端起跑，**8 门全 PASS / 0 失败 / 0 跳过，EXIT=0**）；
> 第 7~8 行的字节数来自**同一棵树**的 v1.2.14 正式出包产物（`--allow-missing` 口径不变；桌面/Android 体积门在出包后复跑一次 EXIT=0，因门禁时 1.2.14 产物尚未生成）。
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
> **⚠️ 前端产物有「双味」现象（2026-10-23 实测定位，判旧勿慌）**：vite 产物字节随 **bash 的 cwd 路径形式**变化——
> MSYS 风格（`cd /d/Software/PasswordMS`，如 build-android.sh 内部）与原生风格（`D:\...`，如直接 `npx tauri build`）
> 各自**稳定复现**不同的资产哈希（本例 `index-D9_aELwE.js`↔`index-ZMORWVLH.js`）。diff 证实差异**仅**为
> 压缩器变量名与 svelte 作用域哈希类名（模块 ID 字符串形式不同 → 命名分配不同），**语义零差异、不含路径串**。
> 因此桌面 exe 与 APK 内嵌的 JS/CSS 哈希**可以不同**（各自是同一份源码的合法构建、JS+CSS 自配对）；
> 「新前端真进包」的判旧判据不受影响——**旧哈希名消失 + 新哈希名在包内**即可，勿要求两端哈希一致。
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

### 1.2a 1.5.30 起：门清单的两处结构性变化（D-176 / D-177）

> 这不是"又加了两道门"，是**两处原有的门其实没在核验它们声称核验的东西**。

**① 体积门核验的是"交付物"，不是"库存"（D-176）**

原来的 `check-size.sh android` 扫 `src-tauri/gen` 与 `gen` 找 `*.apk` —— 那儿只有
`app-universal-release-unsigned.apk`，**构建中间产物**。两个后果：

- 真正要交付的 `target/PasswordMS-<ver>-arm64.apk` **不在扫描范围内**，超标看不见；
- 中间产物在盘上就算"发现产物"，于是**用错了文件也报通过**——报绿而红线没守。

桌面分支同一个毛病：`bundle/` 里躺着历代安装包（1.5.6 起），全扫一遍等于拿旧包凑数，
今天这个包超标照样"通过"。

现在两侧都**按 `tauri.conf.json` 的当前版本收窄**：桌面核验
`bundle/**/PasswordMS_<ver>_*`，Android 核验 `target/PasswordMS-<ver>-arm64.apk`。
只有中间产物时打印一句"签名交付包还没生成，不计入体积门"，而不是默默放过。

`gate-all.sh` 里那段**内联的体积计算已删除**（原来因为 Android 分支不可用而复制了一份），
判据收归 `check-size.sh` 一处；gate-all 只保留"没出包 → 记 SKIP"。

**② 版本同步是四处，现在有门（D-177）**

1.5.29 出包那轮 `bfc3a22` 只改了前三处，`Cargo.lock` 里 `passwordms` / `passwordms-core`
还停在 1.5.28。产物不受影响（构建自己会改锁），坏的是**可复现性**（干净检出
`cargo build --locked` 会失败）与**复验前提**（发布提交自己把工作区弄脏）。

`scripts/check-version.sh`：四处逐一打印比对，全等才算过；再跑
`cargo metadata --locked --offline`——字符相等 ≠ 锁文件可用，第二条才真证明
「锁与清单一致」。作为**第 0 门**跑在最前（秒级）：版本对不上的话，后面每门的数字
都不属于"要发的那一版"，先让人等 6 分钟再报错没有意义。

**③ 锚自检（贯穿全轮）**

安全评审 2026-11-03 自我更正：他报"前端三门全绿"时，vitest 是最后跑的，而四份 i18n
在它之前已被改动——那三门锚的是当时的工作区，却被当成某个提交号的结论引用。

`gate-all.sh` 现在开头/结尾各快照一次 `HEAD + git status --porcelain`：

- 两处不一致 → **锚自检 FAIL**，本轮数字不可引用（并打印差异）；
- 跑之前就脏 → 不拦（开发中天天脏，拦了没人跑门），但**在报告开头如实打印**
  "本轮数字对应工作区，不是提交 X 的结论"。

它检不出的另一种（"跑之前就脏、且引用时写成提交号"）不装作能检——报告里写出来，
让引用的人自己注意。

### 1.2i 最近一次完整实跑（2026-11-03 14:26 起 · v1.5.30 · D-174~D-177）

> 口径：`env -u NODE_OPTIONS bash scripts/gate-all.sh`（日志 `target/_gate_1530.log`），
> 整轮墙钟 **10 分 53 秒**，总判定 **PASS（通过 9 门 / 跳过 1 门 / 失败 0）**。
> **锚：HEAD `933fcbb`，工作区干净，开头/结尾两次快照一致**（锚自检 PASS）——
> 本节所有数字都锚在这个提交上，可以整轮引用。

| # | 门 | 本次实测（v1.5.30） | 判定 |
|---|----|------|------|
| 0 | **版本同步门**（新增 D-177） | 四处一致 `1.5.30` + `cargo metadata --locked --offline` 通过 | ✅ 2s |
| 1 | 前端单测 | **85 文件 809/809 通过**（1.5.29 是 84/798：+1 文件 +8 例 = `backup-hints.test.ts` D-175） | ✅ 6s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 6s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（既有浅色按钮对比度，未达 AA 4.5，已知项） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **397 通过 / 0 失败 / 1 ignored / 47 个测试二进制** | ✅ 620s |
| 6 | clippy | **0 warning** | ✅ 11s |
| 7 | 体积门-桌面（判据已改） | 只核验 1.5.30 两件：NSIS 2.00 MB / MSI 2.83 MB | ✅ 2s |
| 8 | 体积门-Android | 本轮机跑时尚未出包 → 跳过（出包后单独复跑，见下） | ⏭ |

**这一轮门禁逮住了我自己写的一个"更彻底的修法"（值得记）**：

D-174 的第一版让 `remove_folder_direct` 在删壳的同一次操作里把其下条目迁出。
`sync_deletion.rs::case4` 立刻红：分组按 ToTrash 删除时，级联的条目**应该进回收站，
结果停在活跃表**。根因不是测试写法——`tombstone()` 只翻标志位、**沿用删除时的 rev**，
而我在批次中途把条目 `rev` 抬了一格，于是同一批里紧跟其后的「对端删除该条目」墓碑
被读成「本端改过、对端删了」→ 假冲突。

**判据**：批次内的变更共享同一批 `rev` 基准，**中途抬任何一条的 `rev`，同批里针对它的
判定就全部失真**。所以删除只做删除该做的事，修引用留给批末。详见 `docs/06-ui-design.md`
§6.65.3。

这也说明**总门是真的在拦人**：那条用例是 1.5.29 之前就写下的既有回归，不是我为这次
改动新写的——它不认识 D-174，只是照旧断言"删除该进回收站"。

**出包（同日 14:37–14:46 实测，非推算）**：

| 产物 | 字节 | sha256 前缀 |
|---|---|---|
| `PasswordMS_1.5.30_x64-setup.exe`（NSIS） | 2,099,203 | `a2010bf7e8ce` |
| `PasswordMS_1.5.30_x64_en-US.msi` | 2,969,600 | `29f4ace6cb74` |
| `PasswordMS-1.5.30-portable-x64.zip`（4 条目） | 2,472,619 | `799d3681e8e1` |
| `PasswordMS-1.5.30-arm64.apk`（V3 签名 + zipalign） | 7,852,459 | `1f21f28b3685` |
| 裸 exe | 5,093,376 | `715183c4c999` |

- **体积门（出包后复跑 `check-size.sh all`，不加 `--allow-missing`）**：三件逐一达标，
  且**只核验 1.5.30**（新的版本收窄判据第一次实战）。
- **zip 内 exe ↔ 裸 exe**：两个 sha256 完全相同。
- **双端同源**：`assets/index-CLy_biYF.js` 在 `dist/index.html`、exe 资源表、APK 内
  `libpasswordms_lib.so` 三处逐字一致。
- **NSIS 端到端静默安装**：`/S /D=<tmp>` 退出码 0，落地 exe 的
  `FileVersion = 1.5.30`（安装后的 exe 哈希与裸 exe 不同属正常：NSIS 会改写资源段）。
- **产物新鲜度**：`find src src-tauri crates docs package.json -newer <exe>` 排除
  `target/` 与 `src-tauri/gen/`（Android 构建中间产物晚于桌面 exe 是正常时序）后为空。

### 1.2j 最近一次完整实跑（2026-11-03 15:39 起 · v1.5.31 · D-178）

> 口径：`bash scripts/gate-all.sh`（日志 `target/_gate_1531.log`），整轮墙钟
> **8 分 37 秒**（`stat` 实测：15:39:26 起 / 15:48:03 止——比 1.5.30 那轮快，
> 因为 Rust 测试二进制已在盘上、没有重新编译）。
> 总判定 **PASS（通过 9 门 / 跳过 1 门 / 失败 0）**。
> **锚：HEAD `02dfa5d`，开工时工作区已有未提交改动（版本号 + 本轮代码），
> 锚自检比对的是"整轮期间 HEAD 与工作区文件集合未变"——通过，故读数可引用。**

| # | 门 | 本次实测（v1.5.31） | 判定 |
|---|----|------|------|
| 0 | **版本同步门** | 四处一致 `1.5.31` + `cargo metadata --locked --offline` 通过 | ✅ 2s |
| 1 | 前端单测 | **85 文件 816/816 通过**（1.5.30 是 85/809：+7 例，全部来自 D-178 的清单/文案/接线） | ✅ 5s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 6s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（既有浅色按钮对比度，未达 AA 4.5，已知项） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **400 通过 / 0 失败 / 1 ignored / 47 个测试二进制**（397 → 400：+3 例，备份候选清单） | ✅ 482s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 12s |
| 7 | 体积门-桌面 | 只核验 `PasswordMS_1.5.31_*` 两件 | ✅ 5s |
| 8 | 体积门-Android | 跑门时尚未出包 → 跳过（出包后单独复跑，见下） | ⏭ |

**这一轮门禁没逮住东西，是件需要说明的事**：D-178 改的是**候选集的判据**，
而它在此之前没有任何测试覆盖（旧判据只认 `pre-restore`，谁也没为"哪些备份能退回去"
写过断言）。所以本轮同时补了三条**正向**用例——它们是新写的，天然不会红。
真正拦住冒烟的是变异检验（手工做，不进 CI）：

| 变异 | 期望 | 实测 |
|---|---|---|
| `needs_old_password()` 恒返回 `false` | 两条用例红 | ✅ `退回去要列出全部整库副本…` 与 `旧代里的last_bak…` 双双 FAILED |

**如实登记一处不严谨**：本轮实跑期间我改了 `docs/06-ui-design.md`
（补 §6.64.12 与卡片线框图）。锚自检只比对 `git status --porcelain` 的**文件集合**
与 HEAD，文档内容变动它看不见——所以"锚自检通过"这句话在这轮里**不等于"文件树一个
字节没动"**。判据上的后果可控：没有任何门读 `docs/`（`npm run test` / `check` /
`check-css` / `cargo test` / `clippy` / `check-size` 的输入都不含文档），代码与脚本
在整轮期间零改动。要堵这个洞得把锚扩到内容哈希——**已知未修**，见 §1.2a 的锚自检说明。

**出包（同日 15:52–15:58 实测，非推算）**：桌面构建 2 分 48 秒、Android 构建 3 分 37 秒。

| 产物 | 字节 | sha256 前缀 |
|---|---|---|
| `PasswordMS_1.5.31_x64-setup.exe`（NSIS） | 2,104,255 | `6b5eb8829170` |
| `PasswordMS_1.5.31_x64_en-US.msi` | 2,973,696 | `ebcf98382366` |
| `PasswordMS-1.5.31-portable-x64.zip`（4 条目） | 2,476,111 | `829e47c4d291` |
| `PasswordMS-1.5.31-arm64.apk`（V3 签名 + zipalign） | 7,856,555 | `14824b4ea329` |
| 裸 exe | 5,100,544 | `a146683282eb` |

- **体积门（出包后复跑 `check-size.sh all`，不加 `--allow-missing`）**：三件逐一达标
  （NSIS 2.01 MB / MSI 2.84 MB / APK 7.49 MB），且只核验 1.5.31。
- **zip 内 exe ↔ 裸 exe**：`a146683282eb…` 两边逐字节相同。
- **双端同源**：`assets/index-DzGgGnPq.js` 在 `dist/index.html`、裸 exe、APK 内
  `libpasswordms_lib.so` 三处一致（字节级搜资源名字符串）。
- **NSIS 端到端静默安装**：`/S /D=<tmp>` 退出码 0，落地 exe `FileVersion = 1.5.31`、
  `ProductVersion = 1.5.31`、5,100,544 B（安装后哈希与裸 exe 不同属正常，NSIS 改写资源段）。
- **产物新鲜度**：`find src src-tauri crates docs scripts package.json Cargo.toml
  -newer <裸 exe>` 排除 `target/` 与 `src-tauri/gen/` 后**输出为空**——没有源文件或文档
  晚于 exe，说明树在构建前就冻结了。

### 1.2p 最近一次完整实跑（2026-11-04 13:38 起 · v1.5.37 · D-188 / D-189 / D-190）

> 口径：`env -u NODE_OPTIONS bash scripts/gate-all.sh`，整轮墙钟 **8 分 48 秒**
> （Rust 全量 495s 是大头），总判定 **PASS（通过 9 门 / 跳过 1 门 / 失败 0）**。
> **锚：HEAD `a1bb82c`，工作区 9 项未提交改动；首尾快照一致 → 数字对应本工作区。**
> **开跑前冻结文件树，整轮没写一个字**（上一轮 §1.2o 就是在这里白跑过一次）。

| # | 门 | 本次实测（v1.5.37） | 判定 |
|---|----|------|------|
| 0 | 版本同步门 | 四处一致 `1.5.37` + `cargo metadata --locked --offline` 通过 | ✅ 3s |
| 1 | 前端单测 | **89 文件 840/840 通过**（与 1.5.36 同数：本轮零前端改动，新增用例全在 Rust 侧） | ✅ 5s |
| 2 | 前端类型检查 | 0 errors / 0 warnings | ✅ 6s |
| 3 | CSS 层叠契约 | 0 FAIL / 6 WARN（既有浅色按钮对比度，未达 AA 4.5） | ✅ 1s |
| 4 | fmt | 0 处差异 | ✅ 1s |
| 5 | Rust 全量 | **419 通过 / 0 失败 / 1 ignored / 48 个测试二进制**（1.5.36 是 414/47：+5 例、+1 二进制） | ✅ 495s |
| 6 | clippy | **0 warning** | ✅ 11s |
| 7 | 体积门-桌面 | 跑门时尚未出包 → 按 `--allow-missing` 跳过；出包后复跑达标 | ⏭ |
| 8 | 体积门-Android | 同上 | ⏭ |

**+5 例的来源**（不推算，逐条数出来的）：`钉住的备份不会被配额腾挪删掉`、
`钉住之后裁不动就报配额失败而不是删它`、`库变小后旧的大备份不再把配额压死`
（三条核心单测）+ 新集成文件 `export_vault_atomic.rs` 三条 → 其中
`配额不足时由旧到新裁剪…` 是**改名改断言**，不算新增，故净 +5。

**出包（2026-11-04 实测，非推算）**：

| 产物 | 字节 | sha256 前缀 | 与 1.5.36 对比 |
|---|---|---|---|
| `PasswordMS_1.5.37_x64-setup.exe`（NSIS） | 2,115,204 | `5b578e92759b` | 2,115,632 → 变了 |
| `PasswordMS_1.5.37_x64_en-US.msi` | 2,985,984 | `3d518784f7a7` | 同尺寸不同哈希 |
| `PasswordMS-1.5.37-portable-x64.zip`（4 条目） | 2,492,453 | `9ed6e5539c81` | 2,492,229 → 变了 |
| `PasswordMS-1.5.37-arm64.apk`（V3 签名 + zipalign） | 7,889,323 | `f99f9d3adb4d` | 7,893,419 → 变了 |
| 裸 exe | 5,127,680 | `3eb2a8f32003` | **同尺寸**（5,127,168）、哈希变 |

- 体积门 `check-size.sh desktop` / `android`：NSIS 2.02 MB / MSI 2.85 MB / APK 7.52 MB，全部达标。
- zip 内 exe ↔ 裸 exe：`3eb2a8f32003…` 两边逐字节相同（zip 条目带 `PasswordMS-Portable/` 前缀）。
- **裸 exe 与 1.5.36 体积完全相同（5,127,680 vs 5,127,168 差 512 B）、哈希不同**：
  本轮只动 Rust 内部逻辑、不动前端，所以体积落在同一档属正常；判"编进去了没有"看哈希与
  `find … -newer` 新鲜度，**别看体积**（1.5.34/35/36 的 APK 同为 7,893,419 就是这么来的）。
- 前端资源名与 1.5.36 相同（`index-DExT4ix_.js`）——**这是"零前端改动"的结果，不是漏构建**：
  双端 `dist/` 一次构建、NSIS 与 APK 都读同一份 `dist`。
- 产物新鲜度：`find src crates scripts -newer <NSIS>` **输出为空**。
- Android 链自带两道门（REAL_SO mtime ≥ 上游起点、UPSTREAM_RC≠0 一票否决）全过：
  `lib/arm64-v8a/libpasswordms_lib.so` 5,853,568 B、`sha256=bb04d44bc3cd…`，已比对构建产物 4/4。

### 1.2q 本轮实跑（2026-11-04 14:39 · 未发版 · D-191 / D-192 / D-193）

> 口径同前：`env -u NODE_OPTIONS` + 前端三门。**没有跑 `gate-all.sh`**——这一轮没动 Rust
> （`git diff --stat` 里 `.rs` / `.toml` 计数为 0），Rust 三门沿用 v1.5.37 的读数。

| 门 | 读数 |
|---|---|
| `npm run test` | **90 文件 850 例全绿**（v1.5.37 为 89/840；+1 文件 = `d192-feedback-guard.test.ts`，+10 例 = D-191 绊线 4 例 + D-192 6 例） |
| `npm run check` | 0 errors / 0 warnings |
| `check-css-contract.mjs` | 0 FAIL / 6 WARN（与 1.5.37 逐条相同，全是主按钮 3.74:1 那批既有项） |

**新增的人工门（不进 `gate-all.sh`，发版前手动跑一次并贴末尾汇总）**：

```
env -u NODE_OPTIONS node scripts/check-e2e-quotes.mjs
```

它校验 `docs/07-e2e-checklist.md` 里每个引号片段能否在四本字典里找到。

**当前基线（必须带锚，否则十分钟后就不是这个数）**：

```
引号片段 354 条 ｜ 参与检查 288 条 ｜ 命中 288 条（原样 215 / 占位符归一 71 / 缩略 2）
｜ 查不到 0 条 ｜ 无法核对 0 条 ｜ 规则跳过 66 条
```

锚：走查单 blob `846135d5…`、脚本 blob `79919e03…`（`git hash-object` 可复核）。
**引用这几个数字前先用这两个 blob 对齐**——这份表格刚吃过一次教训：同一份走查单，
已提交版与工作树版的读数就差 4 条"无法核对"（357/290/286/**4**/67 vs 354/288/288/**0**/66），
两个人各拿一个状态对话，就会互相认定对方报假数。

为什么不进总门：**退出码恒为 0**（只报警、不判负）。并进去只是多一门给人看绿灯的门，
反而稀释了"门 = 会红"的可信度。等基线稳定、且有人愿意把它的报警当回事之后再议。

四个桶，**方向各不相同**，别用一个标准看：

| 桶 | 现在 | 涨了说明什么 | 该怎么办 |
|---|---|---|---|
| 查不到 | **0** | 真漂移（文案改了、走查单没跟） | 一次都不许有 |
| 无法核对（纯数字/单位） | **0** | 有人在判据里引了机器判不了的正向片段 | **零容忍**：改写成带汉字的样子 |
| 规则跳过（路径/拼接/过短/叙述） | 66 | 口径在变松，或有人用"跳过"绕过检查 | 逐条给理由；新增条目必须带理由 |
| 旧文案桶（双引号里已失效的旧文案） | **2** | 失效文案在积累，或者有人把真文案写进了双引号 | 它**不进分母、不参与校验**，是新开的口子——所以条数必须跟着读数一起报。口径只数表格行：`grep -E '^\| *[0-9]+\.[0-9]+ *\|' docs/07-e2e-checklist.md \| grep -c '旧文案'` |

报"查不到 0 条"时必须带上**分母**。分母用不依赖解析器的独立计数交叉核过
（只数表格行的开括号，它与"引号片段数"的差应恰好等于嵌套内层数，对不上就是分母错了）。
另：脚本**只扫表格行**，写在章节标题/修订记录/规则说明里的「」一律不进分母——
所以界面文案别写在那几处。

另有一项**没有常态门**、靠一次性脚本核的：走查单里反引号包的 `ns.key` 键名。
本轮核了 69 个引用，真错 0 条（11 条"查不到"里 8 条是文件名/存储键、1 条是已删的孤儿键
`create.weak_hint`、1 条是 D-185 的裸键名反例）。要常态化得单写一门，别混进引文脚本。


### 1.2o 最近一次完整实跑（2026-11-04 11:04 起 · v1.5.36 · D-186）

> 口径：`env -u NODE_OPTIONS bash scripts/gate-all.sh`（日志 `target/_gate_1536b.log`），
> 整轮墙钟 **约 11 分**（Rust 全量 619s 是大头），总判定 **PASS（通过 9 门 / 跳过 1 门 / 失败 0）**。
> **锚：HEAD `27ff03c`，工作区 15 项未提交改动；首尾快照一致 → 数字对应本工作区。**

| # | 门 | 本次实测（v1.5.36） | 判定 |
|---|----|------|------|
| 0 | 版本同步门 | 四处一致 `1.5.36` + `cargo metadata --locked --offline` 通过 | ✅ 2s |
| 1 | 前端单测 | **89 文件 840/840 通过**（1.5.35 是 88/835：+1 文件 +5 例 = `d186-backup-error-stale.test.ts`） | ✅ 6s |
| 2 | 前端类型检查 | 0 errors / 0 warnings | ✅ 7s |
| 3 | CSS 层叠契约 | 0 FAIL / 6 WARN（既有浅色按钮对比度，未达 AA 4.5） | ✅ 2s |
| 4 | fmt | 0 处差异 | ✅ 1s |
| 5 | Rust 全量 | **414 通过 / 0 失败 / 1 ignored / 47 个测试二进制** | ✅ 619s |
| 6 | clippy | **0 warning** | ✅ 2s |
| 7 | 体积门-桌面 | 只核验 1.5.36 两件：NSIS 2.02 MB / MSI 2.85 MB | ✅ 3s |
| 8 | 体积门-Android | 跑门时尚未出包 → 跳过；出包后 `check-size.sh all` 复跑通过 | ⏭ |

**这一轮第一次跑是 FAIL 的，两处都记下来**：

1. **clippy rc=101**：`backup::clear_error(repo.path())` 传了 `PathBuf` 给 `&Path` 形参
   （`src-tauri/src/commands/backup_cmd.rs:140`）。核心层单测不覆盖命令层，所以
   `cargo test -p passwordms-core` 是绿的，直到 clippy 才炸——**新加 IPC 命令后先单跑 clippy**。
2. **锚自检 FAIL：门跑期间目录被改**——是我在门跑的同时往 `docs/06-ui-design.md` 追加了
   §6.68。这不是误报，是锚在干本职工作（它只比对文件集合与 HEAD）。**规矩重申：开跑前冻结文件树，
   跑的时候一个字都别写**，否则整轮读数不可引用，只能白跑一遍。

**出包（2026-11-04 实测，非推算）**：

| 产物 | 字节 | sha256 前缀 |
|---|---|---|
| `PasswordMS_1.5.36_x64-setup.exe`（NSIS） | 2,115,632 | `a50cc35fb854` |
| `PasswordMS_1.5.36_x64_en-US.msi` | 2,985,984 | `cec0168a19f0` |
| `PasswordMS-1.5.36-portable-x64.zip`（4 条目） | 2,492,229 | `46025665528f` |
| `PasswordMS-1.5.36-arm64.apk`（V3 签名 + zipalign） | 7,893,419 | `9ae470c5af78` |
| 裸 exe | 5,127,168 | `27f4ef6a5638` |

- 体积门 `check-size.sh all`：NSIS 2.02 MB / MSI 2.85 MB / APK 7.53 MB，全部达标。
- zip 内 exe ↔ 裸 exe：`27f4ef6a5638…` 两边逐字节相同（注意 zip 条目带 `PasswordMS-Portable/` 前缀）。
- 双端同源：`assets/index-DExT4ix_.js` 在 `dist/index.html`、exe 资源表、APK 内
  `libpasswordms_lib.so` 三处一致。
- NSIS 端到端静默安装：`/S /D=<tmp>` 退出码 0，落地 exe `FileVersion = 1.5.36`、
  `ProductVersion = 1.5.36`、5,127,168 B。
- 产物新鲜度：`find src src-tauri crates scripts docs package.json Cargo.toml -newer <裸 exe>`
  排除 `src-tauri/gen/` 后**输出为空**。
- **APK 字节数与 1.5.34 完全相同（7,893,419）**，与 1.5.35 也相同——纯属压缩后尺寸巧合，
  判据看包内 `.so` 的资源名（本轮是新的 `index-DExT4ix_.js`）。

### 1.2n 最近一次完整实跑（2026-11-04 08:58 起 · v1.5.35 · D-184/D-185）

> 口径：`bash scripts/gate-all.sh`，整轮墙钟 **11 分 00 秒**（08:58:35 起 / 09:09:35 止，
> 任务计时实测）。总判定 **PASS（通过 9 门 / 跳过 1 门 / 失败 0）**；
> 跳过的 1 门是 Android 体积门（门禁跑在出包之前，出包后**单独补跑**，见下）。
> 锚：HEAD `72b24f4` + 本轮未提交改动（版本号 + 代码 + 文档）。

| # | 门 | 本次实测（v1.5.35） | 判定 |
|---|----|------|------|
| 0 | **版本同步门** | 四处一致 `1.5.35`（含 Cargo.lock） | ✅ 2s |
| 1 | 前端单测 | **88 文件 835/835**（825 → 835：+6 折叠守门 D-184、+4 失效原因守门 D-185） | ✅ 6s |
| 2 | 前端类型检查 | 0 / 0 | ✅ 7s |
| 3 | CSS 层叠契约 | 0 FAIL / 6 WARN（既有项） | ✅ 1s |
| 4 | fmt | 0 处差异 | ✅ 1s |
| 5 | Rust 全量 | **414 通过 / 0 失败 / 1 ignored / 47 二进制**（本轮未动 Rust；628s，机器负载偏高） | ✅ 628s |
| 6 | clippy | 0 error / 0 warning | ✅ 10s |
| 7 | 体积门-桌面（出包后补跑） | NSIS 2.02 MB / MSI 2.85 MB ≤ 10 MB | ✅ |
| 8 | 体积门-Android（出包后补跑） | APK 7.52 MB ≤ 12 MB | ✅ |

**出包（同日 09:09–09:18 实测）**：桌面 **4 分 02 秒**、Android **4 分 37 秒**。

| 产物 | 字节 | sha256 前缀 |
|---|---|---|
| `PasswordMS_1.5.35_x64-setup.exe`（NSIS） | 2,114,474 | `4aac48bf34b17c9d` |
| `PasswordMS_1.5.35_x64_en-US.msi` | 2,985,984 | `94f9a866512b1b5b` |
| `PasswordMS-1.5.35-portable-x64.zip`（4 条目） | 2,492,056 | `4f225a1361ba13b5` |
| `PasswordMS-1.5.35-arm64.apk`（V3 签名） | 7,889,323 | `e413e94a5d744e33` |
| 裸 exe | 5,126,144 | `feb77287d019d8ba` |

- **双端同源**：`index-CTbP-1t2.js`（+ 第二个入口 chunk `index-D_ghNIYi.js`）在 dist、
  裸 exe、APK 的 `.so` 三处一致。
- **APK 体积与 1.5.33 恰好同为 7,889,323 B 属巧合**（1.5.34 是 7,893,419）。
  已用资源名核对：包内 `.so` 里是**本轮新构建的** `index-CTbP-1t2.js`，不是旧包。
  记在这里免得下次有人拿"字节数没变"当"没重编"的证据。
- **portable zip**：条目在 `PasswordMS-Portable/` 前缀下（取 exe 哈希要带前缀）。
- **NSIS 静默安装**：`/S` + `/D=<dir>` → exit=0，`passwordms.exe` 5,126,144 B、
  `FileVersion=1.5.35`。
- **产物新鲜度**：`find src src-tauri/src crates scripts package.json Cargo.toml -newer <APK>` 为空。
- **D-185 变异自证（实测）**：把 `bioStaleStateKey` 的映射表改回
  `settings.bio_state_stale_${reason}` 现拼 → 「每个 wire 原因在四种语言里都有键」与
  「兜底」两条 **红**，报错正文为
  `master_password_changed → settings.bio_state_stale_master_password_changed 缺 zh-CN`
  （正是主人截图那串）；还原后 4 条全绿。
- **D-184 用例**：`d184-backup-collapsed.test.ts` 6 条结构守门（默认收起、摘要含旧密码份数、
  表格在折叠块内、`backupHints` 在折叠块外、独立类名）；首次编写时故意没做变异实验
  ——它不是修 bug 而是加折叠，结构事实由断言直接钉住。

### 1.2m 最近一次完整实跑（2026-11-03 22:37 起 · v1.5.34 · D-182/D-183）

> 口径：`bash scripts/gate-all.sh`，整轮墙钟 **9 分 15 秒**（22:37:25 起 / 22:46:40 止，
> 任务计时实测）。总判定 **PASS（通过 9 门 / 跳过 1 门 / 失败 0）**；
> 跳过的 1 门是 Android 体积门（门禁跑在出包之前，出包后**单独补跑**，见下）。
> 锚：HEAD `ce6df11` + 本轮未提交改动（版本号 + 代码 + 文档）。

| # | 门 | 本次实测（v1.5.34） | 判定 |
|---|----|------|------|
| 0 | **版本同步门** | 四处一致 `1.5.34`（含 Cargo.lock） | ✅ 2s |
| 1 | 前端单测 | **86 文件 825/825**（819 → 825：+6 首启语言守门，D-183） | ✅ 6s |
| 2 | 前端类型检查 | 0 / 0 | ✅ 6s |
| 3 | CSS 层叠契约 | 0 FAIL / 6 WARN（既有项） | ✅ 1s |
| 4 | fmt | 0 处差异 | ✅ 1s |
| 5 | Rust 全量 | **414 通过 / 0 失败 / 1 ignored / 47 二进制**（412 → 414：+2 校验值判据用例，D-182） | ✅ 522s |
| 6 | clippy | 0 error / 0 warning | ✅ 10s |
| 7 | 体积门-桌面（出包后补跑） | NSIS 2.02 MB / MSI 2.85 MB ≤ 10 MB | ✅ |
| 8 | 体积门-Android（出包后补跑） | APK 7.53 MB ≤ 12 MB | ✅ |

**出包（同日 22:47–22:53 实测）**：桌面 **2 分 34 秒**、Android **3 分 20 秒**。

| 产物 | 字节 | sha256 前缀 |
|---|---|---|
| `PasswordMS_1.5.34_x64-setup.exe`（NSIS） | 2,114,174 | `22266a138808c12b` |
| `PasswordMS_1.5.34_x64_en-US.msi` | 2,985,984 | `3907ef2ca17a5d96` |
| `PasswordMS-1.5.34-portable-x64.zip`（4 条目） | 2,493,013 | `3b9804239896b327` |
| `PasswordMS-1.5.34-arm64.apk`（V3 签名） | 7,893,419 | `c6b81377686ee28e` |
| 裸 exe | 5,129,216 | `ae330086855ce54d` |

- **双端同源**：`assets/index-Dp1HcU7R.js` 在 dist、裸 exe、APK 的 `.so`（`lib/arm64-v8a/libpasswordms_lib.so`）
  三处一致。注意 APK 里前端资源是**编进 .so** 的，`unzip -l` 看不到 `assets/index-*.js`，
  要 `unzip -p ... lib.so | grep -a` 才查得到（别据此判"没进包"）。
- **portable zip**：条目在 `PasswordMS-Portable/` 前缀下，取 exe 哈希要带前缀
  （`unzip -p <zip> PasswordMS-Portable/passwordms.exe`），哈希与裸 exe 相等 `ae330086855ce54d`。
- **NSIS 静默安装**：`Start-Process -Wait '/S', "/D=<dir>"` → exit=0，产出 `passwordms.exe`
  5,129,216 B、`FileVersion=1.5.34`。
- **产物新鲜度**：`find src src-tauri/src crates scripts docs package.json Cargo.toml -newer <APK>` 为空。
- **D-182 变异自证（实测，非推算）**：把 `needs_old_password` 里"比 `verify_mac`"那一级
  整段删掉（只剩 `f.needs_old_password()` 目录判据），重跑备份模块 →
  `旧备份没有世代号时按校验值判_恢复之后同样摆正` **FAILED**，世代那条也 FAILED
  （整段回落目录判据），57 → 55 通过 / 2 失败；还原后 57 全绿。
- **D-183 变异自证（实测）**：把 `ui.setLocale(mapSystemLanguage(tag))` 改成
  `ui.locale = mapSystemLanguage(tag)`（只改内存、不写回），重跑
  `d183-first-run-locale.test.ts` → 「校正结果必须写回」**红**（6 → 5 通过）；还原后 6 全绿。

### 1.2l 最近一次完整实跑（2026-11-03 20:14 起 · v1.5.33 · D-181）

> 口径：`bash scripts/gate-all.sh`，整轮墙钟 **9 分 02 秒**（20:14:41 起 / 20:23:43 止）。
> 总判定 **PASS（通过 11 门 / 跳过 0 门 / 失败 0）**。锚：HEAD `6a83886`。

| # | 门 | 本次实测（v1.5.33） | 判定 |
|---|----|------|------|
| 0 | **版本同步门** | 四处一致 `1.5.33` | ✅ 3s |
| 1 | 前端单测 | **85 文件 819/819**（文案改写不新增用例；世代判据的用例在 Rust 侧） | ✅ 6s |
| 2 | 前端类型检查 | 0 / 0 | ✅ 7s |
| 3 | CSS 层叠契约 | 0 FAIL / 6 WARN（既有项） | ✅ 1s |
| 4 | fmt | 0 处差异 | ✅ 1s |
| 5 | Rust 全量 | **412 通过 / 0 失败 / 1 ignored / 47 二进制**（410 → 412：+2 世代判据用例，d108 内 +世代断言） | ✅ 500s |
| 6 | clippy | 0 warning | ✅ 12s |
| 7/8/9 | 体积门 | NSIS 2.01 MB / MSI 2.85 MB / APK 7.52 MB，只核验 1.5.33 | ✅ |

**出包（同日 20:05–20:14 实测）**：桌面 2 分 51 秒、Android 3 分 47 秒。

| 产物 | 字节 | sha256 前缀 |
|---|---|---|
| `PasswordMS_1.5.33_x64-setup.exe`（NSIS） | 2,110,849 | `8fb007b590ac` |
| `PasswordMS_1.5.33_x64_en-US.msi` | 2,985,984 | `a62b6a230a89` |
| `PasswordMS-1.5.33-portable-x64.zip`（4 条目） | 2,491,486 | `325f5d03dab1` |
| `PasswordMS-1.5.33-arm64.apk`（V3 签名） | 7,889,323 | `92261a54c4ab` |
| 裸 exe | 5,126,656 | `6491ad6b7c2c` |

- **双端同源**：`assets/index-BVw2nVL7.js` 在 dist、裸 exe、APK 三处一致，且两包内只有这一个。
- **NSIS 静默安装**：exit=0，`FileVersion=1.5.33`，5,126,656 B。
- **产物新鲜度**：`find src src-tauri/src crates docs scripts package.json Cargo.toml -newer <APK>` 为空。
- **D-181 变异自证（实测，非推算）**：把 `key_generation` 从 `Option<u32>` 改回裸
  `u32`（模拟"偷懒"写法），重跑备份模块——
  `没有世代字段的旧备份回落目录位置判据` **FAILED**（旧文件被 `serde(default)` 读成
  世代 0，回落分支消失），`要不要旧密码按世代判_…` 保持 ok；还原后 55 条全绿。
  这正是字段必须 `Option` 的原因（docs/06 §6.64.14 第 4 条）。

### 1.2k 最近一次完整实跑（2026-11-03 17:25 起 · v1.5.32 · D-179/D-180）

> 口径：`bash scripts/gate-all.sh`，整轮墙钟 **8 分 11 秒**（17:25:37 起 / 17:33:48 止，
> `stat` 实测）。总判定 **PASS（通过 11 门 / 跳过 0 门 / 失败 0）**。
> **锚：HEAD `4a23f63`，开工时工作区已有未提交改动（版本号 + 本轮代码）。
> 锚自检比对"整轮期间 HEAD 与工作区文件集合未变"——通过，故读数可引用。**

| # | 门 | 本次实测（v1.5.32） | 判定 |
|---|----|------|------|
| 0 | **版本同步门** | 四处一致 `1.5.32` + `cargo metadata --locked --offline` 通过 | ✅ 2s |
| 1 | 前端单测 | **85 文件 819/819 通过**（1.5.31 是 85/816：+3 例，钉住/自动清理接线） | ✅ 5s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 6s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（既有浅色按钮对比度，已知项） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 0s |
| 5 | Rust 全量 | **410 通过 / 0 失败 / 1 ignored / 47 个测试二进制**（400 → 410：+11 例备份，-1 例撤销的总量上限） | ✅ 455s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 10s |
| 7 | 体积门-桌面 | 只核验 `PasswordMS_1.5.32_*` 两件 | ✅ 5s |
| 8 | 体积门-Android | **7.52 MB ≤ 12 MB**（本轮先出包再跑门，不再跳过） | ✅ 0s |
| 9 | 体积门-Android（复跑） | 同上一件，`check-size.sh android` 直跑 | ✅ 1s |

### ⚠️ 这一轮第一次跑是 FAIL 的（必须记下来，否则下次会当成"门禁很严"）

第一次 `gate-all.sh` 跑在 16:41，**Rust 全量 2625s 后失败**：

```
case1_end_to_end_attachment_blob_sync  panicked: 回环配对握手失败: ERR_PAIR_TOKEN_EXPIRED
case5_bidirectional_symmetric_sync     panicked: reply_recv|io:unexpected end of file
test result: FAILED. 10 passed; 2 failed  (sync_blob, 2377s)
```

单独复跑同一个二进制：**12/12 通过，26.79s**（`cargo test -p passwordms-core --test
sync_blob --target x86_64-pc-windows-gnullvm`）。

**判定：负载导致的偶发，不是回归。** 依据三条：

1. 该文件在两次运行里**代码零改动**（本轮改动只有 `store/backup.rs` + DTO + 设置页）。
2. 配对令牌的 TTL 按**真实墙钟**算（`client::pair(&qr, ..., now_ms())`），同一份用例
   27s 跑完 vs 2377s 跑完，差 88 倍——超时是被机器拖出来的，不是逻辑变了。
3. 失败的两条都发生在握手/收发阶段，`unexpected end of file` 是对端线程先 panic
   之后的自然 EOF，属于**同一次卡顿的两个表现**，不是两个独立缺陷。

第二次整轮重跑：**11 门全绿，Rust 455s**。**但这是一次"重跑即绿"，按 §1.4 的纪律
不能当成"修好了"**——真正的结论是：`sync_blob` 在重负载下可偶发失败，属**已知未修**，
下次撞见先单跑确认，不要顺手去改同步逻辑。

**出包（同日 16:34–16:41 实测，非推算）**：桌面构建 2 分 46 秒、Android 构建 3 分 27 秒。

| 产物 | 字节 | sha256 前缀 |
|---|---|---|
| `PasswordMS_1.5.32_x64-setup.exe`（NSIS） | 2,110,636 | `2cd68bca5f70` |
| `PasswordMS_1.5.32_x64_en-US.msi` | 2,985,984 | `97622392cb28` |
| `PasswordMS-1.5.32-portable-x64.zip`（4 条目） | 2,489,778 | `f701a50af87e` |
| `PasswordMS-1.5.32-arm64.apk`（V3 签名 + zipalign） | 7,889,323 | `fc24737afa18` |
| 裸 exe | 5,122,560 | `e2677e1d4c8c` |

- **体积门（出包后 `check-size.sh desktop|android` 不加 `--allow-missing`）**：三件逐一达标
  （NSIS 2.01 MB / MSI 2.85 MB / APK 7.52 MB），且只核验 1.5.32。
- **zip 内 exe ↔ 裸 exe**：`e2677e1d4c8c…` 两边逐字节相同。
- **双端同源**：`assets/index-CLEJgpGZ.js` 在 dist、裸 exe、APK 三处一致（字节级搜名字）；
  两个包里都**只有这一个** `index-*.js`，没有旧资源残留。
- **NSIS 端到端静默安装**：`/S /D=<tmp>` 退出码 0，落地 exe `FileVersion = 1.5.32`、
  `ProductVersion = 1.5.32`、5,122,560 B（安装后哈希与裸 exe 不同属正常，NSIS 改写资源段）。
- **产物新鲜度**：`find src src-tauri/src crates docs scripts package.json Cargo.toml
  -newer <APK>`（排除 `src-tauri/gen/`）**输出为空**；对裸 exe 同样为空——
  树在两次构建之前就冻结了。
  **一处如实登记**：`dist/` 的时间戳晚于桌面 exe 10 秒，原因是 Android 链会**再跑一次**
  前端构建（`beforeBuildCommand`）。两次产出的资源名同为 `index-CLEJgpGZ.js`，
  内容一致，不影响判据；但"看 mtime 判新鲜度"时**不要拿 dist 去比 exe**。

### 1.2b 最近一次完整实跑（2026-10-30 11:40 起 · v1.4.16 · D-133/D-134）

> 口径：`bash scripts/gate-all.sh --keep-going` 一次跑完 8 门，文件树在起跑前已静止（构建产物晚于全部源码）。
> 本节只记**这一次**的定值，引用请连时间一起引。

| # | 门 | 本次实测（v1.4.16） | 判定 |
|---|----|------|------|
| 1 | 前端单测 | **53 文件 475/475 通过**（较 v1.4.15 的 487：删 `auto-sync.test.ts` 20 例（D-133 回退）、新增 `d133-d134-sync-guard.test.ts` 8 例） | ✅ 5s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 9s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（同既有浅色按钮对比度项） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **283 通过 / 0 失败**（`cargo test -p passwordms-core`，30 条 `test result` 行） | ✅ 505s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 17s |
| 7 | 体积门-桌面 | 历史 MSI/NSIS 逐个达标；v1.4.16 正式产物 MSI **2.77 MiB** + NSIS **1.95 MiB** | ✅ 12s |
| 8 | 体积门-Android | 10 个历史 APK 逐个达标；v1.4.16 APK **7.37 MB**（7,725,483 B）≤ 12 MB | ✅ 4s |

**总判定：PASS（8 门全过 / 0 失败 / 0 跳过）。**

> ⚠️ `cargo test --workspace` 在本机**不可用**：`passwordms`（cdylib）作为测试目标链接时报
> `ld: error: export ordinal too large`（windows-gnu + cdylib 的既有环境问题，与本轮改动无关）。
> 因此 Rust 门固定跑 `-p passwordms-core`（业务逻辑与守门用例全都在这个 crate 里），
> 命令层靠 clippy + 前端 `ipc-contract.test.ts`（命令名/属性计数/`rename_all` 全比对）兜住。

### 1.2c 最近一次完整实跑（2026-10-30 12:25 起 · v1.4.17 · D-135）

> 口径：`bash scripts/gate-all.sh --keep-going` 一次跑完 8 门，起跑前文件树静止。
> **注意时序**：本次门禁跑在「版本号已升、产物还是 1.4.16」的时刻，所以桌面体积门里
> 列的是历史产物；1.4.17 的正式产物在构建完成后**单独复跑**了体积门（见第 7、8 行括注）。

| # | 门 | 本次实测（v1.4.17） | 判定 |
|---|----|------|------|
| 1 | 前端单测 | **54 文件 478/478 通过**（较 v1.4.16 的 475：新增 `d135-clipboard-guard.test.ts` 3 例） | ✅ 5s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 7s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（同既有浅色按钮对比度项） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **285 通过 / 0 失败**（`cargo test -p passwordms-core`，较 v1.4.16 的 283：新增 `tests/clipboard_policy.rs` 2 例） | ✅ 441s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 11s |
| 7 | 体积门-桌面 | 历史 NSIS 逐个达标；**构建后单独复跑 8.8s**，1.4.17 正式产物 NSIS **1.95 MiB** + MSI **2.77 MiB** ≤ 10 MiB | ✅ |
| 8 | 体积门-Android | 11 个历史 APK 逐个达标；1.4.17 签名 APK **7.37 MB**（7,725,483 B）≤ 12 MB | ✅ 3s |

**总判定：PASS（8 门全过 / 0 失败 / 0 跳过），整轮墙钟 7 分 58 秒（其中 Rust 门 441s）。**

### 1.2d 最近一次完整实跑（2026-10-30 出包前 · v1.4.18 · D-136）

> 口径：`bash scripts/gate-all.sh --keep-going` 一次跑完 8 门，起跑前文件树静止。
> 这是**第三次**干净跑才 PASS 的一轮——前两次的失败成因见本节末尾「本轮两次假失败」。

| # | 门 | 本次实测（v1.4.18） | 判定 |
|---|----|------|------|
| 1 | 前端单测 | **54 文件 478/478 通过**（与 v1.4.17 持平：本轮只改造 `d133-d134-sync-guard.test.ts` 的既有断言，未新增文件） | ✅ 6s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 7s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（同既有浅色按钮对比度项，`3.74:1` 未达 AA） | ✅ 2s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **288 通过 / 0 失败 / 1 ignored**（较 v1.4.17 的 285：新增 `tests/sync_port_range.rs` 3 例；ignored 仍是 `real_vault_ledger`，需 `PMS_REAL_VAULT` 人工触发） | ✅ 405s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 2s |
| 7 | 体积门-桌面 | 历史 NSIS/MSI 逐个达标；1.4.18 正式产物 NSIS **1.95 MB**（2,044,554 B）+ MSI **2.77 MB**（2,904,064 B）≤ 10 MB | ✅ 12s |
| 8 | 体积门-Android | 历史 APK 逐个达标；1.4.18 签名 APK **7.37 MB**（7,725,483 B）≤ 12 MB | ✅ 3s |

**总判定：PASS（8 门全过 / 0 失败 / 0 跳过）。** 各门耗时相加约 438s（约 7 分 18 秒），Rust 门 405s 占 92%。

**本轮两次假失败（都不是代码问题，别照着改代码）：**

1. **Rust 门第一次 FAIL 是真失败**：`attachment_sync` 三例全红（`CoreError("ERR_IO", "connect|io:entity not found")`）。
   成因是把「端口不在 49152–65535 ⇒ 跳过静态连接」接进了 `resync.rs` 的连接分支，而本机动态端口范围
   被 WSL2/Hyper-V 改成 1024–65535，`bind(0)` 分到 29581——区间外却完全可连。**测试拦住的是一次会误杀
   真实地址的改动**，最终只保留日志标注（见 `docs/03` §13.17.2）。
2. **第二次 FAIL 是 rustc 卡死**：CPU 增量 0（20 分钟只耗 1.2s CPU），`taskkill` 后 `--keep-going` 继续跑
   反而把整轮拖超时。单独复现 `cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm`
   → **7m39s 全绿 exit 0**，证明与代码无关。**判据：`cargo test` 跑超过 10 分钟且 CPU 增量接近 0 就是卡死，
   杀掉重跑，不要怀疑用例。**

### 1.2e 最近一次完整实跑（2026-10-30 22:31 出包前 · v1.4.19 · D-139/D-138/D-140）

> 口径：`env -u NODE_OPTIONS bash scripts/gate-all.sh --keep-going` 一次跑完 8 门，整轮墙钟 **9 分 21 秒**，总判定 PASS。日志存 `target/_gate_1419.log`。

| # | 门 | 本次实测（v1.4.19） | 判定 |
|---|----|------|------|
| 1 | 前端单测 | **54 文件 479/479 通过**（较 v1.4.18 的 478：+1，`pairing-state.test.ts` 新增 completed 分支三件套守门 describe） | ✅ 5s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 9s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（同既有浅色按钮对比度项，`3.74:1` 未达 AA） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **292 通过 / 0 失败 / 1 ignored**（较 v1.4.18 的 288：+4，D-139 身份自愈 4 例——含「重置后必须稳定」幂等锚点与「IO 错误照旧上抛」负例；ignored 仍是 `real_vault_ledger`） | ✅ 519s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 8s |
| 7 | 体积门-桌面 | 历史 NSIS/MSI 逐个达标；1.4.19 正式产物 NSIS **1.96 MB**（2,050,761 B）+ MSI **2.77 MB**（2,908,160 B）≤ 10 MB | ✅ 12s |
| 8 | 体积门-Android | 历史 APK 逐个达标；1.4.19 签名 APK **7.37 MB**（7,729,579 B）≤ 12 MB | ✅ 4s |

**总判定：PASS（8 门全过 / 0 失败 / 0 跳过），一次干净跑通过（无假失败）。** Rust 门 519s 占整轮 92%，与 v1.4.18 持平。

### 1.2f 最近一次完整实跑（2026-10-31 出包前 · v1.5.0 · D-141）

> 口径：`env -u NODE_OPTIONS bash scripts/gate-all.sh --keep-going`，整轮墙钟 **8 分 15 秒**，总判定 PASS。日志存 `target/_gate_150.log`。
> 本轮代码改动**全在 Android 原生侧**（`AndroidManifest.xml` + `MainActivity.kt`）+ 文档，前端与 Rust 零改动，故 1/2/5 门读数应与 v1.4.19 完全持平——**持平本身就是「没碰坏」的证据**。

| # | 门 | 本次实测（v1.5.0） | 判定 |
|---|----|------|------|
| 1 | 前端单测 | **54 文件 479/479 通过**（与 v1.4.19 持平，符合预期） | ✅ 5s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 8s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（同既有浅色按钮对比度项，`3.74:1` 未达 AA） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **292 通过 / 0 失败 / 1 ignored**（与 v1.4.19 持平；ignored 仍是 `real_vault_ledger`） | ✅ 449s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 12s |
| 7 | 体积门-桌面 | 历史 NSIS/MSI 逐个达标；1.5.0 正式产物 NSIS **1.96 MB**（2,051,974 B）+ MSI **2.77 MB**（2,908,160 B）≤ 10 MB | ✅ 12s |
| 8 | 体积门-Android | 历史 APK 逐个达标；1.5.0 签名 APK **7.37 MB**（7,725,483 B）≤ 12 MB | ✅ 4s |

**注**：门禁不覆盖 Android 原生 Java/Kotlin 代码（无对应测试目标），D-141 的验证靠**真机 A/B**
（`docs/02` §3.28.2 的证据链：AppOps 由 `ignore` → `allow; time=+42s`，授权后 42 秒配对成功）。
**不得**把本节 8 门全绿表述为「D-141 已验」——两者是不同性质的门。

### 1.2g 最近一次完整实跑（2026-10-31 08:45 · v1.5.1 · D-142 / D-143）

> 口径：`env -u NODE_OPTIONS bash scripts/gate-all.sh --keep-going`，整轮墙钟 **7 分 23 秒**，总判定 **PASS（8/8，无跳过）**。日志存 `target/_gate_151.log`。
> 本轮改前端（`update-check.ts` / `Settings.svelte` / 四语言字典 / 测试）+ Android 清单 + 文档；**Rust 与 IPC 零改动**，故 Rust 门 292/0/1 与 v1.5.0 **完全持平**——持平即「没碰坏」。

| # | 门 | 本次实测（v1.5.1） | 判定 |
|---|----|------|------|
| 1 | 前端单测 | **55 文件 490/490 通过**（v1.5.0 是 54/479；本轮 +1 文件 +11 例：D-142 逻辑 5 例 + 守门 6 例） | ✅ 5s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 8s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（同既有浅色按钮对比度 `3.74:1` / `4.38:1`，未达 AA 正文 4.5 但过控件底线 3.0） | ✅ 1s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **292 通过 / 0 失败 / 1 ignored**（ignored 仍是 `real_vault_ledger`） | ✅ 409s |
| 6 | clippy | **0 warning**（`--workspace --all-targets`） | ✅ 2s |
| 7 | 体积门-桌面 | 历史 NSIS/MSI 逐个达标；1.5.1 正式产物 NSIS **1.96 MB**（2,051,270 B）+ MSI **2.77 MB**（2,908,160 B）≤ 10 MB | ✅ 10s |
| 8 | 体积门-Android | 历史 APK 逐个达标（15 个产物）；1.5.1 签名 APK **7.37 MB**（7,729,579 B）≤ 12 MB | ✅ 4s |

**1.5.1 实际产物（出包实测，非推算）**：
APK `7,729,579 B`（`748360bc`）/ NSIS `2,051,270 B`（`61796fd0`）/ MSI `2,908,160 B`（`c71c5119`）/
裸 exe `4,978,688 B`（`a4ee62f4`）/ 绿色版 zip `2,411,229 B`（`7f93be4d`，4 条目：
`WebView2Loader.dll`、`passwordms.exe`、`passwordms.portable`、`使用说明.txt`）；
**zip 内 exe 与裸 exe 哈希一致**。
`aapt2 dump xmltree` 解包取证：`targetSdkVersion=37`、`ACCESS_LOCAL_NETWORK` 在、
`allowBackup=false`、**`hasFragileUserData=true`** —— 四条属性全部真进了安装包。

**首轮曾 FAIL 1/8（真红，非假失败）**：守门测试最初用 `node:fs` + `process.cwd()` 读 `AndroidManifest.xml`，
`svelte-check` 直接报 `Cannot find module 'node:fs'` / `Cannot find name 'process'`（tsconfig 无 node 类型）。
改法：`import.meta.glob([...], { query: '?raw', eager: true })` 同时读 `.svelte` 与 `.xml`，
并加一条「守门本身别空跑」的长度断言（`>500` / `>1000`），防 glob 静默返回空集导致全绿假象。
**教训**：前端测试读后端/原生文件不许走 Node API，这也是本项目第一次在 `src/lib/*.test.ts` 里读 `src-tauri/` 下的文件。

**注**：门禁不覆盖 Android 原生清单语义。`hasFragileUserData` 是否真的让系统在卸载时弹「保留数据」，
只能真机验（需从**系统设置**卸载 → 弹框 → 勾选保留 → 重装同签名 APK → 看库是否回来）。
本轮**未做**该真机验证（会清掉主人手机上的真实数据），发布说明里据实标注为「待验证」。

### 1.2h 最近一次完整实跑（2026-10-31 12:14 · v1.5.2 · D-144 / D-145）

> 口径：`env -u NODE_OPTIONS bash scripts/gate-all.sh --keep-going`，整轮墙钟 **7 分 25 秒**，总判定 **PASS（8/8，无跳过）**。日志 `target/_gate_152.log`。
> 本轮改 Rust（`content_uri.rs` / `vault_cmd.rs`）+ 前端（`ipc.ts` / `vault-export.ts` / `save-dialog.ts` / `Settings.svelte` / 四语言 / 新模块 + 测试）。
> Rust 用例数 292 未变（只改实现不改用例）；前端 +21 例（490 → 511）。

| # | 门 | 本次实测（v1.5.2） | 判定 |
|---|----|------|------|
| 1 | 前端单测 | **57 文件 511/511 通过**（v1.5.1 是 55/490；本轮 +2 文件 +21 例：`backup-reminder` 9 + `d144-guard` 9 + `vault-export` +2 + `save-dialog` +1） | ✅ 6s |
| 2 | 前端类型检查 | **0 errors / 0 warnings** | ✅ 8s |
| 3 | CSS 层叠契约 | **0 FAIL / 6 WARN**（同既有浅色按钮对比度，未达 AA 正文 4.5 但过控件底线 3.0） | ✅ 2s |
| 4 | fmt | **0 处差异** | ✅ 1s |
| 5 | Rust 全量 | **292 通过 / 0 失败 / 1 ignored** | ✅ 404s |
| 6 | clippy | **0 warning** | ✅ 5s |
| 7 | 体积门-桌面 | 见下「实际产物」 | ✅ 14s |
| 8 | 体积门-Android | 见下「实际产物」 | ✅ 4s |

**首轮曾 FAIL 2 处（都是真红，不是假失败）**：

1. `cargo fmt` diffs=1 —— `export_vault` 的 `and_then` 闭包被 rustfmt 要求单行；`cargo fmt --all` 解决。
2. clippy `useless_conversion` —— `restore_backup` 里写了 `IpcError::from(backup_reject(...))`，
   而 `backup_reject` 已返回 `IpcError`（标准库有 `impl<T> From<T> for T`，所以能编译，但无意义）。
   去掉那层 `From` 即 0 warning。**这条值得记**：`map_err` 里包一层 `IpcError::from` 是习惯动作，
   一旦内层函数已经产出 `IpcError`，它就从「必要转换」变成「噪音」。

**变异验证记录（守门是否真会拦人）**：把 `content_write_size_mismatch` 改为
`size_check_removed` → `d144-guard` 真红 → 还原后 9/9 绿。
第一次改名用了 `content_write_size_mismatch_TEMP`，**因为新串包含旧串**，
`toContain` 照样通过、没有变红——**变异验证的改名必须让关键字整体消失**。

**1.5.2 实际产物（出包实测，非推算）**：
APK `7,733,675 B`（`877d9398`，V3 签名）/ NSIS `2,048,513 B`（`cba711f0`）/
MSI `2,908,160 B`（`f77f32a9`）/ 裸 exe `4,980,736 B`（`ad3543b4`）/
绿色版 zip `2,412,423 B`（`1259d356`，4 条目）；**zip 内 exe 与裸 exe 哈希一致**。

**1.5.6 门禁（D-70，2026-10-31 实跑）**：前端 `npm run test` **60 文件 528 例全绿**（本轮前端零业务改动；
唯一改动是 `ipc-contract.test.ts` 的 VK 白名单 4→5，见下）/ `npm run check` 0 error 0 warning /
CSS 契约 0 FAIL 6 WARN（老账未动）/ Rust `cargo test -p passwordms-core` 全绿（新增
`d70_identity_fingerprint.rs` 4 例 + `server.rs` 内 3 例指纹单测）/ `cargo fmt --all -- --check` 0 处差异 /
clippy 代码警告 0（唯一 `warning:` 仍是 build.rs windres 兜底）/ 体积门禁桌面 + APK 双过。

**先说一个被守门逮住的真实新增**：`ipc-contract.test.ts` 有一条「命令层直接取 VK 的调用点恒为 4 处
（新增必须有人来解释）」。D-70 在 `sync_join_impl` 锁内段① 取了一次 VK 去解封本机身份文件，
于是这条**立刻红**（`expected [...4] to deeply equal [...3]` 式的差异）。
这正是它存在的意义——当场必须回答「这里为什么也要裸 VK」：**只为读出公钥**随 HELLO 外发，
私钥不出函数、不落盘；与 `sync_start_pairing` 那一处同源同目的。白名单更新为 5 处并把理由写进注释。
**没有把它改成"忽略数量"，也没有放宽成 `<=`。**

**变异验证（两条，均真红）**：
① Host 改用临时公钥 `hello.joiner_pub` 算指纹 → `host_stores` / `legacy_client` /
`fingerprint_survives_repairing` **三条同时红**（含最能说明问题的
`left: "cc417c41…" right: "8bfbc9b3…"`——同一台手机两次配对得到不同指纹）；
② 手机改成发 `joiner_eph.public_bytes()`（临时公钥）→ 同样三条红。
**两条变异红在同一组用例上是对的**：它们破坏的是同一件事实（指纹必须来自长期身份）。

**1.5.6 实际产物（出包实测，非推算）**：
APK `7,737,771 B`（`122c4870`，V3 签名）/ NSIS `2,056,477 B`（`0e684b27`）/
MSI `2,912,256 B`（`e8396701`）/ 裸 exe `4,987,392 B`（`8bda36f4`，ProductVersion 1.5.6）/
绿色版 zip `2,416,423 B`（`45be57cc`，4 条目）；**zip 内 exe 与裸 exe 哈希一致**（`8bda36f4`）。

**包内取证（1.5.6）**——APK 体积 **第三次**与上一版逐字节相同（7,737,771 × 3 个版本），
MSI 也是（2,912,256）。这次拿到了**迄今最硬的一组判据：A/B 对拍同名字符串**：

| 判据 | 1.5.5 | 1.5.6 |
|---|---|---|
| APK 内 `.so` 字节数 | `5,703,592` | `5,703,280`（**−312 B**） |
| APK 内 `.so` sha256 | — | `560d48da…` |
| APK 整体 sha256 | `da336ae4…` | `122c4870…` |
| 包内 `tauri.conf.json` 的 `version` | `1.5.5` | `1.5.6` |
| `.so` 内 `joiner_identity_pub`（serde 字段名） | **0 次** | **1 次** |
| `.so` 内 `load for hello failed`（D-70 新增日志） | **0 次** | **1 次** |

**加代码而 `.so` 小 312 B**：这一点如实记录为**未解释**——代码布局/字符串表重排、
panic 消息去重都可能，我没有下结论。**但结论不依赖它**：`joiner_identity_pub` 是 serde 序列化
字段名（`&'static str`，运行时必需，LLVM 消不掉），1.5.5 里 0 次、1.5.6 里 1 次，
这是二进制级别的铁证。

**顺带固化一条方法论**：`IpcError::io(_detail)` 那种「形参未使用被 LLVM 消掉字面量」的坑（1.5.2 踩过）
**不适用于 serde 字段名**。以后验协议类改动，首选**搜 serde 字段名字符串**——
它必然进二进制、必然可 grep、必然新旧可对拍。

**1.5.18 门禁（D-162，2026-11-01 实跑）**：前端 `npm run test` **75 文件 656 例全绿**
（较 1.5.17 的 71/623 新增 2 文件 33 例：`otpauth.test.ts` 22 + `d162-totp-scan-guard.test.ts` 11；
另 `d97-totp-guard.test.ts` 一条断言形式跟改并要求「归一化早于试算」）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（本轮新增 4 条布局类，
未引入外观规则，WARN 仍是既有主按钮对比度老账）/ Rust **零改动**（只 `Cargo.toml` 版本串）/
`cargo fmt --all -- --check` 干净（EXIT=0）/ clippy 代码警告 0（唯一 warning 行是 windres 兜底提示）/
体积门禁桌面 + APK 双过（MSI 2.79 MB、NSIS 1.97 MB、APK 7.33 MB）。

**1.5.18 实际产物（出包实测，非推算）**：
APK `7,758,251 B`（`a255b128`，versionCode 1005018 / versionName 1.5.18；较 1.5.17 的
`7,750,059 B` **+8,192 B**，两个 4 KB 页）/ NSIS `2,062,632 B`（`d3402377`）/
MSI `2,920,448 B`（`89c4552a`）/ 裸 exe **`4,997,120 B`**（`919505df`，ProductVersion 1.5.18）/
绿色版 zip `2,423,889 B`（`c4c3e42f`）；**zip 内 exe 与裸 exe 哈希一致**（`919505df`，
同为 4,997,120 B）。`sha256sum -c` 在独立空目录四文件全 OK。

> **关于体积的一个校准（重要）**：本轮**新增**了约 400 行前端源码与 4 条 CSS，
> 但裸 exe 比 1.5.17 **小了 1,024 B**。所以 1.5.16 那轮用过的「exe 变小 ⇒ 方向是只删不增」
> 这条**不能反过来用**：前端资源是压缩后嵌入的，注释被剥离、代码被压缩、分包边界一变，
> 二进制大小与源码行数就没有线性关系。**体积只能当辅助观察，不能当方向性证据**——
> 进包与否一律仍由包内产物名判定。

**包内取证（1.5.18）——第六种输入：本轮改了样式（新增 4 条布局类）**：

| 判据 | 位置 | 1.5.17 | 1.5.18 |
|---|---|---|---|
| `CM52YUjI`（本次 `style-*.css`） | APK 内 `libpasswordms_lib.so` | **0** | **1** |
| `jc_arBlM`（1.5.14–1.5.17 的 CSS 名） | 同上 | 1 | **0** |
| `BXbKN0gA` / `DOKQxqCe`（`App-*.js` / `index-*.js`） | 同上 | **0** | **1** |
| `DwupKax5` / `DV6D32ED`（`QuickSearch-*.js` / `theme.svelte-*.js`） | 同上 | **0** | **1** |
| `D8W6LNvP` / `DFktmJF6` / `CHkM6JsF` / `7qndHU-K`（1.5.17 的四个 JS 名） | 同上 | 1 / 1 / 1 / 1 | **0 / 0 / 0 / 0** |
| 版本串 `1.5.17` / `1.5.18` | 同上 | 1 / 0 | **0 / 1** |

> **六轮六种输入，判据与改动内容始终对齐**：1.5.13 改样式 → CSS 名变；1.5.14–1.5.17 零样式
> → CSS 名**连续四版不变**；1.5.18 又动样式（新增 `.editor-scan-*` 四条布局类）→ CSS 名
> **再次变化**（`jc_arBlM` → `CM52YUjI`）。同一条判据在「改了」与「没改」两种输入下给出
> 两种结果，这才是它在测内容、而不是在测「必然变化」。桌面 `passwordms.exe` 的五个新名字
> 同样各出现 1 次，十个旧名字各 0 次。

**1.5.28 门禁（D-172 / Kimi F-07+F-08，2026-11-02 实跑）**：前端 `npm run test` **84 文件 796 例全绿**
（较 1.5.27 的 83/784：新增 `d172-import-retry-guard.test.ts` 12 例）/ `npm run check`
0 error 0 warning / CSS 契约 **0 FAIL 6 WARN**（同前：全是 F-09 浅色对比度口径，新增的
`.imp-result.is-error` 不在检查范围）/ `cargo fmt --all -- --check` 干净 /
`clippy --workspace --all-targets` 代码警告 0 / `cargo test -p passwordms-core` 全通过
（新增 `d172_import_atomic` **5 例**，含 Windows 只读库文件的**真实写盘失败**）。
体积门禁桌面 + APK 双过（NSIS 1.98 MB、APK 7.43 MB）。
**变异实测 10 条全真红**（`target/mutate-172.py`）。

> **本轮变异打「顺序」「回滚」和「消费时机」**：①core 退回逐条落库；②写完不做回读校验；
> ③计数落盘失败不回滚；④截断不判方向；⑤预览先删后拿锁；⑥失败不归还预览；
> ⑦成功也归还预览；⑧空批次照样落头部；⑨泰语字典少一条键；⑩失败提示不标错误色。
>
> **两条教训**：①「零新增不写盘」不能比文件长度——空批次若照样跑 `persist_header`，
> 长度一字不变、只有 `updated_at` 变了，比长度会放过它；要**比整文件字节**。
> ②「归还要在 Err 分支里」不能只写成「在 Ok 之后」——把归还写进 Ok 分支同样满足，
> 必须同时钉住 `Ok` 与 `Err` 两个分支的先后。
>
> **本轮未能覆盖的一处（如实记录）**：`Container::append_frames` 里「写了一半 IO 失败 →
> 截回」这条分支没有自动化用例。可以造出「打开即失败」（只读文件，已覆盖），造不出
> 「写 500 条后第 501 条失败」——那需要故障注入钩子，而生产代码里留测试探针的代价比
> 这条分支的收益大。已有的是：编码全在打开文件之前（编码失败零写入，有判据钉住）、
> 计数与回读失败两条路径都回滚（变异 M2/M3 证明判据咬得住）。

**1.5.28 实际产物（出包实测，非推算）**：
APK **`7,786,923 B`**（`48a2c3a4`，versionCode **1005028** / versionName 1.5.28）/
NSIS `2,072,443 B`（`8343ee0f`）/ MSI `2,936,832 B`（`53d1dc3f`）/
裸 exe **`5,033,472 B`**（`d76de4af`）/ 绿色版 zip `2,441,229 B`（`a0d5521e`）；
**zip 内 exe 与裸 exe 哈希一致**（`d76de4af`，同为 5,033,472 B）。

> APK 比 1.5.27 的 7,782,827 B 多 4,096 B（恰一个 4 KiB 对齐块）。体积对「代码是否进包」
> 依旧零信息量，取证看字符串。

**包内取证（1.5.28）——用「本轮才有的字符串」**：

| 判据 | 位置 | 计数 |
|---|---|---|
| `batch append failed`（`append_frames` 回滚失败时拼的成因串，`format!` 真用到） | 桌面 exe / APK `.so` | 1 / 1 |
| `rollback to`（同上，同一句里的第二段） | 桌面 exe / APK `.so` | 1 / 1 |
| `folder_conflict_no_keep_both`（1.5.27 引入，确认仍在） | 桌面 exe / APK `.so` | 1 / 1 |
| `dist/index.html` mtime 早于 APK | — | 22:15:30 < 22:18:44 |
| 前端 CSS 分块名随本轮变化 | `style-CpIyiKAB` → `style-NEnNv6yg` | 改过导入向导样式，符合预期 |

**1.5.27 门禁（D-171 / Kimi F-01，2026-11-02 实跑）**：前端 `npm run test` **83 文件 784 例全绿**
（较 1.5.26 的 82/770：新增 `d171-folder-conflict-guard.test.ts` 14 例）/ `npm run check`
0 error 0 warning / CSS 契约 **0 FAIL 6 WARN**（与 1.5.26 同：全是已知 F-09 浅色对比度口径）/
`cargo fmt --all -- --check` 干净 / `clippy --workspace --all-targets` 代码警告 0 /
`cargo test -p passwordms-core` 全通过（新增 `d171_folder_conflict` **13 例**）。
体积门禁桌面 + APK 双过（NSIS 1.97 MB、APK 7.42 MB）。
**变异实测 11 条全真红**（`target/mutate-171.py`）。

> **本轮变异打「分流」和「可见性」**：①分组双改直接覆盖；②分组删除不看本端 rev；
> ③KeepLocal 不抬 rev；④TakeRemote 高抬 rev；⑤分组允许 keep_both；⑥仲裁分流放在条目之后；
> ⑦分组冲突复用 0x04；⑧命令层只列条目；⑨面板写死三选一；⑩分组选项塞回 keep_both；
> ⑪泰语字典少一条键。
>
> **两条教训**：①`M2` / `M3` 的第一版片段在**条目与分组分支里各出现一次**（`Some(local) if
> local.rev > change.rev` / `winner.rev = ...max(...) + 1`），`replace(..., 1)` 只改到条目那处，
> 分组语义没变当然不红。判据片段必须**带上分组特有的上下文**（`get_folder_opt` 那一行 /
> `let folder_id = winner.folder_id` 那一行）才算钉住。②`en-US.ts` 里给注释写 "the conflict
> box" 直接把 `d105-locale-polish` 撞红——那条守卫是防英文文案里出现「直译腔」的，注释也算。

**1.5.27 实际产物（出包实测，非推算）**：
APK **`7,782,827 B`**（`46e5d14f`，versionCode **1005027** / versionName 1.5.27）/
NSIS `2,069,843 B`（`da80b2f3`）/ MSI `2,932,736 B`（`d85ee0de`）/
裸 exe **`5,027,840 B`**（`9e772eff`）/ 绿色版 zip `2,438,401 B`（`b7289e0a`）；
**zip 内 exe 与裸 exe 哈希一致**（`9e772eff`，同为 5,027,840 B）。

> **APK 连六版等体积（7,762,347 B，1.5.21–1.5.26）到此结束**，本轮 +20,480 B。上一轮已记
> 「体积零信息量」：`.so` 里的实际字节变化会被 zipalign 的 4 KiB 对齐吸收掉，直到越过下一个
> 对齐边界才反映到 APK 大小上。故**体积变化既不能证明也不能否定代码进了包**，取证仍看字符串。

**包内取证（1.5.27）——只动 Rust 与前端，用「本轮才有的字符串」取证**：

| 判据 | 位置 | 计数 |
|---|---|---|
| `folder_conflict_no_keep_both`（本轮新增错误码） | 桌面 exe / APK `.so` | 1 / 1 |
| `.restore-tmp`（1.5.26 引入，确认仍在） | APK `.so` | 1 |
| `dist/index.html` mtime 早于 APK | — | 21:18:36 < 21:21:55 |
| 前端 CSS 分块名随本轮变化 | `style-DlGng6yX` → `style-CpIyiKAB` | 改过面板样式，符合预期 |

**1.5.26 门禁（D-170 / Kimi F-02+F-03，2026-11-02 实跑）**：前端 `npm run test` **82 文件 770 例全绿**
（较 1.5.25 的 81/760：新增 `d170-backup-restore-guard.test.ts` 10 例）/ `npm run check`
0 error 0 warning / CSS 契约 **0 FAIL 6 WARN** / `cargo fmt --all -- --check` 干净 /
`clippy --workspace --all-targets` 代码警告 0 / `cargo test -p passwordms-core` 全通过
（`restore_backup` 由 6 例扩到 **12 例**）/ 体积门禁桌面 + APK 双过（NSIS 1.97 MB、APK 7.34 MB）。
**变异实测 8 条全真红**（`target/mutate-170.py`）。

> **本轮变异打「顺序」和「接线」**：①恢复改回直接覆盖；②删掉可打开性复检；③错误码多一个
> 字母；④先 rename 再 sync_all；⑤失败不清候选文件；⑥`open` 又长回自己一套 magic 判定；
> ⑦src-tauri 消费分支错拼；⑧泰语字典键名写错。
>
> **③ 的教训值得记：前端那条守卫只红了 0 次**（`ts=0`），全靠 Rust 测试抓住。原因是
> `container.rs` 里 `"unsupported_vault_backup"` 出现两次，脚本只替换第一处，剩下那处
> 让 `toContain` 依旧成立。**「多处出现的关键字」用 `replace(..., 1)` 做变异，
> 只能证明「其中一处还在」**——要么全替换，要么改判据只钉单点。

**1.5.26 实际产物（出包实测，非推算）**：
APK **`7,762,347 B`**（`d56de660`，versionCode **1005026** / versionName 1.5.26）/
NSIS `2,062,797 B`（`f96210d8`）/ MSI `2,924,544 B`（`6d9b8c06`）/
裸 exe **`5,009,920 B`**（`07c87d16`）/ 绿色版 zip `2,429,805 B`（`d891039c`）；
**zip 内 exe 与裸 exe 哈希一致**（`07c87d16`，同为 5,009,920 B）。

> **APK 连续六版 7,762,347 B**（1.5.21–1.5.26）。本轮 `.so` 实测 5,726,656 B（构建日志
> 四门比对通过），差异又被 zipalign 吸收——体积依旧零信息量。

**包内取证（1.5.26）——只动 Rust，用「本轮才有的字符串」取证**：

| 判据 | 位置 | 计数 |
|---|---|---|
| `.restore-tmp`（本轮新增的候选文件后缀，`format!` 真用到） | 桌面 exe / APK `.so` | 1 / 1 |
| `unsupported_vault_backup`（本轮新增错误码） | 桌面 exe / APK `.so` | 1 / 2 |
| `dist/index.html` mtime 早于 APK | — | 19:38:22 < 19:41:57 |

**1.5.25 门禁（D-169，2026-11-02 实跑）**：前端 `npm run test` **81 文件 760 例全绿**
（较 1.5.24 的 80/743：新增 `d169-settings-instant-save-guard.test.ts` 17 例）/ `npm run check`
0 error 0 warning / CSS 契约 **0 FAIL 6 WARN** / **Rust 零改动**：`cargo fmt --all -- --check`
干净、`clippy --workspace --all-targets` 代码警告 0、`cargo test -p passwordms-core` 全通过 /
体积门禁桌面 + APK 双过（NSIS 1.97 MB、APK 7.34 MB）。**变异实测 8 条全真红**
（`target/mutate-169.py`）。

> **本轮变异打「生效方式」**：①空串不特判（回落到 0 秒）；②不夹紧（0 与 9999 都能进库）；
> ③少接一个输入框；④把「保存」按钮加回来；⑤退回会连弹「已保存」的 `save()`；
> ⑥保存失败静默；⑦`min` 写死回数字；⑧一种语言漏了「立即生效」说明。
>
> **过程中真被自己的变异脚本坑了一次**：Python 在 Windows 上默认写文件会把 `\n` 转成
> `\r\n`，脚本把 `Settings.svelte`（原本 LF）整体改成 CRLF，导致 `vault-compact-ui` 与
> `d115-bio-state-guard` 两个历史测试变红——它们用**跨行**字面量 `'<Modal\n  open='` 做
> 切片，对换行敏感；而单行标记的测试完全看不出来。**诊断顺序值得记住**：先怀疑「我改的代码」，
> 但看到失败都在同一个从未触碰的 `Modal` 上时，就该去量文件字节而不是继续读代码。
> 脚本已改为 `newline=""` 读写 + 比较前归一化成 LF、写回还原原风格。

**1.5.25 实际产物（出包实测，非推算）**：
APK **`7,762,347 B`**（`5571c5f3`，versionCode **1005025** / versionName 1.5.25）/
NSIS `2,062,152 B`（`780e464d`）/ MSI `2,924,544 B`（`588e6c66`）/
裸 exe **`5,007,360 B`**（`820e0125`）/ 绿色版 zip `2,428,639 B`（`fd7c6dfc`）；
**zip 内 exe 与裸 exe 哈希一致**（`820e0125`，同为 5,007,360 B）。

> **APK 连续五版 7,762,347 B**（1.5.21–1.5.25），sha256 五版各异；MSI 第四次落在
> 2,924,544 B。体积零信息量，这是第五次现场。

**包内取证（1.5.25）——只动 JS、不动 CSS，带对照组**：

| 判据 | 位置 | 1.5.24 | 1.5.25 |
|---|---|---|---|
| `App-B93wloVb`（新 JS 分块名） | 桌面 exe / APK `.so` | 0 | **1** |
| `App-s2fcPotj`（上一版 JS 分块名） | 同上 | 1 | **0** |
| `style-DlGng6yX`（CSS 名） | 同上 | 1 | **1（不变）** |

**1.5.24 门禁（D-168，2026-11-02 实跑）**：前端 `npm run test` **80 文件 743 例全绿**
（较 1.5.23 的 79/731：新增 `d168-detail-scope-guard.test.ts` 12 例；`d146` 与 `d166` 里
「提示行渲染条件」的切片标记随模板改写为 `{#if copyHintVisible}`——**有意的契约变更**，
同一份契约换了表达方式，判据本身没有放松）/ `npm run check` **0 error 0 warning** /
CSS 契约 **0 FAIL 6 WARN** / **Rust 零改动**：`cargo fmt --all -- --check` 干净、
`clippy --workspace --all-targets` 代码警告 0、`cargo test -p passwordms-core`
67 + 6 + 3 + 3 + 5 全通过 / 体积门禁桌面 + APK 双过（NSIS 1.97 MB、APK 7.34 MB）。
**变异实测 8 条全真红**（`target/mutate-168.py`）。

> **本轮变异打「状态归属」**：①判据去掉「同一条目」；②只判相等不判 null（空详情页也显示）；
> ③模板退回「倒计时没走完就显示」；④复制时不记归属；⑤归零不清归属；
> ⑥当前条目直接拿归属值（恒真）；⑦复位 effect 去掉 id 守卫；⑧守卫挪到清空之后。
>
> **⑧ 第一版写错过**：我把 `lastResetId = id;` 挪到清空之后，然而守卫仍在最前面，
> 语义与原代码**完全等价**——它当然不该红。这说明变异本身也要审：**等价变异不是漏测**。
> 重新设计成「守卫整块挪到清空之后」后立刻变红。这条过程记在这里，避免下次再把
> 「变异没红」一律当成判据不足。
>
> 顺带修掉一处工程噪音：`Reference/`（参考的第三方密码管理器源码）里的 `vite.config.ts`
> 被 svelte-check 当作本项目配置加载，报 `Cannot find package '@tailwindcss/vite'`
> ——已在 `check` 脚本加 `--config ./vite.config.ts`（指定后其下层配置一概忽略）。

**1.5.24 实际产物（出包实测，非推算）**：
APK **`7,762,347 B`**（`1a537536`，versionCode **1005024** / versionName 1.5.24）/
NSIS `2,061,049 B`（`8cbb3484`）/ MSI `2,924,544 B`（`346f6f33`）/
裸 exe **`5,005,824 B`**（`3366742b`）/ 绿色版 zip `2,427,927 B`（`528efe57`）；
**zip 内 exe 与裸 exe 哈希一致**（`3366742b`，同为 5,005,824 B）。

> **APK 连续四版 7,762,347 B**（1.5.21–1.5.24），sha256 四版各异；MSI 第三次落在
> 2,924,544 B。体积对「有没有变」的信息量仍然为零。

**包内取证（1.5.24）——只动 JS、不动 CSS，带对照组**：

| 判据 | 位置 | 1.5.23 | 1.5.24 |
|---|---|---|---|
| `App-s2fcPotj`（新 JS 分块名） | 桌面 exe / APK `.so` | 0 | **1** |
| `App-pRIzlpJ3`（上一版 JS 分块名） | 同上 | 1 | **0** |
| `style-DlGng6yX`（CSS 名） | 同上 | 1 | **1（不变）** |

**1.5.23 门禁（D-167，2026-11-02 实跑）**：前端 `npm run test` **79 文件 731 例全绿**
（较 1.5.22 的 78/713：新增 `d167-totp-clipboard-ttl-guard.test.ts` 18 例；无既有断言被改写
——这一轮改的是「算出来的时长」，不是接口形状）/ `npm run check` 0 error 0 warning /
CSS 契约 **0 FAIL 6 WARN**（仍是主按钮对比度老账）/ **Rust 零改动**：`cargo fmt --all -- --check`
干净、`clippy --workspace --all-targets` 代码警告 0、`cargo test -p passwordms-core`
67 + 6 + 3 + 3 全通过 / 体积门禁桌面 + APK 双过（NSIS 1.96 MB、APK 7.34 MB）。
**变异实测 8 条全真红**（`target/mutate-167.py`）。

> **本轮变异专打「算对没算对」**：①`min` 改 `max`；②字段判定写反（用户名反而跟口令寿命）；
> ③提醒边界 `<=` 改 `<`；④组件拿 `totpRemaining` 当剩余寿命（挂载即清空）；
> ⑤计划算完丢掉、退回设置值；⑥提醒条件反转；⑦短寿提示整行删掉；⑧一种语言漏新文案。
>
> 为了让 ⑥ 能红，`d167` 对提醒分支的断言写的是完整字面 `if (plan.warnExpiring) {`——
> 只查「这个词出现过」的话，反转成 `!plan.warnExpiring` 照样通过，而界面行为正好相反。
> 同理 `n: ttl` 按出现次数断言为 2，挡住「提示里报设置值 30 而实际只留 12 秒」。
> **已知挡不住的形态**：组件层插一句 `if (false && plan.warnExpiring)` 文本判据识别不了，
> 与 1.5.22 记的那条同源（无组件挂载能力）。如实记下，不假装测得。

**1.5.23 实际产物（出包实测，非推算）**：
APK **`7,762,347 B`**（`dd1d0c6c`，versionCode **1005023** / versionName 1.5.23）/
NSIS `2,060,374 B`（`def12b98`）/ MSI `2,924,544 B`（`3d53f543`）/
裸 exe **`5,006,848 B`**（`181d76f6`）/ 绿色版 zip `2,428,435 B`（`81bcc515`）；
**zip 内 exe 与裸 exe 哈希一致**（`181d76f6`，同为 5,006,848 B）。

> **APK 连续三版 7,762,347 B**（1.5.21 / 1.5.22 / 1.5.23 字节数完全相同），sha256 三版各异。
> MSI 也回到 2,924,544 B（与 1.5.21 同）。体积为零信息量这件事，这已经是第五次现场。

**包内取证（1.5.23）——只动 JS、不动 CSS（与 1.5.22 同型，带对照组）**：

| 判据 | 位置 | 1.5.22 | 1.5.23 |
|---|---|---|---|
| `App-pRIzlpJ3`（新 JS 分块名） | 桌面 exe / APK `.so` | 0 | **1** |
| `App-RpSjlegD`（上一版 JS 分块名） | 同上 | 1 | **0** |
| `style-DlGng6yX`（CSS 名） | 同上 | 1 | **1（不变）** |

> 桌面 exe 与 APK `.so` 三行读数**完全一致**，说明两端烤进去的是同一份 `dist/`。

**1.5.22 门禁（D-166，2026-11-02 实跑）**：前端 `npm run test` **78 文件 713 例全绿**
（较 1.5.21 的 77/705：新增 `d166-totp-copy-guard.test.ts` 8 例；`d146` 的「复制出口计数」
与 `d135` 的同名断言由 2 改 3 —— 这是**有意的契约变更**，不是为了让测试变绿）/ `npm run check`
0 error 0 warning / CSS 契约 **0 FAIL 6 WARN**（仍是既有主按钮对比度老账）/ **Rust 零改动**：
`cargo fmt --all -- --check` 干净、`clippy --workspace --all-targets` 代码警告 0、
`cargo test -p passwordms-core` 三组全通过（67 + 6 + 3）/ 体积门禁桌面 + APK 双过。
**变异实测 6 条全真红**（`target/mutate-166.py`）。

> **本轮变异专门打「映射类」缺陷**：①复制按钮把口令当密码报；②空口令也能点；
> ③映射表把 `totp` 指向密码的标签；④提示行写死查「密码」不看实际字段；
> ⑤复制完不记录字段；⑥口令复制绕开带 TTL 的统一路径（`ttl` 传 0）。
> 六条都能让 `d166` 变红，说明这组断言测的是**行为**而不是「代码里写着这个词」。
> 仍有一条文本判据挡不住的形态（在 `copyText` 开头插一句 `if (field === 'totp') return;`）：
> 没有组件挂载能力时无法证伪，已如实记在这里，不假装测得。

**1.5.22 实际产物（出包实测，非推算）**：
APK **`7,762,347 B`**（`6290722`，versionCode **1005022** / versionName 1.5.22）/
NSIS `2,061,243 B`（`cb28e116`）/ MSI `2,920,448 B`（`c825ebed`）/
裸 exe **`5,002,240 B`**（`c865f5fa`）/ 绿色版 zip `2,426,223 B`（`25132b8d`）；
**zip 内 exe 与裸 exe 哈希一致**（`c865f5fa`，同为 5,002,240 B）。

> **体积撞车两处，都要靠哈希说话**：APK 与 1.5.21 **字节数完全相同**（7,762,347 B），
> MSI 与 1.5.19/1.5.20/1.5.21 **连续四版同为 2,920,448 B**，而两者的 sha256 都变了。
> 压缩/对齐容器会把几十字节的差异吸收进对齐填充里 —— 体积既不能证明变了，也不能证明没变。

**包内取证（1.5.22）——第十种输入：只动 JS、不动 CSS**：

| 判据 | 位置 | 1.5.21 | 1.5.22 |
|---|---|---|---|
| `App-RpSjlegD`（新 JS 分块名） | 桌面 exe / APK `.so` | 0 | **1** |
| `App-B1TMs8hf`（旧 JS 分块名） | 同上 | 1 | **0** |
| `style-DlGng6yX`（CSS 名） | 同上 | 1 | **1（不变）** |

> 这一轮的价值在于**对照组**：改了前端（新增按钮 + 文案 + 新模块）而**没改任何样式**，
> 于是 JS 分块名必须变、CSS 名必须不变。两条同时成立，才说明「名字确实跟着内容走」，
> 而不是某种必然变化的噪声。

**1.5.21 门禁（D-165，2026-11-02 实跑）**：前端 `npm run test` **77 文件 705 例全绿**
（较 1.5.20 的 76/695：新增 `d165-screen-crop-guard.test.ts` 23 例；`d164` 随截图链路整体迁移
删掉 13 例、`d163` 删 1 例）/ `npm run check` 0 error 0 warning / CSS 契约 **0 FAIL 6 WARN**
（WARN 仍是既有主按钮对比度老账）/ Rust 有改动：`cargo fmt --all` 干净 /
`clippy -p passwordms --all-targets` **代码警告 0** / `cargo test -p passwordms-core` 全通过
（exit 0，含 doc-test）/ 体积门禁桌面 + APK 双过。
**变异实测 10 条全真红**。

> **clippy 这一轮抓到一条，值得记下来**：首轮 `clippy` 报 `chunks_exact_mut(4)` 建议改用
> `as_chunks_mut::<4>()`（`clippy::chunks_exact_to_as_chunks`）。这属于「已经构建完成之后
> 才改的源码」—— 语义完全等价，但**产物必须与源码一致**，否则提交里的代码与 Release 里的
> 二进制对不上号。于是把桌面与 Android 各重建了一遍，重算全部哈希后才发布。**宁可多花
> 六分钟，也不要让「源码 ≠ 产物」这种说不清的事留在仓库里。**

**1.5.21 实际产物（出包实测，非推算）**：
APK `7,762,347 B`（`3863ac6e`，versionCode **1005021** / versionName 1.5.21）/
NSIS `2,062,514 B`（`29219721`）/ MSI `2,924,544 B`（`451378b1`）/
裸 exe **`5,005,824 B`**（`2e0c84a4`）/ 绿色版 zip `2,428,179 B`（`f1d878db`）/
清单 `SHA256SUMS-1.5.21.txt` 391 B（`35bb9b93`）；
**zip 内 exe 与裸 exe 哈希一致**（`2e0c84a4`，同为 5,005,824 B）；`sha256sum -c` 在独立空目录四文件全 OK。

**包内取证（1.5.21）——第九种输入：既有新增也有删除**：

| 判据 | 位置 | 1.5.20 | 1.5.21 |
|---|---|---|---|
| `capture_screen` | 桌面 exe / APK `.so` | **0** | **1** |
| `open_screenshot_tool` | 同上 | **1** | **0** |
| `clipboard_sequence_number` | 同上 | **1** | **0** |
| `style-CM52YUjI`（旧 CSS 名） | APK `.so` | 1 | **0** |
| `style-DlGng6yX`（新 CSS 名） | 同上 | 0 | **1** |

> **这一轮是本判据第一次出现「双向删除」**：前八轮都是「新名字 0→1」，这次连
> **旧的命令名 1→0** 也测到了 —— 因为这一版把系统截图工具那条链路整套删掉了。
> 一条判据能同时反映「加了什么」和「删了什么」，说明它测的是**二进制里的真实内容**，
> 而不是某种必然成立的形式。
>
> **样式判据在这一轮也再次对齐**：1.5.18 改样式 → CSS 名变；1.5.19、1.5.20 零样式
> → 连续两版不变；1.5.21 新增 `ScreenCrop` 组件样式 → `CM52YUjI` → `DlGng6yX`。
> 同一判据在「改了」（四种输入）与「没改」（四种输入）下都给出一致结论。

**1.5.20 门禁（D-164，2026-11-02 实跑）**：前端 `npm run test` **76 文件 695 例全绿**
（较 1.5.19 的 74/669：新增 `d164-totp-clipboard-robust-guard.test.ts` 20 例 +
`d164-audit-fixes-guard.test.ts` 6 例 + `d163` 第⑥例切片跟改）/ `npm run check` 0 error 0 warning /
CSS 契约 0 FAIL 6 WARN（本轮**零样式改动**，WARN 仍是既有主按钮对比度老账）/
Rust 有改动（`container.rs` 补 `sync_all`、`repository.rs` 调墓碑顺序）：`cargo fmt --all` 干净 /
`clippy -p passwordms-core --all-targets` 代码警告 0 / `cargo test -p passwordms-core` 全通过
（exit 0，含 doc-test）/ 体积门禁桌面 + APK 双过。
**变异实测 16 条全真红**（13 条功能判据 + 3 条审计修复判据）。

**1.5.20 实际产物（出包实测，非推算）**：
APK `7,758,251 B`（`53df1b5d`，versionCode **1005020** / versionName 1.5.20）/
NSIS `2,067,410 B`（`7ab29904`）/ MSI `2,920,448 B`（`69ccc6f8`）/
裸 exe **`5,006,336 B`**（`69bbff3a`）/ 绿色版 zip `2,427,252 B`（`25606316`）；
**zip 内 exe 与裸 exe 哈希一致**（`69bbff3a`，同为 5,006,336 B）。
`sha256sum -c` 在独立空目录四文件全 OK。

**包内取证（1.5.20）——第八种输入：前端大改、零样式改动、零新命令**：

| 判据 | 位置 | 1.5.19 | 1.5.20 |
|---|---|---|---|
| `App-CvCu1Qzv` / `QuickSearch-y5pSExLF` / `index-Bi6MqdAm` / `theme.svelte-CGB4j8MN` | APK 内 `libpasswordms_lib.so` | 各 1 | **各 0** |
| `App-BbU5DDXn` / `QuickSearch-BeN7ukxd` / `index-BIocB0a_` / `theme.svelte-CSIzjUN-` | 同上 | 各 0 | **各 1** |
| `style-CM52YUjI`（`style-*.css`） | 同上 | 1 | **1（不变）** |
| 版本串 `1.5.19` | APK `.so` / MSI | 1 / — | **0 / 0** |
| 版本串 `1.5.20` | 同上 | — | **1 / 1** |

> **这一轮是本判据最干净的一次正反对照**：四个 JS 名双向 1→0、0→1，一个不多一个不少；
> 同时 `style-CM52YUjI` **第三次保持不动**（1.5.17、1.5.18 之后又一轮零样式改动）。
> 「改了会变、没改不变」在八种输入下始终对齐，这条判据才算真的在测内容。

> **体积第三次撞车，这次把成因查到了底**：APK 与 MSI 的体积**连续三版完全相同**
> （`7,758,251 B` / `2,920,448 B`）。1.5.19 已经用哈希 + mtime + 包内 `.so` 排除过一遍，
> 但连续三版一致就不再像巧合，于是本轮补了两条更硬的取证：
> ① **APK 内 `.so` 大小实际变了**（5,725,440 → 5,725,968，+528 B），而 APK 总大小不变 ——
> 因为 zipalign 按 4 字节对齐，多出来的字节被对齐填充吸收；
> ② **MSI 里 `1.5.19` 出现 0 次、`1.5.20` 出现 1 次**（ProductVersion）—— 内容是新的。
> 结论修正上一版的表述：**这不是"可以恰好相同"的偶发，而是压缩/对齐容器的结构性常态**。
> 体积在这类产物上既不能证明"变了"，也不能证明"没变"，只能当辅助观察。

> **本轮变异暴露了判据的共性弱点（三次漏测）**：第一轮 11 条变异里有 3 条跑成 GREEN，
> 全都是同一个毛病 —— **断言了「代码写着」，没断言「代码真的生效」**：
> 函数定义了但没注册、`show()` 在 finally 里但条件恒假、`mix(rgba)` 存在但前面加了提前 return。
> 修法分两类：前两条补「注册行存在」「以 `hidden` 为条件」的结构断言；
> 第三条改成**行为判据**（导出 `fingerprintOf`，喂两份只差一个像素的同尺寸数据断言指纹不同）——
> 文本判据永远挡不住"加一行提前 return"，行为判据能。

**1.5.19 门禁（D-163，2026-11-02 实跑）**：前端 `npm run test` **74 文件 669 例全绿**
（较 1.5.18 的 75/656：新增 `d163-totp-clipboard-guard.test.ts` 13 例；文件数少 1 是因为
本轮只新增一个测试文件，而 1.5.18 是两个）/ `npm run check` 0 error 0 warning /
CSS 契约 0 FAIL 6 WARN（本轮**零样式改动**，WARN 仍是既有主按钮对比度老账）/
Rust 有改动（+2 命令、+1 capability、+1 windows feature）：`cargo fmt --all` EXIT=0 /
clippy 代码警告 0 / 体积门禁桌面 + APK 双过。

**1.5.19 实际产物（出包实测，非推算）**：
APK `7,758,251 B`（`4480b13e`，versionCode **1005019** / versionName 1.5.19）/
NSIS `2,062,204 B`（`d31902a2`）/ MSI `2,920,448 B`（`4de03f1f`）/
裸 exe **`5,004,288 B`**（`4a05e315`）/ 绿色版 zip `2,426,744 B`（`bafbb99f`）；
**zip 内 exe 与裸 exe 哈希一致**（`4a05e315`，同为 5,004,288 B）。
`sha256sum -c` 在独立空目录四文件全 OK。

> **本轮踩到一个「体积撞车」的假信号（值得记）**：APK 与 MSI 的体积**与 1.5.18 完全相同**
> （`7,758,251 B` / `2,920,448 B`），而两个哈希确实都变了。第一反应是「脚本是不是把旧包
> 改名交上来了」——确实该这么怀疑。查证路径：① 哈希不同 → 排除直接复制；② mtime 是本次
> 构建 → 排除保留旧文件；③ 包内 `lib/arm64-v8a/libpasswordms_lib.so` 哈希 `cb2324de`
> （1.5.18 是 `2f60e636`）→ 排除「同包不同名」；④ 二进制里两个新命令名各 1 处、
> 1.5.18 里各 0 处 → 确认新代码真的进了包。
> 结论：zip/MSI 都是压缩容器，内容变化后压缩体积**可以**恰好相同。**体积相同不构成"没变"的证据，
> 就像体积变小不构成"只删不增"的证据**（1.5.18 那条教训的另一面）。

**包内取证（1.5.19）——第七种输入：本轮零样式改动、但有新增 Rust 命令**：

| 判据 | 位置 | 1.5.18 | 1.5.19 |
|---|---|---|---|
| `CM52YUjI`（`style-*.css`） | APK 内 `libpasswordms_lib.so` | 1 | **1（不变）** |
| `Bi6MqdAm` / `CvCu1Qzv` / `CGB4j8MN` / `y5pSExLF`（四个新 JS 名） | 同上 | — | **各 1** |
| 1.5.18 的四个 JS 名（`BXbKN0gA` 等） | 同上 | 各 1 | **各 0** |
| `open_screenshot_tool` | 桌面 exe / APK `.so` | **0** | **1** |
| `clipboard_sequence_number` | 同上 | **0** | **1** |
| 版本串 `1.5.18` | APK `.so` | 1 | **0** |
| 版本串 `1.5.19` | APK `.so` | **1** | 2 |

> **CSS 名本轮不变，正是它该有的表现**：这一版只改了 Svelte 模板结构、图标 path 字符串和
> Rust 命令，没有动任何 CSS 规则，所以 `style-CM52YUjI.css` 必须还是原来那个名字。
> 加上 1.5.18 的「改了 → 变」和 1.5.14–1.5.17 的「没改 → 四版不变」，
> 这条判据现在在**三种输入**下都给出了与事实一致的结果。
>
> **另一条教训：包内取证要选「应用自己新增的标识」**。本轮本想用 `allow-read-image`
> （新加的 capability 串）做对照，实测 1.5.18 的 exe 里也有 1 处 —— 那来自
> `tauri-plugin-clipboard-manager` 自带的 ACL 权限清单，插件被链接进二进制时就在那里了，
> 与我们**是否启用**该权限无关。拿它当判据会得出「旧包也有新功能」的错误结论。
> 只有 `open_screenshot_tool` / `clipboard_sequence_number` 这种由本次改动**首次引入**的
> 字符串，才有 0 → 1 的对照意义。
>
> **版本串也不是干净判据**（同一轮实测）：`1.5.19` 在 **1.5.18 的** `.so` 里就已经出现 1 次
> —— 那是某个依赖的版本号子串（形如 `1.5.190` 之类的字节序列），不是我们的版本。
> 所以这一行的两个方向里只有「旧版本串归零」可用，「新版本串出现」不可用。
> 这也解释了为什么前面几轮一直用**产物名**（哈希化的 JS/CSS 名）当主判据：
> 那些名字是构建产物自己生成的、长度足够、不可能在依赖里撞上。


**1.5.17 门禁（D-161，2026-11-01 实跑）**：前端 `npm run test` **71 文件 623 例全绿**
（较 1.5.16 的 70/612 新增 1 文件 5 例 `d161-favorite-scope-guard.test.ts` + store D-161 段 6 例；
另 `d153-ungrouped-filter-guard.test.ts` 一条既有断言形式跟改，语义未变）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（本轮零样式改动）/
Rust core 零改动（仅 `Cargo.toml` 版本串）/ `cargo fmt --all -- --check` 干净（EXIT=0）/
clippy 代码警告 0（唯一 warning 行仍是 windres 兜底的 build-script 提示）/
体积门禁桌面 + APK 双过（MSI 2.78 MB、NSIS 1.96 MB、APK 7.39 MB）。

**1.5.17 实际产物（出包实测，非推算）**：
APK `7,750,059 B`（`1c3a4a59`，versionCode 1005017 / versionName 1.5.17；**较前八版共同的
`7,754,155 B` 少了 4,096 B —— 第九版首次变化**，形态上是「删掉一行赋值 + 新增一个小方法」的
如实体现；前八版字节数完全相同的原因仍未解释，故所有进包判断依旧只认包内证据）/
NSIS `2,056,344 B`（`be10eaba`）/ MSI `2,916,352 B`（`b59e3c63`）/
裸 exe **`4,998,144 B`**（`c194b337`，ProductVersion 1.5.17；较 1.5.16 的 4,994,560 B
**+3,584 B**——新增 `setScope` 方法与注释）/ 绿色版 zip `2,422,841 B`（`88c1bf37`）；
**zip 内 exe 与裸 exe 哈希一致**（`c194b337`，同为 4,998,144 B）。
`sha256sum -c` 在**独立空目录**（四个资产按发布名拷入）四文件全 OK。

**包内取证（1.5.17）——第五种输入：改 store 结构、零样式、零新 UI**：

| 判据 | 位置 | 1.5.16 | 1.5.17 |
|---|---|---|---|
| `D8W6LNvP`（本次 `App-*.js`） | APK 内 `libpasswordms_lib.so` | **0** | **1** |
| `DFktmJF6`（本次 `index-*.js`） | 同上 | **0** | **1** |
| `CHkM6JsF`（`QuickSearch-*.js`） | 同上 | **0** | **1** |
| `7qndHU-K`（`theme.svelte-*.js`） | 同上 | **0** | **1** |
| `ptvdewIa` / `C_KvDGpM`（1.5.16 的 JS 名） | 同上 | 1 / 1 | **0 / 0** |
| `jc_arBlM`（`style-*.css`） | 同上 | **1** | **1** |
| 版本串 `1.5.16` / `1.5.17` | 同上 | 1 / 0 | **0 / 1** |

> **五轮五种输入**：改样式 → CSS 名变；零样式（1.5.14 / 1.5.15 / 1.5.16 / 1.5.17）→ CSS 名
> **连续四版不变**（`jc_arBlM` 五版都是 1）。这轮连模板都没动，只改 store 里的三个视图位，
> 所以「CSS 名不动」是预期的、不是漏验；四个 JS 产物名全换、两个旧名归零，才是这轮改动
> 真正进包的证据。桌面 `passwordms.exe` 的五个名字同样各出现 1 次。

**1.5.16 门禁（D-160，2026-11-01 实跑）**：前端 `npm run test` **70 文件 612 例全绿**
（较 1.5.15 的 69/602 新增 1 文件 4 例 `d160-row-folder-visibility-guard.test.ts`
+ `list-scope` 6 例）/ `npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN
（零样式改动，判据视觉部分只是复用既有规则）/ Rust core 零改动 /
`cargo fmt --all -- --check` 干净（EXIT=0）/ clippy 代码警告 0（唯一 warning 行仍是 windres
兜底的 build-script 提示）/ 体积门禁桌面 + APK 双过（MSI 2.78 MB、NSIS 1.96 MB、APK 7.39 MB）。

**1.5.16 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`80a23bf9`，versionCode 1005016 / versionName 1.5.16；**字节数与之前八版
完全相同，第八次**）/ NSIS `2,053,190 B`（`454b186d`）/ MSI `2,916,352 B`（`5abef61f`）/
裸 exe **`4,994,560 B`**（`7856c233`，ProductVersion 1.5.16；较 1.5.15 的 4,998,656 B
**−4,096 B**——本轮只删不增：副行的组名在组内视图不再渲染）/
绿色版 zip `2,421,034 B`（`32211cb1`，随 exe 一起变小）；**zip 内 exe 与裸 exe 哈希一致**
（`7856c233`，同为 4,994,560 B）。`sha256sum -c SHA256SUMS-1.5.16.txt` 在**独立空目录**
（四个资产按发布名拷进去）四文件全 OK —— 这才是用户下载后的真实校验方式，
在 `target/` 里按相对路径核验会因资产分处不同目录而误报。

**包内取证（1.5.16）——第四种输入：只删代码、不增 UI 的一版**：

| 判据 | 位置 | 1.5.15 | 1.5.16 |
|---|---|---|---|
| `ptvdewIa`（本次 `App-*.js`） | APK 内 `libpasswordms_lib.so` | **0** | **1** |
| `C_KvDGpM`（本次 `index-*.js`） | 同上 | **0** | **1** |
| `b1ZNiHRj` / `GU9_siLv`（1.5.15 的 JS 名） | 同上 | 1 / 1 | **0 / 0** |
| `CflwYoSr` / `CPpMbYHn`（两个分包名） | 同上 | 1 / 1 | **0 / 0** |
| `jc_arBlM`（`style-*.css`） | 同上 | **1** | **1** |
| 版本串 `1.5.15` / `1.5.16` | 同上 | 1 / 0 | **0 / 1** |

> **四轮四种输入**：1.5.13 改样式 → CSS 名变；1.5.14 零样式改动 → CSS 名不变；
> 1.5.15 改模板文案、仍零样式 → CSS 名不变；1.5.16 **只删了一段模板**（副行组名包进 `{#if}`）
> → CSS 名**第三次不变**，四个 JS 名全换。桌面 `passwordms.exe` 同样如此（新名各 1、
> CSS 名 1）。
> 这一轮还多了一条**体积反证**：exe 二进制**变小** 4,096 B，与「只删不增」的改动方向一致 ——
> 判据（名字换）与量（体积降）互相印证，比单看任一项都可靠。

**1.5.15 门禁（D-159，2026-11-01 实跑）**：前端 `npm run test` **69 文件 602 例全绿**
（较 1.5.14 的 68/596 新增 1 文件 5 例 `d159-search-scope-guard.test.ts`，另 D-158 段的
4 例改写为 D-159 段 5 例）/ `npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN
（老账未动，本轮零样式改动）/ Rust core 零改动（仅 `Cargo.toml` 版本串）/
`cargo fmt --all -- --check` 干净（EXIT=0）/ clippy 代码警告 0（唯一 warning 行是 windres
兜底的 build-script 提示）/ 体积门禁桌面 + APK 双过（MSI 2.78 MB、NSIS 1.96 MB、APK 7.39 MB）。

**1.5.15 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`b6e34927`，versionCode 1005015 / versionName 1.5.15；**字节数与之前七版
完全相同，第七次**）/ NSIS `2,057,438 B`（`7f5abf8b`）/ MSI `2,916,352 B`（`cff27461`）/
裸 exe **`4,998,656 B`**（`f1cb0c6b`，ProductVersion 1.5.15；较 1.5.14 的 4,997,120 B
**+1,536 B**——本轮新增了一个空态按钮与四语言文案，桌面二进制如实变大）/
绿色版 zip `2,422,958 B`（`83ce2a25`）；**zip 内 exe 与裸 exe 哈希一致**（`f1cb0c6b`，
同为 4,998,656 B）。`sha256sum -c target/SHA256SUMS-1.5.15.txt` 四文件全 OK。

**包内取证（1.5.15）——同一判据的第三种输入**：

| 判据 | 位置 | 1.5.14 | 1.5.15 |
|---|---|---|---|
| `b1ZNiHRj`（本次 `App-*.js`） | APK 内 `libpasswordms_lib.so` | **0** | **1** |
| `GU9_siLv`（本次 `index-*.js`） | 同上 | **0** | **1** |
| `BScF0NTj` / `CyNRiLgU`（1.5.14 的 JS 名） | 同上 | 1 / 1 | **0 / 0** |
| `jc_arBlM`（`style-*.css`） | 同上 | **1** | **1** |
| 版本串 `1.5.14` / `1.5.15` | 同上 | 1 / 0 | **0 / 1** |

> **三轮三种输入**：1.5.13 改样式 → CSS 名变；1.5.14 零样式改动 → CSS 名不变（JS 名变）；
> 1.5.15 改了模板与文案、仍零样式改动 → CSS 名**再次不变**（`jc_arBlM` 三版都是 1），
> JS 名照旧换。桌面 `passwordms.exe` 同样含五个名字各 1 次（`CflwYoSr` / `CPpMbYHn`
> 两个分包名也只在新包里出现）。
> 这轮的意义：**CSS 名不变不是「我没验证」，而是本轮确实没动 CSS**——按钮复用了既有的
> `pms-btn is-ghost is-sm`，没有新增规则。判据与改动内容一致，才说明它在测内容。
> 表格里 1.5.14 与 1.5.15 两列的每个数字，都是**两个 APK 同时躺在 `target/` 下时当场数出来的**
> （`zipfile` 读 `lib/arm64-v8a/libpasswordms_lib.so` 逐字节 count），不是抄上一轮的结论。

**1.5.14 门禁（D-158，2026-11-01 实跑）**：前端 `npm run test` **68 文件 596 例全绿**
（较 1.5.13 的 67/584 新增 1 文件 8 例 `d158-search-echo-guard.test.ts`，另 `vault.test.ts`
新增 4 例行为）/ `npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（老账未动，
本轮零样式改动）/ Rust core 零改动 / `cargo fmt --all -- --check` 干净 / clippy 代码警告 0 /
体积门禁桌面 + APK 双过（MSI 2.78 MB、NSIS 1.96 MB、APK 7.33 MB）。

**1.5.14 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`49fc2aff`，versionCode 1005014 / versionName 1.5.14；**字节数与前五版
完全相同，第六次**）/ NSIS `2,056,481 B`（`65ca4701`）/ MSI `2,916,352 B`（`951490b0`）/
裸 exe `4,997,120 B`（`492c4088`，ProductVersion 1.5.14）/ 绿色版 zip `2,422,707 B`
（`fe699a24`）；**zip 内 exe 与裸 exe 哈希一致**（`492c4088`）。
`sha256sum -c target/SHA256SUMS-1.5.14.txt` 四文件全 OK。

**包内取证（1.5.14）——「不该变的不变」第二次得到验证，且这次是反向**：

| 判据 | 位置 | 1.5.13 | 1.5.14 |
|---|---|---|---|
| `BScF0NTj`（本次 `App-*.js`） | APK 内 `libpasswordms_lib.so` | **0** | **1** |
| `CyNRiLgU`（本次 `index-*.js`） | 同上 | **0** | **1** |
| `C9OX3AUS` / `C6Cyrhx7`（1.5.13 的 JS 名） | 同上 | 1 / 1 | **0 / 0** |
| `jc_arBlM`（`style-*.css`） | 同上 | **1** | **1** |
| 版本串 `1.5.13` / `1.5.14` | 同上 | 1 / 0 | 0 / 1 |

> **1.5.13 改了样式 → CSS 名变（`CErjAN2L`→`jc_arBlM`）；1.5.14 零样式改动 → CSS 名不变
> （`jc_arBlM` 两版都是 1）。两轮一正一反，把「Vite 产物名反映内容」这条判据验到了两个方向。
> 桌面 `passwordms.exe` 同样含三个名字各 1 次。

**1.5.13 门禁（D-157，2026-11-01 实跑）**：前端 `npm run test` **67 文件 584 例全绿**
（较 1.5.12 的 65/572 新增 2 文件 12 例：`list-scope.test.ts` 8 例 +
`d157-mobile-scope-title-guard.test.ts` 4 例）/ `npm run check` 0 error 0 warning /
CSS 契约 0 FAIL 6 WARN（老账未动，本轮样式只改窄屏媒体查询内一条）/ Rust core 零改动 /
`cargo fmt --all -- --check` 干净 / clippy 代码警告 0 / 体积门禁桌面 + APK 双过
（MSI 2.78 MB、NSIS 1.96 MB、APK 7.33 MB，均远低于上限）。

**1.5.13 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`42b40902`，versionCode 1005013 / versionName 1.5.13；**字节数与前四版
完全相同，第五次**）/ NSIS `2,056,167 B`（`e20e7226`）/ MSI `2,916,352 B`（`3ab020ff`）/
裸 exe `4,997,120 B`（`1cf72372`，ProductVersion 1.5.13）/ 绿色版 zip `2,422,770 B`
（`ed1d9f55`）；**zip 内 exe 与裸 exe 哈希一致**（`1cf72372`）。
`sha256sum -c target/SHA256SUMS-1.5.13.txt` 四文件全 OK。

**包内取证（1.5.13）——本轮三个名字全换，且与前两版形成对照**：

| 判据 | 位置 | 1.5.12 | 1.5.13 |
|---|---|---|---|
| `C9OX3AUS`（本次 `App-*.js`） | APK 内 `libpasswordms_lib.so` | **0** | **1** |
| `C6Cyrhx7`（本次 `index-*.js`） | 同上 | **0** | **1** |
| `jc_arBlM`（本次 `style-*.css`） | 同上 | **0** | **1** |
| `BK01vCCU` / `CErjAN2L`（1.5.12 的 JS / CSS 名） | 同上 | 1 / 1 | **0 / 0** |
| 版本串 `1.5.12` / `1.5.13` | 同上 | 1 / 0 | 0 / 1 |

> **本轮 CSS 名也变了（`CErjAN2L` → `jc_arBlM`），与 1.5.12 的「CSS 名不变」正好互为对照**：
> 1.5.12 只改模板条件、样式零改动 → CSS 内容哈希应当不变（实测确实不变）；本轮改了
> `.tb-title-mobile` 的样式 → CSS 内容哈希应当变（实测确实变了）。**同一条判据在两种输入下
> 给出两种结果**，才说明它测的是内容而不是「必然变化」。
> 桌面 `passwordms.exe` 同样含 `C9OX3AUS` / `C6Cyrhx7` / `jc_arBlM` 各 1 次、`BK01vCCU` 0 次。

**1.5.12 门禁（D-156，2026-10-31 实跑）**：前端 `npm run test` **65 文件 572 例全绿**
（例数与 1.5.11 相同 —— 本轮只改 1 例断言的**语义**，没加新例）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（老账未动，本轮零样式改动）/
Rust `cargo test -p passwordms-core` 全绿（core 零改动）/ `cargo fmt --all -- --check` 干净 /
clippy 代码警告 0（唯一一条 `warning:` 仍是 build.rs windres 兜底）/ 体积门禁桌面 + APK 双过。

**1.5.12 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`fafb1545`，versionCode 1005012 / versionName 1.5.12；**字节数与前三版
完全相同，第四次**）/ NSIS `2,054,239 B`（`60add0c0`）/ MSI `2,916,352 B`（`471b3200`）/
裸 exe `4,994,048 B`（`07af67c4`，ProductVersion 1.5.12）/ 绿色版 zip `2,420,502 B`
（`57beb040`）；**zip 内 exe 与裸 exe 哈希一致**（`07af67c4`）。
`sha256sum -c target/SHA256SUMS-1.5.12.txt` 四文件全 OK。

**包内取证（1.5.12）——本轮一个意外的正面校验**：

| 判据 | 位置 | 1.5.11 | 1.5.12 |
|---|---|---|---|
| `BK01vCCU`（本次 `App-*.js` 名字） | APK 内 `libpasswordms_lib.so` | **0** | **1** |
| `DXV310Az`（本次 `index-*.js` 名字） | 同上 | **0** | **1** |
| `CErjAN2L`（本次 `style-*.css` 名字） | 同上 | **1** | **1** |
| 版本串 `1.5.11` / `1.5.12` | 同上 | 1 / 0 | 0 / 1 |

> **`CErjAN2L` 两版都是 1，恰恰证明这条判据是有效的**：本轮只改模板条件（`{#if}`），
> 样式一行没动，Vite 给 CSS 算出的内容哈希**理应不变**。如果三个名字全都变了，
> 反而要怀疑判据是不是「必然变化」的假证据。**「该变的变、不该变的不变」比「全变」更可信。**
> 桌面 `passwordms.exe` 同样含 `BK01vCCU` / `DXV310Az` 各 1 次。

**变异 2 条全真红**：① 判据退化成只 `vault.counts.ungrouped > 0`（丢 `|| ungroupedOnly`）；
② 整个 `{#if}` 去掉（回到恒在）。

**1.5.11 门禁（D-155，2026-10-31 实跑）**：前端 `npm run test` **65 文件 572 例全绿**
（较 1.5.10 的 569 例 **+3**，全部新增在 `d153-ungrouped-filter-guard.test.ts`：
头部不许出现分组 / 页脚三项共存 / 页脚允许换行）/ `npm run check` 0 error 0 warning /
CSS 契约 0 FAIL 6 WARN（老账未动）/ Rust `cargo test -p passwordms-core` 全绿
（本轮 core 零改动）/ `cargo fmt --all -- --check` 干净 / clippy 代码警告 0
（唯一一条 `warning:` 仍是 build.rs windres 兜底）/ 体积门禁桌面 + APK 双过。

**1.5.11 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`e4a72fce`，versionCode 1005011 / versionName 1.5.11，V3 签名；
**字节数与 1.5.10 第三次完全相同**）/ NSIS `2,055,894 B`（`032be283`）/
MSI `2,916,352 B`（`7a60fe2d`）/ 裸 exe `4,997,120 B`（`4b031383`，ProductVersion 1.5.11）/
绿色版 zip `2,422,335 B`（`41c7b57c`，4 条目）；**zip 内 exe 与裸 exe 哈希一致**（`4b031383`）。
`sha256sum -c target/SHA256SUMS-1.5.11.txt` 四文件全 OK。

**包内取证（1.5.11）——本轮是纯前端改动，没有新的 serde 字段名可查**，
改用 **dist 资源文件名（内容哈希）** 作为指纹，A/B 双方都是当场从包里取出实测：

| 判据 | 位置 | 1.5.10 | 1.5.11 |
|---|---|---|---|
| `style-CErjAN2L` / `index-CrTGZE5S` / `App-DFEtLUvm`（本次 dist 三个产物名） | APK 内 `libpasswordms_lib.so`（5,721,032 B，两版**同尺寸**） | **0 / 0 / 0** | **1 / 1 / 1** |
| 同上三个 | 桌面 `passwordms.exe` | — | **1 / 1 / 1** |
| `detail-foot-folder`（新类名） | `dist/assets/*.css` | — | 3 |
| `detail-foot-folder` | `dist/assets/*.js` | — | 2 |
| `detail-badges`（已删的旧类名） | `dist` 全量 | — | **0** |

> **方法论升级（1.5.11 新增，比 serde 字段名更通用）**：Vite 产物名带内容哈希，
> 一旦改动进包，名字必然换新；**且这些名字在 `.so` / exe 里是明文**（不随前端资源被压缩），
> 所以「旧名 0 次 → 新名 1 次」是**纯前端改动的首选取证手段**。
> serde 字段名只在有 Rust 改动时可用；`detail-foot-folder` 这类 CSS 类名在二进制里搜不到
> （前端资源压缩嵌入），**不能拿它当包内判据**，只能拿它验 `dist/`。

**变异验证 2 条全真红**：
① 把分组 `<span>` 搬回 `<header class="detail-head">` 内 → 「分组不许回到标题区」红
（`expected '<header class="detail-head">…' not to contain 'folderText'`）；
② 删 `.detail-foot` 的 `flex-wrap: wrap` → 「页脚允许多项换行」红。

> 本轮唯一的假绿风险点在**切片断言**：`section()` 找不到标记时返回空串，而
> `expect(head).not.toContain('folderText')` 对空串是**通过**的。所以每条切片断言前
> 都先钉 `expect(head.length).toBeGreaterThan(0)`。这是 1.5.10「断言被同名代码喂饱」的同族，
> 记在此处以备后续复制。

**1.5.10 门禁（D-153/D-154，2026-10-31 实跑）**：前端 `npm run test` **65 文件 569 例全绿**
（较 1.5.9 的 552 例 **+17**：新增 `folder-label.test.ts` 6 例、
`d153-ungrouped-filter-guard.test.ts` 9 例、`save-visibility.test.ts` +3 例并改写原 9 例夹具，
`d152-item-counts-guard.test.ts` 改 2 例语义）/ `npm run check` 0 error 0 warning /
CSS 契约 0 FAIL 6 WARN（老账未动）/ Rust `cargo test -p passwordms-core` **全绿**
（新增 `d153_ungrouped_filter.rs` 5 例）/ `cargo fmt --all` 已跑 / clippy 代码警告 0
（唯一一条 `warning:` 仍是 build.rs windres 兜底）/ 体积门禁桌面 + APK 双过。

**1.5.10 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`a3f6a241`，V3 签名；**字节数与 1.5.9 完全相同**但哈希不同，
见下方说明）/ NSIS `2,055,607 B`（`56e26031`）/ MSI `2,916,352 B`（`4b1b2050`）/
裸 exe `4,997,120 B`（`0130d178`，较 1.5.9 的 4,992,512 **+4,608 B**）/
绿色版 zip `2,422,123 B`（`8eb049cd`，4 条目）；**zip 内 exe 与裸 exe 哈希一致**（`0130d178`）。

> APK 字节数 1.5.9 / 1.5.10 **逐字节相同（7,754,155）**，这是第二次出现（1.5.7/1.5.8 也相同）。
> sha256 不同（`9a978c48` → `a3f6a241`），且包内 `.so` 从 5,718,576 → **5,721,032（+2,456 B）**。
> 结论**不依赖体积判据**：靠的是下面那张包内取证表。以后遇到"两版同尺寸"不要当成没构建成功。

**包内取证（1.5.10）——A/B 双方都是当场取出实测**：

| 判据 | 位置 | 1.5.9 | 1.5.10 |
|---|---|---|---|
| `ungrouped_only`（新 IPC 参数名） | 桌面 `passwordms.exe` | **0** | **1** |
| `ungrouped_only` | APK 内 `libpasswordms_lib.so`（+2,456 B） | **0** | **1** |
| `未知分组`（新 i18n 文案） | `dist/assets/*.js` | — | 1 |
| `未分组` | 同上 | — | 1 |
| inbox 图标路径 `M22 12h-6l-2 3h-4l-2-3H2` | 同上 | — | 1 |

> `list_ungrouped_items` / `FolderScope` 在两侧都是 0 次：Rust 符号名在 release 下被内联、
> 不进二进制字符串表。**所以取证只认 serde 字段名与命令参数名**（沿用 1.5.2 定下的方法论）。

**变异验证 4 条全真红**（1 条先假绿后收紧）：
① `selectUngrouped()` 里 `this.ungroupedOnly = true` → `false` → 「store 独立布尔位」红；
② `FolderScope::Ungrouped => false` → 行为 5 例**全红**，报错原文
「侧栏显示 3 条未分组，点进去却列出 0 条」——正是本轮要防的故障；
③ 列表行把 `{folderText(folderTag)}` 换成 `{typeLabel(item)}` → 「列表行渲染分组」红；
④ 「全部」行 `active` 删掉 `!vault.ungroupedOnly` → 「两行同时高亮」红。

**这一轮又踩到一次"断言被同名代码喂饱"**：变异 ③ 第一次**没红**，因为断言写的是
`toContain('folderText(folderTag)')`，而同一行还有 `title={folderText(folderTag)}`（悬停提示），
删掉文本输出后该字符串仍在。改成正则直匹配 `\{folderText\(folderTag\)\}</span>`（绑定"作为
文本渲染"的形态）才真红。**与 D-149 同型**：断言要绑形态，不要绑符号是否存在。

**另一条：位置参数多一位，所有精确参数断言一起失配**。`api.items.list` 从 3 参变 4 参后，
`vault.test.ts` 的 5 处 `toHaveBeenNthCalledWith(…, null, false, undefined)` 全红
（received 多一个 `false`）。对象参数不会这样——这是位置参数在测试里的固有代价，
**加参数时必须 grep 该函数的每一处精确断言**。

**1.5.9 门禁（D-152，2026-10-31 实跑）**：前端 `npm run test` **63 文件 552 例全绿**
（较 1.5.8 的 541 例 +11，新增 `d152-item-counts-guard.test.ts`）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（老账未动，本轮只加组件内局部样式）/
Rust `cargo test -p passwordms-core` **全绿**（新增 `d152_item_counts.rs` 3 例）/
`cargo fmt --all` 已跑 / clippy 代码警告 0（唯一一条 `warning:` 仍是 build.rs windres 兜底）/
体积门禁桌面 + APK 双过。

**本轮踩到的一个"假红"（值得记下来）**：给 `vault.load()` 的 `Promise.all` 加了第三路
`api.items.counts()` 之后，`vault.test.ts` 里三条 D-81 用例瞬间红，**报错内容是
「过滤器没恢复」（`expected null to be 'folder-a'`）**——真因只是那份整体 mock `api`
没写 `counts`，Promise 整体拒绝 → `load()` 走 catch → 过滤器恢复逻辑没执行。
报错与真因隔了三层。教训：**给 `Promise.all` 加分支后，必须把所有 mock 了该模块的测试过一遍**；
看到"逻辑没执行"类红灯，先怀疑 mock 而不是先改被断言的逻辑。

另：Rust 测试第一版是我**自己数错夹具**（3+1+2 断言成 5），`left: 6, right: 5` 的红灯
反而证明了 `total` 在正常工作。这类情况应先复核夹具，别急着改实现。

**变异验证 4 条全真红**：① 删 `agg.item_count += 1` → 恒等式与跨解锁两例红；
② `out.total += self.trash.len()`（把回收站算进总数）→ 回收站断言红；
③ `this.counts = { total: items.length, ... }` → 「不许由 items.length 推导」红；
④ 删侧栏 `folder.item_count` 绑定 → 「四个读数都进模板」红。

**1.5.9 实际产物（出包实测，非推算）**：
APK `7,754,155 B`（`9a978c48`，V3 签名；较 1.5.8 的 7,750,059 **+4,096 B**，
不再出现前两版的"逐字节相同"）/ NSIS `2,051,851 B`（`717d342b`）/
MSI `2,912,256 B`（`9258af38`）/ 裸 exe `4,992,512 B`（`43ff169d`，
ProductVersion 1.5.9，文件内 `1.5.8` 零残留）/ 绿色版 zip `2,419,499 B`（`d56242f3`，4 条目）；
**zip 内 exe 与裸 exe 哈希一致**（`43ff169d`）。

**包内取证（1.5.9）**——沿用 serde 字段名 / 命令注册名对拍，并做 A/B：

| 判据 | 位置 | 1.5.8 | 1.5.9 |
|---|---|---|---|
| `item_counts` | 桌面 `passwordms.exe` | — | 1 |
| `ItemCountsDto` | 桌面 `passwordms.exe` | — | 1 |
| `item_counts` | APK 内 `libpasswordms_lib.so`（5,718,576 B） | **0** | **1** |
| `ItemCountsDto` | 同上 | **0** | **1** |
| `ungrouped_note` / `另有` / `item_counts` | `dist/assets/*.js` | — | 存在 |

> `.so` 5,716,352 → 5,718,576（+2,224 B），与一条命令 + 一个 DTO + 计数逻辑的量级一致。
> 1.5.8 的 `.so` 是**当场取出实测**（不是查历史记录），所以 A/B 两侧都是硬数据。

**1.5.8 门禁（D-150/D-151，2026-10-31 实跑）**：前端 `npm run test` **62 文件 541 例全绿**
（较 1.5.7 的 535 例 +6，新增 `d150-sync-readout-guard.test.ts`）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（老账未动，本轮只加一条局部样式）/
Rust `cargo test -p passwordms-core` **全绿**（新增 `sync::status` 3 例 + `d150_last_sync_at.rs` 3 例，
后者为**真 socket** 重同步与重配对）/ `cargo fmt --all` 已跑 / clippy 代码警告 0 /
体积门禁桌面 + APK 双过。

**本轮的方法论收获（写下来防止下次重犯）**：
1. **先证伪「显而易见的 bug」再动手**。直觉是「同步成功却没写 `last_sync_at`」，但真 socket
   回归一跑就绿——说明写入链路完好，问题在 UI 刷新与重配对覆写。若直接去改 `touch_device_sync`，
   会把一个没坏的函数改坏，并因为「症状没变」继续往深处猜。
2. **变异不生效 ≠ 守门有效**。第一版变异脚本用 `str.replace` 删 `loadDevices`，命中的是
   `unpair()` 里那行同名调用（文件共 4 处），被测代码没动，测试自然全绿。判定真红之前
   **必须先断言变异落地**（计数变化），再按函数边界切片重做。
3. 归因逻辑放进 core 而不是命令层，是为了一条可写的单测：造「一台已同步 + 一台从未同步」的
   数据形态，断言 `max` 归到后者且 `never_synced = true`。命令层没有测试载体。

**1.5.8 实际产物（出包实测，非推算）**：
APK `7,750,059 B`（`d1abd3bb`，V3 签名；**字节数与 1.5.7 完全相同**，故用包内字段名 A/B 取证，
见下）/ NSIS `2,053,197 B`（`be020d0e`）/ MSI `2,912,256 B`（`e3181aa6`）/
裸 exe `4,994,048 B`（`35526c9d`，ProductVersion 1.5.8，文件内 `1.5.7` 零残留）/
绿色版 zip `2,420,230 B`（`c1e9d010`，4 条目）；
**zip 内 exe 与裸 exe 哈希一致**（`35526c9d`）。

**包内取证（1.5.8）**——沿用 1.5.6 起的「serde 字段名对拍」判据（运行时必需，LLVM 消不掉）：

| 判据 | 位置 | 1.5.7 | 1.5.8 |
|---|---|---|---|
| `pending_device_name` | 桌面 `passwordms.exe` | 0 | 1 |
| `pending_device_never_synced` | 桌面 `passwordms.exe` | 0 | 1 |
| `pending_device_name` / `_never_synced` | APK 内 `libpasswordms_lib.so`（5,716,352 B） | 0 | 1 / 1 |
| `pending_from_never` / `这台设备从未同步过` | `dist/assets/*.js` | — | 存在 |

> APK 字节数与 1.5.7 逐字节相同（7,750,059）是第二次出现，`.so` 为 5,716,352（较 1.5.7 的
> 5,715,824 +528 B）。**体积相同不代表内容相同**——sha256 不同（`d1abd3bb` vs `f65bf92e`），
> 且两条 serde 字段名从「0 次」变成「1 次」，改动确实进包。

**1.5.7 门禁（D-149，2026-10-31 实跑）**：前端 `npm run test` **61 文件 535 例全绿**
（较 1.5.6 的 528 例 +7，新增 `d149-lan-perm-guard.test.ts`）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（老账未动，本轮只动组件内局部样式）/
Rust `cargo test -p passwordms-core` 全绿（本轮 core 零改动）/ `cargo fmt --all` 已跑 /
clippy 代码警告 0（唯一一条 `warning:` 仍是 build.rs windres 兜底）/ 体积门禁桌面 + APK 双过。

**本轮多跑了一道别人不会跑的编译**：`./gradlew :app:compileArmDebugKotlin`（跳过全部
`rustBuild*`）。理由：`cargo check` 与前端测试**都碰不到 Kotlin 类型**，而本轮改的正是
`MainActivity` 与新增的 Kotlin 插件。**它当场抓到一个真错误**——
`requestPermissions(...) { granted -> cb(granted) }` 报
`actual type is 'Boolean?', but 'Boolean' was expected`：原代码那行只做字符串插值
（`Log.i("... granted=$granted")`），永远不会暴露回调形参可空；我把回调接进 `(Boolean) -> Unit`
才暴露。修法 `cb(granted == true)`（null 归为「未授予」，拿不准就当作没给）。
**教训：改 Kotlin 必须真编译一次，否则「全绿」只是没测到。**

> 附带踩坑：Gradle 的 `:app:compileDebugKotlin` 在本工程**任务名有歧义**
> （候选 `compileArmDebugKotlin` / `compileUniversalDebugKotlin`），必须带 ABI 前缀。
> 另：Kotlin 守护进程在编译上游 `tauri-android` crate 时会因增量缓存文件锁崩
> （`Could not close incremental caches …`），与本项目代码无关；
> `-Pkotlin.compiler.execution.strategy=in-process` 可稳定绕过。

**两条变异真红**：① `lib.rs` 注册类名改 `LanPermPlugin` → 跨语言比对红；
② 模板判据退化成 `{#if lanPerm !== null && !lanPerm.granted}` → 判据断言红。
**第一版断言是假绿**：`toContain('lanPerm.supported && !lanPerm.granted')` 被 `grantPerm()`
里的同名判据喂饱，只变异模板那处仍然全绿；改为正则直匹配 `{#if}` 行才真红
（与 D-148「截断点取错」同类：断言必须钉在你以为它钉的地方）。

**1.5.7 实际产物（出包实测，非推算）**：
APK `7,750,059 B`（`f65bf92e`，V3 签名）/ NSIS `2,052,585 B`（`893dd29c`）/
MSI `2,912,256 B`（`d4908640`）/ 裸 exe `4,988,928 B`（`051b8a58`，ProductVersion 1.5.7，
文件内 `1.5.6` 零残留）/ 绿色版 zip `2,417,115 B`（`4c4d789f`，4 条目）；
**zip 内 exe 与裸 exe 哈希一致**（`051b8a58`）。

**包内取证（1.5.7）**——延续 1.5.6 的「字符串对拍」法，但换一类判据：
本轮验的不是 serde 字段名，而是**命令注册名**（`generate_handler!` 生成的匹配串，
运行时必需、消不掉）：

| 判据 | 位置 | 结果 |
|---|---|---|
| `lan_permission_status` | 桌面 `passwordms.exe` | 存在 |
| `lan_permission_request` | 桌面 `passwordms.exe` | 存在 |
| `lan_permission_status` / `_request` | APK 内 `libpasswordms_lib.so`（5,715,824 B） | 均存在 |
| `sync.lan_perm_missing`（前端文案） | exe | **0 次**（前端资源压缩进 exe，属预期，不作判据） |
| `附近的设备` / `授予权限` | `dist/assets/*.js` | 均存在 |

> APK 体积四版各不相同（7,737,771 × 3 → 7,750,059，+12,288 B），不再出现「逐字节相同」
> 的诡异巧合；本轮唯一仍未解释的残留是 `.so` 从 5,703,280 增到 5,715,824（+12,544 B）——
> 与新增的 Kotlin 插件常量、两条命令的字符串和分发逻辑一致，量级合理，未进一步归因。

**1.5.5 门禁（D-148，2026-10-31 实跑）**：前端 `npm run test` **60 文件 528 例全绿**
（较 1.5.4 的 523 例 +5，新增 `d148-pair-confirm-guard.test.ts`）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（老账未动）/
Rust `cargo test -p passwordms-core` **293 通过 / 0 失败 / 1 ignored**（本轮 Rust 零改动）/
`cargo fmt --all -- --check` 0 处差异 / clippy 代码警告 0（唯一一条 `warning:` 是 build.rs 的
windres 兜底提示，D-33 已知自愈）/ 体积门禁桌面 + APK 双过。

**四条变异逐条真红**：`join` 里加回 `requestTab('status')` → ①④ 双红 / `confirmPairing`
不清 `lastPairSessionCode` → ② 红 / `{#if sync.pairAwaitingConfirm}` 改 `{#if false}` → ③ 红 /
`PairShow` 改回 `sync.endPairing()` → ④ 红。

**1.5.5 实际产物（出包实测，非推算）**：
APK `7,737,771 B`（`da336ae4`，V3 签名）/ NSIS `2,048,550 B`（`2cf240fb`）/
MSI `2,912,256 B`（`1f9e43f9`）/ 裸 exe `4,984,832 B`（`1228d67c`，ProductVersion 1.5.5）/
绿色版 zip `2,414,345 B`（`1e56cfa6`，4 条目）；**zip 内 exe 与裸 exe 哈希一致**。

**包内取证（1.5.5）**——APK 与 MSI 的**体积又一次与上一版逐字节相同**（7,737,771 /
2,912,256），所以照例走同包对拍，不看大小：

| 判据 | 读数 |
|---|---|
| APK 内 `.so` 字节数 | 1.5.4 `5,702,000` → 1.5.5 `5,703,592`（+1,592 B） |
| APK 内 `.so` sha256 | `8ed405f0…`（与 1.5.4 不同） |
| APK 整体 sha256 | `c1dccd0f…` → `da336ae4…` |
| MSI 整体 sha256 | `1813ad0a…` → `1f9e43f9…`（体积相同、内容不同） |
| `assets/tauri.conf.json` 的 `version` | `1.5.5` |
| `dist/assets/*.js` 含 `配对成功。请与电脑屏核对会话码`（D-148 新文案） | ✅ |
| dist 14:36:41 → APK 14:40:06（前端先于打包） | ✅ 顺序正确 |

**1.5.3 门禁（D-146，2026-10-31 实跑）**：前端 `npm run test` **58 文件 518 例全绿**
（较 1.5.2 的 511 例 +7，新增 `d146-clipboard-latest-ui.test.ts`）/
`npm run check` 0 error 0 warning / CSS 契约 0 FAIL 6 WARN（与 1.5.2 同，均为未达 AA 的老账）/
体积门禁 exe+APK 双过。**四条变异逐条真红**：去 token 守卫 / 把清理也塞进 token 判定 /
删 toast 去重滤器 / 提示条件去掉 `copiedField`。

**1.5.3 实际产物（出包实测，非推算）**：
APK `7,733,675 B`（`8e103592`，V3 签名）/ NSIS `2,048,351 B`（`74bc3966`）/
MSI `2,908,160 B`（`011df192`）/ 裸 exe `4,982,272 B`（`834c5051`，ProductVersion 1.5.3）/
绿色版 zip `2,413,172 B`（`06aa1439`，4 条目）；**zip 内 exe 与裸 exe 哈希一致**。

**包内取证（1.5.3）**——这次 APK 与 MSI 的**体积与 1.5.2 逐字节相同**（都是 7,733,675 /
2,908,160），光看大小会误判成「没重新打包」，所以改用**同包对拍**：

| 判据 | 读数 |
|---|---|
| APK 内 `.so` 字节数 | 1.5.2 `5,699,216` → 1.5.3 `5,699,456`（**+240 B**） |
| APK 内 `.so` sha256 | `6a120ca2…` → `aad4a39a…`（不同） |
| APK 整体 sha256 | `877d9398…` → `8e103592…`（不同） |
| `assets/tauri.conf.json` 的 `version` | `1.5.3` |
| `dist/assets/App-*.js` 含 `clipboard-copy`（D-146 新增字面量） | ✅ 2 处 |
| dist 产物 12:59:55 → APK 13:04（前端先于打包） | ✅ 顺序正确 |

**结论**：体积恰好相同是 zip/对齐填充吸收了 +240 B，**不是复用旧产物**。
教训——**产物大小相同不能当作「没变」的证据，也不能当作「变了」的证据**，
必须比内部字节（`.so` 长度 + 哈希）和配置版本。

**包内取证（1.5.2，历史记录）**：

| 判据 | 读数 |
|---|---|
| APK 内 `.so` 含 `passwordms.export-`（D-144 新增的临时文件名） | ✅ 命中 1 次 |
| APK 内 `.so` 含 `passwordms.restore-`（既有恢复分支，对照） | ✅ 命中 1 次 |
| `dist/assets/*.js` 含四条新文案（`备份已保存` / `从未成功导出过备份` / `这个备份文件是空的` 等） | ✅ 全部命中 |
| `dist/index.html` 12:19:45 → APK 12:23:25（前端产物先于打包） | ✅ 顺序正确 |
| `build-android.sh` 对 `.so` 的四条断言（mtime ≥ 上游起点 / UPSTREAM_RC / magic / 字节数） | ✅ 4/4 |

> 注意：`content_write_size_mismatch` / `empty_source` 在 `.so` 里**搜不到是正常的**——
> `IpcError::io(_detail: &str)` 的形参带下划线（实现不用它），LLVM 内联后会消掉这些字面量。
> 用「临时文件名格式串」作判据才是可靠的：它真被 `format!` 用到，必然留在 rodata。
> 前端中文文案在 `.so` 里也搜不到——Tauri 对嵌入的 assets 做了压缩；判据要换成 `dist` 侧。

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
| `discovery` | 8 | **D-74 新增**（UDP 认证发现）：① `discovery_roundtrip_on_loopback`；② `discovery_no_reply_for_unpaired`（未配对**静默不回包**）；③ `discovery_drops_mismatched_q`；④ `discovery_rejects_bad_version_and_oversized_tags`；⑤ `discovery_rate_limited_per_source`；⑥ **`resync_heals_stale_addr_via_discovery`（验收核心：把 `peer_port` 写错仍能同步成功并写回新端口）**；⑦ **`discovery_ignores_self_reply`（自应答过滤，三组断言：过滤组 / 对照组 / 弱化成只判 IP 的探针组）**；⑧ **`discovery_rebroadcast_repeats_within_window`（D-132：借 `self_port` 自过滤让请求方「收不到应答仍继续重播」，用应答方收到的请求数直接量出广播轮数——单轮必须恰好 1 次、多轮必须 ≥2 次）** |
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
> **D-96 批次 10 条清零（2026-10-25 #530，v1.2.19 后）**：D-96 落码未跑 clippy，v1.2.19 门禁实测 10 条（lib 9 + 测试 1），全部 lint 修整、**零行为变更、零公开签名变更**，当轮复跑 = **0 error / 0 warning**：

| lint | 位置 | 修法（一句话） | 涉签名/行为？ |
|------|------|------|------|
| empty line after doc comment | `sync/server.rs:508` | 悬空说明行（描述已并入 `handle_connection` 的旧实现）由 `///` 转普通 `//` 注释 | 否 |
| doc list item without indentation | `constants.rs:110` | 行首 `+ 512`（算式续行被误判列表项）改写为「再 + 512」 | 否 |
| very complex type | `sync/client.rs:141` | 局部变量类型引私有别名 `type PreparedSection = CoreResult<(…)>` | 否（类型不变，纯别名） |
| very complex type | `sync/resync.rs:717` | 同上，别名 `SectionResult` | 否 |
| very complex type | `sync/server.rs:367` | 同上，别名 `SectionResult` | 否 |
| too many arguments (8/7) | `sync/paging.rs:72` | `#[allow(clippy::too_many_arguments)]` + 注释——参数均为协议帧必要字段，打包结构体需改公开签名（红线，同 `session.rs` 先例） | 否（allow，**未重构**） |
| doc list item without indentation ×3 | `sync/paging.rs:110-112` | `recv_push_pages` doc 的 D-96 列表项后补 `///` 空行分段 | 否 |
| let binding then return | `tests/sync_pagination.rs:229` | 取端口的 `let l / let p / p` 改链式调用（listener 仍在语句末 drop，端口已取出，语义不变）；该测试 4/4 复跑通过 | 否 |
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

> **D-96 遗留 50 处拉平（2026-10-25 #530，v1.2.19 后）**：D-96 落码未跑 `cargo fmt`，v1.2.19 门禁 fmt 门实测 **50 处差异**（集中在 `sync/`、`import/` 与新增测试文件）。D-96 已提交推送，按 Q-2 同款纪律做**独立格式化**（`cargo fmt --all`，纯换行重排、零语义改动，12 文件）；拉平后 `--check` EXIT=0，clippy 0/0 不变，`sync_pagination` 4/4 复跑通过。**教训入库：落码命令的收尾必须带 `cargo fmt --all && cargo clippy`，不能等门禁来抓**——本轮 fmt 门第一次真的红在交付读数上（此前 Q-2 负向探针只验证过「会红」）。

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

#### 2.1.1 D-82：NSIS 安装版缺 WebView2Loader.dll（2026-10-23，主人实测发现，已修复）

- **症状**：NSIS 安装版装完只有 `passwordms.exe` + `uninstall.exe`，启动报「由于找不到 WebView2Loader.dll，无法继续执行代码」。MSI / 绿色版 / APK 不受影响（主人长期用 MSI 故未暴露；绿色版 D-62 起由 `make-portable.sh` 手工补 DLL）。
- **根因（上游）**：`tauri-bundler` 的 `nsis/mod.rs::generate_resource_data()` 仅当 **`target.ends_with("-gnu")`** 才把 out_dir 下的 `WebView2Loader.dll` 注入 NSIS；本项目 target 为 `x86_64-pc-windows-gnullvm`——以 `-gnullvm` 结尾，判定为 false → NSIS 包无 DLL。而 WiX/MSI 侧（`msi/mod.rs`）是 out_dir `*.dll` **无条件全量 glob**，所以 MSI 一直正常。属上游对 gnullvm 目标的疏漏。
- **修法（本项目侧，不动上游）**：Tauri 官方 NSIS 钩子机制——新增 `src-tauri/windows/installer-hooks.nsh`（`NSIS_HOOK_POSTINSTALL` 注入 DLL + `NSIS_HOOK_PREUNINSTALL` 删除，保证卸载后 `$INSTDIR` 可移除；文件内注释**纯 ASCII**，理由同 .ps1 教训），`tauri.conf.json > bundle > windows > nsis > installerHooks` 挂载。`.nsi` 里的 DLL 源路径 `..\..\WebView2Loader.dll` 相对生成的 `installer.nsi`（`{out}/nsis/{arch}/`）解析；若上游改布局，makensis **当场报错**（fail-closed，不会静默出坏包）。
- **验证（端到端）**：重打后 setup.exe 1,679,997 → **1,746,668 B**（sha256 `c09f1ee0…`）；静默安装到临时目录 → `WebView2Loader.dll`（160,880 B）就位；`passwordms.exe` 启动 4 秒存活（原「系统错误」弹窗消失）；`uninstall.exe /S` 卸载 exit 0，目录与 HKCU 卸载键**零残留**。
- **教训**：①**每个交付渠道都要端到端启动过一次**——NSIS 渠道自 v1.0 起从未被真机启动过，坏包静默存活了 12 个版本；②「包体增大 110KB」是钩子生效的旁证，但**不能只靠旁证**，必须看到文件真落盘。

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

## 2A. Release 页发布（一条命令 + 六条硬要求）

### 2A.1 为什么要有这一节（2026-11-04，主人的原话）

> 「最近发布在 github 上的 Release 页很不稳定，太随意了，漏了一些细节。」

审计当时的 30 个 Release，退化链条是清楚的三段：

| 版本段 | 名称 | 正文 | 资产 |
|---|---|---|---|
| 1.5.23~1.5.28 | `PasswordMS 1.5.28` ✅ | 中文、讲后果 ✅ | 4 件 + `SHA256SUMS` ✅ |
| 1.5.29 / 1.5.32 | `1.5.29` / `v1.5.32 — …`（产品名丢） | 中文 ✅ | 4 件，**sums 没了** |
| 1.5.33~1.5.35 | `v1.5.35`（裸 tag） | **英文 `## Changed`，从 commit 机翻** | 4 件，**sums 没了** |

**根因不是不会做，是「发布靠记忆」**：每次都是手调 REST，少写一个字段没有任何东西拦。
所以把它变成脚本 + 硬校验（§2A.3），缺件就发不出去——宁可发不出去，也不发一张残缺的页面。

`release/` 目录被 gitignore（`.gitignore:79`），1.5.30~1.5.35 的说明只贴在 GitHub 正文里，
本地一份、版本库一份都没有 → **说明正文改放 `docs/releases/v<版本号>.md`（进版本库）**。

### 2A.2 六条硬要求（`scripts/publish-release.py` 逐条机检）

| # | 要求 | 判据 |
|---|------|------|
| 1 | 名称必须带产品名 | `PasswordMS <版本号>`；裸 tag 一律不许 |
| 2 | 正文中文为主 | 汉字 ≥ 300；末尾附一段 `## English summary` |
| 3 | 三个必备小节 | `## 已知限制`（没有就写「无」）/ `## 交付信息` / `## English summary` |
| 4 | 不许有机翻痕迹 | 正文含 `## Changed` 直接拒 |
| 5 | 五件资产齐全 | NSIS + MSI + portable zip + APK + `SHA256SUMS-<版本>.txt` |
| 6 | 数值必须真 | 说明表里的 sha256 与 `SHA256SUMS` 与磁盘三方逐字一致（终核再对一次线上 `digest`） |

### 2A.3 一条命令

```bash
# 出包后先算校验清单（四行，自带 4/4 回读自检）
bash scripts/release-sums.sh 1.5.36

# 写说明（从模板复制，六条硬要求都在模板注释里）
cp docs/releases/_TEMPLATE.md docs/releases/v1.5.36.md

# 先空跑：只校验，不动线上
python scripts/publish-release.py 1.5.36 --dry-run

# 真发（创建或更新；已有且一致的资产不重传，不一致的删了重传）
export http_proxy=http://127.0.0.1:35934 https_proxy=http://127.0.0.1:35934
python scripts/publish-release.py 1.5.36

# 补历史版本页面（跳过「源文件版本相符」检查）
python scripts/publish-release.py 1.5.30 --historical
```

脚本幂等：重复跑不会重复建 release、不会重复传同名同哈希资产；`--keep-body` 只动名称，
`--strip-body-h1` 去掉正文与标题重复的首行 H1。

**令牌**：`target/.gh_cred`（`git credential fill` 写出），跑完立刻 `rm`。**不许内联管道取令牌**。

### 2A.4 这一轮的负向探针（证明门真的会红，不是装饰）

| 探针 | 期望 | 实测 |
|---|---|---|
| 版本号与源文件不符 | 拒 | ✅ `FATAL: 版本号不一致：package.json=1.5.35 …` |
| 没有 `docs/releases/v1.5.36.md` | 拒 | ✅ `FATAL: 缺发布说明 … 1.5.30~1.5.35 的说明就是这么丢的` |
| 英文正文（汉字 8 个） | 拒 | ✅ `FATAL: 只有 8 个汉字（下限 300）` |
| 中文够但残留 `## Changed` | 拒 | ✅ `FATAL: 里还有自动生成的 ## Changed 小标题` |

### 2A.5 2026-11-04 补账结果（实测）

| tag | 动作 | 终态 |
|---|---|---|
| v1.5.35 / v1.5.34 / v1.5.33 | 改中文说明 + 补 `SHA256SUMS` | 名称带产品名、5 资产、digest 全对 |
| v1.5.32 | 去重名 H1 前缀、补 `SHA256SUMS` | 5 资产 |
| v1.5.31 / v1.5.30 | **原本没有 Release 页**（tag 有、页无），新建 | id 433443866 / 433443708，5 资产 |
| v1.5.29 | 名称补产品名、正文去重复 H1、补 `SHA256SUMS` | 5 资产 |

> 网络注意：这台机器上 GitHub 的可达性会来回翻（同一小时内实测过「直连 SSL EOF」「代理 502」
> 「curl 走代理通但 python 走代理握手失败」三种）。发布脚本因此内建**直连/代理自动轮换**，
> 失败一次换另一条，成功那条一直用下去；别写成单路线裸 urllib。

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
