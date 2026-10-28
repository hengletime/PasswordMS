//! 1.3.0 生物识别快速解锁 · 旁路密封守门（`docs/08-biometric-unlock.md` §9-A，10 例）。
//!
//! 全部**平台无关**：用 [`MockSealer`] 顶替 Windows DPAPI / Android Keystore，
//! 任何机器都能跑。守的核心是三条命门：
//! 1. **改主密码即失效**（绑定熵含 `verify_mac`）——本方案最重要的安全性质；
//! 2. **失败分类不得折叠成 `UnlockFailed`**（否则用户永远不知道是「密码错」还是「该去配 Hello」）；
//! 3. **写入要么完整要么不动**（旧文件一字不改、tmp 保留），且重复开启零残留。

use std::path::PathBuf;

use passwordms_core::biometric::{
    bind_check_of, bind_of, enable, unlock, Availability, InvalidatedReason, SealError, SealState,
    SealStore, Subtype, VaultSealer, SEAL_TMP_FILE,
};
use passwordms_core::session::Session;
use zeroize::Zeroizing;

/// 测试用假 VK（真实 VK 由 RNG 生成，测试不关心具体值）。
const VK_A: [u8; 32] = [0x11; 32];
const VK_B: [u8; 32] = [0x22; 32];
const MAC_1: [u8; 32] = [0xAA; 32];
const MAC_2: [u8; 32] = [0xBB; 32];

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-bio-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 假 SEALER 的行为模式。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// 正常工作。
    Ok,
    /// 用户在弹窗中取消（平台侧拒绝）。
    Cancel,
    /// 平台密钥永久失效（对应 Android 新增指纹）。
    Invalidated,
    /// 加密阶段直接失败（用于验证「失败不动旧文件」）。
    FailSeal,
}

/// 假 SEALER：`keystream` 异或 + 8 字节 tag，语义上等价于「带 AAD 的 AEAD」。
///
/// 不追求密码学强度（这是 mock），只要求两点与真实平台一致：
/// 1. `bind` 不同 ⇒ tag 校验必失败；
/// 2. 密文任一字节被改 ⇒ tag 必失败。
struct MockSealer {
    key: [u8; 32],
    mode: Mode,
    subtype: Subtype,
    availability: Availability,
}

impl MockSealer {
    fn new() -> Self {
        Self {
            key: [0x5A; 32],
            mode: Mode::Ok,
            subtype: Subtype::AndroidKeystore,
            availability: Availability::Available,
        }
    }

    fn with(mode: Mode, availability: Availability) -> Self {
        Self {
            mode,
            availability,
            ..Self::new()
        }
    }

    fn keystream(&self, bind: &[u8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            let k = self.key[i % 32];
            let b = bind[i % bind.len()];
            *byte = k.rotate_left((i % 7) as u32) ^ b ^ (i as u8).wrapping_mul(31);
        }
        out
    }

    /// tag 覆盖 vk 全部 32 字节（任一字节被改都会变），并绑定 `bind`。
    fn tag(&self, bind: &[u8], vk: &[u8]) -> [u8; 8] {
        let ks = self.keystream(bind);
        let mut out = [0u8; 8];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = ks[(i * 3) % 32] ^ vk[i] ^ vk[i + 8] ^ vk[i + 16] ^ vk[i + 24] ^ (i as u8);
        }
        out
    }
}

impl VaultSealer for MockSealer {
    fn availability(&self) -> Availability {
        self.availability
    }

    fn subtype(&self) -> Subtype {
        self.subtype
    }

    fn seal(&self, vk: &[u8], bind: &[u8]) -> Result<Vec<u8>, SealError> {
        if self.mode == Mode::FailSeal {
            return Err(SealError::Platform("mock seal failure".into()));
        }
        if vk.len() != 32 {
            return Err(SealError::Platform("vk len".into()));
        }
        let ks = self.keystream(bind);
        let mut out = self.tag(bind, vk).to_vec();
        out.extend(vk.iter().zip(ks.iter()).map(|(a, b)| a ^ b));
        Ok(out)
    }

