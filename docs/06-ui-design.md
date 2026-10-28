# PasswordMS · UI 设计规范 v1（2026-10-14）

> 本文件是**界面样式的唯一权威**。组件实现与本文冲突时以本文为准。
> 客户已拍板：**范围 = 全 app 一次到位**；**视觉 = 双主题精修，暗色主打科技感**。
>
> 修订：v1.1 强度条空态（D-24）· v1.2 按钮悬停层叠（D-27/28/29）· v1.3 原生 UI 干涉与面板状态机（D-30/31/32）· v1.4 移动端响应式布局（D-35，客户方案 A：单栏推进 + 底部导航）· v1.5 底栏选中态改「胶囊 + 柔光」，取消整格铺色 · v1.6 自绘标题栏（decorations:false，D-36）+ 品牌 LOGO 定稿（A · 盾与钥匙孔）· v1.7 桌面顶栏收缩档（D-37，切英文后顶栏撑爆）· v1.8 顶栏信息架构重排（D-38）：常用外显、低频进设置 · v1.9 配对可用性修复（D-44）：多候选 IP 二维码 + 网络错误文案细分 + 会话死亡感知 · **v2.0 生成器符号自选（D-50）：34 个常用符号芯片可选，默认只选安全集 + - _ . , :** · **v2.1 导出保险库补主密码输入框（D-51）+ 生成器参数（含符号自选）跨会话持久化（D-52）** · **v2.2 导出文案修正（D-54：明文 → 加密副本）+ 备份恢复闭环（D-55：锁屏页「从备份恢复」）** · **v2.3 Toast 时长自适应（D-58a：error 按文案长度 6–12s）+ 同步页初始标签按配对状态（D-58b）** · **v2.4 条目列表「添加附件」入口（D-59：新建菜单三入口，选文件即导入，不进编辑器）** · **v2.5 附件依附父条目（D-60 §3.14）：列表第三按钮撤销（回退 v2.4 入口），登录/笔记详情页新增附件区（区头添加 + 附件行导出/移除），AttachmentView 改造为附件区组件** · **v2.6 自动锁定真持久化（D-61 A 案）：Settings 档位读写走 IPC，全局闲置计时 + Android 回前台补判上线** · **v2.7 法律条款可点开 + 「关于」分组（D-63）：勾选行拆出「免责声明 / 用户协议」两个内联链接（点链接不切换勾选），新增 `LegalDialog` 双语长文弹窗与 Settings「关于」（版本运行时读取 / 作者 / MIT / 项目主页）** · **v2.8 勾选行折行修复（D-64）**：`.pms-link` 字号改 `font: inherit`（不再写死 0.86rem，否则链接比同行文字大、把行撑折），勾选行在 ≤767px 收敛为单行放得下（文案「我已阅读并同意」+ 字号 0.8rem / gap 0.2rem / padding 0.6rem 0.7rem / checkbox 16px，`flex-wrap` 保留作极端窄屏兜底） · **v2.9 设备可辨识与同步状态隔离（D-66）**：设备行名称可重命名（行内编辑）+ 显示配对时间；**+ 设备指纹改用真公钥指纹（D-67）**；「立即同步」忙碌态按设备隔离，未点的那台只置灰不转圈 · **+ 配对两屏显示真指纹（D-68）：电脑显示本机指纹、手机显示从 join 结果拿到的对端指纹，中间人核对从「无处可做」变为「一眼可比」** · **v2.10 配对核对改「会话码」口径（D-69 修正 D-68）**：首版电脑屏显示**长期身份指纹**、手机屏显示**临时 X25519 公钥指纹**，两把不同密钥 → 两屏必然不等（属负收益，会把每台设备都报成中间人）；D-69 改为两屏都显示**同一次会话的会话码**（`sha256(host_eph 公钥)` 前 8 字节 hex），并**严格区分**「会话码（每次变）/ 设备指纹（稳定）/ device_id（只做 title）」三个量；设备指纹为空时该行**整行不渲染**（不再回退 `device_id`） · **v2.11 文件落盘与「取消不是错误」（D-71 / D-72）**：Android 上保存对话框返回的是 `content://` SAF URI（不是内核路径），`std::fs::write` 必失败 → 附件导出报「文件读写出错」（D-71，改走 provider fd 的 `"wt"` 写入）；同时**收口「取消 → 弹红」的三处散乱实现**（`vault-export` 未捕获 rejection / `AttachmentView` 取消弹红 / `ImportWizard` 空 catch 吞真错）为唯一口径 `runSaveToDestination`（D-72）；导入面板补格式说明 + **可下载 CSV 模板**（`save_import_template`），模板与解析器表头一致性由 `tests/import_template.rs` 锁死 · **v2.12 设置页旁路数据失败隔离 + 保险库文件说明（D-73）**：`Settings.svelte` 的 `$effect` 首行曾用 `structuredClone` 深拷 `$state` 代理（结构化克隆算法遇 Proxy 必抛 `DataCloneError`），于是其后的「库文件信息 / 自动锁定档位 / 版本号」三块**从不执行**——四处症状（库文件卡片空白 / 版本显示「—」/ 自动锁定档位恒为默认档 / 连带整卡不刷新），其中「自动锁定档位读不到」客户尚未发现 → 深拷改 `$state.snapshot()`，三块改由 `src/lib/settings-page.ts::loadSettingsPageSide` **逐块独立失败隔离**并显式建模失败态；「保险库文件」补说明行、读不到显示 `settings.vault_file_failed` 而非空白卡片；档位读失败显示 `settings.auto_lock_failed`（否则「5 分钟」会被误读成真实设置）；导入按钮文案消歧（`import.choose_file`）+ 「预览」不再无解释置灰（改可点 + `import.choose_first` 提示）；**手机「关于」组按行类型显式区分**（`.is-info` / `.is-actions`，见 §6.10）——根因不是「规则写错了」而是「**规则的适用范围没写**」：窄屏 `.set-row { flex-direction: column }` 只对**控件行**成立（`.set-ctl` 的 160px 固定宽在 390px 会溢出），一刀切下去把纯信息行与按钮行一起扫了 → 信息行 `.is-info`（左右两端对齐）/ 按钮行 `.is-actions`（横排换行）在**同一个 `@media` 块内 `.set-row` 之后**覆盖回来，**不许用 `:has()`**（WebView 版本差异，别赌）；桌面 ≥768px 不需要任何新规则、逐像素不变 · **v2.13 配对成功态保留会话码（D-75）**：`pairing://ended` 在**成功与失败两种结局都发**，前端旧实现无条件 `sync.pairing = null`，而会话码正挂在这个对象上 —— 手机端「请核对会话码」的提示**恰好在事件之后才出现**，电脑屏的码已被清掉，D-69 的两屏核对**上线即不可用**；修法：载荷经纯函数 `nextPairingState()` 分流，`completed` **保留** `pairing` 并转「配对成功（待核对）」态（无二维码、大字会话码 + 核对提示 + 「完成」按钮），`aborted`/未知 清屏提示重新生成。详见 §5.7.5 · **v2.14 保险库压缩入口（D-76）**：容器只打墓碑标志、**从不回收空间**，且此前无任何压缩路径（导出备份都是原样 `fs::copy`）—— 实测主人库 24.34 MiB 里仅 **0.30 MiB 是活数据（1.28%）**，24.03 MiB 全是墓碑，其中 6 个已删附件占 24.0 MiB；设置页「保险库文件」卡片新增「已删附件占 / 可回收」两行 + 「压缩保险库」按钮（`reclaimable_bytes > 0` 才渲染，同步中禁用），二次确认**必须**给出四个数字与三条提示（不改数据/不要主密码、回收站附件不回收、临时 2 倍磁盘请勿关程序），成功后**必须重新拉取**大小与统计。详见 §6.11 · **v2.15 分组视图保存可见性（D-81）**：选中分组后新建条目「保存就消失」（编辑器不预填分组 → 存进根目录；列表按当前分组过滤 → 刚存条目不在列表）。修两层：编辑器新建时**预填当前选中分组**；store 层硬保证「**保存成功的条目必须立刻可见且选中**」——当前过滤器会藏住它时自动回落到全部条目（判定纯函数 `save-visibility.ts`，9 例单测 + store 接线 7 例）。详见 §5.4.1 · **v2.16 拖拽排序与频度置顶（#531）**：条目列表按「**钉住块（pin_order）→ 未钉按频度（use_count DESC → last_used_at DESC → updated_at DESC）**」排序，**拖拽 = 钉住**（拖动未钉条目到列表顶部即置顶并钉住；拖回原位零 IPC）；未钉区频度序**不可手调**；**收藏不再置顶**；侧栏分组行同样支持钉住与拖拽；「全部条目」固定第一、不可拖、但是**条目的跨分组投放目标**；桌面=鼠标按住即拖，触屏=长按 220ms 武装 + 6px 阈值（自写 pointer 事件，HTML5 DnD 在 Android WebView 不可靠）；行右键 / 触屏长按弹上下文菜单（钉住/取消钉住/移动到分组）。详见 §5.4.2 与 §6.12 · **v2.17 未钉区手动排序（D-84）**：真机走查发现「未钉拖未钉 = 无动作」是交互死锁（一个钉住的都没有 → 永远拖不动），客户拍板改为**拖 A 到 B 上把 B 一并收编进钉住块**、A 按插入方向排 B 前/后（视觉与拖放位置一致、顺序持久）；都未钉 / A已钉B未钉 / A未钉B已钉 / 都已钉四组合统一，拖到空白=已钉挪块尾/未钉无动作；松手结算三连（收编目标先钉 → 被拖者按需钉 → reorder，await 串行），后端模型/IPC 零改动。详见 §5.4.2 · **v2.18 点钉取消钉住 + 分组改名（D-85）**：钉住图标从纯展示改为**可点按钮**——必须是行按钮的**兄弟节点**（嵌套 button 无效 HTML，SSR 解析会被浏览器挪位），点击直接 `set_*_pin(false)`，条目掉回频度区是 D-84 定案的自然结果；分组右键/长按菜单新增「重命名」→ Modal 弹层（预填组名）→ 空名前端先挡；**契约坑**：`update_folder` 的 icon/color 是**整字段覆盖**（None = 清除），改名必须原样回传现有 icon/color（store 的 `vault.updateFolder` 传 null/null，改名路径已避开它，双击改名同路径收敛）。详见 §5.3 · **v2.19 导入弹窗确认按钮常驻 footer（D-86）**：主人报 P0——CSV 选文件、解析都完成，但**确认导入按钮被 85vh 滚动区裁掉、找不到，导入走不完**。修法：Modal 从 Settings 移进 ImportWizard 自持（Settings 只传 `open`/`onclose`），「确认导入」进 **footer snippet 常驻**（不随内容滚动；preview 为 null 时整个 footer 不渲染）；**选完文件自动预览**（`choose()` → `await doPreview()`，取消早退在前），格式手动变更只清 preview 不自动预览、手动预览按钮保留；组件常驻挂载后打开时显式重置向导状态。详见 §5.4 `ImportWizard` 行 · **v2.20 导入编码容错（D-87）**：主人报「文件读写出错 + 确认按钮不见」——真实根因是 Excel/WPS 编辑模板后另存成 **ANSI（GBK）**，后端 `read_to_string` 严格 UTF-8 必炸（`InvalidData` 误归类 `ERR_IO`），而前端表头识别的 `TextDecoder` 非 fatal、坏字节静默替换，表头照常显示把问题完全掩盖；修法：core 新增 `decode_source` 编码探测（UTF-8 BOM 剥离 / UTF-16 LE+BE BOM / UTF-8 严格 / GBK 兜底，仍失败报 `ERR_IMPORT_PARSE` 并改准文案），桌面与 Android `content://` 两路径统一；`encoding_rs`（Firefox 同款）入 §8.1 白名单；导入面板常驻一行编码提示（`import.encoding_hint`）。详见 §5.8.3 · **v2.21 导入合并与条目转换（D-88/89/90）**：导入实测三连反馈。**D-89**：登录表单的 `notes` 状态与保存载荷**一直存在**、却从未有输入控件——导入的备注「详情可见、编辑器消失、改不了」，补 `#ed-notes` textarea（TOTP 之后、自定义字段之前）。**D-88**：详情页登录项工具条新增「转为安全笔记」（二次确认 → **一次 `update_item` 整只替换 kind**，item_id 不变、附件/收藏/分组/同步历史天然延续；A1 映射：notes→正文，其余字段逐行并入正文尾部不丢失）。**D-90**：`import::apply` 先与现有库比对——**同名文件夹合并**（复用 folder_id 并重映射条目归属，不再每次导入多一套「Commander 办公」）+ **同名同型条目跳过**（绝不覆盖，`duplicate_skipped` 计数；回收站条目不阻碍重导），结果行如实上报「重复跳过 n 条 · 合并同名分组 n 个」。守门：`import_merge.rs` 5 例 + `d88-d90-ui-guard.test.ts` · **v2.22 编辑器窄列与操作条底缝（D-92/D-93）**：主人实测——笔记**预览全宽正常**，**编辑时表单缩成约 1/4 宽窄列**（标题/正文/分组全部收窄），保存工具条与窗口底边之间有大缝隙、滚过的分组下拉从缝里透出。两个独立根因：**D-92** 桌面端 `.pane` 为 `display:contents`，`form.editor` 直接成为 `.body`（flex row）的 item，`flex:1` 只写在移动端媒体查询里 → 桌面宽度按 max-content 收缩（`.detail` 有 `flex:1` 所以预览正常；登录与笔记共用同一表单 = 同病）；**D-93** `.editor-actions` 用 `position:sticky; bottom:0`，sticky 吸附范围被**包含块**限制——最低只能贴到 `.editor` 内容盒底，其下 `padding-bottom` 空带里滚过的字段透出（分组下拉露半截），内容不足一屏时工具条停在内容末尾与容器底之间留白。修法（几何上不可能复现）：字段区包进 `.editor-scroll`（`flex:1; min-height:0; overflow:auto`，padding 归滚动层），`.editor-actions` 改 **flex 固定底栏**（`flex-shrink:0` + 不透明背景，移出滚动流）；`form.editor` 改 `flex:1; min-width:0; min-height:0` 桌面/移动统一（移动端媒体查询里的重复规则删除）。全仓 sticky 仅此一处（grep 实证），无同病组件。详见 §5.4 `ItemEditor` 行与 §6.6 · **v2.23 移动端向左滑动返回（D-95）**：主人需求——手机上打开条目后返回不必点左上角 ←，**详情页上向左滑动即可返回**。判定纯函数化（`swipe-back.ts`）：单指触屏、水平向左 ≥72px、水平主导（|dx| ≥ 2|dy|，不与垂直滚动打架）、一次手势一次触发；**仅详情查看态启用**（A 案：编辑/新建态不启用，未保存表单内容不能被一个手势丢掉）；触发即调既有 `goBack()` 不新增导航状态（D-31）；桌面鼠标被 pointerType 过滤免疫。V1 无跟手动画，真机体验后迭代。详见 §6.13 · **v2.24 TOTP 密钥入库校验与展示错误态（D-97）**：主人实测——登录条目「动态口令」填入含 `1` 的串（Base32 字母表只有 A-Z、2-7，**没有 0/1/8/9**），详情页只剩倒计时在跳、验证码区域永远空，且**每个 30 秒窗口 / 每次切回条目都弹一条「输入无效」**（toast 堆积）。三层修：**编辑器入库前真校验**（`normalizeTotpInput` 先把 `otpauth://totp/…?secret=XXX` 整串链接自动抽出 secret——网站两步验证页默认给的就是这格式；再调 `totp_now` 试算（等价 base32 真解码），`ERR_VALIDATION` 即拦截保存、行内红字 `err.totp_invalid`（点名合法形态）+ 聚焦字段，环境类错误放行不卡编辑器）；**详情页错误态**（取码失败 → `totpInvalid` 置位、停止重试循环、渲染 `err.totp_invalid_detail` 可见一行字并停显倒计时——**不渲染 = 把失败伪装成「本来就没有」**；toast 只弹一次不再堆积）；占位符改「BASE32 或 otpauth://」。守门：`d97-totp-guard.test.ts`（normalize 5 例 + 主例回归 1 例 + 源码结构断言 7 例）。详见 §6.14 · **v2.25 强度评分改信息熵（D-100）**：主人实测——生成器里拖长度质量不变。旧 `StrengthMeter.scoreOf` 用「字符类数 + 长度加成」，4 类全开时 `Math.min(4, base)` 恒封顶 → 长度项被吞。改为**信息熵估算** `bits = len × log2(字符池)`（池 = 26/26/10/33 按出现字符累计；阈值 <40 弱 / <60 一般 / <80 强 / ≥80 很强），实现抽到 `lib/strength.ts` 可单测；4 类 6 位≈39 bits=弱、13 位≈85=很强，每档长度可感知。注意熵估算对人类口令（词典词）会高估——仅提示不强制（U6 不变）。守门：`d100-strength.test.ts`（锚点 4 例 + 长度单调性 + 组件源码断言）。详见 §6.15 · **v2.26 体检重复密码分组显示（D-99）**：主人实测——报告「6 条重复密码」，chip 区既看不出是几组、不在当前视图的条目还显示裸 UUID。两个根因：后端把所有重复组**拍平**（`reused: Vec<String>`，分组信息出生即丢）；前端 `titleOf` 用 `vault.items`（**按当前筛选过滤后的列表**）解析标题。修法：`HealthReport` 增加 **`reused_groups: Vec<Vec<String>>`**（每组 = 共用同一密码的条目，组内/组间均按条目 ID 升序、输出稳定；拍平视图 `reused` 保留供统计扣分）；`HealthCheck` 体检时**并行拉全量条目**（`api.items.list(null, false, undefined)`）建标题映射（不再依赖当前筛选）；重复区**按组渲染**（每组一个小框 + `health.reused_group`「第 {n} 组」标签），建议文案改「有 G 组条目重复使用同一密码（共 N 条）」。守门：`d99-health-groups.test.ts`（全量映射 / 并行拉取 / 分组渲染 / 旧拍平路径已删 / i18n 双语）+ `health_and_generator.rs` 多组回归（3+2 形态）。详见 §6.16 · **v2.27 搜索框一键清空（D-101）**：主人反馈搜索词一个一个删麻烦。词非空时 `ItemList` 搜索框尾部渲染 22px 圆形 ❌（既有 `close` 图标 + `aria-label`/`title` 双通道 `common.clear_search` 双语），hover 红图标 + `--pms-danger-soft` 淡底；空态不渲染不占位。行为关键：点击**先取消挂着的 220ms 防抖**再清（否则旧词 `setQuery` 写回），`setQuery('')` 与手输同源；**不推进 `queryEpoch`**（D-81：epoch 只属外部重置路径，自发清空走它会污染「外部重置 → 同步显示」的判定）。守门：`d101-search-clear.test.ts`（空态条件 / 防抖取消先于 setQuery / 不碰 epoch / 可及性 / i18n 双语）。详见 §6.17 · **v2.28 四语言 + 首启跟随系统语言（D-102）**：主人需求——①不同语言电脑上**首次启动跟随电脑语言**（非简体中文切英文），②新增**繁體中文**与**ไทย**。`Locale` 扩四值（zh-CN/zh-TW/en-US/th），设置页语言钮四枚（显示名母语书写不翻译）；字典 zh-TW.ts / th.ts 各约 250 键全真翻（繁体台湾用语：軟體/網路/匯入匯出/建立/裝置/剪貼簿/登入），键一致性守门扩四语全比对；**首启检测**：仅全新安装（localStorage 无存储值）时 `mapSystemLanguage(navigator.language)` 映射——zh-Hant*/TW/HK/MO→繁體、其余 zh-*→简体、th*→ไทย、en*→English、**未支持语言兜底英文**（主人拍板口径），老用户已存语言不受影响；法律文本 A 案全真翻（免责 6 + 协议 8 × 繁/泰，`LegalLang` 扩四值），守门补泰文字形断言与跨文字体系串语粗判（简繁同形标题合法，简繁之间比文档标题）。测试环境：vitest 池固化 `vmForks`（默认 forks 池 worker loader 异常与代码无关，详见 vite.config.ts 注释）。守门：`d102-system-locale.test.ts` 6 例。详见 §6.18 · **v2.29 详情页类型徽章防挤压（D-103）**：主人实测——手机端条目名称太长时，详情页标题右侧「登录/笔记」徽章被挤成逐字换行竖排。根因：`.detail-titles` 横排 flex 里 `h2` 虽有 `text-overflow: ellipsis` 但缺 `min-width: 0`（flex 子项默认 `min-width: auto`，长标题把自身最小宽度撑到内容宽，ellipsis 永不触发），`h2` 不收缩 → 徽章被挤到宽度不足逐字折行。修法（scoped，不动全局 `.pms-badge`）：`h2` 加 `min-width: 0`（真正可收缩，截断生效）；`.detail-titles :global(.pms-badge)` 加 `flex-shrink: 0` + `white-space: nowrap`（徽章永不被挤压换行）。守门：`d103-detail-badge.test.ts` 2 例（源码断言两条规则并存）。详见 §6.19 · **v2.30 泰文母语审校（D-104）**：主人请泰国朋友审阅泰文翻译，反馈两类问题——①**Health 语境错位**：直译用「สุขภาพ」（人体健康）会误导（体检页查的是**密码安全**不是人的健康），导航/体检页/设置分组统一改「ตรวจสอบรหัสผ่าน」（检查密码）语境：`nav.health`→ตรวจสอบรหัสผ่าน、`tab_health`→ตรวจสอบ（tab 78px 窄格取短形）、`health.title/run/score/good`→เริ่มตรวจสอบ/คะแนนความปลอดภัย/รหัสผ่านทั้งหมดอยู่ในสภาพดี、`settings.group_health`→เกณฑ์ตรวจสอบรหัสผ่าน；②**生成器命名冗长**：`gen.title` ตัวสร้างรหัสผ่าน→**สร้างรหัสผ่าน**（泰文软件惯例，如 LINE/Discord），`settings.group_generator`→ค่าเริ่มต้นสร้างรหัสผ่าน；另采纳润色：`group_appearance` ธีมหน้าจอ→**รูปลักษณ์**（标准译）、`clipboard_ttl` 补อัตโนมัติ（自动）、`sync_on_start` เริ่มโปรแกรม→**เปิดโปรแกรม**（惯用语）、`weak_max_len` 补单位 (ตัวอักษร)、`stale_days`→อายุรหัสผ่านที่ถือว่าเก่า (วัน)（原「จำนวนวันเกินกำหนด」看不出是什么的天数）；**保留**：通用按钮 บันทึก / 导航 ซิงค์ / 主题 สว่าง·มืด / 强度 แข็งแรง（密码强度惯用语）——朋友建议按语境取舍不照搬。键集零变化（只改 th 值），i18n 四语键一致性守门不受影响。详见 §6.18 · **v2.30b 泰文全量自查（D-104 续）**：主人追问「其它泰文是否都查过」后通读 th.ts 全部约 250 键与 legal-content.ts 泰文两篇，另修 6 处朋友未扫到的漏网——①`app.tagline` 与免责声明首段：「ศูนย์ความรู้」（零知识直译）因 ศูนย์「零/中心」双义会被读成「知识中心」、「ท้องถิ่นก่อน」语感像「当地人优先」，local-first / zero-knowledge 按泰国科技文本惯例改英文术语；②TERMS 隐私节 telemetry 误译「ข้อมูลวัดระยะไกล」（测距数据）→ telemetry + crash log 夹注；③`sync.rename_too_long`「21 ตัวอักษรจีน」（21 个中文字符）对泰国用户无意义（泰文 UTF-8 同为 3 字节/字符）→ ตัวอักษรไทย；④TERMS 背书句「ห้ามสร้างความเข้าใจว่า」→「ห้ามทำให้ผู้อื่นเข้าใจว่า」；⑤TERMS 争议句「ไกล่เกลี่ย」（第三方调解 mediation）→「เจรจาหาข้อสรุปร่วมกัน」（双方协商）。保留：ลายนิ้วมืออุปกรณ์（微软泰语版同款）、ข้าพเจ้า（法律勾选正式体）、โน้ตความปลอดภัย（Bitwarden 语境） · **v2.31 英/繁全量审校（D-105）**：D-104 同款流程扩到 en-US 与 zh-TW——通读两份约 250 键字典与对应法律文本，修 **26 处**：**①四语言共犯的事实错误**：免责声明写的菜单路径「设置 → 数据 → 导出保险库」，但设置页从未有过「数据」分组（导出保险库是独立区块），zh/en/zh-TW/th 四份一并改为「设置 → 导出保险库」；**②en-US 开发行话清除**：`attach.pending_sync` "once the blob arrives"→"once it has synced"（blob 是开发词）、`settings.auto_lock_failed` "is a fallback"→"may not reflect what was saved"、`gen.symbol_pick` "Tap to…"→"Choose…"（桌面无 tap）、`settings.stale_days` "Stale days"→"Stale after (days)"（与 Auto-lock (minutes) 标签惯例对齐）、`compact_dead_attachments` "use"→"take up"、`sync.rename_too_long` "21 Chinese characters"→"21 CJK characters"（更准确）、`err.restore_conflict` "conflict box"→"Conflicts tab"；**③zh-TW 大陆用语清除**：兜底→預設值、校驗→驗證、閾值→門檻值（×2）、檔位→時間/預設值、出錯→發生錯誤、重放攻擊→**重播攻擊**（台灣資安慣用）、位→個字元（密码长度 ×3）、原始檔案名→原始檔名、「衝突箱」→「衝突」頁面、會話碼→**核對碼**（与 `session_code` 系列统一）、對端→**對方**（×5，电信行话转自然用语）、`health.score` 健康分→健康分數；**④zh-TW 法律文本**：「兜底承諾」→「並非資料安全的保證」。**保留不改**：vault 译「保險庫」（台湾可理解且全 app 一致；Bitwarden zh-TW 用「儲存庫」但与 repository 撞词）、folder 译「群組」（与 zh-CN 概念一致）、`sync.fingerprint` 孤儿键（D-69 后无消费点，仅为键位一致保留）。守门：`d105-locale-polish.test.ts` 4 例（zh-TW 大陆用语禁入清单 / 会话码用语统一 / en-US 行话清单 / 法律路径四语断言）。详见 §6.18 · **v2.31b 英/繁审校第二轮（D-105 续）**：主人复查后做增量审校（与 v2.31 不重复的另一批）。**复核裁定**：vault 维持「保險庫」——第二轮曾试改「保存庫」（Azure 官方口径），与 v2.31 已落盘裁定冲突即回滚，维持原文案。**zh-TW 字典（14 处）**：`common.none` 暫無→無；量词「{n} 條」→「{n} 筆」（items_count / duplicates / result_applied / adopt_requires_empty 共 4 处，台湾资料惯用量词）；`gen.symbol_pick` 润色「點選要允許出現的符號（全不選 = 使用預設安全集…）」；`health.stale` 超期未改→**逾期未更換**、`stale_days` 超期天數→逾期天數（「超期」偏大陆用语）；`conflict.title` 衝突仲裁→**衝突處理**（仲裁是法律术语，软件 UX 惯用处理）；`auto_lock_failed`/`vault_file_failed`/`compact_stats_failed` 三处「未能取得」→「無法取得」并补全宾语；`pair_completed_hint` 语序理顺；`session_code_hint`「被中間人隔開」→「遭到中間人攻擊」（资安标准用语）。**en-US 字典（12 处）**：`create.disclaimer_link` Disclaimers→**Disclaimer**（单数，与弹窗标题一致）；`create.weak_hint` "A longer passphrase"→"a longer one"（passphrase 是专指词组口令的另一个概念）、"check to continue"→"Select the checkbox to continue"；`gen.exclude_ambiguous` 补全→"Exclude ambiguous **characters**"；**英式拼写清除**：recognises→recognizes、recognisable→recognizable（en-US 混入英式）；`clipboard_ttl` "Clipboard clear"→"Clipboard **auto-clear**"；`auto_lock_failed` 二次收敛→"the value shown is the default"（与 zh-CN「显示的是默认值」语义对齐，原 "may not reflect what was saved" 含糊）；`attach.export_confirm_body` "Confirm the location is safe."→"Please make sure the location is safe."（祈使歧义）；`compact_start` Start→**Start compaction**（孤立 Start 指代不明）；`start_pairing` / `err.pair_token_expired` QR→**QR code**（全文统一）；`sync.result_conflicts` "conflicts created"→"conflicts **detected**"（冲突是被发现不是被制造，软件惯例）；`conflict.take_remote` "Take remote"→"**Use remote**"（与 Keep local/Keep both 并列自然）。**zh-TW 法律文本（4 处）**：「現狀」→「現況」（heading+正文，微软台湾 EULA 同款）；適銷性→**適售性**（同上）；系統重裝→系統重新安裝；同步誤操作→同步操作失誤。**保留**：`sync.rename_too_long`「約 21 個漢字」（台湾可读）。守门沿用 `d105-locale-polish.test.ts`；三门 44 文件 341 例全绿。 · **v2.32 外部审查修复（D-106，豆包报告 2026-10-27）**：按主人指令逐条核实 docs/PasswordMS-审查报告-2026-10-27.md（58 条）并修复属实项。**前端**：①WindowControls 三钮 aria/title 硬编码中文入语言包（`common.minimize/maximize_restore/close_window` 四语言，5.2-1）；②ItemEditor TOTP placeholder 硬编码入语言包（`editor.totp_placeholder`，5.2-2）；③`vault.load()` 加单调递增 `loadSeq` 请求序号守卫——过期响应不写状态、不 toast、不复位 busy（3.5-1/3.5-2，recordItemUsage/setQuery 等全部调用点自动受益）；④剪贴板到期清空 IPC 失败 toast `err.clipboard_clear_failed` 不再静默（3.5-3）；⑤剪贴板 TTL 双层钳制 5–120（Settings onchange + 保存时 + 后端 `update_settings` clamp，3.5-4）；⑥启动 `get_status` 失败不再伪装「无库」——`session.statusError` + App.svelte 显示「启动失败 + 重试」面板（防误入创建向导，3.5-5）；⑦拖拽 `clickSuppressed` 加 `setTimeout(0)` 兜底复位（3.5-6）；⑧泰文日期改佛历 DD/MM/YYYY（年+543，5.2-9）。守门：core `d106_external_audit.rs` 3 例（restore 悬空分组 / blob 帧超限 / 同步条目超限）。Rust 侧修复（容器 sync_all、place_frame/tombstone 顺序、write_blob_frame 与 put_item_direct 大小守卫、session 解锁统一化、client.rs 水位、resync 锁外网络 I/O、copy_secret TTL 生效、pending_imports 锁中毒兜底）见 02-arch §3.14。详见 §6.20 · **v2.33 更新检查 + 审查余项（D-107/D-108，2026-10-27）**：①**D-107 更新检查**（方案 B，主理人裁定）：「关于」卡片新增 `自动检查更新` 开关（**默认关**，关=零网络请求）+ `检查更新` 手动按钮 + 新版本警告条与「前往下载」（跳 GitHub Releases，open_url 白名单链路）；纯逻辑抽 `update-check.ts`（版本比较/应答解析/24h 节流纯函数），WebView `fetch` 直连 GitHub API（CSP `connect-src` 放行 + 纵深指令补齐），**零新增 Rust 依赖**；四语言用户协议补披露句。②**D-108 前端余项**：App.svelte 常驻监听 `autolock://locked`（Rust auto-lock 兜底触发后复位 store 切锁屏）；死命令 `sync_accept_pair` 双侧删除（命令面 59→58）；`saveQuiet` 独立 `quietBusy`；CSV 表头读取失败 toast（原静默重置）；解锁后五路加载 silent + allSettled + 失败重试一次 + 合并 `err.load_partial`（原 toast 堆叠）；库文件卡片只显文件名（`file_name`，DTO 更名，四语言 hint「路径见下」→「文件名见下」）；`err.attachment_not_found` 入四语言。守门：`d107-update-check.test.ts` 9 例 + `d108-audit-guard.test.ts` 12 例。详见 §6.21 · **v2.34 复选框行窄屏回归修复（D-110，2026-10-27）**：主人手机实测——设置页「同步」「关于」组复选框跑到文字下一行。根因：v2.12 的窄屏行类型纪律只登记了 `.is-info`/`.is-actions` 两种例外，**复选框行是第三种行类型**漏登记，被 `.set-row { flex-direction: column }` 一刀切堆叠；D-107 新增的「自动检查更新」行沿用裸 `.set-row` 继承同一缺陷。修法：新增 `.is-check` 行类型（3 行标记 + 窄屏媒体块内覆盖回 row + space-between，桌面零规则逐像素不变），守门测试扩 10 例。详见 §6.22 · **v2.35 core 层文案本地化（D-112，2026-10-27）**：泰国朋友实测泰文 UI 下健康检查建议显示中文——全仓排查出 Rust core 硬编码用户文案三处（health.rs 4 条建议 / 冲突副本标题「 (冲突副本)」/ CSV 导入模板示例行），同病根一并修：**core 只出数据、前端出文案**（与 D-102 i18n 架构对齐）。`HealthReport` 删 `suggestions` 字段改前端按计数组装（`health.sug_weak/sug_reused/sug_stale` 四语言同加，皆空用既有 `good`）；`resolve_conflict` 全链加 `copy_suffix` 参数（前端传 `conflict.copy_suffix`，存量副本标题不动）；CSV 模板常量改 `csv_template(&TemplateTexts)` 函数（表头/转义示范结构留 core，示例文本 `import.tpl_*` 5 键随 UI 语言，引号/换行注入 core 拒收）。守门：import_template.rs 参数化双语言跑全量契约 + template.rs 单测；core 34 组绿 / clippy 0 / fmt 过；前端三门绿。需两端同版本升级。详见 §6.23 · **v2.36 快速解锁 UI（D-113 P2，2026-10-28）**：解锁页主按钮下方新增「使用 Windows Hello 解锁」ghost 按钮（**仅** `availability=available && state=ready` 时渲染；取消静默、失效弹原因并重查状态、需主密码给引导语而非点亮「解锁失败」红字）；设置页新增独立「快速解锁」卡片（位置「安全」之后，状态行标 `.set-row.is-info`，开启原地展开主密码输入框 + 确定/取消、关闭走二次确认，平台不可用置灰并给红字原因，始终显示风险说明「能拿到本机 Windows 登录凭据者可绕过 Hello」）。新增 30 键 × 四语言；`is-info` 行 5→6、标记行 10→11、命令面 58→62，三处守门计数同步。详见 §6.24 · **v2.37 快速解锁走查三修（D-114，2026-10-28）**：主人真机实测三条。①**改密无回执**——成功只 `toast('success','common.ok')`，`common.ok` 是按钮标签「确定」，3 s 消失，主人原话「右下角一闪而过」；改为卡片内常驻绿条 `.pms-alert.is-success + role="status"`（`settings.password_changed`），三个输入框任一改动即清除。②**快速解锁静默消失**——命令层按 §18.5 约束 2 删封印，前端既不重查也不解释；改为改密前记 `bioEnabled`、成功后 `refreshBiometric()`，命中时用 `settings.password_changed_bio_off` 如实说明「已因此关闭，重新开启后才能继续用」。③**「没有硬件」误导**——`UserConsentVerifier` 在 Hello 从未配置的机器上回报 `DeviceNotPresent`（非 `NotConfiguredForUser`），旧文案「本机没有可用的 Windows Hello 硬件」把用户带进死胡同；改为「未检测到可用 Windows Hello（多半是尚未设置，不一定缺少硬件）」+ `bio_avail_hint` 指引 + 「打开 Windows 登录选项」按钮（`open_biometric_settings` 无参数窄命令 → `ms-settings:signinoptions`，不放宽 `open_url` 的 http/https 白名单）+ 窗口获焦自动重查。新增 5 键 × 四语言，命令面 62 → 63。详见 §6.25 · **v2.38 错误口令文案与「已开启但不可用」（D-115，2026-10-28）**：①开启快速解锁时主密码输错，提示的是「解锁失败，请重试」——那是**锁屏入口**专用文案（A-05），人在设置页里读不懂；新增 `ERR_WRONG_MASTER_PASSWORD` /「主密码错误，请重新输入。」，只用于**已解锁会话内**的三个复核点（开启快速解锁 / 改密旧密码 / 导出保险库），锁屏入口一字未动；提示位置也从右下角 3 s toast 改为**就地常驻**（开启卡片在输入框下方 `role="alert"`、改密卡片在底部结果条）。②Hello 被关掉时状态行仍写「已开启（日期）」，与下方红字「未检测到可用的 Windows Hello」同屏自相矛盾；新增**显示态** `bioDegraded = ready && !available` → 状态行「已开启，但当前不可用（{date}）」（warning 色 `.bio-state.is-warn`）+ `bio_degraded_hint` 解释「关闭期间一次也用不了 / 重新开通自动恢复 / 撤销请点关闭」（后端 DTO 零改动）。新增 3 键 × 四语言。详见 §6.26 · **v2.39 未实现平台不渲染快速解锁卡（D-116，2026-10-28）**：安卓 v1.3.1 设置页出现「Windows Hello 快速解锁 · 未开启 · 当前平台不支持快速解锁」——名词错（Windows 专属搬到安卓）+ 理由错（不是「不支持」，是 P3 还没做；手机指纹人脸都正常）。根因是 `Availability::Unsupported` 一词双义，core 拆出 `NotImplemented`（本平台无适配器）与 `Unsupported`（有适配器但平台此刻做不了，必须可见）；前端在 `not_implemented` 与 `bio === null` 时**整卡不渲染**（置灰也不行——那是给不存在的功能做广告），`unsupported` 文案改口径为「Windows Hello 组件在当前系统上无法调用」。详见 §6.27 · **v2.40 快速解锁卡片平台化（D-113 P3 / Android Keystore，2026-10-28）**：D-116 当初把这张卡在安卓上整块藏起来，理由是「功能还不存在」。P3 落地后它真的存在了——于是 **D-116 的判定条件从「本平台有没有适配器」变成「本平台是不是 Windows 或 Android」**，安卓首次显示这张卡。随之而来的是**全量平台取词**：`BiometricStatusDto` 新增 `platform` 字段（后端自称身份，不用 UA 嗅探），前端 `lib/bio-platform.ts` 按平台选 `_android` 后缀键；**卡片标题** Windows「Windows Hello 快速解锁」/ Android「指纹 / 人脸快速解锁」；**可用性话术**改用 `{noun}` 插值（Windows 填 `Windows Hello`、Android 填 `指纹 / 人脸`、**英文填 `Biometric unlock`**——`fingerprint / face` 会让动词出错）；**引导路径按平台** Windows「设置 → 账户 → 登录选项」/ Android「设置 → 安全 → 指纹、人脸与密码」，按钮「打开 Windows 登录选项」/「打开系统生物识别设置」；**风险说明必须分平台重写**——Android 是 `AUTH_BIOMETRIC_STRONG` 硬绑定（新增指纹即永久失效、PIN 顶替不了），照搬 Windows 那句「拿到登录凭据者可绕过」在安卓上根本不成立；**失效原因** Android 用「设备的指纹 / 人脸记录已变更」；**改密回执**与**锁屏按钮**（「用指纹 / 人脸解锁」「等待指纹 / 人脸验证…」）同步平台化。新增 12 键 × 四语言。BiometricPrompt 自身的标题/副标题/取消按钮走 Android 资源（`res/values*/strings.xml` 四语言），跟随**系统**语言而非应用内语言——与系统原生安全对话框的预期一致。详见 §6.28 · **v2.41 快速解锁失效闭环（D-119/D-120/D-121，2026-10-28 主人真机）**：①指纹变更后点解锁会提示失效，但设置页仍显示「已开启」、只有「关闭」按钮——状态不收敛，D-117 的重建路径**没有入口可点**。修法：解锁时若平台宣告密钥永久失效，就地给 `biometric.json` 盖**墓碑**（`invalidated` 字段），`inspect` 据此报 `Stale`；记录**不删**（删了就退化成「从未开启」，看不到原因）。锁屏按钮随之消失、设置页状态行改说失效原因、「开启快速解锁」重新出现。②安卓上弹「Windows Hello 配置已变更」——`err.bio_invalidated_enrollment` / `err.biometric_canceled` 两键四语言写死 Windows Hello，前端按平台改写（清单驱动，只改这两条），四语言补 `_android` 变体。③「打开系统生物识别设置」按钮在指纹正常可用时找不到——渲染条件原嵌在「平台不可用」块内，而指纹变更恰好发生在 `availability = available` 时。按主人拍板的 B 案，判据提为 `!bioAvailable || state ∈ {stale, corrupt}`：有事可做才出现，一切正常不打扰。详见 §6.29 · **v2.42 安卓系统设置入口撤除（D-122，2026-10-28 主人真机）**：D-121 让「打开系统生物识别设置」在失效时可见，但真机点击**完全静默**——没有红字就排除了命令失效与 `startActivity` 抛异常（两者都必然 `reject` → toast），说明 Intent 已发出、被 ROM 忽略或立刻 finish，客户端无从判定。各品牌菜单位置不同，没有任何 action 能保证命中，留一个点了没反应的按钮比不给更糟。Android 改为纯文案引导（新增 `bio_manage_hint_android` 四语言，只写「去设置里打开生物识别 / 指纹 / 人脸页面」，**不列品牌、不写菜单层级**；删 `bio_avail_hint_android` / `bio_open_settings_android`），Kotlin `openSettings` 与 Rust 调用一并删除；Windows 的 `ms-settings:signinoptions` 保留。详见 §6.29③。 · **v2.43 安卓提示去掉「自动重查」承诺（2026-10-28 主人真机）**：该句靠 `window` 的 `focus` 事件，Android 上 Activity 回前台时 document 从未失焦，事件不触发 ⇒ 状态不刷新，承诺不成立。删句；守卫测试加禁词（自动重新检测 / re-checks automatically / อัตโนมัติ），变异验证有效。要让承诺成立需 Rust 侧接 `RunEvent::Resumed`（另开）。详见 §6.29③。

