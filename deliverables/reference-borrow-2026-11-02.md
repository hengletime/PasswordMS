# 参考目录（KeePassXC 2.8.0 / SecureVault 1.7.0）可借鉴项清单

- 调研日期：2026-11-02
- 参考源：
  - `Reference/KeePassXC-2.8.0-x64/keepassxc-2.8.0`（C++/Qt 2.8.0 **完整源码**，业界标杆）
  - `Reference/SecureVault/SecureVault-1.7.0`（Tauri + React，技术栈与 PasswordMS 最接近）
- 口径：只列「我们**没有**、且对目标用户（中小企业主 / 普通用户，Windows + Android，局域网同步，零知识）有实际价值」的能力。已有等价实现的（回收站、TOTP 扫码、生物识别解锁、二维码、剪贴板倒计时、冲突箱、原子替换、条目级同步语义）不重复列。
- 编号沿用 `B-nn`（Borrow），与缺陷号 `D-nn`、Kimi 风险项 `R-nn` 区分，便于后续立项。

---

## 一、结论速览

| 编号 | 借鉴项 | 对方做法 | 我们的现状 | 价值 | 成本 | 建议 |
|---|---|---|---|---|---|---|
| **B-01** | 剪贴板「敏感内容」标记 | Win/mac 侧让系统剪贴板历史与云同步忽略该条 | **无标记**：密码会进 Win+V 历史、可能上云 | ★★★ 隐私事故 | 低 | **立刻做** |
| **B-02** | 密码历史（改密码留旧值） | 每次修改前存快照，可配上限与总容量 | **完全没有** | ★★★ 改错就永久丢 | 中 | **立刻做** |
| **B-03** | 保存前自动滚动备份 | 保存前把上一版复制成 `db.kdbx.<时间>.bak`，路径可配 | 只有手动导出/恢复 | ★★★ 误删+清空+压缩=永久丢 | 低 | **立刻做** |
| **B-04** | 口令短语生成（passphrase） | EFF 7776 词表，词数/分隔符/大小写/尾数可调 | 只有随机串 | ★★★ 主密码要好记 | 低 | **立刻做** |
| **B-05** | KDBX（.kdbx）导入 | 原生读写 KeePass 2 格式 | 只支持 Bitwarden JSON/CSV | ★★☆ 迁移与获客入口 | 中 | 排期 |
| **B-06** | 通用 CSV 列映射 + 浏览器预设 | 表头嗅探 + 手动改列映射 + 预览 | 只认 Bitwarden 表头 | ★★☆ 任何浏览器都能迁 | 中 | 排期 |
| **B-07** | 搜索语法 | `field:value`、`-排除`、`+精确`、`is:weak/expired`、`\|` 或 | 纯子串 + 优先级打分 | ★★☆ 库到几百条才痛 | 中 | 排期 |
| **B-08** | 条目到期日 + 到期提醒 | expiry 字段；过期 30 天内每天扣分；解锁时提示 | 只有「多久没改」 | ★★☆ 证照/卡有效期 | 中低 | 排期 |
| **B-09** | 强度改用 zxcvbn 类算法 | ZxcvbnMatch，>256 字符按均熵外推 | `len × log2(池)`，对 `Password123!` 严重高估 | ★★☆ 体检分数可信度 | 中 | 排期 |
| **B-10** | 窗口防截屏 | 默认阻止截屏/录屏，可临时开 | 无 | ★★☆ 投屏/开会场景 | 低（有坑） | 可并入 B 组 |
| **B-11** | 内置弱口令表（"已泄露"检测） | HIBP 离线库（`SHA1:次数`）流式比对 | 无泄露检测 | ★★☆ 与弱口令不同维度 | 中低 | 可选 |
| **B-12** | 浏览器扩展 + 本机通信 | 扩展 ↔ 本机 socket 协议，域名授权 | 无（只能手动复制） | ★★★ 日常使用最大缺口 | **高** | 战略决策 |
| **B-13** | Windows Auto-Type | 全局热键 + 窗口标题匹配 + 动作序列 | 无 | ★★☆ 远程桌面/老客户端 | 高 | 战略决策 |

