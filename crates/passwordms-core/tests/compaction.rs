//! 容器压缩回归（D-76）。
//!
//! 背景（主人真机提问）：「把 2.4M 的附件删除后，设置里的保险库文件显示还是 24M，
//! 重启也没变化，是不是删掉的附件都还保存在库里？」——**是**。`remove_attachment`
//! 只摘引用（`repository.rs:852`），blob 交孤儿 GC 墓碑化，而墓碑化只置 flag、
//! **密文原样留在文件里**（`tombstone_blob` clone 整帧后原地写回），全库没有任何
//! compaction ⇒ 容器只增不减。
//!
//! 本文件锁死五条契约：
//!
//! 1. **统计说得清**：`compaction_stats` 把「活跃字节 / 墓碑字节 / 其中的已删附件」
//!    分开报出——这是回答主人的第一手证据，必须先于压缩上线；
//! 2. **压缩真回收**：删掉的附件密文在压缩后从文件里消失，文件确实变小；
//! 3. **防回魂（最关键）**：压缩后 ITEM 墓碑仍能被 `export_changes` 导出为
//!    `deleted = true`，且 `tombstone_rev` 仍能取到 rev —— 否则对端一份陈旧 live
//!    变更就能把已删条目复活（`merge.rs:156-176`）；
//! 4. **该留的留**：回收站快照引用的 blob 不得被清（否则还原后附件丢失）；
//!    活跃记录本身、分组、设备、同步状态一个不丢，且**主密码仍能解锁**；
//! 5. **活跃记录的历史墓碑可丢**：`place_frame` 每次长度变化都会留一条中转墓碑，
//!    它们没有任何消费者（导出侧按 `alive` 跳过、`tombstone_rev` 只在记录缺位时查）。

use std::path::{Path, PathBuf};

use passwordms_core::constants::RECORD_TYPE_ITEM;
use passwordms_core::model::conflict::ConflictEntry;
use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::sync_state::DeviceSyncState;
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;
use passwordms_core::store::repository::{hash32, ItemDraft, Repository};

const VAULT_PWD: &str = "Compact#Vault-1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-compact-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// 建库并返回 `(dir, vault_path, session)`。
fn setup(tag: &str) -> (PathBuf, PathBuf, Session) {
    let dir = temp_dir(tag);
    let vault = dir.join("vault.vault");
    let session = Session::create(&vault, VAULT_PWD, true).unwrap();
    (dir, vault, session)
}

fn login_draft(title: &str, notes: Option<&str>) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: Some("https://example.com".to_string()),
            username: Some("user".to_string()),
            password: Some("pw".to_string()),
            notes: notes.map(str::to_string),
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
    }
}

/// 空的对端同步基准（`export_changes` 的 peer base：无过滤 = 全量导出）。
fn empty_peer_base() -> DeviceSyncState {
    DeviceSyncState {
        device_id: "peer-test".to_string(),
        base: Default::default(),
        last_recv_seq: 0,
        last_sent_seq: 0,
    }
}

#[test]
fn stats_separate_live_and_dead_attachment_bytes() {
    let (dir, _vault, mut session) = setup("stats");
    let repo = session.repo_mut().unwrap();

    let item = repo.create_item(login_draft("带附件", None)).unwrap();
    let payload = vec![0x5Au8; 128 * 1024];
    let item = repo.add_attachment_bytes(&item.item_id, "big.bin", &payload).unwrap();
    let attachment_id = item.attachments[0].attachment_id.clone();

    // 附件仍被父条目引用 → 不产生「已删附件」字节。
    let referenced = repo.compaction_stats().unwrap();
    assert_eq!(
        referenced.attachment_dead_bytes, 0,
        "附件还被父条目引用时不应算作死数据"
    );
    assert!(
        referenced.attachment_live_bytes > payload.len() as u64,
        "活跃附件字节应已计入（含 AEAD tag 与帧头），实际 {}",
        referenced.attachment_live_bytes
    );
    assert!(referenced.file_bytes > payload.len() as u64);

    // 摘掉引用（模拟主人「删除附件」）——此时 blob 帧还在，只是没人引用。
    repo.remove_attachment(&item.item_id, &attachment_id).unwrap();

    let after = repo.compaction_stats().unwrap();
    assert!(
        after.attachment_dead_bytes >= payload.len() as u64,
        "删掉的附件密文必须被统计为死数据：期望 ≥ {}，实际 {}",
        payload.len(),
        after.attachment_dead_bytes
    );
    assert!(
        after.reclaimable_bytes >= payload.len() as u64,
        "可回收量必须包含这 {} 字节，实际 {}",
        payload.len(),
        after.reclaimable_bytes
    );
    // 这就是 D-76 要解决的问题本身：**删附件反而让文件变大**（旧父条目帧被置墓碑 +
    // 新父条目帧追加），所以「删了东西文件却没小」不是显示 bug，是设计如此。
    assert!(
        after.file_bytes >= referenced.file_bytes,
        "删附件后文件只增不减：{} → {}",
        referenced.file_bytes,
        after.file_bytes
    );
    assert!(after.dead_frames >= 1);

    cleanup(&dir);
}

