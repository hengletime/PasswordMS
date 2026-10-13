//! TOTP（RFC 6238，架构文档 §3.3 F-11）。
//!
//! 组合使用 `hmac` + `sha1` + `base32`（均为 RustCrypto 审计实现）实现 HMAC-SHA1 动态口令，
//! 不引入独立的 `totp-rs`，以压体积。固定 6 位 / 30 秒周期（不支持 HOTP）。

use base32::Alphabet;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

use crate::constants::{TOTP_DIGITS, TOTP_PERIOD_SECS};
use crate::error::{CoreError, CoreResult};

type HmacSha1 = Hmac<Sha1>;

/// TOTP 当前验证码与剩余时间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TotpCode {
    pub code: String,
    pub period: u32,
    /// 距离下一周期的剩余秒数。
    pub remaining: u32,
}

/// 当前时刻的验证码。
pub fn now(secret_base32: &str) -> CoreResult<TotpCode> {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| CoreError::Internal("clock".to_string()))?;
    now_at(secret_base32, unix)
}

/// 指定 Unix 秒的验证码（便于单测）。
pub fn now_at(secret_base32: &str, unix_secs: u64) -> CoreResult<TotpCode> {
    let code = code_at(secret_base32, unix_secs)?;
    let remaining = (TOTP_PERIOD_SECS - (unix_secs % TOTP_PERIOD_SECS)) as u32;
    Ok(TotpCode {
        code,
        period: TOTP_PERIOD_SECS as u32,
        remaining,
    })
}

/// 计算指定 Unix 秒的 6 位验证码。
pub fn code_at(secret_base32: &str, unix_secs: u64) -> CoreResult<String> {
    let key = decode_secret(secret_base32)?;
    let counter = unix_secs / TOTP_PERIOD_SECS;

    let mut mac = HmacSha1::new_from_slice(&key)
        .map_err(|_| CoreError::Validation("totp_key".to_string()))?;
    mac.update(&counter.to_be_bytes());
    let hash = mac.finalize().into_bytes();

    // 动态截断（RFC 4226 §5.3）。
    let offset = (hash[19] & 0x0f) as usize;
    let binary = ((hash[offset] as u32 & 0x7f) << 24)
        | ((hash[offset + 1] as u32) << 16)
        | ((hash[offset + 2] as u32) << 8)
        | (hash[offset + 3] as u32);
    let code = binary % 10u32.pow(TOTP_DIGITS);
    Ok(format!("{:0width$}", code, width = TOTP_DIGITS as usize))
}

/// 解码 Base32 密钥（容错大小写、空格与填充）。
fn decode_secret(secret: &str) -> CoreResult<Vec<u8>> {
    let normalized: String = secret
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=')
        .collect::<String>()
        .to_ascii_uppercase();
    if normalized.is_empty() {
        return Err(CoreError::Validation("totp_secret_empty".to_string()));
    }
    base32::decode(Alphabet::Rfc4648 { padding: false }, &normalized)
        .ok_or_else(|| CoreError::Validation("totp_secret_invalid".to_string()))
}
