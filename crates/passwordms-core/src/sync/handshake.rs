//! 配对握手：X25519 协商 + HKDF 派生 + 双向密钥确认（架构文档 §3.4、§3.8）。
//!
//! 三件套（ECDH + 指纹 + 密钥确认）阻断任何未持有 `shared` 的中间人。

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::constants::{HKDF_INFO_SYNC_CONFIRM, HKDF_INFO_SYNC_KEY};
use crate::crypto::kdf;
use crate::crypto::{aead, rng};
use crate::error::{CoreError, CoreResult};

type HmacSha256 = Hmac<Sha256>;

/// 密钥确认角色标签（Host）。
pub const ROLE_SERVER: &[u8] = b"server";
/// 密钥确认角色标签（Joiner）。
pub const ROLE_CLIENT: &[u8] = b"client";

/// Host 长期身份密钥对（持久化于应用数据目录，用于指纹校验）。
pub struct HostIdentity {
    secret: StaticSecret,
    public: PublicKey,
}

impl HostIdentity {
    /// 生成新的身份密钥对。
    pub fn generate() -> CoreResult<Self> {
        let bytes = rng::random_key()?;
        Ok(Self::from_secret_bytes(&bytes))
    }

    /// 由 32B 私钥恢复。
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        let secret = StaticSecret::from(*bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// 私钥原始字节（用于持久化）。
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// 公钥原始字节。
    pub fn public_bytes(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }

    /// 身份指纹（`sha256(pubkey)` 前 8 字节 hex，共 16 字符）。
    pub fn fingerprint(&self) -> String {
        fingerprint_of(self.public.as_bytes())
    }
}

/// 临时 X25519 密钥对（每次配对/会话独立）。
pub struct EphemeralKeypair {
    secret: StaticSecret,
    public: PublicKey,
}

impl EphemeralKeypair {
    /// 生成临时密钥对。
    pub fn generate() -> CoreResult<Self> {
        let bytes = rng::random_key()?;
        let secret = StaticSecret::from(bytes);
        let public = PublicKey::from(&secret);
        Ok(Self { secret, public })
    }

    /// 公钥原始字节。
    pub fn public_bytes(&self) -> [u8; 32] {
        *self.public.as_bytes()
    }

