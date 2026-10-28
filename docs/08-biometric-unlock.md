# 生物识别解锁 · 安全方案（提案 · 待评审）

> **文档定位**：二期首个功能（1.3.0 候选）的**安全决策文档**，不是实现说明。经主理人评审通过后，
> 结论并入 `01-security-req.md`（新增一节）、`02-arch.md`（§3.14.18）、`06-ui-design.md`（§6.24）、
> `07-e2e-checklist.md`（新增走查章），本文降级为背景材料。
>
> **状态**：☑ 已通过（2026-10-28 主理人拍板三参数 + 采纳外部评审意见，见 §16）· ☐ 已否决
> **编号**：待分配（实现阶段按 D-nn 登记缺陷）
> **日期**：2026-10-27

---

## 1. 结论摘要

| 项 | 结论 |
|---|---|
| 做什么 | 用 **Windows Hello** / **Android 生物识别** 在解锁页代替输入主密码——**保险库创建仍然必须主密码** |
| 怎么做 | **旁路密封**：把 32 字节保险库密钥 VK 用平台密钥设施加密后存进一个**独立文件**，容器格式、同步协议、条目格式**零改动** |
| 关键安全属性 | 密封绑定 `vault_id` + 头部 `verify_mac`；**改主密码 = 旁路自动失效**（熵不匹配，解不开），无需额外协调 |
| 默认状态 | **默认关闭**。开关在「设置 · 安全」，开启需输入一次主密码 |
| 强度真相 | **两端不对称**：Android 可做到密钥绑定生物特征注册集（硬绑定）；Windows 只能做到「Hello 在场校验 + DPAPI 用户域加密」（软绑定，见 §6.3）。**必须在 UI 文案上如实区分，不许统一吹成「指纹加密」** |
| 最大新增风险 | Windows 侧：拿到本机 Windows 用户登录凭据的恶意程序理论上可绕过 Hello 直接调 DPAPI ⇒ **快速解锁的强度上限 = 你的 Windows 账户锁屏强度**。Android 侧无此问题 |
| 回滚方式 | 删掉 `biometric.json` 即回到「只认主密码」。老版本不读该文件，**向下兼容零影响** |

---

## 2. 现状事实（全部来自代码，非推断）

### 2.1 密钥层级

```
主密码 ──Argon2id(m=64MiB, t=3, p=1, salt=16B)──▶ MK（仅内存，Zeroizing）
                                                     │
                                       HKDF-SHA256 ──┼──▶ KEK（包裹 VK 用）
                                                     └──▶ verify_key ──HMAC──▶ verify_mac
VK（32B 随机，数据加密之根）──AEAD(KEK)──▶ wrapped_vault_key ──▶ 容器头部（明文 JSON 字段）
```

代码位置：`crates/passwordms-core/src/session.rs:89-121`（创建）、`:134-218`（解锁）、
`crate::crypto::keys`（包裹/解包）、`crate::crypto::kdf`（派生）。

### 2.2 解锁链路的四个前置事实

| # | 事实 | 影响 |
|---|---|---|
| F1 | 解锁必须跑完整 Argon2id（且失败路径**故意补跑**一次以均衡耗时，`session.rs:337-350`） | 免密解锁的**唯一**价值就是跳过这次 KDF 与「回忆长密码」：单次耗时**本机实测 ≈ 1.89 s / 2.05 s**（`04-qa.md` 时序测试记录的 KDF 基准；低端 Android 为 **预估** 0.8–1.6 s，见 `02-arch.md:150`） |
| F2 | 容器头部是**明文**（`kdf` / `kdf_salt` / `verify_mac` / `wrapped_vault_key`），解锁前即可读 | 绑定用的 `verify_mac` 在**输密码之前就可得**，生物解锁路径可用它做熵 |
| F3 | 会话里的 `kek` 只有**一处**消费：`adopt_vault_key()`（配对加入时用本地 KEK 重包裹对端 VK，`session.rs:310`） | 生物解锁拿不到 KEK ⇒ 只需把该字段改成 `Option<Kek>`，影响面 2 处 |
| F4 | `change_master_password()` 会重写头部 `verify_mac` 与 `wrapped_vault_key`，**VK 本身不变**（`session.rs:277-290`） | `verify_mac` 是「主密码身份」的稳定指纹 ⇒ 拿它做绑定熵，改密码天然失效 |

### 2.3 结论

> 免密解锁**不需要**改动密码学模型：只需把**同一个 VK** 另外用平台密钥设施包一份，
> 平台校验通过（Hello / 指纹）后解出来直接喂给 `Repository::open_with_container`。
> Argon2、KEK、verify_mac、容器格式、同步协议**全部不动**。

---

## 3. 核心矛盾与裁定

### 3.1 矛盾

`01-security-req.md:36` 与安全底线第 3 条冻结了「**主密码不落盘**，仅存内存」。
而「免输主密码」在逻辑上要求**某种能替代主密码的东西落盘**——二者字面冲突。

### 3.2 裁定（建议，待主理人确认）

**不修改底线原文，而是给它补一条限定**：

> **底线 3 增补**：主密码及其**派生前的明文**永不落盘。
> 经用户显式开启后，**保险库密钥 VK** 可由**平台密钥设施**（Windows DPAPI+Hello / Android Keystore+BiometricPrompt）
> 加密后落盘；该密文**不构成主密码的等价物**——它不派生任何东西、不可用于恢复、删除即失去作用域，
> 且**绑定主密码版本**（改主密码立即失效）。

