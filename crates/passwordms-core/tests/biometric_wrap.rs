//! 1.3.0 生物识别快速解锁 · 旁路密封守门（`docs/08-biometric-unlock.md` §9-A，27 例）。
//!
//! 全部**平台无关**：用 [`MockSealer`] 顶替 Windows DPAPI / Android Keystore，
//! 任何机器都能跑。守的命门：
//! 1. **改主密码即失效**（绑定熵含 `verify_mac`）与**换 VK 即失效**（含 `wrapped_vault_key`）——
//!    D-113 补的后者是审计 B-1 的直接修复：否则配对加入后封印会「显示可用、里面却是旧 VK」，
//!    空库照常解锁 ⇒ 新建条目用错密钥加密 ⇒ 整库永久打不开；
//! 2. **失败分类不得折叠成 `UnlockFailed`**（否则用户永远不知道是「密码错」还是「该去配 Hello」）；
//! 3. **写入要么完整要么不动**（旧文件一字不改、tmp 保留），且重复开启零残留；
//! 4. 结构损坏 / 跨平台 / 超限等畸形输入**不得**被误判为「可用」或静默当成「从未开启」。

use std::path::PathBuf;

use passwordms_core::biometric::{
    bind_of, enable, unlock, Availability, InvalidatedReason, SealError, SealState, SealStore,
    SealTarget, Subtype, VaultSealer, SEAL_MAX_BYTES, SEAL_TMP_FILE,
};
use passwordms_core::model::{ItemKind, LoginData, WrappedKey};
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
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

/// 合成一个封印目标（不建真库时用）。
fn synth_wkv(seed: u8) -> WrappedKey {
    WrappedKey {
        nonce: vec![seed; 12],
        ct: vec![seed.wrapping_add(1); 48],
    }
}

fn target<'a>(vault_id: &'a str, mac: &'a [u8], wkv: &'a WrappedKey) -> SealTarget<'a> {
    SealTarget {
        vault_id,
        verify_mac: mac,
        wrapped_vault_key: wkv,
    }
}

fn draft(title: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: None,
            username: Some("user".to_string()),
            password: Some("pw".to_string()),
            notes: None,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
    }
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
    /// 解封成功但返回荒谬长度（用于验证「长度白名单」）。
    ShortVk,
    /// 加密输出过短（模拟适配器把明文 VK 原样返回，用于验证落盘前的长度守卫）。
    ShortSeal,
}

/// 假 SEALER：`keystream` 异或 + 8 字节 tag，语义上等价于「带 AAD 的 AEAD」。
///
/// 不追求密码学强度（这是 mock），只要求两点与真实平台一致：
/// 1. `bind` 不同 ⇒ tag 校验必失败（**这是失效模型的唯一裁判，适配器必须照做**）；
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

    /// 16 字节 tag，覆盖 vk 全部 32 字节（任一字节被改都会变），并绑定 `bind`。
    ///
    /// 用 16 字节而非 8 是让密文总长达到 `SEAL_VK_MIN_LEN`（48 = 32 明文 + 16 tag）：
    /// 真实平台输出不会短于「明文 + GCM tag」，mock 也必须守住同一个下界（复审 L-5）。
    fn tag(&self, bind: &[u8], vk: &[u8]) -> [u8; 16] {
        let ks = self.keystream(bind);
        let mut out = [0u8; 16];
        for (i, byte) in out.iter_mut().enumerate() {
            // 两两覆盖：i 与 i+16 交替取遍 0..32。
            *byte = ks[i] ^ vk[i] ^ vk[i + 16] ^ (i as u8);
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
        if self.mode == Mode::ShortSeal {
            // 模拟适配器把明文原样返回：32 字节，短于 SEAL_VK_MIN_LEN。
            return Ok(vk.to_vec());
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
            Mode::ShortVk => return Ok(Zeroizing::new(vec![0u8; 5])),
            _ => {}
        }
        if sealed.len() != 48 {
            return Err(SealError::Platform("sealed len".into()));
        }
        let ks = self.keystream(bind);
        let vk: Vec<u8> = sealed[16..]
            .iter()
            .zip(ks.iter())
            .map(|(a, b)| a ^ b)
            .collect();
        if self.tag(bind, &vk)[..] != sealed[..16] {
            return Err(SealError::Platform("tag mismatch".into()));
        }
        Ok(Zeroizing::new(vk))
    }
}

