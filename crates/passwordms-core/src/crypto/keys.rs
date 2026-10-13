//! 密钥层级类型与包裹逻辑（架构文档 §2.1）。

use zeroize::Zeroizing;

use crate::constants::{AAD_VAULT_KEY, KEY_LEN};
use crate::crypto::aead;
use crate::error::CoreResult;
use crate::model::vault::WrappedKey;

/// 主密钥 MK（KDF 输出，仅内存）。
pub type MasterKey = Zeroizing<[u8; KEY_LEN]>;
/// 密钥加密密钥 KEK（包裹 VK）。
pub type Kek = Zeroizing<[u8; KEY_LEN]>;
/// 保险库密钥 VK（数据加密之根）。
pub type VaultKey = Zeroizing<[u8; KEY_LEN]>;
/// 条目密钥 IK（每条独立）。
pub type ItemKey = Zeroizing<[u8; KEY_LEN]>;
/// 密码校验键（HMAC 快速负向判定）。
pub type VerifyKey = Zeroizing<[u8; KEY_LEN]>;

/// 记录 AAD = `record_type(1B) ‖ record_id(16B)`（架构文档 §2.3）。
pub fn record_aad(record_type: u8, record_id: &[u8; 16]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(17);
    aad.push(record_type);
    aad.extend_from_slice(record_id);
    aad
}

/// 用 KEK 包裹 VK（AAD = 常量 `"passwordms/v1/vault-key"`）。
pub fn wrap_vault_key(kek: &Kek, vk: &[u8]) -> CoreResult<WrappedKey> {
    aead::wrap_key(&kek[..], vk, AAD_VAULT_KEY)
}

/// 用 KEK 解包 VK。
pub fn unwrap_vault_key(kek: &Kek, wrapped: &WrappedKey) -> CoreResult<VaultKey> {
    aead::unwrap_key(&kek[..], wrapped, AAD_VAULT_KEY)
}

/// 用 VK 包裹 IK（AAD 绑定记录身份）。
pub fn wrap_item_key(vk: &VaultKey, ik: &[u8], aad: &[u8]) -> CoreResult<WrappedKey> {
    aead::wrap_key(&vk[..], ik, aad)
}

/// 用 VK 解包 IK。
pub fn unwrap_item_key(vk: &VaultKey, wrapped: &WrappedKey, aad: &[u8]) -> CoreResult<ItemKey> {
    aead::unwrap_key(&vk[..], wrapped, aad)
}