理由：底线 3 的原始目的是「防止拖库者从磁盘得到口令材料」，而 VK 密文的解密能力被平台（TPM/Keystore/用户登录态）持有，
**离线拿到磁盘副本者什么也得不到**（见 §7 威胁 T1）。

### 3.3 明确越界（不做）

- ❌ 不缓存主密码、不缓存 MK、不缓存 KEK（KEK 解不出也不存）
- ❌ 不做「仅生物特征创建保险库」——首次创建**必须**主密码，保证永远存在一条与人无关的恢复路径
- ❌ 不同步、不上传旁路密文（局域网同步协议不感知它）

---

## 4. 方案设计

### 4.1 总体形状：旁路密封（side-car seal）

```
<app_data>/vault.pmsv            ← 现有容器，一个字节都不改
<app_data>/biometric.json        ← 新增：旁路密封清单
<平台密钥库>                      ← Windows: DPAPI 用户域（无需自建文件）
                                    Android: AndroidKeyStore 别名 passwordms.biometric.vk
```

**为什么独立文件、不塞进容器头部**：
1. 容器头部重写会触发 `update_header` → `rebuild_index`（`repository.rs:556-568`），让「开关一个 UI 选项」牵扯到索引重建，风险与收益不匹配；
2. 旁路文件**丢失/损坏**与「保险库损坏」必须解耦——前者的正确处置是「删掉它，回密码」，后者是灾难；放进同一文件会让恢复逻辑纠缠；
3. Android 的 Keystore 密文本来就只能存应用私有目录，独立文件是两端统一的形状。

### 4.2 `biometric.json` 结构

```json
{
  "version": 1,
  "vault_id": "3f2a…（16 进制 UUID，与容器头部同源）",
  "platform": "windows" | "android",
  "platform_subtype": "windows-dpapi-user" | "windows-tpm2-seal" | "android-keystore",
  "sealed_vk": "base64（平台加密后的 VK 密文，含平台侧 IV/nonce）",
  "created_at": 1761574800000
}
```

- **`platform` 与 `platform_subtype` 分开**（采纳外部评审建议）：前者是平台大类、参与能力判断；后者是保护手段细分。
  **v1 就预留 `windows-tpm2-seal` 取值**——字段一旦发布就无法回头加（老版本读到未识别取值只能当「未开启」），
  现在留一个枚举值成本为零，TPM 实现可以晚到 1.3.x。
- 文件权限：Android 应用私有目录；Windows `%APPDATA%\PasswordMS\`（与容器同目录，同 ACL）
- **不含**任何可用于识别用户的信息；不含主密码任何形式的派生
- 解码后长度、版本、平台名/子类型全部**白名单校验**，不符即当「不存在」处理（不报错、不崩溃）

### 4.3 绑定设计（本方案的关键）

密封时把一段**绑定串**作为平台加密的附加输入：

```
bind = SHA-256( "passwordms/v1/biometric" ‖ vault_id ‖ header.verify_mac )
```

| 平台 | 承载方式 | 效果 |
|---|---|---|
| Android | `Cipher.updateAAD(bind)`（AES-GCM 附加认证数据） | AAD 不符 → `AEADBadTagException` |
| Windows | `CryptProtectData` 的 **`pOptionalEntropy`** | entropy 不符 → `ERROR_INVALID_DATA` |

**由此自动获得三个性质**（不需要任何额外协调代码）：

| 事件 | 结果 |
|---|---|
| 用户改主密码（`verify_mac` 变） | 旁路**立即解不开** ⇒ 提示「主密码已变更，请重新开启快速解锁」 |
| 旁路文件被复制到另一个保险库（`vault_id` 不同） | 解不开 ⇒ 拒绝 |
| 密文被篡改 | AEAD/DPAPI 校验失败 ⇒ 拒绝 |

> 说明：`change_master_password` 成功后再**主动删除** `biometric.json`（双保险：主动清理 + 被动失效），
> 这样用户不会看到「明明刚改过密码却弹一个奇怪的解密失败」。

### 4.4 解锁链路（生物路径）

```
1. 前端读 unlock 页状态 → 调 biometric_status()
2. 后端：读 biometric.json → 白名单校验 → 读头部 verify_mac → 算 bind
3. 后端：平台校验（Windows: UserConsentVerifier；Android: BiometricPrompt+CryptoObject）
   ├─ 用户取消 / 失败 → 返回 BiometricCanceled，前端**静默**回主密码输入框（不弹红、不自动重试）
   ├─ 平台不可用   → 返回 BiometricUnavailable（前端隐藏按钮）
   └─ 通过 → 解封得 VK；解封失败 → BiometricInvalidated（见 §8 矩阵）
