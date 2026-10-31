//! D-150 回归：一次**成功的**重同步必须让两端「上次同步」时间落地。
//!
//! 缺陷现象（主人真机 2026-10-31）：同步过很多次，手机与电脑的「已配对设备」里
//! 仍然显示「上次同步：从未」，且电脑「待同步变更」恒为 58。
//!
//! 本文件只钉一件事，且**必须真跑 socket**：
//! ① 成功重同步后，发起方（Joiner）与响应方（Host）各自设备记录里的 `last_sync_at`
//!    都必须从 `None` 变成 `Some`；
//! ② 该值必须**跨解锁可读**（写进容器，不是内存假象）；
//! ③ 只配对的记录（`last_sync_at == None`）在同步后**不许**被其它路径覆写回 None。
//!
//! 之所以要独立成文件：仓库里此前**零测试**触碰 `touch_device_sync` 的写入结果——
//! `resync_addr_heal.rs` 只断言 `update_paired_addr` 不动该字段，等于反过来把
//! 「谁设它」这件事完全留白。

use std::path::{Path, PathBuf};

use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::resync::{self, ResyncOutcome, ResyncServer};
use passwordms_core::sync::server::SyncServer;

const HOST_PWD: &str = "Sync#Host-Pw1";
const JOINER_PWD: &str = "Sync#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-lastsync-{tag}-{}", uuid::Uuid::new_v4()));
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

