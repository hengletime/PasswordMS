//! 平台适配层：把 core 的 [`passwordms_core::biometric::VaultSealer`] 接到真实平台密钥设施
//! （`docs/08-biometric-unlock.md` §5 / §6 / §23）。
//!
//! 本层**只做平台调用**——封印结构、绑定熵、状态分类全部在
//! `passwordms_core::biometric`（平台无关，任何机器可跑、可测）。这里的代码只回答两件事：
//!
//! 1. 「现在能不能做生物识别」——[`PresenceGate::availability`] / `VaultSealer::availability`；
//! 2. 「用 `bind` 当附加认证数据，把 VK 加密 / 解密」——[`VaultSealer::seal`] / `unseal`。
//!
//! ## 为什么把「在场校验」抽成 trait
//!
//! 真机的 [`PresenceGate`] 会弹系统对话框，**无法在自动化测试里跑**。而「`bind` 必须真的
//! 参与加密」这件事只有适配器自己知道——core 侧用的是 Mock，适配器忽略 `bind` 时 core
//! 测试依然全绿（§17.5 约束 2）。抽出 trait 后，测试可以注入「永远通过」的闸门，直接对
//! **真实的 DPAPI 调用**断言「换 `bind` 必须解不开」（§18.5 约束 1）。
//!
//! ## 三个平台分支
//!
//! | 平台 | 适配器 | 形态 |
//! |---|---|---|
//! | Windows | `DpapiSealer<HelloGate>` | 两段式：DPAPI 用户域 + `UserConsentVerifier` 闸门 |
//! | Android | `KeystoreSealer` | **一段式**：Keystore 密钥硬绑生物识别，`CryptoObject` 让闸门与加解密合一（§23） |
//! | 其它 | `UnsupportedSealer` | 恒 `NotImplemented`，前端整卡不渲染（D-116） |

use passwordms_core::biometric::{Availability, SealError};

/// 在场校验的用途（决定系统对话框文案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceReason {
    /// 开启快速解锁（写封印）。
    Enable,
    /// 用快速解锁打开保险库（读封印）。
    Unlock,
}

/// 在场校验闸门：确认「此刻是本人在操作」。
///
/// 实现方**不得**缓存可用性（`VaultSealer::availability` 的约定：每次现查）。
pub trait PresenceGate: Send + Sync {
    /// 当前可用性（映射平台结论，不抛错——错误在 [`Self::request`] 里报）。
    fn availability(&self) -> Availability;

    /// 请求一次在场校验。用户取消 → [`SealError::Canceled`]（**不是**失效）。
    fn request(&self, reason: PresenceReason) -> Result<(), SealError>;
}

// 纯逻辑（别名派生 / 可用性映射）：无平台依赖，桌面可测。
// 非 Android 构建里这些函数没有调用方（`android_keystore` 不编译），但**测试仍要用**，
// 所以按平台放宽 dead_code——不要为了消警告把它们删掉或塞进 cfg(android)。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
mod keystore_rules;

#[cfg(target_os = "android")]
mod android_keystore;
#[cfg(not(any(windows, target_os = "android")))]
mod unsupported;
#[cfg(windows)]
mod windows_dpapi;

#[cfg(target_os = "android")]
pub use android_keystore::KeystoreSealer;
#[cfg(not(any(windows, target_os = "android")))]
pub use unsupported::UnsupportedSealer;
#[cfg(windows)]
pub use windows_dpapi::{DpapiSealer, HelloGate};

/// 本机构建对应的适配器类型。
#[cfg(windows)]
pub type PlatformSealer = DpapiSealer<HelloGate>;
#[cfg(target_os = "android")]
pub type PlatformSealer = KeystoreSealer<tauri::Wry>;
#[cfg(not(any(windows, target_os = "android")))]
pub type PlatformSealer = UnsupportedSealer;

/// 构造当前平台的适配器（每次现造：封印操作本身是低频的，省掉全局可变状态）。
///
/// Windows 用不到 `app`，但签名统一成吃 `AppHandle`——Android 侧要靠它拿到注册时
/// 存进 state 的 [`crate::BiometricHandle`]（Tauri 的 `PluginHandle`）。
#[cfg(windows)]
pub fn platform_sealer(_app: &tauri::AppHandle) -> PlatformSealer {
    DpapiSealer::new()
}

#[cfg(target_os = "android")]
pub fn platform_sealer(app: &tauri::AppHandle) -> PlatformSealer {
    use tauri::Manager;
    KeystoreSealer::new(app.state::<crate::BiometricHandle>().inner().0.clone())
}

#[cfg(not(any(windows, target_os = "android")))]
pub fn platform_sealer(_app: &tauri::AppHandle) -> PlatformSealer {
    UnsupportedSealer
}

/// 删掉平台侧的密钥材料（撤销的实质动作，`SealStore::remove()` 的搭档）。
///
/// Windows 的 DPAPI **没有**可删除的密钥句柄（主密钥由系统账户派生，不属于我们），
/// 所以这里是空操作；Android 必须真删 Keystore 条目，否则「关闭快速解锁」只删了文件、
/// 密钥还留在系统里，撤销就不彻底。
#[cfg(target_os = "android")]
pub fn purge_platform_key(sealer: &PlatformSealer, bind: &[u8]) {
    sealer.purge_key(bind);
}

#[cfg(not(target_os = "android"))]
pub fn purge_platform_key(_sealer: &PlatformSealer, _bind: &[u8]) {}

/// 唤起系统的生物识别 / 登录设置页（D-114 的成果在 Android 上延续）。
///
/// **无参数**：目标页面是编译期常量，前端无法借这条通道打开任意 Intent / 协议
/// （`open_url` 的 http/https 白名单是 PRD F-05 不变量，不能为它放宽）。
/// 失败恒 `false`——调用方只据此报「打不开」，界面照常可用。
///
/// 三种平台的落点：Windows → `ms-settings:signinoptions`；
/// Android → `Settings.ACTION_BIOMETRIC_ENROLL`（API 30+，低版本退到安全设置）；
/// 其它 → 恒 `false`。
#[cfg(windows)]
pub fn open_biometric_settings(_app: &tauri::AppHandle) -> bool {
    windows_dpapi::open_hello_settings()
}

#[cfg(target_os = "android")]
pub fn open_biometric_settings(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    let handle = app.state::<crate::BiometricHandle>().inner().0.clone();
    // 命令名 = Kotlin 侧 `@Command fun` 的**函数名字面**（`PluginHandle` 里就是
    // `commands[method.name] = ...`，不做 snake_case 转换）——所以这里是 `openSettings`
    // 而不是 `open_settings`。写成后者不会编译报错、也不会 panic，只会让按钮点了没反应。
    handle
        .run_mobile_plugin::<serde_json::Value>("openSettings", serde_json::json!({}))
        .is_ok()
}

#[cfg(not(any(windows, target_os = "android")))]
pub fn open_biometric_settings(_app: &tauri::AppHandle) -> bool {
    false
}