**推荐下一轮范围：B-01 ～ B-04（一个版本收掉），都是"补洞"性质且互相独立。** B-12 需要主人单独拍板，不建议与其它项混做。

---

## 二、A 档：建议下一轮就做

### B-01 剪贴板敏感标记（隐私事故级）

- 对方做法：
  - KeePassXC 在 `src/core/` 层统一封装剪贴板写入，Windows 上同时设置「排除监视器处理 / 不纳入剪贴板历史 / 不上传云剪贴板」三项私有格式标记；macOS 用 `org.nspasteboard.ConcealedType`。
  - SecureVault 只做了「15 秒后写空串」，**没有**标记 —— 这正是不能照抄的那一半（见 `Reference/SecureVault/.../src/App.tsx:397-424`）。
- 我们的现状：`src-tauri/src/commands/system_cmd.rs:58` 直接 `app.clipboard().write_text(...)`（tauri 剪贴板插件底层为 arboard），**不含任何标记**。安卓侧同样没有 `ClipDescription.EXTRA_IS_SENSITIVE`（`src-tauri/gen/android/.../MainActivity.kt` 全文无此常量）。
- 真实后果：
  - Windows：复制密码后按 `Win+V`，密码明文躺在剪贴板历史里；开了「剪贴板云同步」则同步到微软账号与其它设备。**倒计时清空帮不上忙**——历史条目是快照，清空当前剪贴板不影响已经进历史的那一条。
  - Android 13+：系统剪贴板浮层会直接显示密码预览，其它应用可见。
- 建议：新增 `clipboard::write_secret(text)`：
  - Windows：先按常规写入，再补写 3 个 `CF_PRIVATE` 注册格式（`ExcludeClipboardContentFromMonitorProcessing` = 1、`CanIncludeInClipboardHistory` = 0、`CanUploadToCloudClipboard` = 0）。
  - Android：`setPrimaryClip` 时给 `ClipDescription` 的 extras 置 `EXTRA_IS_SENSITIVE=true`（API 33+，低版本忽略）。
  - **必须与「代际归属」逻辑保持同一入口**（`clear_owned_clipboard_for_gen`），不能新开一条旁路，否则清空会失效。
- 成本：低（约一个模块 + 两个 cfg 分支 + 契约测试）。
- 风险：① 私有格式名未见于正式文档，属业界通行约定，需在 Windows 10/11 实测（`Win+V` 里不出现即通过）；② 安卓真机需 API 33+ 才可验；③ 不能因为写了标记就删掉倒计时清空——两者互补。

### B-02 密码历史（改密码留旧值）

- 对方做法：`docs/topics/DatabaseOperations.adoc:240-252`。每次保存前把**旧值**压入该条目 history；可查看/恢复/删除单条或全部；数据库设置里 `Max history items`（建议 ≤10）与 `Max. history size`（建议 6 MiB）双重上限；恢复旧版时当前值会再入历史。
- 我们的现状：`crates/passwordms-core/src/model/item.rs:138-168` 的 `Item` 只有 `rev` / `updated_at`，**没有历史结构**；全仓 grep `password_history` / `HistoryEntry` 零命中。
- 真实后果：改密码时网站其实没改成，或粘贴时手滑覆盖了正确密码 → 旧值永久消失，只能走"忘记密码"流程。这是密码管理器最容易被投诉的一类数据损失。
- 建议：
  - `Item` 增 `#[serde(default)] history: Vec<ItemSnapshot>`（旧帧反序列化为空 → 向后兼容），只存**敏感字段快照**（password / totp_secret / custom_fields），不存整条。
  - 上限：条数（默认 5）+ 总字节（默认 1 MiB），超限丢最旧。
  - **红线（D-79）**：历史快照若含附件引用，必须同步进 `attachment_blob_refs()`，否则压缩时该 blob 被判定无引用而删除 = 永久丢数据。**建议历史快照干脆不包含 `attachments`**，从根上避开这条不变量。
  - 同步语义：历史随条目帧一起走，沿用现有 rev/墓碑，不需要新增记录类型。
  - UI：详情页加「历史版本」抽屉，只读 + 「恢复此版本」；恢复走正常保存路径（于是当前值自动入历史）。
