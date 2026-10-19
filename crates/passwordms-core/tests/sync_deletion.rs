//! D-56 删除传播回归套件（架构文档 §3.13，验收 8 条）。
//!
//! 覆盖：①软删过线→对端进回收站→还原→两端 rev 收敛 ②删除-复活拉锯→冲突箱
//! （死亡快照）+ 两种仲裁收敛 ③peer base 免重发过滤 ④分组删除两形态且对端无自发级联
//! ⑤blob 连带（软删保留/purge 清且不过线）⑥载荷类型白名单 ⑦墓碑密文性（A-01）+
//! 损坏墓碑跳过不中断 + 信封重放被拒（A-08）⑧老格式墓碑帧零迁移。
//!
//! **实现补注（与 §3.13.2 伪码的两处细化，均经用例实证必要）**：
//! 1. merge 幂等判重按 base.hash 细化：`hash == [0;32]` 才代表「该墓碑已应用」——
//!    常规流「同步 rev1 → 删 rev1」中 base.rev == 墓碑 rev，伪码字面的 `>=` 会把
//!    首次删除误判为重放（①② 无法通过）；导出侧 peer 过滤同理。
//! 2. 发送侧 `note_sent_tombstones`：导出墓碑发送成功后推进本端 base，实现③的
//!    「第二次同步墓碑 ChangeRecord = 0」（协议无回执，接收侧 base 无法回传）。
//!
//! 另：merge live 分支加「复活抑制」（本端墓碑 rev ≥ 变更 rev 时不 put）——否则
//! ②「采用远端」仲裁后的收敛会被对端陈旧 live 变更回滚。
//!
//! 测试布局约定：**删除方为发起方（joiner）**——响应方在回包前才合并发起方 push，
//! 若存活方先发起，其陈旧 live 变更会先于墓碑到达（LWW 到达序语义，见遗留风险备案）。

use std::path::{Path, PathBuf};

use passwordms_core::constants::{
    FLAG_TOMBSTONE, RECORD_TYPE_FOLDER, RECORD_TYPE_ITEM, SYNC_TIME_WINDOW_SECS,
};
use passwordms_core::crypto::rng;
use passwordms_core::error::CoreError;
use passwordms_core::model::conflict::{ConflictChoice, ConflictEntry};
use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::model::sync_state::{DeviceSyncState, PairedDevice};
use passwordms_core::model::vault::WrappedKey;
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;
use passwordms_core::store::frame::Frame;
use passwordms_core::store::repository::{FolderDeleteStrategy, ItemDraft, Repository};
use passwordms_core::sync::client::{self, JoinerSyncResult};
use passwordms_core::sync::protocol::{seal_envelope, ChangeRecord, SyncKind, SyncPayload};
use passwordms_core::sync::replay::ReplayGuard;
use passwordms_core::sync::resync::{self, ResyncOutcome, ResyncServer};
use passwordms_core::sync::server::{HostSyncResult, SyncServer};

const HOST_PWD: &str = "Del#Host-Pw1";
const JOINER_PWD: &str = "Del#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-del-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 新建登录条目（可指定分组）。
fn make_login(session: &mut Session, title: &str, folder_id: Option<String>) -> Item {
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
            folder_id,
            favorite: false,
        })
        .unwrap()
}

/// 更新条目内容（rev+1）。
fn touch_item(session: &mut Session, item_id: &str, new_title: &str) -> Item {
    let repo = session.repo_mut().unwrap();
    let item = repo.get_item(item_id).unwrap();
    repo.update_item(
        item_id,
        ItemDraft {
            kind: {
                let mut kind = item.kind.clone();
                kind.set_title(new_title.to_string());
                kind
            },
            folder_id: item.folder_id.clone(),
            favorite: item.favorite,
        },
    )
    .unwrap()
}