4. Repository::open_with_container(container, vk, device_id) → 解锁成功
5. Session 置为 Unlocked { repo, kek: None, … }；重置自动锁定计时
```

**与主密码路径的差异只有一处**：`kek = None`。`adopt_vault_key()`（配对加入）在此状态下返回
`CoreError::NeedMasterPassword`，前端引导「请用主密码解锁后再配对」——该场景仅发生在「空库 + 首次配对」，
实机上不会从生物路径进入（空库是新创建的，创建必须密码）。

### 4.5 开启 / 关闭流程

| 动作 | 要求 | 理由 |
|---|---|---|
| **开启** | 必须**输入主密码**（不能只凭当前已解锁会话） | 防「解锁后离开座位，旁人顺手开启一条永久免密通道」 |
| 开启时平台校验 | 先跑一次平台校验并成功才落盘 | 避免存下一条当场用不了的封印，用户下次才发现 |
| **关闭** | 一次确认弹窗即可，**不要求**主密码 | 关闭是**降级**操作、不泄露任何数据；且忘掉主密码的用户必须能自救 |
| 关闭效果 | 删除 `biometric.json` + 删除 Android Keystore 别名（Windows 无需清理） | 真删除，不留「下次自动恢复」 |
| 重复开启 | 覆盖旧文件 + 先删旧 KeyStore 别名，**保证零残留** | 幂等守门测试项 |

### 4.6 会话形状变更

```rust
pub struct Unlocked {
    pub repo: Repository,
    /// 主密码路径 = Some；生物路径 = None（拿不到 KEK）。
    pub kek: Option<Kek>,
    pub maintenance_warnings: Vec<String>,
}
```

影响面（已逐处核对）：`session.rs:39`（定义）、`:289`（改密后赋值）、`:310`（`adopt_vault_key` 取值）。
`lock()` 的级联清零语义不变（`Option` 内的 `Zeroizing` 照样 Drop）。

### 4.7 写入与损坏处理（沿用容器既有范式，不发明新轮子）

本仓 `container.rs:250-303` 已有一套经过验证的崩溃安全序列，旁路文件**直接复用同一范式**：

| 步骤 | 做法 | 理由 |
|---|---|---|
| 1 | 写同目录 `biometric.tmp.json`（同卷才能原子 rename） | 跨卷 rename 会退化成复制，非原子 |
| 2 | `file.sync_all()`（**不是 `flush()`**，对 `File` 的 `flush` 是 no-op） | 只 rename 不刷盘 = 换上一个内容还在页缓存的空壳 |
| 3 | **rename 之前**重新读回并解析校验 | 落盘的必须已经是完整可用文件 |
| 4 | `fs::rename` 原子替换（Windows 走 `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`；杀软/索引器瞬时占用 → 带退避重试） | 同上，本仓已有重试先例 |
| 失败时 | 旧文件一字未动；**tmp 刻意保留**用于诊断 | 与 `rewrite_with_frames` 的策略一致 |

**不引入 `.bak` 备份文件**（否决外部评审的「bak 兜底」）：rename 前的校验已经保证「被换上去的一定是完整文件」，
唯一真实的破坏窗口是「写到一半掉电」——那一瞬间旧文件仍在，轮不到 bak 出场。
多养一份 `.bak` 只会制造「关闭快速解锁后仍残留一份旧密钥密文」的清理负担与安全审计疑点。

读取侧：`biometric.json` 解析失败 → 当「未开启」→ 设置页显示「快速解锁配置损坏，已关闭，请重新开启」，
并保留 tmp 供事后排查（**不静默删除**，与 §9 反向守门一致）。

---

## 5. 落点与依赖

| 层 | 位置 | 内容 |
|---|---|---|
| core（平台无关） | `crates/passwordms-core/src/biometric.rs`（新） | `SealBlob` 编解码、`bind_of(vault_id, verify_mac)`、`trait VaultSealer`、纯逻辑编排 |
| core 测试 | `crates/passwordms-core/tests/biometric_wrap.rs`（新） | §9 的 8 例（含 mock sealer，全平台无关，任何机器可跑） |
| 桌面 adapter | `src-tauri/src/platform/biometric_windows.rs`（新） | DPAPI + UserConsentVerifier |
| Android adapter | `src-tauri/gen/android/…/BiometricPlugin.kt` + Rust 侧桥（新） | Keystore + BiometricPrompt，Tauri 2 mobile plugin |
| 命令层 | `src-tauri/src/commands/vault_cmd.rs` + `biometric_cmd.rs`（新） | `biometric_status` / `biometric_enable` / `biometric_disable` / `biometric_unlock` |
| 前端 | `src/components/UnlockScreen.svelte`、`Settings.svelte` | 解锁按钮 + 设置行；i18n 四语言 |

新增 Rust 依赖（**仅桌面目标**）：`windows` crate
features = `Win32_Security_Cryptography`（DPAPI）、`Security_Credentials_UI`（UserConsentVerifier）、`Foundation`。
Android 侧不新增 Rust 依赖（Kotlin 插件 + JNI 桥）。

**不用**社区插件 `tauri-plugin-biometric`：它只覆盖 Android/iOS 的 BiometricPrompt 判定，
**不提供 Keystore `CryptoObject` 绑定**——而「密钥与生物特征绑定」正是本方案 Android 侧的全部价值，用它会退化成「弹个框就算过」。

---

## 6. 平台实现要点

### 6.1 Windows：DPAPI 加密 + Hello 在场校验

```
开启：UserConsentVerifier::RequestVerificationAsync("开启快速解锁")  ✅
      → CryptProtectData(vk, entropy=bind, flags=CRYPTPROTECT_UI_FORBIDDEN) → sealed_vk
解锁：UserConsentVerifier::RequestVerificationAsync("解锁 PasswordMS") ✅
      → CryptUnprotectData(sealed_vk, entropy=bind) → vk
可用性：UserConsentVerifier::CheckAvailabilityAsync()
        Available / DeviceNotPresent / NotConfiguredForUser / DisabledByPolicy / DeviceBusy
