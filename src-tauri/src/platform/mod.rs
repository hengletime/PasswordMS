//! 平台适配层：把 core 的 [`passwordms_core::biometric::VaultSealer`] 接到真实平台密钥设施
//! （`docs/08-biometric-unlock.md` §5 / §6）。
//!
//! 本层**只做平台调用**——封印结构、绑定熵、状态分类全部在
//! `passwordms_core::biometric`（平台无关，任何机器可跑、可测）。这里的代码只回答两件事：
//!
//! 1. 「现在能不能做生物识别」——[`PresenceGate::availability`]；
//! 2. 「用 `bind` 当附加认证数据，把 VK 加密 / 解密」——[`VaultSealer::seal`] / [`unseal`]。
//!
//! ## 为什么把「在场校验」抽成 trait
//!
//! 真机的 [`PresenceGate`] 会弹系统对话框，**无法在自动化测试里跑**。而「`bind` 必须真的
//! 参与加密」这件事只有适配器自己知道——core 侧用的是 Mock，适配器忽略 `bind` 时 core
//! 测试依然全绿（§17.5 约束 2）。抽出 trait 后，测试可以注入「永远通过」的闸门，直接对
//! **真实的 DPAPI 调用**断言「换 `bind` 必须解不开」（§18.5 约束 1）。

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

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod windows_dpapi;

#[cfg(not(windows))]
pub use unsupported::UnsupportedSealer;
#[cfg(windows)]
pub use windows_dpapi::{DpapiSealer, HelloGate};

/// 本机构建对应的适配器类型。
///
/// - Windows 桌面 → `DpapiSealer<HelloGate>`（DPAPI 用户域 + Windows Hello 在场闸门）；
/// - Android → P3 接 `Keystore`（当前恒 [`Availability::Unsupported`]，不参与任何封印）。
#[cfg(windows)]
pub type PlatformSealer = DpapiSealer<HelloGate>;
#[cfg(not(windows))]
pub type PlatformSealer = UnsupportedSealer;

/// 构造当前平台的适配器（每次现造：封印操作本身是低频的，省掉全局可变状态）。
#[cfg(windows)]
pub fn platform_sealer() -> PlatformSealer {
    DpapiSealer::new()
}

/// 构造当前平台的适配器。
#[cfg(not(windows))]
pub fn platform_sealer() -> PlatformSealer {
    UnsupportedSealer
}
