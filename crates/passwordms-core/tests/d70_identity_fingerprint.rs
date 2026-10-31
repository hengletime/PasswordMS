//! D-70 验收：HELLO 携带 Joiner 的**长期**身份公钥 → 电脑端得到**稳定**的对端指纹。
//!
//! ## 这条修的是什么
//!
//! D-69 起电脑端 `PairedDevice.fingerprint` 恒为空串：HELLO 只带 Joiner 的临时 ECDH
//! 公钥（`joiner_pub`，每次配对新生成），拿它算指纹会让同一台手机每次配对都变成
//! 「新设备」——用一次性值冒充设备身份（D-67 同类错误）。于是 UI 只能不渲染指纹行，
//! 电脑端无法回答「这台手机是不是我那台」。
//!
//! D-70 的做法是加法式扩展协议：HELLO 增 `#[serde(default)] joiner_identity_pub`，
//! 手机把**身份文件**里的长期公钥发过去，电脑端据此算 `fingerprint_of(pub)`。
//!
//! ## 三条判据（对应 docs/03 §13.7 的验收）
//!
//! ① 新↔新：电脑侧存下 16 位小写 hex 指纹，且等于 `fingerprint_of(那把公钥)`；
//! ② 重配对不变：同一台手机（同一把长期身份）两次配对得到同一个指纹；
//! ③ 老客户端（不发该字段）：仍为空串，不崩、不退化为假指纹。
//!
//! **另有红线**：指纹必须由长期身份公钥算出，绝不能等于会话码
//! （会话码 = `sha256(临时公钥)`，每次配对都变）——两者相等就说明又混用了同一把钥匙。

use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::{fingerprint_of, HostIdentity};
use passwordms_core::sync::server::SyncServer;

const HOST_PWD: &str = "D70#Host-Pw1";
const JOINER_PWD: &str = "D70#Joiner-Pw1";

/// 一次回环配对的结果（电脑端视角 + 手机端视角）。
struct Outcome {
    /// 电脑端写进 `PairedDevice.fingerprint` 的值（`HostSyncResult.fingerprint`）。
    host_stored_fingerprint: String,
    /// 本次配对的会话码（每次都会变）。
    session_code: String,
    /// 手机端自己算出的长期身份指纹（"正确的答案"）。
    expected_fingerprint: String,
}