- 成本：中（模型 + 编码兼容 + 上限裁剪 + 同步 + UI + 契约测试）。
- 风险：① 体积膨胀，必须有硬上限；② 历史里的密文与主值同帧加密，`wipe_plaintext` 要覆盖到（锁定后不得残留）；③ 老客户端读新帧时多出的字段会被忽略 → 老版本改一次就会把历史抹掉，需要在发布说明里写明「升级后再回旧版本会丢历史」。

### B-03 保存前自动滚动备份

- 对方做法：`docs/topics/DatabaseOperations.adoc:322-336`。三种保存策略（安全保存 = 临时文件 + 原子替换，为默认）；另有独立「保存前备份」，目标路径可配、支持占位符命名，也支持按需 `Save Database Backup...`。
- 我们的现状：`crates/passwordms-core/src/model/vault.rs:126-146` 的 `VaultSettings` 只有 `generator` / `health` / `clipboard_clear_secs` —— **没有备份策略**；目前只有用户手动「导出备份」与「恢复备份」（D-170 刚做完的原子替换恢复）。
- 真实后果：误删条目 → 清空回收站 → 容器压缩（丢弃旧帧）→ **数据永久消失**，只剩用户上次手动导出的那份（可能几个月前）。
- 建议：
  - 容器每次**成功写入落盘前**，把当前文件复制为 `<vault>.bak.<yyyyMMdd-HHmmss>`（同目录，`fs::copy` 保 mtime）。
  - 策略：保留最近 N 份（默认 5，可配 3/5/10），超过按时间删最旧；单份上限可配。
  - 备份可直接用已有的 `verify_backup_container` 做健康自检，恢复走已有的 `restore_backup_from` —— **这两个函数是 D-170 刚建好的，等于零新增风险面**。
  - 设置页给一个开关（默认开）+ 保留份数 + 「打开备份目录」。
- 成本：低（复制 + 轮转 + 设置项 + 几个测试）。
- 风险：① 磁盘占用（5 份 × 库体积，普通库几百 KB，可忽略）；② 与同步无关（备份是本地行为，不进变更集）；③ 如果保险库在 `content://`（安卓 SAF），复制要复用 `content_uri` 那套，**别只写桌面分支**——这正是 D-144 的教训。

### B-04 口令短语生成（passphrase）

- 对方做法：`docs/topics/PasswordGenerator.adoc:29-43` + `src/core/PassphraseGenerator.*`。词表可选（内置 EFF large，允许自加），词数滑杆、分隔符任意字符串、大小写（小/大/首字母）、可选追加随机数字。EFF large 词表就在参考目录里：`Reference/KeePassXC-2.8.0-x64/keepassxc-2.8.0/share/wordlists/eff_large.wordlist`（7776 词，每词 12.9 bits → 4 词 ≈ 51.7 bits，6 词 ≈ 77.5 bits）。
- 我们的现状：`src/components/PasswordGenerator.svelte` 只有「长度 + 字符类 + 自定义符号集」；`crates/passwordms-core/src/generator.rs` 无词表路径。
- 真实后果：用户要设**主密码**（唯一必须记住的那个）时，随机串记不住 → 退化成 `abc123456` 或写便利贴，全库安全被这一个点拖垮。
- 建议：
  - 生成器加「随机串 / 口令短语」两个 Tab；词表随包内置（EFF large，约 60KB，许可 CC BY 3.0，需在关于页标注来源）。
  - 参数：词数 3–8、分隔符（默认 `-`）、大小写（默认小写）、末尾追加 2 位数字（可选）。
  - 熵显示按 `词数 × 12.9` 计，与现有强度条口径统一。
  - 生成逻辑放 Rust（与现有 CSPRNG 同源），前端只传参。
