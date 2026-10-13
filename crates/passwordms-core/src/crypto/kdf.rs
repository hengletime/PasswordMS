//! KDF：Argon2id 派生主密钥，HKDF-SHA256 派生 KEK / verify 键（架构文档 §2.2）。

use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::constants::{HKDF_INFO_KEK, HKDF_INFO_VERIFY, KDF_OUTPUT_LEN};
use crate::crypto::keys::{Kek, MasterKey, VerifyKey};
use crate::error::{CoreError, CoreResult};
use crate::model::vault::KdfParams;

/// 由主密码 + 盐 + 参数派生主密钥 MK（32B）。
///
/// 使用文件内存储的参数，保证跨端可复现。
pub fn derive_master_key(
    password: &str,
    salt: &[u8],
    params: &KdfParams,
) -> CoreResult<MasterKey> {
    let argon_params = Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KDF_OUTPUT_LEN),
    )
    .map_err(|_| CoreError::Crypto("argon2 params".to_string()))?;

    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);

    let mut mk = [0u8; KDF_OUTPUT_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut mk)
        .map_err(|_| CoreError::Crypto("argon2 hash".to_string()))?;

    Ok(zeroize::Zeroizing::new(mk))
}

/// 由 MK 经 HKDF-SHA256 派生 KEK（32B）。
pub fn derive_kek(mk: &[u8]) -> CoreResult<Kek> {
    let mut out = [0u8; KDF_OUTPUT_LEN];
    let hk = Hkdf::<Sha256>::new(None, mk);
    hk.expand(HKDF_INFO_KEK.as_bytes(), &mut out)
        .map_err(|_| CoreError::Crypto("hkdf kek".to_string()))?;
    Ok(zeroize::Zeroizing::new(out))
}

/// 由 MK 经 HKDF-SHA256 派生 verify 键（32B），仅用于快速负向判定。
pub fn derive_verify_key(mk: &[u8]) -> CoreResult<VerifyKey> {
    let mut out = [0u8; KDF_OUTPUT_LEN];
    let hk = Hkdf::<Sha256>::new(None, mk);
    hk.expand(HKDF_INFO_VERIFY.as_bytes(), &mut out)
        .map_err(|_| CoreError::Crypto("hkdf verify".to_string()))?;
    Ok(zeroize::Zeroizing::new(out))
}

/// 供同步握手使用的 HKDF-Expand。
pub fn hkdf_expand(prk: &[u8], info: &[u8], out_len: usize) -> CoreResult<Vec<u8>> {
    let hk = Hkdf::<Sha256>::from_prk(prk)
        .map_err(|_| CoreError::Crypto("hkdf prk".to_string()))?;
    let mut out = vec![0u8; out_len];
    hk.expand(info, &mut out)
        .map_err(|_| CoreError::Crypto("hkdf expand".to_string()))?;
    Ok(out)
}

/// 供同步握手使用的 HKDF-Extract。
pub fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> Vec<u8> {
    let (prk, _) = Hkdf::<Sha256>::extract(Some(salt), ikm);
    prk.to_vec()
}
