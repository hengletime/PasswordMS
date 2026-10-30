//! Host 长期身份密钥文件的落盘与加载（【D-96】VK 包裹加密）。
//!
//! 背景：身份私钥（32B，长期使用、不可更换）此前以 Base64 **裸明文**落盘——
//! 任何拿到该文件的本机进程或备份都能伪装本机参与同步握手。本模块把私钥用
//! **保险库主密钥（VK）**经 XChaCha20-Poly1305 包裹后落盘：
//! - 锁定的库 ⇒ VK 不在内存 ⇒ 身份文件读不出私钥（与库同生命周期保护）；
//! - 唯一调用点 `sync_start_pairing` 必然先 `session.repo()?` 解锁，VK 必在。
//!
//! 文件格式：
//! - **新格式**：`IDENTITY_MAGIC(10B) ‖ nonce(24B) ‖ ct(48B = 32B 私钥 + 16B tag)`，
//!   AAD 绑 [`crate::constants::AAD_IDENTITY`]；
//! - **旧格式**：Base64(裸 32B 私钥)（无魔数）。读取时识别出旧格式后**立即改写为
//!   新格式**（兼容迁移，老库无感知；迁移在下一次配对操作时自动发生）。

use std::path::Path;

use zeroize::Zeroizing;

use crate::constants::{AAD_IDENTITY, AEAD_NONCE_LEN, IDENTITY_MAGIC, KEY_LEN, WRAPPED_KEY_CT_LEN};
use crate::crypto::{aead, rng};
use crate::error::{CoreError, CoreResult};

/// 新格式文件总长度：magic(10) + nonce(24) + ct(48) = 82 字节。
/// （D-108 豆包审查 3.3-2：原注释误写 81——magic `PMSIDENT1\0` 是 10 字节。）
pub const IDENTITY_FILE_LEN: usize = IDENTITY_MAGIC.len() + AEAD_NONCE_LEN + WRAPPED_KEY_CT_LEN;