#[test]
fn compact_reclaims_deleted_attachment_bytes() {
    let (dir, _vault, mut session) = setup("reclaim");
    let repo = session.repo_mut().unwrap();

    let item = repo.create_item(login_draft("大附件条目", None)).unwrap();
    let payload = vec![0x33u8; 512 * 1024];
    let item = repo
        .add_attachment_bytes(&item.item_id, "huge.bin", &payload)
        .unwrap();
    let attachment_id = item.attachments[0].attachment_id.clone();
    repo.remove_attachment(&item.item_id, &attachment_id).unwrap();
    let before_bytes = repo.file_size().unwrap();
    let stats_before = repo.compaction_stats().unwrap();

    let outcome = repo.compact().unwrap();
    let stats_after = repo.compaction_stats().unwrap();

    // 硬不变量：压缩**只许回收死数据**，活跃帧字节数必须逐字节不变。
    // 这条同时排除两种错误："把活帧也丢了"与"kept 算法算错了"（后者会让
    // `after_bytes` 与磁盘不一致，但 `live_bytes` 仍可能对 —— 反过来也成立）。
    assert_eq!(
        stats_after.live_bytes, stats_before.live_bytes,
        "压缩改变了活跃帧字节数（{} → {}）：有活数据被误伤",
        stats_before.live_bytes, stats_after.live_bytes
    );
    assert_eq!(outcome.before_bytes, before_bytes);
    assert!(
        outcome.reclaimed_bytes >= payload.len() as u64,
        "应至少回收附件明文字节数：期望 ≥ {}，实际 {}",
        payload.len(),
        outcome.reclaimed_bytes
    );
    assert!(outcome.after_bytes < before_bytes);
    assert_eq!(
        outcome.after_bytes,
        repo.file_size().unwrap(),
        "返回的 after_bytes 必须与磁盘一致"
    );

    // 业务数据不受影响：条目还在，附件元数据已消失，blob 不再可见。
    let reloaded = repo.get_item(&item.item_id).unwrap();
    assert!(reloaded.attachments.is_empty());
    assert_eq!(reloaded.title(), "大附件条目");
    assert!(
        !repo.has_blob(&attachment_id),
        "被回收的 blob 不得再出现在索引里"
    );

    cleanup(&dir);
}

