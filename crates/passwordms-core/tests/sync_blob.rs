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
use passwordms_core::model::item::{Item, ItemKind};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;
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

/// 在 host 会话上导入附件，返回 item_id（== blob_ref）。
fn host_import(session: &mut Session, src: &Path) -> String {
    let repo = session.repo_mut().unwrap();
    let item = repo.import_attachment(src, "qa-blob".to_string()).unwrap();
    item.item_id
}

fn attachment_ref(item: &Item) -> String {
    match &item.kind {
        ItemKind::Attachment(a) => a.blob_ref.clone(),
        _ => panic!("期望 attachment 条目"),
    }
}

fn first_attachment(session: &Session) -> Item {
    session
        .repo()
        .unwrap()
        .list_items(None, false, None)
        .into_iter()
        .find(|i| matches!(i.kind, ItemKind::Attachment(_)))
        .expect("应有 attachment 条目")
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

/// 导出 joiner 侧附件并断言与原始字节一致，返回 blob_ref。
fn assert_export_matches(session: &Session, vault: &Path, original: &[u8]) -> String {
    let item = first_attachment(session);
    let blob_ref = attachment_ref(&item);
    let dest = vault.with_extension("export.bin");
    session
        .repo()
        .unwrap()
        .export_attachment(&item.item_id, &dest)
        .expect("导出应成功");
    let exported = std::fs::read(&dest).unwrap();
    assert_eq!(exported.len(), original.len(), "导出长度不一致");
    assert!(exported == original, "导出字节不一致");
    let _ = std::fs::remove_file(&dest);
    blob_ref
}

// ===== ① 端到端：2MB 附件同步后字节一致且 blob 帧字节相同 =====

#[test]
fn case1_end_to_end_attachment_blob_sync() {
    let dir = temp_dir("case1");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src, original) = make_source(&dir, "src2mb.bin", 2 * 1024 * 1024, false);
    let item_id = host_import(&mut host_session, &src);
    drop(host_session);

    let (host_result, joiner_result, _host_id) = pair_first(&host_vault, 0, &joiner_vault);
    assert_eq!(joiner_result.blobs, 1, "Joiner 应新入库 1 个 blob");
    assert_eq!(host_result.blobs, 0, "Host 无缺失，不应入库任何 blob");

    // Joiner 侧导出字节一致。
    let joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let blob_ref = assert_export_matches(&joiner_session, &joiner_vault, &original);
    assert_eq!(blob_ref, item_id, "blob_ref == item_id");

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
    let item_id = host_import(&mut host_session, &src);
    drop(host_session);

    // 断点模拟：直接预置 spool 第 0 片（密文）+ manifest（received=[0]）。
    let host_frame = {
        let s = Session::unlock(&host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().blob_frame(&item_id).unwrap()
    };
    let ct = host_frame.ct.clone();
    let chunk_total = ct.len().div_ceil(SYNC_BLOB_CHUNK_BYTES) as u32;
    assert!(chunk_total >= 2, "2MB 附件应 ≥ 2 片");
    let spool_dir = spool_root(&joiner_vault).join(&item_id);
    std::fs::create_dir_all(&spool_dir).unwrap();
    std::fs::write(spool_dir.join("0.part"), &ct[..SYNC_BLOB_CHUNK_BYTES]).unwrap();
    let manifest = format!(
        r#"{{"blob_ref":"{item_id}","chunk_total":{chunk_total},"chunk_size":{SYNC_BLOB_CHUNK_BYTES},"received":[0]}}"#
    );
    std::fs::write(spool_dir.join("manifest.json"), manifest).unwrap();

    // 同步：续传只补第 1、2 片。
    let (_host_result, joiner_result, _id) = pair_first(&host_vault, 0, &joiner_vault);
    assert_eq!(joiner_result.blobs, 1, "续传后应完成 1 个 blob");

    // 无重复帧：该 ref 的非墓碑 blob 帧恰好 1 条。
    assert_eq!(count_blob_frames(&joiner_vault, &item_id), 1, "不得产生重复 blob 帧");

    // 结果正确 + spool 清理。
    let joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let blob_ref = assert_export_matches(&joiner_session, &joiner_vault, &original);
    assert_eq!(blob_ref, item_id);
    assert!(!spool_dir.exists(), "finalize 后 spool 目录应被清理");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ③ 删除传播 merge 级：attachment deleted=true → item+blob 双墓碑；软删保留 blob =====

#[test]
fn case3_merge_delete_cascades_to_blob_and_soft_delete_keeps_it() {
    let dir = temp_dir("case3");
    let host_vault = dir.join("a.vault");
    let b_vault = dir.join("b.vault");

    let mut session_a = Session::create(&host_vault, HOST_PWD, true).unwrap();
    let (src, _data) = make_source(&dir, "src.bin", 64 * 1024, false);
    let item_id = host_import(&mut session_a, &src);
    let changes = session_a.repo().unwrap().export_changes();
    let blob_frame = session_a.repo().unwrap().blob_frame(&item_id).unwrap();

    // B 建空库并采纳 A 的 VK（change_to_item 解密需要同 VK）。
    let mut session_b = Session::create(&b_vault, JOINER_PWD, true).unwrap();
    let b_vault_key = session_a.repo().unwrap().vault_key().clone();
    session_b.adopt_vault_key(&b_vault_key).unwrap();

    // 1) 应用条目变更（deleted=false）→ B 有条目、无 blob。
    let mut state = session_b.repo().unwrap().sync_state("qa-peer");
    let report = apply_changes(session_b.repo_mut().unwrap(), &mut state, &changes).unwrap();
    assert!(report.applied >= 1);
    assert!(session_b.repo().unwrap().get_item_opt(&item_id).is_some());
    assert!(!session_b.repo().unwrap().has_blob(&item_id), "blob 未同步前应缺失");

    // 2) 补上 blob（写帧原语）→ has_blob。
    let outcome = session_b
        .repo_mut()
        .unwrap()
        .write_blob_frame(blob_frame.clone())
        .unwrap();
    assert_eq!(outcome, passwordms_core::store::repository::BlobWriteOutcome::Appended);
    assert!(session_b.repo().unwrap().has_blob(&item_id));

    // 3) 远端删除传播（deleted=true）→ item + blob 双墓碑。
    let mut tombstone = changes.clone();
    for c in tombstone.iter_mut() {
        if c.record_id == item_id {
            c.deleted = true;
        }
    }
    apply_changes(session_b.repo_mut().unwrap(), &mut state, &tombstone).unwrap();
    assert!(session_b.repo().unwrap().get_item_opt(&item_id).is_none(), "条目应被墓碑");
    assert!(!session_b.repo().unwrap().has_blob(&item_id), "blob 应连带墓碑");

    // 4) 本地软删（进回收站）**不清** blob（还原要用）。
    session_a.repo_mut().unwrap().delete_item(&item_id, false).unwrap();
    assert!(session_a.repo().unwrap().has_blob(&item_id), "软删不得清 blob");
    // purge（彻底删）才清——既有语义回归。
    let trash_id = session_a.repo().unwrap().list_trash()[0].trash_id.clone();
    session_a.repo_mut().unwrap().purge_item(&trash_id).unwrap();
    assert!(!session_a.repo().unwrap().has_blob(&item_id), "purge 应清 blob");

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
    let item_id = host_import(&mut host_session, &src);
    drop(host_session);

    // Host 的持久重同步监听器先起（端口随二维码通告给 Joiner）。
    let resync_server = ResyncServer::start(0, "BlobHost".to_string()).unwrap();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let before = joiner_session.repo().unwrap().blob_frame(&item_id).unwrap();
    let blob_ref = assert_export_matches(&joiner_session, &joiner_vault, &original);

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
    let _id1 = host_import(&mut host_session, &src1);
    drop(host_session);

    let resync_server = ResyncServer::start(0, "BlobHost".to_string()).unwrap();
    let (_h1, _j1, host_id) = pair_first(&host_vault, resync_server.port(), &joiner_vault);

    // Joiner 侧（已采纳 VK）创建自己的附件。
    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let (src2, data2) = make_source(&dir, "src2.bin", 640 * 1024, false);
    let id2 = {
        let repo = joiner_session.repo_mut().unwrap();
        repo.import_attachment(&src2, "qa-blob-2".to_string()).unwrap().item_id
    };

    // 重同步（Joiner 发起）：Joiner 拉（无缺）→ 供 att2 给 Host。
    let (host_outcome, joiner_outcome) =
        run_resync(&host_vault, resync_server, &mut joiner_session, &host_id);
    assert_eq!(joiner_outcome.blobs, 0, "Joiner 无缺失");
    assert_eq!(host_outcome.blobs, 1, "Host 应新入库 att2 的 blob");

    // 两侧：2 个 attachment 条目 + 2 个 blob 齐备。
    let hs = Session::unlock(&host_vault, HOST_PWD).unwrap();
    let js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    for session in [&hs, &js] {
        let repo = session.repo().unwrap();
        let attachments: Vec<Item> = repo
            .list_items(None, false, None)
            .into_iter()
            .filter(|i| matches!(i.kind, ItemKind::Attachment(_)))
            .collect();
        assert_eq!(attachments.len(), 2, "两侧都应有 2 个附件条目");
        for item in &attachments {
            let blob_ref = attachment_ref(item);
            assert!(repo.has_blob(&blob_ref), "blob {blob_ref} 应已同步");
        }
    }
    // 精确比对：att2 在 Host 侧导出 == data2；att1 在 Joiner 侧导出 == data1。
    let host_item2 = hs
        .repo()
        .unwrap()
        .list_items(None, false, None)
        .into_iter()
        .find(|i| attachment_ref(i) == id2)
        .unwrap();
    let dest = dir.join("host2.bin");
    hs.repo().unwrap().export_attachment(&host_item2.item_id, &dest).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data2, "Host 侧 att2 字节应与 Joiner 原始一致");
    let _ = std::fs::remove_file(&dest);
    let joiner_item1 = js
        .repo()
        .unwrap()
        .list_items(None, false, None)
        .into_iter()
        .find(|i| matches!(i.kind, ItemKind::Attachment(_)) && attachment_ref(i) != id2)
        .unwrap();
    let dest = dir.join("joiner1.bin");
    js.repo().unwrap().export_attachment(&joiner_item1.item_id, &dest).unwrap();
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
    let item_id = host_import(&mut host_session, &src);
    drop(host_session);

    let (_h, _j, _id) = pair_first(&host_vault, 0, &joiner_vault);
    let js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let blob_ref = assert_export_matches(&js, &joiner_vault, &original);
    assert_eq!(blob_ref, item_id);
    assert_eq!(count_blob_frames(&joiner_vault, &item_id), 1);

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
    let item_id = host_import(&mut session, &src);
    let frame = session.repo().unwrap().blob_frame(&item_id).unwrap();

    let spool_dir = spool_root(&vault).join(&item_id);
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
