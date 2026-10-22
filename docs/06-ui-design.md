# PasswordMS · UI 设计规范 v1（2026-10-14）

> 本文件是**界面样式的唯一权威**。组件实现与本文冲突时以本文为准。
> 客户已拍板：**范围 = 全 app 一次到位**；**视觉 = 双主题精修，暗色主打科技感**。
>
> 修订：v1.1 强度条空态（D-24）· v1.2 按钮悬停层叠（D-27/28/29）· v1.3 原生 UI 干涉与面板状态机（D-30/31/32）· v1.4 移动端响应式布局（D-35，客户方案 A：单栏推进 + 底部导航）· v1.5 底栏选中态改「胶囊 + 柔光」，取消整格铺色 · v1.6 自绘标题栏（decorations:false，D-36）+ 品牌 LOGO 定稿（A · 盾与钥匙孔）· v1.7 桌面顶栏收缩档（D-37，切英文后顶栏撑爆）· v1.8 顶栏信息架构重排（D-38）：常用外显、低频进设置 · v1.9 配对可用性修复（D-44）：多候选 IP 二维码 + 网络错误文案细分 + 会话死亡感知 · **v2.0 生成器符号自选（D-50）：34 个常用符号芯片可选，默认只选安全集 + - _ . , :** · **v2.1 导出保险库补主密码输入框（D-51）+ 生成器参数（含符号自选）跨会话持久化（D-52）** · **v2.2 导出文案修正（D-54：明文 → 加密副本）+ 备份恢复闭环（D-55：锁屏页「从备份恢复」）** · **v2.3 Toast 时长自适应（D-58a：error 按文案长度 6–12s）+ 同步页初始标签按配对状态（D-58b）** · **v2.4 条目列表「添加附件」入口（D-59：新建菜单三入口，选文件即导入，不进编辑器）** · **v2.5 附件依附父条目（D-60 §3.14）：列表第三按钮撤销（回退 v2.4 入口），登录/笔记详情页新增附件区（区头添加 + 附件行导出/移除），AttachmentView 改造为附件区组件** · **v2.6 自动锁定真持久化（D-61 A 案）：Settings 档位读写走 IPC，全局闲置计时 + Android 回前台补判上线** · **v2.7 法律条款可点开 + 「关于」分组（D-63）：勾选行拆出「免责声明 / 用户协议」两个内联链接（点链接不切换勾选），新增 `LegalDialog` 双语长文弹窗与 Settings「关于」（版本运行时读取 / 作者 / MIT / 项目主页）** · **v2.8 勾选行折行修复（D-64）**：`.pms-link` 字号改 `font: inherit`（不再写死 0.86rem，否则链接比同行文字大、把行撑折），勾选行在 ≤767px 收敛为单行放得下（文案「我已阅读并同意」+ 字号 0.8rem / gap 0.2rem / padding 0.6rem 0.7rem / checkbox 16px，`flex-wrap` 保留作极端窄屏兜底） · **v2.9 设备可辨识与同步状态隔离（D-66）**：设备行名称可重命名（行内编辑）+ 显示配对时间；**+ 设备指纹改用真公钥指纹（D-67）**；「立即同步」忙碌态按设备隔离，未点的那台只置灰不转圈 · **+ 配对两屏显示真指纹（D-68）：电脑显示本机指纹、手机显示从 join 结果拿到的对端指纹，中间人核对从「无处可做」变为「一眼可比」** · **v2.10 配对核对改「会话码」口径（D-69 修正 D-68）**：首版电脑屏显示**长期身份指纹**、手机屏显示**临时 X25519 公钥指纹**，两把不同密钥 → 两屏必然不等（属负收益，会把每台设备都报成中间人）；D-69 改为两屏都显示**同一次会话的会话码**（`sha256(host_eph 公钥)` 前 8 字节 hex），并**严格区分**「会话码（每次变）/ 设备指纹（稳定）/ device_id（只做 title）」三个量；设备指纹为空时该行**整行不渲染**（不再回退 `device_id`） · **v2.11 文件落盘与「取消不是错误」（D-71 / D-72）**：Android 上保存对话框返回的是 `content://` SAF URI（不是内核路径），`std::fs::write` 必失败 → 附件导出报「文件读写出错」（D-71，改走 provider fd 的 `"wt"` 写入）；同时**收口「取消 → 弹红」的三处散乱实现**（`vault-export` 未捕获 rejection / `AttachmentView` 取消弹红 / `ImportWizard` 空 catch 吞真错）为唯一口径 `runSaveToDestination`（D-72）；导入面板补格式说明 + **可下载 CSV 模板**（`save_import_template`），模板与解析器表头一致性由 `tests/import_template.rs` 锁死 · **v2.12 设置页旁路数据失败隔离 + 保险库文件说明（D-73）**：`Settings.svelte` 的 `$effect` 首行曾用 `structuredClone` 深拷 `$state` 代理（结构化克隆算法遇 Proxy 必抛 `DataCloneError`），于是其后的「库文件信息 / 自动锁定档位 / 版本号」三块**从不执行**——四处症状（库文件卡片空白 / 版本显示「—」/ 自动锁定档位恒为默认档 / 连带整卡不刷新），其中「自动锁定档位读不到」客户尚未发现 → 深拷改 `$state.snapshot()`，三块改由 `src/lib/settings-page.ts::loadSettingsPageSide` **逐块独立失败隔离**并显式建模失败态；「保险库文件」补说明行、读不到显示 `settings.vault_file_failed` 而非空白卡片；档位读失败显示 `settings.auto_lock_failed`（否则「5 分钟」会被误读成真实设置）；导入按钮文案消歧（`import.choose_file`）+ 「预览」不再无解释置灰（改可点 + `import.choose_first` 提示）；**手机「关于」组按行类型显式区分**（`.is-info` / `.is-actions`，见 §6.10）——根因不是「规则写错了」而是「**规则的适用范围没写**」：窄屏 `.set-row { flex-direction: column }` 只对**控件行**成立（`.set-ctl` 的 160px 固定宽在 390px 会溢出），一刀切下去把纯信息行与按钮行一起扫了 → 信息行 `.is-info`（左右两端对齐）/ 按钮行 `.is-actions`（横排换行）在**同一个 `@media` 块内 `.set-row` 之后**覆盖回来，**不许用 `:has()`**（WebView 版本差异，别赌）；桌面 ≥768px 不需要任何新规则、逐像素不变 · **v2.13 配对成功态保留会话码（D-75）**：`pairing://ended` 在**成功与失败两种结局都发**，前端旧实现无条件 `sync.pairing = null`，而会话码正挂在这个对象上 —— 手机端「请核对会话码」的提示**恰好在事件之后才出现**，电脑屏的码已被清掉，D-69 的两屏核对**上线即不可用**；修法：载荷经纯函数 `nextPairingState()` 分流，`completed` **保留** `pairing` 并转「配对成功（待核对）」态（无二维码、大字会话码 + 核对提示 + 「完成」按钮），`aborted`/未知 清屏提示重新生成。详见 §5.7.5 · **v2.14 保险库压缩入口（D-76）**：容器只打墓碑标志、**从不回收空间**，且此前无任何压缩路径（导出备份都是原样 `fs::copy`）—— 实测主人库 24.34 MiB 里仅 **0.30 MiB 是活数据（1.28%）**，24.03 MiB 全是墓碑，其中 6 个已删附件占 24.0 MiB；设置页「保险库文件」卡片新增「已删附件占 / 可回收」两行 + 「压缩保险库」按钮（`reclaimable_bytes > 0` 才渲染，同步中禁用），二次确认**必须**给出四个数字与三条提示（不改数据/不要主密码、回收站附件不回收、临时 2 倍磁盘请勿关程序），成功后**必须重新拉取**大小与统计。详见 §6.11