/// **防回魂回归**（D-76 红线）：压缩后 ITEM 墓碑仍必须能表达「删除」。
///
/// 两条通道各断言一次：
/// - 导出侧：`export_changes` 必须仍给出 `deleted = true`（否则对端永远收不到删除）；
/// - 合并侧：`tombstone_rev` 必须仍能取到 rev（否则对端陈旧 live 变更会复活条目）。
#[test]
fn compact_keeps_item_tombstone_for_sync() {
    let (dir, _vault, mut session) = setup("tombstone");
    let repo = session.repo_mut().unwrap();

    let item = repo.create_item(login_draft("待删除", None)).unwrap();
    let item_id = item.item_id.clone();
    repo.delete_item(&item_id, true).unwrap();
    assert!(repo.get_item_opt(&item_id).is_none());

    repo.compact().unwrap();

    let changes = repo.export_changes(&empty_peer_base());
    let deletion = changes
        .iter()
        .find(|c| c.record_id == item_id)
        .unwrap_or_else(|| panic!("压缩后必须仍导出该条目的删除变更，实际变更 {} 条", changes.len()));
    assert!(
        deletion.deleted,
        "压缩后该条目的变更必须是 deleted = true"
    );
    assert_eq!(deletion.record_type, RECORD_TYPE_ITEM);

    let tomb_rev = repo
        .tombstone_rev(RECORD_TYPE_ITEM, &item_id)
        .expect("压缩后必须仍能取到墓碑 rev（复活抑制依赖它）");
    assert_eq!(
        tomb_rev, deletion.rev,
        "tombstone_rev 与导出载荷的 rev 必须一致"
    );

    cleanup(&dir);
}

/// **该留的留**：回收站快照引用的 blob 不得被清 —— 否则「还原」会得到一个空附件。
#[test]
fn compact_keeps_blob_referenced_by_trash_snapshot() {
    let (dir, _vault, mut session) = setup("trash");
    let repo = session.repo_mut().unwrap();

    let item = repo.create_item(login_draft("进回收站", None)).unwrap();
    let payload = vec![0x77u8; 96 * 1024];
    let item = repo
        .add_attachment_bytes(&item.item_id, "keep.bin", &payload)
        .unwrap();
    let attachment_id = item.attachments[0].attachment_id.clone();

    // 软删（进回收站）：条目快照仍持有 attachments → blob 仍算「有引用」。
    repo.delete_item(&item.item_id, false).unwrap();
    assert_eq!(repo.list_trash().len(), 1);

    let stats = repo.compaction_stats().unwrap();
    assert_eq!(
        stats.attachment_dead_bytes, 0,
        "回收站快照引用的附件不得被算作死数据（还原需要它）"
    );

    repo.compact().unwrap();
    assert!(
        repo.has_blob(&attachment_id),
        "压缩不得清掉回收站快照引用的 blob"
    );

    // 还原后附件必须仍可用。
    let trash_id = repo.list_trash()[0].trash_id.clone();
    let restored = repo.restore_item(&trash_id).unwrap();
    assert_eq!(restored.attachments.len(), 1);
    assert_eq!(
        repo.read_blob(&attachment_id).unwrap(),
        payload,
        "还原后附件字节必须原样可读"
    );

    // 清空回收站 → 引用消失 → 再压缩就应真回收。
    repo.delete_item(&restored.item_id, false).unwrap();
    repo.empty_trash().unwrap();
    let before_bytes = repo.file_size().unwrap();
    let outcome = repo.compact().unwrap();
    assert!(
        !repo.has_blob(&attachment_id),
        "回收站清空后附件应可回收"
    );
    assert!(outcome.reclaimed_bytes >= payload.len() as u64);
    assert!(outcome.after_bytes < before_bytes);

    cleanup(&dir);
}

/// 活跃记录的历史墓碑可丢，但**记录本身一条不能丢**。
#[test]
fn compact_drops_churn_tombstones_of_live_records() {
    let (dir, _vault, mut session) = setup("churn");
    let repo = session.repo_mut().unwrap();

    let item = repo.create_item(login_draft("会被反复编辑", Some("v0"))).unwrap();
    let item_id = item.item_id.clone();
    // 反复编辑并改变长度 → place_frame 每次留一条中转墓碑（同一 id 多条）。
    for i in 1..=6 {
        let notes = "x".repeat(i * 37);
        repo.update_item(&item_id, login_draft(&format!("标题{i}"), Some(&notes)))
            .unwrap();
    }

    let before = repo.compaction_stats().unwrap();
    assert!(
        before.dead_frames > 0,
        "反复编辑必然留下中转墓碑，实际 dead_frames = {}",
        before.dead_frames
    );

    let outcome = repo.compact().unwrap();
    assert!(outcome.dropped_frames > 0);
    assert!(outcome.after_bytes < outcome.before_bytes);

    // 条目仍在、内容是最新版本，且**不得**出现 deleted 变更。
    let reloaded = repo.get_item(&item_id).unwrap();
    assert_eq!(reloaded.title(), "标题6");
    assert_eq!(reloaded.rev, 7, "create=1 加 6 次更新");
    let changes = repo.export_changes(&empty_peer_base());
    assert!(
        changes
            .iter()
            .filter(|c| c.record_id == item_id)
            .all(|c| !c.deleted),
        "活跃记录不得被导出成删除"
    );
    assert!(
        repo.tombstone_rev(RECORD_TYPE_ITEM, &item_id).is_none(),
        "活跃记录的历史墓碑应已清空（它不表达删除意图）"
    );

    cleanup(&dir);
}