---

## 0. 硬约束（不许违反）

1. **零外部资源**。不引 Web 字体、不引图标库、不引 CSS 框架、不加外链。图标继续用 `common/Icon.svelte` 的内联 SVG，字体继续用系统字体栈。
2. **体积**。纯 CSS 改动对体积影响可忽略，但禁止借此引入任何新依赖（Windows ≤10MB / APK ≤12MB 的门禁不许破）。
3. **i18n**。组件内**禁止中文字面量**，一律走 `ui.t()`。新增 key **必须同时**加到 `src/lib/i18n/` 下**全部四个字典**（`zh-CN.ts` / `zh-TW.ts` / `en-US.ts` / `th.ts`，键结构以 zh-CN 为唯一事实来源；有 `i18n.test.ts` / `i18n-usage.test.ts` 做键位与用法校验）。
4. **双主题**。暗色与浅色都要好看，不允许"暗色精修、浅色将就"。

---

## 1. 命名的根因修复（D-19）

### 事故回顾

`global.css` 里有 `.field { display:flex; flex-direction:column; gap:.35rem; margin-bottom:.75rem }`；
`CreateVaultWizard.svelte` / `LockScreen.svelte` 里又各自定义了同名局部类 `.field { display:flex; align-items:center; ... }`。

Svelte 给局部样式加 hash 抬优先级，所以 `display`、`align-items` 由局部胜出；
**但 `flex-direction: column` 只存在于全局规则里，局部没覆盖它，于是"漏"了过来**。

后果：输入框与眼睛按钮被**纵向堆叠**，外面套一层边框 → 看起来是一个怪高的大框，
"至少 8 位"孤零零占上半截、眼睛浮在下半截，**用户找不到该在哪里输入**。

### 从此以后的规定

1. **共享类一律加 `pms-` 前缀**，只允许写在 `src/styles/global.css`。
2. **组件局部类禁止与 `pms-` 共享类同名**；局部类建议加组件前缀（`wz-`、`lock-`、`item-`、`sync-`…）。
3. **列布局与行布局必须是两个不同的类名**，从结构上杜绝这次的事故：
   - `.pms-field` = **纵向**（label 在上，控件在下）
   - `.pms-input-group` = **横向**（输入框 + 尾部按钮）
4. 旧的无前缀类（`.btn` / `.field` / `.icon-btn` / `.check` / `.field-row`）**全部废弃**，本次一并改名，不留别名。

---

## 2. 设计令牌（`global.css` `:root`）

```css
:root {
  /* —— 字体 —— */
  --pms-font: system-ui, -apple-system, 'Segoe UI', 'Noto Sans SC', sans-serif;
  --pms-mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;

  /* —— 圆角 —— */
  --pms-r-sm: 8px;
  --pms-r: 12px;
  --pms-r-lg: 16px;
  --pms-r-pill: 999px;

  /* —— 控件尺寸（桌面）—— */
  --pms-ctl-h: 42px;      /* 单行输入 / 按钮 */
  --pms-ctl-h-lg: 46px;   /* 主行动按钮 */
  --pms-hit: 34px;        /* 图标按钮最小点击区 */

  /* —— 动效 —— */
  --pms-dur: 140ms;
  --pms-ease: cubic-bezier(.4, 0, .2, 1);
}

/* ========== 暗色：主打方向，科技感 ========== */
@media (prefers-color-scheme: dark) {
  :root {
    --pms-bg:            #0A0E14;
    --pms-bg-2:          #0E141C;
    --pms-surface:       #121A24;
    --pms-elevated:      #17212D;
    --pms-border:        #1E2A38;
    --pms-border-strong: #2C3E52;
    --pms-fg:            #E6EEF7;
    --pms-muted:         #8AA0B6;
    --pms-faint:         #5C7186;
    --pms-accent:        #2DD4BF;
    --pms-accent-hover:  #5EEAD4;   /* 修正 v1.2：悬停必须显式给色，不能靠 filter: brightness() */
    --pms-accent-2:      #58A6FF;
    --pms-accent-ink:    #04211D;
    --pms-accent-soft:   rgba(45, 212, 191, .12);
    --pms-danger:        #F87171;
    --pms-danger-hover:  #FCA5A5;
    --pms-danger-ink:    #2A0A0A;   /* 修正 v1.2：原固定 #fff 压浅红 #F87171 只有 2.77:1 */
    --pms-danger-soft:   rgba(248, 113, 113, .12);
    --pms-warning:       #F2B45C;
    --pms-warning-soft:  rgba(242, 180, 92, .12);
    --pms-success:       #3DDC97;
    --pms-track:         #2C3E52;   /* 修正 v1.1：原 #1E2A38 叠在卡片 #121A24 上 RGB 差仅 24，肉眼不可见 */
    --pms-shadow:        0 1px 2px rgba(0,0,0,.55), 0 10px 28px -14px rgba(0,0,0,.8);
    --pms-glow:          0 0 0 1px rgba(45,212,191,.45), 0 0 26px -10px rgba(45,212,191,.55);
  }
}

/* ========== 浅色：精修，干净专业 ========== */
@media (prefers-color-scheme: light) {
  :root {
    --pms-bg:            #F3F5F8;
    --pms-bg-2:          #EAEEF3;
    --pms-surface:       #FFFFFF;
    --pms-elevated:      #FFFFFF;
    --pms-border:        #DFE5EC;
    --pms-border-strong: #C3CDD9;
    --pms-fg:            #0E1620;
    --pms-muted:         #5A6B7E;
    --pms-faint:         #8B9AAB;
    --pms-accent:        #0D9488;
    --pms-accent-hover:  #0F766E;
    --pms-accent-2:      #2563EB;
    --pms-accent-ink:    #FFFFFF;
    --pms-accent-soft:   rgba(13, 148, 136, .10);
    --pms-danger:        #D64545;
    --pms-danger-hover:  #B93A3A;
    --pms-danger-ink:    #FFFFFF;
    --pms-danger-soft:   rgba(214, 69, 69, .10);
    --pms-warning:       #B7791F;
    --pms-warning-soft:  rgba(183, 121, 31, .12);
    --pms-success:       #17845A;
    --pms-track:         #C3CDD9;   /* 修正 v1.1：原 #E3E8EF 太浅 */
    --pms-shadow:        0 1px 2px rgba(16,24,40,.06), 0 10px 26px -16px rgba(16,24,40,.22);
    --pms-glow:          0 0 0 3px rgba(13,148,136,.20);
  }
}
```

> 兼容老令牌名：`--pms-hover` 保留并指向 `--pms-bg-2`，避免一次性改动过大。

---

## 3. 共享类（`global.css`，可直接落盘）

```css
/* ---------- 基线 ---------- */
* { box-sizing: border-box; }
html, body {
  margin: 0; padding: 0;
  background: var(--pms-bg); color: var(--pms-fg);
  font-family: var(--pms-font); font-size: 15px; line-height: 1.55;
  -webkit-font-smoothing: antialiased;
}
#app { min-height: 100vh; }

:where(a, button, input, textarea, select, [tabindex]):focus-visible {
  outline: none;
  box-shadow: var(--pms-glow);
  border-color: var(--pms-accent);
}

/* ---------- 按钮 ----------
 * 【硬性不变量 · D-27】通用规则 `.pms-btn:hover:not(:disabled)` 的特异性是 (0,3,0)，
 * 会抢走任何特异性更低的选择器所设的 background。因此：
 *   ① 每个颜色变体（is-primary / is-danger / is-ghost）必须自带 `:hover:not(:disabled)` 规则；
 *   ② 该规则必须把 background / border-color / color **三项全部显式写出**；
 *   ③ 不要用 filter: brightness() 表达悬停 —— 它同时创建层叠上下文，且无法被本规范校验。
 * 违反后果实测：主按钮悬停时 background 被抢成 --pms-bg-2，白字压浅灰，
 * 浅色 1.17:1 / 暗色 1.09:1 —— 按钮"一片空白"，且两个主题全中招。
 * 由 scripts/check-css-contract.mjs 的 INV-1 守门，新增变体需同步其 CASES 矩阵。 */
.pms-btn {
  display: inline-flex; align-items: center; justify-content: center;
  gap: .4rem; min-height: var(--pms-ctl-h); padding: 0 .95rem;
  border: 1px solid var(--pms-border-strong);
  border-radius: var(--pms-r); background: var(--pms-elevated);
  color: var(--pms-fg); font-size: .875rem; font-family: inherit;
  font-weight: 500; cursor: pointer; white-space: nowrap;
  transition: background var(--pms-dur) var(--pms-ease),
              border-color var(--pms-dur) var(--pms-ease),
              transform var(--pms-dur) var(--pms-ease);
}
.pms-btn:hover:not(:disabled) { background: var(--pms-bg-2); border-color: var(--pms-accent); }
.pms-btn:active:not(:disabled) { transform: translateY(1px); }
.pms-btn:disabled { opacity: .5; cursor: not-allowed; }

.pms-btn.is-primary {
  background: var(--pms-accent); border-color: var(--pms-accent);
  color: var(--pms-accent-ink); font-weight: 600;
}
.pms-btn.is-primary:hover:not(:disabled) {
  background: var(--pms-accent-hover); border-color: var(--pms-accent-hover);
  color: var(--pms-accent-ink);
}
.pms-btn.is-danger {
  background: var(--pms-danger); border-color: var(--pms-danger);
  color: var(--pms-danger-ink); font-weight: 600;
}
.pms-btn.is-danger:hover:not(:disabled) {
  background: var(--pms-danger-hover); border-color: var(--pms-danger-hover);
  color: var(--pms-danger-ink);
}
.pms-btn.is-ghost { background: transparent; border-color: transparent; color: var(--pms-muted); }
.pms-btn.is-ghost:hover:not(:disabled) {
  background: var(--pms-bg-2); border-color: transparent; color: var(--pms-fg);
}
.pms-btn.is-sm      { min-height: 30px; padding: 0 .6rem; font-size: .8rem; border-radius: var(--pms-r-sm); }
.pms-btn.is-block   { width: 100%; }
.pms-btn.is-lg      { min-height: var(--pms-ctl-h-lg); font-size: .95rem; }

/* ---------- 图标按钮：右端对齐、有热区 ---------- */
.pms-icon-btn {
  display: inline-flex; align-items: center; justify-content: center;
  width: var(--pms-hit); height: var(--pms-hit); flex: 0 0 var(--pms-hit);
  border: none; background: transparent; color: var(--pms-muted);
  border-radius: var(--pms-r-sm); cursor: pointer;
  transition: color var(--pms-dur) var(--pms-ease), background var(--pms-dur) var(--pms-ease);
}
.pms-icon-btn:hover { color: var(--pms-fg); background: var(--pms-bg-2); }

/* ---------- 输入族 ---------- */
.pms-input {
  width: 100%; min-height: var(--pms-ctl-h);
  padding: 0 .75rem;
  border: 1px solid var(--pms-border-strong);
  border-radius: var(--pms-r);
  background: var(--pms-bg-2); color: var(--pms-fg);
  font-size: .93rem; font-family: inherit; outline: none;
  transition: border-color var(--pms-dur) var(--pms-ease), box-shadow var(--pms-dur) var(--pms-ease);
}
.pms-input::placeholder { color: var(--pms-faint); }
.pms-input:hover:not(:disabled) { border-color: var(--pms-accent); }
.pms-input:disabled { opacity: .55; }
textarea.pms-input { padding: .6rem .75rem; line-height: 1.6; resize: vertical; min-height: 96px; }

/* 纵向：label 上、控件下 */
.pms-field { display: flex; flex-direction: column; gap: .4rem; margin-bottom: .9rem; }
.pms-field > label,
.pms-label { font-size: .8rem; font-weight: 500; color: var(--pms-muted); }

/* 横向：输入框 + 尾部按钮（就是原来想要的那个"一行"结构） */
.pms-input-group {
  display: flex; flex-direction: row; align-items: center; gap: .25rem;
  padding-right: .25rem;
  border: 1px solid var(--pms-border-strong);
  border-radius: var(--pms-r); background: var(--pms-bg-2);
  transition: border-color var(--pms-dur) var(--pms-ease), box-shadow var(--pms-dur) var(--pms-ease);
}
.pms-input-group:focus-within { border-color: var(--pms-accent); box-shadow: var(--pms-glow); }
.pms-input-group > .pms-input {
  flex: 1 1 auto; border: none; background: transparent; min-width: 0;
}
.pms-input-group > .pms-input:focus-visible,
.pms-input-group > .pms-input:hover { border-color: transparent; box-shadow: none; }

/* ---------- 复选框行 ---------- */
.pms-check {
  display: flex; align-items: center; gap: .55rem;
  font-size: .86rem; color: var(--pms-fg); cursor: pointer;
  user-select: none;
}
.pms-check input[type='checkbox'] {
  width: 17px; height: 17px; flex: 0 0 17px; margin: 0;
  accent-color: var(--pms-accent); cursor: pointer;
}

/* ---------- 内联链接按钮（D-63：勾选行的「免责声明 / 用户协议」入口） ---------- */
/* 语义 = 按钮但长得像链接；不继承 .pms-btn 的盒子（padding/bg/border 全清）。
   字号一律 `font: inherit`（D-64 起不再写死 0.86rem）——窄屏勾选行靠容器收敛字号，
   写死会让链接比同行文字大、把行撑折。下划线 offset 2px 防止贴住中文笔画。
   与 checkbox 的 label 必须互为**兄弟节点**（见 §5.5「点链接不切换勾选」）。 */
.pms-link {
  display: inline;
  border: none; background: none; padding: 0; margin: 0;
  color: var(--pms-accent);
  font: inherit;
  cursor: pointer;
  text-decoration: underline; text-underline-offset: 2px;
}
.pms-link:hover { color: var(--pms-accent-hover); }
.pms-link:focus-visible { outline: 2px solid var(--pms-accent); outline-offset: 2px; border-radius: 2px; }

/* ---------- 容器与文字 ---------- */
.pms-card {
  background: var(--pms-surface);
  border: 1px solid var(--pms-border);
  border-radius: var(--pms-r-lg);
  box-shadow: var(--pms-shadow);
}
.pms-meta  { font-size: .8rem; color: var(--pms-muted); }
.pms-hint  { font-size: .78rem; color: var(--pms-faint); margin: .1rem 0 0; }

.pms-badge {
  display: inline-flex; align-items: center; gap: .25rem;
  padding: .1rem .5rem; border-radius: var(--pms-r-pill);
  font-size: .74rem; font-weight: 600;
}
.pms-badge.is-accent  { background: var(--pms-accent-soft);  color: var(--pms-accent); }
.pms-badge.is-danger  { background: var(--pms-danger-soft);  color: var(--pms-danger); }
.pms-badge.is-warning { background: var(--pms-warning-soft); color: var(--pms-warning); }

/* ---------- 提示条 ---------- */
.pms-alert {
  display: flex; align-items: flex-start; gap: .55rem;
  padding: .7rem .85rem; border-radius: var(--pms-r);
  font-size: .82rem; line-height: 1.5;
}
.pms-alert.is-warning { background: var(--pms-warning-soft); color: var(--pms-warning); }
.pms-alert.is-danger  { background: var(--pms-danger-soft);  color: var(--pms-danger); }

@media (prefers-reduced-motion: reduce) {
  * { transition: none !important; animation: none !important; }
}
```

---

## 4. 交互规范

| 项 | 规则 |
|---|---|
| **密码可见性图标** | **隐藏（显示星号）时 → `eye_off`（带斜杠的眼睛）**；**明文时 → `eye`（睁眼）**。即 `name={showPassword ? 'eye' : 'eye_off'}`。**当前 4 处全反了**（见 §6 D-20）。 |
| **图标位置** | 眼睛一律在输入框**最右端**，用 `.pms-input-group` 承载；`--pms-hit` 34px 热区，鼠标悬停有底色。 |
| **label 与 placeholder** | 两者**文案必须不同**。label 说"是什么"（主密码），placeholder 说"怎么填"（请输入主密码 / 至少 8 位 / 再次输入主密码）。**禁止同一条 key 同时当 label 与 placeholder**。 |
| **焦点环** | 统一 `:focus-visible` → `--pms-glow`；输入组用 `:focus-within`。不许出现浏览器默认蓝框。 |
| **错误提示** | 就地内联小字（`.pms-hint` 形态 + `--pms-danger` 色），**不要**弹大块红字。 |
| **触控目标** | 所有可点元素 ≥ 40px（移动端 ≥ 44px）。 |
| **动效** | 只做 background / border-color / transform / opacity，≤160ms；尊重 `prefers-reduced-motion`。 |
| **原生控件的干涉（D-30）** | WebView2/Blink 会为 `input[type=password]` **渲染一个原生密码揭示按钮**（`::-ms-reveal`）。它和我们自己的 `.pms-icon-btn` 眼睛会**同时出现**，用户看到两个眼睛。**凡是自绘了眼睛的密码框，必须一并关掉原生那个**（见 §3 `::-ms-reveal` 段）。同理，`::-ms-clear`（原生清除叉）也要关。 |
| **右侧详情面板的状态机（D-31）** | 右侧面板只有三种态：`detail`（看）/ `create`（新建）/ `edit`（编辑）。**`select`（点击列表项）、`done`（保存完成）、`section`（点顶栏分区）三个事件都必须强制回到 `detail`**。绝对禁止把"是否处于新建态"与"当前选中了哪一项"拆成两个互不相关的布尔/字符串来各自维护 —— 那正是 D-31 的成因。逻辑收敛在 `src/lib/pane-mode.ts` 的纯 reducer 里，有 `pane-mode.test.ts` 8 例守门（含 `create + select → detail` 回归用例）。 |
| **窗口标题栏主题（D-32）** | 原生标题栏（`tauri.conf.json` 的 `app.windows[0].theme`）与应用主题**必须同步**。默认窗口主题是 `Dark`；用户把主题改成 `light` / `dark` 时，由 `theme.svelte.ts` 的 `syncWindowChrome()` 调 `getCurrentWindow().setTheme()` 跟随；选 `system` 时传 `null` 交还系统。**不这么做的话，浅色系统上会出现"白色标题栏 + 深色应用"的硬切缝**（实测 y=0~56 是 `rgb(242,242,242)`，y=60 起是 `#121A24`，无过渡）。 |
| **生成器必须"能被发现"** | 密码生成器是核心卖点功能，**必须有两个入口**：① 密码输入框右侧的骰子图标（`.pms-icon-btn` + `title` 提示）；② 顶栏的「生成密码」按钮（只生成+复制，不写回条目）。**禁止**把它藏在一个无 tooltip 的 `⟳` 图标里 —— 客户实测反馈"根本找不到"。弹窗形态用 `PasswordGeneratorModal.svelte`（Modal 内部 `{#if open}` 包裹 → 关闭即卸载，不残留请求/toast）。 |

---

## 5. 逐页改造要求

### 5.1 解锁页 `LockScreen.svelte`（P0）

```
.pms-lock            /* 居中容器，min-height:100vh，padding 1.5rem，背景加一层极淡的青绿径向光晕 */
  .pms-card.pms-lock__card   /* width: min(400px,100%)，padding 2rem */
    .pms-lock__brand
      .pms-lock__logo   /* 44px 圆角方块，--pms-accent-soft 底，内嵌 shield 图标 --pms-accent 色 */
      h1  PasswordMS     /* 1.35rem / 600 */
      p   tagline        /* .pms-meta */
    form
      .pms-field
        label  "主密码"                      ← 新增 key `lock.password_label`
        .pms-input-group
          input.pms-input  placeholder="请输入主密码"
          button.pms-icon-btn（眼睛，右端）
      {错误时} p.pms-hint（--pms-danger）
      button.pms-btn.is-primary.is-block.is-lg  "解锁"
    button.pms-btn.is-ghost.is-block  "创建新保险库"
    button.pms-btn.is-ghost.is-block  "从备份恢复"（D-55，紧随其后，ghost 块钮）
      ↳ 流程：dialog.open（filter 仅 .vault）选备份 → 主密码为空先 toast
        `lock.restore_password_required`（不新增第二个密码框，复用上方密码框的值）
        → ui.confirm 警告「备份将替换本机当前库，原库自动备份为 .bak」
        → `restore_backup`（后端校验 magic，不合法返回 ERR_VALIDATION 且不触碰本机库；
          本机库已存在时先留 `passwordms.vault.bak-<unix_ms>`；fs::copy 保留 mtime）
        → 成功后用密码框的值走既有 `unlock` 流程（unlock_check 校验的是备份创建时
          的主密码；失败仍走 A-05 统一提示）。恢复过程中解锁/恢复/创建钮全部禁用。
```

### 5.2 创建保险库 `CreateVaultWizard.svelte`（P0）

