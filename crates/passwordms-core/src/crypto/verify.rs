//! 主密码验证（架构文档 §2.5）。
//!
//! - `verify_mac = HMAC-SHA256(verify_key, "passwordms/v1/verify")` 提供**快速负向判定**。
//! - 为避免侧信道，`verify_mac` 校验与 VK 解包**均返回同一错误码** [`CoreError::UnlockFailed`]，
//!   且比较使用 [`subtle::ConstantTimeEq`] 常量时间实现。

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::constants::AAD_VERIFY;
use crate::crypto::keys::{unwrap_vault_key, VaultKey, VerifyKey};
use crate::error::{CoreError, CoreResult};
use crate::model::vault::WrappedKey;

/// verify_mac 覆盖的固定上下文。
const VERIFY_CONTEXT: &[u8] = AAD_VERIFY;

/// 计算 `verify_mac`（32B）。
pub fn compute_verify_mac(verify_key: &VerifyKey) -> CoreResult<Vec<u8>> {
    let mut mac = Hmac::<Sha256>::new_from_slice(&verify_key[..])
        .map_err(|_| CoreError::Crypto("hmac key".to_string()))?;
    mac.update(VERIFY_CONTEXT);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// 常量时间比较两个字节序列。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        // 长度不等直接判否（长度本身非机密）。
        return false;
    }
    a.ct_eq(b).into()
}

/// 校验 verify_mac。
///
/// 返回 `bool` 而非 `Result`：**调用方不得据此区分失败原因**，须与 VK 解包失败合并为统一错误。
pub fn check_verify_mac(expected: &[u8], computed: &[u8]) -> bool {
    constant_time_eq(expected, computed)
}

/// 一次性完成「校验 + 解包」，任何失败都统一映射为 [`CoreError::UnlockFailed`]。
///
/// 该函数是解锁路径的唯一入口，保证 A-05：不区分「主密码错误」与「数据损坏」。
pub fn unlock_check(
    kek: &crate::crypto::keys::Kek,
    verify_key: &VerifyKey,
    expected_verify_mac: &[u8],
    wrapped_vault_key: &WrappedKey,
) -> CoreResult<VaultKey> {
    let computed = compute_verify_mac(verify_key)?;
    let mac_ok = check_verify_mac(expected_verify_mac, &computed);
    let unwrap_res = unwrap_vault_key(kek, wrapped_vault_key);
    match unwrap_res {
        Ok(vk) if mac_ok => Ok(vk),
        // 任一环节失败 → 统一错误，绝不暴露原因分类。
        _ => Err(CoreError::UnlockFailed),
    }
}