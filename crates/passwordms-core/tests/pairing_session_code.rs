//! D-69 验收测试：**会话码两屏同值**（可逐字比对）+ **设备指纹稳定**（不可混用）。
//!
//! ## 这一版修掉的是什么（实测结论，非推断）
//!
//! v1.2.5 早期实现的 D-68 是**负收益**：电脑屏取 `identity.fingerprint()`
//! = `sha256(长期身份公钥)`，手机屏取 `fingerprint_of(&accept.host_pub)`
//! = `sha256(每次配对重新生成的临时 X25519 公钥)` —— 两侧来自两把不同的密钥，
//! 用户按提示核对**必然**看到「不一致」。根因是 `Accept`（`protocol.rs:136`）
//! 里根本没有长期身份公钥，手机拿不到能与电脑屏对齐的值。
//!
//! D-69 的修法：把「核对码」定义为**本次会话绑定的量**——
//! `session_code = sha256(host_eph 公钥) 前 8 字节 hex`，由
//! `SyncServer::start` 一次生成（`server.rs::session_code`），手机端由 `Accept.host_pub`
//! 算出同一个值（`client.rs::JoinerSession::session_code`）。两屏**构造上必然相等**，
//! 而中继式中间人两侧看到的 `host_pub` 不同 → 两码不等 → 能被抓出来。
//!
//! 同时把两件长期混淆的事分开命名：
//! - **会话码**（`session_code`）：每次配对都变，只用于本次人工核对；
//! - **设备指纹**（`device_fingerprint` / `PairedDevice.fingerprint`）：长期身份密钥，
//!   稳定不变，只用于设备列表辨识（D-67）。
//!
//! 电脑端**不写入**手机的指纹：HELLO 只带临时公钥，拿它算指纹就是又一次「冒充设备指纹」
//! （D-67 同类错误），故 `HostSyncResult.fingerprint` 留空，等 D-70 扩展 HELLO 再说。

use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::server::SyncServer;

const HOST_PWD: &str = "Sc#Host-Pw1";
const JOINER_PWD: &str = "Sc#Joiner-Pw1";

/// 一次完整回环配对后两侧屏幕上会显示的值。
struct Screens {
    /// 手机端记下的 Host 长期设备指纹（写入 `PairedDevice.fingerprint`）。
    device_fingerprint: String,
    /// 电脑屏会显示的值（`PairingDto.session_code`，取自 `SyncServer::session_code`）。
    host_session_code: String,
    /// 手机屏会显示的值（`PairResultDto.session_code`）。
    phone_session_code: String,
    /// 电脑端存下的对端（手机）指纹 —— D-69 起必须为空串。
    host_stored_fingerprint: String,
}

/// `identity_secret` 固定 → 代表「同一台电脑的长期身份」（真实场景：身份密钥
/// 持久化在应用数据目录，重启/重装都不变）。
fn pair_once(tag: &str, identity_secret: [u8; 32]) -> Screens {
    let dir = std::env::temp_dir().join(format!("pms-scode-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let host_id = host_session.repo().unwrap().device_id().to_string();
    let identity = HostIdentity::from_secret_bytes(&identity_secret);

    let mut server =
        SyncServer::start(0, host_id, "ScHost".to_string(), identity, 0, now_ms()).unwrap();
    let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();
    // 电脑屏显示的会话码：**手机连上之前就能拿到**（临时密钥在 start 内已生成）。
    let host_session_code = server.session_code();

    let host_handle = {
        let mut host_session = host_session;
        std::thread::spawn(move || {
            let repo = host_session.repo_mut().unwrap();
            server.accept_and_sync(repo, now_ms())
        })
    };

    let mut joiner_session = Session::create(&joiner_vault, JOINER_PWD, true).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();
    let mut joiner = client::pair(&qr, &joiner_id, "ScJoiner", 0, now_ms()).unwrap();
    let phone_session_code = joiner.session_code().to_string();
    let device_fingerprint = joiner.device_fingerprint().to_string();

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
    assert!(
        host_result.is_ok(),
        "回环配对失败，会话码比对无从谈起: {:?}",
        host_result.err()
    );

    let _ = std::fs::remove_dir_all(&dir);
    Screens {
        device_fingerprint,
        host_session_code,
        phone_session_code,
        host_stored_fingerprint: host_result.unwrap().fingerprint,
    }
}

/// 核心判据：同一次配对里，电脑屏与手机屏必须显示**同一个会话码**。
#[test]
fn both_screens_show_the_same_session_code() {
    let s = pair_once("same", [11u8; 32]);
    assert_eq!(
        s.phone_session_code, s.host_session_code,
        "手机屏的会话码必须等于电脑屏的会话码，否则用户按提示比对必然看到「不一致」"
    );
    assert_eq!(
        s.host_session_code.len(),
        16,
        "会话码是 8 字节 hex = 16 个字符"
    );
    assert!(
        s.host_session_code
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "会话码必须是 16 位小写 hex：{}",
        s.host_session_code
    );
}

/// 第二判据：会话码**每次配对都变**（会话绑定），设备指纹**跨配对不变**（长期身份）。
///
/// 这条同时守住「别再拿临时公钥冒充设备指纹」：两者若相等说明又混用了同一把钥匙。
#[test]
fn session_code_rotates_but_device_fingerprint_is_stable() {
    let secret = [13u8; 32];
    let a = pair_once("rot-a", secret);
    let b = pair_once("rot-b", secret);

    assert_ne!(
        a.host_session_code, b.host_session_code,
        "会话码绑本次握手的临时密钥，重配对必须换新值（否则它就不是会话绑定的）"
    );
    assert_eq!(
        a.device_fingerprint, b.device_fingerprint,
        "同一台电脑的长期设备指纹在两次配对间必须完全一致"
    );
    assert_ne!(
        a.host_session_code, a.device_fingerprint,
        "会话码与设备指纹必须是两个不同的量（混用就是这次要修的缺陷本身）"
    );
    assert_eq!(a.device_fingerprint.len(), 16, "设备指纹同样 16 位小写 hex");
}

/// 第三判据：电脑端不掌握手机长期身份时**留空**，不得拿临时公钥算个值冒充。
#[test]
fn host_side_leaves_peer_fingerprint_empty() {
    let s = pair_once("empty", [17u8; 32]);
    assert!(
        s.host_stored_fingerprint.is_empty(),
        "HELLO 只带临时公钥，电脑端算出的任何指纹都是「冒充设备指纹」；应为空串，实得 {:?}",
        s.host_stored_fingerprint
    );
}