/// 首次配对：Host 预置数据，Joiner 全新建库 + 采纳 VK + 双向同步。
/// 返回（host 结果、joiner 结果、host_device_id）。
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
        "DelHost".to_string(),
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
    let mut joiner = client::pair(&qr, &joiner_id, "DelJoiner", 0, now_ms()).expect("回环配对握手失败");
    joiner_session.adopt_vault_key(joiner.host_vault_key()).unwrap();
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

/// 一次直连重同步（Joiner 发起）；Host 侧 ResyncServer 由调用方持有并移入本函数。
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
        resync::resync_with_peer(repo, host_device_id, now_ms()).unwrap()
    };
    let host_outcome = handle
        .join()
        .unwrap()
        .expect("host 轮询失败")
        .expect("host 应收到一次连接");
    (host_outcome, joiner_outcome)
}

/// 某侧导出载荷中的墓碑 id 清单（对 peer 过滤后口径）。
fn tombstone_ids(repo: &Repository, peer_device_id: &str) -> Vec<String> {
    let state: DeviceSyncState = repo.sync_state(peer_device_id);
    repo.export_changes(&state)
        .iter()
        .filter(|c| c.deleted)
        .map(|c| c.record_id.clone())
        .collect()
}

/// joiner 侧设备 id（host 的已配对设备表中非 host 的那条）。
fn joiner_device_id(host_vault: &Path, host_device_id: &str) -> String {
    let s = Session::unlock(host_vault, HOST_PWD).unwrap();
    s.repo()
        .unwrap()
        .list_paired_devices()
        .into_iter()
        .map(|d| d.device_id)
        .find(|id| id != host_device_id)
        .expect("host 应已配对 joiner")
}

/// 在同一端口重绑重同步监听器（上一实例随同步线程结束而释放；joiner 一直拨配对时
/// 通告的端口，逐次同步不得换端口）。
fn rebind(port: u16) -> ResyncServer {
    ResyncServer::start(port, "DelHost".to_string()).unwrap()
}

// ===== ① 软删过线 → 对端进回收站；还原 → 再同步 → 两端存活 rev 一致 =====

#[test]
fn case1_remote_delete_lands_in_trash_and_restore_converges() {
    let dir = temp_dir("case1");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let item = make_login(&mut host_session, "site-a", None);
    let item_id = item.item_id;
    drop(host_session);

    let resync_server = ResyncServer::start(0, "DelHost".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    // Joiner（发起方）软删。
    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    joiner_session
        .repo_mut()
        .unwrap()
        .delete_item(&item_id, false)
        .unwrap();
    assert!(joiner_session.repo().unwrap().get_item_opt(&item_id).is_none());

    // 同步 → Host 走软删（进回收站，非物理删）。
    let (host_out, _j1o) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    assert!(host_out.applied >= 1, "Host 应应用删除变更");
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        assert!(repo.get_item_opt(&item_id).is_none(), "条目应离开活跃表");
        assert_eq!(repo.list_trash().len(), 1, "远端删除必须进回收站（O-05）");
        assert_eq!(repo.list_trash()[0].item_snapshot.item_id, item_id);
    }

    // Host 还原 → rev+1 → 再同步 → 两端存活且 rev 一致。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let trash_id = hs.repo().unwrap().list_trash()[0].trash_id.clone();
        let restored = hs.repo_mut().unwrap().restore_item(&trash_id).unwrap();
        assert_eq!(restored.rev, 2);
    }
    let server2 = rebind(resync_port);
    let (_h2, joiner_out2) = run_resync(&host_vault, server2, &mut joiner_session, &host_id);
    assert!(joiner_out2.applied >= 1, "还原变更应过线");

    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let host_rev = hs.repo().unwrap().get_item(&item_id).unwrap().rev;
        let joiner_rev = joiner_session.repo().unwrap().get_item(&item_id).unwrap().rev;
        assert_eq!(host_rev, 2);
        assert_eq!(joiner_rev, 2, "两端存活 rev 一致");
        assert!(joiner_session.repo().unwrap().get_item_opt(&item_id).is_some());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ② 删除-复活拉锯 → 冲突箱（死亡快照）；两种仲裁均收敛 =====