```
.pms-wizard            /* 同布局；width: min(460px,100%) */
  .pms-card.pms-wizard__card   /* padding 2rem */
    h1 "创建保险库" + p.pms-meta tagline
    .pms-alert.is-warning   /* 图标 + "主密码无法找回。请妥善保管……" —— 换成真正的提示条，不要再是裸橙字 */

    .pms-field
      label "主密码"
      .pms-input-group  >  input.pms-input[placeholder="至少 8 位"]  +  眼睛 .pms-icon-btn
      StrengthMeter       ← 改成 4 段彩色条 + 右侧文字（弱/一般/强/很强），未输入时整条压暗
      p.pms-hint "8 位以上，建议混合大小写、数字与符号"
      {tooShort} p.pms-hint（--pms-danger）

    .pms-field
      label "确认主密码"
      .pms-input-group  >  input.pms-input[placeholder="再次输入主密码"]  +  眼睛 .pms-icon-btn
      ← **必须与上面完全同构**（含眼睛），否则两个框长得不一样又会让人困惑
      {mismatch} p.pms-hint（--pms-danger）

    .pms-wizard__ack      /* 浅底圆角块，包住 checkbox 行，视觉上独立成区 */
      /* v2.8（D-64）：**必须单行放下**（375px 手机实测原排版折两行）。
         文案精炼为「我已阅读并同意」（15 字）+ 整行字号收敛：≤767px 时 0.8rem / gap 0.2rem /
         padding 0.6rem 0.7rem / checkbox 16px。flex-wrap 保留作兜底（极端窄屏仍可折行不溢出）。 */
      label.pms-check  "我已阅读并同意"      /* 只包 checkbox + create.acknowledge_prefix */
      button.pms-link  "免责声明"            /* ← label 的**兄弟节点**，点它只开弹窗 */
      span.wz-ack-mid  "与"                  /* create.acknowledge_middle */
      button.pms-link  "用户协议"            /* ← 同上 */

    button.pms-btn.is-primary.is-block.is-lg  "创建并解锁"
    button.pms-btn.is-ghost.is-block  "返回"
```

> `MIN_MASTER_PASSWORD_LEN` 的硬拦截、`acknowledged` 门禁、`canSubmit` 逻辑**一律不许改**，只动样式与结构。
> 新增 i18n key：`lock.password_label`、`create.password_label`、`create.confirm_placeholder`、`create.hint`。
> **v2.7（D-63）**：`create.acknowledge` 一条整串拆成 `acknowledge_prefix` / `disclaimer_link` / `acknowledge_middle` / `terms_link` 四键（旧键已删）。**两个链接严禁写进 `<label>` 内部**——label 的隐式关联会让点击冒泡成 toggle，用户点「用户协议」顺手把勾选取消掉。详见 §5.5。

### 5.3 主界面外壳 `MainLayout.svelte` / `FolderSidebar.svelte`

- 侧栏 240px，底色 `--pms-bg-2`，右侧 1px `--pms-border`。
- 品牌区：小号 logo 方块 + "PasswordMS"（1rem / 600）。
- 主导航项：40px 高，圆角 `--pms-r-sm`；**选中态 = `--pms-accent-soft` 底 + `--pms-accent` 文字 + 左侧 2px 青绿竖条**；悬停 = `--pms-bg-2`。
- 底部分区：导入 / 锁定，用分隔线隔开。
- **（#531，v2.16）侧栏拖拽与钉住**（完整语义见 §5.4.2）：
  - 「全部条目」行 `data-drop-id="__all__"`：**不可拖**、固定第一，但是条目行的**跨分组投放目标**（拖条目到它 = 移出分组，`move_item_to_folder(null)`）。
  - 分组行：可拖拽重排（D-84，v2.17：未钉分组之间拖动 = 一并收编进钉住块，与条目同语义）；既是分组行的投放目标（分组钉住排序），也是条目行的投放目标（拖条目进分组 = `move_item_to_folder`）。
  - **（D-85，v2.18）点钉 = 直接取消钉住**：钉住图标是可点 `button`（`side-pin`，行按钮的**兄弟节点**、与 `.side-del` 同位型 26px——嵌套 button 无效 HTML；兄弟节点天然不触发行点击/拖拽/双击改名），点击 `setFolderPin(false)`，hover 变亮；title/aria 用 `menu.unpin`。条目侧同款（`ItemList.svelte` 的 `list-pin`，`li` 相对定位 + 行 `has-pin` 右内边距让位收藏星）。
  - **（D-85，v2.18）分组重命名**：右键/长按菜单新增「重命名」（`menu.rename`）→ `Modal`（`folder.rename_title`）+ `pms-input` 预填当前组名 + 保存按钮（`folder.rename_confirm`）；**空名/纯空白前端先挡**（不发 IPC）；提交走 `renameFolder()` = `api.folders.update(id, name, 现有icon, 现有color)` + `vault.load()`，失败按现有 `toAppError` toast 惯例；**icon/color 必须原样回传**（`update_folder` 整字段覆盖，None=清除；`vault.updateFolder` 传 null/null 不能用）；Esc/遮罩/取消不改动。**双击改名保留**，且与菜单改名共用同一条 `renameFolder()` 路径（守门：`d85-ui-guard.test.ts` 5 例，含「不得调 `vault.updateFolder(`」负向断言）。
  - 分组行右键 / 触屏长按弹上下文菜单：钉住 / 取消钉住。
  - 投放位置指示：行上缘 / 下缘 2px `--pms-accent` 指示线（`drop-above` / `drop-below` 类）。
  - 拖拽进行中渲染浮动 drag-chip（跟随指针，`pointer-events: none`，否则挡住 `elementFromPoint` 探测）。
- 顶部工具条（`.topbar`，`background: var(--pms-surface)`，`min-height: 56px`）：
  - **左**：搜索框（`.pms-input`，带 search 图标）+ "新建"主按钮。
  - **右**（`.topbar-right`，按此顺序）：**「生成密码」ghost 小按钮（骰子图标，D-30/D-33 客户反馈新增）** → 主题切换 `ThemeToggle` → 语言切换 → 导入 → 锁定。
  - 判据：**顶栏右区必须能一眼看到"生成密码"**。它不带 `onuse`，只生成 + 复制，绝不写回任何条目。

### 5.4 其余面板（同一套语言，逐条过）

| 文件 | 要点 |
|---|---|
| `ItemList.svelte` | 行高 56px；标题 600、副行 `.pms-meta`；悬停 `--pms-bg-2`；选中左侧 2px 青绿竖条；空态 = 居中图标 + 文案。**列表头两入口**：「新建登录」（主按钮）+「新建笔记」（ghost）。**（v2.5 D-60 回退 v2.4 的第三按钮）**：附件入口移入详情页附件区（附件是父条目附属内容，不再有独立附件条目）。**（#531，v2.16）行支持拖拽排序、钉住标记与上下文菜单**（钉住的行带 pin 图标）；桌面鼠标按住 6px 即拖、触屏长按 220ms 武装；右键 / 长按弹菜单（钉住 / 取消钉住 / 移动到分组）；完整语义见 §5.4.2。**（D-85，v2.18）钉住图标可点 = 直接取消钉住**（行按钮的兄弟节点，不嵌套 button；`li.has-pin` 相对定位 + 行右内边距 2.3rem 让位）。**（D-101，v2.27）搜索框一键清空**：词非空时 input 尾部渲染 22px 圆形 ❌ 按钮（既有 `close` 图标，`aria-label`/`title` 双通道），hover 红 icon + `--pms-danger-soft` 淡底；空态不渲染不占位。**点击必须先取消挂着的 220ms 防抖**（否则旧词随后 `setQuery` 写回），再 `query=''` + `setQuery('')`；不推进 `queryEpoch`（epoch 只属 D-81 外部重置路径）。守门：`d101-search-clear.test.ts` |
| `ItemDetail.svelte` | 字段用"标签 + 值 + 行尾复制按钮"三段式；密码值用 `--pms-mono`；复制按钮是 `.pms-icon-btn`。**D-60：登录/笔记条目详情底部渲染附件区**（`<AttachmentView {item} />`，仅 `login`/`secure_note` 两类父条目；遗留独立附件条目经解锁迁移后不可达）。**（D-88，v2.21）工具条第 4 个 icon 按钮「转为安全笔记」**（`item.convert_note`，file 图标，仅登录项渲染、位于编辑与删除之间）：二次确认（`item.convert_note_title/body`，明确「原字段并入正文尾部、不能自动撤销」）→ **一次 `vault.updateItem` 整只替换 kind**（后端 `repo.update_item` 允许 `item.kind = draft.kind`，item_id 不变 → 附件/收藏/分组/同步历史天然延续）；**A1 映射**：notes → 正文，url/username/password/totp/自定义字段逐行拼进正文尾部（`item.convert_fields_header` 分隔），一样不丢；成功 toast `item.convert_done`。守门：`d88-d90-ui-guard.test.ts`。**（D-97，v2.24）动态口令行错误态**：`totpSecret` 非空才渲染本行；取码失败 → `totpInvalid` 置位（toast 只弹一次）→ 行内渲染 `err.totp_invalid_detail` 红字、**停显倒计时与验证码**、停止取码循环（`if (cancelled || totpInvalid) return`）；切换条目复位。错误必须是可见的一行字——不渲染 = 把失败伪装成「本来就没有」 |
| `ItemEditor.svelte` | 全部换成 `.pms-field` / `.pms-input-group`；自定义字段区用 .pms-card 内嵌浅底块；**底部操作条 = flex 固定底栏（v2.22 D-93 起，不再是 sticky）**：字段区包进 `.editor-scroll`（`flex:1; min-height:0; overflow:auto`，padding 归滚动层），`.editor-actions` 移出滚动流（`flex-shrink:0` + border-top + 不透明背景）——sticky 的吸附范围被包含块限制，最低只能贴到内容盒底，其下 padding 空带露出、滚过的字段（分组下拉）从带里透出；内容不足一屏时工具条还停在内容末尾与容器底之间留白。**（D-81，v2.15）新建时分组预填当前选中分组**：`item?.folder_id ?? vault.selectedFolderId` —— 原来恒为 `null`，条目存进根目录，选中分组时保存即「消失」；编辑已有条目路径不变。**（D-89，v2.21）登录表单补「备注」textarea**（`#ed-notes`，绑定**早已存在**的 `notes` 状态——它一直在 save 载荷里，只是表单从未有输入控件，导入的备注「详情可见、编辑器消失、改不了」）；置于 TOTP 之后、自定义字段之前。**（D-92，v2.22）编辑表单桌面端撑满右栏**：`.pane` 桌面端 `display:contents`，`form.editor` 直接是 `.body`（flex row）的 item——旧版只在移动端媒体查询给 `flex:1`，桌面宽度按内容收缩成窄列（笔记与登录共用同一表单，同病）；现 `flex:1; min-width:0; min-height:0` 桌面/移动统一。守门：`d92-editor-layout.test.ts`。**（D-97，v2.24）TOTP 字段入库前真校验**：占位符「BASE32 或 otpauth://」；保存时先 `normalizeTotpInput`（otpauth:// 链接自动抽 secret），再 `totp_now` 试算，`ERR_VALIDATION` → 行内红字 `err.totp_invalid`（点名合法形态 A-Z / 2-7）+ `aria-invalid` + 聚焦字段 + **阻止保存**（环境类错误放行）；草稿取校验后的值。守门：`d97-totp-guard.test.ts` |
| `AttachmentView.svelte` | **D-60 改造为父条目附件区**（不再是独立附件条目的整页详情）：区头 = `paperclip` 图标 + 区标题（`attach.section_title`）+「添加附件」ghost 按钮（`attach.add`，常驻可点——空附件数组时行列表隐藏但入口保留）；附件行 = `--pms-bg-2` 浅底圆角卡（图标 + 文件名 600 + `MIME · 大小` meta 行 + 行尾两个 `.pms-icon-btn` 导出/移除）。**待同步态（D-53 语义保留）**：`attachment_blob_ready(attachment_id) = false` 时行内展示 `attach.pending_sync`（`--pms-warning` 色）并禁用导出按钮（强导会报 `ERR_BLOB_MISSING`）。导出保留明文风险二次确认（`attach.export_confirm_*`）；**导出的选目标与写盘走 `runSaveToDestination`（D-71/D-72）——取消安静返回、失败才弹红**，Android 的 `content://` 由后端经 provider fd 写入（详见 §5.8）；移除走二次确认（`attach.remove_confirm_*`，确认体带文件名插值）→ `remove_attachment` → blob 交孤儿 GC。添加/移除成功后 `vault.load()` 刷新（父条目 rev+1） |
| `ImportWizard.svelte` | 三步走：①「选择文件」+ 路径回显 ②格式单选（`json` / `csv`）③「解析预览」。**格式选择器下方必须有格式说明，不能让用户点完解析才知道文件不合法（D-72）**：`.pms-meta` 一行写支持范围（`import.supported_hint`）、`.pms-hint` 一行写细则（`import.template_hint`：CSV 列清单与 `type` 取值），两行都**常显**、不随格式切换隐藏。下一行是 `.pms-btn.is-ghost` 的「下载 CSV 模板」按钮（`download` 图标 + `import.download_template`）→ 系统保存对话框 → `save_import_template` 写盘 → success toast `import.template_saved`。**取消/失败的判定见 §5.8.2**（取消必须安静，失败必须红）。CSV 解析后展示映射表：已知列用 `.pms-badge.is-accent` 芯片、扩展列用 `.pms-badge-muted`。**（D-86，v2.19）Modal 归属本组件 + 确认导入常驻 footer**：旧版确认按钮沉在弹窗内容最底部、被 85vh 滚动区裁掉 = 主人「找不到确认按钮、导入走不完」（P0）。改为 ImportWizard 自持 `<Modal open onclose>`（Settings 只传 `open={importOpen}` + `onclose`），「确认导入」（`import.apply`，`is-primary`、`disabled={busy}`）放 **footer snippet** 且**按 preview 状态条件传入**——`footer={preview !== null ? applyFooter : undefined}`，未预览时整个 footer 不渲染（保持「先预览后导入」流程心智，不放 disabled 按钮）。**选完文件自动预览**：`choose()` 在取消早退之后 `await doPreview()`（D-73 与本次 P0 都卡在「还要手动点预览」）；格式手动变更只清 preview（`onchange`，旧 token 跨格式不可复用）不自动预览，「预览」按钮保留。组件常驻挂载后每次打开**显式重置**向导状态（原内容随 Modal 卸载天然清空，归属变更后必须补）；导入成功 `onclose()` 随 footer 一起关弹窗。**（D-87，v2.20）编码提示常驻**：`.pms-meta` 一行 `import.encoding_hint`（Excel/WPS 编辑后建议另存「CSV UTF-8」；GBK/UTF-16 自动识别转换），置于细则行之后、模板按钮之前——后端已容错，此行防止用户另存 ANSI 后误以为应用坏了。守门：`import-wizard-ui.test.ts`（5 例） |
| `Settings.svelte` | 分组标题 1rem/600 + 分组卡；每行 label 左、控件右；危险操作（改主密码、导出）用 `.pms-alert` 提示。**「导出保险库」卡片含当前主密码输入框**（D-51：后端导出用它做 `unlock_check`，为空时前端不发 IPC、直接提示 `settings.export_password_required`）。**导出的是整库的加密副本而非明文（D-54）**，且**文件的修改时间 = 库数据的最后写入时间**（`fs::copy` 保留 LastWriteTime）；备份可在锁屏页用「从备份恢复」（D-55）导入。**自动锁定档位真持久化（D-61 A 案）**：`auto_lock_minutes` 经 `get_auto_lock`/`set_auto_lock` 读写库明文头部（初始化读 IPC，换档即写 + success toast `settings.auto_lock_updated`，失败 error toast 并回滚 select 显示值）；执行层为全局闲置计时——`pointerdown`/`keydown`（passive）重置闲置起点，闲置达档位即走既有 `lockNow()`（reset 三个 store + 切锁屏路由）；Android 后台冻结计时器由 `visibilitychange` 补判（回前台超档立即锁，未超档后台时间计入闲置重排剩余）；无「从不」档；纯逻辑层 `src/lib/auto-lock.ts` 依赖注入可测（假时钟 8 例）。**「关于」分组（D-63 v2.7）**：末尾追加独立卡片（`settings.group_about`），四行 = 产品名 + 版本 / 作者 / 许可证 / 三按钮（项目主页 + 免责声明 + 用户协议，均为 `is-ghost is-sm`）；版本经 `getVersion()` **运行时读取，禁硬编码**，读取失败显示 `—`。**旁路数据失败隔离（D-73 v2.12）**：三块旁路数据（库文件信息 / 自动锁定档位 / 版本号）一律经 `src/lib/settings-page.ts::loadSettingsPageSide` **逐块独立失败隔离**（任一失败不影响其余两项；失败态显式建模，不再被 `.catch(() => null)` 吞成空白）；`draft` 深拷**必须**用 `$state.snapshot()`，**禁止 `structuredClone`**（对 `$state` 代理必抛 `DataCloneError`，且抛在 `$effect` 首行会让其后逻辑全部不执行；全仓由 `src/lib/state-clone-guard.test.ts` 守门），也**不得**用浅拷贝 `{ ...value }`（`generator` / `health` 两层仍是代理，`bind:value` 会写穿 store）；「保险库文件」卡片**有说明行** `settings.vault_file_hint`，读不到时显示 `settings.vault_file_failed` 而**不是**空白卡片。详见 §5.5 |
| `PasswordGenerator.svelte` / `PasswordGeneratorModal.svelte` | 结果用 `--pms-mono` 大字 + `.pms-input-group` 承载复制/刷新按钮；选项区两列网格。**一律以弹窗形态出现**（`PasswordGeneratorModal` 内 Modal 自带 `{#if open}` 包裹 → 关闭即卸载，不残留请求/toast）。位数用**数字输入框**（输入中不钳制，失焦/回车才 clamp 到 8–64）；四类字符复选；**一类都不选时生成按钮 `disabled` 并给出提示**；结果下方带 `StrengthMeter`。**生成器参数（位数 / 四类字符 / 排除易混 / 自选符号集）变更后防抖 300ms 静默持久化**（D-52：走 `SettingsStore.saveQuiet` 不弹 toast，用显式变更点触发而非 `$effect` → 初始化不会把未变更默认值写回）；初始值优先级 `initial`（调用方显式传入）> 设置里持久化的偏好 > 默认值，`symbol_set` 空时回退 `+-_.,:` |
| `common/ThemeToggle.svelte` | 三态循环（系统 → 浅色 → 暗色），图标用 `sun` / `moon`，并带 `title`。切换时**同时**更新 `document.documentElement` 的 `data-theme` **和**原生窗口标题栏（见 §4 D-32）。挂载点：解锁页右上角、主界面顶栏右区。 |
| `HealthCheck.svelte` | 健康分做大字环形/大数字 + 4 个统计卡（弱口令/重复/超期/总数），弱项用 `.pms-badge.is-danger`。**（D-99，v2.26）重复密码按组渲染**：每组一个小框（`health-group`，边框圆角）+ `health.reused_group`「第 {n} 组」标签 + 组内 chip；标题映射用体检时并行拉取的**全量**条目（`titleMap`），不得用 `vault.items`（筛选后列表，视图外条目退化为裸 UUID） |
| `Trash.svelte` | 与 ItemList 同构；到期时间用 `.pms-badge.is-warning` |
| `sync/*.svelte` | 4 个 tab 统一；配对二维码容器加 `.pms-card` 与留白；设备行 = 图标 + 名称（**可重命名，行内编辑，D-66**）+ **指纹**（= `DeviceDto.fingerprint`，16 位小写 hex；**不是** `device_id`，D-67）+ **配对时间** + 上次同步 + 铅笔改名 `.pms-icon-btn` + 解绑 ghost 按钮。**初始标签按配对状态（D-58b）**：主菜单进入「同步」时，若用户从未手动点过任何 tab，则已有配对设备（`sync.devices.length > 0`）优先显示「状态」，无配对设备优先显示「配对」；用户一旦手动点过任何 tab 即永久尊重其选择（`syncTabTouched`），不再自动调整；不做持久化。**「立即同步」忙碌态按设备隔离（D-66）**：占用态是 `sync.syncingDeviceId`（单个设备 id，不是 store 级布尔）——被点的那台显示 `<Spinner>` + `sync.syncing`，其余台仅 `disabled` **不转圈**，期间**全部**不可点（禁止并发重同步）；判定收敛在纯函数 `syncButtonState()`（`src/lib/device-label.ts`，5 例单测守门）。设备行内改名的硬约束见 §5.6，指纹取值规则见 §5.6.3。**配对两屏显示会话码（D-68 立项 / D-69 修正口径）**：电脑屏在二维码/配对文本下方显示 `PairingDto.session_code`（本次配对核对码）+ `sync.session_code_hint`，手机屏在加入表单下方显示 `sync.lastPairSessionCode`（电脑的会话码）+ 对端设备名 + 同一句提示；判定走纯函数 `hasSessionCode()`，无值则**整块不渲染**（不显示空标签）。**会话码每次配对都变，不是设备指纹**；方向硬约束与样式约束见 §5.7 |
| `common/Modal.svelte` / `ConfirmDialog.svelte` | 遮罩 `rgba(0,0,0,.55)` + 轻微模糊；卡片 `--pms-r-lg` + `--pms-shadow`；确认按钮危险态用 `is-danger` |
| `common/Toast.svelte` | 右下角；`--pms-r`；左侧 3px 状态色竖条。**时长自适应（D-58a）**：success 3000ms；info 5000ms；error 按当前语言渲染后文案长度自适应 `min(12000, max(6000, 字数 × 150))`（6s 起步、每字 150ms、12s 封顶）——长错误文案必须给足阅读时间。关闭按钮不变 |
| `common/StrengthMeter.svelte` | 4 段；弱=红 / 一般=橙 / 强=青绿 / 很强=青绿加粗；带文字标签。**空态（未输入）不要"压暗到看不见"，也不要让空 label 占位**：轨道用中性可见色 `--pms-track`，并 `.meter[data-score='0'] .label { display:none }` 让 4 段占满整行。<br>（修正 v1.1：原写法实测暗色下条带与卡片 RGB 差仅 24，肉眼不可见，且右侧凭空多 47px 留白。）<br>**（D-100，v2.25）评分 = 信息熵**：`scoreOf` 抽到 `lib/strength.ts`（`bits = len × log2(池)`，池 26/26/10/33；阈值 40/60/80）——旧「类数+长度加成」公式 4 类全开恒封顶，生成器拖长度质量不变（主人实测）。熵假设均匀随机，对人类口令会高估，仅提示不强制（U6）。守门：`d100-strength.test.ts` |
| `common/Spinner.svelte` | 用 `currentColor`，随按钮文字色走 |

#### 5.4.1 列表过滤器与保存可见性（D-81，v2.15）

> 症状：选中某分组 → 新建登录/笔记 → 保存 → **编辑器关闭、列表没有、详情区空白**，条目「消失」。
> 实际没丢——存进了根目录，是**两层叠加**：编辑器不预填分组（层①）+ 过滤视图吞掉刚存条目（层②）。

**硬保证（store 层，`vault.svelte.ts`）**：**保存成功的条目必须立刻可见且选中**。

- `createItem` / `updateItem` 成功后先用保存前的过滤器 `load()` 一次；若条目**不在**返回列表里
  （分组 / query / favoriteOnly 任一藏了它），按纯函数 `src/lib/save-visibility.ts::revealFilterAfterSave`
  给出的回落过滤器重置并**再拉一次**，然后置 `selectedItemId` 为刚存条目。
- 回落**只清藏了它的那几位**（组不匹配清组、query 不匹配清 query、收藏不匹配清收藏），
  其余过滤位保留 —— 用户不该因为一次保存丢掉无关的过滤状态。
- 三个过滤位按字段判断全都匹配、条目却不在列表里（状态不一致 / 陈旧）→ **保守回落全部条目**。
- 搜索匹配语义与后端 `Repository::list_items`（`item_matches`）**逐字对齐**：needle =
  `query.trim().toLowerCase()`，匹配 title / login.username / login.url，子串包含、大小写不敏感；
  **后端语义一变，`matchesSavedItemQuery` 必须跟着变**，否则判定会说「可见」而列表里没有。
- **搜索框显示 = 实际过滤**（验收项）：回落会清空 store 的 `query`，`ItemList` 的搜索框是
  本地 state + 220ms 防抖，靠 `vault.queryEpoch`（外部重置时 +1；用户打字走 `setQuery` 不加）
  区分「自己的防抖延迟」与「外部重置」——前者不动输入框（否则每个按键被清掉），
  后者**取消挂着的防抖并同步显示**，防止迟到的 `setQuery` 把刚清掉的过滤原样写回。
- 判据见 `src/lib/save-visibility.test.ts`（9 例：五种藏匿情形 + 三个边界）与
  `src/lib/stores/vault.test.ts`（7 例：回落 / 原样 / 只清一位 / epoch 信号）。

#### 5.4.2 拖拽排序与频度置顶（#531，v2.16；D-84 修订，v2.17）

> 客户拍板的排序模型：**钉住块 → 未钉按频度**。收藏不再置顶（旧「收藏第一」语义作废）。
> **D-84 修订（v2.17，真机走查反馈）**：**未钉区也可手动排序**——拖 A 到 B 上把 B 一并收编进钉住块（原「未钉拖未钉 = 无动作」是交互死锁：主人库一个钉住的都没有，未钉之间永远拖不动）。

**排序语义（后端裁决序，前端不自行排序）**：

- 列表序 = 钉住条目块（按 `pin_order` 升序）+ 未钉条目块（`use_count` DESC → `last_used_at` DESC → `updated_at` DESC）。从未被拖过的条目继续按频度自动排（钉住块之后），不变。
- **频度由复制动作累积**：详情页复制密码 / 用户名 / TOTP 成功后调 `record_item_usage`（`ItemDetail.svelte` 的 `copyText` 成功路径），后端 `use_count + 1` 并刷新 `last_used_at`。看条目不算用，复制才算。
- **（D-84）拖 A 到 B 上 = B 与 A 一并收编进钉住块**，A 按插入方向排在 B 前/后（before → [...钉住块, A, B]；after → [...钉住块, B, A]）——视觉与拖放位置一致（两者相邻），顺序持久。覆盖四种组合：都未钉 / A 已钉 B 未钉（B 收编）/ A 未钉 B 已钉（既有行为）/ 都已钉（纯重排，不变）。分组侧完全同理。
- **（D-84）拖到空白处/非行目标（targetId=null）**：已钉 A = 挪到钉住块末尾；未钉 A = 无动作。拖回原位 / 拖到自己 = 零 IPC；目标已在钉住集合不重复收编。结算纯函数 `dnd.ts::pinnedOrderAfterDrop`（23 例单测，其中 D-84 收编语义 6 例）。
- **（D-84）松手结算三连（全部 await 串行）**：新收编目标先 `set_*_pin(true)` → 被拖者按需 `set_*_pin(true)` → `reorder_pinned_*(新序)`。串行保证 reorder 时后端 pinned 集合已含两者（后端整体校验「id 必须已钉住」，否则 `ERR_VALIDATION`）；order 数组原样下发（已含新收编目标），无需预过滤。后端模型 / IPC 契约零改动。
- 钉住的条目带 pin 图标标记；分组侧栏同理（分组 `use_count` 为组内聚合值）。
- **「全部条目」固定第一、不可拖**，仅作为条目的跨分组投放目标（§5.3）。
- 后端字段未落地时组件以 `pinned === true` 守卫读 DTO 新字段（防旧帧 `undefined` 误发命令）。

**拖拽基建（自写 pointer 事件，零第三方库）**：

- 为什么不用 HTML5 DnD：Android WebView 上 `dragstart` / `drop` 不可靠且样式不可控 → 全自写。
- `src/lib/dnd.ts` 纯逻辑（可 node 单测）：`DRAG_START_THRESHOLD_PX = 6`、`LONG_PRESS_MS = 220`、`exceededDragThreshold` / `isLongPress` / `insertBefore` / `computeDropTarget` / `autoScrollDelta` / `pinnedOrderAfterDrop`（23 例单测，含 D-84 收编语义）。
- `src/lib/dnd.svelte.ts` 状态与控制器（`$state` 只能在 `.svelte.ts`）：`drag` 单例 store（active / kind / draggedId / label / x / y / overKind / overId / overBefore）+ `beginRowDrag(kind, id, event, callbacks)` 控制器 + `finishDrag` 结算路径（D-83：**先 onDrop 后 end()**，时序由 `dnd-settle.test.ts` 3 例钉死）。
- **桌面**：鼠标按下、位移超 6px 即进入拖拽。**触屏**：按下后长按 220ms 武装；武装前位移超 6px = 取消（当作滚动）；武装后位移超 6px = 开始拖。
- 拖拽激活后挂 non-passive `touchmove` 并 `preventDefault` **压住页面滚动**；列表容器边缘 48px 内由 `autoScrollDelta` 驱动 rAF 自动滚动，底部预留区参数计入移动端底部导航 56px（§6.12）。
- **跨组件投放解耦**：发起组件在 `pointermove` 里用 `elementFromPoint` + `closest('[data-drop-id]')` 探测目标（浮动 chip 必须 `pointer-events: none`），目标组件不重复挂监听、不互相 import；结算回调由发起方持有——条目拖放的结算在 `ItemList.handleItemDrop`（含进分组 / 移出分组 / 钉住序），分组拖放的结算在 `FolderSidebar.handleFolderDrop`。
- 拖后抑制 click：拖拽结束的同一指针序列里抑制行点击（`clickSuppressed`），避免松手误开详情。

**上下文菜单**：

- 行右键（桌面）/ 触屏长按（`isLongPress`，220ms 未超位移，长按后震感由系统处理）弹 `ContextMenu.svelte`。
- 条目菜单：钉住 / 取消钉住 / 移动到分组（后者打开 `Modal` 移动分组弹层，含「未分组」项）。
- 分组菜单（`FolderSidebar`）：钉住 / 取消钉住。
- 菜单打开后拖拽状态必须先结束；长按菜单的落点用拖拽控制器记录的 lastX/lastY（长按场景浏览器不派发后续坐标事件）。

**守门**：`dnd.test.ts`（23 例，含 D-84 收编语义）、`dnd-settle.test.ts`（3 例，D-83 结算时序）、`vault.test.ts` 扩到 13 例（6 命令透传 + 失败不抛不 reload）、`ipc-contract.test.ts` 六命令双侧在册锚点（`rename_all` 计数 53 → 59，后端 `ordering_cmd.rs` 已落地）、`usage-tracking.test.ts`（5 例，复制成功路径埋点源码断言：ItemDetail 三出口 + SecureNoteView 正文第四出口）。

### 5.5 「关于」分组与法律条款弹窗（D-63，v2.7）

> 需求来源（客户原话）：「我已知晓并同意免责申明和用户协议，免责申明和用户协议应该可以点击进去，并有实质内容。」「请添加关于内容，否则不知道软件的版本是多少，作者是谁。」
> 两个文件：`src/components/LegalDialog.svelte`（弹窗）、`src/lib/legal-content.ts`（正文数据源）。正文**不进 i18n**——它是长文，混进 key 表会污染键位校验与翻译流程。

#### 5.5.1 勾选行：链接与 checkbox 必须互为兄弟节点（硬约束）

```
.wz-ack                     /* display:flex; flex-wrap:wrap; align-items:center; gap:.25rem */
  label.pms-check           /* ← 只包 input[type=checkbox] + create.acknowledge_prefix */
    input[type=checkbox]
    span                    /* 「我已阅读并同意」 */
  button.pms-link           /* 「免责声明」 create.disclaimer_link  ← 兄弟节点 */
  span.wz-ack-mid           /* 「与」      create.acknowledge_middle */
  button.pms-link           /* 「用户协议」 create.terms_link      ← 兄弟节点 */
```