// ── 1. 编解码与基本往返 ───────────────────────────────────────────────
#[test]
fn seal_record_roundtrip() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    let rec = enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    assert_eq!(rec.platform_subtype, Subtype::AndroidKeystore);
    assert_eq!(rec.wkv_check.len(), 8);
    assert_eq!(rec.mac_check.len(), 8);

    match store.inspect(&t).unwrap() {
        SealState::Ready(back) => assert_eq!(back, rec, "读回必须与写入完全一致"),
        other => panic!("应为 Ready，实为 {other:?}"),
    }
    let vk = unlock(&store, &sealer, &t).unwrap();
    assert_eq!(&vk[..], &VK_A[..]);
}

// ── 2. 跨库复用必须被拒（vault_id 不匹配）─────────────────────────────
#[test]
fn foreign_vault_seal_is_absent() {
    let dir = temp_dir("foreign");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    enable(
        &store,
        &sealer,
        &target("vault-1", &MAC_1, &wkv),
        &Zeroizing::new(VK_A),
    )
    .unwrap();

    // 换个库读同一个文件：应当当「未开启」，既不复用、也不报奇怪错误。
    let other = target("vault-2", &MAC_1, &wkv);
    assert!(matches!(store.inspect(&other).unwrap(), SealState::Absent));
    let err = unlock(&store, &sealer, &other).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");
}

// ── 3. 改主密码 ⇒ 失效（合成头部，单测绑定熵语义）───────────────────
#[test]
fn master_password_change_invalidates_seal() {
    let dir = temp_dir("passwd");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    enable(
        &store,
        &sealer,
        &target("vault-1", &MAC_1, &wkv),
        &Zeroizing::new(VK_A),
    )
    .unwrap();

    // `verify_mac` 变、`wrapped_vault_key` 不变 ⇒ 必须归「改主密码」而非「换 VK」。
    let after = target("vault-1", &MAC_2, &wkv);
    match store.inspect(&after).unwrap() {
        SealState::Stale(InvalidatedReason::MasterPasswordChanged) => {}
        other => panic!("应判为改主密码失效，实为 {other:?}"),
    }
    let err = unlock(&store, &sealer, &after).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_INVALIDATED");
    assert_eq!(err.message_key(), "err.bio_invalidated_password");
    // 绝不能折叠成解锁失败——那会让用户去怀疑主密码。
    assert_ne!(err.code(), "ERR_UNLOCK_FAILED");
}

