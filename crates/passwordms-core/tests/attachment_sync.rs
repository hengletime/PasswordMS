//! 附件删除跨端传播回归（v1.2.4 根因 A 定性套件）。
//!
//! 背景：主人实测「手机删除某条目的一个附件并点过立即同步（该方向成功），电脑上该附件仍在」。
//! `Repository::remove_attachment` 在 core 侧**零覆盖**（本文件是本仓第一个触及它的测试），
//! 因此先用双端 loopback 把「删除能不能过线」这件事**证伪或证实**，再谈修。
//!
//! 覆盖：
//! ① B 删附件 → B 发起重同步 → A 侧 attachments 减一、剩的是哪一个、引用集合不含被删 id；
//! ② A 侧 GC 之前 blob 帧仍可读（设计允许延迟回收）、GC 之后帧被 tombstone；
//! ③ 同一会话内 A 的回程 Push 不得把附件「推回去」（旧列表覆盖型复活）；
//! ④ 反向：A 删 → A 发起重同步 → B 侧同样减一；
//! ⑤ 连续两轮重同步不产生抖动（幂等）。

use std::path::{Path, PathBuf};

use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::resync::{self, ResyncOutcome, ResyncServer};
use passwordms_core::sync::server::{HostSyncResult, SyncServer};

const HOST_PWD: &str = "Att#Host-Pw1";
const JOINER_PWD: &str = "Att#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-attsync-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 确定性伪随机内容（LCG），避免 48/64KB 附件在密文层出现可压缩的巧合。
fn lcg_data(len: usize) -> Vec<u8> {
    let mut data = vec![0u8; len];
    let mut x: u32 = 0x9E37_79B9;
    for b in data.iter_mut() {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        *b = (x >> 16) as u8;
    }
    data
}

fn make_source(dir: &Path, name: &str, len: usize) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, lcg_data(len)).unwrap();
    path
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

/// 建父条目并挂两个附件，返回 `(item_id, att1, att2)`（att1 是第一个挂上的）。
fn host_add_two_attachments(
    session: &mut Session,
    src1: &Path,
    src2: &Path,
) -> (String, String, String) {
    let repo = session.repo_mut().unwrap();
    let parent = repo.create_item(login_draft("att-sync-parent")).unwrap();
    let parent = repo.add_attachment(&parent.item_id, src1, "").unwrap();
    let parent = repo.add_attachment(&parent.item_id, src2, "").unwrap();
    assert_eq!(parent.attachments.len(), 2, "父条目应挂上 2 个附件");
    (
        parent.item_id.clone(),
        parent.attachments[0].attachment_id.clone(),
        parent.attachments[1].attachment_id.clone(),
    )
}