---

## 0. 硬约束（不许违反）

1. **零外部资源**。不引 Web 字体、不引图标库、不引 CSS 框架、不加外链。图标继续用 `common/Icon.svelte` 的内联 SVG，字体继续用系统字体栈。
2. **体积**。纯 CSS 改动对体积影响可忽略，但禁止借此引入任何新依赖（Windows ≤10MB / APK ≤12MB 的门禁不许破）。
3. **i18n**。组件内**禁止中文字面量**，一律走 `ui.t()`。新增 key **必须同时**加到 `src/lib/i18n/zh-CN.ts` 与 `en-US.ts`（有 `i18n.test.ts` / `i18n-usage.test.ts` 做键位与用法校验）。
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
- 顶部工具条（`.topbar`，`background: var(--pms-surface)`，`min-height: 56px`）：
  - **左**：搜索框（`.pms-input`，带 search 图标）+ "新建"主按钮。
  - **右**（`.topbar-right`，按此顺序）：**「生成密码」ghost 小按钮（骰子图标，D-30/D-33 客户反馈新增）** → 主题切换 `ThemeToggle` → 语言切换 → 导入 → 锁定。
  - 判据：**顶栏右区必须能一眼看到"生成密码"**。它不带 `onuse`，只生成 + 复制，绝不写回任何条目。