/// 封装：私钥用 VK 包裹，产出新格式文件字节（随机 nonce，AAD 绑 `b"identity"`）。
pub fn seal_identity_bytes(secret: &[u8; KEY_LEN], vault_key: &[u8]) -> CoreResult<Vec<u8>> {
    let nonce = rng::random_nonce()?;
    let ct = aead::seal(vault_key, &nonce, AAD_IDENTITY, secret.as_slice())?;
    let mut out = Vec::with_capacity(IDENTITY_FILE_LEN);
    out.extend_from_slice(IDENTITY_MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// 解封新格式文件字节：magic 不符或长度异常返回 [`CoreError::Validation`]，
/// tag 校验失败（VK 不对 / 密文被篡改）返回 [`CoreError::Crypto`]。
pub fn open_identity_bytes(bytes: &[u8], vault_key: &[u8]) -> CoreResult<Zeroizing<[u8; KEY_LEN]>> {
    if !bytes.starts_with(IDENTITY_MAGIC) {
        return Err(CoreError::Validation("identity_format".to_string()));
    }
    let rest = &bytes[IDENTITY_MAGIC.len()..];
    if rest.len() != AEAD_NONCE_LEN + WRAPPED_KEY_CT_LEN {
        return Err(CoreError::Validation("identity_shape".to_string()));
    }
    let (nonce, ct) = rest.split_at(AEAD_NONCE_LEN);
    let pt = aead::open(vault_key, nonce, AAD_IDENTITY, ct)?;
    if pt.len() != KEY_LEN {
        return Err(CoreError::Validation("identity_len".to_string()));
    }
    let mut secret = [0u8; KEY_LEN];
    secret.copy_from_slice(&pt[..]);
    Ok(Zeroizing::new(secret))
}

/// 原子落盘（临时文件 + rename；Unix 侧临时文件与目标文件均 chmod 600）。
///
/// 临时文件与目标同目录（保证同卷 rename 原子性）；Windows 上无 POSIX 权限位，
/// 依赖目录 ACL（与库文件同目录，保护级别一致）。
fn write_identity_file(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let tmp = path.with_extension("identity.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        std::io::Write::write_all(&mut f, bytes)?;
        std::io::Write::flush(&mut f)?;
        // 仅 flush 到 OS；身份文件无「跨掉电一致性」诉求（丢了可重新配对），
        // 不做 sync_all（与库文件的头部写路径不同，见 container.rs 的裁定）。
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 封装并原子落盘（D-139：配对加入采纳对端 VK 后重包裹身份文件用）。
pub fn store_identity_file(
    path: &Path,
    secret: &[u8; KEY_LEN],
    vault_key: &[u8],
) -> CoreResult<()> {
    let wrapped = seal_identity_bytes(secret, vault_key)?;
    write_identity_file(path, &wrapped)
}

/// 加载或创建 Host 身份私钥（文件不存在则生成并落盘为新格式）。
///
/// - 新格式 → 用 VK 解封；
/// - 旧格式（Base64 裸私钥）→ 解出后**立即改写为新格式**（自动迁移，幂等：
///   迁移失败的唯一后果是下次再迁一次）；
/// - 文件不存在 / 全部不可解析 → 生成新身份落盘（旧格式解析失败视为文件损坏，
///   重新生成等价于重置身份——与旧行为一致，对端只需重新配对）。
///
/// 【D-139】新格式文件**解封失败**（Crypto：包裹它的 VK 已不在；Validation：形状损坏）
/// 同样按「损坏等价重置」处理，落一份新身份——包裹密钥随「配对加入采纳对端 VK」
/// 永久消失后，这份私钥再也取不回来，保持报错只会让 Host 侧从此永远
/// 「加密操作失败」（主人实测：热点配对成功后生成二维码必现）。
/// **只吞打开类失败**；`std::fs::read` 的 IO 错误照旧上抛——磁盘抖动时不得把
/// 好文件当损坏重置。
pub fn load_or_create_identity_file(
    path: &Path,
    vault_key: &[u8],
) -> CoreResult<Zeroizing<[u8; KEY_LEN]>> {
    if path.exists() {
        let raw = std::fs::read(path)?;
        if raw.starts_with(IDENTITY_MAGIC) {
            match open_identity_bytes(&raw, vault_key) {
                Ok(secret) => return Ok(secret),
                // D-139：打开类失败（密钥丢失/形状损坏）→ 落入下方「新建」路径重置。
                Err(CoreError::Crypto(_) | CoreError::Validation(_)) => {}
                Err(e) => return Err(e),
            }
        }
        // 旧格式：Base64 裸私钥。
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        let text = String::from_utf8_lossy(&raw);
        if let Ok(bytes) = STANDARD.decode(text.trim().as_bytes()) {
            if bytes.len() == KEY_LEN {
                let mut secret = [0u8; KEY_LEN];
                secret.copy_from_slice(&bytes);
                // 迁移：立即改写为新格式（失败不阻断——旧文件仍在，下次重试）。
                if let Ok(wrapped) = seal_identity_bytes(&secret, vault_key) {
                    let _ = write_identity_file(path, &wrapped);
                }
                return Ok(Zeroizing::new(secret));
            }
        }
    }
    // 新建。
    let secret = rng::random_key()?;
    let wrapped = seal_identity_bytes(&secret, vault_key)?;
    write_identity_file(path, &wrapped)?;
    Ok(Zeroizing::new(secret))
}

/// 【D-96】身份文件格式单测：新格式封/解回环、旧格式迁移、错误 VK 拒绝、原子写。
#[cfg(test)]
mod tests {
    use super::*;

    fn vk() -> Vec<u8> {
        rng::random_key().expect("vk").to_vec()
    }

    #[test]
    fn seal_open_roundtrip() {
        let key = vk();
        let secret: [u8; KEY_LEN] = [7u8; KEY_LEN];
        let bytes = seal_identity_bytes(&secret, &key).unwrap();
        assert_eq!(bytes.len(), IDENTITY_FILE_LEN);
        assert!(bytes.starts_with(IDENTITY_MAGIC), "必须以魔数开头");
        let opened = open_identity_bytes(&bytes, &key).unwrap();
        assert_eq!(&opened[..], &secret);
    }

    #[test]
    fn wrong_vault_key_rejected() {
        let bytes = seal_identity_bytes(&[1u8; KEY_LEN], &vk()).unwrap();
        // 另一把 VK 解不开（tag 校验失败），不得泄露私钥。
        assert!(open_identity_bytes(&bytes, &vk()).is_err());
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        let key = vk();
        let mut bytes = seal_identity_bytes(&[2u8; KEY_LEN], &key).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        assert!(open_identity_bytes(&bytes, &key).is_err());
    }

    #[test]
    fn non_magic_input_rejected() {
        assert!(open_identity_bytes(b"garbage", &vk()).is_err());
        assert!(open_identity_bytes(&[], &vk()).is_err());
    }

    #[test]
    fn old_base64_format_migrates_to_wrapped_format() {
        // 旧格式迁移：Base64 裸私钥文件 → 首次加载即改写为新格式，且私钥值不变。
        let dir = std::env::temp_dir().join(format!("pms-d96-ident-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity");
        let secret: [u8; KEY_LEN] = rng::random_key().unwrap();
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        std::fs::write(&path, STANDARD.encode(secret)).unwrap();

        let key = vk();
        let loaded = load_or_create_identity_file(&path, &key).unwrap();
        assert_eq!(&loaded[..], &secret, "迁移不改变私钥值");

        // 文件已改写为新格式。
        let raw = std::fs::read(&path).unwrap();
        assert!(raw.starts_with(IDENTITY_MAGIC), "迁移后必须为新格式");
        let reopened = load_or_create_identity_file(&path, &key).unwrap();
        assert_eq!(&reopened[..], &secret, "新格式读回同一私钥");

        // 幂等：再次加载仍是同一私钥（迁移后走新格式稳定路径）。
        let again = load_or_create_identity_file(&path, &key).unwrap();
        assert_eq!(&again[..], &secret);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_new_identity_when_missing() {
        let dir = std::env::temp_dir().join(format!("pms-d96-ident-new-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity");
        let key = vk();
        let first = load_or_create_identity_file(&path, &key).unwrap();
        let raw = std::fs::read(&path).unwrap();
        assert!(raw.starts_with(IDENTITY_MAGIC), "新建即新格式");
        let second = load_or_create_identity_file(&path, &key).unwrap();
        assert_eq!(&first[..], &second[..], "已存在的身份不得重新生成");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 【D-139】文件在、但 VK 已变（配对加入采纳对端 VK 后旧 VK 永久丢失）：
    /// 加载不得报错卡死 Host 侧，而是重置出一份新身份并以当前 VK 落盘。
    #[test]
    fn unopenable_identity_resets_instead_of_erroring() {
        let dir = std::env::temp_dir().join(format!("pms-d139-reset-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity");
        let vk_old = vk();
        let vk_new = vk();
        let stale = load_or_create_identity_file(&path, &vk_old).unwrap();

        // 旧 VK 已丢，新 VK 加载：不再报 Crypto，而是重置。
        let reset = load_or_create_identity_file(&path, &vk_new).unwrap();
        assert_ne!(&reset[..], &stale[..], "旧私钥取不回，必须是新身份");

        // 重置后的文件以新 VK 稳定可读（幂等，不再反复重置）。
        let again = load_or_create_identity_file(&path, &vk_new).unwrap();
        assert_eq!(&reset[..], &again[..], "重置一次后必须稳定");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 【D-139】只有「打开类失败」才重置；`fs::read` 的 IO 失败照旧上抛——
    /// 磁盘抖动不得把好文件当损坏。用「同路径是个目录」构造 read 失败。
    #[test]
    fn io_error_on_read_still_propagates() {
        let dir = std::env::temp_dir().join(format!("pms-d139-io-{}", uuid::Uuid::new_v4()));
        let path = dir.join("identity");
        std::fs::create_dir_all(&path).unwrap(); // 目录：read 必 IO 失败
        let result = load_or_create_identity_file(&path, &vk());
        assert!(
            matches!(result, Err(CoreError::Io(_))),
            "IO 错误必须上抛，不得重置"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 【D-139】store_identity_file：采纳 VK 后重包裹的回环——新 VK 可读回同一私钥。
    #[test]
    fn store_identity_file_rewrap_roundtrip() {
        let dir = std::env::temp_dir().join(format!("pms-d139-rewrap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity");
        let vk_old = vk();
        let vk_new = vk();
        let secret = load_or_create_identity_file(&path, &vk_old).unwrap();

        store_identity_file(&path, &secret, &vk_new).unwrap();
        let reopened = load_or_create_identity_file(&path, &vk_new).unwrap();
        assert_eq!(&reopened[..], &secret[..], "重包裹后私钥值不变");
        // 旧 VK 已解不开（包裹密钥换了）。
        assert!(load_or_create_identity_file(&path, &vk_old).is_ok()); // 自愈路径，但私钥已换
        let by_old = load_or_create_identity_file(&path, &vk_old).unwrap();
        assert_ne!(
            &by_old[..],
            &secret[..],
            "旧 VK 加载只能触发重置，取不回原私钥"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn file_permissions_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pms-d96-ident-perm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity");
        let _ = load_or_create_identity_file(&path, &vk()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "身份文件必须 600（实得 {:o}）", mode);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