/// 首次配对（Host 已有数据 → Joiner 采纳 VK + 双向同步，含 blob 阶段）。
fn pair_first(
    host_vault: &Path,
    resync_port: u16,
    joiner_vault: &Path,
) -> (HostSyncResult, String) {
    let host_device_id = {
        let s = Session::unlock(host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };
    let identity = HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_device_id.clone(),
        "AttHost".to_string(),
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
        client::pair(&qr, &joiner_id, "AttJoiner", 0, now_ms()).expect("回环配对握手失败");
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
    (host_result, host_device_id)
}

/// 连续 `times` 次直连重同步（固定发起方为 Joiner；Host 侧 ResyncServer 复用同一 listener，
/// 模拟真机上「监听器常驻、同步可重复」的形态）。
fn run_resync_times(
    host_vault: &Path,
    resync_server: ResyncServer,
    joiner_session: &mut Session,
    host_device_id: &str,
    times: usize,
) -> (Vec<ResyncOutcome>, Vec<ResyncOutcome>) {
    let hv = host_vault.to_path_buf();
    let handle = std::thread::spawn(move || -> Vec<ResyncOutcome> {
        let mut host_session = Session::unlock(&hv, HOST_PWD).unwrap();
        let mut resync_server = resync_server;
        let mut out = Vec::new();
        while out.len() < times {
            let repo = host_session.repo_mut().unwrap();
            match resync_server.poll_once(repo, now_ms()) {
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Ok(Some(outcome)) => out.push(outcome),
                Err(e) => panic!("host 重同步失败: {e:?}"),
            }
        }
        out
    });

    let mut joiner_out = Vec::new();
    for _ in 0..times {
        let repo = joiner_session.repo_mut().unwrap();
        joiner_out.push(resync::resync_with_peer(repo, host_device_id, now_ms(), 0).unwrap());
    }
    let host_out = handle.join().unwrap();
    (host_out, joiner_out)
}

/// 统计库文件中某 blob_ref 的**非墓碑** blob 帧数。
fn count_live_blob_frames(vault: &Path, blob_ref: &str) -> usize {
    let container = Container::open(vault).unwrap();
    container
        .read_frames()
        .unwrap()
        .iter()
        .filter(|(_, f)| f.is_blob() && !f.is_tombstone() && f.record_id_string() == blob_ref)
        .count()
}

/// 取某条目当前附件 id 列表。
fn attachment_ids(session: &Session, item_id: &str) -> Vec<String> {
    let repo = session.repo().unwrap();
    let item: Item = repo.get_item(item_id).unwrap().clone();
    item.attachments
        .iter()
        .map(|a| a.attachment_id.clone())
        .collect()
}

// ===== ① B（手机侧角色）删附件 → 过线到 A（电脑侧角色）=====

#[test]
fn joiner_attachment_removal_propagates_to_host() {
    let dir = temp_dir("joiner-removes");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // A：建父条目 + 2 个附件（48KB / 64KB）。
    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let src1 = make_source(&dir, "att1.bin", 48 * 1024);
    let src2 = make_source(&dir, "att2.bin", 64 * 1024);
    let (item_id, att1, att2) = host_add_two_attachments(&mut host_session, &src1, &src2);
    drop(host_session);

    let resync_server = ResyncServer::start(0, "AttHost".to_string()).unwrap();
    let (_h1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    // 断言前置：配对后 B 侧 2 个附件 + 2 个 blob 齐备。
    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    {
        let repo = joiner_session.repo().unwrap();
        assert_eq!(
            attachment_ids(&joiner_session, &item_id),
            vec![att1.clone(), att2.clone()]
        );
        assert!(repo.has_blob(&att1), "B 侧应有 att1 的 blob");
        assert!(repo.has_blob(&att2), "B 侧应有 att2 的 blob");
    }

    // B：删掉第一个附件（rev+1）。
    let removed_rev = {
        let repo = joiner_session.repo_mut().unwrap();
        let before_rev = repo.get_item(&item_id).unwrap().rev;
        let updated = repo.remove_attachment(&item_id, &att1).unwrap();
        assert_eq!(updated.attachments.len(), 1, "本地删除应立即生效");
        assert_eq!(updated.rev, before_rev + 1, "删除必须让父条目 rev+1");
        assert_eq!(updated.attachments[0].attachment_id, att2);
        updated.rev
    };

    // B 发起重同步（B=client / A=server）。
    let (host_out, _joiner_out) =
        run_resync_times(&host_vault, resync_server, &mut joiner_session, &host_id, 1);
    assert_eq!(host_out.len(), 1, "Host 应收到一次连接");
    assert!(
        host_out[0].applied >= 1,
        "Host 应至少应用 1 条变更（父条目），实际 {}",
        host_out[0].applied
    );

    // ③ 回程 Push 不得复活：B 侧仍是 1 个附件。
    assert_eq!(
        attachment_ids(&joiner_session, &item_id),
        vec![att2.clone()],
        "同一会话的回程 Push 不得把被删附件推回 B（旧列表覆盖型复活）"
    );

    // ① A 侧断言。
    // 注意：`Session::unlock` 本身会跑一次 `gc_orphan_blobs`（session.rs:160，
    // 「pbkdf 解锁 → 幂等 GC」），所以这里已经开不出「GC 前」的窗口——
    // 也就是说「GC 之前 blob 帧仍在」**不是**可断言的契约，只能断言引用集合与最终态。
    let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
    let blob_survived_unlock = {
        let repo = hs.repo().unwrap();
        let item = repo.get_item(&item_id).unwrap();
        assert_eq!(
            item.attachments.len(),
            1,
            "删除必须过线：A 侧应只剩 1 个附件"
        );
        assert_eq!(
            item.attachments[0].attachment_id, att2,
            "剩下的必须是第二个附件"
        );
        assert!(item.rev >= removed_rev, "A 侧 rev 不应低于删除后的 rev");
        let refs = repo.attachment_blob_refs();
        assert!(
            !refs.contains(&att1),
            "A 侧引用集合不得包含被删 attachment_id"
        );
        assert!(refs.contains(&att2), "A 侧引用集合必须包含存活附件");
        repo.has_blob(&att1)
    };

    // ② GC 之后：被删 blob 帧必须是墓碑，且存活附件不受牵连。
    if blob_survived_unlock {
        let collected = hs.repo_mut().unwrap().gc_orphan_blobs().unwrap();
        assert_eq!(collected, 1, "GC 应回收 1 个孤儿 blob（att1）");
    }
    {
        let repo = hs.repo().unwrap();
        assert!(!repo.has_blob(&att1), "GC 后 att1 不应再出现在 blobs 索引");
        assert!(repo.has_blob(&att2), "GC 不得误杀存活附件");
        assert!(repo.attachment_blob_refs().iter().all(|r| r != &att1));
    }
    assert_eq!(
        count_live_blob_frames(&host_vault, &att1),
        0,
        "GC 后 att1 的 blob 帧应已是墓碑"
    );
}

// ===== ④ 反向：A（电脑侧角色）删附件 → 过线到 B =====

#[test]
fn host_attachment_removal_propagates_to_joiner() {
    let dir = temp_dir("host-removes");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let src1 = make_source(&dir, "att1.bin", 40 * 1024);
    let src2 = make_source(&dir, "att2.bin", 56 * 1024);
    let (item_id, att1, att2) = host_add_two_attachments(&mut host_session, &src1, &src2);
    drop(host_session);

    let resync_server = ResyncServer::start(0, "AttHost".to_string()).unwrap();
    let (_h1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    // A 侧删第一个附件。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        hs.repo_mut()
            .unwrap()
            .remove_attachment(&item_id, &att1)
            .unwrap();
    }

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let (host_out, _joiner_out) =
        run_resync_times(&host_vault, resync_server, &mut joiner_session, &host_id, 1);
    assert_eq!(host_out.len(), 1);

    assert_eq!(
        attachment_ids(&joiner_session, &item_id),
        vec![att2.clone()],
        "反向删除必须过线：B 侧应只剩第二个附件"
    );
    {
        let repo = joiner_session.repo().unwrap();
        let refs = repo.attachment_blob_refs();
        assert!(
            !refs.contains(&att1),
            "B 侧引用集合不得包含被删 attachment_id"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑤ 幂等：删完再同步两轮，不抖动、不复活 =====

#[test]
fn attachment_removal_is_idempotent_across_repeated_resyncs() {
    let dir = temp_dir("idempotent");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let src1 = make_source(&dir, "att1.bin", 36 * 1024);
    let src2 = make_source(&dir, "att2.bin", 52 * 1024);
    let (item_id, att1, att2) = host_add_two_attachments(&mut host_session, &src1, &src2);
    drop(host_session);

    let resync_server = ResyncServer::start(0, "AttHost".to_string()).unwrap();
    let (_h1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    joiner_session
        .repo_mut()
        .unwrap()
        .remove_attachment(&item_id, &att1)
        .unwrap();

    let (host_out, _joiner_out) =
        run_resync_times(&host_vault, resync_server, &mut joiner_session, &host_id, 2);
    assert_eq!(host_out.len(), 2, "Host 应收到两次连接");

    // 两轮之后两端都稳定在「1 个附件 = att2」。
    assert_eq!(
        attachment_ids(&joiner_session, &item_id),
        vec![att2.clone()]
    );
    let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
    assert_eq!(
        attachment_ids(&hs, &item_id),
        vec![att2],
        "重复同步不得让附件复活"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