### 5.4 其余面板（同一套语言，逐条过）

| 文件 | 要点 |
|---|---|
| `ItemList.svelte` | 行高 56px；标题 600、副行 `.pms-meta`；悬停 `--pms-bg-2`；选中左侧 2px 青绿竖条；空态 = 居中图标 + 文案。**列表头两入口**：「新建登录」（主按钮）+「新建笔记」（ghost）。**（v2.5 D-60 回退 v2.4 的第三按钮）**：附件入口移入详情页附件区（附件是父条目附属内容，不再有独立附件条目） |
| `ItemDetail.svelte` | 字段用"标签 + 值 + 行尾复制按钮"三段式；密码值用 `--pms-mono`；复制按钮是 `.pms-icon-btn`。**D-60：登录/笔记条目详情底部渲染附件区**（`<AttachmentView {item} />`，仅 `login`/`secure_note` 两类父条目；遗留独立附件条目经解锁迁移后不可达） |
| `ItemEditor.svelte` | 全部换成 `.pms-field` / `.pms-input-group`；自定义字段区用 .pms-card 内嵌浅底块；底部操作条 sticky |
| `AttachmentView.svelte` | **D-60 改造为父条目附件区**（不再是独立附件条目的整页详情）：区头 = `paperclip` 图标 + 区标题（`attach.section_title`）+「添加附件」ghost 按钮（`attach.add`，常驻可点——空附件数组时行列表隐藏但入口保留）；附件行 = `--pms-bg-2` 浅底圆角卡（图标 + 文件名 600 + `MIME · 大小` meta 行 + 行尾两个 `.pms-icon-btn` 导出/移除）。**待同步态（D-53 语义保留）**：`attachment_blob_ready(attachment_id) = false` 时行内展示 `attach.pending_sync`（`--pms-warning` 色）并禁用导出按钮（强导会报 `ERR_BLOB_MISSING`）。导出保留明文风险二次确认（`attach.export_confirm_*`）；**导出的选目标与写盘走 `runSaveToDestination`（D-71/D-72）——取消安静返回、失败才弹红**，Android 的 `content://` 由后端经 provider fd 写入（详见 §5.8）；移除走二次确认（`attach.remove_confirm_*`，确认体带文件名插值）→ `remove_attachment` → blob 交孤儿 GC。添加/移除成功后 `vault.load()` 刷新（父条目 rev+1） |
| `ImportWizard.svelte` | 三步走：①「选择文件」+ 路径回显 ②格式单选（`json` / `csv`）③「解析预览」。**格式选择器下方必须有格式说明，不能让用户点完解析才知道文件不合法（D-72）**：`.pms-meta` 一行写支持范围（`import.supported_hint`）、`.pms-hint` 一行写细则（`import.template_hint`：CSV 列清单与 `type` 取值），两行都**常显**、不随格式切换隐藏。下一行是 `.pms-btn.is-ghost` 的「下载 CSV 模板」按钮（`download` 图标 + `import.download_template`）→ 系统保存对话框 → `save_import_template` 写盘 → success toast `import.template_saved`。**取消/失败的判定见 §5.8.2**（取消必须安静，失败必须红）。CSV 解析后展示映射表：已知列用 `.pms-badge.is-accent` 芯片、扩展列用 `.pms-badge-muted` |
| `Settings.svelte` | 分组标题 1rem/600 + 分组卡；每行 label 左、控件右；危险操作（改主密码、导出）用 `.pms-alert` 提示。**「导出保险库」卡片含当前主密码输入框**（D-51：后端导出用它做 `unlock_check`，为空时前端不发 IPC、直接提示 `settings.export_password_required`）。**导出的是整库的加密副本而非明文（D-54）**，且**文件的修改时间 = 库数据的最后写入时间**（`fs::copy` 保留 LastWriteTime）；备份可在锁屏页用「从备份恢复」（D-55）导入。**自动锁定档位真持久化（D-61 A 案）**：`auto_lock_minutes` 经 `get_auto_lock`/`set_auto_lock` 读写库明文头部（初始化读 IPC，换档即写 + success toast `settings.auto_lock_updated`，失败 error toast 并回滚 select 显示值）；执行层为全局闲置计时——`pointerdown`/`keydown`（passive）重置闲置起点，闲置达档位即走既有 `lockNow()`（reset 三个 store + 切锁屏路由）；Android 后台冻结计时器由 `visibilitychange` 补判（回前台超档立即锁，未超档后台时间计入闲置重排剩余）；无「从不」档；纯逻辑层 `src/lib/auto-lock.ts` 依赖注入可测（假时钟 8 例）。**「关于」分组（D-63 v2.7）**：末尾追加独立卡片（`settings.group_about`），四行 = 产品名 + 版本 / 作者 / 许可证 / 三按钮（项目主页 + 免责声明 + 用户协议，均为 `is-ghost is-sm`）；版本经 `getVersion()` **运行时读取，禁硬编码**，读取失败显示 `—`。**旁路数据失败隔离（D-73 v2.12）**：三块旁路数据（库文件信息 / 自动锁定档位 / 版本号）一律经 `src/lib/settings-page.ts::loadSettingsPageSide` **逐块独立失败隔离**（任一失败不影响其余两项；失败态显式建模，不再被 `.catch(() => null)` 吞成空白）；`draft` 深拷**必须**用 `$state.snapshot()`，**禁止 `structuredClone`**（对 `$state` 代理必抛 `DataCloneError`，且抛在 `$effect` 首行会让其后逻辑全部不执行；全仓由 `src/lib/state-clone-guard.test.ts` 守门），也**不得**用浅拷贝 `{ ...value }`（`generator` / `health` 两层仍是代理，`bind:value` 会写穿 store）；「保险库文件」卡片**有说明行** `settings.vault_file_hint`，读不到时显示 `settings.vault_file_failed` 而**不是**空白卡片。详见 §5.5 |
| `PasswordGenerator.svelte` / `PasswordGeneratorModal.svelte` | 结果用 `--pms-mono` 大字 + `.pms-input-group` 承载复制/刷新按钮；选项区两列网格。**一律以弹窗形态出现**（`PasswordGeneratorModal` 内 Modal 自带 `{#if open}` 包裹 → 关闭即卸载，不残留请求/toast）。位数用**数字输入框**（输入中不钳制，失焦/回车才 clamp 到 8–64）；四类字符复选；**一类都不选时生成按钮 `disabled` 并给出提示**；结果下方带 `StrengthMeter`。**生成器参数（位数 / 四类字符 / 排除易混 / 自选符号集）变更后防抖 300ms 静默持久化**（D-52：走 `SettingsStore.saveQuiet` 不弹 toast，用显式变更点触发而非 `$effect` → 初始化不会把未变更默认值写回）；初始值优先级 `initial`（调用方显式传入）> 设置里持久化的偏好 > 默认值，`symbol_set` 空时回退 `+-_.,:` |
| `common/ThemeToggle.svelte` | 三态循环（系统 → 浅色 → 暗色），图标用 `sun` / `moon`，并带 `title`。切换时**同时**更新 `document.documentElement` 的 `data-theme` **和**原生窗口标题栏（见 §4 D-32）。挂载点：解锁页右上角、主界面顶栏右区。 |
| `HealthCheck.svelte` | 健康分做大字环形/大数字 + 4 个统计卡（弱口令/重复/超期/总数），弱项用 `.pms-badge.is-danger` |
| `Trash.svelte` | 与 ItemList 同构；到期时间用 `.pms-badge.is-warning` |
| `sync/*.svelte` | 4 个 tab 统一；配对二维码容器加 `.pms-card` 与留白；设备行 = 图标 + 名称（**可重命名，行内编辑，D-66**）+ **指纹**（= `DeviceDto.fingerprint`，16 位小写 hex；**不是** `device_id`，D-67）+ **配对时间** + 上次同步 + 铅笔改名 `.pms-icon-btn` + 解绑 ghost 按钮。**初始标签按配对状态（D-58b）**：主菜单进入「同步」时，若用户从未手动点过任何 tab，则已有配对设备（`sync.devices.length > 0`）优先显示「状态」，无配对设备优先显示「配对」；用户一旦手动点过任何 tab 即永久尊重其选择（`syncTabTouched`），不再自动调整；不做持久化。**「立即同步」忙碌态按设备隔离（D-66）**：占用态是 `sync.syncingDeviceId`（单个设备 id，不是 store 级布尔）——被点的那台显示 `<Spinner>` + `sync.syncing`，其余台仅 `disabled` **不转圈**，期间**全部**不可点（禁止并发重同步）；判定收敛在纯函数 `syncButtonState()`（`src/lib/device-label.ts`，5 例单测守门）。设备行内改名的硬约束见 §5.6，指纹取值规则见 §5.6.3。**配对两屏显示会话码（D-68 立项 / D-69 修正口径）**：电脑屏在二维码/配对文本下方显示 `PairingDto.session_code`（本次配对核对码）+ `sync.session_code_hint`，手机屏在加入表单下方显示 `sync.lastPairSessionCode`（电脑的会话码）+ 对端设备名 + 同一句提示；判定走纯函数 `hasSessionCode()`，无值则**整块不渲染**（不显示空标签）。**会话码每次配对都变，不是设备指纹**；方向硬约束与样式约束见 §5.7 |
| `common/Modal.svelte` / `ConfirmDialog.svelte` | 遮罩 `rgba(0,0,0,.55)` + 轻微模糊；卡片 `--pms-r-lg` + `--pms-shadow`；确认按钮危险态用 `is-danger` |
| `common/Toast.svelte` | 右下角；`--pms-r`；左侧 3px 状态色竖条。**时长自适应（D-58a）**：success 3000ms；info 5000ms；error 按当前语言渲染后文案长度自适应 `min(12000, max(6000, 字数 × 150))`（6s 起步、每字 150ms、12s 封顶）——长错误文案必须给足阅读时间。关闭按钮不变 |
| `common/StrengthMeter.svelte` | 4 段；弱=红 / 一般=橙 / 强=青绿 / 很强=青绿加粗；带文字标签。**空态（未输入）不要"压暗到看不见"，也不要让空 label 占位**：轨道用中性可见色 `--pms-track`，并 `.meter[data-score='0'] .label { display:none }` 让 4 段占满整行。<br>（修正 v1.1：原写法实测暗色下条带与卡片 RGB 差仅 24，肉眼不可见，且右侧凭空多 47px 留白。） |
| `common/Spinner.svelte` | 用 `currentColor`，随按钮文字色走 |

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
| 语言 | `$derived`：`ui.locale === 'en-US' ? 'en' : 'zh'`，**跟随 UI 语言即时切换**（不缓存、不落盘） |
| 滚动区 | `.legal { max-height: min(60vh, 420px); overflow-y: auto; padding-right: .4rem }` —— `padding-right` 是给滚动条留槽位，避免长短文档切换时正文横向抖动 |
| 排版 | `legal.updated` meta 行（`.pms-meta`）置顶 → 每节 `h4`（0.9rem / 600 / `--pms-fg`，编号 `N.` 由渲染时拼）→ 每段 `p`（0.85rem / line-height 1.65 / `--pms-muted`）。节间距 `h4 { margin: 1rem 0 .35rem }`，首节 `margin-top: .5rem` |
| 底部 | `{#snippet footer()}` 单个 `pms-btn.is-primary`「我知道了」（`legal.understood`）→ `open = false`；右上角 × 与点遮罩同样关闭 |
| 正文数据 | `src/lib/legal-content.ts`：免责 6 条 / 用户协议 8 条，每条 1–2 段实质内容（生效日期等条目可为 1 段）；`LEGAL_UPDATED = '2026-10-20'` 四份文档统一；导出 `getDisclaimer(lang)` / `getTerms(lang)` |

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
| 模板按钮 | `.pms-btn.is-ghost` | `import.download_template` | `download` 图标（15px）；成功后 toast `import.template_saved` |

