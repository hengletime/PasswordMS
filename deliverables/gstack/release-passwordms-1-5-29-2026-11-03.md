# PasswordMS 1.5.29 发布交付报告

**日期**：2026-11-03
**场景**：全流程交付（实现 → 双评审 → 出包 → 发布）
**参与成员**：产品官（gstack-product-reviewer）+ 安全卫士（gstack-security-officer）

---

## 📌 TL;DR（执行摘要）

- 整体结论：🟢 **已发布**。`v1.5.29` tag → `8b49bce`，GitHub Release 四件产物齐备，资产 digest 与本地 sha256 逐一相等。
- 阻塞项数量：自动化侧 0；**真机走查 5 行未执行**（已拆成任务 #8，见「待完善」）。
- 下一步：主人在装好的 1.5.29 上走 `docs/07-e2e-checklist.md` ①1.16–1.20，然后进 1.5.30 六项清单。

---

## 🎯 核心结论卡片

| 项目 | 内容 |
|------|------|
| Go / No-Go | 🟢 Go（两位评审各自独立复跑后放行同一 sha） |
| 严重度分布 | 🔴 0 / 🟠 0 / 🟡 2（进 1.5.30）/ 🟢 — |
| 关键行动项 | 6 条（全部进 1.5.30，见行动清单） |
| 建议负责人 | 小李（实现与出包）· 主人（真机走查拍板） |

**发布对象与产物的关系（必须是这个写法）**：

```
tag v1.5.29 → 8b49bce（tree 621fe1a）
产物构建自   d5b1bd4（tree 04bc8f6）
两者差异只有：docs/07-e2e-checklist.md、src-tauri/gen/android/.gitignore
```

`bfc3a22` 单独 checkout 时 `cargo metadata --locked` 会 exit 101（锁还是 1.5.28）——**不许把它写成「已修好」**，它与 `7c65d65`、`d5b1bd4` 合起来才是完整的一次发布，只有 `d5b1bd4`（及之后的 `8b49bce`）是可复现状态。

---

## 1. 各成员核心结论

### 🔍 产品官（产品评审）
- **核心判断**：这一版的价值不是措辞，是**一个报警因为没有后果而失去动机**。release note 里那句必须按这个口径写，不许写成「文案修正」。
- **关键建议**：`backupHints(status)` 必须返回 `HintId[]` 而不是单个 `HintId`——两格可以同时为真，写成单值等于在类型上禁止一条已经修对的正确行为。

### 🛡️ 安全卫士（OWASP + STRIDE 审计）
- **核心判断**：`d5b1bd4` 与远端 HEAD 三方一致（我验的树 = 你推的树 = 远端那棵树），六门全绿，逐条复跑一致。
- **关键建议**：这轮真正的产出不是修掉 B-03，而是**清掉了六个「写错不会红」的地方**：`target_existed` 的注释、`本机造不出来`、`新克隆即可编译`、`提交说明是唯一物证`、`Cargo.lock`、任务板。

---

## 2. 综合审查发现（按严重度排序）

| # | 严重度 | 类别 | 位置 | 问题描述 | 处理 |
|---|--------|------|------|---------|------|
| 1 | 🟡 | 可测性 | `Settings.svelte:1275–1330` | 两格的正确性依赖两个 `{#if}` 的**兄弟关系**（布局不是逻辑），重构时被顺手嵌进去则测试全绿、界面那格静默消失 | 进 1.5.30 第 1 项：抽 `backupHints(status): HintId[]` + 「两格同时为真时两个 HintId 都在场」的断言 |
| 2 | 🟡 | 门禁可用性 | `scripts/check-size.sh` | `check_android` 只在 `src-tauri/gen gen` 下找 `*.apk`，而本工程 APK 落在 `target/`；`check-size.sh all` 的 android 分支会**误红**，只能绕过 | 进 1.5.30：把 `target` 加入 apk 搜索路径，或让 `all` 明确走 target |
| 3 | 🟢 | 文档 | `docs/07-e2e-checklist.md` | 1.5.29 主角「自动回滚点」一条走查项都没有（清单是按版本维护的，v1.5.2[5-9] 命中 9 处 ⇒ 是这一版漏了） | **已修**：补 ①1.16–1.20（`8b49bce`） |
| 4 | 🟢 | 取数门 | `src-tauri/gen/android/.kotlin/` | Gradle 缓存未显式忽略，`sessions/` 换个扩展名就会让「PRE/POST 三元组逐字相等」永久为红 | **已修**：`gen/android/.gitignore` 加 `.kotlin`（`8b49bce`） |
| 5 | 🟢 | 版本门 | `Cargo.lock` | 版本同步是**四处**不是三处（缺锁文件时干净检出 `cargo build --locked` 失败） | **已修**：`7c65d65`；门本身已实测为纯读（陈旧锁 exit 101、锁一字节不动） |