// ── 4. 真跑 change_master_password ⇒ 封印必须失效（端到端，非合成常量）──
#[test]
fn real_password_change_invalidates_seal() {
    const PWD: &str = "Bio#Real-Pw1";
    const PWD2: &str = "Bio#Real-Pw2";
    let dir = temp_dir("realpasswd");
    let path = dir.join("real.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();

    let mut s = Session::create(&path, PWD, true).unwrap();
    s.repo_mut().unwrap().create_item(draft("条目")).unwrap();
    let header = s.repo().unwrap().header().clone();
    let vk = s.repo().unwrap().vault_key().clone();
    enable(&store, &sealer, &SealTarget::from_header(&header), &vk).unwrap();

    // 真改密码：`verify_mac` 与 `wrapped_vault_key` 都会变。
    s.change_master_password(PWD, PWD2).unwrap();
    let after = s.repo().unwrap().header().clone();
    drop(s);

    match store.inspect(&SealTarget::from_header(&after)).unwrap() {
        SealState::Stale(InvalidatedReason::MasterPasswordChanged) => {}
        other => panic!("真改密码后应判失效，实为 {other:?}"),
    }
    let err = unlock(&store, &sealer, &SealTarget::from_header(&after)).unwrap_err();
    assert_eq!(err.message_key(), "err.bio_invalidated_password");
}

// ── 5. 【D-113 / 审计 B-1 核心】配对换 VK ⇒ 封印必须失效且不得交出旧 VK ──
#[test]
fn adoption_invalidates_seal() {
    const PWD: &str = "Bio#Adopt-VK1";
    let dir = temp_dir("adoptvk");
    let path = dir.join("adopt.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();

    let mut s = Session::create(&path, PWD, true).unwrap();
    let header = s.repo().unwrap().header().clone();
    let vk_before = s.repo().unwrap().vault_key().clone();
    enable(
        &store,
        &sealer,
        &SealTarget::from_header(&header),
        &vk_before,
    )
    .unwrap();

    // 作为 Joiner 配对加入：VK 换成 Host 的，`verify_mac` **不变**（这正是 B-1 的成因）。
    s.adopt_vault_key(&[0x77; 32]).unwrap();
    let after = s.repo().unwrap().header().clone();
    drop(s);

    assert_eq!(
        after.verify_mac, header.verify_mac,
        "前提：配对不改变 verify_mac（否则本用例守不到东西）"
    );
    assert_ne!(
        after.wrapped_vault_key, header.wrapped_vault_key,
        "前提：配对必须改变 wrapped_vault_key"
    );

    // 修复前：这里会返回 Ready，随后解出**旧 VK**，把库锁死。
    match store.inspect(&SealTarget::from_header(&after)).unwrap() {
        SealState::Stale(InvalidatedReason::VaultKeyChanged) => {}
        other => panic!("配对后封印必须判为换 VK 失效，实为 {other:?}"),
    }
    let err = unlock(&store, &sealer, &SealTarget::from_header(&after)).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_INVALIDATED");
    assert_eq!(err.message_key(), "err.bio_invalidated_vault_key");
}

// ── 6. 密文被篡改 ⇒ 判篡改（而非「密码变了」）────────────────────────
#[test]
fn tampered_ciphertext_is_classified_as_tampered() {
    let dir = temp_dir("tamper");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    // 直接在磁盘上改密文的一个字节，校验值保持不动（模拟「文件被改坏/被改」）。
    let raw = std::fs::read(store.path()).unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let b64 = v["sealed_vk"].as_str().unwrap().to_string();
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let mut sealed = STANDARD.decode(b64).unwrap();
    sealed[10] ^= 0x01;
    v["sealed_vk"] = serde_json::Value::String(STANDARD.encode(&sealed));
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();

    // 校验值没变 ⇒ 仍应判 Ready（否则就成了「改密码」误报）。
    assert!(matches!(store.inspect(&t).unwrap(), SealState::Ready(_)));
    let err = unlock(&store, &sealer, &t).unwrap_err();
    assert_eq!(err.message_key(), "err.bio_invalidated_tampered");
}

// ── 7. 未知版本 ⇒ 当未开启（不 panic、不误判可用）──────────────────────
#[test]
fn unknown_version_is_absent() {
    let dir = temp_dir("version");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    let rec = enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    let mut v = serde_json::to_value(&rec).unwrap();
    v["version"] = serde_json::json!(99);
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();

    assert!(matches!(store.inspect(&t).unwrap(), SealState::Absent));
}

// ── 8. 未知子类型 ⇒ 当未开启（前向兼容：旧版本读新 blob）──────────────
#[test]
fn unknown_subtype_is_absent() {
    let dir = temp_dir("subtype");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    let rec = enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    let mut v = serde_json::to_value(&rec).unwrap();
    v["platform_subtype"] = serde_json::json!("windows-future-mode");
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();

    // 前向兼容：未来新增的保护手段不该让老版本报「损坏」，只当没开启。
    assert!(matches!(store.inspect(&t).unwrap(), SealState::Absent));
    let err = unlock(&store, &sealer, &t).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");
}

// ── 9. 结构损坏 ⇒ Corrupt（不得伪装成「从未开启」，审计 M-3）────────────
#[test]
fn corrupt_json_is_corrupt_not_absent() {
    let dir = temp_dir("corrupt");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    std::fs::write(store.path(), b"{ this is not json").unwrap();
    assert!(matches!(store.inspect(&t).unwrap(), SealState::Corrupt));
    // 损坏时 unlock 必须拒绝，且归到「需重新开启」一类（Tampered 文案）。
    let err = unlock(&store, &sealer, &t).unwrap_err();
    assert_eq!(err.message_key(), "err.bio_invalidated_tampered");
}

// ── 10. 超限文件 ⇒ Corrupt，不整份读入内存（审计 M-4）────────────────
#[test]
fn oversized_seal_file_is_corrupt() {
    let dir = temp_dir("oversize");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    std::fs::write(store.path(), vec![b'x'; SEAL_MAX_BYTES as usize + 1]).unwrap();
    assert!(matches!(store.inspect(&t).unwrap(), SealState::Corrupt));
}

// ── 11. 跨平台子类型 ⇒ 不可用（不是「损坏」，别据此删封印，审计 M-1）──
#[test]
fn cross_platform_subtype_is_unavailable() {
    let dir = temp_dir("xplat");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    // 把封印改成 Windows 子类型（模拟「目录从 Windows 拷到 Android」）。
    let raw = std::fs::read(store.path()).unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    v["platform"] = serde_json::json!("windows");
    v["platform_subtype"] = serde_json::json!("windows-dpapi-user");
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();

    // 结构自洽（都是 windows）⇒ 应判 Ready，但解封时因 subtype 不符报「不可用」。
    let err = unlock(&store, &sealer, &t).unwrap_err();
    assert_eq!(
        err.code(),
        "ERR_BIOMETRIC_UNAVAILABLE",
        "跨平台封印必须报「本机不可用」，绝不能报「已损坏/被篡改」让用户去删"
    );
}

// ── 12. 重复开启 ⇒ 只留新封印、且无 tmp 残留 ──────────────────────────
#[test]
fn re_enable_replaces_and_leaves_no_tmp() {
    let dir = temp_dir("reenable");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    let first = enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();
    let second = enable(&store, &sealer, &t, &Zeroizing::new(VK_B)).unwrap();
    assert_ne!(first.sealed_vk, second.sealed_vk, "新封印必须真的换了密文");

    let vk = unlock(&store, &sealer, &t).unwrap();
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

// ── 13. 开启失败 ⇒ 旧文件一字不动（且不留半截新文件）──────────────────
#[test]
fn failed_enable_keeps_previous_seal() {
    let dir = temp_dir("failen");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let ok = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &ok, &t, &Zeroizing::new(VK_A)).unwrap();
    let before = std::fs::read(store.path()).unwrap();

    let broken = MockSealer::with(Mode::FailSeal, Availability::Available);
    let err = enable(&store, &broken, &t, &Zeroizing::new(VK_B)).unwrap_err();
    assert_eq!(err.code(), "ERR_CRYPTO");

    assert_eq!(
        std::fs::read(store.path()).unwrap(),
        before,
        "失败的开启不许改动既有封印"
    );
    let vk = unlock(&store, &ok, &t).unwrap();
    assert_eq!(&vk[..], &VK_A[..], "旧封印必须仍然可用");
}

// ── 14. 无旁路文件 ⇒ 主密码解锁行为完全不变（回归）─────────────────────
#[test]
fn password_unlock_is_unaffected_without_seal() {
    const PWD: &str = "Bio#Guard-Pw1";
    let dir = temp_dir("regress");
    let path = dir.join("bio.vault");
    let store = SealStore::new(&path);

    let mut s = Session::create(&path, PWD, true).unwrap();
    let header = s.repo().unwrap().header().clone();

    // 从未开启快速解锁：状态必须是「未开启」而不是任何错误。
    assert!(matches!(
        store.inspect(&SealTarget::from_header(&header)).unwrap(),
        SealState::Absent
    ));

    s.lock();
    assert!(!s.is_unlocked());
    let reopened = Session::unlock(&path, PWD).unwrap();
    assert!(reopened.is_unlocked(), "无侧面文件时主密码解锁必须照旧");
}

// ── 15. 生物会话没有 KEK ⇒ 配对加入要求主密码（且不能被「需要空库」抢先）──
#[test]
fn sealed_session_needs_master_password_for_adoption() {
    const PWD: &str = "Bio#Adopt-Pw1";
    let dir = temp_dir("nokek");
    let path = dir.join("bio.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();

    let mut s = Session::create(&path, PWD, true).unwrap();
    // 必须非空：否则生物路径会（按设计）直接要求主密码，本用例就守不到 adopt。
    s.repo_mut().unwrap().create_item(draft("条目")).unwrap();
    let header = s.repo().unwrap().header().clone();
    let vk = s.repo().unwrap().vault_key().clone();
    enable(&store, &sealer, &SealTarget::from_header(&header), &vk).unwrap();

    s.lock();
    let vk = unlock(&store, &sealer, &SealTarget::from_header(&header)).unwrap();
    let mut bio = Session::unlock_with_vault_key(&path, vk).unwrap();
    assert!(bio.is_unlocked(), "生物路径应能打开保险库");
    assert!(!bio.has_kek(), "生物会话不得持有 KEK");

    // 非空库 + 无 KEK：必须先要主密码。若判定顺序反了，这里会返回
    // `ERR_VK_ADOPT_REQUIRES_EMPTY`（审计 L-1），把用户引向「去新建空库」的错方向。
    let err = bio.adopt_vault_key(&[0x33; 32]).unwrap_err();
    assert_eq!(err.code(), "ERR_NEED_MASTER_PASSWORD");
}

// ── 16. 空库 + 生物路径 ⇒ 必须要求主密码（无法证明 VK 属于本库）─────────
#[test]
fn empty_vault_biometric_unlock_requires_master_password() {
    const PWD: &str = "Bio#Empty-Pw1";
    let dir = temp_dir("emptyvault");
    let path = dir.join("empty.vault");

    let s = Session::create(&path, PWD, true).unwrap();
    drop(s);

    // 空库没有任何活帧可解密 ⇒ VK 对不对无从检验。生物路径必须拒绝，
    // 否则错 VK 会静默进入，新建条目全部用错密钥加密（审计 B-1 的底层成因）。
    match Session::unlock_with_vault_key(&path, Zeroizing::new([0xAB; 32])) {
        Ok(_) => panic!("空库生物解锁必须被拒绝，不得静默接受未知 VK"),
        Err(e) => assert_eq!(e.code(), "ERR_NEED_MASTER_PASSWORD"),
    }

    // 主密码路径不受影响。
    assert!(Session::unlock(&path, PWD).unwrap().is_unlocked());
}

// ── 17. 用户取消 ⇒ 独立错误码（前端据此静默回密码框，不弹红）─────────
#[test]
fn canceled_prompt_is_reported_as_canceled() {
    let dir = temp_dir("cancel");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let ok = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &ok, &t, &Zeroizing::new(VK_A)).unwrap();

    let canceled = MockSealer::with(Mode::Cancel, Availability::Available);
    let err = unlock(&store, &canceled, &t).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_CANCELED");
    assert_eq!(err.message_key(), "err.biometric_canceled");
    // 取消不是「解锁失败」：它必须能被前端区分出来做静默降级。
    assert_ne!(err.code(), "ERR_UNLOCK_FAILED");
}

// ── 18. 平台密钥失效 ⇒ 归类为「生物特征变更」（而非篡改/改密）────────
#[test]
fn platform_invalidation_maps_to_enrollment_change() {
    let dir = temp_dir("enroll");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let ok = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &ok, &t, &Zeroizing::new(VK_A)).unwrap();

    // 模拟 Android 新增指纹后 KeyPermanentlyInvalidated：绑定串没变，但平台拒绝解封。
    let invalidated = MockSealer::with(Mode::Invalidated, Availability::Available);
    let err = unlock(&store, &invalidated, &t).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_INVALIDATED");
    assert_eq!(err.message_key(), "err.bio_invalidated_enrollment");
}

// ── 19. 平台不可用（未配置 Hello / 未录入指纹）⇒ 不可用，不误判失效 ──
#[test]
fn unavailable_platform_is_not_treated_as_invalidated() {
    let dir = temp_dir("unavail");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let ok = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &ok, &t, &Zeroizing::new(VK_A)).unwrap();

    let unconfigured = MockSealer::with(Mode::Ok, Availability::NotConfigured);
    let err = unlock(&store, &unconfigured, &t).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");

    // 关键：开启时平台不可用必须直接拒绝，不许写下一份「当场用不了」的封印。
    let err = enable(&store, &unconfigured, &t, &Zeroizing::new(VK_A)).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");
    let vk = unlock(&store, &ok, &t).unwrap();
    assert_eq!(&vk[..], &VK_A[..], "被拒的开启不得破坏既有封印");
}

