//! D-98 回归套件：回收站「彻底删除」意图跨端传播（C 案）。
//!
//! 背景（主人 2026-10-25 实测）：手机彻底删除回收站条目后同步，电脑端回收站纹丝不动
//! ——TRASH 记录不过同步线（D-56 裁定四），purge 原本纯本端生效，只靠 30 天过期兜底。
//!
//! 修法（C 案）：`purge_item` 以回收站死亡快照为载荷、`rev+1` 重封一枚 ITEM 墓碑
//! 追加落盘（`append_tombstone_frame`，不进 live 索引）；对端 merge 的删除分支在
//! 「无活跃条目 + 持有 rev ≤ 墓碑的回收站快照」时顺带彻底删除快照
//! （`purge_trash_snapshots`）。
//!
//! 覆盖：
//! 1. **传播收敛**：软删过线 → 对端进回收站 → 发起方 purge → 同步 → 对端快照清空，
//!    第三轮 applied=0（无回声、墓碑零重发）；
//! 2. **rev 保护**：本端删了更新版本（快照 rev > 对端墓碑）→ 快照保留；
//! 3. **等 rev 竞速**：对端在 purge 到达前还原（live rev == 墓碑 rev）→ 沿用既有
//!    「等 rev 删除胜」语义软删（可从回收站再恢复，不丢数据，文档化边界）；
//! 4. **单机语义**：purge 后 `tombstone_rev` = 死亡快照 rev+1；对空对端导出含
//!    `deleted=true` 且 rev 已提升的墓碑；
//! 5. **批量**：`empty_trash` 逐条提升，全部过线；
//! 6. **对端从未见过条目**：新建→删→purge→同步，对端不复活、不报错、base 正常推进。

use std::path::{Path, PathBuf};

use passwordms_core::constants::RECORD_TYPE_ITEM;
use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client::{self, JoinerSyncResult};
use passwordms_core::sync::resync::{ResyncOutcome, ResyncServer};
use passwordms_core::sync::server::{HostSyncResult, SyncServer};

const HOST_PWD: &str = "D98#Host-Pw1";
const JOINER_PWD: &str = "D98#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d98-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_login(session: &mut Session, title: &str) -> passwordms_core::model::item::Item {
    session
        .repo_mut()
        .unwrap()
        .create_item(ItemDraft {
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
        })
        .unwrap()
}

/// 首次配对（与 sync_deletion 同款回环布局）。返回（host 结果、joiner 结果、host_device_id）。
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
        "D98Host".to_string(),
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
        client::pair(&qr, &joiner_id, "D98Joiner", 0, None, now_ms()).expect("回环配对握手失败");
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

/// 一次直连重同步（Joiner 发起）。
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
        passwordms_core::sync::resync::resync_with_peer(repo, host_device_id, now_ms(), 0).unwrap()
    };
    let host_outcome = handle
        .join()
        .unwrap()
        .expect("host 轮询失败")
        .expect("host 应收到一次连接");
    (host_outcome, joiner_outcome)
}

fn rebind(port: u16) -> ResyncServer {
    ResyncServer::start(port, "D98Host".to_string()).unwrap()
}

// ===== 1. 传播收敛：purge → 对端回收站快照清空 → 第三轮零回声 =====

#[test]
fn case1_purge_propagates_and_converges() {
    let dir = temp_dir("case1");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let item_id = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        make_login(&mut hs, "purge-me").item_id
    };
    let resync_server = ResyncServer::start(0, "D98Host".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    // 软删 → 同步 → Host 进回收站。
    joiner_session
        .repo_mut()
        .unwrap()
        .delete_item(&item_id, false)
        .unwrap();
    let (_h1o, _j1o) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert_eq!(
            hs.repo().unwrap().list_trash().len(),
            1,
            "软删先照旧进回收站"
        );
    }

    // 发起方 purge → 同步 → Host 回收站快照顺带清除。
    {
        let trash_id = joiner_session.repo().unwrap().list_trash()[0]
            .trash_id
            .clone();
        joiner_session
            .repo_mut()
            .unwrap()
            .purge_item(&trash_id)
            .unwrap();
        assert!(joiner_session.repo().unwrap().list_trash().is_empty());
    }
    let (h2, _j2) = run_resync(
        &host_vault,
        rebind(resync_port),
        &mut joiner_session,
        &host_id,
    );
    assert!(h2.applied >= 1, "purge 墓碑应过线（applied 计入）");
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert!(
            hs.repo().unwrap().list_trash().is_empty(),
            "D-98：对端 purge 应清空本端回收站快照"
        );
        assert!(hs.repo().unwrap().get_item_opt(&item_id).is_none());
    }

    // 第三轮：收敛无回声。
    let (h3, j3) = run_resync(
        &host_vault,
        rebind(resync_port),
        &mut joiner_session,
        &host_id,
    );
    assert_eq!(h3.applied, 0, "收敛后 applied=0");
    assert_eq!(j3.applied, 0, "收敛后 applied=0");
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== 2. rev 保护：本端删了更新版本 → 快照保留 =====