- 两行说明**常显**，不随格式单选切换而隐藏（用户要判断的正是「我手上这个 json / csv 能不能导」）。
- 模板内容固定取 `passwordms_core::import::template::CSV_TEMPLATE`，**禁止在前端拼字符串**——前端拼的模板迟早和解析器漂移。
- **模板自身导入必须 0 跳过**：表头 10 列与 `KNOWN_COLUMNS` **逐字同序**；示例行**不得溢出到 `custom_fields`**（列数 = 表头数）。两条均由 `crates/passwordms-core/tests/import_template.rs` 锁死，改模板必须同步改测试。
- 模板只做「照着填」的示范，**不做通用列映射向导**（主人本轮拍板：不做）。

#### 5.8.4 样式与交互硬约束

| 约束 | 规则 |
|---|---|
| 类名 | 说明两行用 `.pms-meta`（中性说明）/ `.pms-hint`（细则），**不新造类**；按钮复用 `.pms-btn.is-ghost` |
| 顺序 | 格式单选 → `.pms-meta` → `.pms-hint` → 模板按钮 → 映射表。**说明必须在解析按钮之前**，这是本条缺陷的核心 |
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
| `ItemEditor.svelte` | 加 `flex: 1; min-width: 0`（独占整屏）；底部 sticky 操作条因 `.body` 预留了底栏高度而不会被遮挡 |
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

