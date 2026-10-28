//! 非 Windows 平台占位适配器（Android 在 P3 接 Keystore）。
//!
//! 存在的意义是让 **Android 也能编译通过**，同时把「本机做不了生物识别」表达成
//! [`Availability::Unsupported`] 而不是编译错误或 panic：
//!
//! - `enable` 会在第一步就因不可用而拒绝，**不会**写出任何封印文件；
//! - `inspect` 仍能正常读 `biometric.json`（例如两端共用同一数据目录的罕见场景），
//!   只是 `unlock` 会返回不可用——上层据此显示「本机不支持」，而不是删掉封印。

use passwordms_core::biometric::{Availability, SealError, Subtype, VaultSealer};
use zeroize::Zeroizing;

/// 非 Windows 没有系统「登录选项」可唤（Android 的等价物是系统生物识别设置页，P3 再接）。
///
/// 恒 `false` 而不是 panic：调用方据此只报「打不开」，界面照常可用。
pub fn open_hello_settings() -> bool {
    false
}

/// 恒不可用的适配器。
#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedSealer;

impl VaultSealer for UnsupportedSealer {
    fn availability(&self) -> Availability {
        Availability::Unsupported
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
        Err(SealError::Unavailable(Availability::Unsupported))
    }

    fn unseal(&self, _sealed: &[u8], _bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        Err(SealError::Unavailable(Availability::Unsupported))
    }
}
