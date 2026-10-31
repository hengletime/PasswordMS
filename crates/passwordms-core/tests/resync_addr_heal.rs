//! 对端地址自愈回归（v1.2.4 根因 B）。
//!
//! 缺陷：`peer_ip` / `peer_port` 此前**只在配对那一刻**由 Host 记录一次
//! （`sync/server.rs::handle_stream` 的 `persist_device`）。手机换 IP（DHCP 续租）、
//! 或本机监听端口 rebind（`resync.port` 被占用 → `ensure_resync_listener` fallback 到
//! 随机端口）之后，Host 的配对记录永久过期 → 电脑点「立即同步」永远连不上，
//! 只能重新配对。
//!
//! 修法：`ResyncHello` 增加 `sync_port`（本机监听端口），Host 在**校验通过且同步成功**
//! 之后用「accept 出的真实来源 IP + 对端自报端口」刷新配对记录。
//!
//! 本文件守三条：
//! ① 地址被改坏 → 一次成功同步后自动修回，**不需要重新配对**；
//! ② 老客户端（`sync_port == 0`，不发该字段）→ `peer_ip` 修复但 `peer_port` **保留原值**
//!    （不得被 0 清空）；
//! ③ 失败/被拒的连接不得篡改地址（未配对设备的连接不写任何东西）。

use std::path::{Path, PathBuf};

use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::resync::{self, ResyncOutcome, ResyncServer};
use passwordms_core::sync::server::SyncServer;

const HOST_PWD: &str = "Addr#Host-Pw1";
const JOINER_PWD: &str = "Addr#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-addr-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn login_draft(title: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: None,
            username: None,
            password: None,
            notes: None,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
    }
}

/// 配对（Host 侧带 resync 端口 `resync_port`），返回 `(host_device_id, joiner_device_id)`。
fn pair_first(host_vault: &Path, resync_port: u16, joiner_vault: &Path) -> (String, String) {
    let host_device_id = {
        let s = Session::unlock(host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };
    let identity = HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_device_id.clone(),
        "AddrHost".to_string(),
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
        client::pair(&qr, &joiner_id, "AddrJoiner", 0, None, now_ms()).expect("回环配对握手失败");
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
    let _ = result.expect("joiner 配对同步失败");
    let _ = host_result;
    (host_device_id, joiner_id)
}

/// 一次重同步：`local_port` 为发起方（Joiner）自报的监听端口。
fn resync_once(
    host_vault: &Path,
    resync_server: ResyncServer,
    joiner_session: &mut Session,
    host_device_id: &str,
    local_port: u16,
) -> (ResyncOutcome, ResyncOutcome) {
    let hv = host_vault.to_path_buf();
    let handle = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&hv, HOST_PWD).unwrap();
        let mut resync_server = resync_server;
        loop {
            let repo = host_session.repo_mut().unwrap();
            match resync_server.poll_once(repo, now_ms()) {
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Ok(Some(outcome)) => return outcome,
                Err(e) => panic!("host 重同步失败: {e:?}"),
            }
        }
    });
    let joiner_outcome = {
        let repo = joiner_session.repo_mut().unwrap();
        resync::resync_with_peer(repo, host_device_id, now_ms(), local_port).unwrap()
    };
    (handle.join().unwrap(), joiner_outcome)
}

/// 读取 Host 侧记录的某对端地址。
fn host_recorded_addr(vault: &Path, device_id: &str) -> (Option<String>, Option<u16>) {
    let s = Session::unlock(vault, HOST_PWD).unwrap();
    let dev = s
        .repo()
        .unwrap()
        .get_paired_device(device_id)
        .expect("Host 应保留该配对记录");
    (dev.peer_ip, dev.peer_port)
}

/// 直接把 Host 侧记录改坏（模拟「手机换了 IP / 端口 rebind 之后」的过期状态）。
fn corrupt_host_record(vault: &Path, device_id: &str, ip: &str, port: Option<u16>) {
    let mut s = Session::unlock(vault, HOST_PWD).unwrap();
    let repo = s.repo_mut().unwrap();
    let mut dev = repo.get_paired_device(device_id).expect("应有配对记录");
    dev.peer_ip = Some(ip.to_string());
    dev.peer_port = port;
    repo.upsert_paired_device(dev).unwrap();
}

// ===== ① 地址被改坏 → 一次成功同步后自愈 =====