// ── 20. 适配器返回荒谬长度的 VK ⇒ 判篡改，不得当密钥用 ─────────────────
#[test]
fn short_vk_from_adapter_is_rejected() {
    let dir = temp_dir("shortvk");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let ok = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &ok, &t, &Zeroizing::new(VK_A)).unwrap();

    let short = MockSealer::with(Mode::ShortVk, Availability::Available);
    let err = unlock(&store, &short, &t).unwrap_err();
    assert_eq!(err.message_key(), "err.bio_invalidated_tampered");
}

// ── 21. 【B-1 防线的另一半】错 VK + 非空库必须失败（复审 M-2）──────────
#[test]
fn wrong_vk_cannot_open_nonempty_vault() {
    const PWD: &str = "Bio#WrongVK-Pw1";
    let dir = temp_dir("wrongvk");
    let path = dir.join("wrong.vault");

    let mut s = Session::create(&path, PWD, true).unwrap();
    s.repo_mut().unwrap().create_item(draft("条目")).unwrap();
    drop(s);

    // 「错 VK 打不开非空库」靠的是 `rebuild_index` 的 AEAD —— 今天没人守着这条。
    // 若哪天有人把 `open_frame(&frame)?` 的 `?` 改成 `continue`，B-1 立刻复发而其余用例全绿。
    if Session::unlock_with_vault_key(&path, Zeroizing::new([0xAB; 32])).is_ok() {
        panic!("非空库用错 VK 必须失败");
    }
    // 且失败不得破坏库：主密码仍能正常打开全部数据。
    let back = Session::unlock(&path, PWD).unwrap();
    assert_eq!(back.repo().unwrap().record_count(), 1);
}