**为什么不能把 `<button>` 塞进 `<label>`**：`<label>` 对内部任意可点元素的点击都会**隐式转发**给关联控件（HTML 规范行为，不是 bug）。链接一旦进了 label，点击就会冒泡成「勾选/取消勾选」——用户在同意页点「用户协议」，弹窗打开的同时勾选被取消，关掉弹窗才发现按钮变灰。绕过方式只有 `preventDefault() + stopPropagation()` 这种「靠人记得写」的写法；**拆成兄弟节点是从结构上消灭这个 bug**，任何后续改动都复现不出来。

- 尺寸：`.pms-link` 与 `.pms-check` 同为 `0.86rem`，视觉上是一条完整句子；`text-underline-offset: 2px` 防止下划线贴中文字脚。
- 触控：行内 `padding: 0`，靠 `.wz-ack` 的 `gap:.25rem` 与 `.pms-check` 的 `gap:.55rem` 留白；移动端 `<768px` 由 `flex-wrap` 自然折行，**不要**给 `.pms-link` 加 `min-height:44px`（会把一行句子撑成大块，视觉上不再是「一句话」）。
- 门禁不变：`acknowledged`、`canSubmit`、`MIN_MASTER_PASSWORD_LEN` 一个字都不动；链接回调 `openLegal(kind)` **只写弹窗状态**（`legalOpen` / `legalKind`），**从不写 `acknowledged`**。

#### 5.5.2 法律条款弹窗 `LegalDialog.svelte`

| 项 | 规范 |
|---|---|
| 承载 | 复用 `common/Modal.svelte`（遮罩 `rgba(0,0,0,.55)` + 模糊，卡片 `--pms-r-lg` + `--pms-shadow`，宽度 `min(560px,100%)`） |
| props | `open`（`$bindable(false)`，父组件 `bind:open`）+ `kind: 'disclaimer' \| 'terms'`。**不传 open 时默认 false**，避免误挂载 |
| 标题 | `modal titleKey` = `legal.disclaimer_title` / `legal.terms_title`；同时作为 `aria-label` |
| 语言 | `$derived`：`ui.locale === 'zh-CN' ? 'zh' : ui.locale === 'en-US' ? 'en' : ui.locale`（zh-TW→zh-TW、th→th 直通），**跟随 UI 语言即时切换**（不缓存、不落盘） |
| 滚动区 | `.legal { max-height: min(60vh, 420px); overflow-y: auto; padding-right: .4rem }` —— `padding-right` 是给滚动条留槽位，避免长短文档切换时正文横向抖动 |
| 排版 | `legal.updated` meta 行（`.pms-meta`）置顶 → 每节 `h4`（0.9rem / 600 / `--pms-fg`，编号 `N.` 由渲染时拼）→ 每段 `p`（0.85rem / line-height 1.65 / `--pms-muted`）。节间距 `h4 { margin: 1rem 0 .35rem }`，首节 `margin-top: .5rem` |
| 底部 | `{#snippet footer()}` 单个 `pms-btn.is-primary`「我知道了」（`legal.understood`）→ `open = false`；右上角 × 与点遮罩同样关闭 |
| 正文数据 | `src/lib/legal-content.ts`：免责 6 条 / 用户协议 8 条，每条 1–2 段实质内容（生效日期等条目可为 1 段）；`LEGAL_UPDATED = '2026-10-20'` 八份文档（4 语言 × 2 篇）统一；导出 `getDisclaimer(lang)` / `getTerms(lang)`，`LegalLang = 'zh' \| 'zh-TW' \| 'en' \| 'th'`（D-102，四语言全真翻，A 案主人拍板 2026-10-26） |

**样式落点**：`.legal*` 三条规则写在 `LegalDialog.svelte` 的 `<style>` 内（组件私有，不污染 `global.css`）；只有 `.pms-link` 进 `global.css`（跨组件共享类，见 §3）。

#### 5.5.3 Settings「关于」分组

```
div.pms-card.set-card
  h3                              settings.group_about
  .set-row.is-info  span.set-name  "PasswordMS"   +   span.pms-meta  "版本: {appVersion ?? '—'}"
  .set-row.is-info  span.set-label 作者    +   span.pms-meta  "hengletime"
  .set-row.is-info  span.set-label 许可证  +   span.pms-meta  "MIT License"
  .set-row.is-actions  button.pms-btn.is-ghost.is-sm ×3
              ① Icon(name="external",14) + settings.about_homepage
              ② settings.about_disclaimer
              ③ settings.about_terms
```

- **行类型必须显式标注**（D-73，v2.12）：前三行是纯信息行 → `.is-info`；末行是按钮行 → `.is-actions`。
  漏标就会被窄屏的 `.set-row { flex-direction: column }` 一并堆叠（信息行左右拆散、按钮拉成满宽竖排）。详见 **§6.10**。
- **版本禁止硬编码**：`getVersion()`（`@tauri-apps/api/app`）在 `$effect` 里读一次，失败置 `null` → 模板渲染 `appVersion ?? '—'`。写死 `1.2.2` 会在下个版本立刻变成假信息。
- **项目主页**：常量 `HOMEPAGE_URL = 'https://github.com/hengletime/PasswordMS'`，点击前先过 `isSafeUrl()`（复用 `src/lib/url-safety.ts`，与条目 URL 同一套白名单），再走既有 `api.system.openUrl`。**不要**引 `@tauri-apps/plugin-opener` 新依赖——已有 IPC 够了。
- **图标**：主页按钮用 `external`（Icon 里没有 `link`，写错名会静默不渲染）。按钮尺寸 `is-ghost is-sm`，与卡片内其它行同高，不用主色，避免「关于」抢走安全设置的注意力。
- 这一组**必须排在最后**（在「保险库文件」卡之后）：关于信息是低频内容，不给它抢首屏。

### 5.6 设备行内改名与同步忙碌态隔离（D-66 / D-67，v2.9）

> 需求来源（主人真机反馈）：「设备」页两台手机都显示 `PasswordMS-3dd4390a` 这类名字，**分不清哪台是哪台**；
> 点其中一台「立即同步」，**两台同时显示正在同步**，看不出在等哪台。

#### 5.6.1 忙碌态按设备隔离（硬约束）

```
同步 store
  syncingDeviceId: string | null     ← **唯一**占用态真相（D-66 新增）
  busy: boolean                      ← 语义收窄为「**配对流程**进行中」（PairScan / PairShow 用）
```

- **禁止**用 store 级单一布尔控制 `{#each}` 里的每行按钮 —— 那正是本缺陷成因：一台在同步，
  所有行都命中 `{#if sync.busy}`，两台手机一起转圈。
- 每行状态由纯函数解算，**不在模板里写判定表达式**：
  ```ts
  // src/lib/device-label.ts
  syncButtonState(syncingDeviceId, deviceId): { syncing: boolean; disabled: boolean }
  //   全空闲                → { syncing: false, disabled: false }
  //   命中那台              → { syncing: true,  disabled: true  }
  //   未命中但整体忙碌      → { syncing: false, disabled: true  }   ← 只置灰，不转圈
  ```
- `syncNow(deviceId)`：**不再碰 `busy`**；进函数先 `if (syncingDeviceId !== null) return;` 防重入，
  然后 `syncingDeviceId = deviceId`，`finally` 置回 `null`。`reset()`（锁定）必须一并置 `null`，
  否则解锁后所有同步按钮永久置灰。
- 空字符串 `deviceId` **永不判为命中**（空 id 不是合法设备，跟着转圈等于把「查不到设备」
  伪装成「正在同步」）。

#### 5.6.2 行内改名（交互硬约束）

```
li.sync-device
  span.sync-device-icon        Icon(name="device")
  div.sync-device-meta         纵向
    {#if 编辑态}
      input.pms-input           ← 初值 = 当前名，挂载即聚焦并全选（`use:` action）
      div.sync-rename-actions   保存 is-primary is-sm + 取消 is-ghost is-sm
    {:else}
      span.sync-device-name
    {/if}
    span.sync-device-sub       指纹 fingerprint（.sync-mono，title=device_id）
    span.sync-device-sub       配对时间 paired_at
    span.sync-device-sub       上次同步 last_sync_at
  button.pms-icon-btn          pencil（非编辑态才渲染）
  button.pms-btn.is-ghost.is-sm.sync-danger   解除配对
```

| 约束 | 规则 |
|---|---|
| 编辑态唯一性 | **同时只允许一行**处于编辑态。用 `let editingId = $state<string \| null>(null)` **单一状态源**，禁止再拆一个 `isEditing` 布尔（同 D-31 的教训） |
| 键盘 | `Enter` 保存、`Esc` 取消，两者都 `preventDefault()`（防 Enter 触发表单默认提交 / Esc 冒泡） |
| 空名 | `draft.trim().length === 0` → 保存按钮 `disabled`，且 `saveEdit` 提前返回（不打 IPC） |
| 字节口径 | 数字节**必须在 `trim()` 之后**（模板的 `tooLong` 与 `saveEdit` 的守卫都用 `draft.trim()`），且与后端 `name.trim()` 同序。**两处必须调同一个 trim 方法**：不许一处 `draft.trim()`、另一处 `draft.trimEnd()`，否则「尾部空格算不算进 64 字节」在「提示」和「保存」之间会不一致 —— 提示说不超、保存却被拒（或反过来），用户会以为按钮坏了。反例：拿原始 `draft` 数，「21 个汉字 + 首尾各一空格」= 65 字节会被前端误拦，而后端 trim 后（63 字节）本来收得下 → 前端比后端严，用户会觉得「明明放得下却不让存」 |
| 超长 | 上限 **64 字节**（与 core 的 `MAX_DEVICE_NAME_BYTES` 同口径），**按 UTF-8 字节算**：22 个汉字 = 66 字节即超限。超限时同样 `disabled` + 就地 `.pms-hint`（`--pms-danger`）显示 `sync.rename_too_long`。**光靠 `maxlength` 不够**——它只数 UTF-16 码元，会放过 64 个汉字的名称，让后端回笼统的「输入无效。」。改这个数字要**四处同改**：前端常量 / core 常量 / core 测试 `rename_rejects_over_64_bytes` 的边界构造 / `sync.rename_too_long` 中英文案（其中「约 21 个汉字」= `floor(64/3)`；Emoji 4 字节/个，故文案只说汉字、不说「或 N 个字符」） |
| 提交中 | `saving` 为真时保存/取消按钮都 `disabled`，防重复提交 |
| 输入框样式 | **必须** `class="pms-input"`，**不得**在组件 `<style>` 里重写 `border` / `background` / `font-size`（只允许父级布局类） |
| 按钮文案 | 复用既有 `common.rename`（铅笔 `title` / `aria-label`、输入框 `aria-label`）、`common.save`、`common.cancel`；**不新建重复键** |
| 成功后 | 由 store 的 `loadDevices()` 刷新数据（含后端返回的新名称），组件只负责 `editingId = null` |
| 副行顺序 | 指纹 → **配对时间** → 上次同步（配对时间用 `DeviceDto.paired_at` + `formatDateTime`，为 0 时回退 `common.unknown`） |
| 指纹来源 | 必须显示 `DeviceDto.fingerprint`（16 位小写 hex），**不得**用 `device_id` 冒充；判定走纯函数 `fingerprintText()`，详见 §5.6.3 |
| 昵称语义 | **本地昵称**：只存在本机设备表，不回写对端、不进同步载荷；文案不承诺「会同步到手机」。**这不是约定而是结构保证**——`Repository::export_changes` 的类型白名单只放行 `RECORD_TYPE_ITEM` / `RECORD_TYPE_FOLDER`，`RECORD_TYPE_DEVICE` 等其余记录一律 `continue`（搜 `repository.rs` 里 `export_changes` 函数体第一个 `if`），所以设备名**在结构上不可能**进 wire。同时改名只写 `device_name` 后 `upsert_paired_device`，不调 `touch_device_sync` → `last_sync_at` / `paired_at` / 指纹 / 共享密钥均不变（core 测试 `rename_touches_only_device_name` 钉死）。**引用源码位置一律给符号名不给行号**——本条白名单从 `1259` 漂到 `1528` 只用了一天（D-66 插入 + `cargo fmt` 各贡献一段），行号的保鲜期是「天」级 |

#### 5.6.3 指纹必须是真指纹（D-67，硬约束）

**缺陷回顾**：设备行的「设备指纹」一行原先渲染的是 `device.device_id`（36 位 UUID），而 `DeviceDto.fingerprint` 字段一直存在、却**在 UI 上一处都没露过**（全仓 `sync.fingerprint` 只此一处；旧键 `sync.confirm_fingerprint` 长期零调用，已在 D-68 删除）。

这不只是显示难看 —— 是**安全提示在说谎**：

| | 真指纹 | 被冒充的 `device_id` |
|---|---|---|
| 来源 | `handshake::fingerprint_of()` = `sha256(对端公钥)` 前 8 字节 | 随机 UUID（v4） |
| 形式 | **16 位小写 hex** | 36 位带连字符 |
| 写入点 | 配对握手时写入 `PairedDevice.fingerprint`（主机侧 `server.rs` / 加入侧 `client.rs`） | 配对时生成 |
| 安全含义 | 与对端公钥绑定 → 中间人无法伪造出相同值 | **零**：与公钥无关，中间人隔开两端时用户照 UUID 比对也永远「一致」 |

硬约束：

```
{#if hasSessionCode(device.fingerprint)}
  {ui.t('sync.fingerprint')}: <span class="sync-mono" title={device.device_id}>
    {fingerprintText(device.fingerprint)}
  </span>
{/if}
```

| 约束 | 规则 |
|---|---|
| 取值 | 必须 `DeviceDto.fingerprint`，**禁止**把 `device_id` 当指纹渲染 |
| 判定位置 | 走纯函数 `hasSessionCode()`（非空判定）+ `fingerprintText()`（仅 `trim`）（`src/lib/device-label.ts`），**不在模板里写 `||` / 三元判定** |
| 空值**不渲染** | 指纹为空串 / 纯空白 / `null` / `undefined` → **该行整行不渲染**（不是回退 `device_id`、也不是留空白）。**D-69 改口径**：回退 `device_id` 等于用 UUID 冒充指纹，正是 D-67 定性过的错误。空值不是假设——`PairedDevice.fingerprint` 带 `#[serde(default)]`，老记录读出来就是空串；且**电脑端记录手机设备时现在恒为空**（HELLO 不含手机长期身份公钥，见 §5.7.1 与 backlog D-70） |
| `device_id` 的位置 | 只挂 `<span title={device.device_id}>`，悬停可查（排查用）；**不单独占一行**，副行已够挤且 UUID 对用户零意义 |
| 大小写 | 后端输出即小写 hex，前端**不做**大小写变换（比对的是同一个串，改了反而对不上） |
| 单测 | `device-label.test.ts` 覆盖：正常 / 前后空白 / 空串 / 纯空白 / `null` / `undefined`，且断言**任何输入都不返回 `device_id` 顶替** |

### 5.7 配对**会话码**核对（D-69，v2.9 修正 D-68）

> 缺陷背景（分两层，别混）：
>
> **第一层（D-68 立项理由）**：i18n 里曾躺着一条 `sync.confirm_fingerprint`「请核对两端显示的设备指纹是否一致。」——
> **只叫用户核对、不告诉他核对出问题怎么办**，且**全仓零调用点**。因为配对两屏都不显示指纹：
> 电脑屏（`PairShow`）只出二维码/地址/配对文本，手机屏（`PairScan`）配对成功后只弹一句「配对成功」。
> **核对动作在 UI 上无处可做**，那句文案等于一句没人能执行的嘱咐。（D-67 修的是「设备页不再说谎」，不等于「能核对」。）
>
> **第二层（D-68 首版实现是负收益，D-69 修正）**：首版让电脑屏显示 `identity.fingerprint()`
> （`sha256(长期身份公钥)`，与二维码 `QRPayload.fp` 同源），却让手机屏显示
> `fingerprint_of(&Accept.host_pub)` —— 而 `Accept.host_pub` 是**每次配对重新生成的临时 X25519 公钥**
> （`server.rs` 的 `handle_stream`）。两把不同的密钥 → **两屏必然不等**，用户按提示核对必然看到
> 「不一致」。根因是 `Accept`（`protocol.rs`）里**根本没有长期身份公钥**，手机端拿不到能与电脑屏对齐的值。
> 结论：**显示一个永远不等的值，比不显示更糟**——它会把每台设备都报成中间人。

#### 5.7.1 三个量必须分清（**混用就是缺陷本身**）

| 量 | 数据来源 | 稳定吗 | 用途 | 出现在哪 |
|---|---|---|---|---|
| **会话码** `session_code` | `sha256(host_eph 公钥)` 前 8 字节 hex（16 位小写） | **每次配对都变**（绑本次握手的临时密钥） | **本次**人工核对（中间人检测） | 电脑屏 `PairShow`、手机屏 `PairScan` |
| **设备指纹** `DeviceDto.fingerprint` | `sha256(长期身份公钥)` 前 8 字节 hex（16 位小写） | **稳定不变** | 设备列表辨识「这台是谁」 | 「同步 → 设备」页副行（§5.6.3） |
| `device_id` | 随机 UUID v4 | 稳定 | 排查用（只做 `title` 悬停） | 悬停提示，**永不作为指纹文本渲染** |

会话码**不能**当设备身份用（下次配对就变），设备指纹**不能**当核对码用（它不绑本次握手，
中继式中间人两侧看到的是同一台电脑的同一个长期指纹，比对永远「一致」→ 检测不出任何东西）。

#### 5.7.2 方向硬约束（**禁止反过来**）

| 屏幕 | 显示谁的会话码 | 数据来源 |
|---|---|---|
| 电脑 `PairShow.svelte` | **电脑自己的**（主机侧本次会话） | `PairingDto.session_code`（=`SyncServer::session_code()`） |
| 手机 `PairScan.svelte` | **电脑的**（对端本次会话） | `PairResultDto.session_code`（=`JoinerSession::session_code()`，由 `Accept.host_pub` 算出）→ 落在 `sync.lastPairSessionCode` |

**为什么两屏「必然相同」**（这是核对能成立的根据，不是巧合）：

- 电脑屏的值来自 `SyncServer::start()` 里一次生成的 `host_eph`，`session_code()` 就是 `fingerprint_of(host_eph.public_bytes())`；
- 手机收到的 `Accept.host_pub` **就是同一把 `host_eph` 的公钥**（`server.rs` 的 `handle_stream` 用 `self.host_eph` 组包）；
- 两者输入完全相同的 32 字节 → 输出必然逐字相同。**没有第二把密钥参与**，所以不存在时序差。

临时密钥**从每连接生成前移到 `start()` 一次生成**，正是为了让电脑屏在手机连上之前就能显示这个值
（否则要等握手完成才知道，用户没得比）。配对令牌用后即焚（`used`），一个 `SyncServer` 生命周期内
只允许完成一次配对，因此复用这把临时密钥不引入额外风险。

**中间人为什么能被抓出来**：中继式攻击者必须对两侧分别用自己的临时公钥，于是电脑屏算的是
`fp(真实 host_eph)`、手机屏算的是 `fp(攻击者的公钥)` → 两码不等 → 用户看到不一致即中止。
（反过来说，若两屏显示的是同一个**长期**指纹，中继两侧看到的仍是同一个值，比对必然「一致」——这就是
首版负收益的技术原因。）

前端**不做任何自动比对、也不做任何格式化**（不转大小写、不插分隔空格、不截断）：一旦美化就会让「逐字比对」失效。
折行只用 `word-break: break-all`，它改变的是**渲染换行**、不改变字符内容。

**手机不显示自己的会话码**：电脑的「设备」页已经能看到手机这台设备（§5.6.3 / D-67），
配对屏再显示一遍只会让用户不知道该拿哪个和哪个比 —— 方向一乱，核对本身就失效了。

#### 5.7.3 文案键与判定

| 位置 | 键 | 说明 |
|---|---|---|
| 电脑屏标签 | `sync.session_code` | 「本次配对核对码」/ "Pairing code (this session)" |
| 手机屏标签 | `sync.peer_session_code` | 「电脑上的核对码」/ "Code shown on the computer" |
| 手机屏对端设备名 | 直接渲染 `sync.lastPairDeviceName` | 让用户确认「是哪台」，非 i18n 键 |
| 两屏共同提示 | `sync.session_code_hint` | 含**不一致时的处置**：立即解除配对并重试 |

> 文案纪律：这两处**不得**再出现「设备指纹」字样。会话码每次配对都变，叫成指纹会诱导用户
> 把它记下来当身份凭据（那是 §5.6.3 「设备指纹」的职责，稳定不变）。

显示判定必须走纯函数，**模板里不写 `if (x)` 判定**：

```ts
// src/lib/device-label.ts
hasSessionCode(value: string | null | undefined): boolean
//   string 且 trim() 后非空 → true；null / undefined / 空串 / 纯空白 → false
```

**缺失时整块不渲染**（不是渲染一个空标签）—— 「核对码：」后面留空的危害比不显示更大：
用户会读成「码是空的，那应该没事吧」。这条同时兜住「老后端 `PairingDto` 还没带
`session_code` 字段」的过渡期：拿到 `undefined` → 静默不显示，不会崩也不会显示半个空块。

#### 5.7.4 样式与交互硬约束

| 约束 | 规则 |
|---|---|
| 类名 | 块容器 `.sync-fp`、等宽码 `.sync-mono.sync-fp-code`；标签用 `.pms-label`、说明用 `.pms-meta` / `.pms-hint` |
| 只做布局 | `.sync-fp` 仅 `display:flex; flex-direction:column; gap`；`.sync-fp-code` 仅 `font-family: var(--pms-mono)` + `word-break: break-all`。**不得**自写 `font-size` / `color` / `border` / `background` |
| 断行 | 16 位 hex 在窄屏（手机 390px）必须能整块看到，故必须 `word-break: break-all`，禁止 `text-overflow: ellipsis`（截断后无法逐字核对） |
| 提示色 | 手机屏用 `.pms-hint.is-warning`（`--pms-warning`）—— 这是**要求用户执行的安全动作**，不是普通说明；电脑屏用 `.pms-meta`（中性说明） |
| 位置 | 电脑屏：二维码 + 配对文本 + 有效期之后、取消按钮之前；手机屏：加入表单之后 |
| 生命周期 | `join()` 进门先清 `lastPairSessionCode = null` / `lastPairDeviceName = ''`（防止把「上一台电脑」的码显示成「这一台」的）；`reset()`（锁定）同样清空，不跨解锁残留 |
| 配对完成后 | 配对页的块只服务于**配对当下**的核对；事后再想认设备看「同步 → 设备」页的**设备指纹**（§5.6.3），两者不是同一个量 |

#### 5.7.5 配对面板状态机与「成功态」（D-75，v2.13）

> 缺陷来源（主人真机实测）：手机上扫完电脑的配对二维码 → 手机端弹出「请核对电脑上的会话码」，
> **而电脑屏的配对面板在这一瞬间已经关闭了**，会话码随之消失；重新生成二维码 → 又是新会话、新码。
> ⇒ §5.7 那套核对**从未真正可用**（功能写着，用不上）。

**根因不是显示 bug，是时序互斥**（写成一句话以便不再犯）：

> 「会话结束」事件把「核对会话码」这个动作的**前提**一起删掉了 —— 会话码挂在 `pairing` 对象上，
> 而事件一到就无条件清空它；偏偏手机端的核对提示**正好在事件之后才出现**。

后端在两种结局都发同一个事件（`sync_cmd.rs`）：`completed`（成功）/ `aborted`（失败/超时）。
前端旧实现只按「结束」处理、不分辨结局，于是成功也被当成「结束」清屏。

**状态机（四个态，`completed` 与 `aborted` 分叉）**

| 态 | 判据 | 面板显示 |
|---|---|---|
| 生成中 | `busy === true` | 「生成配对二维码」按钮转圈、禁用 |
| 已生成 | `pairing !== null && !pairCompleted` | 二维码 + 配对文本 + 有效期 + **会话码** + 取消按钮 |
| **配对成功（待核对）** | `pairCompleted && pairing !== null` | **无二维码**；一行「配对成功」+ 一行核对提示 + **会话码大字** + 主按钮「完成」 |
| 失败/超时 | `pairing === null && pairEnded` | `sync.pair_ended` 提示 + 「生成配对二维码」按钮 |

**硬约束**：

1. `pairing://ended` 的载荷**必须**经纯函数 `nextPairingState(payload)`（`src/lib/pairing-state.ts`）翻译成
   `{ clearPairing, completed, ended }` **三件套**，不许在 `listen` 回调里直接写 `if (payload === 'completed')`：
   - `completed` → `clearPairing: false`（**保留** `pairing`，会话码还在里面）、`completed: true`、`ended: false`；
   - 其余（`aborted` / 空串 / `null` / 未知）→ `clearPairing: true`、`completed: false`、`ended: true`。
2. **未知载荷按失败处理**（宁可让用户重新生成一次二维码，也不留一个「看起来成功了、其实没核对」的假成功态）。
3. 成功态**优先判定**（模板里排在二维码分支之前），且成功态**不渲染二维码** —— 码已经扫过了，留着会让人以为还能再扫一次。
4. 退出成功态只有两条路：用户点「完成」（`sync.endPairing()`，只清内存、**不打 IPC** —— 后端发 `completed` 之前
   已 `*state.pairing = None` 并 `break` 了 worker）／重新生成 / 取消 / 锁定，后三者都把 `pairCompleted` 复位为 `false`。
5. 会话码仍走 `hasSessionCode()` 判空后渲染（§5.7.3），成功态不得例外。

**机器守卫**：`src/lib/pairing-state.test.ts`（16 例）三层：
① 纯函数语义（成功不清屏 / 未知保守 / 三字段互斥）；② store 的 handler **必须先算状态再决定清屏**（`nextPairingState(` 的下标早于任何 `sync.pairing = null`），且不得退回字面量比较；③ `PairShow.svelte` 成功态分支存在、位于二维码分支之前、块内无 `qr_svg` / `sync-qr`（含「块长度 > 200」的前置断言，防止整块被删后负向断言恒绿）。


---

### 5.8 文件落盘：Android SAF 与「取消不是错误」（D-71 / D-72，v2.11）

> 需求来源（客户原话）：「手机端想把附件下载保存时，提示：文件读写错误。」「在设置有个导入功能，对导入内容的格式有什么要求？是否需要给用户一个模板？」

#### 5.8.1 症状与根因：两个独立缺陷，同一条流程

| 缺陷 | 症状 | 根因 |
|---|---|---|
| **D-71** | 手机导出附件 → 红字「文件读写出错。」 | 保存对话框回传的是 `content://` SAF URI，被当内核路径交给 `std::fs::write` → `EINVAL` → `ERR_IO` |
| **D-72** | ① 导入面板不说格式要求、无模板可抄 ② 保存流程「点取消弹红」「真失败静默」 | ① 能力缺口 ② 「Android 取消是 reject」这条约定被外包给每个调用点，三处各写一遍、三种错法 |

D-71 因果链（**不是权限问题、不是路径分隔符问题**）：

1. `@tauri-apps/plugin-dialog` 的 `save()` 在 Android 走 `Intent.ACTION_CREATE_DOCUMENT`，回传 `content://com.android.providers.downloads.documents/…`（`DialogPlugin.kt`：`callResult.put("file", uri.toString())`）；
2. 前端把这个字符串当路径交给后端，后端 `std::fs::write(&dest_path, …)` → **ContentProvider URI 没有内核路径** → `ERR_IO`；
3. 前端 toast `err.io`「文件读写出错。」（`src/lib/i18n/zh-CN.ts:390`）。

修复收敛在 `src-tauri/src/content_uri.rs`，经 provider 的 file descriptor 读写（`tauri-plugin-fs` 的 Android 实现 → `FsPlugin.kt::openAssetFileDescriptor`）：

| 决策 | 取值 | 理由 |
|---|---|---|
| 读模式 | `"r"` | 只读打开 |
| **写模式** | **`"wt"`** | `OpenOptions` = read=false + write=true + truncate=**true** → `android_mode()` 映射 `"wt"` |
| 为何必须 truncate | — | `ACTION_CREATE_DOCUMENT` **已经建好**目标文件；新内容比旧的短时不截断会留尾部残字节 |
| 为何不用 `"rwt"` | — | 多要一个读权限，部分 provider 直接拒 |
| 为何不带 create | — | 文件由系统创建，`"wt"` 不含 `O_CREAT` 语义 |
| capabilities | **无需改动** | Rust 侧直接调插件 API，不经 IPC 权限层 |
| 读上限 | `MAX_ATTACHMENT_BYTES` + 1 字节 | SAF 允许选中几百 MB 视频，不能先整份读进内存再让 core 拒掉（Android 上是 OOM 级尖峰）；超限即 `ERR_ATTACHMENT_TOO_LARGE` |

⚠️ `content_uri.rs` 的两个函数**不加 `cfg` 门控**：桌面侧 `Fs::open` / `FilePath` / `OpenOptions` 与 Android 同名同签名，不门控才能被 `cargo check` / clippy 在桌面目标下覆盖（**没编过等于没写过**）。只有 **Android 才会真正调用**，故各命令的 `content://` 分支都在 `cfg(target_os = "android")` 内。

#### 5.8.2 「取消不是错误」的唯一口径（硬约束）

Android 上 dialog 插件**取消时 reject**，桌面**resolve `null`**。历史实现散落三处、三种错法：

| 位置 | 错法 | 用户实际看到 |
|---|---|---|
| `src/lib/vault-export.ts` `pickDestination()` 写在 try **外** | Android 取消 = reject 逃出函数，`Settings.svelte` 那侧无 try/catch → **未捕获 rejection** | 点取消**毫无反应**（控制台一条未捕获异常） |
| `AttachmentView.exportAttachment` try 包住 save + IPC | 取消被当成失败 | **点取消弹红「内部错误」** |
| `ImportWizard.downloadTemplate` 空 `catch { return }` | 真失败也被一起吞掉 | 写盘真失败 → **静默无反应** |

**硬约束一：判定只有一处。** 一律走 `src/lib/save-dialog.ts` 的 `runSaveToDestination(deps): Promise<SaveOutcome>`，其中 `SaveOutcome = { kind: 'cancelled' } | { kind: 'saved' } | { kind: 'failed'; messageKey: string }`。三个调用点（`vault-export` / `AttachmentView` / `ImportWizard`）**全部走它**，组件里**禁止**再出现 `catch {}` 或 `dest === null` 之类的判定——留两处判断就是留两个口径。

**硬约束二：try 里只放对话框调用。** 对话框抛 = 取消（用户主动行为，不是错误）；`write` 抛 = 真实失败（必须红）。这样**不用赌**插件取消时到底走 reject 还是 resolve(null)，两条都判成取消。

**硬约束三：取消必须完全安静，失败必须红。** 取消 → 无 toast、且 `write` **一次都不得被调用**；失败 → error toast，文案取 `toAppError(err).messageKey`，**不得自定义**（保持 `err.io` 等既有码位可解释）。

**硬约束四：空串按取消处理**（`typeof dest !== 'string' || dest.length === 0`）。只判 `dest === null` 会在「插件回空串」时漏掉，`save-dialog.test.ts` 有专门一例钉住。

#### 5.8.3 导入格式说明与模板按钮（D-72）

导入面板必须在**用户选文件之前**就说清「什么能导、列名叫什么」——而不是等解析完给一堆跳过计数让用户自己猜。三件套：

