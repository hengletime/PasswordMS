//! 重放 / 未配对拒绝（架构文档 §8.4、§3.7，验收点 A-06 / A-07 / A-08）。
//!
//! 状态：已在本机实跑通过（`cargo test -p passwordms-core`，见 docs/03b-verification.md）。
//!
//! **时间源说明（T-4）**：防重放的「当前时间」由调用方以 `now_ms` 参数**注入**
//! （见 [`ReplayGuard::check_ts`] / [`ReplayGuard::accept`]），因此时钟回拨可在测试内
//! 直接模拟，无需伪造系统时钟。本文件据此补充了回拨用例。

use std::path::{Path, PathBuf};
use std::time::Duration;

use passwordms_core::constants::{PAIR_TOKEN_TTL_SECS, QR_PROTOCOL_VERSION, SYNC_TIME_WINDOW_SECS};
use passwordms_core::crypto::rng;
use passwordms_core::session::Session;
use passwordms_core::store::repository::Repository;
use passwordms_core::sync::handshake;
use passwordms_core::sync::protocol::{
    open_envelope, seal_envelope, HandshakeMessage, SyncKind, SyncPayload,
};
use passwordms_core::sync::replay::ReplayGuard;
use passwordms_core::sync::resync::{ResyncHello, ResyncMessage, ResyncServer};
use passwordms_core::sync::server::SyncServer;
use passwordms_core::sync::transport;
use passwordms_core::sync::{Hello, HostIdentity};

// ===== 用例支撑工具 =====

/// 建一个临时空库并返回已解锁会话与文件路径。
fn temp_session() -> (Session, PathBuf) {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "passwordms_test_syncreject_{}.vault",
        uuid::Uuid::new_v4()
    ));
    let session = Session::create(&path, "qa_test_master_password", true).expect("create");
    (session, path)
}

/// 清理临时库文件及其设备文件。
fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("device"));
}

/// 模拟 Joiner：连接 Host 端口，发送 HELLO，返回收到的 REJECT 错误码。
fn client_send_hello(port: u16, token: Vec<u8>) -> String {
    let mut stream = transport::connect("127.0.0.1", port).expect("client connect");
    let hello = HandshakeMessage::Hello(Hello {
        v: QR_PROTOCOL_VERSION,
        token,
        joiner_id: "qa_test_joiner".to_string(),
        joiner_name: "QA-Joiner".to_string(),
        joiner_pub: vec![1u8; 32],
        sync_port: 0,
    });
    transport::send_frame(&mut stream, &serde_json::to_vec(&hello).expect("ser"))
        .expect("send hello");
    let bytes = transport::recv_frame(&mut stream).expect("recv reject");
    match serde_json::from_slice::<HandshakeMessage>(&bytes).expect("de") {
        HandshakeMessage::Reject(r) => r.code,
        other => panic!("期望 REJECT，实际收到 {other:?}"),
    }
}

/// 驱动一次「Host 配对令牌拒绝」，返回 `(服务端错误码, 客户端收到码)`。
///
/// - `cancel_first`：启动后立即 `cancel()` → 令牌标记为已用；
/// - `wrong_token`：客户端发送一个被篡改（首字节取反）的令牌；
/// - `expire`：以超出 `PAIR_TOKEN_TTL_SECS` 的 `now` 驱动验收 → 令牌过期。
fn run_server_reject(
    repo: &mut Repository,
    cancel_first: bool,
    wrong_token: bool,
    expire: bool,
) -> (String, String) {
    let start = 1_700_000_000_000i64;
    let identity = HostIdentity::generate().expect("identity");
    let mut server = SyncServer::start(
        0,
        "qa_test_host".to_string(),
        "QA-Host".to_string(),
        identity,
        0,
        start,
    )
    .expect("start");
    if cancel_first {
        server.cancel();
    }
    let token = if wrong_token {
        let mut t = server.token();
        t[0] ^= 0xff;
        t.to_vec()
    } else {
        server.token().to_vec()
    };
    let accept_now = if expire {
        start + (PAIR_TOKEN_TTL_SECS as i64) * 1000 + 1_000
    } else {
        start
    };
    let port = server.local_port().expect("local_port");

    std::thread::scope(|scope| {
        let client = scope.spawn(move || client_send_hello(port, token));
        let mut server_code: Option<String> = None;
        for _ in 0..500 {
            match server.poll_accept_and_sync(repo, accept_now) {
                Ok(Some(_)) => panic!("配对不应成功"),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => {
                    server_code = Some(e.code().to_string());
                    break;
                }
            }
        }
        let server_code = server_code.expect("服务端应在 5s 内返回拒绝");
        let client_code = client.join().expect("client join");
        (server_code, client_code)
    })
}

