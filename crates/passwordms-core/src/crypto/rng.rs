//! CSPRNG 封装（架构文档 §2.4）。
//!
//! 统一使用 `getrandom`（Windows `BCryptGenRandom` / Android `getrandom(2)`）。
//! **禁止**在密码学用途使用任何用户态可预测 PRNG。

use crate::constants::{AEAD_NONCE_LEN, KEY_LEN};
use crate::error::{CoreError, CoreResult};

/// 填充给定缓冲区为随机字节。
pub fn fill(dest: &mut [u8]) -> CoreResult<()> {
    getrandom::getrandom(dest).map_err(|_| CoreError::Crypto("rng".to_string()))
}

/// 生成 `n` 字节随机数据。
pub fn random_bytes(n: usize) -> CoreResult<Vec<u8>> {
    let mut buf = vec![0u8; n];
    fill(&mut buf)?;
    Ok(buf)
}

/// 生成 32 字节随机密钥。
pub fn random_key() -> CoreResult<[u8; KEY_LEN]> {
    let mut out = [0u8; KEY_LEN];
    fill(&mut out)?;
    Ok(out)
}

/// 生成 24 字节随机 nonce。
pub fn random_nonce() -> CoreResult<[u8; AEAD_NONCE_LEN]> {
    let mut out = [0u8; AEAD_NONCE_LEN];
    fill(&mut out)?;
    Ok(out)
}

/// 生成 32 字节一次性配对令牌。
pub fn random_token() -> CoreResult<[u8; 32]> {
    let mut out = [0u8; 32];
    fill(&mut out)?;
    Ok(out)
}

/// 生成 16 字节随机盐。
pub fn random_salt16() -> CoreResult<[u8; 16]> {
    let mut out = [0u8; 16];
    fill(&mut out)?;
    Ok(out)
}

/// 无模偏置地生成 `[0, n)` 区间内的随机数。
///
/// 拒绝采样：丢弃落在 `[limit, 2^32)` 的值以消除 `% n` 的模偏置。
/// **v1.5.29 起从 `generator.rs` 提到这里**：口令短语（`passphrase.rs`）要用同一套
/// 抽样逻辑。两份实现各写一遍迟早会漂移——一份修了偏置、另一份没修，就是
/// 「密码生成器安全、短语生成器不安全」这种最难发现的问题。
///
/// # Panics
/// `n == 0` 返回 `Internal` 错误而非 panic：调用方都是「池子非空」的前置校验之后的
/// 路径，真走到这里说明上游逻辑错了，报错比崩掉更容易定位。
pub fn below(n: u32) -> CoreResult<u32> {
    if n == 0 {
        return Err(CoreError::Internal("rng::below(0)".to_string()));
    }
    let limit = u32::MAX - (u32::MAX % n);
    loop {
        let bytes = random_bytes(4)?;
        let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if v < limit {
            return Ok(v % n);
        }
    }
}