| 元素 | 类名 | 文案键 | 内容 |
|---|---|---|---|
| 支持范围 | `.pms-meta` | `import.supported_hint` | 一行说清支持 Bitwarden JSON / CSV |
| 细则 | `.pms-hint` | `import.template_hint` | CSV 列清单 + `type` 取值，可照着填 |
| **编码提示（D-87，v2.20）** | `.pms-meta` | `import.encoding_hint` | Excel / WPS 编辑后建议另存「CSV UTF-8」；GBK（ANSI）/ UTF-16 自动识别转换 |
| 模板按钮 | `.pms-btn.is-ghost` | `import.download_template` | `download` 图标（15px）；成功后 toast `import.template_saved` |

- 两行说明**常显**，不随格式单选切换而隐藏（用户要判断的正是「我手上这个 json / csv 能不能导」）。
- **D-87 编码容错（后端行为，UI 只做说明）**：中文 Windows 上 Excel / WPS 编辑 CSV 另存会写成 ANSI（GBK），旧实现 `std::fs::read_to_string` 严格 UTF-8 → `InvalidData` 被误映射 `ERR_IO`「文件读写出错」，且前端表头识别走 `TextDecoder`（非 fatal、坏字节静默替换）把问题掩盖——表头照常识别、预览必失败、确认按钮（D-86 footer，preview 非 null 才渲染）随之不出现。修法：core 新增 `import::decode_source`（UTF-8 BOM 剥离 → UTF-16 LE/BE BOM → UTF-8 严格 → GBK 兜底 → 仍失败报 `ERR_IMPORT_PARSE`，文案 `err.import_parse` 同步改准确），桌面路径与 Android `content://` 字节路径统一走它；`encoding_rs` 入 §8.1 白名单。守门：`crates/passwordms-core/tests/import_encoding.rs`（8 例，含「错误码不得回落 ERR_IO」回归钉）+ `import-wizard-ui.test.ts` D-87 组。
- 模板内容固定取 `passwordms_core::import::template::CSV_TEMPLATE`，**禁止在前端拼字符串**——前端拼的模板迟早和解析器漂移。
- **模板自身导入必须 0 跳过**：表头 10 列与 `KNOWN_COLUMNS` **逐字同序**；示例行**不得溢出到 `custom_fields`**（列数 = 表头数）。两条均由 `crates/passwordms-core/tests/import_template.rs` 锁死，改模板必须同步改测试。
- 模板只做「照着填」的示范，**不做通用列映射向导**（主人本轮拍板：不做）。

#### 5.8.4 样式与交互硬约束

| 约束 | 规则 |
|---|---|
| 类名 | 说明两行用 `.pms-meta`（中性说明）/ `.pms-hint`（细则），**不新造类**；按钮复用 `.pms-btn.is-ghost` |
| 顺序 | 格式单选 → `.pms-meta`（支持范围）→ `.pms-hint` → `.pms-meta`（编码提示，D-87）→ 模板按钮 → 映射表。**说明必须在解析按钮之前**，这是本条缺陷的核心 |
| 图标 | `Icon name="download"`，`size={15}`（与生成器同一档 ghost 按钮） |
| 禁用态 | 模板按钮**不因「未选文件」而禁用**——用户常常先拿模板、再填文件 |
| 模板文件名 | `passwordms-import-template.csv`（`filters` 限 `csv`），桌面 `defaultPath` 即此名 |

---

## 6. 移动端布局（<768px）

> 客户拍板 **方案 A：单栏推进 + 底部导航**。目标机型：安卓 APK（手机竖屏，视口 ≈ 390×844）。
> 缺陷背景见 §7 D-35：改动前三栏写死、零断点，手机视口下**最小内容宽度 785px**、**82 个元素横向溢出**，主界面不可用。

### 6.1 断点约定（硬约束）

- `@media (max-width: 767px)` = **移动端布局**。
- `@media (min-width: 768px)` / 默认 = **桌面端，必须完全不变**（验收红线：1100×720 逐像素对比）。
- 桌面三栏宽度（分组 240 / 列表 300 / 详情 `flex:1`）**一个字都不改**；所有移动端规则一律只写在 `<768px` 块内。

### 6.2 顶栏改造（`MainLayout.svelte`）

| 元素 | 桌面（≥768px） | 移动（<768px） |
|---|---|---|
| `.topbar-nav`（5 个 tab） | 显示 | **隐藏**（移到 §6.3 底部导航） |
| `.topbar-right`（生成密码 / 语言 / 导入 / 锁定） | 显示 | **隐藏**，收进「⋯」溢出菜单 |
| 左侧按钮 `.topbar-lead` | 隐藏 | 显示：列表态 = ☰（打开分组抽屉）；详情/编辑态 = ←（返回） |
| 标题 | `app.title` | 当前分区名（`NAV` 里按 `ui.section` 取 `ui.t(entry.label)`） |
| 「⋯」菜单 `.topbar-more` | 隐藏 | 显示，`z-index: 70` 高于内容 |

- 「⋯」下拉菜单项依次：**生成密码**（`genOpen=true`）/ **语言**（显示当前 locale，`switchLocale()`）/ **主题**（三态：系统·浅色·暗色，复用 `theme` store 的 `mode` + `setMode()`，同 `Settings.svelte` 写法）/ **导入**（`importOpen=true`）/ **锁定**（`lockNow()`）。
- **点菜单外部即关闭**（`<svelte:window onclick>` 判定点击是否落在菜单容器外）。
- 左侧按钮仅在 `ui.section === 'items'` 渲染；health / trash / sync / settings 四个分区**不显示**它。

### 6.3 底部导航栏（移动端 only）

- `position: fixed; bottom: 0; left: 0; right: 0`，高度 `56px`，**并加 `padding-bottom: env(safe-area-inset-bottom)`**（安卓手势条 / 刘海机型）。
- 背景 `var(--pms-surface)`，顶部 1px `var(--pms-border)`，`z-index: 50`。
- **5 个 tab**，与顶栏 `NAV` 数组一一对应（items / health / trash / sync / settings），每个 `flex: 1` 平分，**图标在上、文字在下**（小字号）。
- **选中态（v1.5 定稿）：胶囊 + 柔光，不给整格铺色。**
  - 图标必须先包一层 `<span class="tab-glyph">`（46×26，`border-radius: var(--pms-r-pill)`），柔光加在这层上才有形状；直接加在 `<button>` 上又会变成"整格铺色"。
  - 选中：`.bottom-nav-tab.active` 只改文字/图标颜色为 `var(--pms-accent)`（按钮自身 `background: transparent`）；
    `.bottom-nav-tab.active .tab-glyph { background: var(--pms-accent-soft); box-shadow: 0 0 16px -6px var(--pms-accent); transform: translateY(-1px); }`
  - 悬停：`.bottom-nav-tab:hover .tab-glyph { background: var(--pms-bg-2); }`
  - **`.tab-glyph` 上不能有 `overflow: hidden`**（由 `.tab-label` 单独承担省略号），否则柔光会被裁掉。
  - 理由：原先 `.active` 是整格 `--pms-accent-soft` 铺底，底栏被切成 5 个色块，方块感重、视线被"框住"；改为胶囊后**底栏始终是一条干净的横线**，当前分区靠一颗亮起的胶囊 + 柔光表达。
- 点击行为与桌面顶栏**完全一致**：`ui.setSection(key)` 然后 `dispatch({ type: 'section' })`。
- **短标签**：底部格子仅约 78px 宽，塞不下「全部条目」「密码体检」这类长文案，故新增短标签键（`nav.tab_items` / `nav.tab_health` / `nav.tab_trash` / `nav.tab_sync` / `nav.tab_settings`）。
- 因为底栏固定，**主内容区 `.body` 移动端预留 `padding-bottom: calc(56px + env(safe-area-inset-bottom))`**，否则最后一条会被挡住。
- 桌面端 `display: none`。

### 6.4 分组抽屉（移动端 only）

`FolderSidebar` 由 `.drawer` 包裹层 + `.drawer-backdrop` 遮罩承载：

- **桌面端**：`.drawer { display: contents }` —— 包裹层在布局上「透明」，三栏布局**一个字都不变**。
- **移动端**：
  ```css
  .drawer { position: fixed; top: 0; bottom: 0; left: 0;
            width: min(84vw, 300px); z-index: 60;
            transform: translateX(-100%);
            transition: transform var(--pms-dur) var(--pms-ease); }
  .drawer.open { transform: none; }
  .drawer-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,.5); z-index: 55; }
  ```
- 点遮罩关闭抽屉；在抽屉里选了分组 / 收藏后**自动关闭**（`FolderSidebar` 新增可选 `onpicked?: () => void` prop）。
- `FolderSidebar.svelte` 自身在 `<768px` 把 `width: 240px` 改为 `width: 100%; height: 100%`，否则抽屉里会留白。

### 6.5 单栏推进（移动端 only）—— 关键设计决策

**列表与详情两栏在移动端同屏只显示一栏**，用「已选条目」作唯一真相推进：

- 给 `<main class="body">` 加动态 class：`class:showing-detail={vault.selectedItemId !== null || editing}`。
- 列表与详情各包一层 `.pane`：桌面端 `.pane { display: contents }`（保证三栏布局不变）；移动端：
  ```css
  @media (max-width: 767px) {
    .pane { display: flex; flex: 1; min-width: 0; }
    .body:not(.showing-detail) .pane-detail { display: none; }
    .body.showing-detail .pane-list { display: none; }
  }
  ```
- 移动端 `.body` 本身保持 `display: flex`（横向），因为同一时刻只显示一栏。
- **返回**：`editing` → `finishEdit()`（回只读详情；新建态因 `selectedItemId` 为 null 自然落到列表）；`!editing && selectedItemId !== null` → `vault.selectItem(null)`（回列表）。

> **设计决策与理由（务必遵守）**：单栏切换**只用 `vault.selectedItemId`（配合派生于单一 `pane` reducer 的 `editing`）作为唯一真相**，**禁止**新增任何「当前是列表还是详情」的独立布尔/媒体查询状态变量。
> 理由：本项目出过一次 **D-31 事故** —— 用两个独立状态分别描述「是否新建态」与「选中了哪一项」，二者不同步，导致点条目右侧毫无反应。**同一件事只能有一个状态源**；`selectItem(itemId: string | null)` 本就支持传 `null`，`selectedItem` 是 getter，`selectedItemId === null` 时返回 `null`，足以表达全部状态。

### 6.6 子组件宽度适配（各自加 `@media (max-width: 767px)`）

| 文件 | 移动端改动 |
|---|---|
| `ItemList.svelte` | `width: 300px` → `width: auto; flex: 1; border-right: none;` |
| `ItemDetail.svelte` | 已具备 `flex: 1; min-width: 0`，无需改动 |
| `ItemEditor.svelte` | `flex: 1; min-width: 0` 桌面/移动**统一生效**（v2.22 D-92：不再只写在移动端媒体查询里；桌面端 `.pane` 是 `display:contents`，不给就按内容收缩）；底部操作条 v2.22 起为 flex 固定底栏（非 sticky），随 `.editor` 撑满贴底，移动端因 `.body` 预留了底栏高度而不会被遮挡 |
| `Settings.svelte` | `.set-row` 由「label 左 / 控件右」改为上下堆叠，`.set-ctl` 占满整宽（避免 160px 固定宽控件 + 长文案溢出）。⚠️ **仅限控件行**；纯信息行加 `.is-info`、按钮行加 `.is-actions` 覆盖回左右 / 横排，详见 **§6.10**（D-73 同轮） |
| `Trash.svelte` | `.trash-row` 允许换行，动作按钮落到下一行 |
| `MainLayout.svelte` `.sync-grid` | `minmax(300px, 1fr)` 在 390px 下会溢出 → 改单列 `grid-template-columns: 1fr` |

### 6.7 新增图标与 i18n

- `common/Icon.svelte` 新增 `menu`（汉堡）、`arrow_left`（返回）、`more`（⋯）三个内联 SVG（24×24 viewBox、`fill="none"`、`stroke="currentColor"`，与既有条目同构，零外部依赖）。
- 新增 i18n 键（zh-CN / en-US 双份）：`nav.tab_items`、`nav.tab_health`、`nav.tab_trash`、`nav.tab_sync`、`nav.tab_settings`、`nav.more`、`nav.open_folders`、`nav.language`。

### 6.8 移动端类名契约（供自动化断言，不得随意改名）

| 元素 | 类名 | 选中/打开态 |
|---|---|---|
| 底部导航栏容器 | `.bottom-nav` | — |
| 底栏每个 tab | `.bottom-nav-tab` | 选中加 `.active` |
| 底栏 tab 的图标胶囊层 | `.tab-glyph` | 随父级 `.active` 亮起（背景 + 柔光） |
| 底栏 tab 的文字层 | `.tab-label` | — |
| 分组抽屉容器 | `.drawer` | 打开加 `.open` |
| 抽屉遮罩 | `.drawer-backdrop` | 打开加 `.open` |
| 顶栏左侧「☰ / ←」按钮 | `.topbar-lead` | — |
| 顶栏右侧「⋯」按钮 | `.topbar-more` | — |
| 「⋯」弹出的菜单容器 | `.topbar-more-menu` | — |

可断言行为（硬性）：

1. **桌面（≥768px）**：`.bottom-nav` 的 computed `display` = `none`；`.drawer` 的 computed `display` = `contents`；`.topbar-tab` 共 5 个且可见。
2. **移动（<768px）**：`.bottom-nav` 可见；`.topbar-tab` 不可见（本实现**显式**给 `.topbar-tab { display: none }`，故 computed `display` 直接为 `none`，不依赖父级链）；`.topbar-more` 可见。
3. 移动端点 `.bottom-nav-tab` 能切换 `ui.section`（与桌面顶栏 tab 等效，同为 `pickSection(key)`）。

---

### 6.9 顶栏信息架构与桌面收缩档（D-37/D-38，≥768px）

> 起因一（D-37）：切到英文后顶栏撑爆 —— 英文文案比中文长一大截（`Health check` / `Password generator` / `Import`），
> 全展开需 **~1100px** → 顶栏文字折行 + **整页横向滚动条**。
> 起因二（D-38，客户裁定）：信息架构不科学 —— 常用动作（生成器/锁定）和低频设置（语言/主题/导入）混在一处，
> 窄窗时**常用的反被收起、低频的却常驻**。

**信息架构（D-38，客户拍板「常用的放外面，不常用的放设置里」）**：

- **顶栏右区只放高频动作**：`密码生成` / `锁定` 两个按钮。
- **低频项全部移入「设置」页**，并按频率分组排卡：**外观**（主题 + 语言，置顶）→ 安全 → 体检阈值 → 同步 → **数据**（导入 + 导出）→ 设备 → 修改主密码 → 库文件（沉底）。
- 「⋯」溢出菜单**从桌面收缩序列中退役**，仅移动端（`<768px`）使用，内容同步精简为 生成 / 锁定 两项。
- 顶栏文本一律 `white-space: nowrap`，宁可让层级提前收缩，也不许出现折行。

| 窗宽 | 档位 | 右区两钮 | 顶栏 tab |
|---|---|---|---|
| `<768px` | 移动端（§6） | 收进「⋯」 | 隐藏，走底栏 |
| `768–899px` | **桌面紧凑二档** | 图标化（title 保留） | **纯图标**（title 保留），`padding` 收窄 |
| `900–1199px` | **桌面紧凑一档** | 图标化（title 保留） | 文字齐全，`padding` 收窄 |
| `≥1200px` | 完整桌面 | 图标 + 文字 | 文字齐全 |

- 收缩手段从「整体藏进 ⋯」改为**按钮文字图标化**（文案包在 `.btn-text` span 里，窄档 `display:none`）——
  右区只有两个高频钮，藏起来等于自废入口；图标化既保入口又在最窄档省 ~180px。
- 窗口三钮 `WindowControls` **不受档位影响**，永远在最右（它是 `.topbar` 直接子级，见 §8）。
- 需求宽实测账：紧凑二档 ≈ 660px（768 起）、紧凑一档 ≈ 870px（900 起，英文 tab 最坏值）、完整档 ≈ 1060px（1200 起），各档均有余量。

### 6.10 设置页：控件行与信息行必须分开处理（D-73，v2.12）

> 起因：主人装 1.2.6 后发来 390px 截图 —— 「关于」卡片里**每个「标签 / 值」都被拆成上下两行、左对齐**，
> 三个按钮被拉成**三条满宽竖排**。这是 §6.6 表里 `Settings.svelte` 那条改动**误伤**出来的。

**根因不是「规则写错了」，而是「规则的适用范围没写」**：

```css
/* ↓ 这条规则是为「控件行」写的：.set-ctl 那 160px 固定宽在 390px 会溢出 */
@media (max-width: 767px) {
  .set-row { flex-direction: column; align-items: stretch; gap: 0.35rem; }
  .set-ctl { width: 100%; }
  .set-seg { align-self: flex-start; }
}
```

它只判了 `.set-row` 一律堆叠，于是把**纯信息行**和**按钮行**一起改了：

- 信息行：`作者` 与 `hengletime` 变成两行左对齐 → 读起来像两条独立信息，且与上一行的对应关系丢失；
- 按钮行：`align-items: stretch` 把三个 `.pms-btn.is-ghost.is-sm` 拉成满宽竖排 → 三条按钮吃掉小半屏。

**硬约束**（写死，不许再犯）：

> `@media (max-width: 767px)` 的 `.set-row { flex-direction: column }` **只适用于含控件的行**
> （`.set-ctl` / 输入 / 分段控件）。
> 纯信息行（左标签 + 右文本）必须加 `.is-info` 保持左右两端对齐；按钮行必须加 `.is-actions` 保持横排换行。
> 判据：390px 下「关于」组仍是「作者 ……… hengletime」同一行两端对齐，三个按钮横向排布（必要时换行）；
> 1100px 下逐像素不变。

**实现要点**：

1. 三类行用**显式类名**区分，**不许用 `:has()`**（WebView 版本差异，别赌）。
2. 覆盖规则必须写在同一个 `@media` 块内、且**位于 `.set-row` 之后** —— 同位阶靠源码顺序取胜；**不许新开第二个 `@media`**，会把层叠关系搅乱。
3. 桌面（≥768px）**一个像素都不许变**：`.set-row` 本来就是 `space-between`，`is-info` / `is-actions` 在桌面不需要任何新规则。
4. 主题 / 语言 / 自动锁定 / 剪贴板 / 体检阈值 / 改密码 那些行**都是控件行，窄屏堆叠是对的**，继续走原规则，不许加 `is-*`。
5. 设备组用的是 `.set-list-row`（§6.8 契约），走另一条链路，**不受本规则影响，不要去动它**。
6. **机器守卫**：`src/lib/settings-row-type-guard.test.ts`（8 例）扫 `Settings.svelte` 原文，锁死
   ① 恰好 **5** 条 `is-info` + 1 条 `is-actions`，且**全文件只有这 6 条**带 `is-*`（控件行不许被误标）
      —— 计数由 D-76 从「3 + 1」升到「5 + 1」：新增的两条是「保险库文件」卡片里的体积构成行
      （`已删附件占 24.0 MB` / `可回收 24.0 MB`），它们同样是**纯信息行**，必须带 `is-info`
      否则窄屏会被堆叠规则拆成两行、读成两条无关信息（与 D-73 症状⑤ 同型）。
      **改这个数字等于改「哪些行是信息行」的定义，必须同时改本节与 §6.11**；
   ② 覆盖规则在**同一个** `@media (max-width: 767px)` 块内、位于一刀切 `.set-row` 之后；
   ③ 媒体块**外**没有任何 `.set-row.is-info` / `.set-row.is-actions` ⇒ 桌面逐像素不变
      （判定前先剥注释：markup 说明里会写出选择器本身，直接扫全文会把注释当规则）；
   ④ 全文件无 `:has(`（同上，负向断言先剥注释再扫，否则注释里写「不许用 `:has()`」会把自己判红）。

### 6.11 保险库压缩入口（D-76，v2.14）

> 起因：主人实测「把 2.4M 的附件删除后，设置里的保险库文件还是 24M，重启也没变化。保险库有那么大吗？」
> **实测结论：24.34 MiB 里只有 0.30 MiB 是活数据（1.28%），24.03 MiB 全是墓碑帧**（其中 6 个已删附件占 24.0 MiB）。
> 容器是原地改写 + 只打墓碑标志，**从不回收空间**，且全系统此前**没有任何 compression 路径**（连导出备份都是 `fs::copy` 原样复制）。

**本节解决两件事**：① 让用户**看见**体积构成（回答"为什么这么大"）；② 提供一个**可控的回收动作**。

#### 6.11.1 位置与结构（设置在「保险库文件」卡片内，不新增卡片）

在现有卡片（`Settings.svelte:333` `settings.vault_file`）内，`size_bytes` 那行**之后**追加，顺序固定：

```
保险库文件                          ← 卡片标题（既有）
这条说明……                          ← settings.vault_file_hint（既有，D-73）
<路径，等宽可折行>                  ← fileInfo.path（既有）
大小: 24.3 MB                       ← settings.size（既有）
已删附件占 24.0 MB                  ← 新增，仅 attachment_dead_bytes > 0 时显示
可回收 24.0 MB                      ← 新增，仅 reclaimable_bytes > 0 时显示
[压缩保险库]                        ← 新增按钮，仅 reclaimable_bytes > 0 时显示
没有可回收的空间                    ← 新增，reclaimable_bytes == 0 时显示这句（不显示按钮）
```

**为什么单独显示「已删附件占」**：主人问的就是这一句。只给「可回收」不解释来源，用户会以为体积统计坏了。
两个数字都走 `formatBytes()`，与既有大小同一格式函数，不许自己拼单位。

> **D-80（v1.2.11）文案语义核对 —— 前端零改动，本轮不动任何 i18n 键**：
> D-80 让**已仲裁冲突**停止保活其两端快照里的附件（`attachment_blob_refs()` 遇 `resolved` 跳过），
> 副作用是**仲裁后 `reclaimable_bytes` 会增大**——那些 blob 从「被已仲裁快照钉住」转为「可回收」。
> 语义核对：`reclaimable_bytes = file_bytes − (header_bytes + kept)`（`repository.rs:1822`），
> 定义就是**「现在压缩能省多少」**，本来就该随引用集合变化 ⇒ **「可回收 / Reclaimable」文案正确，不用改**；
> `compact_label_dead`（「已删附件占 / Deleted attachments」）对应的是**墓碑帧字节**，
> 而仲裁**不产生新墓碑**（只改 `resolved` 标志 + 写活条目/副本）⇒ **该数字不受影响**。
> 结论：**不需要新增或修改任何界面文案**；`settings-row-type-guard.test.ts` 的「5 + 1」信息行计数同样不变。

#### 6.11.2 按钮与禁用条件（三条，缺一不可）

| 条件 | 处理 | 理由 |
|---|---|---|
| `reclaimable_bytes === 0` | **不渲染按钮**，显示 `settings.compact_none` | 一个点了没有效果的按钮 = 假功能 |
| `sync.syncingDeviceId !== null`（同步进行中） | **禁用** + 提示 `settings.compact_wait_sync` | 后端 `compact_vault` 在 `state.session` 锁内执行，与同步**结构互斥**；不禁用的话用户会以为卡死 |
| 压缩自身进行中 | 禁用 + `Spinner`，文案换 `settings.compacting` | 重写 200MB 级容器可能几秒 |

按钮样式：`.pms-btn.is-ghost`（**不是** `is-primary` / `is-danger`）。压缩不改数据、不破坏内容，**不许用危险色**误导用户以为会删东西。

#### 6.11.3 二次确认（必须列出四个数字 + 三条提示）

用现有 `ui.confirm()` 机制不可行（它只支持标题 + 一句正文），**必须用自定义确认块**（同 `LegalDialog` 的形态），内容：

```
压缩保险库？

  当前大小      24.3 MB
  压缩后约      317 KB
  可回收        24.0 MB
  已删附件占    24.0 MB

  · 压缩不改任何数据，也不需要输入主密码。
  · 回收站里的附件不会被回收，需先清空回收站。
  · 压缩期间会临时占用约 2 倍磁盘空间，请勿关闭程序。

                        [取消]  [开始压缩]
```

**三条提示缺一不可**，理由：
1. 「不改数据、不要主密码」——否则用户以为要重新验密码，直接放弃；
2. 「回收站附件不回收」——**这是最容易被误解成"功能没生效"的地方**：`attachment_blob_refs` 把回收站快照里的附件算作"有引用"（为了还原），所以删进回收站的附件压缩不掉。**同理，冲突快照（`local_snapshot` / `remote_snapshot`）引用的附件也压缩不掉**（D-79）——这是正确的：用户还没仲裁，任何一侧的附件都不能替他扔掉。文案只说"回收站"是因为对客户而言那是他能操作的那一个；冲突引用是系统内部的保活，不需要他理解。
3. 「2 倍磁盘 + 别关程序」——压缩先写 tmp 再原子替换，峰值是 2×；中途杀进程虽然安全（旧库还在）但会让用户白等。

#### 6.11.4 完成与失败反馈

- 成功：`ui.toast('success', 'settings.compact_done', { size })`，**并且必须刷新** `fileInfo` 与 `compactionStats`（否则卡片上还是旧大小，用户以为没生效）。
- 失败：`ui.toast('error', toAppError(err).messageKey)`，**不改动任何显示数字**（宁可显示旧值，不许显示"已压缩"）。
- 失败后**不要**自动重试。

#### 6.11.5 数据来源与加载时机

`vault_compaction_stats` 是**只读、廉价**（只读元数据 + 解密少量墓碑帧），可以随 `loadSettingsPageSide` 一起加载，作为它的**第四块**，同样遵守 D-73 的**逐块失败隔离**（stats 失败只影响「已删附件占 / 可回收」两行与按钮，不影响大小、档位、版本号）。

⚠️ **不许**为了拿 stats 去调 `compact_vault`，也**不许**在 stats 失败时把按钮当"可点"渲染。

#### 6.11.6 i18n 键（zh-CN / en-US 都要，键名固定）

| 键 | zh | en |
|---|---|---|
| `settings.compact` | 压缩保险库 | Compact vault |
| `settings.compact_dead_attachments` | 已删附件占 {size} | Deleted attachments use {size} |
| `settings.compact_reclaimable` | 可回收 {size} | {size} reclaimable |
| `settings.compact_none` | 没有可回收的空间 | Nothing to reclaim |
| `settings.compact_stats_failed` | 读取失败：未能取得体积构成（条目本身不受影响）。 | Could not read the size breakdown. Your items are unaffected. |
| `settings.compacting` | 正在压缩… | Compacting… |
| `settings.compact_wait_sync` | 同步进行中，请稍候 | Syncing, please wait |
| `settings.compact_confirm_title` | 压缩保险库？ | Compact the vault? |
| `settings.compact_label_before` | 当前大小 | Current size |
| `settings.compact_label_after` | 压缩后约 | After compaction |
| `settings.compact_label_reclaim` | 可回收 | Reclaimable |
| `settings.compact_label_dead` | 已删附件占 | Deleted attachments |
| `settings.compact_hint_no_password` | 压缩不改任何数据，也不需要输入主密码。 | Compaction changes no data and does not require your master password. |
| `settings.compact_hint_trash` | 回收站里的附件不会被回收，需先清空回收站。 | Attachments in the trash are not reclaimed; empty the trash first. |
| `settings.compact_hint_disk` | 压缩期间会临时占用约 2 倍磁盘空间，请勿关闭程序。 | Compaction temporarily needs about twice the disk space. Do not close the app. |
| `settings.compact_start` | 开始压缩 | Start |
| `settings.compact_done` | 已回收 {size} | Reclaimed {size} |
| `settings.compact_failed` | 压缩未完成，保险库原样保留，数据没有变化。 | Compaction did not finish. Your vault is unchanged. |

#### 6.11.7 验收判据（可测，不许只靠肉眼）

1. `reclaimable_bytes === 0` 时**按钮不存在**于 DOM（不是 disabled）。
2. 同步进行中按钮 `disabled`，且出现 `settings.compact_wait_sync`。
3. 确认块内**四个数字与三条提示全部存在**（可对 markup 做结构断言）。
4. 压缩成功后 `fileInfo.size_bytes` 与 `compactionStats` **都被重新拉取**（断言代码里有一次 reload，不是只 toast）。
5. 压缩失败时界面数字**保持压缩前值**。
6. 窄屏 390px 下：新增的两行信息走 `.is-info`，确认块可滚动、按钮不溢出一屏（见 §6.10）。
7. 压缩进行中 `reloadSideData()` **短路返回**（不得重拉 stats：后端持 session 锁时会挂住）；
   成功路径那次 reload 用 `reloadSideData(true)`（`force`）绕过守卫 —— **不要**改成
   「先复位 `compacting` 再 reload」：那样 reload 与 effect 触发的 reload 会并发，
   且按钮在 reload 窗口内已可点。
8. 压缩失败时，通用内部错误显示 `settings.compact_failed`（说明库原样保留），不是 `err.internal`；
   体积构成读取失败显示 `settings.compact_stats_failed`，且**不渲染压缩按钮**。

**实现补记（v2.14 落地时补的五点，均已写进代码与守卫）**：

- **键表补了一个 `settings.compact_label_dead`。** 原表只有整句 `compact_dead_attachments`（「已删附件占 {size}」），
  而确认块是「左标签 / 右数值」四行，整句塞不进标签格。故补这一个**标签形态**键；
  整句那个仍用于卡片正文（对客户「是不是附件都还在库里」的回答用整句更自然）。
- **键表补了一个 `settings.compact_stats_failed`（规格缺口）。** 原表没有「体积构成读取失败」的文案，
  最省事的写法是复用 `settings.vault_file_failed`，但那条的主语是**保险库文件路径/大小**，
  安到体积构成上是**归因错位**；而留空、或退化成 `compact_none`（「没有可回收的空间」）更糟——
  后者是一个**结论**，可我们此刻根本不知道（失败伪装成结论 = D-27 / D-73 同族）。
  故单起一句，只陈述「没读到」并立刻安抚「条目本身不受影响」。失败分支也**不渲染压缩按钮**。
- **卡片那两行的实际形态是「整句 + `is-info`」**：文案键本身就带 `{size}`，所以每行只有一个
  `<span>`，`.is-info` 在这里的作用是**让该行不被窄屏 `.set-row` 堆叠规则误伤**，
  而不是真的把一句话劈成左右两段。若将来要真正的「左标签 / 右数值」两端对齐，
  换成 `compact_label_dead` + `compact_label_reclaim` 两个标签键即可（键已在表里）。
- **新增判据 7（竞态）：压缩期间 `reloadSideData()` 必须短路。** 后端 `compact_vault` 在
  `state.session` 锁内执行，此时再调 `vault_compaction_stats` **不失败、也不超时，会一直挂住**——
  前六条判据与 D-73 的「逐块失败隔离」都救不了「挂住」（那不是失败）。触发路径真实存在：
  压缩中离开设置页再回来 → `$effect` 重跑 → 第四次 IPC 挂住，表现为「已删附件占 / 可回收」两行空着。
  修法：`reloadSideData(force = false)`，守卫写成 `if (!force && compacting) return;`；
  成功路径那次显式刷新传 `true`（那一刻 `compact_vault` 已返回、session 锁已释放，读到的是真值）。
  **反例（务必不要这么写）**：`if (compacting) return;` + 在 `try` 里先 `compacting = false` 再
  `await reloadSideData()`。表面上 reload 能绕过守卫，但它让「压缩期间」与「压缩之后」两个意图
  靠**副作用时序**区分，且 `compacting` 是 `$effect` 的依赖（`reloadSideData` 在首个 `await` 之前
  同步读它），复位会再触发一次 effect reload → 两组四块读取并发、按钮在 reload 窗口内已可点。
  用参数区分意图，语义清楚且显式那次**一定真的执行**。
