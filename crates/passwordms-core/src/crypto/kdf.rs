//! KDF：Argon2id 派生主密钥，HKDF-SHA256 派生 KEK / verify 键（架构文档 §2.2）。

use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::constants::{
    HKDF_INFO_KEK, HKDF_INFO_VERIFY, KDF_MAX_ITERATIONS, KDF_MAX_MEMORY_KIB, KDF_MAX_PARALLELISM,
    KDF_OUTPUT_LEN,
};
use crate::crypto::keys::{Kek, MasterKey, VerifyKey};
use crate::error::{CoreError, CoreResult};
use crate::model::vault::KdfParams;

/// 【D-96】KDF 参数夹取：头部 KDF 参数是**不可信输入**（库文件可被整体替换），
/// 恶意头部声明超大 memory / iterations / parallelism 可让解锁路径分配巨量内存或
/// 永久挂起（DoS）。入口处统一校验，超限返回 [`CoreError::Validation`]（`ERR_VALIDATION`）。
///
/// 默认参数（64 MiB / 3 / 1）远低于上限，**存量库零影响**；参数只可能由创建库时写入，
/// 正常应用无法产生超限参数——超限只可能来自被篡改的库文件。
fn validate_kdf_params(params: &KdfParams) -> CoreResult<()> {
    if params.memory_kib == 0
        || params.memory_kib > KDF_MAX_MEMORY_KIB
        || params.iterations == 0
        || params.iterations > KDF_MAX_ITERATIONS
        || params.parallelism == 0
        || params.parallelism > KDF_MAX_PARALLELISM
    {
        return Err(CoreError::Validation("kdf_params_out_of_range".to_string()));
    }
    Ok(())
}

/// 由主密码 + 盐 + 参数派生主密钥 MK（32B）。
///
/// 使用文件内存储的参数，保证跨端可复现。
///
/// 【D-96】入口先做参数夹取（见 [`validate_kdf_params`]）：超限直接拒绝，
/// 绝不以超限参数执行 Argon2id。
pub fn derive_master_key(password: &str, salt: &[u8], params: &KdfParams) -> CoreResult<MasterKey> {
    validate_kdf_params(params)?;
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
    let hk =
        Hkdf::<Sha256>::from_prk(prk).map_err(|_| CoreError::Crypto("hkdf prk".to_string()))?;
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

/// 【D-96】KDF 参数夹取单测：默认参数放行、边界值放行、越界与零值拒绝。
#[cfg(test)]
mod d96_param_clamp_tests {
    use super::*;

    fn params(m: u32, t: u32, p: u32) -> KdfParams {
        KdfParams {
            algorithm: "argon2id".to_string(),
            memory_kib: m,
            iterations: t,
            parallelism: p,
            salt_len: 16,
        }
    }

    #[test]
    fn default_params_pass_clamp() {
        // 默认 64 MiB / 3 / 1：存量库零影响的回归断言。
        assert!(validate_kdf_params(&KdfParams::default()).is_ok());
    }

    #[test]
    fn boundary_values_pass_clamp() {
        // 边界值（1 GiB / 16 / 8）恰好放行（用极小参数真实执行，避免大内存分配）。
        assert!(validate_kdf_params(&params(1_048_576, 16, 8)).is_ok());
        // 下界（最小合法值）放行。
        assert!(validate_kdf_params(&params(8, 1, 1)).is_ok());
    }

    #[test]
    fn out_of_range_params_rejected() {
        for bad in [
            params(1_048_577, 3, 1), // memory 越界（+1 KiB）
            params(65_536, 17, 1),   // iterations 越界
            params(65_536, 3, 9),    // parallelism 越界
            params(0, 3, 1),         // memory = 0（Argon2 必炸的非法值）
            params(65_536, 0, 1),    // iterations = 0
            params(65_536, 3, 0),    // parallelism = 0
        ] {
            let err = derive_master_key("pw", &[0u8; 16], &bad).unwrap_err();
            assert!(
                matches!(err, CoreError::Validation(ref d) if d == "kdf_params_out_of_range"),
                "参数 {:?} 应被拒绝，实得 {err:?}",
                bad
            );
        }
    }
}