    /// 计算共享密钥 `X25519(priv_eph, peer_pub)`。
    pub fn shared_secret(&self, peer_public: &[u8; 32]) -> CoreResult<Zeroizing<[u8; 32]>> {
        let peer = PublicKey::from(*peer_public);
        let shared = self.secret.diffie_hellman(&peer);
        if !shared.was_contributory() {
            // 低阶点攻击防护：贡献值异常直接判失败。
            return Err(CoreError::FingerprintMismatch);
        }
        Ok(Zeroizing::new(*shared.as_bytes()))
    }
}

/// 由共享密钥派生会话密钥。
pub struct SessionKeys {
    pub sync_key: Zeroizing<[u8; 32]>,
    pub confirm_key: Zeroizing<[u8; 32]>,
}

/// `prk = HKDF-Extract(salt, shared)`；展开 `sync_key` / `confirm_key`。
pub fn derive_session_keys(shared: &[u8; 32], salt: &[u8]) -> CoreResult<SessionKeys> {
    let prk = kdf::hkdf_extract(salt, shared);
    let sk = kdf::hkdf_expand(&prk, HKDF_INFO_SYNC_KEY.as_bytes(), 32)?;
    let ck = kdf::hkdf_expand(&prk, HKDF_INFO_SYNC_CONFIRM.as_bytes(), 32)?;
    Ok(SessionKeys {
        sync_key: to_arr32(&sk)?,
        confirm_key: to_arr32(&ck)?,
    })
}

/// 会话（已配对设备复连）密钥派生：`sk = HKDF(sync_key, salt=nonce_a‖nonce_b, info=session)`。
pub fn derive_session_sk(
    sync_key: &[u8; 32],
    nonce_a: &[u8],
    nonce_b: &[u8],
) -> CoreResult<Zeroizing<[u8; 32]>> {
    let mut salt = Vec::with_capacity(nonce_a.len() + nonce_b.len());
    salt.extend_from_slice(nonce_a);
    salt.extend_from_slice(nonce_b);
    let prk = kdf::hkdf_extract(&salt, sync_key);
    let sk = kdf::hkdf_expand(
        &prk,
        crate::constants::HKDF_INFO_SYNC_SESSION.as_bytes(),
        32,
    )?;
    to_arr32(&sk)
}

/// `transcript = sha256(HELLO ‖ ACCEPT)`。
pub fn transcript_hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// `HMAC(confirm_key, role ‖ transcript)`。
pub fn confirm_mac(
    confirm_key: &[u8; 32],
    role: &[u8],
    transcript: &[u8; 32],
) -> CoreResult<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(confirm_key)
        .map_err(|_| CoreError::Crypto("hmac key".to_string()))?;
    mac.update(role);
    mac.update(transcript);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// 常量时间比对密钥确认 MAC。
pub fn verify_mac(expected: &[u8], provided: &[u8]) -> bool {
    if expected.len() != provided.len() {
        return false;
    }
    expected.ct_eq(provided).into()
}

/// 常量时间比对配对令牌。
pub fn token_matches(expected: &[u8; 32], provided: &[u8]) -> bool {
    if provided.len() != 32 {
        return false;
    }
    expected.ct_eq(provided).into()
}

/// 用 `sync_key` 加密 VK（`AEAD(sync_key, nonce, VK, aad=session_id)`）。
pub fn seal_vault_key(
    sync_key: &[u8],
    vault_key: &[u8],
    session_id: &[u8],
) -> CoreResult<(Vec<u8>, Vec<u8>)> {
    let nonce = rng::random_nonce()?;
    let ct = aead::seal(sync_key, &nonce, session_id, vault_key)?;
    Ok((nonce.to_vec(), ct))
}

/// 解出 VK 信封。
pub fn open_vault_key(
    sync_key: &[u8],
    vk_nonce: &[u8],
    vk_ct: &[u8],
    session_id: &[u8],
) -> CoreResult<Zeroizing<[u8; 32]>> {
    if vk_nonce.len() != crate::constants::AEAD_NONCE_LEN {
        return Err(CoreError::FingerprintMismatch);
    }
    let pt = aead::open(sync_key, vk_nonce, session_id, vk_ct)?;
    if pt.len() != 32 {
        return Err(CoreError::FingerprintMismatch);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&pt[..]);
    Ok(Zeroizing::new(out))
}

/// 计算公钥指纹：`sha256(pubkey)` 前 8 字节 hex。
pub fn fingerprint_of(public: &[u8; 32]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public);
    let digest = hasher.finalize();
    to_hex(&digest[..8])
}

/// 字节数组 → 小写 hex。
fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// 切片 → 定长 32 字节数组。
pub fn to_arr32(bytes: &[u8]) -> CoreResult<Zeroizing<[u8; 32]>> {
    if bytes.len() != 32 {
        return Err(CoreError::Crypto("expected 32 bytes".to_string()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(Zeroizing::new(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D-68：指纹必须是 `sha256(本机公钥)` 前 8 字节的小写 hex（16 字符），
    /// 且**来自本机公钥本身**——不做大小写/分组美化，否则两屏「逐字比对」失效。
    #[test]
    fn fingerprint_is_16_lowercase_hex_of_own_public_key() {
        let identity = HostIdentity::generate().unwrap();
        let fp = identity.fingerprint();
        assert_eq!(fp.len(), 16, "指纹必须恰为 16 个字符（8 字节 hex）");
        assert!(
            fp.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "指纹必须是小写 hex，实际: {fp}"
        );
        // 方向校验：等价于对本机公钥字节求指纹（不是对端指纹、也不是 device_id 派生）。
        assert_eq!(fp, fingerprint_of(&identity.public_bytes()));
    }

    /// D-68：同一把私钥恒得同一指纹（两屏可比对的前提），不同身份必须不同。
    #[test]
    fn fingerprint_is_deterministic_per_secret() {
        let secret = [0x42u8; 32];
        let a = HostIdentity::from_secret_bytes(&secret);
        let b = HostIdentity::from_secret_bytes(&secret);
        assert_eq!(
            a.fingerprint(),
            b.fingerprint(),
            "同一私钥两次构造指纹必须相同"
        );
        assert_eq!(a.public_bytes(), b.public_bytes());

        let other = HostIdentity::generate().unwrap();
        assert_ne!(
            a.fingerprint(),
            other.fingerprint(),
            "不同身份的指纹必须不同（否则中间人不可辨）"
        );
    }
}
