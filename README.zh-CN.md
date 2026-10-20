# PasswordMS

**零知识、跨平台的本地密码管理器** —— Windows 桌面 + Android，支持局域网端到端加密同步。

[English](README.md) | 简体中文

[下载最新版本](https://github.com/hengletime/PasswordMS/releases/latest) · [问题反馈](https://github.com/hengletime/PasswordMS/issues)

---

## ✨ 特性

- **零知识加密**：整卷加密（Argon2id 密钥派生 + XChaCha20-Poly1305 认证加密），主密码不上传、不可找回
- **双平台**：Windows 10/11（安装版 / 免安装绿色版）+ Android（arm64）
- **局域网同步**：扫码配对，端到端加密，数据不经任何云端服务器
- **自动锁定**：窗口闲置自动锁定（1 / 5 / 15 分钟可选），Android 切后台超时回前台立即锁定
- **附件**：登录 / 笔记条目可挂附件（单条目最多 16 个，单附件 ≤ 2MB），随保险库一起加密
- **回收站**：软删除 → 还原 → 永久清除，附件随条目级联清理
- **极小体积**：桌面安装包约 1.6MB，APK 约 6.9MB

## 📦 下载

到 [Releases](https://github.com/hengletime/PasswordMS/releases/latest) 页面下载：

| 文件 | 平台 | 说明 |
|---|---|---|
| `PasswordMS_x.y.z_x64-setup.exe` | Windows 10/11 x64 | 安装版（推荐） |
| `PasswordMS_x.y.z_x64_en-US.msi` | Windows 10/11 x64 | MSI 安装版（企业部署） |
| `PasswordMS_x.y.z-portable-x64.zip` | Windows 10/11 x64 | **免安装绿色版**：解压即用，数据随文件夹走（U 盘可用） |
| `PasswordMS-x.y.z-arm64.apk` | Android 7.0+ (arm64) | 侧载安装（允许安装未知来源应用） |

> 绿色版使用说明：解压到任意**可写目录**（不要在压缩软件预览窗里直接运行），双击 `passwordms.exe`，数据文件自动生成在同目录。升级时用新版 `passwordms.exe` 与 `WebView2Loader.dll` 覆盖旧文件即可。

## 🚀 快速开始

1. 下载并安装（或解压绿色版）
2. 首次启动创建保险库，设置**主密码** —— 请务必牢记：**忘记主密码没有任何找回手段**
3. 添加登录 / 笔记条目，可拖入附件
4. 多设备同步：`设备 → 与新设备配对`，用另一台设备扫码即可（两台设备需在同一局域网）

## 🔐 安全模型

- 保险库为**整卷加密**，密钥由主密码经 Argon2id 派生，绝不落盘
- 局域网同步采用端到端加密：配对后共享保险库密钥，传输数据对局域网内其他设备不可读
- 窗口闲置自动锁定；锁定状态下内存中的敏感数据被清零（zeroize）
- 本软件**不联网上传任何数据**——所有数据仅存本机（及你主动配对的局域网设备）

## ⚠️ 免责声明 / 用户须知

1. **主密码无找回机制**：本软件不设后门、不提供密钥托管。忘记主密码意味着数据永久不可恢复，请妥善保管并定期使用「设置 → 导出」备份。
2. **软件按"现状"提供**：尽管开发中已尽力测试，仍不保证软件完全无缺陷。使用者应自行承担使用风险，并**定期备份重要数据**。
3. **不构成专业安全建议**：本软件不能替代完整的信息安全方案；请在可信设备上使用，并保持操作系统与 WebView2 运行时更新。
4. **局域网风险**：配对设备共享加密数据，请勿在不受信任的网络（公共 Wi-Fi 等）中进行配对操作。
5. 使用本软件即表示您已阅读并同意上述条款。软件依据 [MIT 许可证](LICENSE) 发布，许可条款与上述须知同时适用。

## 🛠️ 本地构建

技术栈：Tauri 2 + Svelte 5 (runes) + Vite 6 + TypeScript 5 + Rust workspace。

```bash
npm install                      # 安装前端依赖
npm run build                    # 前端构建（产物 dist/）
npx tauri build --target x86_64-pc-windows-gnullvm   # Windows 桌面（安装包 + 绿色版 exe）
bash scripts/make-portable.sh    # 打包绿色版 zip
bash scripts/build-android.sh    # Android APK
```

## 📄 许可证

[MIT](LICENSE)