#[test]
fn resync_heals_stale_peer_address_without_repairing() {
    let dir = temp_dir("heal");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    host_session
        .repo_mut()
        .unwrap()
        .create_item(login_draft("addr-item"))
        .unwrap();
    drop(host_session);

    let host_resync = ResyncServer::start(0, "AddrHost".to_string()).unwrap();
    let (host_id, joiner_id) = pair_first(&host_vault, host_resync.port(), &joiner_vault);

    // Joiner 自己的持久监听端口（本机实际端口，随 HELLO 通告）。
    let joiner_listener = ResyncServer::start(0, "AddrJoiner".to_string()).unwrap();
    let joiner_port = joiner_listener.port();
    assert_ne!(joiner_port, 0);

    // 把 Host 侧记录改坏：不可路由 IP + 端口 1。
    corrupt_host_record(&host_vault, &joiner_id, "10.255.255.1", Some(1));
    assert_eq!(
        host_recorded_addr(&host_vault, &joiner_id),
        (Some("10.255.255.1".to_string()), Some(1)),
        "前置：记录已过期"
    );

    // 配对后 Host 再建一条，让本轮同步确实有「新东西」可应用（applied ≥ 1 才有意义）。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        hs.repo_mut()
            .unwrap()
            .create_item(login_draft("after-pair"))
            .unwrap();
    }

    // 一次正常重同步（Joiner 通告自己的监听端口）。
    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let (_host_out, joiner_out) = resync_once(
        &host_vault,
        host_resync,
        &mut joiner_session,
        &host_id,
        joiner_port,
    );
    assert!(
        joiner_out.applied >= 1,
        "Joiner 应至少应用 1 条（配对后新增的条目）"
    );

    // 断言：Host 侧记录已自动修回真实值，无需重新配对。
    assert_eq!(
        host_recorded_addr(&host_vault, &joiner_id),
        (Some("127.0.0.1".to_string()), Some(joiner_port)),
        "一次成功同步后对端地址应自愈（IP=本次真实来源，端口=对端自报监听端口）"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ② 老客户端（sync_port == 0）不得把端口清空 =====

#[test]
fn legacy_client_without_sync_port_keeps_recorded_port() {
    let dir = temp_dir("legacy");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    host_session
        .repo_mut()
        .unwrap()
        .create_item(login_draft("legacy-item"))
        .unwrap();
    drop(host_session);

    let host_resync = ResyncServer::start(0, "AddrHost".to_string()).unwrap();
    let (host_id, joiner_id) = pair_first(&host_vault, host_resync.port(), &joiner_vault);

    // 记录里有一个「此前可用」的端口，但 IP 过期。
    corrupt_host_record(&host_vault, &joiner_id, "10.255.255.1", Some(4242));

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let (_host_out, _joiner_out) =
        resync_once(&host_vault, host_resync, &mut joiner_session, &host_id, 0);

    assert_eq!(
        host_recorded_addr(&host_vault, &joiner_id),
        (Some("127.0.0.1".to_string()), Some(4242)),
        "sync_port == 0（老客户端不发该字段）时必须保留原 peer_port，只修 IP"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ③ `update_paired_addr` 自身的三条契约 =====

#[test]
fn update_paired_addr_contract() {
    let dir = temp_dir("contract");
    let vault = dir.join("vault.vault");
    let mut session = Session::create(&vault, HOST_PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();

    // 未知设备 → NotPaired（不得静默创建一条空记录）。
    assert!(matches!(
        repo.update_paired_addr("no-such-device", "127.0.0.1", 5000),
        Err(passwordms_core::error::CoreError::NotPaired)
    ));
    assert!(repo.get_paired_device("no-such-device").is_none());

    // 构造一条配对记录（device_id 必须是 UUID 形态：`record_id_to_bytes` 会校验）。
    let peer_id = uuid::Uuid::new_v4().to_string();
    let device = passwordms_core::model::sync_state::PairedDevice {
        device_id: peer_id.clone(),
        device_name: "peer".to_string(),
        fingerprint: "fp".to_string(),
        sync_key_wrapped: passwordms_core::model::vault::WrappedKey {
            nonce: vec![0u8; 12],
            ct: vec![0u8; 32],
        },
        paired_at: 111,
        last_sync_at: None,
        peer_ip: Some("10.0.0.9".to_string()),
        peer_port: Some(7000),
    };
    repo.upsert_paired_device(device.clone()).unwrap();

    // (a) sync_port == 0 → 只改 IP，端口保留。
    repo.update_paired_addr(&peer_id, "192.168.1.5", 0).unwrap();
    let after = repo.get_paired_device(&peer_id).unwrap();
    assert_eq!(after.peer_ip.as_deref(), Some("192.168.1.5"));
    assert_eq!(after.peer_port, Some(7000), "sync_port=0 不得清空已有端口");

    // (b) sync_port != 0 → 两个字段都刷新，且其它字段一字不动。
    repo.update_paired_addr(&peer_id, "192.168.1.6", 7001)
        .unwrap();
    let after = repo.get_paired_device(&peer_id).unwrap();
    assert_eq!(after.peer_ip.as_deref(), Some("192.168.1.6"));
    assert_eq!(after.peer_port, Some(7001));
    assert_eq!(after.paired_at, device.paired_at, "不得改动 paired_at");
    assert_eq!(
        after.fingerprint, device.fingerprint,
        "不得改动 fingerprint"
    );
    assert_eq!(after.last_sync_at, None, "本方法不承担记录同步时间的职责");
    assert_eq!(
        after.sync_key_wrapped, device.sync_key_wrapped,
        "不得改动共享密钥"
    );

    // (c) 值未变化 → 幂等（重启后仍一致）。
    repo.update_paired_addr(&peer_id, "192.168.1.6", 7001)
        .unwrap();
    drop(session);
    let reopened = Session::unlock(&vault, HOST_PWD).unwrap();
    let dev = reopened
        .repo()
        .unwrap()
        .get_paired_device(&peer_id)
        .unwrap();
    assert_eq!(
        (dev.peer_ip.as_deref(), dev.peer_port),
        (Some("192.168.1.6"), Some(7001))
    );

    let _ = std::fs::remove_dir_all(&dir);
}