    fn unseal(&self, sealed: &[u8], bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        match self.mode {
            Mode::Cancel => return Err(SealError::Canceled),
            Mode::Invalidated => return Err(SealError::Invalidated),
            _ => {}
        }
        if sealed.len() != 40 {
            return Err(SealError::Platform("sealed len".into()));
        }
        let ks = self.keystream(bind);
        let vk: Vec<u8> = sealed[8..]
            .iter()
            .zip(ks.iter())
            .map(|(a, b)| a ^ b)
            .collect();
        if self.tag(bind, &vk)[..] != sealed[..8] {
            return Err(SealError::Platform("tag mismatch".into()));
        }
        Ok(Zeroizing::new(vk))
    }
}

// ── 1. 编解码 round-trip ──────────────────────────────────────────────
#[test]
fn seal_record_roundtrip() {
    let dir = temp_dir("roundtrip");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();
    let rec = enable(&store, &sealer, "vault-1", &MAC_1, &VK_A).unwrap();

    assert_eq!(rec.platform_subtype, Subtype::AndroidKeystore);
    assert_eq!(
        rec.bind_check,
        bind_check_of(&bind_of("vault-1", &MAC_1)).to_vec()
    );

    match store.inspect("vault-1", &MAC_1).unwrap() {
        SealState::Ready(back) => assert_eq!(back, rec, "读回必须与写入完全一致"),
        other => panic!("应为 Ready，实为 {other:?}"),
    }
    let vk = unlock(&store, &sealer, "vault-1", &MAC_1).unwrap();
    assert_eq!(&vk[..], &VK_A[..]);
}

// ── 2. 跨库复用必须被拒（vault_id 不匹配）─────────────────────────────
#[test]
fn foreign_vault_seal_is_absent() {
    let dir = temp_dir("foreign");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();
    enable(&store, &sealer, "vault-1", &MAC_1, &VK_A).unwrap();

    // 换个库读同一个文件：应当当「未开启」，既不复用、也不报奇怪错误。
    assert!(matches!(
        store.inspect("vault-2", &MAC_1).unwrap(),
        SealState::Absent
    ));
    let err = unlock(&store, &sealer, "vault-2", &MAC_1).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");
}

// ── 3. 改主密码 ⇒ 失效（本方案的核心性质）─────────────────────────────
#[test]
fn master_password_change_invalidates_seal() {
    let dir = temp_dir("passwd");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();
    enable(&store, &sealer, "vault-1", &MAC_1, &VK_A).unwrap();

    match store.inspect("vault-1", &MAC_2).unwrap() {
        SealState::Stale(InvalidatedReason::MasterPasswordChanged) => {}
        other => panic!("应判为改主密码失效，实为 {other:?}"),
    }
    let err = unlock(&store, &sealer, "vault-1", &MAC_2).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_INVALIDATED");
    assert_eq!(err.message_key(), "err.bio_invalidated_password");
    // 绝不能折叠成解锁失败——那会让用户去怀疑主密码。
    assert_ne!(err.code(), "ERR_UNLOCK_FAILED");
}

// ── 4. 密文被篡改 ⇒ 判篡改（而非「密码变了」）────────────────────────
#[test]
fn tampered_ciphertext_is_classified_as_tampered() {
    let dir = temp_dir("tamper");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();
    enable(&store, &sealer, "vault-1", &MAC_1, &VK_A).unwrap();

    // 直接在磁盘上改密文的一个字节，绑定校验值保持不动（模拟「文件被改坏/被改」）。
    let raw = std::fs::read(store.path()).unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let b64 = v["sealed_vk"].as_str().unwrap().to_string();
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let mut sealed = STANDARD.decode(b64).unwrap();
    sealed[10] ^= 0x01;
    v["sealed_vk"] = serde_json::Value::String(STANDARD.encode(&sealed));
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();

    // 绑定串没变 ⇒ 仍应判 Ready（否则就成了「改密码」误报）。
    assert!(matches!(
        store.inspect("vault-1", &MAC_1).unwrap(),
        SealState::Ready(_)
    ));
    let err = unlock(&store, &sealer, "vault-1", &MAC_1).unwrap_err();
    assert_eq!(err.message_key(), "err.bio_invalidated_tampered");
}