#[test]
fn case2_delete_modify_tug_goes_to_conflict_box_and_converges() {
    // ---- 子场景 A：仲裁「保留本机」→ 两端复活收敛 ----
    let dir = temp_dir("case2a");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let item_id = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        make_login(&mut hs, "tug-item", None).item_id
    };
    let resync_server = ResyncServer::start(0, "DelHost".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    joiner_session.repo_mut().unwrap().delete_item(&item_id, false).unwrap();
    // Host 未同步前修改 → rev2。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        touch_item(&mut hs, &item_id, "tug-item-modified");
    }

    let (host_out, _jo) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    assert!(
        host_out.conflicts.iter().any(|c| c == &item_id),
        "删除-修改拉锯应进冲突箱：{:?}",
        host_out.conflicts
    );
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        let conflicts = repo.list_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].remote_snapshot.rev, 1, "remote_snapshot = 死亡前快照");
        assert!(conflicts[0].remote_deleted, "删除冲突标记");
        assert_eq!(conflicts[0].local_snapshot.rev, 2);
        assert!(repo.get_item_opt(&item_id).is_some(), "保留本机为正式条目");
    }
    // 仲裁「保留本机」。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let cid = hs.repo().unwrap().list_conflicts()[0].conflict_id.clone();
        hs.repo_mut().unwrap().resolve_conflict(&cid, ConflictChoice::KeepLocal).unwrap();
    }
    let server2 = rebind(resync_port);
    let (_h2, _j2) = run_resync(&host_vault, server2, &mut joiner_session, &host_id);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert_eq!(hs.repo().unwrap().get_item(&item_id).unwrap().rev, 2);
        assert_eq!(
            joiner_session.repo().unwrap().get_item(&item_id).unwrap().rev,
            2,
            "「保留本机」→ 两端复活收敛"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);

    // ---- 子场景 B：仲裁「采用远端」→ 两端软删收敛 ----
    let dir = temp_dir("case2b");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let item_id = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        make_login(&mut hs, "tug-item-b", None).item_id
    };
    let resync_server = ResyncServer::start(0, "DelHost".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    joiner_session.repo_mut().unwrap().delete_item(&item_id, false).unwrap();
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        touch_item(&mut hs, &item_id, "tug-item-b-modified");
    }
    let (_host_out, _jo) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    // 仲裁「采用远端」= 接受删除 → 软删（不复活死亡快照）。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let cid = hs.repo().unwrap().list_conflicts()[0].conflict_id.clone();
        hs.repo_mut().unwrap().resolve_conflict(&cid, ConflictChoice::TakeRemote).unwrap();
        let repo = hs.repo().unwrap();
        assert!(repo.get_item_opt(&item_id).is_none(), "采用远端删除 → 本端软删");
        assert_eq!(repo.list_trash().len(), 1);
    }
    let server2 = rebind(resync_port);
    let (_h2, _j2) = run_resync(&host_vault, server2, &mut joiner_session, &host_id);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert!(hs.repo().unwrap().get_item_opt(&item_id).is_none());
    }
    assert!(
        joiner_session.repo().unwrap().get_item_opt(&item_id).is_none(),
        "「采用远端」→ Joiner 陈旧 live 变更不得复活对端，最终软删收敛"
    );
    // 第三次同步：墓碑零重发、零重复应用。
    let server3 = rebind(resync_port);
    let (h3, j3) = run_resync(&host_vault, server3, &mut joiner_session, &host_id);
    assert_eq!(h3.applied, 0, "收敛后 applied=0");
    assert_eq!(j3.applied, 0, "收敛后 applied=0");
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ③ peer base 免重发过滤：第二次同步墓碑 ChangeRecord=0 + applied=0 =====