/// 压缩不改数据、不改密钥材料：主密码原样能解锁，各集合计数逐项一致。
#[test]
fn compact_preserves_header_counts_and_master_password() {
    let (dir, vault, mut session) = setup("preserve");
    {
        let repo = session.repo_mut().unwrap();
        let folder = repo
            .create_folder("分组A".to_string(), None, None)
            .unwrap();
        let a = repo.create_item(login_draft("条目A", Some("note-a"))).unwrap();
        let b = repo
            .create_item(ItemDraft {
                folder_id: Some(folder.folder_id.clone()),
                ..login_draft("条目B", None)
            })
            .unwrap();
        let payload = vec![0x11u8; 32 * 1024];
        let b = repo
            .add_attachment_bytes(&b.item_id, "small.bin", &payload)
            .unwrap();
        // 一条进回收站（TRASH 帧必须活下来），一条彻底删除（墓碑必须活下来）。
        repo.delete_item(&b.item_id, false).unwrap();
        repo.delete_item(&a.item_id, true).unwrap();
    }

    let header_before = session.repo().unwrap().header().clone();
    let repo = session.repo().unwrap();
    let counts_before = (
        repo.list_items(None, false, None).len(),
        repo.list_folders().len(),
        repo.list_trash().len(),
        repo.list_conflicts().len(),
        repo.list_paired_devices().len(),
    );
    assert_eq!(counts_before, (0, 1, 1, 0, 0));

    session.repo_mut().unwrap().compact().unwrap();

    let repo = session.repo().unwrap();
    let counts_after = (
        repo.list_items(None, false, None).len(),
        repo.list_folders().len(),
        repo.list_trash().len(),
        repo.list_conflicts().len(),
        repo.list_paired_devices().len(),
    );
    assert_eq!(counts_after, counts_before, "压缩前后各集合计数必须逐项相等");

    let header_after = repo.header().clone();
    // 头部逐字段沿用：主密码相关的三件套一个字节都不能变（否则用户被锁在门外）。
    assert_eq!(header_after.vault_id, header_before.vault_id);
    assert_eq!(header_after.kdf_salt, header_before.kdf_salt);
    assert_eq!(header_after.verify_mac, header_before.verify_mac);
    assert_eq!(
        header_after.wrapped_vault_key, header_before.wrapped_vault_key,
        "wrapped_vault_key 必须原样保留"
    );
    assert_eq!(header_after.kdf, header_before.kdf);
    assert_eq!(header_after.created_at, header_before.created_at);
    assert_eq!(
        header_after.updated_at, header_before.updated_at,
        "压缩不改业务数据，不得刷新 updated_at"
    );
    assert_eq!(header_after.item_count, counts_before.0 as u32);
    assert_eq!(header_after.folder_count, counts_before.1 as u32);

    // 原主密码必须仍能解锁（端到端证明 header 没被破坏）。
    drop(session);
    let reopened = Session::unlock(&vault, VAULT_PWD).expect("压缩后主密码必须仍能解锁");
    assert_eq!(reopened.repo().unwrap().list_folders().len(), 1);
    drop(reopened);
    // 错误密码仍必须失败。
    assert!(Session::unlock(&vault, "wrong-password").is_err());

    // 备份文件不得残留（压缩成功后立即删除）。
    let leftovers: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name()))
        .map(PathBuf::from)
        .filter(|n| n.to_string_lossy().contains("precompact"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "压缩成功后不得残留备份文件，实际: {leftovers:?}"
    );

    cleanup(&dir);
}

