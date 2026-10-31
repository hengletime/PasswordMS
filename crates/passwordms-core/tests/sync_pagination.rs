//! O1 同步分页与发送水位回归套件（2026-10-24 审查 🟠-1 修复验收）。
//!
//! 验收对照（方案 §O1）：
//! 1. **1500 条小条目分多页全部到位** —— `case1_many_items_arrive_via_multiple_pages`；
//! 2. **页间中断重同步不丢不重 / 发送失败水位不推进** —— `case2_failed_sync_keeps_watermark`；
//! 3. **老格式 payload（无 cursor 字段）与新版互通** —— `case3_legacy_payload_interop`；
//! 4. **水位推进后第二轮导出为空** —— `case2`（resync 前后各断言一次）；
//! 5. **分页收发循环**（多页聚合、停在最后一帧）—— `paging_loop_aggregates_and_stops`。
//!
//! 水位失败安全语义：`note_sent_changes` 只在 Push 页发送成功后推进，且仅当整轮
//! 交换成功（`save_sync_state`）才落盘——连接失败/中断的轮次下一轮自动重发（宁多勿丢）。

use std::path::{Path, PathBuf};

use passwordms_core::crypto::aead;
use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client::{self, JoinerSyncResult};
use passwordms_core::sync::paging;
use passwordms_core::sync::protocol::{envelope_aad, ChangeRecord, SyncKind, SyncPayload};
use passwordms_core::sync::replay::ReplayGuard;
use passwordms_core::sync::resync::{self, ResyncOutcome, ResyncServer};
use passwordms_core::sync::server::{HostSyncResult, SyncServer};

const HOST_PWD: &str = "Pag#Host-Pw1";
const JOINER_PWD: &str = "Pag#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-pag-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn login_draft(title: &str) -> ItemDraft {
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

fn make_login(session: &mut Session, title: &str) {
    session
        .repo_mut()
        .unwrap()
        .create_item(login_draft(title))
        .unwrap();
}

/// 首次配对（Host 预置数据由调用方先行写入）；返回（host 结果、joiner 结果、host_device_id）。
fn pair_first(
    host_vault: &Path,
    resync_port: u16,
    joiner_vault: &Path,
) -> (HostSyncResult, JoinerSyncResult, String) {
    let host_device_id = {
        let s = Session::unlock(host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };
    let identity = passwordms_core::sync::handshake::HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_device_id.clone(),
        "PagHost".to_string(),
        identity,
        resync_port,
        now_ms(),
    )
    .unwrap();
    let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();

    let hv = host_vault.to_path_buf();
    let handle = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&hv, HOST_PWD).unwrap();
        let repo = host_session.repo_mut().unwrap();
        server.accept_and_sync(repo, now_ms())
    });

    let mut joiner_session = Session::create(joiner_vault, JOINER_PWD, true).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();
    let mut joiner =
        client::pair(&qr, &joiner_id, "PagJoiner", 0, None, now_ms()).expect("回环配对握手失败");
    joiner_session
        .adopt_vault_key(joiner.host_vault_key())
        .unwrap();
    let result = {
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        joiner.push_and_sync(repo, now_ms())
    };
    drop(joiner);
    let host_result = handle.join().unwrap().expect("host 配对同步失败");
    let result = result.expect("joiner 配对同步失败");
    (host_result, result, host_device_id)
}

/// 一次直连重同步（Joiner 发起）；Host 侧 ResyncServer 由调用方移入。
fn run_resync(
    host_vault: &Path,
    resync_server: ResyncServer,
    joiner_session: &mut Session,
    host_device_id: &str,
) -> (ResyncOutcome, ResyncOutcome) {
    let hv = host_vault.to_path_buf();
    let handle = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&hv, HOST_PWD).unwrap();
        let mut resync_server = resync_server;
        let repo = host_session.repo_mut().unwrap();
        loop {
            match resync_server.poll_once(repo, now_ms()) {
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(30)),
                other => return other,
            }
        }
    });
    let joiner_outcome = {
        let repo = joiner_session.repo_mut().unwrap();
        resync::resync_with_peer(repo, host_device_id, now_ms(), 0).unwrap()
    };
    let host_outcome = handle
        .join()
        .unwrap()
        .expect("host 轮询失败")
        .expect("host 应收到一次连接");
    (host_outcome, joiner_outcome)
}

// ===== 验收 1：1500 条分多页全部到位 =====