- [ ] `npm run test` 全绿（含 i18n 键位校验；新增 key 已同步 zh-CN 与 en-US）
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
- [ ] **D-75**（v2.13 已实现）：**配对成功后会话码留在电脑屏上，两屏核对第一次真正可做**
  - **核心判据（时序）**：手机扫码成功后，**电脑屏不得收起面板** —— 面板须转为「配对成功（待核对）」态，里面仍是**本次会话**的会话码；此时手机屏的核对块也正好出现，两者逐字相同。
  - **结构判据（可单测）**：`pairing://ended` 的载荷必须经 `nextPairingState()` 翻译；`completed → { clearPairing: false, completed: true, ended: false }`；`aborted`/未知 → `{ clearPairing: true, completed: false, ended: true }`。`src/lib/pairing-state.test.ts` 16 例守门（纯函数语义 / handler 先算后清 / 组件分支存在且优先于二维码分支）。
  - **成功态约束**：`pairCompleted && pairing !== null` 时**不渲染二维码**（已扫过，留着会让人以为还能再扫），显示「配对成功」+ `sync.pair_completed_hint` + 会话码（`hasSessionCode()` 判空后）+ 主按钮「完成」（`sync.endPairing()`）。按钮文案 `sync.pair_done`。
  - **退出路径**：点「完成」/ 重新生成二维码 / 取消配对 / 锁定，四条路都必须让 `pairCompleted` 回到 `false`（否则下一轮二维码旁边会挂着上一轮的成功提示）。
  - **失败态不回归**：`aborted` 仍清屏 + 提示 `sync.pair_ended`「请重新生成二维码」。