/// 跑一次完整回环。
///
/// `joiner_identity_pub` 为手机端长期身份**公钥**（生产路径传的就是 `public_bytes()`）；
/// 传 `None` 模拟**老客户端**（不发该字段）。
fn pair_once(tag: &str, joiner_identity_pub: Option<[u8; 32]>) -> Outcome {
    let dir = std::env::temp_dir().join(format!("pms-d70-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let host_id = host_session.repo().unwrap().device_id().to_string();
    // 电脑端有自己的长期身份（与本次判据无关，固定值即可）。
    let host_identity = HostIdentity::from_secret_bytes(&[21u8; 32]);

    let mut server = SyncServer::start(
        0,
        host_id,
        "D70Host".to_string(),
        host_identity,
        0,
        now_ms(),
    )
    .unwrap();
    let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();

    let host_handle = {
        let mut host_session = host_session;
        std::thread::spawn(move || {
            let repo = host_session.repo_mut().unwrap();
            server.accept_and_sync(repo, now_ms())
        })
    };

    let mut joiner_session = Session::create(&joiner_vault, JOINER_PWD, true).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();
    let mut joiner = client::pair(
        &qr,
        &joiner_id,
        "D70Joiner",
        0,
        joiner_identity_pub,
        now_ms(),
    )
    .unwrap();
    let session_code = joiner.session_code().to_string();

    joiner_session
        .adopt_vault_key(joiner.host_vault_key())
        .unwrap();
    {
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        let _ = joiner.push_and_sync(repo, now_ms()).unwrap();
    }
    drop(joiner);
    let host_result = host_handle.join().unwrap();
    let host_result = host_result.expect("回环配对失败");

    let _ = std::fs::remove_dir_all(&dir);
    Outcome {
        host_stored_fingerprint: host_result.fingerprint,
        session_code,
        // 手机端「正确答案」：由同一把长期身份公钥算出的指纹（与电脑端同算法同长度）。
        expected_fingerprint: joiner_identity_pub
            .map(|k| fingerprint_of(&k))
            .unwrap_or_default(),
    }
}

/// 判据①：新↔新 —— 电脑端存下手机**长期身份**的指纹（16 位小写 hex），值完全一致。
#[test]
fn host_stores_joiner_identity_fingerprint() {
    let secret = [31u8; 32];
    let phone_identity = HostIdentity::from_secret_bytes(&secret);
    let pubkey = phone_identity.public_bytes();
    let out = pair_once("store", Some(pubkey));

    assert_eq!(
        out.host_stored_fingerprint, out.expected_fingerprint,
        "电脑端存的必须是「手机长期身份公钥的 sha256 前 8 字节」，实得 {:?}",
        out.host_stored_fingerprint
    );
    assert_eq!(
        out.host_stored_fingerprint,
        fingerprint_of(&pubkey),
        "指纹算法必须是 fingerprint_of(长期公钥)"
    );
    assert_eq!(
        out.host_stored_fingerprint.len(),
        16,
        "设备指纹是 16 位小写 hex，实得 {:?}",
        out.host_stored_fingerprint
    );
    assert!(
        out.host_stored_fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "必须小写 hex：{}",
        out.host_stored_fingerprint
    );
    assert_eq!(
        out.host_stored_fingerprint,
        phone_identity.fingerprint(),
        "电脑端算出的指纹必须等于手机端自己算的身份指纹（同算法同输入）"
    );
}

/// 判据②：同一台手机（同一把长期身份）重配对，指纹不变；但会话码必须变。
///
/// 这条同时守住「别把会话码当指纹」：两者若相等，说明还在用临时公钥冒充身份。
#[test]
fn fingerprint_survives_repairing_but_session_code_rotates() {
    let pubkey = HostIdentity::from_secret_bytes(&[41u8; 32]).public_bytes();
    let a = pair_once("repair-a", Some(pubkey));
    let b = pair_once("repair-b", Some(pubkey));

    assert_eq!(
        a.host_stored_fingerprint, b.host_stored_fingerprint,
        "同一台手机的长期身份指纹跨配对必须一致（否则电脑端每次都当新设备）"
    );
    assert!(!a.host_stored_fingerprint.is_empty(), "指纹不得为空");
    assert_ne!(
        a.session_code, b.session_code,
        "会话码绑本次握手的临时密钥，重配对必须换新值"
    );
    assert_ne!(
        a.host_stored_fingerprint, a.session_code,
        "设备指纹绝不能等于会话码——相等即说明又拿临时公钥算了指纹"
    );
}

/// 判据③：老客户端（不发 `joiner_identity_pub`）→ 电脑端留空，不崩、不退化。
#[test]
fn legacy_client_without_identity_stays_empty() {
    let out = pair_once("legacy", None);
    assert!(
        out.host_stored_fingerprint.is_empty(),
        "老客户端不发长期身份 → 必须留空串（UI 不渲染指纹行），实得 {:?}",
        out.host_stored_fingerprint
    );
}

/// 判据③的加强：**不同**手机（不同长期身份）→ 不同指纹（否则认不出是两台设备）。
#[test]
fn different_devices_get_different_fingerprints() {
    let a = pair_once(
        "diff-a",
        Some(HostIdentity::from_secret_bytes(&[51u8; 32]).public_bytes()),
    );
    let b = pair_once(
        "diff-b",
        Some(HostIdentity::from_secret_bytes(&[52u8; 32]).public_bytes()),
    );
    assert_ne!(
        a.host_stored_fingerprint, b.host_stored_fingerprint,
        "两台不同手机必须得到不同指纹"
    );
}