- 成本：低（词表打包 + 一个生成函数 + UI 一个 Tab + i18n 四语言）。
- 风险：① 词表体积进包（60KB，对 10MB 门禁无影响）；② **中文字符的词表暂无**，别承诺"支持中文短语"；③ 用词表生成的密码**必须**按"词数×12.9"算熵，不能用现有 `len × log2(池)` 公式（会把 `correct-horse` 这类算得很低，或把 `apple-apple` 算得很高）。

---

## 三、B 档：值得排期

### B-05 KDBX 导入（迁移入口）

- 对方做法：原生实现 KDBX 3.1/4 读写（`src/format/KdbxReader.*`，整个 `format/` 约 9.6k 行）。SecureVault 走了捷径：用 `keepass` crate（`Reference/SecureVault/.../src-tauri/src/main.rs:535-640`），仅密码解锁、递归遍历分组、映射标题/用户名/密码/URL/备注/自定义字段/到期/历史/otp，并用 `examples/kdbx_roundtrip.rs` 做往返验证。
- 我们的现状：`crates/passwordms-core/src/import/mod.rs:23-38` 只有 `Json` / `Csv` 两种，解析器绑死 Bitwarden 表头。
- 价值：KeePassXC / KeePassDX / 各类 KeePass 衍生 App 是**最可能主动换到我们自己产品的人群**，现在他们无法迁入（只能先绕 Bitwarden 中转，实际上没人会这么折腾）。
- 建议：接 `keepass` crate（**只读，不做写**——写 KDBX 是另一件难得多的事，且我们不需要）。要点：
  - 支持密码 + **keyfile**（SecureVault 没做，是它的短板）；硬件密钥明确报"不支持"而不是静默失败。
  - 过滤 KeePass 回收站分组（否则导入一堆已删条目）。
  - 附件：能取则取（走现有附件 blob 路径），取不到必须在预览里**明确计数告知**，不许静默丢。
  - 沿用现有两段式：`preview`（内存规划）→ `put_records_atomic`（全有或全无），D-172 刚建立的语义直接复用。
- 成本：中（crate 集成 + 映射 + keyfile + 预览计数 + 测试）。
- 风险：① crate 版本与 `zeroize` 配合（解出的明文要能擦）；② 大库（几千条）预览内存；③ **不要顺手做 KDBX 导出**，范围会失控。

### B-06 通用 CSV 列映射 + 浏览器预设

- 对方做法：`docs/topics/ImportExport.adoc:24-34` 的 CSV 向导——自动嗅探 + 允许用户重映射列 + 底部实时预览。SecureVault 则是按「表头特异性」自动识别（`src/utils/browserImport.ts:130-162`），零交互但覆盖有限。
- 我们的现状：只认 Bitwarden 表头（`crates/passwordms-core/src/import/bitwarden_csv.rs`）。
- 建议：两步走，**先做预设、后做映射**：
  1. 预设表头（Chrome/Edge、Firefox、Safari、LastPass、1Password CSV）——纯映射表，成本很低。
  2. 预设置信度不足时，退回「列映射 UI」：让用户把源列拖到我们的 5 个目标字段（标题/用户名/密码/网址/备注）。
- 成本：中。
- 风险：编码（GBK 的 CSV 在国内很常见）、BOM、字段内换行/引号转义 —— 解析器要按 RFC4180 写并有单测；导入完成后提示「源文件是明文，请安全删除」（对方在 `ImportExport.adoc:25` 明确警告，我们目前没有这句）。

### B-07 搜索语法（子集）

- 对方做法：`docs/topics/DatabaseOperations.adoc:255-313`。`-`/`!` 排除、`+` 精确、`*` 正则、`field:value`（t/u/p/url/n/attr/attach/g/tag）、`|` 或、`is:expired`、`is:weak`，通配 `*` `?`。
- 我们的现状：`src/lib/quick-search.ts:44-54` 纯子串打分（标题前缀 > 标题含 > 用户名 > 网址），无任何语法。
- 建议：**只做最小子集**，避免变成"要学一门语言"：
  - `is:weak` / `is:reused` / `is:expired` / `is:fav`（直接复用体检结果，价值最高）
  - `folder:组名`、`-词` 排除
  - 其余（正则、`+` 精确、`|` 或）不做。