#[test]
fn case3_peer_base_filter_suppresses_reexport() {
    let dir = temp_dir("case3");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let item_id = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        make_login(&mut hs, "filter-item", None).item_id
    };
    let resync_server = ResyncServer::start(0, "DelHost".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);
    let joiner_id = joiner_device_id(&host_vault, &host_id);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    joiner_session.repo_mut().unwrap().delete_item(&item_id, false).unwrap();

    // 正例控制：删除后首导出含墓碑。
    assert_eq!(
        tombstone_ids(joiner_session.repo().unwrap(), &host_id),
        vec![item_id.clone()],
        "首次导出应含墓碑"
    );

    // 第一次同步：墓碑过线，Host 软删。
    let (host_out1, _j1o) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    assert!(host_out1.applied >= 1);

    // 第二次同步前：双侧导出的墓碑 ChangeRecord 均为 0（peer base 过滤生效）。
    assert!(
        tombstone_ids(joiner_session.repo().unwrap(), &host_id).is_empty(),
        "Joiner 侧发送推进后应零重发"
    );
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert!(
            tombstone_ids(hs.repo().unwrap(), &joiner_id).is_empty(),
            "Host 侧接收推进后应零重发"
        );
    }

    let server2 = rebind(resync_port);
    let (host_out2, joiner_out2) = run_resync(&host_vault, server2, &mut joiner_session, &host_id);
    assert_eq!(host_out2.applied, 0, "第二次同步 applied=0");
    assert_eq!(joiner_out2.applied, 0, "第二次同步 applied=0");
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ④ 分组删除两形态（ToTrash 级联 / ToUngrouped 壳消失）且对端无自发级联 =====

#[test]
fn case4_folder_deletion_two_strategies_no_spontaneous_cascade() {
    let dir = temp_dir("case4");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // Host 预置：g1(ToTrash 演练) + item1；g2(ToUngrouped 演练) + item2；item0 未分组。
    let (g1_id, g2_id, item0_id, item1_id, item2_id) = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        let repo = hs.repo_mut().unwrap();
        let g1 = repo.create_folder("g1".to_string(), None, None).unwrap();
        let g2 = repo.create_folder("g2".to_string(), None, None).unwrap();
        let item0 = make_login(&mut hs, "free-item", None);
        let item1 = make_login(&mut hs, "g1-item", Some(g1.folder_id.clone()));
        let item2 = make_login(&mut hs, "g2-item", Some(g2.folder_id.clone()));
        (g1.folder_id, g2.folder_id, item0.item_id, item1.item_id, item2.item_id)
    };
    let resync_server = ResyncServer::start(0, "DelHost".to_string()).unwrap();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    {
        let repo = joiner_session.repo_mut().unwrap();
        assert!(repo.get_item_opt(&item1_id).is_some());
        repo.delete_folder(&g1_id, FolderDeleteStrategy::ToTrash).unwrap();
        repo.delete_folder(&g2_id, FolderDeleteStrategy::ToUngrouped).unwrap();
    }
    let (_host_out, _joiner_out) =
        run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        // 两形态：g1 级联进回收站（靠 A 端逐条 item 墓碑过线）。
        assert!(repo.list_folders().is_empty(), "两个分组壳均应消失");
        assert!(repo.get_item_opt(&item1_id).is_none(), "ToTrash 条目离开活跃表");
        assert!(repo.list_trash().iter().any(|t| t.item_snapshot.item_id == item1_id));
        // ToUngrouped：条目存活、移出分组。
        let item2 = repo.get_item_opt(&item2_id).expect("ToUngrouped 条目应存活");
        assert!(item2.folder_id.is_none(), "应移至未分组");
        // 对端无自发级联：未分组条目与 g2 条目都不因分组删除被连带清除。
        assert!(repo.get_item_opt(&item0_id).is_some(), "未分组条目不受影响");
        assert!(repo.get_item_opt(&item2_id).is_some());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑤ blob 连带：远端软删 blob 保留、还原导出一致；purge → blob 墓碑且不过线 =====