/// 无可回收时是幂等空转（不重写文件、不产生副作用）。
#[test]
fn compact_is_noop_when_nothing_to_reclaim() {
    let (dir, _vault, mut session) = setup("noop");
    let repo = session.repo_mut().unwrap();
    repo.create_item(login_draft("干净库", None)).unwrap();

    let before = repo.compaction_stats().unwrap();
    assert_eq!(
        before.reclaimable_bytes, 0,
        "全新库没有死帧，可回收量应为 0（实际 {}）",
        before.reclaimable_bytes
    );

    let first = repo.compact().unwrap();
    assert_eq!(first.reclaimed_bytes, 0);
    assert_eq!(first.before_bytes, first.after_bytes);
    assert_eq!(first.dropped_frames, 0);

    let second = repo.compact().unwrap();
    assert_eq!(second.reclaimed_bytes, 0, "压缩必须幂等");
    assert_eq!(repo.file_size().unwrap(), first.after_bytes);

    cleanup(&dir);
}

/// 造一条未仲裁的冲突记录（两端快照由调用方给定）。
fn add_conflict(repo: &mut Repository, item_id: &str, local: Item, remote: Item) {
    repo.add_conflict(ConflictEntry {
        conflict_id: uuid::Uuid::new_v4().to_string(),
        item_id: item_id.to_string(),
        local_snapshot: local,
        remote_snapshot: remote,
        local_updated_at: 0,
        remote_updated_at: 0,
        detected_at: 0,
        resolved: false,
        remote_deleted: false,
    })
    .unwrap();
}

/// **P0 回归**：冲突快照引用的 blob 必须保活（压缩不得把它当作"无人引用"扔掉）。
///
/// 真实触发路径：电脑删掉附件 A（`attachments = [B]`）、手机同时编辑（`[A, B]`）→ 同步
/// 检出冲突 → `conflicts.remote_snapshot.attachments = [A, B]`。此刻**活条目只引用 B**，
/// A 的引用**只存在于冲突快照里**。若引用集合看不见 `conflicts`，压缩会把 A 的 blob 帧
/// 当垃圾丢掉；用户随后点「采用手机版本」→ 得到一个引用 A 的条目 → `BlobMissing`，
/// **附件永久丢失**（帧已不在文件里，连 GC 都救不回来）。
///
/// 这条比回收站那条更必须：回收站是「用户已经决定删了」，冲突是「用户还没决定」。
#[test]
fn compact_keeps_blob_referenced_by_conflict_snapshot() {
    let (dir, _vault, mut session) = setup("conflict-blob");
    let repo = session.repo_mut().unwrap();

    let a_payload = vec![0xA1u8; 64 * 1024];
    let b_payload = vec![0xB2u8; 16 * 1024];

    let item = repo.create_item(login_draft("冲突条目", None)).unwrap();
    let item = repo
        .add_attachment_bytes(&item.item_id, "a.bin", &a_payload)
        .unwrap();
    let att_a = item.attachments[0].attachment_id.clone();
    let item = repo
        .add_attachment_bytes(&item.item_id, "b.bin", &b_payload)
        .unwrap();
    let att_b = item.attachments[1].attachment_id.clone();

    // 冲突的「远端快照」= 删 A 之前的样子（带 A、B）。
    let remote_snapshot = repo.get_item(&item.item_id).unwrap();
    // 活条目删掉 A：此刻 A 的引用只剩冲突快照这一处。
    let live = repo.remove_attachment(&item.item_id, &att_a).unwrap();
    assert!(live.attachments.iter().all(|x| x.attachment_id != att_a));
    add_conflict(repo, &item.item_id, live.clone(), remote_snapshot);

    assert!(repo.has_blob(&att_a), "构造前提：A 的 blob 帧此刻还在库里");
    let hash_before = hash32(&repo.read_blob(&att_a).unwrap());

    repo.compact().unwrap();

    assert!(
        repo.has_blob(&att_a),
        "冲突快照引用的附件被压缩删掉了 —— 用户一仲裁就会 BlobMissing"
    );
    assert_eq!(
        hash32(&repo.read_blob(&att_a).unwrap()),
        hash_before,
        "保活下来的附件字节必须与压缩前逐字节一致"
    );
    assert!(repo.has_blob(&att_b), "活条目引用的附件当然也要在");

    // 「保活后被引用字节」自洽性：压缩完不该再有可回收的死数据。
    let stats = repo.compaction_stats().unwrap();
    assert_eq!(
        stats.reclaimable_bytes, 0,
        "压缩后不应仍报告可回收字节（说明引用集合与写入集合一致）"
    );
    assert_eq!(stats.file_bytes, repo.file_size().unwrap());

    cleanup(&dir);
}