- **新增键 `settings.compact_failed`。** 压缩失败时只弹 `toAppError(err).messageKey`，而通用兜底是
  `err.internal`（「内部错误」）——压缩是大动作，这句话会让用户以为库被写坏。故**通用内部错误**
  换成本键（「保险库原样保留，数据没有变化」）；后端给了**具体原因**时仍照原样透出，
  因为那句更可操作，且同样不改变「数据没变」这个事实（判据 5）。

> 判据 1/2/3/4/5 + 竞态（7）+ 失败文案的守门文件是 `src/lib/vault-compact-ui.test.ts`（**19 例**），
> 判据 6 见 §6.10 与 `src/lib/settings-row-type-guard.test.ts`。
> **已知且接受的代价**：`finally` 里复位 `compacting` 会让 `$effect` 再跑一次 `reloadSideData()`
> （force=false、compacting=false → 放行），所以成功路径一共**两组**四块读取。第二组在压缩结束、
> 锁已释放之后，读到的是同一份真值，不会阻塞也不会写错；为省它引 `untrack` 属过度设计。
> ⚠️ 「两组」这个**条数是按依赖关系推的，不是测出来的，且已决定不测**（成本 > 收益，不是测不出）：
> `environment: 'node'` 下 `$effect` 在 `.svelte.ts` 里不 flush（试过 `$effect.root` + `flushSync` /
> `tick` / `settled` 三种刷法，effect body 一次都没执行）；唯一可测路径是单出一份开 devtools 的
> release 构建、数 `vault_compaction_stats` 出现次数，而它**不改变任何用户可见结果**，不值得多跑一轮
> 构建。**所以不要把它当成已测量的事实引用。** 真正需要保证的是「显式 `reloadSideData(true)` 确实
> 执行了」，它已由 `vault-compact-ui.test.ts` 的 `force` 用例钉住，并有现场判据：压缩点完**停在
> 设置页不动**，大小与两行构成应在 1–2 秒内自动更新；若必须切走再回来才变，即为缺陷。

### 6.12 移动端长按交互与触屏拖拽（#531，v2.16）

- 触屏**没有右键**：上下文菜单统一由**长按 220ms**（位移 ≤6px）唤出，与桌面右键同一份 `ContextMenu`；菜单落点用控制器记录的 lastX/lastY。
- **长按与滚动的边界**：按下后 220ms 内位移超 6px = 用户在滚动，取消武装、不弹菜单；武装后拖拽激活即挂 non-passive `touchmove` `preventDefault` 压住滚动（§5.4.2）。长按原地点菜单、竖直滑动列表滚动的两条手势路径互不干扰。
- **自动滚动底部预留**：`autoScrollDelta` 的 `bottomReserve` 计入移动端底部导航高度 **56px**（`ItemList.svelte` 的 `MOBILE_BOTTOM_NAV_PX`）+ `env(safe-area-inset-bottom)`，否则拖到屏幕底缘时条目被底栏挡住、看不到投放位置。桌面端 reserve 为 0。
- 移动端分组抽屉（§6.4）内的分组行同样支持长按菜单与拖拽排序；抽屉滚动手势与上述边界同一套规则。
- **待真机验证**（已知未决点）：Android WebView 上长按震感 / 长按系统文本选择是否被抑制、220ms 阈值手感、drag-chip 在触屏上的渲染位置——需真机过一轮，模拟器不替代。

### 6.13 移动端「向左滑动返回」（D-95，v2.23）

> 需求（主人 2026-10-24）：手机上打开条目后返回要点左上角 ←，改为**在详情页上向左滑动**即可返回
> （手指向左 = 详情内容跟手往左退 = 回到左边的列表，主人自己的心智模型，顺其自然）。

- **手势判定**（`src/lib/swipe-back.ts`，纯函数状态机可单测）：单指触屏（`pointerType === 'touch'` 且 `isPrimary`）、
  **水平向左 ≥ 72px**（`SWIPE_BACK_THRESHOLD_PX`）且**水平主导**（`|dx| ≥ 2|dy|`，`SWIPE_BACK_VERTICAL_RATIO`——垂直滚动时浏览器
  接管指针、dy 大，判定不成立，天然不打架）；一次手势只触发一次（`triggered` 锁定，滑过头不重复返回）；
  多指只跟主 pointerId，第二根手指事件直接忽略。
- **启用范围（A 案，主人拍板）**：仅**详情查看态**（`inItems && !editing && vault.selectedItemId !== null`）。
  **编辑/新建态刻意不启用**——未保存的表单内容不能被一个手势丢掉，取消按钮语义保留；主人若后续要编辑态，必须带脏确认弹窗。
- **接线**（`MainLayout.svelte`）：4 个 pointer 事件绑在 `.pane pane-detail`（`role="presentation"` 满足 a11y），
  触发即调**既有 `goBack()`**——编辑态回详情、详情态回列表全走它，**不新增任何导航状态**（D-31 铁律）；
  `pointerup`/`pointercancel` 都清手势状态（跨手势不复用）。桌面鼠标被 `pointerType` 过滤天然免疫
  （`display:contents` 下事件会冒泡到 pane-detail，靠 pointerType 挡，**不赌盒模型**）。
- **与系统手势的关系**：Android 全面屏的边缘返回手势由系统处理，页内**全页任意位置**向左滑不与其冲突；
  顶部 ← 键保留，两种返回方式并存。
- **V1 不做跟手动画**（松手前无位移动画，达标即返回）；真机体验后若嫌生硬再加跟手位移 + 阈值弹回。
- **⚠️ 真机坑（v1.2.18 补丁，主人实测「完全没反应」）**：`touch-action` 全仓未设时，Android WebView 对
  触摸位移的滚动/手势判定会发 **`pointercancel`**——`.detail` 是默认 `auto`，向左滑的 pointermove 序列
  在达到 72px 阈值前就被掐断，手势永不成立。修法 = `.detail { touch-action: pan-y; }`：垂直滚动照常由
  浏览器处理（判定不成立的场景本就不该触发），水平方向归 JS 手势；桌面鼠标不受该属性影响。
  **教训：WebView 里写 pointer 手势，`touch-action` 不是可选项，是手势成立的前提。** 守门：`swipe-back.test.ts`
  第 14 例锁 `.detail` 必须带 `touch-action: pan-y`。
- **验证口径**：触屏手势**桌面走查包验不了**（鼠标不启用），最终判据 = Android 真机；守门 `swipe-back.test.ts`
  14 例（判定 10 例 + 接线源码断言 3 例 + touch-action 守门 1 例）。

## 7. 本次必须修的缺陷


| 编号 | 级别 | 内容 | 位置 |
|---|---|---|---|
| **D-19** | P0 可用性 | 全局 `.field{flex-direction:column}` 泄漏进组件同名局部类 → 输入框与眼睛**纵向堆叠**，用户找不到输入位置 | `CreateVaultWizard.svelte:133`、`LockScreen.svelte:106` |
| **D-20** | P0 语义 | 眼睛图标**语义反了**（隐藏时给 `eye`，明文时给 `eye_off`） | `CreateVaultWizard:60`、`LockScreen:49`、`ItemDetail:207`、`ItemEditor:132` |
| **D-21** | P1 文案 | label 与 placeholder 用了**同一条 key** `lock.password_placeholder`，界面出现两遍"请输入主密码" | `LockScreen.svelte:33 / :39` |
| **D-22** | P1 体验 | 眼睛无固定位置、热区偏小（`padding:.35rem` 且居中） | 同上两处，随 `.pms-input-group` 一并解决 |
| **D-23** | P2 一致性 | 全 app 视觉语言不统一（字号/圆角/间距/阴影各写各的） | 全组件，随设计令牌统一 |
| **D-27** | **P0 不可用** | **本规范 §3 自带的缺陷**：`.is-primary:hover` 只补了 `filter`/`border-color`，**漏了 `background`** → 被通用 `:hover` (0,3,0) 抢走 → 悬停时文字压同明度底色，**浅色 1.17:1 / 暗色 1.09:1，按钮"一片空白"**。`.is-danger` 同病；`.is-ghost` 悬停时 `border-color` 被抢成青色，平白多出一圈描边 | 规范 §3 按钮段 + `global.css:155-178`，v1.2 已修 |
| **D-28** | P1 可读性 | `.pms-btn.is-danger` 写死 `color:#fff`，暗色下 `#fff` 压 `#F87171` 只有 **2.77:1**（低于控件底线 3.0）。同一 bug 此前无人察觉，因为危险按钮只在确认框/彻底删除里出现 | v1.2 引入 `--pms-danger-ink`（浅 `#FFFFFF` / 暗 `#2A0A0A`）→ 6.62:1 |
| **D-29** | P1 可读性 | `.pms-icon-btn` 静止色用 `--pms-faint`，浅色下 `#8B9AAB` 压白底仅 **2.87:1**（眼睛图标、可见性开关全是这个色） | v1.2 改用 `--pms-muted` → 浅 5.47:1 / 暗 6.49:1 |
| **D-30** | **P0 观感** | **密码框出现两个眼睛**：WebView2/Blink 会给 `input[type=password]` 渲染**原生密码揭示按钮**（`::-ms-reveal`），与我们自己的 `.pms-icon-btn` 眼睛并存。客户截图实测：一个 42px 高的密码框里挤了 **4 个图标**（3 个原生 + 1 个自绘） | `global.css` 新增 `input[type='password']::-ms-reveal, ::-ms-clear { display:none }`（v1.3） |
| **D-31** | **P0 功能不可用** | **点击已保存条目，右侧不显示详情**：`MainLayout` 把「是否处于新建/编辑态」（`editing` / `creatingKind`）与「当前选中项」（`vault.selectedItem`）**当成两个独立状态各自维护**，二者完全解耦。用户一旦点过"新建笔记"，`editing` 就恒为 `true`，此后点列表项只改 `selectedItem`，右侧渲染分支仍停在新建表单 → **右侧毫无变化** | 抽成纯 reducer `src/lib/pane-mode.ts`（`detail` / `create` / `edit` 三态），**`select` / `done` / `section` 三个事件一律强制回 `detail`**；`MainLayout` 改为 `pane` 单一状态源 + `dispatch()`；有 8 例单测守门（v1.3） |
| **D-32** | P1 观感 | **顶部白色标题栏与应用暗色界面硬切**：`tauri.conf.json` 的窗口配置里没有 `theme` 字段，原生标题栏跟随系统（浅色），而应用是暗色。逐像素实测：y=0~56 为 `rgb(242,242,242)`（99% 浅色像素），y=60 起为 `#121A24`（99% 深色），**中间无过渡** | ① `tauri.conf.json` 加 `"theme": "Dark"`；② `capabilities/default.json` 加 `core:window:allow-set-theme`；③ `theme.svelte.ts` 新增 `syncWindowChrome()`，主题切 `light`/`dark` 时调 `getCurrentWindow().setTheme()`，选 `system` 传 `null`（v1.3） |
| **D-33** | **P0 构建** | **不是 UI 缺陷，但让上面所有 UI 改动无法出包**：`tauri-winres` → `embed-resource` 在 Windows+GNU 宿主上写死 `Command::new("windres")`（只认 PATH），而本机 windres 在自备目录里、那条 `export PATH` 从未固化 → 一换构建入口就 `NotAttempted("windres")` panic。已知无效的修法：`.cargo/config.toml` 的 `[env] PATH prepend`（Windows 下被系统注入的 `Path` 吃掉）、`RC` 环境变量、`set_windres_path()`（no-op）、`~/bin`（不在 Windows PATH 上） | `src-tauri/build.rs` 新增 `ensure_windres_on_path()`，按 `PMS_WINRES_DIR` → `%USERPROFILE%\.workbuddy\tools\mingw-bin` → 常见 mingw 路径 探测并前置进本进程 PATH（v1.3，详见 `03b-verification.md` §8.2） |
| **D-34** | P1 可发现性 | **密码生成器"根本找不到"**：功能早已实现（位数 8–64 + 四类字符 + 排除易混），但入口是密码框右侧一个**无 tooltip 的 `⟳` 刷新图标**、行内展开 | ① 图标换成骰子 `dice` 并加 `title`；② 改为 `PasswordGeneratorModal.svelte` 弹窗；③ 顶栏新增「生成密码」入口；④ 补数字输入框、强度条、"至少选一类字符"约束（v1.3） |
| **D-35** | **P0 不可用** | **手机端主界面横向溢出 / 不可用**：全项目零响应式断点（`global.css` 仅 `prefers-color-scheme` / `prefers-reduced-motion` 两条 `@media`，组件 `@media` 数量为 0），三栏宽度写死（`FolderSidebar.svelte` `width:240px` / `ItemList.svelte` `width:300px`）。390×844 手机视口实测：**最小内容宽度 785px、82 个元素横向溢出**（顶栏合计需 785px = 品牌区 159 + `nav.topbar-nav` 286 + `div.topbar-right` 324）—— 顶栏导航文字被压成**竖排单字**，顶栏右侧 4 个按钮被**整个挤出屏幕**，分组栏吃掉近半屏，详情面板在屏幕外，生成器弹窗被挤到右下角且内容裁切。桌面 1100px 窗口一切正常，故长期未暴露 | 见 §6：移动端单栏推进 + 底部导航（v1.4）。实测（无头 Edge，390×844，注入假 IPC 经真实解锁进入主界面）：改后 `documentElement.scrollWidth = 390`、溢出元素 `0`；桌面 1100px 下 `side=240 / list=300` 等逐项不变 |
| **D-36** | P1 观感 | **原生标题栏与页面品牌重复**：原生标题栏（图标 + `PasswordMS`）紧贴页面顶栏（品牌标 + `PasswordMS`），上下两套标题；且原生图标长期是 Tauri 默认假图标（`icon.ico` 仅 390 B）。另查明：桌面/标题栏图标"没改过来"实为**装了旧包**——20:24 前的 exe 内嵌的还是旧图标（已用 ico 内 PNG 指纹在 exe 二进制中命中 1 次证明新 exe 嵌入了新图标），装新包 + 刷新 Windows 图标缓存（`ie4uinit -show`）即可 | 见 §8：`decorations:false` + `WindowControls.svelte` + `data-tauri-drag-region`（v1.6）。原生标题条整条消失，重复不复存在；桌面/任务栏/安装包图标统一为新 LOGO |
| **D-37** | **P1 可用性** | **切英文后桌面顶栏撑爆**：英文文案比中文长（`Health check`/`Password generator`/`Import`），顶栏全展开需 ~1100px，默认窗宽 1100 / 实测 1081 装不下 → tab 文字折行（"All items" 两行）+ **整页横向滚动条**。根因：桌面顶栏**没有任何收缩策略**，中文塞得下纯属巧合；且顶栏文本未锁 `nowrap` | 见 §6.9（v1.7）：①顶栏文本一律 `white-space: nowrap`；②新增两档桌面收缩——`900–1199px` 右区四钮收进「⋯」（复用移动端交互）、`768–899px` tab 再收成纯图标；`≥1200px` 才完整展开。各档需求宽（≈600 / ≈800 / ≈1100）均有余量 |
| **D-38** | **P1 信息架构** | **顶栏常用/低频动作混排**（客户实测裁定）：右区四钮把低频的「语言 / 导入」常驻顶栏，而 ⋯ 菜单里塞了生成器/语言/主题/导入/锁定五项大杂烩；窄窗收缩时**常用的生成器/锁定反被收起**。设置页分组也无频率层次（主题卡独占"通用"置顶，导入入口却在顶栏） | 见 §6.9（v1.8）：①顶栏右区只留 `密码生成`/`锁定`；②语言/主题移入设置「外观」组（语言改 中文/English 分段钮）、导入移入设置「数据」组（Modal 随迁）；③⋯ 菜单退役为移动端专用（仅 生成/锁定）；④桌面收缩档改按钮图标化（`.btn-text`），不再整体藏 ⋯；⑤设置页按频率重排：外观→安全→体检→同步→数据→设备→改密码→库文件 |
| **D-39** | P2 状态可视 | **已收藏项目的五角星仍是空心**：列表行与详情页的收藏星用同一个 outline 图标，选中与否毫无区分，扫一眼分不清哪些收藏过（客户截图反馈） | `Icon.svelte` 新增 `filled` prop（`fill: currentColor` 覆盖全局 `fill="none"`）；列表星恒实心（仅在收藏时渲染）+ 详情星 `filled={item.favorite}` 且选中态整钮转 `--pms-warning` 金色、补 `aria-pressed`（v1.8） |
| **D-40** | **P0 功能不可用（Android）** | **手机端扫码必报「发生内部错误」**：`AndroidManifest.xml` 未声明 `CAMERA` 权限，而 wry 的运行时 `requestPermissions` 对未在 manifest 声明的权限**必被系统拒绝** → `getUserMedia` 抛 `NotAllowedError` → 前端兜底 toast 笼统的 `err.internal`。电脑端摄像头可开但识别率低：jsQR 全帧解码 + 400ms 抓拍间隔，定焦前摄拍手机屏困难 | ① manifest 加 `<uses-permission android:name="android.permission.CAMERA"/>`（wry 0.57 授权链已通：onPermissionRequest → 运行时请求 → grant）；② PairScan 错误分类：`NotAllowedError/NotFoundError` → 专用可行动文案 `sync.camera_unavailable`，其余留 `err.internal` + console.error 留痕；③ 相机约束 `width/height ideal 1280×720`、抓拍 400→250ms；④ `decodeQrFromVideo` 中心裁剪 75%（jsQR 建议二维码与画面边缘留白，裁剪等效放大目标） |
| **D-41** | **P0 不可用（Android）** | **系统状态栏压住应用顶栏，汉堡按钮点不到**（客户真机截图：状态栏时间「23:04」与顶栏 ☰ 重叠）：`MainActivity.kt` 里 Tauri 模板显式 `enableEdgeToEdge()` → WebView 铺满全屏，而 wry 0.57 **不处理 window insets** → 状态栏悬浮于前端顶栏之上，抢占其点击热区 | `MainActivity.kt` 给 `android.R.id.content` 根视图挂 `ViewCompat.setOnApplyWindowInsetsListener`：按 `systemBars` insets 打 padding（top/left/right/bottom），把 WebView 推到系统条之内；返回原 insets 不消费，键盘 IME insets 继续下传。前端零改动（桌面 WebView2 不经过此路径，不受影响） |
| **D-42** | P1 体验 | **界面语言重启后丢失**：`ui.locale` 仅存内存，重启回退中文 | localStorage 键 `passwordms.locale` 持久化（try/catch 兜底），`setLocale` 写入（v1.8） |
| **D-43** | **P0 可诊断性** | **Windows GUI 无 stderr，配对诊断全部丢失**：`eprintln!` 在无控制台的 GUI 进程里被丢弃，配对失败只能看到前端笼统报错，无从取证 | `state.rs` 新增 `append_log()`：落盘数据目录 `passwordms.log`（>1MB 截断重建）；`sync_join` 失败与 pairing worker 的 completed/aborted 全部留痕（v1.8） |
| **D-44** | **P0 功能不可用（配对）** | **手机配对必败且报错误导**（客户真机：扫码报「文件读写错误」、粘贴挂 ~21s 后报「同步已中止」；Host 日志仅 `CoreError("ERR_IO")`）。回环测试（`pairing_loopback.rs`，127.0.0.1 全链路）通过 → 协议无错，根因在网络层四处叠加：①多网卡机器（WSL2/Hyper-V）UDP 选路探测只广播**一个** IP，选中手机路由不到的虚拟网卡即必败；②Android `EHOSTUNREACH` 在 stable Rust 折叠为 `Uncategorized` → 文案落到「文件读写错误」；③`TcpStream::connect` 失败一律折叠成 `SyncAborted` 且无超时（SYN 被静默丢弃要挂 ~21s）；④会话死后二维码仍挂着，Host 侧 io 错误无阶段信息 | ①`QRPayload` 新增 `ips: Vec<String>`（`serde(default)` 兼容旧码），`local_ipv4s()` 枚举全部非回环 IPv4、主选排最前，Joiner 逐个尝试（各 4s 超时）；②`connect_with_timeout` 保留 io kind（超时/不可达 → 专用文案 `err.network_unreachable` 等）；③`CoreError::Io` Debug 放出 `stage\|io:Kind`（server.rs 各阶段标注 hello_recv/accept_send/…），日志可定位断开点；④worker 结束时 emit `pairing://ended`，前端收二维码 + 提示「请重新生成二维码」（`sync.pair_ended`）；⑤配对页显示本机地址 + 防火墙放行提示（v1.9） |
| **D-50** | P1 生成器体验 | **符号集不可自选**（主人需求）：D-49 把符号池精编为 6 个安全符号是「一刀切」，主人要求「用户愿意选哪些，可以参与生成密码，把常用的列出来」 | `PasswordGenerator.svelte` 勾选「符号」后展示 **34 个常用符号芯片**（`- _ . , : + ~ ? ! * @ # $ % ^ & ( ) [ ] { } < > = \| ; / \ ' " \`），点击切换、选中态用 `--pms-accent` 底 + `aria-pressed`；核心层 `GeneratorOptions` 新增 `symbol_set: Option<String>`（`serde(default)` 兼容旧库/旧前端），`effective_symbol_set()` 去重并过滤为可打印 ASCII 非字母数字，空集回退默认精编集 `+-_.,:`。回归测试：自定义集 200×32 扫描 + 过滤/兜底断言（v2.0） |
| **D-51** | **P0 功能不可用** | **「导出保险库」必报「解锁失败，请重试。」**（用户实测）：导出卡片根本没有密码输入框，而 `exportVault()` 误把「修改主密码」卡片的 `oldPassword`（几乎总是空串）传给 `api.vault.exportVault(dest, oldPassword)`；后端 `repo.export_vault` 要先对**当前主密码**做 `unlock_check` 再 copy 库文件 → 空密码必 `UnlockFailed` → 前端 toast `err.unlock_failed`，报错误导用户 | ① 导出卡片导出按钮**上方**新增当前主密码输入框（`.set-row` + `.set-label` + `.pms-input set-ctl` + `type="password"` + `autocomplete="current-password"`；文案复用 `settings.change_password_old`「当前主密码」语义一致，`id="set-pw-export"` 避免与改密码卡片重复）；② `exportVault()` 改用独立的 `exportPassword`，**为空时不发 IPC** 直接 toast `settings.export_password_required`，成功后清空（v2.1） |
| **D-54** | P1 文案 | **导出确认文案误称「明文」**：`settings.export_confirm_body` 写「导出后文件将以明文形式保存到磁盘」，而实际导出是 `unlock_check` 通过后 `fs::copy` 的**字节级加密副本**（实测导出文件与本机库 sha256 相同、magic 同为 `PMSVAULT`）——事实错误且吓退用户，还漏了「mtime = 库数据最后写入时间」这一可感知行为 | 改为「导出的是整库的加密副本（非明文），用于备份或迁移；可在锁屏页用『从备份恢复』导入。文件的修改时间为库数据的最后写入时间。」（zh/en 同步，v2.2） |
| **D-55** | **P0 功能闭环缺失** | **导出的备份全 app 无导入通道**：用户把库导出后换机/误删，没有任何界面能从备份恢复——备份只能靠手工替换 `%APPDATA%\com.passwordms.app\passwordms.vault` | 锁屏页新增「从备份恢复」入口（D-55）：core 层 `restore_backup_from(src, dest)`（magic 校验 + **恢复前整库帧流完整性校验**——尾部截断/遗留尾料一律拒绝 `corrupt_vault_backup`，杜绝「restore 成功但记录静默丢失」+ `fs::copy` 保留 mtime）+ 3 例回归测试（原密码解锁且条目数一致 / 非 vault 文件拒绝 / 错主密码 UnlockFailed）；src-tauri `restore_backup` 命令（须锁定态、覆盖前留 `.bak-<unix_ms>`、不自动解锁）；前端 dialog 选 .vault → 警告确认 → 用密码框的值走既有 unlock 流程（v2.2） |
| **D-59** | **P0 功能不可达** | **附件功能前端零入口**（#530 真机走查发现）：后端 `import_attachment` 命令、core 层校验（≤20 MB）、D-53 blob 同步、`AttachmentView` 详情渲染全就绪，但列表「新建」区只有登录/笔记两个按钮，`api.keydocs.importAttachment` **全项目零调用**——F-13 密钥文档（附件）从上线起不可达 | 列表头新增第三个「添加附件」按钮（`paperclip` 图标，zh `添加附件` / en `Add attachment`）：点击 → 系统文件对话框（单选）→ `import_attachment` 导入成功即建条目 + `vault.load()` 刷新 + 选中新条目（**不进编辑器**；标题默认取文件名含扩展名，与 core 层空 title 回退语义一致）；对话框取消 → 无动作无 toast；IPC 失败（含超限 `ERR_ATTACHMENT_TOO_LARGE`）→ 统一错误 toast。流程逻辑抽 `src/lib/attachment-import.ts` 纯逻辑层（守卫/取消/成功/失败 4 分支有单测守门）+ ipc-contract 补 `import_attachment` 双侧在册断言（v2.4）**——v2.5 起该入口按 D-60 回退：附件改依附父条目，入口移入详情页附件区（见 §5.4 AttachmentView 行）** |
| **D-75** | **P0 功能形同虚设** | **配对成功后会话码立刻消失，两屏核对从未可用**（主人真机）：手机扫码成功 → 手机端弹「请核对电脑上的会话码」，而**电脑屏的配对面板在数据库发出 `completed` 的同一瞬间已收起**，会话码无处可看；重新生成二维码 → 新会话、新码，永远差一步。D-69 的两屏核对（防中间人）**上线即不可用** | 按载荷分流（`nextPairingState`，纯函数有单测）：`completed` **保留** `pairing` 并置 `pairCompleted`，面板转为**成功态**（无二维码、大字会话码、核对提示、「完成」按钮）；`aborted`/未知 清屏并提示重新生成。详见 §5.7.5（v2.13） |
| **D-81** | **P0 数据丢失错觉** | **分组视图下新建条目「保存就消失」**（#530-W3 走查，主人实测 v1.2.10 桌面）：选中分组 → 新建登录/笔记 → 保存 → 编辑器关闭、列表没有、详情区空白。**两层叠加，条目其实没丢（在根目录）**：① `ItemEditor.svelte:34` 新建时 `folderId` 恒 `null`，**不预填当前选中分组** → 条目存进根目录；② `vault.load()` 按 `selectedFolderId` 过滤 → 刚存条目不在返回列表 → `selectedItem` getter 找不到。触发条件 = 选中了分组再新建（`selectedFolderId = null` 时一切正常，故 §② CRUD 首次走查才暴露） | ① 编辑器预填 `vault.selectedFolderId`（编辑路径不变）；② store 硬保证「保存成功的条目必须立刻可见且选中」：`createItem`/`updateItem` 成功后条目不在当前过滤列表 → 按纯函数 `save-visibility.ts::revealFilterAfterSave` 回落（只清藏住它的过滤位）并重拉、保持选中；连带 `ItemList` 搜索框经 `queryEpoch` 与 store 同步（防迟到防抖写回）。详见 §5.4.1（v2.15） |
| **D-83** | **P0 功能不可用** | **#531 拖拽松手结算全部静默 no-op**（主人真机 v1.2.13 桌面实测，发布当晚）：拖条目/分组「有边缘动画但没有拖动」——插入指示线（`drop-above/drop-below`/`drop-hover`）拖拽期间实时渲染正常，但松手后排序不动、跨分组不动。**根因是结算时序**：`dnd.svelte.ts` 的 `finish()` **先** `drag.end()`（把共享 `drag.state` 清成 null）**再** `callbacks.onDrop()`，而两个消费端（`ItemList.handleItemDrop:134` / `FolderSidebar.handleFolderDrop:108`）都在 onDrop **同步段首行**读 `drag.state` 取投放目标 → 恒为 null → 条目排序钉住 / 跨分组移动 / 分组排序三类投放全部落空且无任何报错。`onCancel` 路径不调 onDrop，不受影响 | 结算时序抽成导出函数 `finishDrag(draggedId, onDrop)`——**先 onDrop 后 `drag.end()`**（注释写明顺序不可交换），控制器 `finish()` 只调它；`end()` 顺带补 x/y 复位；新 `src/lib/dnd-settle.test.ts` 3 例防回归（onDrop 时目标仍在 / 回调返回后全清空 / x-y 复位），前端 253→256 例（v2.16.1） |

---

## 8. 自绘标题栏（decorations:false，v1.6）

> 起因（D-36）：原生标题栏的图标 + `PasswordMS` 与页面内顶栏的品牌标 + `PasswordMS` **上下重复**，观感怪。
> 主人裁定：**去掉原生标题栏**，右上角三钮与窗口拖动自己实现。

- `src-tauri/tauri.conf.json` 主窗口加 **`"decorations": false`**；`resizable` 保持 `true`
  （tao 在 Windows 上对无边框窗口仍做边缘命中测试，可拖边缩放；**此项必须真机验收**）。
- `src-tauri/capabilities/default.json` 新增：`core:window:allow-minimize` / `allow-toggle-maximize` / `allow-close` / `allow-start-dragging`。
- **窗口控制**：`common/WindowControls.svelte`（最小化 / 最大化·还原 / 关闭），46×40 命中区，
  悬停 `--pms-bg-2`，**关闭钮悬停转 `--pms-danger` + `--pms-danger-ink`**（与 Windows 习惯一致）。
  安卓上**整个不渲染**（UA 判 `Android`）—— ⚠️ 不能改用 `<768px` 媒体查询：桌面窗口缩到 360px 时
  若按钮也消失，窗口就关不掉了。
- **拖动区** = 各屏头部容器挂 `data-tauri-drag-region`（Tauri 运行时内置处理，**双击即最大化/还原**）：
  `MainLayout` 的 `.topbar`、`LockScreen` 的 `.lock-screen` + `.win-chrome`、`CreateVaultWizard` 同款。
  该属性只在「点中的正是带属性的元素」时生效，子级按钮/输入框不受影响。
- **顶条 `.win-chrome`**（锁屏 / 建库向导）：`position:fixed; top:0; left:0; right:0; height:40px`，
  右对齐放 `ThemeToggle` + `WindowControls`，背景透明（底下就是页面背景）。
- `WindowControls` 在主界面**必须是 `.topbar` 的直接子级、且排在最后**——绝不能塞进 `.topbar-right`：
  那条在 `<768px` 会被 `display:none`，而桌面窗口缩窄时恰恰最需要关闭钮。
  右对齐靠既有单数 `margin-left:auto`（桌面 `.topbar-right` / 移动 `.topbar-more-wrap`，同一时刻只有一个可见）。
- **品牌标单一事实源**：`common/Logo.svelte`（A · 盾与钥匙孔）与应用图标同一份矢量；
  改形状只改它一处，再跑 `npx tauri icon` 重出全套图标。

## 9. 验收清单

- [ ] `npm run test` 全绿（含 i18n 键位校验；新增 key 已同步全部四个字典 zh-CN / zh-TW / en-US / th）
- [ ] `npm run check`（svelte-check）零 error
- [ ] `npm run check:css`（`scripts/check-css-contract.mjs`）**0 FAIL**
- [ ] `npm run build` 成功
- [ ] **暗色与浅色两套都截图检查**（用浏览器打开 `dist/`，不依赖 Tauri）
- [ ] 解锁页与创建页：**输入框是一行**，眼睛在最右端；输入框高度正常（≈42px）
- [ ] 点眼睛：**星号→明文**时图标从"带斜杠"变"睁眼"（语义方向正确）
- [ ] 两页的 label 与 placeholder 文案**不相同**
- [ ] 确认主密码框与主密码框**外观同构**
- [ ] 全局搜索无残留旧类名：`.field` / `.icon-btn` / `.btn`（无前缀那批）已全部清除
- [ ] **D-30** 密码框内**只有一个眼睛**（原生的 `::-ms-reveal` 已关掉）
- [ ] **D-31** 点任意已保存条目，右侧**立即切到详情**；**点过"新建笔记"之后再点条目，也必须切过去**（回归重点）
- [ ] **D-32** 原生标题栏颜色**跟随应用主题**，标题栏与内容之间无硬切缝；切到"浅色"时标题栏也变浅
- [ ] **D-34** 生成器能从**两个入口**打开（密码框右侧骰子图标 / 顶栏「生成密码」）；位数可自定义；**四类字符全不勾时生成按钮禁用**
- [ ] **出包**：exe 三条硬验证通过（产物比源码新 · 新哈希名在包内且旧哈希名消失 · 体积门禁 EXIT=0）
- [ ] 无任何外部资源引用（`grep -r "http" src/` 只应命中 schema 或注释）
- [ ] **D-35** 移动端（390×844 视口）：主界面 `documentElement.scrollWidth ≤ 390` 且无横向溢出元素；底部导航 5 个 tab 可切换且与顶栏行为一致；分组抽屉可开、可点遮罩关、选分组后自动关；点列表条目进入详情屏、返回箭头回列表屏（单栏推进）；「⋯」菜单可开、点菜单外可关
- [ ] **D-36** 自绘标题栏（**真机验收，无头环境测不了原生窗口行为**）：窗口无原生标题条；顶栏/锁屏空白处按住可拖动窗口、双击可最大化还原；右上角三钮可用、关闭钮悬停变红；拖窗口四边可缩放；桌面窗口缩到 360px 宽时三钮仍在；锁屏与建库向导顶部同样有三钮；安卓 APK 里不出现三钮
- [ ] **D-37** 桌面顶栏收缩：切英文后，1100px / 1080px 窗宽下顶栏**不折行、整页无横向滚动条**；缩到 850px 时 tab 变纯图标；拉宽到 ≥1200px 右区恢复「图标+文字」；中文下 1100px 观感不变
- [ ] **D-38** 顶栏信息架构：桌面顶栏右区只有 `密码生成`/`锁定` 两钮（无语言/导入）；语言与主题在设置页「外观」组、导入在「数据」组且弹窗可用；设置页分组顺序为 外观→安全→体检→同步→数据→设备→改密码→库文件；移动端「⋯」菜单只剩 生成/锁定 两项
- [ ] **桌面端红线**（1100×720）：三栏布局（分组 240 / 列表 300 / 详情 flex）与移动端改造前**逐像素一致**；顶栏导航与右区按钮照旧、无底部导航、无汉堡按钮
- [ ] **D-60**（v2.5 已实现前端批）：附件改依附父条目——列表「添加附件」入口已撤销（回退 D-59 入口部分）；登录/笔记详情页附件区已上线（区头「添加附件」+ 附件行：文件名/MIME/大小 + 导出/移除，导出保留明文风险二次确认与待同步态，移除二次确认）；`AttachmentView` 已改造为附件区组件；流程逻辑 `runAttachmentImport(deps, itemId)` 改按父条目注入（守卫四分支单测照过）；i18n `item.new_attachment` 已删、`attach.add/remove/remove_confirm_*/section_title` 中英双份、`err.attachment_limit` 已补。**回收站父条目附件数徽标本批未做**（Trash 组件，随核批补）
- [ ] **D-61**（v2.6 已实现）：设置页「自动锁定」换档后**重启应用仍保持**（`get_auto_lock` 读回）；无操作达档位时**真的锁屏**（不是只弹 toast）；锁屏后再解锁可继续使用；安卓端切后台超过档位、回到前台**立即锁屏**，未超档则后台时间计入闲置；无「从不」档
- [ ] **D-63**（v2.7 已实现）：创建向导勾选行的「免责声明 / 用户协议」**可点击**且**点它们不会改变勾选状态**（判据：两个链接是 `label.pms-check` 的兄弟节点，`openLegal()` 只写弹窗状态；`src/lib/legal-ui.test.ts` 8 例 + 对抗实验「把链接塞回 label → 2 例变红」守门）；两份文档**有实质内容**（免责 6 条 / 协议 8 条，每条 1–2 段）；弹窗语言跟随 UI 语言切换；设置页「关于」显示**运行时版本号**（非硬编码）、作者 hengletime、MIT、项目主页可打开且过 `isSafeUrl` 白名单
- [ ] **D-66**（v2.9 已实现）：**设备可辨识 + 同步状态不串台**
  - **忙碌态隔离**（核心判据）：设备页有 A、B 两台时点 A 的「立即同步」→ **只有 A 转圈并显示「正在同步…」**，B **保持「立即同步」文案、仅置灰不转圈**；期间 B 点不动（并发重同步被拦）；A 完成后两个按钮同时恢复可点。判定由 `syncButtonState()` 5 例单测守门（含「三台设备只有一台转圈」与「空字符串 id 不误命中」）。
  - **重命名**：点设备行铅笔 → 名称变输入框（初值 = 当前名，已聚焦全选）→ 改名 + Enter（或「保存」）→ success toast `sync.renamed`，行内名称立即更新；**重启应用 / 切页回来仍在**（后端持久化）。
  - **空名不可保存**：清空输入框 → 「保存」按钮 disabled；此时按 Enter 无任何请求（不发 IPC、无 toast）。
  - **超长不可保存**：粘贴 22 个汉字（66 字节 > 64 字节上限）→ 保存按钮 disabled + 就地红字提示 `sync.rename_too_long`；21 个汉字（63 字节）可正常保存。
  - **Esc 取消**：改名后按 Esc → 恢复原名、不发请求。
  - **一次只编辑一行**：A 行处于编辑态时点 B 行铅笔 → 只有 B 行变输入框（`editingId` 单一状态源，A 自动退出）。
  - **配对时间可见**：设备行副行顺序为 指纹 → **配对时间** → 上次同步，配对时间为格式化时间（`formatDateTime`）。
  - **本地昵称**：改名**不影响**对端手机上的设备名，也不改变「上次同步 / 待同步变更」任何计数（昵称只存本机设备表；不进 wire 由 `export_changes` 的 ITEM/FOLDER 白名单结构性保证，core 测试 `rename_touches_only_device_name` 断言 `last_sync_at` / `paired_at` / 指纹 / 共享密钥全不变）。
- [ ] **D-67**（v2.9 已实现）：**设备行「指纹」必须是真指纹，不得用 `device_id` 冒充**
  - **形式判据**：设备行「设备指纹」显示的是 **16 位小写 hex**（`sha256(对端长期身份公钥)` 前 8 字节），不是 36 位带连字符的 UUID。
  - **口径一致性**：手机端该值取自**二维码 `QRPayload.fp`**（长期身份指纹，D-69 修正；不再取临时公钥指纹）；电脑端因 HELLO 只带临时公钥而**留空**（backlog D-70 再补），前端只负责如实显示。
  - **`device_id` 不再当指纹**：完整 UUID 只出现在指纹的 `title` 悬停提示里，副行不单独占一行。
  - **空指纹整行不渲染**：构造一条 `fingerprint` 为空串的配对记录（老记录无该字段时 `#[serde(default)]` 的结果，以及电脑端的手机行）→ **该行整行不渲染**，既不留空白、**也不回退显示 `device_id`**（D-69 改口径：回退 UUID 正是本条要消灭的行为）。
  - 判定收敛在 `hasSessionCode()`（非空判定）+ `fingerprintText()`（仅 `trim`，无回退分支），单测覆盖：正常 / 前后空白 / 空串 / 纯空白 / `null` / `undefined`。
