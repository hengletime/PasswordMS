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