```

**必须如实告知用户的两个事实**：
1. `UserConsentVerifier` **不暴露认证方式**——PIN、指纹、人脸都算通过，程序无法强制「必须指纹」。
   所以 Windows 侧一律称「**Windows Hello 快速解锁**」，不称「指纹解锁」。
2. DPAPI 的用户域加密绑定的是「**本机 Windows 用户登录态**」。因此快速解锁的安全上限 = 该账户的锁屏强度；
   能拿到该用户登录凭据的本地恶意程序可以绕过 Hello 直接解密。开启时的说明文案必须写明这一点。

> **为什么不从 Hello 签名派生密钥**：Hello 私钥不可导出、只能签名，看似可「签名固定挑战 → HKDF 出 KEK」。
> 但 (a) 签名是否确定性属**未文档化的实现细节**（算法可随平台版本变化，一旦变成随机化签名，用户数据即永久无法恢复）；
> (b) Hello 密钥可被用户在系统设置中重置或设备重置撤销，派生路径会连带毁掉密钥。
> 这两点都是「不可接受的可用性风险」，故不采用。

### 6.2 Android：Keystore 硬绑定 + BiometricPrompt

```
生成（API 30+）：
  KeyGenParameterSpec.Builder("passwordms.biometric.vk", PURPOSE_ENCRYPT|DECRYPT)
    .setBlockModes(GCM).setEncryptionPaddings(NONE)
    .setKeySize(256)
    .setUserAuthenticationRequired(true)
    .setUserAuthenticationParameters(0, AUTH_BIOMETRIC_STRONG)   // 0 = 每次使用都要认证
    .setInvalidatedByBiometricEnrollment(true)                  // 新增指纹 → 密钥永久失效
    .setUnlockedDeviceRequired(true)                            // API 28+
    .setRandomizedEncryptionRequired(true)
加密：cipher.init(ENCRYPT, key); cipher.updateAAD(bind); sealed = cipher.doFinal(vk)
解锁：BiometricPrompt.authenticate(promptInfo, CryptoObject(cipher))
      → 认证通过后 cipher 才可用 → cipher.updateAAD(bind); vk = cipher.doFinal(sealed)