- 成本：中（解析器 + 与现有打分协同 + 空结果文案）。
- 风险：中文输入法下全角冒号（`：`）要一并认；语法解析失败不能"什么都搜不到"，要退回按字面量搜。

### B-08 条目到期日 + 到期提醒

- 对方做法：`DatabaseOperations.adoc:109`（到期日可设预设）+ `:84`（解锁时提示 3 天内到期）+ `PasswordHealth.cpp:172-200`（已过期直接 0 分；30 天内每天扣 2 分）。
- 我们的现状：`HealthCheck.svelte` 的「超期」是 `HEALTH_STALE_DAYS`（**多少天没改**），不是"到期时间"。
- 价值：护照/身份证/银行卡/域名/证书/订阅到期，是中小企业主真实会记的东西。
- 建议：`LoginData` 增可选 `expires_at: Option<i64>`（`#[serde(default)]` 兼容旧帧）；列表/详情显示徽标；体检把"30 天内到期"单列一类；解锁后若有过期条目给一条非阻塞提示。
- 成本：中低（字段 + 三处 UI + 体检口径 + 同步天然跟随）。
- 风险：时区语义（一律存 UTC 毫秒，按本地日界显示，别重蹈 SecureVault 的 `-0` 坑）；"到期"与"多久没改"两个概念在 UI 上必须用不同措辞，否则用户分不清。

### B-09 强度算法换 zxcvbn

- 对方做法：`src/core/PasswordHealth.cpp:35-45` —— zxcvbn（`ZxcvbnMatch`）算熵，超 256 字符按平均熵外推；`quality()` 分 5 档（`Bad/Poor/Weak/Good/Excellent`，阈值 0/40/75/100）。
- 我们的现状：`src/lib/strength.ts:24-38`，`bits = len × log2(出现过的字符池)`，假设字符均匀随机。注释里也自己承认"对人类口令会高估"。
- 真实后果：`Password123!` 现在算出 11 × log2(94) ≈ 72 bits → 判「强」；zxcvbn 会判它极弱（字典词 + 递增 + 常见后缀）。体检的"弱口令"一类因此**系统性漏报**。
- 建议：前端引入 `zxcvbn-ts`（纯 JS，词表可只带常用包），或 Rust 侧 `zxcvbn` crate（更省前端体积、与"生成在 Rust 侧"的既有分工一致）。阈值沿用现有 4 档，避免 UI 改动。
- 成本：中（引入库 + 打包体积 + 与现有 `scoreOf` 的双轨兼容）。
- 风险：① 体积（zxcvbn 词典约 400KB，gzip 后 ~120KB，需评估前端预算）；② 生成器的强度条语义要区分——**生成结果的熵是可算的**（字符池公式正确），**用户手输的密码**才该用 zxcvbn；两处不能共用同一个函数。

### B-10 窗口防截屏

- 对方做法：`docs/topics/UserInterface.adoc:44-45`，Windows/macOS 默认阻止录屏与截图，需要时可临时 `Allow Screen Capture` 或命令行 `--allow-screencapture`。
- 我们的现状：无（全仓 grep `DisplayAffinity` 零命中）。
- 使用场景：投屏开会、远程协助时装着密码管理器，旁边人一眼看全。
- 建议：Windows `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)`，设置页给开关（默认开）。
- 成本：低。
- **风险（重要）**：我们自己的 TOTP「截图取码」功能（D-162～D-165 那套 `ScreenCrop`）是应用内自己抓屏——窗口一旦被排除捕获，**自己的截图也会变黑**。要么在进入截图流程时临时解除、退出恢复，要么该窗口独立不设标记。这个交互必须先定，否则会变成"扫码功能突然坏了"的新缺陷。

### B-11 内置弱口令表（"已泄露"检测）