- [ ] **D-69**（v2.9 已实现，修正 D-68 首版）：**配对两屏显示同一次会话的会话码，中间人核对可执行**
  - **电脑屏**：点「生成配对二维码」后，二维码下方出现「本次配对核对码」= `PairingDto.session_code`，**16 位小写 hex**，来源于 `SyncServer::session_code()`（`sha256(host_eph 公钥)`）。**手机连上之前就有值**（临时密钥在 `SyncServer::start` 一次生成）。
  - **手机屏**：`sync_join` 成功（出现「配对成功」toast）后，加入表单下方出现「电脑上的核对码」= 电脑的会话码 + 电脑设备名 + 核对提示；**之前不显示**（进门时 `lastPairSessionCode` 已清空）。
  - **两屏必然同值**：同一对设备配对时两屏**逐字相同**——两侧算的是同一把 `host_eph` 公钥（`Accept.host_pub` 就是它），不是「一般相同」。core 测试 `pairing_session_code::both_screens_show_the_same_session_code` 钉住。
  - **会话码 ≠ 设备指纹（硬）**：会话码每次配对都变（`session_code_rotates_but_device_fingerprint_is_stable` 钉住），因此**不得**把它当设备身份、不得在文案里叫「设备指纹」、不得写进 `PairedDevice.fingerprint`。
  - **缺失不显示空块**：`PairingDto.session_code` / `PairResultDto.session_code` 为空或缺失（老后端过渡期）时，核对块**整块不渲染**，不出现「核对码：」后面空白的半截 UI（`hasSessionCode()` 单测守门：正常值 / `null` / `undefined` / 空串与纯空白）。
  - **不一致的处置可见**：两屏都显示 `sync.session_code_hint`，文案里含「立即解除配对并重试」的具体动作；手机屏用 `.pms-hint.is-warning`（警示色），电脑屏用 `.pms-meta`（中性）。
- [ ] **D-71**（v2.11 已实现）：**Android 导出附件不再报「文件读写出错」**
  - **真机判据**：详情页附件行点导出 → 明文风险二次确认 → 系统保存对话框选位置 → **出现成功 toast**，且用系统「文件」App 打开该位置能读到原文件（大小与源一致）。
  - **不走文件系统 API**：`dest_path` 以 `content://` 开头时后端必须经 provider fd 写入（`src-tauri/src/content_uri.rs::write_bytes`，`OpenOptions` read=false + write=true + **truncate=true** → `"wt"`）。**判据：文件比同名旧文件短时不得残留尾部旧字节**（不 truncate 的典型症状）。
  - **桌面不受影响**：非 `content://`（Windows 任意路径）仍走 `std::fs::write`，导出结果与 v1.2.4 一致。
  - **读侧同源**：Android `import_attachment` 的 `content://` 源码同样经 `read_to_bytes`（上限 `MAX_ATTACHMENT_BYTES` + 1 字节，超限 `ERR_ATTACHMENT_TOO_LARGE`，不得先整份读进内存）。
  - **capabilities 不得改动**：Rust 侧直调插件 API，不经 IPC 权限层。
- [ ] **D-72**（v2.11 已实现）：**「取消不是错误」唯一口径 + 导入格式说明与 CSV 模板**
  - **取消安静**（三处同判）：附件导出 / 导出保险库 / 下载模板，在保存对话框点取消 → **无 toast、无写盘、无控制台未捕获 rejection**（`vault-export` 那条历史上是 unhandled rejection，`runVaultExport` 必须**正常 resolve** 且 `invoked === false`）。
  - **失败必红**：对话框确认后写盘真失败（如磁盘只读）→ error toast，文案取 `toAppError(err).messageKey`，**不得被 catch 吞掉**（`ImportWizard` 那条历史上是空 catch）。
  - **判定单点**：三处一律走 `src/lib/save-dialog.ts::runSaveToDestination`，组件内**不得**再出现 `catch {}` 或 `dest === null` 判断。try 里**只放对话框调用**——取消时 `write` **一次都不得被调用**（单测钉住）。
  - **空串按取消**：`typeof dest !== 'string' || dest.length === 0`，不只判 `null`。
  - **导入面板说清格式**：格式单选下方常显 `import.supported_hint`（`.pms-meta`）+ `import.template_hint`（`.pms-hint`），再下面是 `.pms-btn.is-ghost` 的「下载 CSV 模板」按钮；**说明出现在解析按钮之前**，且不随格式切换隐藏。
  - **模板可导入**：下载得到的 CSV **直接喂回本应用必须 0 跳过**（`total == 2` / `items.len() == 2` / `skipped == 0`），表头 10 列与 `KNOWN_COLUMNS` 逐字同序，示例行**不得溢出到 `custom_fields`**——全部由 `crates/passwordms-core/tests/import_template.rs` 锁死。
  - **模板内容不在前端拼**：固定取 `passwordms_core::import::template::CSV_TEMPLATE`。
- [ ] **D-73**（v2.12 已实现）：**设置页五个症状全部消失**（**两处独立根因**：④ 及之前是 `$effect` 首行深拷抛错导致其后全哑；⑤ 是窄屏规则未限定适用范围。两者同轮发现、**不同根因**，修好深拷不会让⑤自动消失）
  - **根因判据（结构性）**：`Settings.svelte` 的 `$effect` 里 `draft` 的深拷**只允许** `$state.snapshot(settings.value)`。全仓（`src/**/*.{svelte,ts}`）**不得出现** `structuredClone` 的调用形态——对 `$state` 代理必抛 `DataCloneError`，而抛在 `$effect` 首行会让同一 `$effect` 内**后面所有逻辑静默不执行**（`src/lib/state-clone-guard.test.ts` 两例守门：扫描面有效性 + 零命中）。
  - **症状① 版本可见**：「关于」分组显示**真实版本号**（如 `1.2.7`），不是 `—`。
  - **症状② 库文件卡片有内容**：「保险库文件」卡片显示**路径 + 大小**两项非空（路径等宽 `.set-mono` 可折行），且**标题下方有说明行** `settings.vault_file_hint`（回答「这个文件是干什么的」），不再只有标题。
  - **症状③ 自动锁定档位真实**（客户尚未发现的隐藏症状）：把档位改成 **15 分钟** → 重启应用 → 设置页仍显示 **15 分钟**（缺陷期为恒显示 5 分钟）；改回 **1 分钟** 同样成立。判据是「显示值 == 库里 `auto_lock_minutes`」。
  - **症状④ 失败不再伪装成空**：构造库文件信息读取失败（后端返回错误）→ 卡片显示 `settings.vault_file_failed`（**有一行字**），**不得**是空白卡片；且此时版本号与自动锁定档位**仍正常显示**（逐块失败隔离的回归锚点）。
  - **失败隔离判据（可单测）**：`loadSettingsPageSide` 中任一 load reject 时，其余两项**必须仍有值**；`fileInfoFailed` / `autoLockFailed` 成功路径必须为 `false`。`src/lib/settings-page.test.ts` 5 例（三者全成功 / 库文件失败 / 版本失败 / 档位失败 / 档位返回非有限数）。
  - **导入按钮文案消歧**：按钮文案为「选择要导入的文件」/ "Choose a file to import"（不再是「选择导出文件」——本应用导出的是 `.vault` 加密备份，不从该入口导入）。
  - **「预览」可解释**：未选文件时按钮**可点**，点击出现 `import.choose_first` 提示（toast），**不得**再「置灰且无任何说明」。
  - **症状⑤ 手机「关于」组排版不散乱**（客户 1.2.6 真机截图追加）：390px 下「作者」与 `hengletime` 必须**同一行两端对齐**（标签左、值右），**不得**被拆成上下两行左对齐；「许可证 / MIT License」同理；三个按钮（项目主页 / 免责声明 / 用户协议）必须**横向排布**（必要时换行），**不得**被拉成三条满宽竖排。1100px 桌面下**逐像素不变**。结构判据由 `src/lib/settings-row-type-guard.test.ts`（8 例）锁死：恰好 3 条 `.is-info` + 1 条 `.is-actions`、全文件仅此 4 条 `is-*`、覆盖规则在同一 `@media` 块内且位于 `.set-row` 之后、媒体块外无 `is-*` 规则（⇒ 桌面不变）、无 `:has(`。
  - **症状① 在手机上也成立**：390px 下「关于」第一行显示**真实版本号**（如 `1.2.7`）而不是 `—` —— 与症状① 同源同判据，窄屏不额外退化。
  - **档位失败有出口**：`autoLockFailed = true` 时下拉旁必须出现 `settings.auto_lock_failed` 一行字（否则「5 分钟」会被误读成真实设置）。
- [x] **D-75**（v2.13 已实现）：**配对成功后会话码留在电脑屏上，两屏核对第一次真正可做**
  —— **2026-10-22 真机实测通过（v1.2.10）**：主人重新配对时电脑端保留核对码
  - **核心判据（时序）**：手机扫码成功后，**电脑屏不得收起面板** —— 面板须转为「配对成功（待核对）」态，里面仍是**本次会话**的会话码；此时手机屏的核对块也正好出现，两者逐字相同。
  - **结构判据（可单测）**：`pairing://ended` 的载荷必须经 `nextPairingState()` 翻译；`completed → { clearPairing: false, completed: true, ended: false }`；`aborted`/未知 → `{ clearPairing: true, completed: false, ended: true }`。`src/lib/pairing-state.test.ts` 16 例守门（纯函数语义 / handler 先算后清 / 组件分支存在且优先于二维码分支）。
  - **成功态约束**：`pairCompleted && pairing !== null` 时**不渲染二维码**（已扫过，留着会让人以为还能再扫），显示「配对成功」+ `sync.pair_completed_hint` + 会话码（`hasSessionCode()` 判空后）+ 主按钮「完成」（`sync.endPairing()`）。按钮文案 `sync.pair_done`。
  - **退出路径**：点「完成」/ 重新生成二维码 / 取消配对 / 锁定，四条路都必须让 `pairCompleted` 回到 `false`（否则下一轮二维码旁边会挂着上一轮的成功提示）。
  - **失败态不回归**：`aborted` 仍清屏 + 提示 `sync.pair_ended`「请重新生成二维码」。


### 6.14 TOTP 密钥入库校验与展示错误态（D-97，v2.24）

**症状（主人 2026-10-25 实测，v1.2.19）**：新建登录条目，「动态口令」填 `qwer242411321`（随手测试串）。详情页「动态口令」行只剩倒计时在跳（29 秒 → 归零 → 重来），验证码区域**永远空白**；倒计时归零时、以及**每次切回该条目**都弹一条「输入无效。」toast（切几次堆几条）。

**根因（两处缺口叠加）**：

| 层 | 缺口 |
|---|---|
| 编辑器输入端 | TOTP 字段保存前只做 `trim()`，**零校验**——任意字符串都入库。`1` 不在 Base32 字母表（A-Z、2-7）内，Rust 侧 `base32::decode` 必失败（`Validation("totp_secret_invalid")`）。同理 `0/8/9`、以及直接粘贴 `otpauth://` 整串链接（含 `:` `/` `?` `&`）都解不了。 |
| 详情页展示端 | 取码失败只管弹 toast：倒计时照转（纯前端数学，永远「正常」）、不进错误态、不停止重试——**每个 30s 窗口一次 + 每次切回条目一次**。 |

**修法（三层）**：

1. **编辑器入库前真校验（A）**：`save()` 里 `kind === 'login' && totp 非空` 时——先 `normalizeTotpInput(totp)`，再 `await currentTotp(secret)` 试算（调 Rust `totp_now`，**等价 base32 真解码**，前端不重复实现密码学）。失败且 `ERR_VALIDATION` → `totpInvalid = true`：行内红字 `err.totp_invalid`（**点名合法形态**：「字母 A-Z、数字 2-7，不含 0/1/8/9，或直接粘贴 otpauth:// 链接」——只说「输入无效」用户不知道改什么）+ `aria-invalid` + `totpInput.focus()` + **阻止保存**。**环境类错误（非 ERR_VALIDATION）放行**原保存流程——不因 IPC 故障卡死编辑器。草稿 `totp_secret` 取**校验后**的 `totpSecret`（旧版直接读原始输入的路径已删）。
2. **otpauth 链接自动提取（C）**：`src/lib/totp.ts::normalizeTotpInput`——`/^otpauth://///i` 命中即 `new URL` 解析取 `secret` 参数（网站两步验证设置页默认复制格式，现在直接粘就行）；缺 `secret` 参数或解析失败原样返回（交给试算拦截）。
3. **详情页错误态（B）**：取码 catch → `totpInvalid = true`（toast **只弹一次**）→ `tick` 开头 `if (cancelled || totpInvalid) return` 停止重试循环 → 行内渲染 `err.totp_invalid_detail` 红字、**停显倒计时与验证码**。成功取码/切换条目均复位。**错误必须是可见的一行字**——不渲染 = 把失败伪装成「本来就没有」（D-73 教训同源）。

**存量坏数据处理**：本修复不自动清洗已入库的非法 secret（展示端错误态已明确指路）；用户编辑该条目重新保存时，入口校验会要求先修正——届时才能通过保存。

**守门**：`src/lib/d97-totp-guard.test.ts` 13 例——normalizeTotpInput 5 例（原样 / 提取 / 大写 scheme+空格 / 缺参数 / 非链接不误伤）、主例回归（`qwer242411321` 必须 `isValidBase32 === false`）、编辑器结构断言 3 例（校验顺序 / 草稿取校验后值 / 行内错误不静默）、详情页结构断言 3 例（停止重试 / 错误行可见且错误分支无倒计时 / 切换复位）、i18n 两键双语齐备 1 例。

- [ ] **D-97**（v2.24 已实现）：**TOTP 三层守门**
  - 编辑器：非法密钥（含 0/1/8/9、otpauth 整串、空串变体）**保存被拦**，行内红字点名合法形态并聚焦字段；合法密钥与 otpauth 链接照常保存（链接自动提取后入库的是 secret 本体）。
  - 详情页：存量坏密钥 → 一条错误 toast + 行内「动态口令密钥无效，请编辑条目修正」+ 倒计时停显 + 不再重复弹窗。
  - i18n：`err.totp_invalid` / `err.totp_invalid_detail` 双语齐备；zh 文案必须点名 Base32 字符范围。

### 6.15 强度评分改信息熵（D-100，v2.25）

**症状（主人 2026-10-25 实测）**：生成器里拖长度滑块，质量判定（弱→强）纹丝不动。

**根因**：`StrengthMeter.scoreOf` 旧公式 = `字符类数 + 长度加成（≥12 位 +1、≥20 位 +1）`，再 `Math.min(4, base)` 封顶。4 类全开时 base 起步就是 4 → **封顶把长度项整个吞掉**，任何长度都「很强」；3 类时长度也只在 12/20 两个边界点起效。

**修法**：评分改为**信息熵估算**——`bits = len × log2(字符池)`，池按实际出现的字符累计（小写 26 / 大写 26 / 数字 10 / 符号 33），阈值 `<40` 弱 / `<60` 一般 / `<80` 强 / `≥80` 很强。实现抽到 `src/lib/strength.ts`（组件零逻辑，可真单测）。

| 场景（生成器） | 旧评分 | 新评分 |
|---|---|---|
| 4 类 6 位（≈39 bits） | 很强（封顶） | 弱 |
| 4 类 8 位（≈52 bits） | 很强 | 一般 |
| 4 类 10 位（≈66 bits） | 很强 | 强 |
| 4 类 13 位（≈85 bits） | 很强 | 很强 |
| 纯小写 8 位（≈38 bits） | 弱 | 弱 |
| 纯小写 20 位（≈94 bits） | 强 | 很强 |

**已知边界（文档化）**：熵估算假设字符均匀随机——对人类口令（词典词组合、键盘序列）会**高估**，与旧公式同等局限；本指示仅提示不强制（U6 裁定不变），主密码强度确认流程不受影响。

**守门**：`src/lib/d100-strength.test.ts` 5 例——典型锚点（4 类 6/8/10/13 位 = 1/2/3/4）、长度单调不减且 6→16 位至少升 2 档、纯小写 8 位弱 / 20 位很强、空串 0、组件源码必须引用 `lib/strength` 且不含旧公式。

- [ ] **D-100**（v2.25 已实现）：**强度评分 = 字符池 × 长度的熵**
  - 生成器 / 编辑器 / 主密码向导三处共用同一 `StrengthMeter`，拖长度逐档可感知；
  - 文档化高估边界；不改变任何强制校验（U6）。

### 6.16 体检重复密码分组显示（D-99，v2.26）

**症状（主人 2026-10-25 实测）**：体检报告「重复密码 6」，chip 区 6 个徽章一排铺开——看不出 6 条是**一组共用同一密码**还是**几组各用各的**；且其中 5 个 chip 显示裸 UUID 而非条目标题。

**根因（两处叠加）**：

| 层 | 缺口 |
|---|---|
| 后端 `health.rs` | 重复判定按密码分组后**立即拍平**进 `reused: Vec<String>`——分组信息出生即丢，UI 从数据结构上不可能还原「几组」。建议文案「有 N 条条目重复使用同一密码」也是拍平口径，多组时误导。 |
| 前端 `HealthCheck.svelte` | `titleOf` 从 `vault.items` 解析标题——它是**按当前筛选过滤后的列表**（分组/搜索/收藏），不在当前视图的条目解析不到 → 回退显示裸 UUID。 |

**修法（两层）**：

1. **后端分组结构**：`HealthReport` 新增 `reused_groups: Vec<Vec<String>>`——每组 = 共用同一密码的条目 ID 列表（≥2 条才成组）。确定性输出：组内按条目 ID 升序、组间按组内最小 ID 升序（HashMap 遍历序不稳定，不排则快照比对不可复现）。拍平视图 `reused` **保留**（统计卡与扣分逻辑零改动），不变量：拍平 groups 排序去重 = reused（测试锁死）。建议文案改「有 **G 组**条目重复使用同一密码（共 N 条）」。
2. **前端全量映射 + 按组渲染**：`run()` 里体检与全量条目 `Promise.all` 并行拉取（`api.items.list(null, false, undefined)`），建 `titleMap` 供 `titleOf` 查询——不再依赖当前筛选视图；重复区每组一个小框（`.health-group` 边框圆角）+「第 {n} 组」标签（`health.reused_group` 双语）+ 组内 chip。弱口令/超期区沿用单层 chip（它们无分组语义，但标题映射已随全量修复受益）。

**存量数据兼容**：字段为纯新增，旧版本读新报告只多一个忽略字段；同版本前后端一起发，无迁移。

**守门**：`src/lib/d99-health-groups.test.ts` 4 例（全量映射结构 / Promise.all 并行 / `reused_groups` 分组渲染 + 旧拍平路径已删 / i18n 双语齐备）+ `health_and_generator.rs` 2 例（单组形态 + 3+2 多组回归与拍平不变量）。

- [ ] **D-99**（v2.26 已实现）：**体检重复密码按组显示**
  - 「6 条重复」现在一眼可辨是一组还是多组；不在当前视图的条目显示真标题不再裸 UUID；
  - 建议文案按组数措辞；确定性输出（组内/组间升序）可快照比对。

### 6.17 搜索框一键清空（D-101，v2.27）

**症状（主人 2026-10-26 反馈）**：搜索词只能逐字符退格删除，长词麻烦——搜索框要一个 ❌ 一键清空。

**设计**：

- **渲染条件**：`query.length > 0` 才在 `.pms-input-group` 尾部渲染（flex 布局 input `flex:1`，按钮天然靠右）；空态不渲染、不占位。
- **视觉**：22px 圆形 hitbox、既有 `close` 图标 14px、默认 `--pms-faint`；hover = `--pms-danger` 图标 + `--pms-danger-soft`（红 10% 半透明，双主题自动适配）淡底。`aria-label` + `title` 双通道 `common.clear_search`（清空搜索 / Clear search）。
- **行为（防 D-81 冲突）**：点击 = ① 取消挂着的 220ms 防抖 timer（**必须最先做**，否则旧词随后 `setQuery` 写回搜索框）→ ② `query = ''` → ③ `void vault.setQuery('')`（与手输路径同源，走 `load()` 刷新）。**不推进 `queryEpoch`**——epoch 是 D-81 用来区分「外部重置（同步显示）」与「自己打字中（不动输入框）」的信号，自发清空推进它会误触发显示同步。

**守门**：`src/lib/d101-search-clear.test.ts` 5 例（空态渲染条件 / clearTimeout 先于 setQuery / 不碰 epoch / aria+title+close 图标 / i18n 双语齐备）。

- [ ] **D-101**（v2.27 已实现）：**搜索框一键清空**
  - 词非空出 ❌、一键清空即刷新列表；防抖先取消；可及性双通道。

### 6.18 四语言（繁體中文 / ไทย）+ 首启跟随系统语言（D-102，v2.28）

**需求（主人 2026-10-26）**：①程序在不同语言的电脑上安装使用，**首次启动跟随电脑语言**（非简体中文切英文）；②新增**繁体中文**与**泰文**。

**设计**：

- **首启检测（D-102）**：`readStoredLocale()` 仅当 localStorage **从未存储过语言**（全新安装）时检测 `navigator.language`（Tauri WebView 恒有），经纯函数 `mapSystemLanguage()` 映射：`zh-Hant*/zh-TW/zh-HK/zh-MO → 繁體`、其余 `zh-* → 简体`、`th* → ไทย`、`en* → English`、**未支持语言一律兜底 English**（主人口径「非简体中文切换到英文」）。老用户已存语言不受影响；`navigator` 缺失（Node 测试环境）落 `DEFAULT_LOCALE`。
- **四语言清单**：`LOCALES = ['zh-CN', 'zh-TW', 'en-US', 'th']`；`Locale` 联合类型扩四值；设置页语言钮四枚、显示名用母语书写（专有名词不翻译）：中文 / 繁體中文 / English / ไทย。
- **字典**：`zh-TW.ts` / `th.ts` 各约 250 键全真翻（繁体按台湾用语：軟體/網路/匯入匯出/建立/裝置/剪貼簿/登入/使用者名稱/QR 碼/資源回收筒）；键结构以 zh-CN 为唯一事实来源，`i18n.test.ts` 键一致性测试**从两语扩展为四语全比对**。
- **法律文本（A 案拍板）**：`legal-content.ts` 新增繁体/泰文两篇 × 2（免责 6 条 + 协议 8 条结构不变，`LEGAL_UPDATED` 不变）；守门测试扩四语言，另有「泰文文档必须含泰文字符」字形守门与跨语言串语粗判（简繁同形标题如「使用限制」属合法，逐条比对只在跨文字体系间做，简繁之间比文档标题）。
- **测试环境池**：`vite.config.ts` 固化 `test.pool = 'vmForks'`（2026-10-26 默认 forks 池 worker loader 出现与项目代码无关的状态异常，VM 池实测全绿后固化；详见 vite.config.ts 注释）。

**守门**：`src/lib/d102-system-locale.test.ts` 6 例（简体变体 / 繁体变体 / 泰语 / 英语 / 未支持语言兜底 / 大小写不敏感）+ `i18n.test.ts` 四语言键集一致 + 空串禁令 + `legal-content.test.ts` 四语言结构与字形守门。

**泰文母语审校（v2.30，D-104）**：泰国朋友审阅后两类修订——① Health 语境（体检页查密码安全，非人体健康）：สุขภาพ → ตรวจสอบรหัสผ่าน 语境，波及 `nav.health` / `tab_health` / `health.title` / `health.run` / `health.score` / `health.good` / `settings.group_health` 七键；② 生成器命名从「ตัวสร้างรหัสผ่าน」（生成器）简化为「สร้างรหัสผ่าน」（泰文软件惯例）。另采纳 5 处润色（รูปลักษณ์ / เปิดโปรแกรม / 单位标注 / อัตโนมัติ / stale_days 语义补全），并按语境**保留**通用短形（บันทึก / ซิงค์ / สว่าง·มืด / แข็งแรง）。键集零变化。