/// 同根因的另一条出口：`gc_orphan_blobs()` 也不能把冲突快照的附件墓碑掉。
///
/// 它与压缩共用 [`Repository::attachment_blob_refs`]，所以压缩那个洞修好之前，这条
/// 在解锁 / purge / empty_trash 触发的 GC 上同样会丢数据。
#[test]
fn gc_orphan_blobs_keeps_blob_referenced_by_conflict_snapshot() {
    let (dir, _vault, mut session) = setup("conflict-gc");
    let repo = session.repo_mut().unwrap();

    let a_payload = vec![0xC3u8; 32 * 1024];
    let item = repo.create_item(login_draft("冲突条目2", None)).unwrap();
    let item = repo
        .add_attachment_bytes(&item.item_id, "c.bin", &a_payload)
        .unwrap();
    let att_c = item.attachments[0].attachment_id.clone();

    let remote_snapshot = repo.get_item(&item.item_id).unwrap();
    let live = repo.remove_attachment(&item.item_id, &att_c).unwrap();
    add_conflict(repo, &item.item_id, live.clone(), remote_snapshot);

    let collected = repo.gc_orphan_blobs().unwrap();

    assert!(
        repo.has_blob(&att_c),
        "孤儿 GC 把冲突快照引用的附件墓碑掉了（返回 {collected} 个孤儿）"
    );
    assert_eq!(
        repo.read_blob(&att_c).unwrap(),
        a_payload,
        "GC 后附件字节必须仍可读"
    );

    cleanup(&dir);
}

/// **防过修**：引用集合不能退化成"只要有冲突就全保"。
///
/// 构造一个**谁都不引用**的 blob（冲突存在，但两端快照里都没有它），压缩后它必须消失；
/// 同时活条目引用的另一个 blob 必须留下 —— 这才证明集合是**按引用关系**算的。
#[test]
fn compact_drops_blob_referenced_by_nobody() {
    let (dir, _vault, mut session) = setup("nobody");
    let repo = session.repo_mut().unwrap();

    let keep_payload = vec![0x11u8; 8 * 1024];
    let drop_payload = vec![0x99u8; 48 * 1024];

    let item = repo.create_item(login_draft("两边都不引用", None)).unwrap();
    let item = repo
        .add_attachment_bytes(&item.item_id, "keep.bin", &keep_payload)
        .unwrap();
    let att_keep = item.attachments[0].attachment_id.clone();
    let item = repo
        .add_attachment_bytes(&item.item_id, "drop.bin", &drop_payload)
        .unwrap();
    let att_drop = item.attachments[1].attachment_id.clone();

    // 关键顺序：**先删 drop，再取快照** —— 否则快照里仍带着 drop，它就成了「被冲突引用」，
    // 本用例就退化成在测保活而不是测丢弃（第一版就是这么写错的，被本用例自己抓出来）。
    let live = repo.remove_attachment(&item.item_id, &att_drop).unwrap();
    assert!(live.attachments.iter().all(|x| x.attachment_id != att_drop));
    // 冲突两端快照都只含 keep；drop 谁都不引用。
    add_conflict(repo, &item.item_id, live.clone(), live);

    assert!(repo.has_blob(&att_drop), "构造前提：drop 的帧此刻还在");

    repo.compact().unwrap();

    assert!(
        !repo.has_blob(&att_drop),
        "谁都不引用的 blob 必须被压缩丢掉（引用集合被放宽成了「什么都保」？）"
    );
    assert!(
        repo.read_blob(&att_drop).is_err(),
        "被丢掉的 blob 不得仍可读"
    );
    assert!(repo.has_blob(&att_keep), "被引用的 blob 必须留下");
    assert_eq!(repo.read_blob(&att_keep).unwrap(), keep_payload);

    cleanup(&dir);
}