```

- **API 24-29 分叉**：`setUserAuthenticationValidityDurationSeconds(-1)` + 每次 `CryptoObject` 绑定（语义等价）
- minSdk = 24（现状），因此必须写双路径，守门测试覆盖低版本逻辑分支（逻辑可单测，Keystore 行为只能真机验）
- 异常映射：`KeyPermanentlyInvalidatedException` → 删除旁路 + `BiometricInvalidated`（提示「生物特征有变更，已关闭快速解锁」）
- `BiometricManager.canAuthenticate(BIOMETRIC_STRONG)`：未录入 / 硬件不可用 → 设置页显示「不可用」并禁用开关

### 6.3 两端强度不对称（不许模糊）

| 维度 | Android | Windows |
|---|---|---|
| 密钥与生物特征的关系 | **硬绑定**：密钥在 Keystore 内，用一次要认证一次，新增指纹即永久失效 | **软绑定**：DPAPI 用户域 + Hello 在场闸门，程序可被绕过 |
| 设备 PIN 能否顶替 | ❌ 不能（限 `AUTH_BIOMETRIC_STRONG`） | ✅ 能（API 不区分） |
| 离线磁盘副本 | 无用 | 无用 |
| 拿到用户登录态的攻击者 | 仍需生物认证 | **可通过 DPAPI 解密** |
| UI 文案 | 「指纹 / 面容解锁」 | 「Windows Hello 快速解锁」+ 风险说明 |

---

## 7. 威胁模型

| # | 攻击者能力 | 结果 | 结论 |
|---|---|---|---|
| T1 | 拿到磁盘/备份副本（无设备、无登录态） | 旁路密文无法解密（DPAPI 用户域密钥不在磁盘 / Keystore 在 TEE） | ✅ 与现状同强度 |
| T2 | 拿到设备 + 知道锁屏密码（Windows） | 可调 DPAPI 解密 ⇒ 绕过 Hello | ⚠️ **已知降级**，靠开启时告知 + 建议强锁屏密码缓解 |
| T3 | 拿到设备 + 知道锁屏密码（Android） | 仍需通过 BiometricPrompt（密钥要求每次生物认证） | ✅ 无降级 |
| T4 | 恶意程序以本用户身份运行（Windows） | 可调 DPAPI ⇒ 绕过 Hello | ⚠️ 与 T2 同源；但该程序在内存中本就能抓已解锁会话，增量风险有限 |
| T5 | 恶意程序以本用户身份运行（Android，已 root） | Keystore 密钥不可导出；需诱导用户通过一次生物认证 | ✅ 显著更强 |
| T6 | 攻击者把旧 `biometric.json` 拷回（回滚） | 主密码未变时仍可用（等价于「没改过密码」），主密码变过则失效 | ✅ 无新增面 |
| T7 | 攻击者篡改 `biometric.json`（改 vault_id/密文） | 绑定熵不符 ⇒ 解密失败 ⇒ 回密码 | ✅ |
| T8 | 内存取证（解锁后） | VK 在内存 —— **与现状完全一致** | ✅ 无变化 |
| T9 | 攻击者关闭快速解锁后植入？ | 关闭不产生任何持久密钥；重新开启需主密码 | ✅ |
| T10 | 恶意 App 抢在 PasswordMS 前弹生物框（Android 覆盖攻击） | Tauri 走全屏 Activity，BiometricPrompt 由系统绘制；仍需结合 `setUnlockedDeviceRequired` 与锁屏 | ⚠️ 平台级残余风险，登记为已知限制 |

---

## 8. 失效与恢复矩阵

| 事件 | 旁路状态 | 用户看到 | 需要动作 |
|---|---|---|---|
| 改主密码 | 主动删除 + 被动失效 | 「主密码已变更，快速解锁已关闭」 | 重新开启（输新主密码） |
| Android 新增/删除指纹 | 密钥永久失效 | 「生物特征有变更，已关闭快速解锁」 | 重新开启 |
| Windows Hello 被重置/PIN 变更 | DPAPI 仍可解（用户域未变）→ Hello 校验失败 | 「Windows Hello 不可用，请用主密码」 | 修复 Hello 后可继续用 |
| 换机器 / 重装系统 | 旁路文件不存在或 DPAPI 解不开 | 只剩密码框 | 密码解锁即可，无需处理 |
| **远程桌面（RDP）会话** | `UserConsentVerifier` 在 RDP 下不可用 | 隐藏「Hello 解锁」按钮，**不删**旁路 | 本地会话照常可用；不因临时不可用而毁掉封印 |
| **Android 备份/换机恢复** | Keystore 密钥不随备份迁移 ⇒ 封印必然解不开；且本项目 `allowBackup="false"`（manifest 已在册），系统本就不会备份 | 「生物特征/设备已变更，已关闭快速解锁」 | 密码解锁后重新开启 |
| 用户手动删 `biometric.json` | 视为未开启 | 只剩密码框 | 无 |
| 生物校验被取消 | 不变 | **静默**回密码框（无红条、无重试） | 输入密码 |
| 生物校验连续失败 | 不变（节流交给系统，不自行加锁） | 提示「多次失败，请用主密码」 | 输入密码 |
| 保险库文件损坏 | — | 走既有 `UnlockFailed` 统一报错（A-05） | 不变 |

---

## 9. 守门测试清单（实现阶段必须全绿）

**A. core 层（平台无关，mock sealer，任何机器可跑）** — `tests/biometric_wrap.rs`（**已落地，13 例全绿**）

| # | 用例（测试函数名） | 守的是什么 |
|---|---|---|
| 1 | `seal_record_roundtrip`：编解码 round-trip + 解封取回 VK | 结构稳定 |
| 2 | `foreign_vault_seal_is_absent`：`vault_id` 不匹配 → 当「未开启」 | 跨库复用（且不误报错误） |
| 3 | `master_password_change_invalidates_seal`：`verify_mac` 变 → `Stale(MasterPasswordChanged)` | **改主密码自动失效**（本方案核心性质） |
| 4 | `tampered_ciphertext_is_classified_as_tampered`：改密文一字节 → 判篡改（且不误判为改密） | 完整性 + 失效原因可区分 |
| 5 | `unknown_version_is_absent`：版本号越界 → 当「未开启」，不 panic | 畸形输入 |
| 6 | `unknown_subtype_is_absent`：`platform_subtype` 未识别 → 当「未开启」 | 前向兼容（旧版本读新 blob） |
| 7 | `re_enable_replaces_and_leaves_no_tmp`：重复开启 → 只留新密文、**无 tmp 残留** | 幂等 |
| 8 | `failed_enable_keeps_previous_seal`：开启失败 → 既有封印**字节不变**且仍可用 | 失败不动旧文件（§4.7） |
| 9 | `password_unlock_is_unaffected_without_seal`：无旁路文件时主密码解锁照旧 | 回归（老用户零影响） |
| 10 | `sealed_session_needs_master_password_for_adoption`：`kek=None` + `has_kek()=false` → `NeedMasterPassword` | 会话形状变更（§4.6） |
| 11 | `canceled_prompt_is_reported_as_canceled`：取消 → `ERR_BIOMETRIC_CANCELED`（**非** `UNLOCK_FAILED`） | 静默降级可判别 |
| 12 | `platform_invalidation_maps_to_enrollment_change`：平台密钥失效 → `..._enrollment` 文案 | 指纹变更可区分 |
| 13 | `unavailable_platform_is_not_treated_as_invalidated`：平台不可用 → 不可用错误，且**拒绝开启** | 不误判失效、不产「当场用不了」的死封印 |

**B. 平台 e2e（真机，进 `07-e2e-checklist.md`）**

| # | 场景 |
|---|---|
| 1 | Win：开启 → 锁定 → Hello 解锁 → 进主界面 |
| 2 | Win：开启 → 改主密码 → 提示已关闭 → 重新开启可用 |
| 3 | Win：取消 Hello 弹框 → 静默回密码页（无红条） |
| 4 | Android：开启 → 强杀进程 → 指纹解锁 |
| 5 | Android：新增指纹 → 提示失效 → 密码解锁 → 重新开启 |
| 6 | Android：取消生物框 → 静默回密码页 |
| 7 | 两端：关闭开关 → 只剩密码框，开关状态正确 |
| 8 | 两端：删 `biometric.json` 后启动不崩、开关显示关闭 |

**C. 反向守门（防「为了绿而绿」）**
- 断言失败路径返回的**错误码分类**，不只断言「失败」——否则「一律返回 UnlockFailed」也能让测试变绿，掩盖 UX 退化。

---

## 10. 错误码与 i18n

| 场景 | 建议错误码 / key | 前端表现 |
|---|---|---|
| 平台不可用/未配置 | `ERR_BIOMETRIC_UNAVAILABLE` | 设置页开关禁用 + 原因文案；解锁页不渲染按钮 |
| 用户取消 | `ERR_BIOMETRIC_CANCELED` | **静默**回密码框 |
| 密文失效 · 主密码已变更 | `ERR_BIOMETRIC_INVALIDATED` + `InvalidatedReason::MasterPasswordChanged` | 提示「主密码已变更，快速解锁已关闭」+ 删旁路 + 开关置关 |
| 密文失效 · 生物特征有变更 | 同上 + `BiometricEnrollmentChanged` | 提示「生物特征有变更，已关闭快速解锁」 |
| 密文失效 · 密文被篡改 | 同上 + `Tampered` | 提示「快速解锁配置损坏，已关闭」+ **保留 tmp** 供排查 |
| 生物态需主密码 | `ERR_NEED_MASTER_PASSWORD` | 引导输入密码（配对入口） |

i18n 新增键（按 D-102，**四语言同加**）：`unlock.bio_button` / `settings.bio_title` / `settings.bio_on` /
`settings.bio_off` / `bio.enable_hint`（Windows 含锁屏强度说明）/ `bio.invalidated` / `bio.unavailable` / `bio.canceled`（可空文案）。

---

## 11. 分批实施（每批自带可验证门）

| 批 | 内容 | 交付门 |
|---|---|---|
| **P1** ✅ **已完成（2026-10-28）** | core：`biometric.rs` + `trait VaultSealer` + §9-A 全 13 例 + `kek: Option<Kek>` 改造 + 原子写（复用 `container.rs` 范式） | core **251 例全绿**（含新增 13 集成 + 2 单测）、clippy 0、fmt 过；前端零改动 ✅ |
| **P2** | Windows adapter + 四个命令 + 设置页/解锁页 + i18n 四语言 + 三处「主密码不可丢失」文案触点 + 失效原因细分文案 | 前端三门绿；`npx tauri build` 出包；Win 真机 e2e 1-3 |
| **P3** | Android 插件（Kotlin + 桥）+ 移动端 UI + `canAuthenticate(BIOMETRIC_STRONG)` 预检 | `build-android.sh` 双门过；Android 真机 e2e 4-6 |
| **P4** | 文档融合（01-security-req 新节 / 02-arch §3.14.18 / 06-ui §6.24 / 07-e2e 新增）+ 两端全量回归 | 四文档同改 + e2e 8 项全勾 + v1.3.0 出包 |
| **1.3.x（backlog）** | ①Windows TPM2 密封（`platform_subtype` 已预留）②生物会话「导出全库」需主密码 ③VK 不跨 JS 边界的契约测试 | 各自由独立轮次开工，不阻塞 1.3.0 |

> **P1 可独立合入**：不影响任何现有路径（默认关闭 + 无旁路文件时行为不变），出包也不改变用户可见行为。
> 这样即使 P2/P3 拖长，P1 的守门测试已经在守「改主密码必须失效」这条命门。

---

## 12. 明确不做

| 不做 | 理由 |
|---|---|
| 生物特征创建保险库 | 会失去「与人无关的恢复路径」；忘密码 = 永久丢数据，与产品底线冲突 |
| 缓存主密码 / MK / KEK | 直接违背底线 3 |
| 同步旁路密文 | 会让「换机免密」变成「跨设备密钥分发」，安全模型整体变形 |
| 依赖 `tauri-plugin-biometric` 做解密 | 它只有「弹框判定」，没有 Keystore `CryptoObject` 绑定 ⇒ 安全价值归零 |
| Windows 侧从 Hello 签名派生密钥 | 依赖未文档化的签名确定性 + Hello 密钥可被撤销（§6.1） |
| 自行设计失败节流 | 与 Hello / BiometricPrompt 的系统级 lockout 打架，反而降低安全性 |

---

## 13. 三个参数（**已拍板** 2026-10-28，主理人确认按建议执行）

| # | 议题 | 拍板结果 | 落地要点 |
|---|---|---|---|
| **P-1** | Windows 侧是否接受「PIN 也算通过」（API 无法区分 PIN 与指纹） | ✅ **接受**（方案 A） | 文案统一写「Windows Hello 快速解锁」；开启时披露「安全上限 = 本机 Windows 锁屏强度」 |
| **P-2** | 开启快速解锁是否强制输入主密码 | ✅ **强制**（方案 A） | 开启流程要求输一次主密码，不认可「仅凭已解锁会话」 |
| **P-3** | 默认关闭 vs 创建向导里询问一次 | ✅ **默认关闭**（方案 A） | 仅在「设置 · 安全」提供入口；创建流程零打扰 |

---

## 14. 风险登记

| 风险 | 级别 | 缓解 |
|---|---|---|
| Windows 强度上限受锁屏密码拖累（T2） | 中 | 开启文案披露；文档写明「不是指纹加密」；后续可议 TPM 直绑定 |
| Android 双 API 路径（24-29 / 30+）分支多 | 中 | 逻辑分支单测覆盖；Keystore 行为真机回归；e2e 至少覆盖一台 API<30 设备 |
| 加解密失败被误判为「密码错」 | 低 | 错误码分开（§10），且**不得**把生物失败折叠成 `UnlockFailed` |
| P1 的 `Option<Kek>` 改动波及 | 低 | 影响面 2 处（已核对 session.rs:289/310）；`lock()` 清零语义不变 |
| 用户误以为「开了生物识别就不用记密码」 | 中 | 开启与关闭的文案、帮助文本统一强调「主密码仍是唯一恢复手段」 |

---

## 15. 与原文档的关系

| 文档 | 待并入内容（P4 执行） |
|---|---|
| `01-security-req.md` | 新增 §「平台密钥设施与快速解锁」，含底线 3 增补原文（§3.2） |
| `02-arch.md` | 新增 §3.14.18：旁路密封结构、绑定熵、`kek: Option`、适配层 trait |
| `06-ui-design.md` | 新增 §6.24：解锁页按钮、设置行、四语言文案、失败态（静默/提示分级） |
| `07-e2e-checklist.md` | 新增「快速解锁」章，8 项（§9-B） |
| `00-scope.md` | P2 列表中「生物识别解锁」标注「已立项 · 1.3.0」 |

---

## 16. 外部评审处置（豆包建议，2026-10-27 收到 · 小李逐条评估）

> 评估原则：**只采纳能提高真实安全性或真实可用性的条目**；对「听起来安全但实际不改变攻击面」的条目宁可拒绝，
> 避免把复杂度当成安全性。

### 16.1 逐条结论

| # | 豆包建议 | 处置 | 理由 / 落地位置 |
|---|---|---|---|
| 1 | Windows 加 TPM2 密封备选路径 | ⚠️ **部分采纳**（关键判断纠正） | **收益方向要改写**：TPM 密封**不缓解**「同用户进程绕过」（同进程照样能调 `NCryptUnseal`），它真正挡的是「**磁盘镜像离线解密**」——DPAPI 主密钥在知道 Windows 账户口令时可离机解开，TPM 密文不能离机使用。故：①`platform_subtype` **v1 就预留** `windows-tpm2-seal`（字段晚加即不兼容，见 §4.2）；②**实现排 1.3.x**，不进 1.3.0（Win 侧要测无 TPM 的虚拟机回退、PCR 策略、系统更新后失效面，工程量与收益不匹配首发）。§4.2 / §11 |
| 2 | `kek=None` 的 UX 引导 + 「会话内记住主密码验证」 | ⚠️ **部分采纳** | 引导模态框**采纳**（它只是把已有错误码翻译成人话）；「记住主密码/KEK」**否决**——`adopt_vault_key` 只需要**一次性** KEK，用完即弃，缓存它零收益、只增暴露面。P2 落地 |
| 3 | `biometric.json` 原子写 + `.bak` 备份 | ⚠️ **部分采纳** | 原子写**必须做**，且**直接复用 `container.rs:250-303` 既有范式**（tmp → `sync_all` → rename 前校验 → 带重试 rename，失败保留 tmp）；**`.bak` 否决**：rename 前校验已保证「换上去的一定是完整文件」，唯一的真实破坏窗口（写一半掉电）发生时旧文件还在，轮不到 bak；多养一份 `.bak` 反而留下「关闭后仍有旧密钥密文残留」的清理负担。§4.7 / §9-A#8 |
| 4 | 多触点强化「主密码不可丢失」 | ✅ **采纳** | 成本极低、直接对冲「误以为生物识别可替代备份」这一最现实的用户风险。三处触点写进 P2（开启勾选确认 / 设置页常驻小字 / 解锁页回退提示） |
| 5 | 失效原因细分枚举 + 分类文案 | ✅ **采纳** | `MasterPasswordChanged` / `BiometricEnrollmentChanged` / `Tampered` 三值，映射三套文案；不构成侧信道（这些信息本地可自行推断）。§10 |
| 6 | 高危操作强制二次验证主密码（默认开启） | ⚠️ **收窄后延后** | 全量强制会改变**所有**用户（含主密码会话）的操作流，属独立产品决策，不能夹带进生物识别首发。**收窄为**：仅「生物会话 + 导出全库」需主密码，排 1.3.x |
| 7 | 胁迫密码 / 诱饵保险库 | ❌ **否决**（1.x 不做） | ①「轻量版」= 解锁后偷偷禁用导出，是**安全剧场**，且攻击者一旦发现异常可能升级暴力；②真做需要**双库架构**，与同步/回收站/附件/体检全交叉，属大版本工程；③与「个人工具 · 单库 · 零知识」范围冻结冲突 |
| 8 | Android 生物安全等级检测与警告 | ⚠️ **部分采纳** | 开启前 `BiometricManager.canAuthenticate(BIOMETRIC_STRONG)` 预检 + 不可用则禁用开关：**采纳**。但「**允许用户强行开启**」（豆包原文）**自相矛盾**：密钥生成时要求 `AUTH_BIOMETRIC_STRONG`，WEAK-only 设备上生成/使用必然失败——强行开启只会造出一个永远解不开的封印。故 WEAK 设备一律**禁止开启**，文案说明原因。P3 |
| 9 | VK 明文生命周期收窄 / 不跨 JS 边界 | ⚠️ **部分采纳** | 现状已满足（VK 只在 core 内，IPC 只回 DTO，从未跨 JS 传明文）；采纳的是**把它变成可执行约束**：加一条契约测试（命令返回类型中不得出现 `vk` 字段）+ 代码注释铁律，排 1.3.x |
| 10 | 多保险库：`biometric.json` 改数组结构 | ❌ **暂不采纳** | 当前是单库设计；`version` 字段已留升级位，真到多库时再上 `version:2` 的 `seals[]`，现在就改成数组是**为不存在的需求付复杂度** |
| 11 | Windows Hello 撤销「主动检测并自动删」 | ⚠️ **部分采纳** | 检测**采纳**（每次进解锁页探测可用性）；**自动删除否决**——`DeviceBusy` / RDP 会话 / 系统更新中途都会瞬时不可用，静默删旁路会让用户「什么都没做，快速解锁就没了」。改为：不可用时**隐藏按钮 + 显示原因**，仅在用户主动操作或明确的「本机未配置 Hello」时提示清理。§8 |
| 12 | 安全诊断页面 | ❌ **暂不采纳** | 属运维/排障向 UI，收益低、四语言与窄屏成本实打实；先把「当前保护方式」一行放进设置页（P2 已含），完整诊断页留给有真实排障需求时再说 |

### 16.2 它对、但本项目已经做到了的两点

| 豆包的担心 | 现状 |
|---|---|
| 「Android 备份会把旁路文件带走，恢复后解不开」 | `AndroidManifest.xml:12` 已 `android:allowBackup="false"` + `dataExtractionRules` / `fullBackupContent`，系统本就不备份应用私有目录；方案里只需把它写成约束（§8 新增一行） |
| 「VK 可能泄露到 JS 上下文」 | 现状 IPC 只返回 DTO，VK 从未出过 core；补一条契约测试即可（见 16.1 #9） |

### 16.3 它的四条红线，与本方案一致

「不并入主容器」「不进同步」「不缓存主密码/MK」「不允许生物识别创建或恢复保险库」——与本文 §3.3 / §12 完全同向，无需调整。

### 16.4 小李额外补的两条（豆包未提）

1. **RDP 会话**：`UserConsentVerifier` 在远程桌面下不可用，必须「隐藏按钮、不删封印」，否则远程排障一次就把本地快速解锁毁了（已入 §8）。
2. **Windows 无 TPM 的虚拟机**：VM 里 DPAPI 可用但无硬件信任根，开启文案不应暗示「有硬件保护」——与 §6.3 的「不许吹」同一条纪律。

---

## 17. P1 落地记录（2026-10-28 · core 层已完成）

### 17.1 新增 / 改动文件

| 文件 | 性质 | 内容 |
|---|---|---|
| `crates/passwordms-core/src/biometric.rs` | **新增** | `SealRecord`、`SealStore`（原子写 / 幂等删除 / 三态检查）、`VaultSealer` trait、`Availability` / `SealError` / `InvalidatedReason` / `SealState`、`bind_of` / `bind_check_of`、`enable` / `unlock` |
| `crates/passwordms-core/src/session.rs` | 改 | `Unlocked.kek` → `Option<Kek>`；新增 `unlock_with_vault_key()`（生物入口）、`has_kek()`；抽出两条路径共用的 `finish_unlock()`；`adopt_vault_key()` 无 KEK 时返回 `NeedMasterPassword` |
| `crates/passwordms-core/src/error.rs` | 改 | 新增 4 个错误码 + 4 个变体（失效原因细分三套文案） |
| `crates/passwordms-core/src/lib.rs` | 改 | 注册 `pub mod biometric` |
| `crates/passwordms-core/tests/biometric_wrap.rs` | **新增** | 13 例守门（§9-A） |

### 17.2 对外 API（P2/P3 直接接线）

```rust
// 开启（命令层须先用主密码校验身份，再取已解锁会话的 vk / 头部字段）
enable(&store, &sealer, &header.vault_id, &header.verify_mac, &vk[..]) -> CoreResult<SealRecord>
// 解锁
unlock(&store, &sealer, &header.vault_id, &header.verify_mac) -> CoreResult<VaultKey>
// 状态（设置页/解锁页渲染用）
store.inspect(&vault_id, &verify_mac) -> CoreResult<SealState>   // Absent / Ready / Stale(reason)
store.remove()                                                  // 关闭开关（幂等）
sealer.availability()                                           // 每次现查，别缓存
```

平台适配器只需实现 `VaultSealer` 的 4 个方法（`availability` / `subtype` / `seal` / `unseal`），
把平台异常映射到 `SealError` 即可——**core 不感知任何平台 API**。

### 17.3 新增错误码

| code | 触发 | 前端文案 key |
|---|---|---|
| `ERR_BIOMETRIC_UNAVAILABLE` | 平台不可用 / 未开启 | `err.biometric_unavailable` |
| `ERR_BIOMETRIC_CANCELED` | 用户取消 | `err.biometric_canceled` |
| `ERR_BIOMETRIC_INVALIDATED` | 改主密码 / 注册集变更 / 篡改 | `err.bio_invalidated_password` / `_enrollment` / `_tampered` |
| `ERR_NEED_MASTER_PASSWORD` | 生物会话缺 KEK | `err.need_master_password` |

### 17.4 验证数据（实测）

- `cargo test -p passwordms-core`：**35 组 / 251 例全绿，0 失败**（新增 13 集成 + 2 单测）
- `cargo clippy --all --all-targets`：0 告警（仅 D-33 windres 兜底提示）
- `cargo fmt --all`：已跑
- 本机偶发 `rustc` 崩 `0xc0000005`，重跑即过——与代码无关，遇到先重跑再排查

### 17.5 留给 P2/P3 的接线点

1. 命令层（`src-tauri/src/commands/biometric_cmd.rs`）：4 个命令 + `copy_suffix` 式的参数形状；
2. Windows adapter：`CryptProtectData` / `CryptUnprotectData` + `UserConsentVerifier`，`windows` crate 三个 feature；
3. Android 插件：Keystore 生成 + `BiometricPrompt` 绑 `CryptoObject`，异常映射 `SealError::Invalidated`；
4. 前端：解锁页按钮、设置页开关、i18n 四语言（含本次新增 4 个 `err.*` 与 §10 的 8 个 UI 键）；
5. `change_master_password` 成功后**主动** `store.remove()`（双保险之一，另一重是绑定熵被动失效）。
