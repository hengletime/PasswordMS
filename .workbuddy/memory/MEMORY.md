# PasswordMS · 项目长期记忆

## 构建（本项目最容易踩坑的地方）

**桌面构建命令**（本机 Bash 工具的环境里默认**没有** cargo，必须显式加）：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cd D:/Software/PasswordMS
npx tauri build --target x86_64-pc-windows-gnullvm
```

- **必须带 `--target x86_64-pc-windows-gnullvm`**。本机非管理员、装不了 MSVC，默认目标编不过。
- **`dist/` 在 Rust 编译期被烤进 exe**（`frontendDist: "../dist"` + `generate_context!`）。
  **只改前端不重编 exe = 用户看不到任何变化**。
- **`windres` 不需要手工 export**：由 `src-tauri/build.rs` 的 `ensure_windres_on_path()` 自愈。
  工具在 `~/.workbuddy/tools/mingw-bin/`（含自研 Rust windres 包装器，因为 GNU windres 默认要先跑 `gcc -E` 而本机无 C 编译器）。
  换机器只需设 `PMS_WINRES_DIR`。详见 `docs/03b-verification.md` §8.1 / §8.2（事故 D-33）。
- `.cargo/config.toml` 是**本机私有**（`.gitignore` 已排除），装 linker / rustflags；`.example` 是给别人的模板。

**Android APK 构建唯一正道 = `bash scripts/build-android.sh`**（D-58 实证，勿绕过）：
- 直接 `npx tauri android build --apk` 在本机必产**空壳包**（symlink 缺陷 → 包内 .so 全 0 字节），
  且默认 universal 4-ABI——就算手工补 .so 也会 22MB 超 12MB 红线（多 ABI 从来不是交付口径，arm64 单 ABI 才是）。
- 脚本一条龙：上游构建→解包四断言校验→自动修复（拷 arm64 真 .so + Gradle `-x rustBuild*`）→apksigner V3 签名→zipalign→体积门禁。
- 直跑 `./gradlew` 会因 rustBuild* 任务起 `npm.bat` 失败，必须 `-x` 全部 rustBuild*。

### 引导：别再走的路（都实测证伪过）

- `.cargo/config.toml` 里 `[env] PATH = { prepend = true }` **在 Windows 下无效** ——
  系统注入的环境变量名是 `Path`，cargo 又把 `PATH` 当新条目塞进去，两条并存，build script 读到的仍是旧值。
  （对照探针：同段自定义变量能传进 build script，**只有 PATH 传不进**。）
- `RC` / `RC_<target>` 环境变量、`WindowsResource::set_windres_path()` 对 `embed-resource` 同样无效
  （前者只在 msvc 分支被消费，后者 0.1.1 起是 no-op）。
- **不要对 exe 做明文内容级 grep**：Tauri 会把前端资源**压缩**后再烤进二进制，搜 `pms-btn` 之类必然搜不到（假阳性）。
  可靠判据 = **新哈希名在包内 + 旧哈希名消失**。
- **不要靠肉眼判断 CSS 层叠问题**：D-27 那次肉眼只看到客户报的 1 处，机器排查抓出 6 处。

## 交付前必过的门（缺一不可）

前端三道：

```bash
npm run test        # 期望全绿
npm run check       # 期望 0 error / 0 warning
npm run check:css   # 期望 0 FAIL（scripts/check-css-contract.mjs，自研层叠解算器）
```

exe 三条硬验证：

1. **产物新鲜度**：`find src src-tauri -newer <exe>` 输出为空。
2. **新前端真进包**：新哈希名在 exe 内、旧哈希名已消失。
3. **体积门禁**：`bash scripts/check-size.sh desktop` EXIT=0（Windows ≤10MB / APK ≤12MB）。

## 约定

- **P2 backlog 排期**（主人拍板 2026-10-18）：HLC 时钟 / Done base 回执 / 到达序不对称文档化 → 统一排在 **#530 质量门与端到端回归之后**，勿提前插队。
- **icons 策略**：`/src-tauri/icons/icon.icns` 与 `/src-tauri/icons/ios/` 已入 `.gitignore`（B 案，2026-10-18）；真做 macOS/iOS 端时删掉那两行。

- 缺陷编号用 `D-nn`，事故同时写进 `docs/06-ui-design.md` §6 缺陷表与对应章节。
- **改 UI 必须同步 `docs/06-ui-design.md`** —— 它是界面样式的**唯一权威**。
  规范与代码不一致会直接制造 bug（D-27 就是规范本身写错，工程师照抄）。
- **构建的前置条件必须写进构建脚本，不能靠操作者记得 export**。
  "文档里写了" ≠ "生效"；一条命令在"我记得先 export 过"的终端里能过，就是下次翻车的伏笔。
- **提交由主人统一做**：助手**不执行** `git add` / `commit` / `push`，只提供完整命令。
  ⚠️ **每一次"更改完成交付"都必须随附完整可一键执行的 git 命令**（`git add` 带全部路径 + heredoc 的 commit message），
  主人明确要求过（2026-10-14 因连续两轮交付没附命令被点名）。交付清单里没有 git 命令 = 交付不完整。
  命令须按当轮 `git status --porcelain` 现点现开；跨多轮未提交时，一条命令覆盖全部待提交内容并注明。