- [ ] **D-102**（v2.28 已实现）：**四语言 + 首启跟随系统语言**
  - 全新安装 navigator.language 映射四语言、未支持兜底英文；老用户已存语言不受影响；设置页四枚语言钮。

### 6.19 详情页类型徽章防挤压（D-103，v2.29）

**症状**：手机端条目名称太长时，详情页标题右侧「登录/笔记」徽章被挤成逐字换行竖排（「登」上「下」）。

**根因**：`ItemDetail.svelte` 头部 `.detail-titles` 为横排 flex（`h2` 标题 + `.pms-badge` 徽章）。`h2` 虽写了 `overflow: hidden; text-overflow: ellipsis; white-space: nowrap`，但 **flex 子项默认 `min-width: auto`**——最小宽度被内容撑到全宽，`h2` 不收缩、ellipsis 永不触发，剩余空间全被吃掉，徽章宽度不足时文字逐字折行。

**修法**（scoped，不动全局 `.pms-badge`）：

1. `h2` 加 `min-width: 0` —— 允许 flex 收缩，ellipsis 真正生效（长标题截断显示省略号）。
2. `.detail-titles :global(.pms-badge)` 加 `flex-shrink: 0` + `white-space: nowrap` —— 徽章永不被压缩、永不高换行。

**守门**：`src/lib/d103-detail-badge.test.ts` 2 例（源码断言 `min-width: 0` 与 `flex-shrink: 0` + `nowrap` 两条规则并存；源码读取走 `import.meta.glob('?raw')`，svelte-check 不认 `node:fs`）。

- [ ] **D-103**（v2.29 已实现）：**详情页类型徽章防挤压**
  - `h2` 可收缩截断；徽章禁压缩禁换行；桌面/移动端同一份 scoped 规则，行为一致。

### 6.20 外部审查修复（D-106，v2.32）

豆包外部审查（`docs/PasswordMS-审查报告-2026-10-27.md`，58 条）经逐条核实后，涉及 UI/前端的修复：

| # | 审查编号 | 修复 | 位置 |
|---|---|---|---|
| 1 | 5.2-1 | 窗口三钮 aria/title 入语言包（`common.minimize` / `maximize_restore` / `close_window`） | `WindowControls.svelte` |
| 2 | 5.2-2 | TOTP 输入框 placeholder 入语言包（`editor.totp_placeholder`） | `ItemEditor.svelte` |
| 3 | 3.5-1/2 | `vault.load()` 请求序号守卫：过期响应不写状态 / 不 toast / 不复位 busy | `vault.svelte.ts` |
| 4 | 3.5-3 | 剪贴板到期清空失败 toast（`err.clipboard_clear_failed`） | `clipboard.ts` |
| 5 | 3.5-4 | 剪贴板 TTL 双层钳制 5–120（前端 onchange/保存 + 后端 clamp） | `Settings.svelte` + `settings_cmd.rs` |
| 6 | 3.5-5 | 启动状态查询失败显式化（`statusError` + 重试面板，不进创建向导） | `session.svelte.ts` + `App.svelte` |
| 7 | 3.5-6 | 拖拽 `clickSuppressed` 兜底复位（`setTimeout(0)`） | `ItemList.svelte` |
| 8 | 5.2-9 | 泰文日期佛历 DD/MM/YYYY（年 + 543） | `constants.ts` |

Rust 侧（容器掉电一致性 / 同步大小守卫 / 解锁统一化 / 配对水位 / resync 锁 / 剪贴板 TTL）见 `docs/02-arch.md` §3.14 D-106 段；core 守门 `d106_external_audit.rs` 3 例。

**不修项**（核实后维持原状，理由见汇报）：IPC 路径白名单（3.4-4，本地单用户威胁模型下为接受风险）、Rust 侧 auto-lock 兜底线程（3.4-6，跨层改动另行排期）、read_blob 错误码细化（3.1-10，新增错误变体牵动四语言映射）、workspace cargo test（Q3，windres 环境缺 DLL，属环境项）。

### 6.21 更新检查 + 审查余项 UI（D-107/D-108，v2.33）

**D-107 更新检查**（方案 B，主理人裁定：自动检查**默认关** + 手动按钮；只提醒不下载）：

- 位置：设置 → 「关于」卡片，版本行之下。纯逻辑抽 `src/lib/update-check.ts`（版本比较 / GitHub 应答解析 / 24h 节流全为可注入单测的纯函数），UI 只做编排。
- 结构：`自动检查更新` 开关行（checkbox，默认不勾）→ 一句说明（关=零网络请求；开=每天最多一次、不含用户数据）→ 状态行（检查中 / 已是最新 / 检查失败小字）→ 发现新版本时警告条 `发现新版本 v{version}` + `前往下载` 按钮（跳 GitHub Releases，走 open_url 白名单链路）→ `检查更新` 手动按钮并入既有 is-actions 按钮行（homepage 左侧）。
- 手动检查失败显式提示、自动检查失败静默（GitHub 不可达不该打扰用户）；自动检查查到过的新版本号暂存 localStorage，再进设置页直接显示提示。
- 零 Rust 依赖：WebView `fetch` 直连 API（CSP `connect-src` 显式放行 `https://api.github.com`），不加 HTTP crate（§8.1 上限 + 10MB 体积门禁）。
- 用户协议「数据与隐私」节四语言补披露句；设置页 is-actions 行 1 → 2（守门测试同步，见 `settings-row-type-guard.test.ts`）。
- i18n 新键（四语言）：`settings.update_auto / update_auto_hint / update_check_now / update_checking / update_uptodate / update_available / update_failed / update_download`。

**D-108 前端余项**：

| # | 审查编号 | 修复 | 位置 |
|---|---|---|---|
| 1 | 3.4-6 前端侧 | Rust auto-lock 兜底触发后：App.svelte 常驻监听 `autolock://locked` → 复位三个 store + 切锁屏（后端已锁定，不再发 IPC） | `App.svelte` + `auto-lock-event.ts` |
| 2 | 3.5-7 | 死命令 `sync_accept_pair` 双侧删除（命令面 59 → 58） | `ipc.ts` / `sync_cmd.rs` / `lib.rs` |
| 3 | 3.5-8 | `saveQuiet` 独立 `quietBusy` 标志（原复用 busy） | `settings.svelte.ts` |
| 4 | 3.5-9 | CSV 表头读取失败 toast（`import.header_read_failed`，原静默重置） | `ImportWizard.svelte` |
| 5 | 3.5-10 | 解锁后五路加载：stores 的 load 加 `silent` 参数（失败上抛不弹 toast）→ MainLayout `loadAfterUnlock()` allSettled + 失败立即整组重试一次 → 仍失败合并一条 `err.load_partial` | `MainLayout.svelte` + 三个 store |
| 6 | 3.4-8 | 库文件卡片显示文件名（`fileInfo.file_name`，不再暴露绝对路径）；四语言 `vault_file_hint` 「路径见下」→「文件名见下/檔名見下/ดูชื่อไฟล์」 | `Settings.svelte` / `types.ts` / i18n×3 |
| 7 | 3.1-10 | `err.attachment_not_found` 入四语言（remove_attachment 附件定位失败） | i18n×4 |

守门：`d107-update-check.test.ts` 9 例 + `d108-audit-guard.test.ts` 12 例（含 capabilities 无 opener、CSP 纵深指令、死代码双侧删除、四语言键与披露句齐全）。全量：vitest 46 文件 362 例 / check 0/0 / CSS PASS；core 225 例 / clippy 0 / fmt 过。

### 6.22 设置页复选框行窄屏回归修复（D-110，v2.34）

**症状**（主人手机实测 1.2.24）：设置页「同步」组与「关于」组的复选框（自动同步 / 启动时同步 / 自动检查更新）跑到对应文字的**下一行**——文字一行、复选框孤零零一块。

**根因**：§6.10（v2.12/D-73）定下的窄屏行类型纪律只登记了两种例外——纯信息行 `.is-info`、按钮行 `.is-actions`。复选框行是**第三种行类型**，既非控件行（无 `.set-ctl` 固定宽输入，不需要堆叠）也非信息/按钮行，却没标任何 `is-*`，被窄屏一刀切 `.set-row { flex-direction: column; align-items: stretch }` 误伤：`.set-label` 与 `.pms-check` 被堆成上下两行。D-107 新增「自动检查更新」行时沿用了裸 `.set-row`（继承同一缺陷），手机升级 1.2.24 后三处同时显形。

**修法**（对齐既有纪律，**不许用 `:has()`**）：

- 新增第三种行类型 `.is-check`：三个复选框行 markup 标 `class="set-row is-check"`；窄屏媒体块内 `.set-row` 之后覆盖回 `flex-direction: row; align-items: center; justify-content: space-between`（文字左 / 复选框右，同桌面）。桌面 ≥768px 基础 `.set-row` 本就是横排，**零桌面规则、逐像素不变**。
- 守门测试 `settings-row-type-guard.test.ts` 扩到 10 例：is-check 共 3 条、覆盖规则在同一媒体块内且位于一刀切之后、窄屏规则含 row + space-between 且无 stretch/column、媒体块外无任何 `.set-row.is-check` 规则；标记行总数 7 → 10。
- 教训入册：**新增设置行必须先归类行类型**——控件行（裸）/ 信息行（is-info）/ 按钮行（is-actions）/ 复选框行（is-check），四类穷尽，别让「第三种行」漏网。

守门：`settings-row-type-guard.test.ts` 10 例全绿；全量 vitest / check / CSS 三门复跑通过。

### 6.23 core 层文案本地化——UI 语言全链跟随（D-112，v2.35）

**症状**（泰国朋友真机实测）：泰文 UI 下健康检查「建议」区显示中文（「密码健康状况良好，继续保持。」）。全仓排查出同病根三处：`health.rs` 4 条建议、冲突副本标题 ` (冲突副本)`、CSV 导入模板示例行——全是 Rust core 硬编码中文。

**修法**（架构裁定：**core 只出数据，前端出文案**，与 D-102 i18n 架构对齐）：

- **健康建议**：`HealthReport` 移除 `suggestions` 字段；`HealthCheck.svelte` 以 `$derived.by` 按 weak / reused_groups / stale 计数组装建议（`health.sug_weak` / `sug_reused` / `sug_stale`，皆空用既有 `health.good`），四语言字典同加。UI 外观零变化，仅文案来源换了。
- **冲突副本后缀**：仲裁「两者都留」的副本标题后缀改由前端传 `conflict.copy_suffix`（四语言： (冲突副本) / (衝突副本) / (conflict copy) / (สำเนาข้อขัดแย้ง)）。**存量副本标题不动**（数据非 UI）。
- **CSV 模板**：「下载 CSV 模板」产出的示例行随 UI 语言（`import.tpl_*` 5 键），表头与引号转义示范结构不变；泰文用户拿到泰文模板。
- 教训入册：**Rust 层禁止出现任何用户可见句子**——core 报错用错误码/键（既有 `messageKey` 机制），展示文案一律前端 i18n；新增 core→UI 文本通道时先问「这句谁翻译」。

守门：core `tests/import_template.rs` 参数化（中文基线 + 泰文双跑全量契约）、`template.rs` 单测（引号/换行注入拒绝）；core 34 组全绿、clippy 0 告警；前端三门绿。**D-112 需两端同版本升级**（`resolve_conflict` 前后端签名联动）。

### 6.24 快速解锁 UI（D-113 P2 / `docs/08`，v2.36）

两处新增，都在既有视觉体系内，**不新造控件样式**。

**① 解锁页（`LockScreen.svelte`）**：主「解锁」按钮**下方**加一枚 ghost 块按钮「使用 Windows Hello 解锁」（`lock.biometric_unlock`，图标复用既有 `unlock`）。

- **显示条件严格**：`biometric_status` 返回 `availability === 'available'` **且** `state === 'ready'`。`stale` / `corrupt` / `absent` / 平台不可用一律不渲染——失效封印点了只会弹一句「已失效」，反而像按钮坏了；重新开启的入口在设置页（那里能看到具体原因）。
- 位置语义：与主解锁按钮同组（`margin-top: .5rem`），与下方「创建新保险库 / 从备份恢复」拉开，避免被读成同级的次要入口。
- 忙碌态复用 `Spinner` + `lock.biometric_waiting`（Hello 弹框可能停留数十秒，必须让用户知道在等什么）。
- 失败提示**不折叠**（与 A-05 的密码解锁相反，理由见 `docs/08` §10）：取消 → 静默不弹；已失效 → 弹具体原因并**重查状态**（按钮随之消失）；需主密码 → 引导语且**不**点亮密码框的红色「解锁失败」。

**② 设置页新增「快速解锁」卡片（`Settings.svelte`）**：位置在「安全」之后、「体检阈值」之前。

- 独立卡片而不是塞进「安全」分组：它是**立即生效的动作**，不归底部「保存」按钮管；混进安全卡片会让用户以为改完要按保存。
- 行结构：状态行标 `.set-row.is-info`（纯信息行，窄屏不堆叠，见 §6.10 / v2.12 纪律）——左「Windows Hello 快速解锁」，右状态文字 `.bio-state`（`text-align: right; max-width: 62%`，**不套 `.set-ctl`**，那 160px 固定宽会把「主密码已修改，快速解锁已失效。」挤成竖条）。
- 状态文案四态：`已开启（{date}）` / `未开启` / `配置已损坏，请关闭后重新开启。` / 四种失效原因各一句（`bio_state_stale_password` / `_vault_key` / `_enrollment` / `_tampered`）。
- 下方两行说明（`.pms-hint`）：功能说明 + **风险说明**（能拿到本机 Windows 登录凭据者可绕过 Hello）。平台不可用时追加一行红字（`is-danger`）+ 开启按钮置灰。
- 开启流程：点「开启」**原地展开**一个主密码输入框（`set-bio-pw`，裸 `.set-row` 控件行，窄屏正常堆叠）+ 确定/取消（`.bio-actions` 横排）。**不弹 Modal** —— 这是一个两字段动作，弹层反而更重。
- 关闭流程：`ui.confirm` 二次确认后直接删封印，**不要求主密码**（降级操作、且忘密码的用户必须能自救）。

i18n：新增 30 键 × 四语言（`lock.biometric_unlock` / `biometric_waiting`；`settings.group_biometric` / `bio_*` 25 键；`err.biometric_*` 4 键 + `err.need_master_password` + `err.bio_invalidated_*` 4 键）。

守门：`i18n.test.ts` 四语键一致性全比对、`i18n-usage.test.ts` 字面量键存在性、`settings-row-type-guard.test.ts` 行类型计数（`is-info` 5 → 6、标记行 10 → 11）、`ipc-contract.test.ts` 命令数 58 → 62 与 `invoke`/`generate_handler!` 双向一致。


### 6.25 快速解锁走查三修（D-114，v2.37）

三条都来自主人真机走查，均为**文案与流程**问题，控件样式零新增（只多一条 `.pms-alert.is-success` 的底色）。

**① 改主密码的成功回执必须留在原地**

| 项 | 旧 | 新 |
|---|---|---|
| 文案 | `common.ok`（= 按钮上的「确定」） | `settings.password_changed`「主密码已修改，下次解锁请使用新密码。」 |
| 载体 | 仅 toast，3 s 自动消失（`TOAST_SUCCESS_MS`） | 「修改主密码」卡片内**常驻**结果条 `.pms-alert.is-success`，带 `check` 图标与 `role="status"`；toast 仍给但不是唯一回执 |
| 清除时机 | —— | 三个密码输入框**任一** `oninput` 即清（避免「已修改」与「修改失败」同屏） |

**② 「快速解锁为什么没了」必须说出来**

- 改密**会**删掉快速解锁封印（§18.5 约束 2，安全上不可退让），所以不能只显示「未开启」。
- 判定：改密**前**取 `bioEnabled`（改密后恒为 false，事后取拿不到真相）→ 成功且原先开着 → `settings.password_changed_bio_off`「主密码已修改。快速解锁已因此关闭——重新开启后才能继续用 Windows Hello 解锁。」
- 同时 `refreshBiometric()`：不重查的话卡片会继续谎报「已开启（日期）」，比不提示更坏。

**③ 平台不可用的文案与出口**

- 根因（`docs/08` §20）：`UserConsentVerifierAvailability` 在「Windows Hello 从未配置」的机器上返回的是 **`DeviceNotPresent`**，而不是 `NotConfiguredForUser`——所以「没有硬件」这句话在绝大多数情况下是**错的**。
- 文案改为只陈述检测结论并留出可能性：`未检测到可用的 Windows Hello（多半是尚未设置，不一定缺少硬件）。`
- 追加一行操作指引 `bio_avail_hint` + 按钮 `bio_open_settings`「打开 Windows 登录选项」。仅当 `availability ∈ {not_present, not_configured}` 时渲染（`disabled_by_policy` 开设置页也改不了，不给假出口）。
- 命令 `open_biometric_settings` **无参数**、URI 是 Rust 编译期常量 → **不放宽** `open_url` 的 http/https 白名单（PRD F-05）。
- 窗口 `focus` 时静默重查：用户去系统设置开通后切回来，卡片立刻从「未检测到」变「可用」，不用重启应用。

四语言新增/改写：`bio_avail_not_present`（改写）、`bio_avail_not_configured`（改写）、`bio_avail_hint`、`bio_open_settings`、`password_changed`、`password_changed_bio_off`（共 4 新增 + 2 改写，四语言同步）。

守门：`src/lib/d114-feedback-guard.test.ts` 7 例。

### 6.26 错误口令文案与「已开启但不可用」（D-115，v2.38）

两条都来自 v1.3.1 真机走查。控件与布局**零新增**，只多一条 warning 色的状态行修饰类。

**① 主密码输错就说主密码输错**

| 项 | 旧 | 新 |
|---|---|---|
| 文案 | `err.unlock_failed`「解锁失败，请重试。」 | `err.wrong_master_password`「主密码错误，请重新输入。」 |
| 位置 | `ui.toast('error', …)`，右下角 3 s | **就地常驻**：`role="alert"`，输入即清 |
| 覆盖 | 开启快速解锁 | 开启快速解锁 + 改密旧密码 + 导出保险库 |

为什么可以点名「主密码」而不违反 A-05：A-05 的范围是**锁屏入口**（未认证调用者，防错误码被当 oracle）。这三个点只在**已解锁会话**里可达，用户已认证、手里就有库文件（可自行离线试），隐藏「密码对不对」零收益。

`is-danger` 提示在开启卡片里紧贴输入框下方（`.pms-hint.is-danger`），在改密卡片里用 `.pms-alert.is-danger.pwd-result`（与 §6.25 的成功绿条同一位置、同一清除机制 `clearPwdReceipt`）。

**② 「已开启」与「不可用」不许同屏打架**

`biometric_status` 的 `state` 与 `availability` 是**两个独立事实**：

| 组合 | 含义 | 状态行 |
|---|---|---|
| `ready` + `available` | 开着、能用 | 已开启（{date}） （`.bio-state`，正文色） |
| `ready` + 非 `available` | **封印完好，但当前用不了**（Hello 被关/被策略禁用/被占用） | **已开启，但当前不可用（{date}）**（`.bio-state.is-warn`，warning 色） |
| `stale` / `corrupt` | 已失效 / 已损坏 | 沿用既有三条文案 |
| `absent` | 未开启 | 未开启 |

降级态追加一句 `bio_degraded_hint`（`.pms-hint`，非红字——这不是错误，是状态说明）：

> Windows Hello 关闭或未开通期间，快速解锁一次也用不了。重新开通后会自动恢复——已保存的配置不会被删除，也不需要重输主密码；要彻底撤销请点「关闭」。

这句话同时回答三个必问：**现在能不能用**（不能）、**会不会丢配置**（不会）、**想彻底撤销怎么做**（按关闭）。缺任何一句，用户都会自己猜，而这次猜错了方向（以为关 Hello 就等于撤销）。

四语言新增 3 键：`err.wrong_master_password`、`bio_state_on_unavailable`（含 `{date}`）、`bio_degraded_hint`。

守门：`src/lib/d115-bio-state-guard.test.ts` 7 例（含「状态行必须先判降级再判已开启」的顺序断言——顺序反了降级分支永远走不到）。

### 6.27 未实现平台不渲染快速解锁卡片（D-116，v2.39）

安卓 v1.3.1 装完，设置页出现「Windows Hello 快速解锁 · 未开启 · 当前平台不支持快速解锁」——手机明明有指纹和人脸。两处都错：名词错（Windows 专属）、理由错（不是「不支持」，是「还没做」）。

**规则（新增）：** 快速解锁卡片只在**本平台存在适配器**时渲染。

| `availability` | 卡片 | 理由 |
|---|---|---|
| `available` / `not_configured` / `not_present` / `disabled_by_policy` / `busy` / `unsupported` | 渲染 | 都是「本机能谈这件事」，差别只是在不在状态 |
| **`not_implemented`** | **整卡不渲染** | 平台根本没接（Android 属 P3）。置灰也不行——那是给不存在的功能做广告 |
| `bio === null`（状态未读到 / 读失败） | 不渲染 | 宁晚一帧，不谎报「未开启」 |

**`unsupported` 的文案改口径**（四语言）：`当前平台不支持快速解锁` → `Windows Hello 组件在当前系统上无法调用，暂时不能使用快速解锁。` 它现在只对应真实故障（WinRT 激活失败 / 系统被裁剪）。

**不采用「置灰 + 敬请期待」**：①置灰入口会被当成承诺，用户会反复回来找；②真要显示，Android 文案必须是「指纹 / 人脸」，那是 P3 落地才有的东西，现在写就是二次说谎；③桌面端从未接过的平台也不该出现在设置里。

P3 落地时这张卡按平台取词（Windows = Windows Hello / Android = 指纹、人脸），风险说明也要按平台重写——Android 的 `AUTH_BIOMETRIC_STRONG` 是硬绑定（新增指纹即永久失效、PIN 顶替不了），不能照搬 Windows 的「登录凭据可绕过」。登记见 `docs/08` §22.4。

守门：`src/lib/d116-platform-gate.test.ts` 4 例。

### 6.28 快速解锁卡片平台化（D-113 P3 / Android Keystore，v2.40）

D-116 把这张卡在安卓上整块藏起来，理由是「功能还不存在」。P3 落地后它真的存在了，**D-116 的判定条件因此改写**：

| 平台 | `platform` | 卡片 |
|---|---|---|
| Windows | `windows` | 渲染（`availability` 判据不变） |
| Android | `android` | **渲染**（P3 起），全部文案走 `_android` 变体 |
| 其它 | `other` | 不渲染（`not_implemented`） |

**① 为什么由后端说「我是谁」**

`BiometricStatusDto` 新增 `platform: "windows" | "android" | "other"`。不用 `navigator.userAgent`：WebView 的 UA 随系统版本和厂商 ROM 漂移（有的 ROM 会把 UA 改得看不出平台），而后端**编译期就知道自己是哪个 target**，这是唯一不会说谎的来源。

**② 名词替换表（`{noun}` 插值）**

同一句通用话（「本机尚未设置 {noun}」「{noun} 已被系统策略禁用」）在两端要填不同的词。四种语言各写两遍会变成 8 个键，用插值只需 2 个：

| 语言 | `bio_noun`（Windows） | `bio_noun_android` |
|---|---|---|
| zh-CN | Windows Hello | 指纹 / 人脸 |
| zh-TW | Windows Hello | 指紋 / 臉部 |
| en-US | Windows Hello | **Biometric unlock** |
| th | Windows Hello | ชีวมิติ |

英文这一列特意不是 `fingerprint / face`——那会让「…is not set up」「…was detected」这类句子动词错位（复数主语 + 单数谓语的直觉冲突，读起来像机器翻译）。

**③ 必须分平台重写的三处**

| 键 | Windows | Android | 为什么不能共用 |
|---|---|---|---|
| `bio_desc` | 密钥只在本机、本 Windows 账户下可用 | 密钥由设备的安全芯片保管，绑定本机的指纹 / 人脸 | 存储机制不同（DPAPI 用户域 vs Keystore 硬绑定） |
| `bio_risk` | 能拿到本机 Windows 登录凭据的人可以绕过 | PIN / 密码不能代替生物识别；**新增或删除指纹、人脸会让快速解锁永久失效** | Windows 是软绑定（T1b 可绕过），Android 是硬绑定，威胁模型完全不同 |
| `bio_avail_hint` | 设置 → 账户 → 登录选项 | 设置 → 安全 → 指纹、人脸与密码 | 系统设置路径 |

「打开设置」按钮同样分平台：Windows「打开 Windows 登录选项」（`ms-settings:signinoptions`）/ Android「打开系统生物识别设置」（`ACTION_BIOMETRIC_ENROLL`，API 30+；24–29 退 `ACTION_SECURITY_SETTINGS`）。命令仍是**无参数**，目标在编译期定死。

**④ 失效原因与锁屏按钮**

- `stale + enrollment_changed`：Windows「Windows Hello 配置已变更」/ Android「设备的指纹 / 人脸记录已变更」。安卓这句是**真会发生**的（`setInvalidatedByBiometricEnrollment(true)`），Windows 侧至今没有等价信号（§21.4 候选 C）。
- 锁屏 ghost 按钮：Windows「使用 Windows Hello 解锁」/ Android「用指纹 / 人脸解锁」；等待态同理（「等待指纹 / 人脸验证…」）。
- 改密回执 `password_changed_bio_off`：Windows 说「…继续用 Windows Hello 解锁」，Android 说「…继续用指纹 / 人脸解锁」。

**⑤ 对话框文案跟随系统语言，不跟随应用语言**

`BiometricPrompt` 的标题/副标题/取消按钮来自 Android 资源（`res/values` / `values-zh-rCN` / `values-zh-rTW` / `values-th`），由**系统**选。理由：这是系统级安全对话框，用户对「系统的指纹框说系统语言」有既定预期；让它跟随应用内切换反而突兀（系统的锁屏也是这个行为）。

**⑥ 守门**

`src/lib/bio-android-guard.test.ts` 21 例：Kotlin 侧无阻塞等待、`doFinal` 只在认证回调内、五个命令全 `async`、`CryptoObject` 绑死、`bind` 双向走 AAD、禁改失效联动、auth-per-use、撤销彻底（删文件 + 删密钥别名）、平台取词、四语言 12 键齐全、对话框资源四语言。
### 6.29 快速解锁状态收敛与平台入口（D-119 / D-120 / D-121，v2.41）

主人真机新增指纹后走查，三条连在一起。

**① 设置页状态行必须如实说「失效」，不能说「已开启」**

指纹变更废掉的是 Keystore 里那把钥匙，`biometric.json` 本身完好，所以 `inspect` 一直返回 `Ready`。修复后状态语义是**墓碑**：

| 状态行 | 何时出现 |
|---|---|
| 已开启（{date}） | `ready` |
| **{noun} 记录已变更，快速解锁已失效** | `stale + enrollment_changed`（原 `bio_state_stale_enrollment_android`，四语言已有） |
| 已开启，但当前不可用（{date}） | `ready` 且平台不可用（D-115） |
| 未开启 | `absent` |

失效时下方按钮变成「开启快速解锁」（原地展开主密码输入框，复用 D-113 P2 的流程），锁屏那个 ghost 按钮同时消失。**两个动作成对出现**：说失效就必须给出重开的入口。

**② 失效 / 取消的提示文案按平台取词**

| 键 | Windows | Android |
|---|---|---|
| `err.bio_invalidated_enrollment` | Windows Hello 配置已变更… | 设备的指纹 / 人脸记录已变更… |
| `err.biometric_canceled` | 已取消 Windows Hello 验证 | 已取消指纹 / 人脸验证 |

只有这两条含平台名词，其余错误（主密码已改 / 保险库密钥已换 / 配置被篡改）两端一字不差，**不造同义键**。

**③ 系统设置入口：Windows 给按钮，Android 只给文字（D-121 → D-122）**

第一轮（D-121，B 案）把判据从「平台不可用」提为 `!bioAvailable || state ∈ {stale, corrupt}`——指纹变更废的是 Keystore 里那把钥匙，`availability` 照样是 `available`，条件嵌在里面就永远走不到。

第二轮（D-122）真机发现：按钮在 Android 上**点了完全没反应，而且一点提示都没有**。这句「没有提示」直接排除了所有失败路径（命令失效或 `startActivity` 抛异常都必然走 `invoke.reject` → 前端弹红字），说明 Intent 已发出、被 ROM 忽略或秒退。客户端无法判定「用户到底看到那个页面没有」，各品牌菜单位置又完全不同——**于是 Android 整个撤掉跳转**。

| `state` / `availability` | 状态行 | 主按钮 | Windows 入口 | Android 入口 |
|---|---|---|---|---|
| `ready` / `available` | 已开启（日期） | 关闭快速解锁 | 不显示 | 不显示 |
| `stale` / `available`（指纹变更后） | 记录已变更，已失效 | 开启快速解锁 | **按钮** | **说明文字** |
| `ready` / `not_configured` | 已开启但当前不可用 | 关闭快速解锁 | **按钮** | **说明文字** |
| `absent` / `not_configured` | 未开启 | 开启（置灰） | **按钮** | **说明文字** |
| `absent` / `available`（从未开启） | 未开启 | 开启快速解锁 | 不显示 | 不显示 |

Android 那句说明文字（`settings.bio_manage_hint_android`，四语言）：

> 需要录入、增删指纹或人脸时，请到手机的「设置」里打开生物识别 / 指纹 / 人脸页面操作。

文案只**说页面类别**，三条都不做（主人定，也写进了守卫测试）：

| 不做 | 理由 |
|---|---|
| 不列品牌 | 全球机型列不完，写三家等于告诉另外几百家「没为你考虑」 |
| 不写菜单层级 | 旧文案的「设置 → 安全 → 指纹、人脸与密码」抄的是 iOS 路由，安卓没有这一层 |
| 不承诺具体位置 | 各 ROM 自己改，写死了就是下一次「点了没反应」 |
| 不承诺「切回来自动重查」 | 那句原来有，真机照做后发现没变化——Android 上根本做不到（见下） |

（指纹 / 人脸出厂即开启，用户要做的是增删而不是从零找入口；会用密码管理器的人自己找得到，把显而易见的系统页面讲成教程是低估用户。）

**关于「自动重新检测」这句为什么删**：设置页确实挂了 `window.addEventListener('focus', …)` → `refreshBiometric()`，但它在 **Windows** 上才成立（窗口真的丢焦点又拿回来）。Android 是 Activity 进后台再回来，WebView 里的 document 从未失去焦点，`focus` 事件不触发 ⇒ 状态停在旧值。承诺一件做不到的事比不承诺更伤人：用户照做、回来没变化，只会觉得这 App 在瞎说。要让这句成立，得在 Rust 侧接 `RunEvent::Resumed` 发事件、前端再重查（可另开一条做）。守卫测试已禁止这类措辞回流。

**④ 守门**

`src/lib/bio-android-guard.test.ts` 33 例。D-121/D-122 新增：入口判据独立、按钮只给 Windows、Android 分支不许出现 `<button>`、Kotlin/Rust 的跳转链路已删干净（Windows 的 `open_hello_settings` 必须留着）、两个废弃键四语言同步消失、文案**不许出现任何品牌名**且必须提到「设置」。两次变异验证都生效：入口判据改回 `!bioAvailable` → 变红；文案里加回「华为在…」→ 变红。
