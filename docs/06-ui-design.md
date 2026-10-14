# PasswordMS · UI 设计规范 v1（2026-10-14）

> 本文件是**界面样式的唯一权威**。组件实现与本文冲突时以本文为准。
> 客户已拍板：**范围 = 全 app 一次到位**；**视觉 = 双主题精修，暗色主打科技感**。

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
    --pms-accent-2:      #58A6FF;
    --pms-accent-ink:    #04211D;
    --pms-accent-soft:   rgba(45, 212, 191, .12);
    --pms-danger:        #F87171;
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
    --pms-accent-2:      #2563EB;
    --pms-accent-ink:    #FFFFFF;
    --pms-accent-soft:   rgba(13, 148, 136, .10);
    --pms-danger:        #D64545;
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

/* ---------- 按钮 ---------- */
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
.pms-btn.is-primary:hover:not(:disabled) { filter: brightness(1.07); border-color: var(--pms-accent); }
.pms-btn.is-danger  { background: var(--pms-danger); border-color: var(--pms-danger); color: #fff; }
.pms-btn.is-ghost   { background: transparent; border-color: transparent; color: var(--pms-muted); }
.pms-btn.is-ghost:hover:not(:disabled) { background: var(--pms-bg-2); color: var(--pms-fg); }
.pms-btn.is-sm      { min-height: 30px; padding: 0 .6rem; font-size: .8rem; border-radius: var(--pms-r-sm); }
.pms-btn.is-block   { width: 100%; }
.pms-btn.is-lg      { min-height: var(--pms-ctl-h-lg); font-size: .95rem; }

/* ---------- 图标按钮：右端对齐、有热区 ---------- */
.pms-icon-btn {
  display: inline-flex; align-items: center; justify-content: center;
  width: var(--pms-hit); height: var(--pms-hit); flex: 0 0 var(--pms-hit);
  border: none; background: transparent; color: var(--pms-faint);
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
- 顶部工具条：搜索框（`.pms-input`，带 search 图标）+ 右侧"新建"主按钮。

### 5.4 其余面板（同一套语言，逐条过）

| 文件 | 要点 |
|---|---|
| `ItemList.svelte` | 行高 56px；标题 600、副行 `.pms-meta`；悬停 `--pms-bg-2`；选中左侧 2px 青绿竖条；空态 = 居中图标 + 文案 |
| `ItemDetail.svelte` | 字段用"标签 + 值 + 行尾复制按钮"三段式；密码值用 `--pms-mono`；复制按钮是 `.pms-icon-btn` |
| `ItemEditor.svelte` | 全部换成 `.pms-field` / `.pms-input-group`；自定义字段区用 .pms-card 内嵌浅底块；底部操作条 sticky |
| `Settings.svelte` | 分组标题 1rem/600 + 分组卡；每行 label 左、控件右；危险操作（改主密码、导出）用 `.pms-alert` 提示 |
| `PasswordGenerator.svelte` | 结果用 `--pms-mono` 大字 + `.pms-input-group` 承载复制/刷新按钮；选项区两列网格 |
| `HealthCheck.svelte` | 健康分做大字环形/大数字 + 4 个统计卡（弱口令/重复/超期/总数），弱项用 `.pms-badge.is-danger` |
| `Trash.svelte` | 与 ItemList 同构；到期时间用 `.pms-badge.is-warning` |
| `sync/*.svelte` | 4 个 tab 统一；配对二维码容器加 `.pms-card` 与留白；设备行 = 图标 + 名称 + 指纹 `.pms-meta` + 解绑 ghost 按钮 |
| `common/Modal.svelte` / `ConfirmDialog.svelte` | 遮罩 `rgba(0,0,0,.55)` + 轻微模糊；卡片 `--pms-r-lg` + `--pms-shadow`；确认按钮危险态用 `is-danger` |
| `common/Toast.svelte` | 右下角；`--pms-r`；左侧 3px 状态色竖条 |
| `common/StrengthMeter.svelte` | 4 段；弱=红 / 一般=橙 / 强=青绿 / 很强=青绿加粗；带文字标签。**空态（未输入）不要"压暗到看不见"，也不要让空 label 占位**：轨道用中性可见色 `--pms-track`，并 `.meter[data-score='0'] .label { display:none }` 让 4 段占满整行。<br>（修正 v1.1：原写法实测暗色下条带与卡片 RGB 差仅 24，肉眼不可见，且右侧凭空多 47px 留白。） |
| `common/Spinner.svelte` | 用 `currentColor`，随按钮文字色走 |

---

## 6. 本次必须修的缺陷

| 编号 | 级别 | 内容 | 位置 |
|---|---|---|---|
| **D-19** | P0 可用性 | 全局 `.field{flex-direction:column}` 泄漏进组件同名局部类 → 输入框与眼睛**纵向堆叠**，用户找不到输入位置 | `CreateVaultWizard.svelte:133`、`LockScreen.svelte:106` |
| **D-20** | P0 语义 | 眼睛图标**语义反了**（隐藏时给 `eye`，明文时给 `eye_off`） | `CreateVaultWizard:60`、`LockScreen:49`、`ItemDetail:207`、`ItemEditor:132` |
| **D-21** | P1 文案 | label 与 placeholder 用了**同一条 key** `lock.password_placeholder`，界面出现两遍"请输入主密码" | `LockScreen.svelte:33 / :39` |
| **D-22** | P1 体验 | 眼睛无固定位置、热区偏小（`padding:.35rem` 且居中） | 同上两处，随 `.pms-input-group` 一并解决 |
| **D-23** | P2 一致性 | 全 app 视觉语言不统一（字号/圆角/间距/阴影各写各的） | 全组件，随设计令牌统一 |

---

## 7. 验收清单

- [ ] `npm run test` 全绿（含 i18n 键位校验；新增 key 已同步 zh-CN 与 en-US）
- [ ] `npm run check`（svelte-check）零 error
- [ ] `npm run build` 成功
- [ ] **暗色与浅色两套都截图检查**（用浏览器打开 `dist/`，不依赖 Tauri）
- [ ] 解锁页与创建页：**输入框是一行**，眼睛在最右端；输入框高度正常（≈42px）
- [ ] 点眼睛：**星号→明文**时图标从"带斜杠"变"睁眼"（语义方向正确）
- [ ] 两页的 label 与 placeholder 文案**不相同**
- [ ] 确认主密码框与主密码框**外观同构**
- [ ] 全局搜索无残留旧类名：`.field` / `.icon-btn` / `.btn`（无前缀那批）已全部清除
- [ ] 无任何外部资源引用（`grep -r "http" src/` 只应命中 schema 或注释）
