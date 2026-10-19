# PasswordMS · UI 设计规范 v1（2026-10-14）

> 本文件是**界面样式的唯一权威**。组件实现与本文冲突时以本文为准。
> 客户已拍板：**范围 = 全 app 一次到位**；**视觉 = 双主题精修，暗色主打科技感**。
>
> 修订：v1.1 强度条空态（D-24）· v1.2 按钮悬停层叠（D-27/28/29）· v1.3 原生 UI 干涉与面板状态机（D-30/31/32）· v1.4 移动端响应式布局（D-35，客户方案 A：单栏推进 + 底部导航）· v1.5 底栏选中态改「胶囊 + 柔光」，取消整格铺色 · v1.6 自绘标题栏（decorations:false，D-36）+ 品牌 LOGO 定稿（A · 盾与钥匙孔）· v1.7 桌面顶栏收缩档（D-37，切英文后顶栏撑爆）· v1.8 顶栏信息架构重排（D-38）：常用外显、低频进设置 · v1.9 配对可用性修复（D-44）：多候选 IP 二维码 + 网络错误文案细分 + 会话死亡感知 · **v2.0 生成器符号自选（D-50）：34 个常用符号芯片可选，默认只选安全集 + - _ . , :** · **v2.1 导出保险库补主密码输入框（D-51）+ 生成器参数（含符号自选）跨会话持久化（D-52）** · **v2.2 导出文案修正（D-54：明文 → 加密副本）+ 备份恢复闭环（D-55：锁屏页「从备份恢复」）** · **v2.3 Toast 时长自适应（D-58a：error 按文案长度 6–12s）+ 同步页初始标签按配对状态（D-58b）** · **v2.4 条目列表「添加附件」入口（D-59：新建菜单三入口，选文件即导入，不进编辑器）**

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
      label.pms-check  "我已知晓并继续"

    button.pms-btn.is-primary.is-block.is-lg  "创建并解锁"
    button.pms-btn.is-ghost.is-block  "返回"