/// 模拟发起方：连接重同步监听端口，发送未配对 HELLO，返回收到的 REJECT 错误码。
fn client_send_resync_hello(port: u16) -> String {
    let mut stream = transport::connect("127.0.0.1", port).expect("client connect");
    let hello = ResyncMessage::Hello(ResyncHello {
        v: QR_PROTOCOL_VERSION,
        client_id: "qa_test_unpaired_device".to_string(),
        client_name: "QA-Joiner".to_string(),
        nonce_c: vec![7u8; 32],
    });
    transport::send_frame(&mut stream, &serde_json::to_vec(&hello).expect("ser"))
        .expect("send hello");
    let bytes = transport::recv_frame(&mut stream).expect("recv reject");
    match serde_json::from_slice::<ResyncMessage>(&bytes).expect("de") {
        ResyncMessage::Reject(r) => r.code,
        other => panic!("期望 REJECT，实际收到 {other:?}"),
    }
}

// ===== 防重放（A-08）=====

#[test]
fn replay_rejected_on_non_monotonic_seq() {
    let mut guard = ReplayGuard::new(SYNC_TIME_WINDOW_SECS);
    let now = 1_000_000i64;
    guard.accept(1, now, now).expect("first");
    // 相同或更小序号 → 重放。
    let err = guard.accept(1, now, now).unwrap_err();
    assert_eq!(err.code(), "ERR_REPLAY_DETECTED");
    let err2 = guard.accept(0, now, now).unwrap_err();
    assert_eq!(err2.code(), "ERR_REPLAY_DETECTED");
    // 递增序号 → 通过。
    guard.accept(2, now, now).expect("second");
}

#[test]
fn replay_rejected_outside_time_window() {
    let guard = ReplayGuard::new(SYNC_TIME_WINDOW_SECS);
    let now = 1_000_000_000i64;
    let too_old = now - (SYNC_TIME_WINDOW_SECS + 1) * 1000;
    let too_new = now + (SYNC_TIME_WINDOW_SECS + 1) * 1000;
    assert_eq!(guard.check_ts(too_old, now).unwrap_err().code(), "ERR_REPLAY_DETECTED");
    assert_eq!(guard.check_ts(too_new, now).unwrap_err().code(), "ERR_REPLAY_DETECTED");
    assert!(guard.check_ts(now, now).is_ok());
}

#[test]
fn replay_rejected_after_clock_rollback() {
    // 时钟回拨保护（时间源由调用方注入 `now_ms`，故可精确模拟）：
    // 同一 `ts` 在正常时刻落在 ±window 内可接受；一旦本机时钟被大幅回拨，
    // 该 `ts` 相对新的 `now` 落到窗外 → 被拒（阻断「回拨时钟绕过时间窗」）。
    let guard = ReplayGuard::new(SYNC_TIME_WINDOW_SECS);
    let ts = 1_700_000_000_000i64;
    // 正常时刻：`now == ts` → 窗内，接受。
    assert!(guard.check_ts(ts, ts).is_ok());
    // 模拟时钟回拨到窗口之外（`now` 远小于 `ts`）→ 原 `ts` 落到窗外，拒绝。
    let rolled_back = ts - (SYNC_TIME_WINDOW_SECS + 60) * 1000;
    assert_eq!(
        guard.check_ts(ts, rolled_back).unwrap_err().code(),
        "ERR_REPLAY_DETECTED"
    );
}

#[test]
fn envelope_aad_binds_session_and_seq() {
    let key = rng::random_key().expect("key");
    let session_id = rng::random_bytes(16).expect("sid");
    let payload = SyncPayload {
        kind: SyncKind::Push,
        device_id: "qa_test_device".to_string(),
        cursor: 0,
        changes: Vec::new(),
        conflicts: Vec::new(),
        blob_support: false,
    };
    let now = 1_700_000_000_000i64;
    let env = seal_envelope(&key, &session_id, 1, now, &payload).expect("seal");
    let opened = open_envelope(&key, &session_id, &env).expect("open");
    assert_eq!(opened.device_id, "qa_test_device");

    // 跨会话重放：会话 ID 不匹配 → 直接拒绝。
    let other_session = rng::random_bytes(16).expect("sid2");
    assert!(open_envelope(&key, &other_session, &env).is_err());

    // 同会话、不同 seq：`seq` 参与 AAD，篡改后 tag 校验失败（A-08 绑定强度）。
    let mut seq_tampered = env.clone();
    seq_tampered.seq = env.seq + 1;
    assert!(
        open_envelope(&key, &session_id, &seq_tampered).is_err(),
        "同会话改动 seq 必须导致解密失败（AAD 绑定 seq）"
    );

    // 同会话、不同 ts：`ts` 亦参与 AAD，篡改后同样失败。
    let mut ts_tampered = env.clone();
    ts_tampered.ts = env.ts + 1;
    assert!(
        open_envelope(&key, &session_id, &ts_tampered).is_err(),
        "同会话改动 ts 必须导致解密失败（AAD 绑定 ts）"
    );
}