// ── 22. 【核心推理固化】checks 通过 ≠ VK 对：回滚旧密文必须判篡改 ──────
#[test]
fn forged_checks_with_stale_ciphertext_is_tampered() {
    let dir = temp_dir("stale");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    // 用**另一个 bind**（模拟换过 VK 的旧封印密文）封一份，再塞进当前记录，
    // 两个 check 保持不动 —— 它们在磁盘上是公开可算的，攻击者能改。
    let stale = sealer
        .seal(&VK_B, &bind_of(&target("vault-1", &MAC_1, &synth_wkv(9))))
        .unwrap();
    patch_json(&store, |v| {
        v["sealed_vk"] = serde_json::json!(b64(&stale));
    });

    // check 全对 ⇒ 分类判可用（这是设计边界：check 不是预言机）
    assert!(matches!(store.inspect(&t).unwrap(), SealState::Ready(_)));
    // 但解封必须失败：平台 AAD/entropy 才是唯一裁判
    let err = unlock(&store, &sealer, &t).unwrap_err();
    assert_eq!(err.message_key(), "err.bio_invalidated_tampered");
}

// ── 23. 密文长度白名单两端（复审：48 是下限，32 的明文绝不能收）────────
#[test]
fn seal_length_boundaries_are_corrupt() {
    let dir = temp_dir("bounds");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    let rec = enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    for len in [31usize, 32, 47, 1025] {
        let mut v = serde_json::to_value(&rec).unwrap();
        v["sealed_vk"] = serde_json::json!(b64(&vec![0u8; len]));
        std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();
        assert!(
            matches!(store.inspect(&t).unwrap(), SealState::Corrupt),
            "长度 {len} 应判损坏（下限 {} / 上限 1024）",
            passwordms_core::biometric::SEAL_VK_MIN_LEN
        );
    }
}