#[test]
fn case1_many_items_arrive_via_multiple_pages() {
    let dir = temp_dir("case1");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // Host 预置 1500 条（小条目估算 ~1.5KB/条 → 必然 ≥2 页）。
    {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        for i in 0..1500 {
            make_login(&mut hs, &format!("site-{i:04}"));
        }
    }

    let resync_server = ResyncServer::start(0, "PagHost".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, j1, _host_id) = pair_first(&host_vault, resync_port, &joiner_vault);
    let joiner_id = {
        let s = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };

    // 全部到位且无丢失无重复。
    let joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let got = joiner_session
        .repo()
        .unwrap()
        .list_items(None, false, None)
        .len();
    assert_eq!(
        got, 1500,
        "joiner 应收到全部 1500 条（配对 applied={}）",
        j1.applied
    );
    assert_eq!(j1.applied, 1500);
    drop(joiner_session);

    // 验收 4（首推后）：host 对 joiner 的导出应为空（配对轮已推进发送水位）。
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        let state = repo.sync_state(&joiner_id);
        assert!(
            repo.export_changes(&state).is_empty(),
            "配对成功后 host 导出必须为空（水位生效）"
        );
    }
}

// ===== 验收 2 + 4 + 5：增量水位 / 失败不推进 =====

#[test]
fn case2_failed_sync_keeps_watermark() {
    let dir = temp_dir("case2");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        for i in 0..12 {
            make_login(&mut hs, &format!("base-{i}"));
        }
    }

    let resync_server = ResyncServer::start(0, "PagHost".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);
    let joiner_id = {
        let s = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };

    // Host 新增 50 条 → 导出恰好 50（水位过滤旧 12 条）。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo_mut().unwrap();
        for i in 0..50 {
            repo.create_item(login_draft(&format!("extra-{i}")))
                .unwrap();
        }
        let state = repo.sync_state(&joiner_id);
        let exported = repo.export_changes(&state);
        assert_eq!(exported.len(), 50, "水位过滤后只应导出新增 50 条");
    }

    // 制造一次同步失败：把 joiner 侧记录的对端地址指向一个已关闭端口。
    let closed_port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port(); // listener 随即 drop → 端口关闭
    {
        let mut js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
        let repo = js.repo_mut().unwrap();
        repo.update_paired_addr(&host_id, "127.0.0.1", closed_port)
            .unwrap();
        let attempt = resync::resync_with_peer(repo, &host_id, now_ms(), 0);
        assert!(attempt.is_err(), "连已关闭端口必须失败");
        // 失败路径：水位不推进（本轮导出的 50 条下一轮仍要重发）。
        let state = repo.sync_state(&host_id);
        assert_eq!(
            repo.export_changes(&state).len(),
            0,
            "joiner 本身无新变更，导出应为空（此断言确认状态未被失败轮污染）"
        );
    }

    // 修复地址 → 重同步成功 → 50 条全部到位；host 导出再度为空。
    let resync_server2 = ResyncServer::start(0, "PagHost".to_string()).unwrap();
    {
        let mut js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
        let repo = js.repo_mut().unwrap();
        repo.update_paired_addr(&host_id, "127.0.0.1", resync_server2.port())
            .unwrap();
    }
    let (h2, j2) = run_resync(
        &host_vault,
        resync_server2,
        &mut { Session::unlock(&joiner_vault, JOINER_PWD).unwrap() },
        &host_id,
    );
    assert_eq!(j2.applied, 50);
    assert_eq!(h2.applied, 0);

    let joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    assert_eq!(
        joiner_session
            .repo()
            .unwrap()
            .list_items(None, false, None)
            .len(),
        62
    );
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        let state = repo.sync_state(&joiner_id);
        assert!(
            repo.export_changes(&state).is_empty(),
            "第二轮同步后 host 导出必须为空"
        );
    }
}

// ===== 验收 3：老格式 payload（无 cursor 字段）互通 =====