**已实测、可写进文档的两条判据**（来自安全卫士在 `%TEMP%` 隔离拷贝上的对照实验，仓库零改动）：

- `cargo metadata --locked --offline`：锁陈旧 → **exit 101，锁不被改写**；同树去掉 `--locked` → exit 0 且锁被改写。两个假设被分开了，性质定为「只读」。
- 重新生成的锁与 `bfc3a22` **只差两行 version**，零依赖漂移 ⇒ `7c65d65` 那份锁是 cargo 自己生成的，不是手改，提交它是安全的。

---

## ✅ 行动清单

| # | 行动 | 负责方 | 紧急度 | 验收判据 |
|---|------|--------|--------|---------|
| 1 | 抽 `backupHints(status): HintId[]`，断言「两格同时为真 → 两个 HintId 都在数组里」 | 小李 | P1 | 单值签名会让该断言无法编译 |
| 2 | 正向绊线：locale × 格，两格都要后果 token；键集对齐 `LOCALES` | 小李 | P1 | 新增语言漏键时红 |
| 3 | 反向断言：`{count}` 必有无、`\p{Nd}` 禁数字（含泰语 ๐-๙）、`disk space` 改断言式 | 小李 | P1 | 变异验证：改回「仍然占用磁盘空间」必红 |
| 4 | 锚自检约 10 行（`include_str!("backup.rs").matches(CITED).count() == 1`，0 与 ≥2 分开报） | 小李 | P2 | 文本锚只管「引用还指着东西」，语义归行为测试 |
| 5 | 版本同步「三处 → 四处」+ 出包前 `cargo metadata --locked --offline` 写进 `docs/07-quality-gate.md` | 小李 | P2 | 门旁必须写「**不要照 cargo 的 help 做**」——那条 help 教人拆门 |
| 6 | 修 `check-size.sh` 的 android 分支（见发现 #2） | 小李 | P2 | `check-size.sh all` 在正式布局下能过 |
| 7 | 真机走查 ①1.16–1.20 | 主人 | P0（验收） | 五行全勾；不勾不算这个功能验收通过 |

---

## ⚠️ 待完善 / 已知局限

- **真机走查未执行**。1.16 / 1.17 那两格在正常使用里永远碰不到（必须人为拒掉目录读取权限才触发），而显示层没有自动化测试覆盖 ⇒ 真机走查是唯一验收手段。**这一版是「已交付、待验收」，不是「已验证」。**
- **取数窗口纪律要分级**：` M`/`A`/`D` ⇒ 整组作废；`??` ⇒ 不作废但必须点名列出；边界：`??` 若落在 `include_str!` 路径上仍作废（本工程确实在用 `include_str!`）。
- **「六门全绿在 sha X 上」不是一条结论，是六条结论**。报的时候必须能给出同一组 PRE/POST 三元组（`HEAD` / `HEAD^{tree}` / `git status --porcelain` 逐字输出），两次不完全相同 ⇒ 整组作废，不甄别。
- **两条纪律别合并**：正交坐标（一处读工作区、一处读提交态）管**审改动**；PRE/POST 三元组管**取数**。合并成一套仪式会加一倍动作而不加一寸覆盖。
- 口令短语（B-04）核心层已就位但**界面无入口**（`grep -rn passphrase src/` 为空），本版用不到。
- CSS 契约 6 条常驻 WARN（禁用主按钮对比度 3.74:1）仍未处理。

---

## 📚 成员产出索引

- gstack-product-reviewer（产品官）：`deliverables/gstack/product-review-b03-scope-tradeoff-2026-11-03.md`，及本轮关于 `backupHints` 类型形状、e2e 五行、取数窗口分级的四封结论
- gstack-security-officer（安全卫士）：`deliverables/gstack/full-audit-passwordms-2026-10-24.md`，及本轮 `--locked` 对照实验（`git archive bfc3a22` 到 `%TEMP%`，仓库零改动）
- 发行说明：`release/notes-v1.5.29.md`
- 发布页：https://github.com/hengletime/PasswordMS/releases/tag/v1.5.29

---

> 本报告由软件工坊 AI 协作生成，关键决策请由工程负责人复核。