```

> `MIN_MASTER_PASSWORD_LEN` 的硬拦截、`acknowledged` 门禁、`canSubmit` 逻辑**一律不许改**，只动样式与结构。
> 新增 i18n key：`lock.password_label`、`create.password_label`、`create.confirm_placeholder`、`create.hint`。

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
| `ItemList.svelte` | 行高 56px；标题 600、副行 `.pms-meta`；悬停 `--pms-bg-2`；选中左侧 2px 青绿竖条；空态 = 居中图标 + 文案。**列表头三入口（D-59）**：「新建登录」（主按钮）+「新建笔记」+「添加附件」（均 ghost）；附件按钮 = `paperclip` 图标，点击 → 系统文件对话框（单选）→ `import_attachment` 导入成功即建条目并选中（**不进编辑器**，标题默认取文件名含扩展名）；对话框取消 → 无动作无 toast；IPC 失败 → 统一错误 toast。桌面与移动端共用本组件（<768px 下三按钮 `flex: 1` 等宽排布不变） |
| `ItemDetail.svelte` | 字段用"标签 + 值 + 行尾复制按钮"三段式；密码值用 `--pms-mono`；复制按钮是 `.pms-icon-btn` |
| `ItemEditor.svelte` | 全部换成 `.pms-field` / `.pms-input-group`；自定义字段区用 .pms-card 内嵌浅底块；底部操作条 sticky |
| `AttachmentView.svelte` | 元信息三行（文件名/类型/大小）+ 导出 ghost 按钮。**D-53「附件内容待同步」态**：blob 未同步到本机（`attachment_blob_ready` = false）时，导出按钮上方展示 `.pms-alert is-warning`（key `attach.pending_sync`）并禁用导出（强导会报 `ERR_BLOB_MISSING`） |
| `Settings.svelte` | 分组标题 1rem/600 + 分组卡；每行 label 左、控件右；危险操作（改主密码、导出）用 `.pms-alert` 提示。**「导出保险库」卡片含当前主密码输入框**（D-51：后端导出用它做 `unlock_check`，为空时前端不发 IPC、直接提示 `settings.export_password_required`）。**导出的是整库的加密副本而非明文（D-54）**，且**文件的修改时间 = 库数据的最后写入时间**（`fs::copy` 保留 LastWriteTime）；备份可在锁屏页用「从备份恢复」（D-55）导入 |
| `PasswordGenerator.svelte` / `PasswordGeneratorModal.svelte` | 结果用 `--pms-mono` 大字 + `.pms-input-group` 承载复制/刷新按钮；选项区两列网格。**一律以弹窗形态出现**（`PasswordGeneratorModal` 内 Modal 自带 `{#if open}` 包裹 → 关闭即卸载，不残留请求/toast）。位数用**数字输入框**（输入中不钳制，失焦/回车才 clamp 到 8–64）；四类字符复选；**一类都不选时生成按钮 `disabled` 并给出提示**；结果下方带 `StrengthMeter`。**生成器参数（位数 / 四类字符 / 排除易混 / 自选符号集）变更后防抖 300ms 静默持久化**（D-52：走 `SettingsStore.saveQuiet` 不弹 toast，用显式变更点触发而非 `$effect` → 初始化不会把未变更默认值写回）；初始值优先级 `initial`（调用方显式传入）> 设置里持久化的偏好 > 默认值，`symbol_set` 空时回退 `+-_.,:` |
| `common/ThemeToggle.svelte` | 三态循环（系统 → 浅色 → 暗色），图标用 `sun` / `moon`，并带 `title`。切换时**同时**更新 `document.documentElement` 的 `data-theme` **和**原生窗口标题栏（见 §4 D-32）。挂载点：解锁页右上角、主界面顶栏右区。 |
| `HealthCheck.svelte` | 健康分做大字环形/大数字 + 4 个统计卡（弱口令/重复/超期/总数），弱项用 `.pms-badge.is-danger` |
| `Trash.svelte` | 与 ItemList 同构；到期时间用 `.pms-badge.is-warning` |
| `sync/*.svelte` | 4 个 tab 统一；配对二维码容器加 `.pms-card` 与留白；设备行 = 图标 + 名称 + 指纹 `.pms-meta` + 解绑 ghost 按钮。**初始标签按配对状态（D-58b）**：主菜单进入「同步」时，若用户从未手动点过任何 tab，则已有配对设备（`sync.devices.length > 0`）优先显示「状态」，无配对设备优先显示「配对」；用户一旦手动点过任何 tab 即永久尊重其选择（`syncTabTouched`），不再自动调整；不做持久化 |
| `common/Modal.svelte` / `ConfirmDialog.svelte` | 遮罩 `rgba(0,0,0,.55)` + 轻微模糊；卡片 `--pms-r-lg` + `--pms-shadow`；确认按钮危险态用 `is-danger` |
| `common/Toast.svelte` | 右下角；`--pms-r`；左侧 3px 状态色竖条。**时长自适应（D-58a）**：success 3000ms；info 5000ms；error 按当前语言渲染后文案长度自适应 `min(12000, max(6000, 字数 × 150))`（6s 起步、每字 150ms、12s 封顶）——长错误文案必须给足阅读时间。关闭按钮不变 |
| `common/StrengthMeter.svelte` | 4 段；弱=红 / 一般=橙 / 强=青绿 / 很强=青绿加粗；带文字标签。**空态（未输入）不要"压暗到看不见"，也不要让空 label 占位**：轨道用中性可见色 `--pms-track`，并 `.meter[data-score='0'] .label { display:none }` 让 4 段占满整行。<br>（修正 v1.1：原写法实测暗色下条带与卡片 RGB 差仅 24，肉眼不可见，且右侧凭空多 47px 留白。） |
| `common/Spinner.svelte` | 用 `currentColor`，随按钮文字色走 |

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
| `Settings.svelte` | `.set-row` 由「label 左 / 控件右」改为上下堆叠，`.set-ctl` 占满整宽（避免 160px 固定宽控件 + 长文案溢出） |
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
| **D-59** | **P0 功能不可达** | **附件功能前端零入口**（#530 真机走查发现）：后端 `import_attachment` 命令、core 层校验（≤20 MB）、D-53 blob 同步、`AttachmentView` 详情渲染全就绪，但列表「新建」区只有登录/笔记两个按钮，`api.keydocs.importAttachment` **全项目零调用**——F-13 密钥文档（附件）从上线起不可达 | 列表头新增第三个「添加附件」按钮（`paperclip` 图标，zh `添加附件` / en `Add attachment`）：点击 → 系统文件对话框（单选）→ `import_attachment` 导入成功即建条目 + `vault.load()` 刷新 + 选中新条目（**不进编辑器**；标题默认取文件名含扩展名，与 core 层空 title 回退语义一致）；对话框取消 → 无动作无 toast；IPC 失败（含超限 `ERR_ATTACHMENT_TOO_LARGE`）→ 统一错误 toast。流程逻辑抽 `src/lib/attachment-import.ts` 纯逻辑层（守卫/取消/成功/失败 4 分支有单测守门）+ ipc-contract 补 `import_attachment` 双侧在册断言（v2.4） |

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
- [ ] **D-60**（待实现，设计见 02-arch.md §3.14）：附件改依附父条目——移除列表页「添加附件」入口（回退 D-59 入口部分）；登录/笔记详情页新增附件区（区头「添加附件」+ 行：文件名/大小/MIME + 导出/删除，导出保留明文风险二次确认与待同步态）；AttachmentView 改造为附件行组件；回收站父条目行显示附件数徽标；i18n 中英双份 `attachments.*` 键（`item.new_attachment` 撤销）
