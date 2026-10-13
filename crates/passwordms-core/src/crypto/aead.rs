//! AEAD：XChaCha20-Poly1305 加解密与密钥包裹（架构文档 §2.3）。
//!
//! 密钥参数统一接受 `&[u8]`（长度必须为 32），以便直接接收 `Zeroizing` 载体而无需拷贝。

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::constants::{AEAD_NONCE_LEN, KEY_LEN, WRAPPED_KEY_CT_LEN};
use crate::crypto::rng;
use crate::error::{CoreError, CoreResult};
use crate::model::vault::WrappedKey;

/// 从 32 字节密钥构造 AEAD 实例。
fn cipher_for(key: &[u8]) -> CoreResult<XChaCha20Poly1305> {
    if key.len() != KEY_LEN {
        return Err(CoreError::Crypto("aead key len".to_string()));
    }
    XChaCha20Poly1305::new_from_slice(key).map_err(|_| CoreError::Crypto("aead key".to_string()))
}

/// 校验 nonce 长度并转成定长数组。
fn nonce24(nonce: &[u8]) -> CoreResult<[u8; AEAD_NONCE_LEN]> {
    if nonce.len() != AEAD_NONCE_LEN {
        return Err(CoreError::Crypto("aead nonce len".to_string()));
    }
    let mut out = [0u8; AEAD_NONCE_LEN];
    out.copy_from_slice(nonce);
    Ok(out)
}

/// 认证加密：返回 `ct(明文 + 16B tag)`。
pub fn seal(key: &[u8], nonce: &[u8], aad: &[u8], pt: &[u8]) -> CoreResult<Vec<u8>> {
    let cipher = cipher_for(key)?;
    let nonce = nonce24(nonce)?;
    cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: pt, aad })
        .map_err(|_| CoreError::Crypto("aead seal".to_string()))
}

/// 认证解密：tag 校验失败返回 [`CoreError::Crypto`]（调用方决定是否统一映射）。
pub fn open(key: &[u8], nonce: &[u8], aad: &[u8], ct: &[u8]) -> CoreResult<Zeroizing<Vec<u8>>> {
    let cipher = cipher_for(key)?;
    let nonce = nonce24(nonce)?;
    let pt = cipher
        .decrypt(XNonce::from_slice(&nonce), Payload { msg: ct, aad })
        .map_err(|_| CoreError::Crypto("aead open".to_string()))?;
    Ok(Zeroizing::new(pt))
}

/// 用包裹密钥（KEK 或 VK）包裹一段 32B 原始密钥。
///
/// `aad` 绑定记录身份，防密文移植。随机 24B nonce 与密文一并存储。
pub fn wrap_key(key: &[u8], raw: &[u8], aad: &[u8]) -> CoreResult<WrappedKey> {
    let nonce = rng::random_nonce()?;
    let ct = seal(key, &nonce, aad, raw)?;
    Ok(WrappedKey {
        nonce: nonce.to_vec(),
        ct,
    })
}

/// 解包被包裹的 32B 密钥。
///
/// 失败一律返回 [`CoreError::Crypto`]；解锁路径由 [`crate::crypto::verify`] 统一映射为
/// `ERR_UNLOCK_FAILED`。
pub fn unwrap_key(
    key: &[u8],
    wrapped: &WrappedKey,
    aad: &[u8],
) -> CoreResult<Zeroizing<[u8; KEY_LEN]>> {
    if wrapped.nonce.len() != AEAD_NONCE_LEN || wrapped.ct.len() != WRAPPED_KEY_CT_LEN {
        return Err(CoreError::Crypto("wrapped key shape".to_string()));
    }
    let pt = open(key, &wrapped.nonce, aad, &wrapped.ct)?;
    if pt.len() != KEY_LEN {
        return Err(CoreError::Crypto("wrapped key len".to_string()));
    }
    let mut raw = [0u8; KEY_LEN];
    raw.copy_from_slice(&pt[..]);
    Ok(Zeroizing::new(raw))
}