#[test]
fn case5_attachment_soft_delete_keeps_blob_and_purge_stays_local() {
    let dir = temp_dir("case5");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // Host 建父条目挂 64KB 附件（D-60：附件元数据随父帧过线，blob 经 D-53 阶段到 Joiner）。
    let (parent_id, att_id, original) = {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        let src = dir.join("src.bin");
        let mut data = vec![0u8; 64 * 1024];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        std::fs::write(&src, &data).unwrap();
        let parent = make_login(&mut hs, "att-parent", None);
        let parent = hs
            .repo_mut()
            .unwrap()
            .add_attachment(&parent.item_id, &src, "")
            .unwrap();
        let att_id = parent.attachments[0].attachment_id.clone();
        (parent.item_id, att_id, data)
    };
    let resync_server = ResyncServer::start(0, "DelHost".to_string()).unwrap();
    let resync_port = resync_server.port();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_port, &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    assert!(joiner_session.repo().unwrap().has_blob(&att_id), "配对后 blob 应已同步");

    // Joiner 软删父条目 → Host 软删且 blob 保留（trash 快照仍引用，§3.14.4）。
    joiner_session.repo_mut().unwrap().delete_item(&parent_id, false).unwrap();
    let (_h1o, _j1o) = run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let repo = hs.repo().unwrap();
        assert!(repo.get_item_opt(&parent_id).is_none());
        assert!(repo.has_blob(&att_id), "远端软删必须保留 blob（还原需要）");
        assert_eq!(repo.list_trash().len(), 1);
    }

    // Joiner purge（彻底删其回收站快照）→ GC 清 blob，仅本地生效。
    {
        let trash_id = joiner_session.repo().unwrap().list_trash()[0].trash_id.clone();
        joiner_session.repo_mut().unwrap().purge_item(&trash_id).unwrap();
        assert!(!joiner_session.repo().unwrap().has_blob(&att_id), "purge 应清 blob");
    }
    // purge 事件不过线：TRASH 墓碑（白名单外）与 blob 墓碑（不独立导出）均不出网，
    // Host 的 blob 与回收站不受影响。
    let (_h2o, _j2o) = run_resync(&host_vault, rebind(resync_port), &mut joiner_session, &host_id);
    {
        let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        assert!(hs.repo().unwrap().has_blob(&att_id), "对端 blob 不受本端 purge 影响");
        assert_eq!(hs.repo().unwrap().list_trash().len(), 1, "purge 事件不过线");
    }

    // Host 还原 → blob 可用、导出字节一致 → 再同步 → Joiner 复活 + blob 经 D-53 阶段补齐。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let trash_id = hs.repo().unwrap().list_trash()[0].trash_id.clone();
        hs.repo_mut().unwrap().restore_item(&trash_id).unwrap();
        let dest = dir.join("host-export.bin");
        hs.repo()
            .unwrap()
            .export_attachment(&parent_id, &att_id, &dest)
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), original, "还原后导出字节一致");
    }
    let (_h3o, j3) = run_resync(&host_vault, rebind(resync_port), &mut joiner_session, &host_id);
    assert!(j3.applied >= 1, "还原变更应过线");
    assert!(joiner_session.repo().unwrap().get_item_opt(&parent_id).is_some());
    assert!(joiner_session.repo().unwrap().has_blob(&att_id), "存活父条目缺失 blob 应经 D-53 阶段补齐");
    {
        let dest = dir.join("joiner-export.bin");
        joiner_session
            .repo()
            .unwrap()
            .export_attachment(&parent_id, &att_id, &dest)
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), original);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑥ 载荷类型白名单：TRASH/CONFLICT/DEVICE/SYNC_STATE 不出网 =====