/// 真实库**只读**帧账本（人工触发，CI 不跑）。
///
/// 为什么单独有这个工具：`compaction_stats()` 必须在**解锁态**运行（它要读 `items`
/// 判断哪些 blob/墓碑还活着，所以要么有 VK、要么另开一条无密钥的旁路）。而
/// 「24 MB 里有多少是死帧」这个问题本身**不需要解密**——帧头就带着类型、flags 与
/// 长度。于是这里用 `Container::open` + `read_frames`（两者都是既有公开 API）做一次
/// 纯结构统计，用于独立核对 `compaction_stats` 的输入数字。
///
/// 用法：
/// ```text
/// PMS_REAL_VAULT=<vault 路径> cargo test -p passwordms-core --test compaction \
///     -- --ignored --nocapture real_vault_ledger
/// ```
///
/// 输出只含**尺寸与计数**，不含任何路径片段、UUID、明文或密文。
///
/// # 安全约束（不要把这个工具接成对外服务）
///
/// 本工具只用于**本机开发者自检**。`PMS_REAL_VAULT` 必须指向可信来源（自己机器上的库
/// 路径）。**不得**把它包装成接受任意路径的 IPC 命令 / HTTP 接口 / CLI 子命令暴露出去：
/// 它虽然只输出尺寸与计数，但「能指定任意路径并被远程触发」就等于给攻击者一个
/// **存在性与体积探针**（可对任意路径做存在性判定、对任意库做体积指纹），而我们无法
/// 保证调用方不把返回值当可信信号。要在 UI 里提供同类能力，走
/// `vault_compaction_stats` —— 它只能读**当前会话已解锁的那一个**库，路径不由调用方指定。
#[test]
#[ignore = "人工触发：需要 PMS_REAL_VAULT 指向一个真实库；CI 不跑"]
fn real_vault_ledger() {
    let Ok(path) = std::env::var("PMS_REAL_VAULT") else {
        // 未被显式触发时静默返回（这是工具，不是断言）。
        return;
    };
    let container = Container::open(Path::new(&path)).expect("真实库头部必须可读");
    let header_bytes = container.record_start();
    let file_bytes = container.file_size().unwrap();
    let frames = container.read_frames().expect("帧流必须可完整解码");

    let mut live_bytes = 0u64;
    let mut tombstone_bytes = 0u64;
    let mut blob_live_bytes = 0u64;
    let mut blob_tomb_bytes = 0u64;
    let mut live_frames = 0u32;
    let mut tomb_frames = 0u32;
    for (_, frame) in &frames {
        let size = frame.encoded_len() as u64;
        if frame.is_tombstone() {
            tombstone_bytes += size;
            tomb_frames += 1;
            if frame.is_blob() {
                blob_tomb_bytes += size;
            }
        } else {
            live_bytes += size;
            live_frames += 1;
            if frame.is_blob() {
                blob_live_bytes += size;
            }
        }
    }

    println!("file_bytes        = {file_bytes}");
    println!("header_bytes      = {header_bytes}");
    println!("frames            = {} (live {live_frames} / tomb {tomb_frames})", frames.len());
    println!("live_bytes        = {live_bytes}");
    println!("tombstone_bytes   = {tombstone_bytes}");
    println!("blob_live_bytes   = {blob_live_bytes}");
    println!("blob_tomb_bytes   = {blob_tomb_bytes}");
    println!("projected_after   = {}", header_bytes + live_bytes);

    // 账本必须平：头部 + 全部帧 == 文件长度（否则说明有尾料或解析错位）。
    assert_eq!(
        header_bytes + live_bytes + tombstone_bytes,
        file_bytes,
        "帧账本必须与文件长度相等（存在尾料则说明容器不干净）"
    );
}