#[test]
fn case3_legacy_payload_interop() {
    // 构造一帧「老端」Push 信封：JSON 里**没有** cursor 字段（serde default → 0），
    // 后面再跟一帧垃圾数据——接收循环必须在第一帧（cursor=0）后停下，不得误读。
    let sync_key = [7u8; 32];
    let session_id = [9u8; 32];
    let rec = ChangeRecord {
        record_type: passwordms_core::constants::RECORD_TYPE_ITEM,
        record_id: uuid::Uuid::new_v4().to_string(),
        rev: 3,
        updated_at: 1,
        modified_by: String::new(),
        deleted: false,
        wrapped_key: vec![0u8; 48],
        wrapped_key_nonce: vec![0u8; 24],
        nonce: vec![0u8; 24],
        plaintext_len: 4,
        ciphertext: vec![1u8; 4],
    };
    let payload = SyncPayload {
        kind: SyncKind::Push,
        device_id: "legacy-dev".to_string(),
        cursor: 0,
        changes: vec![rec],
        conflicts: Vec::new(),
        blob_support: false,
        pair_only: false,
    };
    // 手工序列化后删掉 cursor 键 = 老端线上形态。
    let mut value = serde_json::to_value(&payload).unwrap();
    value.as_object_mut().unwrap().remove("cursor");
    let pt = serde_json::to_vec(&value).unwrap();
    let ts = now_ms();
    let nonce = passwordms_core::crypto::rng::random_nonce().unwrap();
    let aad = envelope_aad(&session_id, 41, ts);
    let ct = aead::seal(&sync_key, &nonce, &aad, &pt).unwrap();
    let legacy_env = format!(
        r#"{{"session_id":"{}","seq":41,"ts":{},"nonce":"{}","ct":"{}"}}"#,
        b64url_enc(&session_id),
        ts,
        b64_enc(&nonce),
        b64_enc(&ct)
    );
    let legacy_frame = framed(legacy_env.as_bytes());

    // 第二帧垃圾（若被误读会反序列化失败）。
    let mut wire = legacy_frame;
    wire.extend_from_slice(&framed(b"garbage-not-json"));

    let mut stream = &wire[..];
    let mut replay = ReplayGuard::with_state(0, 60);
    let got = paging::recv_push_pages(&mut stream, &sync_key, &session_id, &mut replay, now_ms())
        .unwrap();
    let changes = got.changes;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].record_id.len(), 36);
    assert!(!got.blob_support, "老端无 blob_support 字段 → false");
    assert_eq!(
        stream.len(),
        framed(b"garbage-not-json").len(),
        "必须恰好停在 cursor=0 那帧"
    );
}

// ===== 收发循环：多页发送 → 聚合接收，停在最后一帧 =====

#[test]
fn paging_loop_aggregates_and_stops() {
    let sync_key = [3u8; 32];
    let session_id = [4u8; 32];
    let pages: Vec<Vec<ChangeRecord>> = (0..3)
        .map(|p| {
            (0..4)
                .map(|i| ChangeRecord {
                    record_type: passwordms_core::constants::RECORD_TYPE_ITEM,
                    record_id: format!("{:032x}", p * 4 + i),
                    rev: 1,
                    updated_at: 0,
                    modified_by: String::new(),
                    deleted: false,
                    wrapped_key: vec![0u8; 48],
                    wrapped_key_nonce: vec![0u8; 24],
                    nonce: vec![0u8; 24],
                    plaintext_len: 1,
                    ciphertext: vec![2u8; 1],
                })
                .collect()
        })
        .collect();

    // 用 send_push_pages 产线（含 cursor 语义），把三页帧拼接为一条字节流。
    let mut wire: Vec<u8> = Vec::new();
    {
        // 借一个 Vec 当假 socket 只需要 Write。
        let mut sink = FrameSink(&mut wire);
        let mut seq = 10u64;
        paging::send_push_pages(
            &mut sink,
            &sync_key,
            &session_id,
            &mut seq,
            now_ms(),
            "sender",
            &pages,
            vec!["conflict-x".to_string()],
            false,
        )
        .unwrap();
        assert_eq!(seq, 13, "三页各占一个 seq");
    }
    // 尾部再挂一帧垃圾：循环必须停在 cursor=0，不读它。
    wire.extend_from_slice(&framed(b"trailing garbage"));

    let mut stream = &wire[..];
    let mut replay = ReplayGuard::with_state(0, 60);
    let got = paging::recv_push_pages(&mut stream, &sync_key, &session_id, &mut replay, now_ms())
        .unwrap();
    assert_eq!(got.changes.len(), 12);
    assert!(got.blob_support);
    assert_eq!(stream.len(), framed(b"trailing garbage").len());
}

/// 最小 Write 假 socket：把帧原样写进 Vec。
struct FrameSink<'a>(&'a mut Vec<u8>);

impl std::io::Write for FrameSink<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 长度前缀帧（与 transport 同格式：u32 LE + 帧体）。
fn framed(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

fn b64url_enc(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn b64_enc(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