#[test]
fn case2_newer_local_trash_snapshot_is_kept() {
    let dir = temp_dir("case2");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let item_id = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        make_login(&mut hs, "rev-guard").item_id
    };
    let resync_server = ResyncServer::start(0, "D98Host".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    // Joiner 删 rev1 → 同步（Host 进回收站，快照 rev1）。
    joiner_session
        .repo_mut()
        .unwrap()
        .delete_item(&item_id, false)
        .unwrap();
    let (_h1o, _j1o) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    // Host 本地演进到 rev3 再删（还原 → 编辑 → 再删），**不**同步——
    // 制造「本端快照 rev3 > 对端墓碑 rev2」的形态。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo_mut().unwrap();
        let trash_id = repo.list_trash()[0].trash_id.clone();
        let restored = repo.restore_item(&trash_id).unwrap();
        assert_eq!(restored.rev, 2);
        let edited = {
            let item = repo.get_item(&item_id).unwrap();
            repo.update_item(
                &item_id,
                ItemDraft {
                    kind: {
                        let mut kind = item.kind.clone();
                        kind.set_title("rev-guard-v3".to_string());
                        kind
                    },
                    folder_id: item.folder_id.clone(),
                    favorite: item.favorite,
                },
            )
            .unwrap()
        };
        assert_eq!(edited.rev, 3);
        repo.delete_item(&item_id, false).unwrap();
        assert_eq!(repo.list_trash()[0].item_snapshot.rev, 3);
    }
    // Joiner（其快照 rev1）purge → 墓碑 rev2 过线。
    {
        let trash_id = joiner_session.repo().unwrap().list_trash()[0]
            .trash_id
            .clone();
        joiner_session
            .repo_mut()
            .unwrap()
            .purge_item(&trash_id)
            .unwrap();
    }
    let (_h2, _j2) = run_resync(
        &host_vault,
        rebind(resync_port),
        &mut joiner_session,
        &host_id,
    );
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        assert_eq!(
            repo.list_trash().len(),
            1,
            "本端快照 rev3 > 对端墓碑 rev2 → 保留"
        );
        assert_eq!(repo.list_trash()[0].item_snapshot.rev, 3);
        assert_eq!(
            repo.list_trash()[0].item_snapshot.title(),
            "rev-guard-v3",
            "保留的是本端更新的死亡快照"
        );
    }
    // 收尾收敛：Host purge 自己的 rev3 快照 → 墓碑 rev4 → 两端归零。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let trash_id = hs.repo().unwrap().list_trash()[0].trash_id.clone();
        hs.repo_mut().unwrap().purge_item(&trash_id).unwrap();
    }
    let (_h3, _j3) = run_resync(
        &host_vault,
        rebind(resync_port),
        &mut joiner_session,
        &host_id,
    );
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert!(hs.repo().unwrap().list_trash().is_empty());
    }
    assert!(joiner_session.repo().unwrap().list_trash().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== 3. 等 rev 竞速：对端在 purge 到达前还原 → 等 rev 删除胜（软删可恢复） =====

#[test]
fn case3_restore_before_purge_equal_rev_delete_wins() {
    let dir = temp_dir("case3");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let item_id = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        make_login(&mut hs, "race-item").item_id
    };
    let resync_server = ResyncServer::start(0, "D98Host".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    joiner_session
        .repo_mut()
        .unwrap()
        .delete_item(&item_id, false)
        .unwrap();
    let (_h1o, _j1o) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);

    // Host 还原（rev2）但**不**同步；Joiner purge（墓碑 rev2）。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let trash_id = hs.repo().unwrap().list_trash()[0].trash_id.clone();
        let restored = hs.repo_mut().unwrap().restore_item(&trash_id).unwrap();
        assert_eq!(restored.rev, 2);
    }
    {
        let trash_id = joiner_session.repo().unwrap().list_trash()[0]
            .trash_id
            .clone();
        joiner_session
            .repo_mut()
            .unwrap()
            .purge_item(&trash_id)
            .unwrap();
    }
    let (_h2, _j2) = run_resync(
        &host_vault,
        rebind(resync_port),
        &mut joiner_session,
        &host_id,
    );
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        assert!(
            repo.get_item_opt(&item_id).is_none(),
            "等 rev 删除胜（既有语义）：还原版被软删"
        );
        assert_eq!(
            repo.list_trash().len(),
            1,
            "软删而非物理删——数据可从回收站再恢复，不丢"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== 4. 单机语义：tombstone_rev 提升 + 对空对端导出 rev+1 墓碑 =====

#[test]
fn case4_purge_bumps_tombstone_rev_in_export() {
    let dir = temp_dir("case4");
    let vault = dir.join("w.vault");
    let item_id = {
        let mut s = Session::create(&vault, HOST_PWD, true).unwrap();
        let item = make_login(&mut s, "solo-item");
        s.repo_mut()
            .unwrap()
            .delete_item(&item.item_id, false)
            .unwrap();
        item.item_id
    };
    let mut s = Session::unlock(&vault, HOST_PWD).unwrap();
    let repo = s.repo().unwrap();
    assert_eq!(
        repo.tombstone_rev(RECORD_TYPE_ITEM, &item_id),
        Some(1),
        "软删后墓碑 rev1"
    );
    let empty = repo.sync_state("qa-peer");
    let first = repo.export_changes(&empty);
    assert_eq!(first.len(), 1);
    assert!(first[0].deleted);
    assert_eq!(first[0].rev, 1);
    let trash_id = s.repo().unwrap().list_trash()[0].trash_id.clone();
    s.repo_mut().unwrap().purge_item(&trash_id).unwrap();
    let repo = s.repo().unwrap();
    assert_eq!(
        repo.tombstone_rev(RECORD_TYPE_ITEM, &item_id),
        Some(2),
        "purge 后墓碑 rev+1（死亡快照载荷重封）"
    );
    let empty = repo.sync_state("qa-peer");
    let changes = repo.export_changes(&empty);
    assert_eq!(
        changes.len(),
        1,
        "旧 rev1 墓碑与新 rev2 墓碑同 id → 只导最高"
    );
    assert!(changes[0].deleted);
    assert_eq!(changes[0].rev, 2);
    // 载荷仍可解出死亡快照（对端冲突箱/审计依赖）。
    let snapshot = repo.change_to_item(&changes[0]).unwrap();
    assert_eq!(snapshot.item_id, item_id);
    assert_eq!(snapshot.rev, 2, "载荷内 rev 同步提升（头尾一致）");
    assert_eq!(snapshot.title(), "solo-item");
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== 5. 批量：empty_trash 逐条提升、全部过线 =====

#[test]
fn case5_empty_trash_propagates_every_item() {
    let dir = temp_dir("case5");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let (id_a, id_b, id_c) = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        (
            make_login(&mut hs, "batch-a").item_id,
            make_login(&mut hs, "batch-b").item_id,
            make_login(&mut hs, "batch-c").item_id,
        )
    };
    let resync_server = ResyncServer::start(0, "D98Host".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    {
        let repo = joiner_session.repo_mut().unwrap();
        repo.delete_item(&id_a, false).unwrap();
        repo.delete_item(&id_b, false).unwrap();
        repo.delete_item(&id_c, false).unwrap();
    }
    let (_h1o, _j1o) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert_eq!(hs.repo().unwrap().list_trash().len(), 3);
    }
    joiner_session.repo_mut().unwrap().empty_trash().unwrap();
    assert!(joiner_session.repo().unwrap().list_trash().is_empty());
    let (_h2, _j2) = run_resync(
        &host_vault,
        rebind(resync_port),
        &mut joiner_session,
        &host_id,
    );
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert!(
            hs.repo().unwrap().list_trash().is_empty(),
            "empty_trash 三条墓碑全部过线"
        );
        assert!(hs.repo().unwrap().get_item_opt(&id_a).is_none());
        assert!(hs.repo().unwrap().get_item_opt(&id_b).is_none());
        assert!(hs.repo().unwrap().get_item_opt(&id_c).is_none());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== 6. 对端从未见过条目：新建→删→purge→同步，不复活、不报错 =====

#[test]
fn case6_purge_of_item_peer_never_saw() {
    let dir = temp_dir("case6");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let item_id = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        make_login(&mut hs, "seed").item_id
    };
    let resync_server = ResyncServer::start(0, "D98Host".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    // 配对后 Joiner 新建（Host 未见过）→ 删 → purge → 一次性同步三变更（墓碑形态）。
    let fresh = make_login(&mut joiner_session, "never-seen");
    joiner_session
        .repo_mut()
        .unwrap()
        .delete_item(&fresh.item_id, false)
        .unwrap();
    let trash_id = joiner_session.repo().unwrap().list_trash()[0]
        .trash_id
        .clone();
    joiner_session
        .repo_mut()
        .unwrap()
        .purge_item(&trash_id)
        .unwrap();
    let (h2, _j2) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    assert!(h2.applied >= 1);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        assert!(
            repo.get_item_opt(&fresh.item_id).is_none(),
            "对端不得复活从未见过的条目（复活抑制）"
        );
        assert!(repo.list_trash().is_empty());
        assert!(repo.get_item_opt(&item_id).is_some(), "种子条目不受影响");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
