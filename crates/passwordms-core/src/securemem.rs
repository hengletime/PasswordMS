//! 敏感内存类型别名与清零辅助。
//!
//! 核心原则（架构文档 §2.7、安全需求 §3.2）：不得依赖「作用域结束即回收」，
//! 敏感缓冲区离开作用域或显式清零时须被覆写为 0。

pub use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::constants::KEY_LEN;

/// 32 字节密钥（MK/KEK/VK/IK/verify）的统一载体，Drop 时自动清零。
pub type Secret32 = Zeroizing<[u8; KEY_LEN]>;

/// 敏感字符串（解密后的密码 / 正文 / TOTP 密钥）载体，Drop 时自动清零。
pub type SecretString = Zeroizing<String>;

/// 敏感字节缓冲（解密后的明文）载体，Drop 时自动清零。
pub type SecretBytes = Zeroizing<Vec<u8>>;

/// 显式擦除一段可变字节。
pub fn wipe(buf: &mut [u8]) {
    buf.zeroize();
}

/// 由原始数组构造 [`Secret32`]。
pub fn secret32(raw: [u8; KEY_LEN]) -> Secret32 {
    Zeroizing::new(raw)
}