// ── 5. 未知版本 ⇒ 当未开启（不 panic、不误判可用）──────────────────────
#[test]
fn unknown_version_is_absent() {
    let dir = temp_dir("version");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();
    let rec = enable(&store, &sealer, "vault-1", &MAC_1, &VK_A).unwrap();

    let mut v = serde_json::to_value(&rec).unwrap();
    v["version"] = serde_json::json!(99);
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();

    assert!(matches!(
        store.inspect("vault-1", &MAC_1).unwrap(),
        SealState::Absent
    ));
}

// ── 6. 未知子类型 ⇒ 当未开启（前向兼容：旧版本读新 blob）──────────────
#[test]
fn unknown_subtype_is_absent() {
    let dir = temp_dir("subtype");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();
    let rec = enable(&store, &sealer, "vault-1", &MAC_1, &VK_A).unwrap();

    let mut v = serde_json::to_value(&rec).unwrap();
    v["platform_subtype"] = serde_json::json!("windows-future-mode");
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();

    assert!(matches!(
        store.inspect("vault-1", &MAC_1).unwrap(),
        SealState::Absent
    ));
}

// ── 7. 重复开启 ⇒ 只留新封印、且无 tmp 残留 ───────────────────────────
#[test]
fn re_enable_replaces_and_leaves_no_tmp() {
    let dir = temp_dir("reenable");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();
    let first = enable(&store, &sealer, "vault-1", &MAC_1, &VK_A).unwrap();
    let second = enable(&store, &sealer, "vault-1", &MAC_1, &VK_B).unwrap();
    assert_ne!(first.sealed_vk, second.sealed_vk, "新封印必须真的换了密文");

    let vk = unlock(&store, &sealer, "vault-1", &MAC_1).unwrap();
    assert_eq!(&vk[..], &VK_B[..], "读到的必须是新 VK");

    assert!(!store.tmp_path().exists(), "成功后不许留 tmp");
    let files = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(
        !files.iter().any(|f| f == SEAL_TMP_FILE),
        "目录里不许有 {SEAL_TMP_FILE}，实有 {files:?}"
    );
}

// ── 8. 开启失败 ⇒ 旧文件一字不动（且不留半截新文件）──────────────────
#[test]
fn failed_enable_keeps_previous_seal() {
    let dir = temp_dir("failen");
    let store = SealStore::new(&dir);
    let ok = MockSealer::new();
    enable(&store, &ok, "vault-1", &MAC_1, &VK_A).unwrap();
    let before = std::fs::read(store.path()).unwrap();

    let broken = MockSealer::with(Mode::FailSeal, Availability::Available);
    let err = enable(&store, &broken, "vault-1", &MAC_1, &VK_B).unwrap_err();
    assert_eq!(err.code(), "ERR_CRYPTO");

    assert_eq!(
        std::fs::read(store.path()).unwrap(),
        before,
        "失败的开启不许改动既有封印"
    );
    let vk = unlock(&store, &ok, "vault-1", &MAC_1).unwrap();
    assert_eq!(&vk[..], &VK_A[..], "旧封印必须仍然可用");
}

// ── 9. 无旁路文件 ⇒ 主密码解锁行为完全不变（回归）─────────────────────
#[test]
fn password_unlock_is_unaffected_without_seal() {
    const PWD: &str = "Bio#Guard-Pw1";
    let dir = temp_dir("regress");
    let path = dir.join("bio.vault");
    let store = SealStore::new(&dir);

    let mut s = Session::create(&path, PWD, true).unwrap();
    let header = s.repo().unwrap().header().clone();

    // 从未开启快速解锁：状态必须是「未开启」而不是任何错误。
    assert!(matches!(
        store.inspect(&header.vault_id, &header.verify_mac).unwrap(),
        SealState::Absent
    ));

    s.lock();
    assert!(!s.is_unlocked());
    let reopened = Session::unlock(&path, PWD).unwrap();
    assert!(reopened.is_unlocked(), "无侧面文件时主密码解锁必须照旧");
}