// ── 24. platform 与 platform_subtype 自相矛盾 ⇒ 损坏（不是「未开启」）──
#[test]
fn contradictory_platform_fields_is_corrupt() {
    let dir = temp_dir("contra");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    patch_json(&store, |v| {
        // platform 说 android，subtype 说 windows —— 自相矛盾 = 被改坏
        v["platform_subtype"] = serde_json::json!("windows-dpapi-user");
    });
    assert!(matches!(store.inspect(&t).unwrap(), SealState::Corrupt));
}

// ── 25. 字段缺失 / 类型错 ⇒ 损坏（不得伪装成「从未开启」，复审 L-4）────
#[test]
fn missing_fields_is_corrupt() {
    let dir = temp_dir("missing");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    let rec = enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    // 缺 version
    let mut v = serde_json::to_value(&rec).unwrap();
    v.as_object_mut().unwrap().remove("version");
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(
        matches!(store.inspect(&t).unwrap(), SealState::Corrupt),
        "缺 version"
    );

    // 缺 platform_subtype
    let mut v = serde_json::to_value(&rec).unwrap();
    v.as_object_mut().unwrap().remove("platform_subtype");
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(
        matches!(store.inspect(&t).unwrap(), SealState::Corrupt),
        "缺 platform_subtype"
    );

    // version 类型错（字符串）
    let mut v = serde_json::to_value(&rec).unwrap();
    v["version"] = serde_json::json!("two");
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(
        matches!(store.inspect(&t).unwrap(), SealState::Corrupt),
        "version 类型错"
    );

    // 空对象
    std::fs::write(store.path(), b"{}").unwrap();
    assert!(
        matches!(store.inspect(&t).unwrap(), SealState::Corrupt),
        "空对象"
    );
}