// ===== 配对令牌一次性 / 时效（A-06）=====

#[test]
fn pair_token_rejections_via_server() {
    // A-06：以真实 `SyncServer` 行为验证三类拒绝，替代原「常量 > 0」的同义反复断言。
    let (mut session, path) = temp_session();
    let repo = session.repo_mut().expect("unlocked");

    // 1) 令牌已被使用（旧令牌作废）→ ERR_PAIR_TOKEN_USED。
    let (server, client) = run_server_reject(repo, true, false, false);
    assert_eq!(server, "ERR_PAIR_TOKEN_USED", "服务端应以「已用」拒绝");
    assert_eq!(client, "ERR_PAIR_TOKEN_USED", "客户端应收到「已用」");

    // 2) 令牌已过期 → ERR_PAIR_TOKEN_EXPIRED。
    let (server, client) = run_server_reject(repo, false, false, true);
    assert_eq!(server, "ERR_PAIR_TOKEN_EXPIRED", "服务端应以「过期」拒绝");
    assert_eq!(client, "ERR_PAIR_TOKEN_EXPIRED", "客户端应收到「过期」");

    // 3) 令牌不匹配 → ERR_PAIR_TOKEN_INVALID。
    let (server, client) = run_server_reject(repo, false, true, false);
    assert_eq!(server, "ERR_PAIR_TOKEN_INVALID", "服务端应以「无效」拒绝");
    assert_eq!(client, "ERR_PAIR_TOKEN_INVALID", "客户端应收到「无效」");

    cleanup(&path);
}

#[test]
fn pair_token_matches_semantics() {
    // 令牌常量时间比对的边界语义（长度为 0 / 32 / 内容差异）。
    let token = rng::random_token().expect("token");
    assert!(handshake::token_matches(&token, &token));
    let mut other = token;
    other[0] ^= 0xff;
    assert!(!handshake::token_matches(&token, &other));
    assert!(!handshake::token_matches(&token, &[]));
}

// ===== 未配对拒绝（A-07）=====

#[test]
fn resync_unpaired_is_rejected() {
    // A-07：未配对设备发起直连重同步 → 被拒 `ERR_NOT_PAIRED`（替代原
    // `matches!(CoreError::NotPaired.code(), "ERR_NOT_PAIRED")` 的同义反复断言）。
    let (mut session, path) = temp_session();
    let repo = session.repo_mut().expect("unlocked");
    let mut server = ResyncServer::start(0, "QA-Host".to_string()).expect("start");
    let port = server.port();

    std::thread::scope(|scope| {
        let client = scope.spawn(move || client_send_resync_hello(port));
        let mut server_err: Option<String> = None;
        for _ in 0..500 {
            match server.poll_once(repo, 1_700_000_000_000i64) {
                Ok(Some(_)) => panic!("未配对设备不应重同步成功"),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => {
                    server_err = Some(e.code().to_string());
                    break;
                }
            }
        }
        let server_err = server_err.expect("服务端应在 5s 内拒绝未配对设备");
        let client_code = client.join().expect("client join");
        assert_eq!(server_err, "ERR_NOT_PAIRED", "服务端应以「未配对」拒绝");
        assert_eq!(client_code, "ERR_NOT_PAIRED", "客户端应收到「未配对」");
    });

    cleanup(&path);
}

#[test]
fn key_confirmation_mismatch_is_detected() {
    let confirm_key = rng::random_key().expect("ck");
    let parts: [&[u8]; 2] = [b"hello", b"accept"];
    let transcript = handshake::transcript_hash(&parts);
    let mac = handshake::confirm_mac(&confirm_key, handshake::ROLE_SERVER, &transcript).expect("mac");
    let expected = handshake::confirm_mac(&confirm_key, handshake::ROLE_SERVER, &transcript).expect("mac");
    assert!(handshake::verify_mac(&expected, &mac));

    // 角色不同 → MAC 不同（阻断中间人重放）。
    let wrong = handshake::confirm_mac(&confirm_key, handshake::ROLE_CLIENT, &transcript).expect("mac");
    assert!(!handshake::verify_mac(&expected, &wrong));
}