- 对方做法：`src/core/HibpOffline.cpp:79-107` —— 读 HIBP 官方 `SHA1:次数` 排序文件，把库内所有密码的 SHA-1 建索引，流式扫文件命中即记录次数；不联网，密码不出本机。
- 我们的现状：无泄露维度。
- 现实约束：完整 HIBP 文件约 11GB（压缩后 2.7GB），不现实；在线 k-Anonymity API（只发 SHA-1 前 5 位）在国内网络可靠性差，且与我们"不联网"的产品叙事冲突。
- 建议（缩小口径）：内置 **rockyou / 常见弱口令 Top 10k**（约 100～200KB），命中即标「常见弱口令」，与现有"弱口令"并列但**独立成类**（算法强度合格但仍在泄露表里，是两种不同的病）。UI 必须写明覆盖率，不能暗示"查过 HIBP"。
- 成本：中低。
- 风险：**诚实性**。SecureVault 的教训（`src/hooks/useSecurityAudit.ts:40` 只有 7 个硬编码口令，UI 却宣称 HIBP/K-Anonymity）是我们绝对不能重演的那类问题。

---

## 四、C 档：战略级，需要主人单独拍板

### B-12 浏览器扩展 + 本机通信

- 对方做法：KeePassXC `src/browser/` 约 7.6k 行（本机 socket 协议、按域名授权、组级访问控制、passkey、防钓鱼校验）。SecureVault 取巧：运行时用 JSZip 现场生成一个 MV3 扩展 ZIP（`src/utils/extensionGenerator.ts`），靠"复制加密 JSON 再粘贴进扩展"同步，**没有本机通信**——代价是每次改密码都要手动搬一次，且把加密库暴露成 `web_accessible_resources`（任意网站可 fetch 去做离线爆破，见其 `:61-66`）。
- 我们的现状：完全没有。用户现在的流程是「打开 PasswordMS → 搜索 → 复制 → 切窗口 → 粘贴」。
- 判断：这是**日常使用体验上最大的一块缺口**——不做，产品永远停留在"偶尔查一下密码的保险箱"，而不是"每天用的密码管理器"。
- 但也必须说清代价：MV3 扩展 + 原生消息宿主（Windows 注册表注册 + Chrome/Firefox 各一套）+ 域名匹配与防钓鱼 + 授权 UI，等于**一个独立子项目**，且要维护两个浏览器生态的兼容性。
- 建议：**不在当前路线图里夹做**。若要做，先只做 Chrome/Edge + 原生消息（Named Pipe），只支持"填充/新建"，不做 passkey、不做同步推送。

### B-13 Windows Auto-Type

- 对方做法：`src/autotype/` 约 4.6k 行（全局热键、窗口标题匹配含正则、动作序列解析、虚拟键盘模式、`{PICKCHARS}` 交互取字符）。文档见 `docs/topics/AutoType.adoc`。
- 我们的现状：无。
- 价值：浏览器扩展覆盖不到的场合——远程桌面、老式 Win32 客户端、需要"只输密码不输用户名"的场景；且**不需要外部生态配合**，纯本机能力。
- 成本：高（键盘注入 + IME 兼容 + 热键 + 序列解析 + 误填风险）。
- 风险：① 输错窗口把密码打进聊天框（对方在文档里专门警告）；② 中文输入法会干扰模拟按键；③ 需要 UAC 提权窗口时注入会失败。建议与 B-12 二选一，不要同时做。

---

## 五、明确不做（及理由）