// ── 26. 平台不可用优先于「文件损坏」（别让损坏抢走更基础的原因）─────────
#[test]
fn unavailable_platform_takes_priority_over_corrupt() {
    let dir = temp_dir("prio");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let sealer = MockSealer::new();
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);
    enable(&store, &sealer, &t, &Zeroizing::new(VK_A)).unwrap();

    // 文件坏掉 + 平台未配置：用户该看到「本机不可用」，而不是「配置损坏请重新开启」。
    std::fs::write(store.path(), b"{ broken").unwrap();
    let unconfigured = MockSealer::with(Mode::Ok, Availability::NotConfigured);
    let err = unlock(&store, &unconfigured, &t).unwrap_err();
    assert_eq!(err.code(), "ERR_BIOMETRIC_UNAVAILABLE");
}

// ── 27. 适配器把明文 VK 原样返回 ⇒ 拒绝落盘（复审 L-5）────────────────
#[test]
fn enable_rejects_short_ciphertext() {
    let dir = temp_dir("shortseal");
    let path = dir.join("v.vault");
    let store = SealStore::new(&path);
    let wkv = synth_wkv(1);
    let t = target("vault-1", &MAC_1, &wkv);

    // 32 字节输出 = 明文 VK 长度，若被收下就是「明文密钥落盘」。
    let broken = MockSealer::with(Mode::ShortSeal, Availability::Available);
    let err = enable(&store, &broken, &t, &Zeroizing::new(VK_A)).unwrap_err();
    assert_eq!(err.code(), "ERR_CRYPTO");
    assert!(!store.path().exists(), "拒绝的开启不得留下任何文件");
}

// ── 测试助手 ─────────────────────────────────────────────────────────
fn b64(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    STANDARD.encode(bytes)
}

/// 就地改写封印文件里的某个字段（模拟磁盘被改）。
fn patch_json(store: &SealStore, f: impl FnOnce(&mut serde_json::Value)) {
    let raw = std::fs::read(store.path()).unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    f(&mut v);
    std::fs::write(store.path(), serde_json::to_vec(&v).unwrap()).unwrap();
}
