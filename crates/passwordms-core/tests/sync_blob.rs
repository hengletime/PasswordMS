//! D-53 附件 blob 跨设备同步回归套件（架构文档 §3.12，验收 9 条）。
//!
//! 覆盖：①端到端字节一致 ②断连续传只补差额 ③删除传播 merge 级连带 ④元数据更新零
//! blob 传输 ⑤双向对称 ⑥20MB 压满 27 片逐信封 ≤ 上限 ⑦能力协商门 ⑧容量预检 ⑨
//! spool 无明文（A-01）+ 信封重放被拒（A-08）。
//!
//! 备案（本套件范围裁定）：⑧的「Meta 拒绝」分支以纯函数 `capacity_ok` 单测覆盖——
//! 回环里真造 200MB 库不经济；容量拒绝的消费丢弃路径与预检同函数串联，结构性保证
//! 「不落半个分片」（spool 在预检通过后才创建）。

use std::path::{Path, PathBuf};

use passwordms_core::constants::{
    MAX_VAULT_BYTES, SYNC_BLOB_CHUNK_BYTES, SYNC_MAX_FRAME_BYTES, SYNC_TIME_WINDOW_SECS,
};
use passwordms_core::error::CoreError;
use passwordms_core::model::item::{AttachmentData, Item, ItemKind, LoginData, SecureNoteData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;
use passwordms_core::store::repository::{BlobWriteOutcome, ItemDraft};
use passwordms_core::sync::blob_transfer::{self, seal_blob_envelope, open_blob_envelope};
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::merge::apply_changes;
use passwordms_core::sync::protocol::{BlobMessage, SyncEnvelope};
use passwordms_core::sync::replay::ReplayGuard;
use passwordms_core::sync::resync::{self, ResyncOutcome, ResyncServer};
use passwordms_core::sync::server::{SyncServer, HostSyncResult};
use passwordms_core::sync::client::JoinerSyncResult;

const HOST_PWD: &str = "Blob#Host-Pw1";
const JOINER_PWD: &str = "Blob#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-blob-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 生成确定性伪随机内容（LCG），头部带可检索明文金丝雀（⑨ A-01 断言用）。
fn lcg_data(len: usize, canary: bool) -> Vec<u8> {
    let mut data = vec![0u8; len];
    let mut x: u32 = 0x1234_5678;
    for b in data.iter_mut() {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        *b = (x >> 16) as u8;
    }
    if canary {
        let c = b"PMS-PLAINTEXT-CANARY-0123456789";
        data[..c.len()].copy_from_slice(c);
    }
    data
}

fn make_source(dir: &Path, name: &str, len: usize, canary: bool) -> (PathBuf, Vec<u8>) {
    let path = dir.join(name);
    let data = lcg_data(len, canary);
    std::fs::write(&path, &data).unwrap();
    (path, data)
}

/// 建一个登录型父条目草稿（D-60：附件挂在父条目下）。
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

/// 在会话上建父条目并挂附件（D-60 §3.14.5 `add_attachment`），返回 (parent_id, attachment_id)。
fn host_add_attachment(session: &mut Session, src: &Path) -> (String, String) {
    let repo = session.repo_mut().unwrap();
    let parent = repo.create_item(login_draft("att-parent")).unwrap();
    let parent = repo.add_attachment(&parent.item_id, src, "").unwrap();
    let att = parent.attachments[0].attachment_id.clone();
    (parent.item_id, att)
}

/// 首个附件元数据的 attachment_id（D-60：≠ 父 item_id，即 blob 帧 record_id）。
fn attachment_ref(item: &Item) -> String {
    item.attachments
        .first()
        .expect("条目应有附件")
        .attachment_id
        .clone()
}

fn first_with_attachment(session: &Session) -> Item {
    session
        .repo()
        .unwrap()
        .list_items(None, false, None)
        .into_iter()
        .find(|i| !i.attachments.is_empty())
        .expect("应有带附件的条目")
}

/// 统计库文件中某 blob_ref 的**非墓碑** blob 帧数（「无重复帧」断言用）。
fn count_blob_frames(vault: &Path, blob_ref: &str) -> usize {
    let container = Container::open(vault).unwrap();
    container
        .read_frames()
        .unwrap()
        .iter()
        .filter(|(_, f)| f.is_blob() && !f.is_tombstone() && f.record_id_string() == blob_ref)
        .count()
}

fn spool_root(vault: &Path) -> PathBuf {
    let mut s = vault.as_os_str().to_os_string();
    s.push(".blobspool");
    PathBuf::from(s)
}

/// 首次配对：Host（已有库与数据）起会话，Joiner 全新建库 + 采纳 VK + 双向同步
/// （含 D-53 blob 阶段）。返回两侧结果与 host_device_id。
fn pair_first(
    host_vault: &Path,
    resync_port: u16,
    joiner_vault: &Path,
) -> (HostSyncResult, JoinerSyncResult, String) {
    let host_device_id = {
        let s = Session::unlock(host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };
    let identity = HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_device_id.clone(),
        "BlobHost".to_string(),
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
    let mut joiner = client::pair(&qr, &joiner_id, "BlobJoiner", 0, now_ms())
        .expect("回环配对握手失败");
    joiner_session.adopt_vault_key(joiner.host_vault_key()).unwrap();
    let result = {
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        joiner.push_and_sync(repo, now_ms())
    };
    drop(joiner);
    // 先 unwrap joiner 侧再收 host 线程：任一侧失败时 panic 消息即含具体错误，
    // 且 host 线程不会被提前 abort（joiner 先失败时对端自然 EOF 报错）。
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
        .expect("host 重同步失败")
        .expect("host 轮询应收到一次连接");
    (host_outcome, joiner_outcome)
}

/// 导出 joiner 侧首个附件并断言与原始字节一致，返回 attachment_id。
fn assert_export_matches(session: &Session, vault: &Path, original: &[u8]) -> String {
    let item = first_with_attachment(session);
    let att = attachment_ref(&item);
    let dest = vault.with_extension("export.bin");
    session
        .repo()
        .unwrap()
        .export_attachment(&item.item_id, &att, &dest)
        .expect("导出应成功");
    let exported = std::fs::read(&dest).unwrap();
    assert_eq!(exported.len(), original.len(), "导出长度不一致");
    assert!(exported == original, "导出字节不一致");
    let _ = std::fs::remove_file(&dest);
    att
}

// ===== ① 端到端：2MB 附件同步后字节一致且 blob 帧字节相同 =====

#[test]
fn case1_end_to_end_attachment_blob_sync() {
    let dir = temp_dir("case1");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src, original) = make_source(&dir, "src2mb.bin", 2 * 1024 * 1024, false);
    let (item_id, attachment_id) = host_add_attachment(&mut host_session, &src);
    drop(host_session);

    let (host_result, joiner_result, _host_id) = pair_first(&host_vault, 0, &joiner_vault);
    assert_eq!(joiner_result.blobs, 1, "Joiner 应新入库 1 个 blob");
    assert_eq!(host_result.blobs, 0, "Host 无缺失，不应入库任何 blob");

    // Joiner 侧导出字节一致。
    let joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let blob_ref = assert_export_matches(&joiner_session, &joiner_vault, &original);
    // D-60 §3.14.1：attachment_id 与父 item_id 解耦（blob 帧 record_id = attachment_id）。
    assert_eq!(blob_ref, attachment_id, "blob 帧 record_id == attachment_id");
    assert_ne!(blob_ref, item_id, "attachment_id 不得再与父 item_id 强耦合");

    // 双端 blob 帧字节完全一致（可原样 append_frame）。
    let host_session = Session::unlock(&host_vault, HOST_PWD).unwrap();
    let host_frame = host_session.repo().unwrap().blob_frame(&blob_ref).expect("host 应有 blob 帧");
    let joiner_frame = joiner_session.repo().unwrap().blob_frame(&blob_ref).expect("joiner 应有 blob 帧");
    assert_eq!(host_frame, joiner_frame, "双端 blob 帧字节应完全一致");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ② 断连续传：预置第 0 片 → 重同步只补差额 → 无重复帧 =====

#[test]
fn case2_resume_from_partial_spool_no_duplicate_frames() {
    let dir = temp_dir("case2");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src, original) = make_source(&dir, "src2mb.bin", 2 * 1024 * 1024, false);
    let (_item_id, blob_ref) = host_add_attachment(&mut host_session, &src);
    drop(host_session);

    // 断点模拟：直接预置 spool 第 0 片（密文）+ manifest（received=[0]）。
    let host_frame = {
        let s = Session::unlock(&host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().blob_frame(&blob_ref).unwrap()
    };
    let ct = host_frame.ct.clone();
    let chunk_total = ct.len().div_ceil(SYNC_BLOB_CHUNK_BYTES) as u32;
    assert!(chunk_total >= 2, "2MB 附件应 ≥ 2 片");
    let spool_dir = spool_root(&joiner_vault).join(&blob_ref);
    std::fs::create_dir_all(&spool_dir).unwrap();
    std::fs::write(spool_dir.join("0.part"), &ct[..SYNC_BLOB_CHUNK_BYTES]).unwrap();
    let manifest = format!(
        r#"{{"blob_ref":"{blob_ref}","chunk_total":{chunk_total},"chunk_size":{SYNC_BLOB_CHUNK_BYTES},"received":[0]}}"#
    );
    std::fs::write(spool_dir.join("manifest.json"), manifest).unwrap();

    // 同步：续传只补第 1、2 片。
    let (_host_result, joiner_result, _id) = pair_first(&host_vault, 0, &joiner_vault);
    assert_eq!(joiner_result.blobs, 1, "续传后应完成 1 个 blob");

    // 无重复帧：该 ref 的非墓碑 blob 帧恰好 1 条。
    assert_eq!(count_blob_frames(&joiner_vault, &blob_ref), 1, "不得产生重复 blob 帧");

    // 结果正确 + spool 清理。
    let joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let att = assert_export_matches(&joiner_session, &joiner_vault, &original);
    assert_eq!(att, blob_ref);
    assert!(!spool_dir.exists(), "finalize 后 spool 目录应被清理");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ③ 删除传播 merge 级（D-60 修订）：父墓碑=软删进回收站且 blob 保留；purge → GC 清 =====

#[test]
fn case3_merge_delete_cascades_to_blob_and_soft_delete_keeps_it() {
    let dir = temp_dir("case3");
    let host_vault = dir.join("a.vault");
    let b_vault = dir.join("b.vault");

    let mut session_a = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src, _data) = make_source(&dir, "src.bin", 64 * 1024, false);
    let (item_id, attachment_id) = host_add_attachment(&mut session_a, &src);
    // D-56：export_changes 需要对端 base（此处单端构造，空 base = 全量导出）。
    let changes = {
        let repo = session_a.repo().unwrap();
        repo.export_changes(&repo.sync_state("qa-peer"))
    };
    let blob_frame = session_a.repo().unwrap().blob_frame(&attachment_id).unwrap();

    // B 建空库并采纳 A 的 VK（change_to_item 解密需要同 VK）。
    let mut session_b = Session::create(&b_vault, JOINER_PWD, true).unwrap();
    let b_vault_key = session_a.repo().unwrap().vault_key().clone();
    session_b.adopt_vault_key(&b_vault_key).unwrap();

    // 1) 应用条目变更（deleted=false）→ B 有父条目（含附件元数据）、无 blob。
    let mut state = session_b.repo().unwrap().sync_state("qa-peer");
    let report = apply_changes(session_b.repo_mut().unwrap(), &mut state, &changes).unwrap();
    assert!(report.applied >= 1);
    let b_item = session_b.repo().unwrap().get_item_opt(&item_id).expect("父条目应存在");
    assert_eq!(b_item.attachments.len(), 1, "附件元数据应随父帧过线");
    assert_eq!(b_item.attachments[0].attachment_id, attachment_id);
    assert!(!session_b.repo().unwrap().has_blob(&attachment_id), "blob 未同步前应缺失");

    // 2) 补上 blob（写帧原语）→ has_blob。
    let outcome = session_b
        .repo_mut()
        .unwrap()
        .write_blob_frame(blob_frame.clone())
        .unwrap();
    assert_eq!(outcome, BlobWriteOutcome::Appended);
    assert!(session_b.repo().unwrap().has_blob(&attachment_id));

    // 3) 远端删除传播（父帧 deleted=true）→ D-56 裁定二：**软删**进回收站，blob **保留**
    //    （回收站还原需要；D-60 后附件元数据随父帧死亡，还原依赖 trash 快照）。
    let mut tombstone = changes.clone();
    for c in tombstone.iter_mut() {
        if c.record_id == item_id {
            c.deleted = true;
        }
    }
    apply_changes(session_b.repo_mut().unwrap(), &mut state, &tombstone).unwrap();
    assert!(
        session_b.repo().unwrap().get_item_opt(&item_id).is_none(),
        "条目应离开活跃表"
    );
    assert!(session_b.repo().unwrap().has_blob(&attachment_id), "远端软删必须保留 blob");
    assert_eq!(session_b.repo().unwrap().list_trash().len(), 1, "应进回收站");

    // 4) 本地软删（进回收站）**不清** blob（还原要用）。
    session_a.repo_mut().unwrap().delete_item(&item_id, false).unwrap();
    assert!(session_a.repo().unwrap().has_blob(&attachment_id), "软删不得清 blob");
    // purge（彻底删）→ 孤儿 GC 清 blob——D-60 §3.14.4 修订：GC 而非 1:1 连带。
    let trash_id = session_a.repo().unwrap().list_trash()[0].trash_id.clone();
    session_a.repo_mut().unwrap().purge_item(&trash_id).unwrap();
    assert!(!session_a.repo().unwrap().has_blob(&attachment_id), "purge 后 GC 应清 blob");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ④ 单端仅元数据更新 → blob 零传输（Pull 清单空）=====

#[test]
fn case4_metadata_only_update_transfers_no_blob() {
    let dir = temp_dir("case4");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src, original) = make_source(&dir, "src.bin", 512 * 1024, false);
    let (item_id, blob_ref) = host_add_attachment(&mut host_session, &src);
    drop(host_session);

    // Host 的持久重同步监听器先起（端口随二维码通告给 Joiner）。
    let resync_server = ResyncServer::start(0, "BlobHost".to_string()).unwrap();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let before = joiner_session.repo().unwrap().blob_frame(&blob_ref).unwrap();
    let att = assert_export_matches(&joiner_session, &joiner_vault, &original);
    assert_eq!(att, blob_ref);

    // 仅元数据更新（favorite 翻转 → rev+1），blob 内容不变。
    {
        let mut hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
        hs.repo_mut().unwrap().toggle_favorite(&item_id, true).unwrap();
    }

    let (host_outcome, joiner_outcome) =
        run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    assert_eq!(host_outcome.blobs, 0, "Host 拉取清单应为空");
    assert_eq!(joiner_outcome.blobs, 0, "Joiner 拉取清单应为空：blob 零传输");

    // Joiner 的 blob 帧字节不变；无 spool 残留。
    let after = joiner_session.repo().unwrap().blob_frame(&blob_ref).unwrap();
    assert_eq!(before, after, "元数据更新不得触碰 blob 帧");
    assert!(!spool_root(&joiner_vault).exists(), "不得留下 spool 目录");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑤ 双向对称：两端各有附件 → 重同步后两侧全量可用 =====

#[test]
fn case5_bidirectional_symmetric_sync() {
    let dir = temp_dir("case5");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src1, data1) = make_source(&dir, "src1.bin", 384 * 1024, false);
    let (_pid1, att1) = host_add_attachment(&mut host_session, &src1);
    drop(host_session);

    let resync_server = ResyncServer::start(0, "BlobHost".to_string()).unwrap();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    // Joiner 侧（已采纳 VK）创建自己的父条目并挂附件（D-60 §3.14.5）。
    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let (src2, data2) = make_source(&dir, "src2.bin", 640 * 1024, false);
    let att2 = {
        let repo = joiner_session.repo_mut().unwrap();
        let parent = repo.create_item(login_draft("joiner-parent")).unwrap();
        let parent = repo.add_attachment(&parent.item_id, &src2, "").unwrap();
        attachment_ref(&parent)
    };

    // 重同步（Joiner 发起）：Joiner 拉（无缺）→ 供 att2 给 Host。
    let (host_outcome, joiner_outcome) =
        run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    assert_eq!(joiner_outcome.blobs, 0, "Joiner 无缺失");
    assert_eq!(host_outcome.blobs, 1, "Host 应新入库 att2 的 blob");

    // 两侧：2 个带附件的父条目 + 2 个 blob 齐备。
    let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
    let js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    for session in [&hs, &js] {
        let repo = session.repo().unwrap();
        let parents: Vec<Item> = repo
            .list_items(None, false, None)
            .into_iter()
            .filter(|i| !i.attachments.is_empty())
            .collect();
        assert_eq!(parents.len(), 2, "两侧都应有 2 个带附件的父条目");
        for item in &parents {
            let blob_ref = attachment_ref(item);
            assert!(repo.has_blob(&blob_ref), "blob {blob_ref} 应已同步");
        }
    }
    // 精确比对：att2 在 Host 侧导出 == data2；att1 在 Joiner 侧导出 == data1。
    let host_parent2 = hs
        .repo()
        .unwrap()
        .list_items(None, false, None)
        .into_iter()
        .find(|i| attachment_ref(i) == att2)
        .unwrap();
    let dest = dir.join("host2.bin");
    hs.repo()
        .unwrap()
        .export_attachment(&host_parent2.item_id, &att2, &dest)
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data2, "Host 侧 att2 字节应与 Joiner 原始一致");
    let _ = std::fs::remove_file(&dest);
    let joiner_parent1 = js
        .repo()
        .unwrap()
        .list_items(None, false, None)
        .into_iter()
        .find(|i| attachment_ref(i) == att1)
        .unwrap();
    let dest = dir.join("joiner1.bin");
    js.repo()
        .unwrap()
        .export_attachment(&joiner_parent1.item_id, &att1, &dest)
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data1, "Joiner 侧 att1 字节应与 Host 原始一致");
    let _ = std::fs::remove_file(&dest);

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑥ 20MB 压满：chunk_total ≈ 27，逐信封 ≤ SYNC_MAX_FRAME_BYTES =====

#[test]
fn case6_20mb_attachment_chunk_math_and_envelope_limit() {
    // 分片数学：20MB 明文 → ct = 20MB + 16B tag → ceil(ct / 768KiB) = 27。
    let ct_len = 20 * 1024 * 1024usize + 16;
    let chunk_total = ct_len.div_ceil(SYNC_BLOB_CHUNK_BYTES);
    assert_eq!(chunk_total, 27, "20MB 附件应恰为 27 片");

    // 逐片信封（真实 seal 路径）必须全部 < SYNC_MAX_FRAME_BYTES（强制单测，T-D53-01）。
    let key = [7u8; 32];
    let session_id = [1u8; 16];
    let blob_ref = "00000000-0000-4000-8000-000000000001".to_string();
    for index in 0..chunk_total {
        let slice_len = if index == chunk_total - 1 {
            ct_len - (chunk_total - 1) * SYNC_BLOB_CHUNK_BYTES
        } else {
            SYNC_BLOB_CHUNK_BYTES
        };
        let msg = BlobMessage::Data {
            blob_ref: blob_ref.clone(),
            index: index as u32,
            ct_slice: vec![0xABu8; slice_len],
        };
        let env = seal_blob_envelope(&key, &session_id, 100 + index as u64, now_ms(), &msg).unwrap();
        let wire = serde_json::to_vec(&env).unwrap();
        assert!(
            wire.len() < SYNC_MAX_FRAME_BYTES as usize,
            "第 {index} 片信封 {} B 超限（上限 {} B）",
            wire.len(),
            SYNC_MAX_FRAME_BYTES
        );
    }

    // 端到端：20MB 附件真实走一遍配对同步。
    let dir = temp_dir("case6e2e");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");
    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src, original) = make_source(&dir, "src20mb.bin", 20 * 1024 * 1024, false);
    let (item_id, attachment_id) = host_add_attachment(&mut host_session, &src);
    drop(host_session);

    let (_h, _j, _id) = pair_first(&host_vault, 0, &joiner_vault);
    let js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let blob_ref = assert_export_matches(&js, &joiner_vault, &original);
    assert_eq!(blob_ref, attachment_id, "blob 帧 record_id == attachment_id");
    assert_ne!(blob_ref, item_id, "attachment_id 与父 item_id 解耦");
    assert_eq!(count_blob_frames(&joiner_vault, &attachment_id), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑦ 能力协商：对端 blob_support=false → 门关闭（不发 BlobPull）=====

#[test]
fn case7_capability_gate_matches_legacy_peers() {
    // 协商门：任一端不支持即不进入 blob 阶段（老端永远收不到新报文）。
    assert!(!blob_transfer::blob_phase_enabled(false, false));
    assert!(!blob_transfer::blob_phase_enabled(false, true));
    assert!(!blob_transfer::blob_phase_enabled(true, false));
    assert!(blob_transfer::blob_phase_enabled(true, true));

    // 老端报文（无 blob_support 字段）反序列化 → false（serde default，协议层单测同款断言）。
    let legacy = r#"{"kind":"push","device_id":"dev","changes":[],"conflicts":[]}"#;
    let payload: passwordms_core::sync::protocol::SyncPayload = serde_json::from_str(legacy).unwrap();
    assert!(!payload.blob_support);
    assert!(!blob_transfer::blob_phase_enabled(true, payload.blob_support));
}

// ===== ⑧ 容量预检：超限 Meta 阶段即拒，不落半个分片 =====

#[test]
fn case8_capacity_precheck_rejects_before_first_chunk() {
    // 20MB 附件在接近满库时拒绝。
    assert!(!blob_transfer::capacity_ok(MAX_VAULT_BYTES - 4096, 20 * 1024 * 1024));
    assert!(!blob_transfer::capacity_ok(MAX_VAULT_BYTES, 0));
    // 正常场景放行。
    assert!(blob_transfer::capacity_ok(0, 20 * 1024 * 1024));
    assert!(blob_transfer::capacity_ok(100 * 1024 * 1024, 1024 * 1024));
    // 结构性保证：pull_missing 中 spool 仅在预检通过后创建（Spool::open_or_create
    // 位于 capacity_ok 分支之内）→ 超限时不存在任何分片文件。
}

// ===== ⑨ spool 无明文（A-01）+ 重放旧 BlobData 被拒（A-08）=====

#[test]
fn case9_spool_ciphertext_only_and_envelope_replay_rejected() {
    // A-08：同一 blob 信封重放（seq 已消费）→ ReplayDetected；session_id 不符 → 同。
    let key = [7u8; 32];
    let session_id = [1u8; 16];
    let msg = BlobMessage::Data {
        blob_ref: "00000000-0000-4000-8000-000000000009".to_string(),
        index: 3,
        ct_slice: vec![1u8; 64],
    };
    let env: SyncEnvelope = seal_blob_envelope(&key, &session_id, 5, now_ms(), &msg).unwrap();
    let mut replay = ReplayGuard::new(SYNC_TIME_WINDOW_SECS);
    replay.accept(env.seq, env.ts, now_ms()).expect("首次接收应通过");
    assert!(
        matches!(
            replay.accept(env.seq, env.ts, now_ms()),
            Err(CoreError::ReplayDetected)
        ),
        "重放旧 BlobData 必须被拒"
    );
    assert!(matches!(
        open_blob_envelope(&key, &[2u8; 16], &env),
        Err(CoreError::ReplayDetected)
    ));

    // A-01：spool 目录只含密文切片——明文金丝雀不得出现在任何 spool 文件里。
    let dir = temp_dir("case9");
    let vault = dir.join("a.vault");
    let mut session = Session::create(&vault, HOST_PWD, true).unwrap();
    let (src, _original) = make_source(&dir, "src.bin", 128 * 1024, true);
    let (_item_id, attachment_id) = host_add_attachment(&mut session, &src);
    let frame = session.repo().unwrap().blob_frame(&attachment_id).unwrap();

    let spool_dir = spool_root(&vault).join(&attachment_id);
    std::fs::create_dir_all(&spool_dir).unwrap();
    // 模拟真实接收：落两片**密文**切片（取自帧 ct）。
    std::fs::write(spool_dir.join("0.part"), &frame.ct[..100 * 1024]).unwrap();
    std::fs::write(spool_dir.join("1.part"), &frame.ct[100 * 1024..]).unwrap();

    let canary = b"PMS-PLAINTEXT-CANARY-0123456789";
    for entry in std::fs::read_dir(&spool_dir).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(
            !bytes
                .windows(canary.len())
                .any(|w| w == canary),
            "spool 文件中出现明文金丝雀（A-01 违规）"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑩ D-60：多附件（≥3）逐个断连续传 + 单条目 16 上限拒绝 + 255B 文件名边界 =====

#[test]
fn case10_multi_attachment_resume_and_limit_and_filename_bounds() {
    let dir = temp_dir("case10");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // 单父条目挂 3 个附件（每个 ≥ 768KiB，保证 ≥ 2 片可续传）。
    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src_a, data_a) = make_source(&dir, "src_a.bin", 1024 * 1024, false);
    let (src_b, data_b) = make_source(&dir, "src_b.bin", 1536 * 1024, false);
    let (src_c, data_c) = make_source(&dir, "src_c.bin", 2 * 1024 * 1024, false);
    let (parent_id, att_ids) = {
        let repo = host_session.repo_mut().unwrap();
        let parent = repo.create_item(login_draft("multi-att-parent")).unwrap();
        let pid = parent.item_id.clone();
        repo.add_attachment(&pid, &src_a, "").unwrap();
        repo.add_attachment(&pid, &src_b, "").unwrap();
        let parent = repo.add_attachment(&pid, &src_c, "").unwrap();
        let ids: Vec<String> = parent
            .attachments
            .iter()
            .map(|m| m.attachment_id.clone())
            .collect();
        (pid, ids)
    };
    drop(host_session);
    assert_eq!(att_ids.len(), 3, "应有 3 个附件");
    // D-60 §3.14.1：每个 attachment_id 互不相同且均 ≠ 父 item_id。
    for (i, id) in att_ids.iter().enumerate() {
        assert_ne!(id, &parent_id, "attachment_id 不得等于父 item_id");
        for other in att_ids.iter().skip(i + 1) {
            assert_ne!(id, other, "attachment_id 应两两互异");
        }
    }

    // 断点预置：3 个附件各留第 0 片密文 + manifest（模拟三路同时中断）。
    {
        let s = Session::unlock(&host_vault, HOST_PWD).unwrap();
        let spool_base = spool_root(&joiner_vault);
        for id in &att_ids {
            let frame = s.repo().unwrap().blob_frame(id).unwrap();
            let ct = frame.ct.clone();
            let chunk_total = ct.len().div_ceil(SYNC_BLOB_CHUNK_BYTES) as u32;
            assert!(chunk_total >= 2, "附件 {id} 应 ≥ 2 片才叫续传");
            let spool_dir = spool_base.join(id);
            std::fs::create_dir_all(&spool_dir).unwrap();
            std::fs::write(spool_dir.join("0.part"), &ct[..SYNC_BLOB_CHUNK_BYTES]).unwrap();
            let manifest = format!(
                r#"{{"blob_ref":"{id}","chunk_total":{chunk_total},"chunk_size":{SYNC_BLOB_CHUNK_BYTES},"received":[0]}}"#
            );
            std::fs::write(spool_dir.join("manifest.json"), manifest).unwrap();
        }
    }

    // 同步：3 个 blob 全部续传完成。
    let (_host_result, joiner_result, _id) = pair_first(&host_vault, 0, &joiner_vault);
    assert_eq!(joiner_result.blobs, 3, "续传后应完成 3 个 blob");
    for id in &att_ids {
        assert_eq!(count_blob_frames(&joiner_vault, id), 1, "附件 {id} 不得产生重复帧");
    }

    // Joiner 侧逐个导出 == 原始字节（顺序对应挂载顺序 a/b/c）。
    let mut js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let originals = [data_a, data_b, data_c];
    for (i, id) in att_ids.iter().enumerate() {
        let dest = dir.join(format!("out{i}.bin"));
        js.repo()
            .unwrap()
            .export_attachment(&parent_id, id, &dest)
            .expect("导出应成功");
        assert_eq!(std::fs::read(&dest).unwrap(), originals[i], "附件 {i} 字节不一致");
        let _ = std::fs::remove_file(&dest);
    }

    // 16 上限：已有 3 个 → 再补 13 个成功，第 17 个拒绝（ERR_ATTACHMENT_LIMIT 语义）。
    let tiny = dir.join("tiny.bin");
    std::fs::write(&tiny, [0u8; 1024]).unwrap();
    for _ in 0..13 {
        js.repo_mut().unwrap().add_attachment(&parent_id, &tiny, "").unwrap();
    }
    let err = js
        .repo_mut()
        .unwrap()
        .add_attachment(&parent_id, &tiny, "")
        .unwrap_err();
    assert!(
        matches!(err, CoreError::AttachmentLimit),
        "第 17 个附件必须被 AttachmentLimit 拒绝，实际 {err:?}"
    );

    // 255B 文件名边界（新父条目，避开上限干扰）：255B 通过 / 256B 拒绝。
    // 用三字节中文字符精确控制字节数（85×3=255 / 86×3=258），同时验证 UTF-8 字节语义。
    let repo = js.repo_mut().unwrap();
    let parent2 = repo.create_item(login_draft("name-boundary")).unwrap();
    let ok_name = "附".repeat(85);
    let ok_path = dir.join(&ok_name);
    std::fs::write(&ok_path, [1u8; 16]).unwrap();
    assert_eq!(ok_name.len(), 255);
    let ok_item = repo.add_attachment(&parent2.item_id, &ok_path, "").unwrap();
    assert_eq!(ok_item.attachments.len(), 1, "255B 文件名应放行");
    let bad_name = "附".repeat(86);
    let bad_path = dir.join(&bad_name);
    std::fs::write(&bad_path, [1u8; 16]).unwrap();
    assert_eq!(bad_name.len(), 258);
    let err = repo.add_attachment(&parent2.item_id, &bad_path, "").unwrap_err();
    assert!(
        matches!(err, CoreError::Validation(ref m) if m == "filename_too_long"),
        "256B+ 文件名必须被拒，实际 {err:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑪ D-60 §3.14.3：遗留 Attachment 条目解锁迁移 → SecureNote，blob 帧字节零重写，幂等 =====

#[test]
fn case11_legacy_attachment_migration_idempotent() {
    let dir = temp_dir("case11");
    let vault = dir.join("legacy.vault");
    let original = lcg_data(96 * 1024, false);
    let src = dir.join("legacy.bin");
    std::fs::write(&src, &original).unwrap();

    // 构造「存量旧库」形态：
    // 1) 正常会话里造一个合法 blob 帧（真密文，迁移后导出可解）；
    // 2) put_item_direct 注入 ItemKind::Attachment 遗留条目（item_id == blob_ref == X）；
    // 3) 造帧用的临时父条目 purge 掉（遗留条目的 blob_ref 仍在 GC 引用集合内，blob 保活）。
    //    返回 (attachment_id, 迁移前 blob 帧)——迁移后须与该帧字节一致（零重写）。
    let (x, frame_before) = {
        let mut s = Session::create(&vault, HOST_PWD, true).unwrap();
        let repo = s.repo_mut().unwrap();
        let vehicle = repo.create_item(login_draft("blob-vehicle")).unwrap();
        let vehicle = repo.add_attachment(&vehicle.item_id, &src, "").unwrap();
        let x = attachment_ref(&vehicle);
        let meta = &vehicle.attachments[0];
        #[allow(deprecated)]
        let legacy = Item {
            item_id: x.clone(), // 旧语义：item_id == blob_ref → 迁移后 blob 帧零重写
            kind: ItemKind::Attachment(AttachmentData {
                title: "old-attachment".to_string(),
                original_filename: "legacy.bin".to_string(),
                mime_type: "application/octet-stream".to_string(),
                size_bytes: meta.size_bytes,
                blob_ref: x.clone(),
                blob_size_bytes: meta.blob_size_bytes,
            }),
            folder_id: None,
            favorite: false,
            created_at: vehicle.created_at,
            updated_at: vehicle.updated_at,
            rev: 1,
            modified_by: String::new(),
            attachments: Vec::new(),
        };
        repo.put_item_direct(legacy).unwrap();
        // 清掉造帧用的临时父条目（软删 → purge；purge 的 GC 不会误杀 X——
        // 遗留条目的 blob_ref 仍被 attachment_blob_refs 收纳，§3.14.4 兼容窗口）。
        repo.delete_item(&vehicle.item_id, false).unwrap();
        let trash_id = repo.list_trash()[0].trash_id.clone();
        repo.purge_item(&trash_id).unwrap();
        let frame_before = repo.blob_frame(&x).unwrap();
        (x, frame_before)
    };

    // 解锁（unlock 链自动迁移）→ 断言迁移产物。
    let s = Session::unlock(&vault, HOST_PWD).unwrap();
    {
        let repo = s.repo().unwrap();
        let items = repo.list_items(None, false, None);
        // 遗留型条目清零。
        #[allow(deprecated)]
        let legacy_left = items.iter().filter(|i| matches!(i.kind, ItemKind::Attachment(_))).count();
        assert_eq!(legacy_left, 0, "迁移后不得残留遗留型条目");
        // 迁移产物：SecureNote{ title: 原 title }，attachment_id == 旧 item_id（== blob_ref）。
        let migrated = items
            .iter()
            .find(|i| i.title() == "old-attachment")
            .expect("迁移产物 SecureNote 应存在");
        assert!(matches!(migrated.kind, ItemKind::SecureNote(_)), "迁移产物应为 SecureNote");
        assert_eq!(migrated.attachments.len(), 1);
        let meta = &migrated.attachments[0];
        assert_eq!(meta.attachment_id, x, "attachment_id 必须沿用旧 item_id（blob 零重写前提）");
        assert_eq!(meta.filename, "legacy.bin");
        assert_eq!(meta.size_bytes, original.len() as u64);
        // blob 帧字节级零重写。
        assert_eq!(
            repo.blob_frame(&x).unwrap(),
            frame_before,
            "迁移不得重写 blob 帧"
        );
        assert_eq!(count_blob_frames(&vault, &x), 1, "不得产生重复 blob 帧");
        // 迁移产物可导出原字节。
        let dest = dir.join("migrated.bin");
        repo.export_attachment(&migrated.item_id, &x, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), original, "迁移后导出应与原始一致");
        let _ = std::fs::remove_file(&dest);
    }
    drop(s);

    // 幂等：二次解锁不再迁移、无重复产物。
    let s2 = Session::unlock(&vault, HOST_PWD).unwrap();
    let repo = s2.repo().unwrap();
    let again = repo.list_items(None, false, None);
    assert_eq!(
        again.iter().filter(|i| i.title() == "old-attachment").count(),
        1,
        "二次解锁不得重复迁移"
    );
    assert_eq!(count_blob_frames(&vault, &x), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑫ D-60 §3.14.4：GC KeepBoth——双父共享 attachment_id，删光才清 =====

#[test]
fn case12_gc_keeps_shared_blob_while_any_parent_references_it() {
    let dir = temp_dir("case12");
    let vault = dir.join("a.vault");
    let mut session = Session::create(&vault, HOST_PWD, true).unwrap();
    let (src, data) = make_source(&dir, "shared.bin", 128 * 1024, false);

    // 父A 正常挂附件 → attachment_id X。
    let (parent_a, x) = {
        let repo = session.repo_mut().unwrap();
        let parent = repo.create_item(login_draft("keep-both-a")).unwrap();
        let parent = repo.add_attachment(&parent.item_id, &src, "").unwrap();
        (parent.item_id.clone(), attachment_ref(&parent))
    };
    // 模拟 KeepBoth 仲裁产物：父B 与父A 携带同一 attachment_id（§3.14.1 解耦前提）。
    let parent_b = {
        let mut b = session.repo().unwrap().get_item_opt(&parent_a).unwrap().clone();
        b.item_id = uuid::Uuid::new_v4().to_string();
        b.kind = ItemKind::SecureNote(SecureNoteData {
            title: "keep-both-b".to_string(),
            body: String::new(),
        });
        session.repo_mut().unwrap().put_item_direct(b.clone()).unwrap();
        b.item_id
    };

    let repo = session.repo_mut().unwrap();

    // 1) 父A 软删进回收站：trash 快照仍引用 X → blob 保活，GC 空转。
    repo.delete_item(&parent_a, false).unwrap();
    assert!(repo.has_blob(&x), "父A 软删不得清共享 blob");
    assert_eq!(repo.gc_orphan_blobs().unwrap(), 0, "快照引用在，GC 不得动手");

    // 2) 父A 彻底删：父B 仍活跃引用 X → blob 仍保活（KeepBoth 核心断言）。
    let trash_id = repo.list_trash()[0].trash_id.clone();
    repo.purge_item(&trash_id).unwrap();
    assert!(repo.has_blob(&x), "KeepBoth：父B 仍引用，purge 父A 不得清 blob");
    assert_eq!(repo.gc_orphan_blobs().unwrap(), 0);

    // 3) 父B 仍可导出原字节。
    let dest = dir.join("b-export.bin");
    repo.export_attachment(&parent_b, &x, &dest).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    let _ = std::fs::remove_file(&dest);

    // 4) 父B 软删 → trash 快照引用仍在 → blob 保活。
    repo.delete_item(&parent_b, false).unwrap();
    assert!(repo.has_blob(&x), "父B 软删（还原可用）不得清 blob");
    assert_eq!(repo.gc_orphan_blobs().unwrap(), 0);

    // 5) 父B 彻底删：引用集合彻底脱钩 → GC tombstone blob。
    let trash_id = repo.list_trash()[0].trash_id.clone();
    repo.purge_item(&trash_id).unwrap();
    assert!(!repo.has_blob(&x), "两个引用方都 purge 后 GC 应清 blob");
    assert_eq!(repo.gc_orphan_blobs().unwrap(), 0, "已清，二次 GC 空转");

    let _ = std::fs::remove_dir_all(&dir);
}