| 项目 | 对方实现 | 不做的理由 |
|---|---|---|
| Passkeys / WebAuthn | `docs/topics/Passkeys.adoc` + `src/browser/` 内 CTAP 实现 | 依赖浏览器扩展与 RP ID 校验，没有 B-12 就是空中楼阁；且需要 FIDO 认证级别的实现质量 |
| YubiKey Challenge-Response | `src/keys/` 约 2.9k 行 | 目标用户基本无硬件密钥；PC/SC 与 HID 两套协议成本高，ROI 极低 |
| SSH Agent | `src/sshagent/` 约 3.1k 行 | 面向开发者，与我们的中小企业主画像不符；Windows 上还要实现 OpenSSH named pipe |
| FDO Secret Service | `src/fdosecrets/` 约 8.1k 行 | 纯 Linux 桌面集成，我们只出 Windows + Android |
| KeeShare（分组共享给他人） | `src/keeshare/` 约 2.6k 行 | 与我们的"局域网多设备同步"是两套模型，并存会让用户混淆"分享"与"同步"；且要引入证书体系 |
| 多数据库标签页 | `UserInterface.adoc:8` | 我们的容器是单保险库模型，改造成本远超收益；用户要分库可以用两个安装目录，但更该做的是分组 |
| 命令行工具 | `src/cli/` 约 4.8k 行 | 目标用户不用终端；且要重新设计"非交互解锁"的密钥生命周期，风险不对称 |
| KDBX **导出** | `src/format/KdbxWriter.*` | 写入 KDBX4 的正确性门槛极高（AES-KDF/Argon2 + HMAC 块流 + 内层流），写坏 = 用户数据丢；只做导入（B-05）就够 |
| HTML 打印「纸质备份」 | `ImportExport.adoc:107-111` | 明文打印是**反向**安全属性，与我们"零知识"叙事冲突；要做也应该是"打印恢复码"，另立项 |

### 反面清单：SecureVault 里**不要照抄**的地方

技术栈近，反而更容易顺手抄错。明确记录：

1. **KDF 参数不落盘**（`src/hooks/useVault.ts:74`）→ 换过 KDF 的库重启后必须手动切回去才能解锁。我们已有自描述头部（`kdf` + `kdf_salt`），保持住。
2. **整文件覆盖式同步**（500ms 防抖全量重加密后覆盖）→ 多设备同时改必丢数据。我们的条目级 rev/墓碑 + 冲突箱是对的，别倒退。
3. **密钥以 Base64 字符串在 IPC 里来回传**（`useVault.ts:67`），明文条目常驻 JS 堆，无 zeroize。我们"密钥不出 Rust 侧"的分工要保持。
4. **审计造假**（7 个硬编码口令却宣称 HIBP）。见 B-11 的风险条。
5. **扩展把加密库设为 `web_accessible_resources` 且 `<all_urls>`**（`extensionGenerator.ts:61-66`）——等于给任意网站一份可离线爆破的副本。
6. **`csp: null` + 开放任意路径写文件命令**（`src-tauri/tauri.conf.json:25`、`src-tauri/src/main.rs:269-317`）——渲染层一旦 XSS 就是任意文件写。

---

## 六、顺手可做的小打磨（不单独立项，跟着别的版本走）

1. **TOTP 与密码同库的安全提示**：KeePassXC 在 `DatabaseOperations.adoc:129` 明确警告"同库存 TOTP 会抵消 2FA 优势"。我们在 TOTP 设置处加一句同等提示即可（不改变功能）。
2. **导入完成提示"源文件是明文，请安全删除"**：对方 `ImportExport.adoc:25` 有，我们没有。
3. **生成器「每类至少出现一个」**：对方 `CharFromEveryGroup` 是默认开（`PasswordGenerator.h:48-55`）。我们目前只保证"至少选了一类"，不保证结果里每类都有——用户勾了符号却生成出纯字母串时会觉得开关坏了。
4. **条目克隆**（`DatabaseOperations.adoc:168-191`）：同一站点多账号时，"以现有条目为模板新建"比"新建后手填"省事。我们的"复制粘贴条目"若有，则此条已覆盖，可略。
5. **备份文件命名带时间戳**：默认 `xxx.bak.20261102-2230`，别用 `xxx (1).bak`——用户无法判断哪份是哪个时间点。

---

## 七、下一步建议

- **A 档四项（B-01～B-04）可以合成一个版本**（D-173 起编）：都是补洞，互不依赖，总改动可控，且都能写自动化测试（B-01 需 Windows 真机看 `Win+V`；B-03 需验证 SAF 分支；B-04 需守门词表不被误改）。
- **B-05（KDBX 导入）建议单独一版**：它引入新依赖，要独立承担"依赖审计 + 体积变化 + 大库内存"三条风险。
- **B-12（浏览器扩展）需要主人先定方向**：做 / 不做 / 什么时候做。这是唯一一项能改变产品定位的决策，其余都是打磨。