// ── 10. 生物会话没有 KEK ⇒ 配对加入要求主密码（不 panic）───────────────
#[test]
fn sealed_session_needs_master_password_for_adoption() {
    const PWD: &str = "Bio#Adopt-Pw1";
    let dir = temp_dir("nokek");
    let path = dir.join("bio.vault");
    let store = SealStore::new(&dir);
    let sealer = MockSealer::new();

    let mut s = Session::create(&path, PWD, true).unwrap();
    let header = s.repo().unwrap().header().clone();
    let vk: [u8; 32] = **s.repo().unwrap().vault_key();
    enable(&store, &sealer, &header.vault_id, &header.verify_mac, &vk).unwrap();

    s.lock();
    let vk = unlock(&store, &sealer, &header.vault_id, &header.verify_mac).unwrap();
    let mut bio = Session::unlock_with_vault_key(&path, vk).unwrap();
    assert!(bio.is_unlocked(), "生物路径应能打开保险库");
    assert!(!bio.has_kek(), "生物会话不得持有 KEK");

    // 空库 + 无 KEK：必须明确要求主密码，而不是 panic 或静默用空密钥包裹。
    let err = bio.adopt_vault_key(&[0x33; 32]).unwrap_err();
    assert_eq!(err.code(), "ERR_NEED_MASTER_PASSWORD");
}

// ── 11. 用户取消 ⇒ 独立错误码（前端据此静默回密码框，不弹红）─────────
#[test]
fn canceled_prompt_is_reported_as_canceled() {
    let dir = temp_dir("cancel");
    let store = SealStore::new(&dir);
    let ok = MockSealer::new();
    enable(&store, &ok, "vault-1", &MAC_1, &VK_A).unwrap();

    let canceled = MockSealer::with(Mode::Cancel, Availability::Available);
    let err = unlock(&store, &canceled, "vault-1", &MAC_1).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_CANCELED");
    assert_eq!(err.message_key(), "err.biometric_canceled");
    // 取消不是「解锁失败」：它必须能被前端区分出来做静默降级。
    assert_ne!(err.code(), "ERR_UNLOCK_FAILED");
}

// ── 12. 平台密钥失效 ⇒ 归类为「生物特征变更」（而非篡改/改密）────────
#[test]
fn platform_invalidation_maps_to_enrollment_change() {
    let dir = temp_dir("enroll");
    let store = SealStore::new(&dir);
    let ok = MockSealer::new();
    enable(&store, &ok, "vault-1", &MAC_1, &VK_A).unwrap();

    // 模拟 Android 新增指纹后 KeyPermanentlyInvalidated：绑定串没变，但平台拒绝解封。
    let invalidated = MockSealer::with(Mode::Invalidated, Availability::Available);
    let err = unlock(&store, &invalidated, "vault-1", &MAC_1).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_INVALIDATED");
    assert_eq!(err.message_key(), "err.bio_invalidated_enrollment");
}

// ── 13. 平台不可用（未配置 Hello / 未录入指纹）⇒ 不可用，不误判失效 ──
#[test]
fn unavailable_platform_is_not_treated_as_invalidated() {
    let dir = temp_dir("unavail");
    let store = SealStore::new(&dir);
    let ok = MockSealer::new();
    enable(&store, &ok, "vault-1", &MAC_1, &VK_A).unwrap();

    let unconfigured = MockSealer::with(Mode::Ok, Availability::NotConfigured);
    let err = unlock(&store, &unconfigured, "vault-1", &MAC_1).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");

    // 关键：开启时平台不可用必须直接拒绝，不许写下一份「当场用不了」的封印。
    let err = enable(&store, &unconfigured, "vault-1", &MAC_1, &VK_A).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");
    let vk = unlock(&store, &ok, "vault-1", &MAC_1).unwrap();
    assert_eq!(&vk[..], &VK_A[..], "被拒的开启不得破坏既有封印");
}