fn pair_first(host_vault: &Path, resync_port: u16, joiner_vault: &Path) -> (String, String) {
    let host_device_id = {
        let s = Session::unlock(host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };
    let identity = HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_device_id.clone(),
        "SyncHost".to_string(),
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
        client::pair(&qr, &joiner_id, "SyncJoiner", 0, None, now_ms()).expect("回环配对握手失败");
    joiner_session
        .adopt_vault_key(joiner.host_vault_key())
        .unwrap();
    {
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        // D-147 语义：配对只建立关系（pair_only），这里显式标注只配对。
        joiner.mark_pair_only();
        joiner.push_and_sync(repo, now_ms()).unwrap();
    }
    drop(joiner);
    handle.join().unwrap().expect("host 配对失败");
    (host_device_id, joiner_id)
}

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

fn last_sync_of(vault: &Path, pwd: &str, device_id: &str) -> Option<i64> {
    let s = Session::unlock(vault, pwd).unwrap();
    s.repo()
        .unwrap()
        .get_paired_device(device_id)
        .expect("应有配对记录")
        .last_sync_at
}

fn paired_at_of(vault: &Path, pwd: &str, device_id: &str) -> i64 {
    let s = Session::unlock(vault, pwd).unwrap();
    s.repo()
        .unwrap()
        .get_paired_device(device_id)
        .expect("应有配对记录")
        .paired_at
}

/// 同一对保险库**再配一次**（两端 device_id 都不变）。
fn pair_again(host_vault: &Path, resync_port: u16, joiner_vault: &Path) -> (String, String) {
    let host_device_id = {
        let s = Session::unlock(host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };
    let identity = HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_device_id.clone(),
        "SyncHost".to_string(),
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

    let mut joiner_session = Session::unlock(joiner_vault, JOINER_PWD).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();
    let mut joiner =
        client::pair(&qr, &joiner_id, "SyncJoiner", 0, None, now_ms()).expect("重配对握手失败");
    // D-47/D-62：同组重配对豁免「非空即拒」——本机 VK 已与对端一致时不再采纳
    // （`adopt_vault_key` 对非空库返回 `VkAdoptRequiresEmpty`，这是保护而非缺陷）。
    let same_key = {
        let repo = joiner_session.repo().unwrap();
        repo.matches_vault_key(joiner.host_vault_key())
    };
    if !same_key {
        joiner_session
            .adopt_vault_key(joiner.host_vault_key())
            .unwrap();
    }
    {
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        joiner.mark_pair_only();
        joiner.push_and_sync(repo, now_ms()).unwrap();
    }
    drop(joiner);
    handle.join().unwrap().expect("host 重配对失败");
    (host_device_id, joiner_id)
}

/// ① 成功重同步 → 两端 `last_sync_at` 都必须落地。
#[test]
fn successful_resync_stamps_last_sync_at_on_both_ends() {
    let dir = temp_dir("both");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        hs.repo_mut()
            .unwrap()
            .create_item(login_draft("stamp-item"))
            .unwrap();
    }

    let host_resync = ResyncServer::start(0, "SyncHost".to_string()).unwrap();
    let (host_id, joiner_id) = pair_first(&host_vault, host_resync.port(), &joiner_vault);

    // 配对只是建立关系：此刻两端都应是「从未」。
    assert_eq!(
        last_sync_of(&host_vault, HOST_PWD, &joiner_id),
        None,
        "只配对（pair_only）不得写同步时间"
    );

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let (_host_out, joiner_out) =
        resync_once(&host_vault, host_resync, &mut joiner_session, &host_id, 0);
    assert!(joiner_out.applied >= 1, "Joiner 应至少应用 1 条");

    // ② 跨解锁可读：Host 侧重新解锁再读（写进了容器，不是内存假象）。
    assert!(
        last_sync_of(&host_vault, HOST_PWD, &joiner_id).is_some(),
        "Host 侧：成功重同步后 last_sync_at 必须落地"
    );
    drop(joiner_session);
    assert!(
        last_sync_of(&joiner_vault, JOINER_PWD, &host_id).is_some(),
        "Joiner 侧：成功重同步后 last_sync_at 必须落地"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// ③ 同一台设备重新配对：`last_sync_at` 必须保留，`paired_at` 必须刷新。
///
/// 原实现两处 (`client::persist_host_device` / `server::persist_device`) 都无条件写
/// `last_sync_at: None`，于是「重配一下就变成从未同步过」——而重配对恰恰是排查同步
/// 问题时的常规动作，用户重配完再一看，比修之前更慌。
#[test]
fn repair_keeps_last_sync_at_but_refreshes_paired_at() {
    let dir = temp_dir("repair");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        hs.repo_mut()
            .unwrap()
            .create_item(login_draft("repair-item"))
            .unwrap();
    }

    let host_resync = ResyncServer::start(0, "SyncHost".to_string()).unwrap();
    let host_port = host_resync.port();
    let (host_id, joiner_id) = pair_first(&host_vault, host_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    resync_once(&host_vault, host_resync, &mut joiner_session, &host_id, 0);
    drop(joiner_session);

    let host_stamp = last_sync_of(&host_vault, HOST_PWD, &joiner_id);
    let joiner_stamp = last_sync_of(&joiner_vault, JOINER_PWD, &host_id);
    assert!(host_stamp.is_some(), "同步后 Host 侧应有时间戳");
    assert!(joiner_stamp.is_some(), "同步后 Joiner 侧应有时间戳");
    let old_paired_at = paired_at_of(&host_vault, HOST_PWD, &joiner_id);

    // 同 device_id 再配一次（模拟「同步不通 → 重配对」这个常规动作）。
    pair_again(&host_vault, host_port, &joiner_vault);

    assert_eq!(
        last_sync_of(&host_vault, HOST_PWD, &joiner_id),
        host_stamp,
        "Host 侧：重配对不得把「上次同步」清回从未"
    );
    assert_eq!(
        last_sync_of(&joiner_vault, JOINER_PWD, &host_id),
        joiner_stamp,
        "Joiner 侧：重配对不得把「上次同步」清回从未"
    );
    assert!(
        paired_at_of(&host_vault, HOST_PWD, &joiner_id) >= old_paired_at,
        "配对时间应反映本次配对（只有它该刷新）"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// ③ 待同步计数：一条已同步的设备不得再计入（回归 58 那条）。
#[test]
fn pending_changes_drops_to_zero_after_sync() {
    let dir = temp_dir("pending");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        let repo = hs.repo_mut().unwrap();
        for i in 0..5 {
            repo.create_item(login_draft(&format!("p{i}"))).unwrap();
        }
    }

    let host_resync = ResyncServer::start(0, "SyncHost".to_string()).unwrap();
    let (host_id, joiner_id) = pair_first(&host_vault, host_resync.port(), &joiner_vault);

    let pending_before = {
        let s = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = s.repo().unwrap();
        repo.export_changes(&repo.sync_state(&joiner_id)).len()
    };
    assert!(pending_before >= 5, "同步前应有 5 条以上待发");

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    resync_once(&host_vault, host_resync, &mut joiner_session, &host_id, 0);

    let pending_after = {
        let s = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = s.repo().unwrap();
        repo.export_changes(&repo.sync_state(&joiner_id)).len()
    };
    assert_eq!(pending_after, 0, "成功同步后该设备的待发必须归零");

    let _ = std::fs::remove_dir_all(&dir);
}
