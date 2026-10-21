//! 加密测试向量与统一报错（架构文档 §8.4，验收点 A-01 / A-05）。
//!
//! 状态：已在本机实跑通过（`cargo test -p passwordms-core`，见 docs/03b-verification.md）。

use passwordms_core::constants::{
    AAD_VAULT_KEY, AEAD_NONCE_LEN, ENCRYPTION_SCHEME, KDF_ITERATIONS, KDF_MEMORY_KIB,
    KDF_PARALLELISM, KDF_SALT_LEN,
};
use passwordms_core::crypto::{aead, kdf, keys, rng, verify};
use passwordms_core::error::CoreError;
use passwordms_core::model::vault::{KdfParams, WrappedKey};

#[test]
fn kdf_params_match_spec() {
    let p = KdfParams::default();
    assert_eq!(p.algorithm, "argon2id");
    assert_eq!(p.memory_kib, KDF_MEMORY_KIB);
    assert_eq!(p.iterations, KDF_ITERATIONS);
    assert_eq!(p.parallelism, KDF_PARALLELISM);
    assert_eq!(p.salt_len as usize, KDF_SALT_LEN);
    assert_eq!(ENCRYPTION_SCHEME, "xchacha20poly1305");
}

#[test]
fn aead_roundtrip_and_tamper_detection() {
    let key = rng::random_key().expect("rng");
    let nonce = rng::random_nonce().expect("nonce");
    let aad = b"record-aad";
    let pt = b"secret payload";

    let ct = aead::seal(&key, &nonce, aad, pt).expect("seal");
    assert_ne!(&ct[..pt.len()], &pt[..], "密文不得等于明文");
    let opened = aead::open(&key, &nonce, aad, &ct).expect("open");
    assert_eq!(&opened[..], pt);

    // 篡改密文 → tag 校验失败。
    let mut bad = ct.clone();
    bad[0] ^= 0xff;
    assert!(aead::open(&key, &nonce, aad, &bad).is_err());
}

#[test]
fn vault_key_wrap_roundtrip() {
    let kek = kdf::derive_kek(&rng::random_key().expect("mk")).expect("kek");
    let vk = rng::random_key().expect("vk");
    let wrapped = keys::wrap_vault_key(&kek, &vk).expect("wrap");
    let unwrapped = keys::unwrap_vault_key(&kek, &wrapped).expect("unwrap");
    assert_eq!(&unwrapped[..], &vk[..]);
}

#[test]
fn unlock_failure_is_unified_error() {
    let password = "correct horse battery staple";
    let salt = rng::random_salt16().expect("salt");
    let params = KdfParams::default();
    let mk = kdf::derive_master_key(password, &salt, &params).expect("mk");
    let kek = kdf::derive_kek(&mk[..]).expect("kek");
    let verify_key = kdf::derive_verify_key(&mk[..]).expect("verify");
    let vk = rng::random_key().expect("vk");
    let wrapped = keys::wrap_vault_key(&kek, &vk).expect("wrap");
    let mac = verify::compute_verify_mac(&verify_key).expect("mac");

    // 正确密码 → 成功。
    assert!(verify::unlock_check(&kek, &verify_key, &mac, &wrapped).is_ok());

    // 错误密码 → 统一错误码 ERR_UNLOCK_FAILED。
    let wrong_mk = kdf::derive_master_key("wrong password", &salt, &params).expect("mk");
    let wrong_kek = kdf::derive_kek(&wrong_mk[..]).expect("kek");
    let wrong_verify = kdf::derive_verify_key(&wrong_mk[..]).expect("verify");
    let err = verify::unlock_check(&wrong_kek, &wrong_verify, &mac, &wrapped).unwrap_err();
    assert_eq!(err.code(), "ERR_UNLOCK_FAILED");

    // 数据损坏（verify_mac 不匹配）→ 同一错误码。
    let mut corrupted = mac.clone();
    corrupted[0] ^= 0xff;
    let err2 = verify::unlock_check(&kek, &verify_key, &corrupted, &wrapped).unwrap_err();
    assert_eq!(err2.code(), "ERR_UNLOCK_FAILED");

    // 密文被替换 → 同一错误码。
    let bad_wrapped = WrappedKey {
        nonce: wrapped.nonce.clone(),
        ct: vec![0u8; wrapped.ct.len()],
    };
    let err3 = verify::unlock_check(&kek, &verify_key, &mac, &bad_wrapped).unwrap_err();
    assert_eq!(err3.code(), "ERR_UNLOCK_FAILED");

    // A-05 原文（01-security-req.md:155）要求的是「**文案一致**」——因此不仅错误码要相同，
    // 面向用户的 i18n key 也必须完全相同。Phase 4c 补强：逐条断言 message_key 一致。
    assert_eq!(
        err.message_key(),
        "err.unlock_failed",
        "错误密码的文案 key 应统一"
    );
    assert_eq!(
        err2.message_key(),
        "err.unlock_failed",
        "数据损坏的文案 key 应统一"
    );
    assert_eq!(
        err3.message_key(),
        "err.unlock_failed",
        "密文被替换的文案 key 应统一"
    );
    assert_eq!(
        CoreError::UnlockFailed.message_key(),
        "err.unlock_failed",
        "统一错误变体的文案 key 应固定"
    );
    // 手写 Debug 只输出错误码（不得泄露内部上下文）。
    assert_eq!(format!("{err:?}"), "CoreError(\"ERR_UNLOCK_FAILED\")");

    // AAD 常量存在且固定。
    assert_eq!(AAD_VAULT_KEY, b"passwordms/v1/vault-key");
    assert_eq!(AEAD_NONCE_LEN, 24);
    assert!(matches!(CoreError::UnlockFailed, CoreError::UnlockFailed));
}
