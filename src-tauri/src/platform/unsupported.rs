//! 无生物识别适配器的平台占位（macOS / Linux 等）。
//!
//! Windows 走 `windows_dpapi`、Android 走 `android_keystore`；本模块是**剩下的平台**的兜底，
//! 存在的意义是让它们编译通过，同时把「本机做不了生物识别」表达成
//! [`Availability::NotImplemented`] 而不是编译错误或 panic：
//!
//! - `enable` 会在第一步就因不可用而拒绝，**不会**写出任何封印文件；
//! - `inspect` 仍能正常读 `biometric.json`（例如两端共用同一数据目录的罕见场景），
//!   只是 `unlock` 会返回不可用——上层据此显示「本机不支持」，而不是删掉封印。
//!
//! **为什么是 `NotImplemented` 而不是 `Unsupported`**（D-116）：前端据此**整块不渲染**
//! 快速解锁卡片。安卓 v1.3.1 曾显示「Windows Hello 快速解锁 · 未开启 · 当前平台不支持」
//! ——既把 Windows 专属名词搬到安卓，又给一个还没做的功能挂了块「不支持」的牌子
//! （主人真机实测）。`Unsupported` 保留给「有适配器但平台此刻做不了」的真实故障
//! （Windows 的 WinRT 激活失败、Android 的安全更新缺失等）。

use passwordms_core::biometric::{Availability, SealError, Subtype, VaultSealer};
use zeroize::Zeroizing;

/// 恒不可用的适配器。
#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedSealer;

impl VaultSealer for UnsupportedSealer {
    fn availability(&self) -> Availability {
        Availability::NotImplemented
    }

    fn subtype(&self) -> Subtype {
        // 仅用于填写记录；本实现从不产出记录（`seal` 恒失败），故不会落盘成错误子类型。
        #[cfg(target_os = "android")]
        {
            Subtype::AndroidKeystore
        }
        #[cfg(not(target_os = "android"))]
        {
            Subtype::WindowsDpapiUser
        }
    }

    fn seal(&self, _vk: &[u8], _bind: &[u8]) -> Result<Vec<u8>, SealError> {
        Err(SealError::Unavailable(Availability::NotImplemented))
    }

    fn unseal(&self, _sealed: &[u8], _bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        Err(SealError::Unavailable(Availability::NotImplemented))
    }
}