#[test]
fn case6_export_payload_whitelist_no_internal_records() {
    let dir = temp_dir("case6");
    let vault = dir.join("w.vault");
    let mut session = Session::create(&vault, HOST_PWD, true).unwrap();
    let live_item = make_login(&mut session, "keep-alive", None);
    let doomed = make_login(&mut session, "to-delete", None);
    session.repo_mut().unwrap().delete_item(&doomed.item_id, false).unwrap();

    // 造全量内部 live 记录：TRASH（上一步已产生）、CONFLICT、DEVICE、SYNC_STATE。
    {
        let repo = session.repo_mut().unwrap();
        let entry = ConflictEntry {
            conflict_id: uuid::Uuid::new_v4().to_string(),
            item_id: live_item.item_id.clone(),
            local_snapshot: live_item.clone(),
            remote_snapshot: live_item.clone(),
            local_updated_at: 1,
            remote_updated_at: 1,
            detected_at: 1,
            resolved: false,
            remote_deleted: false,
        };
        repo.add_conflict(entry).unwrap();
        let peer_id = uuid::Uuid::new_v4().to_string();
        let device = PairedDevice {
            // record_id 落帧头须为合法 UUID（record_id_to_bytes 校验）。
            device_id: peer_id.clone(),
            device_name: "qa".to_string(),
            fingerprint: String::new(),
            sync_key_wrapped: WrappedKey { nonce: vec![0u8; 24], ct: vec![0u8; 48] },
            paired_at: now_ms(),
            last_sync_at: None,
            peer_ip: None,
            peer_port: None,
        };
        repo.upsert_paired_device(device).unwrap();
        repo.save_sync_state(repo.sync_state(&peer_id)).unwrap();
    }

    let (deleted_ids, types) = {
        let repo = session.repo().unwrap();
        let empty = repo.sync_state("qa-peer");
        let changes: Vec<ChangeRecord> = repo.export_changes(&empty);
        let deleted_ids: Vec<String> =
            changes.iter().filter(|c| c.deleted).map(|c| c.record_id.clone()).collect();
        let types: Vec<u8> = changes.iter().map(|c| c.record_type).collect();
        (deleted_ids, types)
    };
    assert_eq!(deleted_ids, vec![doomed.item_id], "墓碑恰好一条（deleted=true）");
    assert!(
        types.iter().all(|&t| t == RECORD_TYPE_ITEM || t == RECORD_TYPE_FOLDER),
        "载荷类型越界（TRASH/CONFLICT/DEVICE/SYNC_STATE 不得出网）：{types:?}"
    );
    assert!(
        types.contains(&RECORD_TYPE_ITEM),
        "活跃条目仍应导出"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑦ 墓碑 ciphertext 仍密文（A-01）+ 损坏墓碑跳过不中断 + 重放被拒（A-08）=====

#[test]
fn case7_tombstone_ciphertext_fault_tolerant_export_and_replay() {
    let dir = temp_dir("case7");
    let vault = dir.join("ct.vault");
    let item_id = {
        let mut s = Session::create(&vault, HOST_PWD, true).unwrap();
        let item = make_login(&mut s, "TombSite-SECRET", None);
        s.repo_mut().unwrap().delete_item(&item.item_id, false).unwrap();
        item.item_id
    };

    // 追加一帧「解不开」的墓碑（IK 材料随机 → unwrap/AEAD 必败），验证容错跳过。
    {
        let container = Container::open(&vault).unwrap();
        let mut rid = [0u8; 16];
        rid.copy_from_slice(&rng::random_bytes(16).unwrap());
        let fake = Frame {
            record_type: RECORD_TYPE_ITEM,
            flags: FLAG_TOMBSTONE,
            record_id: rid,
            wrapped_key: WrappedKey { nonce: vec![0u8; 24], ct: vec![0u8; 48] },
            nonce: [0u8; 24],
            plaintext_len: 10,
            ct: vec![7u8; 26],
        };
        container.append_frame(&fake).unwrap();
    }

    let session = Session::unlock(&vault, HOST_PWD).unwrap();
    let (tomb_changes, all_count) = {
        let repo = session.repo().unwrap();
        let empty = repo.sync_state("qa-peer");
        let changes = repo.export_changes(&empty);
        let tombs: Vec<ChangeRecord> =
            changes.iter().filter(|c| c.deleted).cloned().collect();
        (tombs, changes.len())
    };
    assert_eq!(all_count, 1, "损坏墓碑被跳过，仅导出真实墓碑（不中断）");
    assert_eq!(tomb_changes[0].record_id, item_id);

    // A-01：导出的墓碑 ciphertext 不含明文标题；本端 VK 可解出死亡快照（roundtrip）。
    let title = b"TombSite-SECRET";
    assert!(
        !tomb_changes[0].ciphertext.windows(title.len()).any(|w| w == title),
        "墓碑载荷必须仍是密文"
    );
    let snapshot = session.repo().unwrap().change_to_item(&tomb_changes[0]).unwrap();
    assert_eq!(snapshot.item_id, item_id);
    assert_eq!(snapshot.title(), "TombSite-SECRET");
    assert_eq!(snapshot.rev, 1);
    drop(session);

    // A-08：同一信封重放被 ReplayGuard 拒绝。
    let key = rng::random_key().unwrap();
    let sid = rng::random_bytes(16).unwrap();
    let payload = SyncPayload {
        kind: SyncKind::Push,
        device_id: "qa".to_string(),
        cursor: 0,
        changes: Vec::new(),
        conflicts: Vec::new(),
        blob_support: false,
    };
    let env = seal_envelope(&key, &sid, 1, now_ms(), &payload).unwrap();
    let mut replay = ReplayGuard::with_state(0, SYNC_TIME_WINDOW_SECS);
    let now = now_ms();
    replay.accept(env.seq, env.ts, now).expect("首次应通过");
    match replay.accept(env.seq, env.ts, now) {
        Err(CoreError::ReplayDetected) => {}
        other => panic!("重放应被拒绝，实际：{other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑧ 手造老格式墓碑帧（payload=原密文仅 flags 置位）→ 正常导出 rev（零迁移）=====

#[test]
fn case8_legacy_tombstone_frame_exports_without_migration() {
    let dir = temp_dir("case8");
    let vault = dir.join("legacy.vault");
    let item_id = {
        let mut s = Session::create(&vault, HOST_PWD, true).unwrap();
        make_login(&mut s, "legacy-item", None).item_id
    };

    // 模拟老格式墓碑：原帧密文原样，仅 flags |= FLAG_TOMBSTONE（与 tombstone() 等价）。
    {
        let container = Container::open(&vault).unwrap();
        let frames = container.read_frames().unwrap();
        let (offset, frame) = frames
            .iter()
            .find(|(_, f)| f.record_type == RECORD_TYPE_ITEM && !f.is_blob())
            .expect("应有一帧 item")
            .clone();
        let mut tomb = frame;
        tomb.flags |= FLAG_TOMBSTONE;
        container.pwrite_frame(offset, &tomb).unwrap();
    }

    let session = Session::unlock(&vault, HOST_PWD).unwrap();
    let repo = session.repo().unwrap();
    let empty = repo.sync_state("qa-peer");
    let changes = repo.export_changes(&empty);
    assert_eq!(changes.len(), 1, "老格式墓碑应被正常导出");
    assert!(changes[0].deleted);
    assert_eq!(changes[0].record_id, item_id);
    assert_eq!(changes[0].rev, 1, "payload 未重写 → rev 直接解出（零迁移）");
    assert_eq!(repo.tombstone_rev(RECORD_TYPE_ITEM, &item_id), Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}
