//! D-171 分组同步冲突检测（Kimi 审计 F-01：分组直接覆盖、无冲突检测）。
//!
//! 事故形态：两端各改同一个分组（A 改名、B 换图标/颜色），旧实现 `put_folder_direct`
//! 无条件落地——后同步的一端赢，另一端**看不到任何提示**，改过的名字就这么没了。
//! 条目自 D-56 起就有冲突箱，分组一直缺席（`merge.rs` 的 FOLDER 分支只有复活抑制）。
//!
//! 本套件锁定：
//! ① 双改 → 冲突入箱、本端分组保留（不静默覆盖）；
//! ② 单侧改 → 不报冲突（常规 LWW 传播照旧）；
//! ③ 双改但内容相同 → 不报冲突；
//! ④ KeepLocal → 抬 rev 落地本机版本并上交修正清单（下一轮同步能传出去）；
//! ⑤ TakeRemote → 原样落地远端快照（rev 不高抬，避免双端互采振荡）；
//! ⑥ KeepBoth → 显式拒绝且冲突保持未解决（分组没有「两者都留」语义）；
//! ⑦ 对端删分组 vs 本端改过 → 冲突（remote_deleted），TakeRemote = 接受删除；
//! ⑧ 对端删分组 vs 本端未改 → 直接删，不报冲突；
//! ⑨ 统一冲突箱按 detected_at 倒序混排两类冲突；
//! ⑩ 分组冲突落盘后重开库仍在（新记录类型 0x08 自洽）；
//! ⑪ 更新版本覆盖分组后，旧分组冲突「事实解决」（不残留过时快照）；
//! ⑫ 分组内容摘要口径：只认业务字段（D-109 同款）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use passwordms_core::constants::{RECORD_TYPE_CONFLICT, RECORD_TYPE_FOLDER_CONFLICT};
use passwordms_core::model::conflict::{ConflictChoice, FolderConflictEntry};
use passwordms_core::model::folder::Folder;
use passwordms_core::model::now_ms;
use passwordms_core::model::sync_state::{BaseRev, DeviceSyncState};
use passwordms_core::session::Session;
use passwordms_core::store::repository::{folder_hash, FolderDeleteStrategy};
use passwordms_core::sync::merge;

const PWD_A: &str = "D171#Vault-A1";
const PWD_B: &str = "D171#Vault-B2";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d171-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("passwordms.vault")
}

fn folder(id: &str, name: &str, rev: u64) -> Folder {
    Folder {
        folder_id: id.to_string(),
        name: name.to_string(),
        icon: None,
        color: None,
        sort_order: 0,
        rev,
        updated_at: rev as i64,
        pinned: false,
        pin_order: 0,
    }
}

struct Side {
    session: Session,
    /// 本端「对另一端」的同步状态（base + 发送水位），等价真实链路的 per-peer state。
    state: DeviceSyncState,
}

impl Side {
    fn create(vault: &Path, pwd: &str, device_id: &str) -> Side {
        let session = Session::create(vault, pwd, true).expect("create vault");
        Side {
            session,
            state: DeviceSyncState {
                device_id: device_id.to_string(),
                base: HashMap::new(),
                last_recv_seq: 0,
                last_sent_seq: 0,
                sent_watermark: HashMap::new(),
            },
        }
    }

    fn create_folder(&mut self, name: &str) -> String {
        self.session
            .repo_mut()
            .expect("repo")
            .create_folder(name.to_string(), None, None)
            .expect("create_folder")
            .folder_id
    }

    fn rename_folder(&mut self, folder_id: &str, name: &str) {
        let repo = self.session.repo_mut().expect("repo");
        repo.update_folder(folder_id, name.to_string(), None, None)
            .expect("update_folder");
    }

    fn rename_item(&mut self, item_id: &str, new_title: &str) {
        let repo = self.session.repo_mut().expect("repo");
        let item = repo.get_item(item_id).expect("item");
        let mut kind = item.kind.clone();
        kind.set_title(new_title.to_string());
        repo.update_item(
            item_id,
            passwordms_core::store::repository::ItemDraft {
                kind,
                folder_id: item.folder_id.clone(),
                favorite: item.favorite,
            },
        )
        .expect("update_item");
    }

    fn folder(&self, folder_id: &str) -> Option<Folder> {
        self.session.repo().expect("repo").get_folder_opt(folder_id)
    }

    fn folder_name(&self, folder_id: &str) -> Option<String> {
        self.folder(folder_id).map(|f| f.name)
    }

    fn folder_conflicts(&self) -> Vec<FolderConflictEntry> {
        self.session
            .repo()
            .expect("repo")
            .list_folder_conflicts()
            .into_iter()
            .collect()
    }
}

/// 单向交换：sender 按（对 receiver 的）状态导出 → receiver 合并。
fn pull(receiver: &mut Side, sender: &mut Side) -> merge::MergeReport {
    let changes = sender
        .session
        .repo_mut()
        .expect("repo")
        .export_changes(&sender.state);
    merge::note_sent_changes(&mut sender.state, &changes);
    merge::note_sent_tombstones(&mut sender.state, &changes);
    let rep = merge::apply_changes(
        receiver.session.repo_mut().expect("repo"),
        &mut receiver.state,
        &changes,
    )
    .expect("apply_changes");
    merge::note_received_changes(
        receiver.session.repo().expect("repo"),
        &mut receiver.state,
        &changes,
    );
    rep
}

/// 新建设备端并**采用对端的 VK**（真实配对是把 host 的 VK 用 ECDH 包给 joiner；
/// 这里直接取用，跨端解密才有同一把钥匙）。
fn side_with_vk(vault: &Path, pwd: &str, device_id: &str, host: &Side) -> Side {
    let mut side = Side::create(vault, pwd, device_id);
    let vk = host.session.repo().expect("repo").vault_key().clone();
    side.session.adopt_vault_key(&vk).expect("adopt_vault_key");
    side
}

/// 把两端都推到「同步过一次、分组 rev=1 且双方都有 base」的起点。
fn paired(tag: &str) -> (Side, Side, String) {
    let vault_a = temp_vault(&format!("{tag}-a"));
    let mut a = Side::create(&vault_a, PWD_A, "device-a");
    let folder_id = a.create_folder("Work");
    let mut b = side_with_vk(&temp_vault(&format!("{tag}-b")), PWD_B, "device-b", &a);
    let rep = pull(&mut b, &mut a);
    assert!(rep.conflicts.is_empty(), "起点不该有冲突");
    assert_eq!(b.folder_name(&folder_id).as_deref(), Some("Work"));
    // 再反向同步一次，令 A 也持有对 B 的 base（两端视角一致）。
    pull(&mut a, &mut b);
    (a, b, folder_id)
}

// ── ①②③ 冲突探测 ──

#[test]
fn d171_folder_conflict_when_both_sides_changed() {
    let (mut a, mut b, folder_id) = paired("both-changed");
    // 双改：A 改名、B 同时换颜色（内容都变了）。
    a.rename_folder(&folder_id, "Work-A");
    b.session
        .repo_mut()
        .expect("repo")
        .update_folder(
            &folder_id,
            "Work".to_string(),
            Some("star".to_string()),
            None,
        )
        .expect("update_folder b");

    let rep = pull(&mut b, &mut a);

    assert_eq!(rep.conflicts.len(), 1, "分组双改必须进冲突箱");
    assert_eq!(rep.conflicts[0], folder_id);
    // 本端（B）保留自己的版本，不被对端静默覆盖。
    assert_eq!(b.folder_name(&folder_id).as_deref(), Some("Work"));
    let entry = &b.folder_conflicts()[0];
    assert_eq!(entry.folder_id, folder_id);
    assert!(!entry.remote_deleted);
    assert_eq!(entry.local_snapshot.icon.as_deref(), Some("star"));
    assert_eq!(entry.remote_snapshot.name, "Work-A");
}

#[test]
fn d171_no_conflict_when_only_one_side_changed() {
    let (mut a, mut b, folder_id) = paired("one-side");
    a.rename_folder(&folder_id, "Work-A");

    let rep = pull(&mut b, &mut a);

    assert!(rep.conflicts.is_empty(), "单侧修改不是冲突，走 LWW 传播");
    assert_eq!(b.folder_name(&folder_id).as_deref(), Some("Work-A"));
    assert!(b.folder_conflicts().is_empty());
}

#[test]
fn d171_no_conflict_when_both_changed_to_same_content() {
    let (mut a, mut b, folder_id) = paired("same-content");
    a.rename_folder(&folder_id, "Team");
    b.rename_folder(&folder_id, "Team");

    let rep = pull(&mut b, &mut a);

    assert!(rep.conflicts.is_empty(), "双改但内容一致 ⇒ 无冲突");
    assert_eq!(b.folder_name(&folder_id).as_deref(), Some("Team"));
}

// ── ④⑤⑥ 仲裁 ──

#[test]
fn d171_keep_local_bumps_rev_and_reports_reset() {
    let (mut a, mut b, folder_id) = paired("keep-local");
    a.rename_folder(&folder_id, "Work-A");
    b.rename_folder(&folder_id, "Work-B");
    let rep = pull(&mut b, &mut a);
    assert_eq!(rep.conflicts.len(), 1);

    let conflict_id = b.folder_conflicts()[0].conflict_id.clone();
    let remote_rev = b.folder_conflicts()[0].remote_snapshot.rev;
    let resets = b
        .session
        .repo_mut()
        .expect("repo")
        .resolve_conflict(&conflict_id, ConflictChoice::KeepLocal, " (copy)")
        .expect("resolve keep_local");

    // 抬 rev：必须高于双方 rev，下一轮同步才传得出去（否则 rev ≤ 水位免重发）。
    let landed = b.folder(&folder_id).expect("folder");
    assert_eq!(landed.name, "Work-B");
    assert!(
        landed.rev > remote_rev,
        "KeepLocal 必须抬 rev（{} > {}）",
        landed.rev,
        remote_rev
    );
    assert_eq!(resets.len(), 1, "KeepLocal 必须上交修正清单");
    assert_eq!(resets[0].record_id, folder_id);
    assert_eq!(resets[0].new_rev, landed.rev);
    assert_eq!(resets[0].new_hash, folder_hash(&landed).expect("hash"));
    assert!(b.folder_conflicts().is_empty(), "仲裁后冲突应已解决");
}

#[test]
fn d171_take_remote_lands_remote_snapshot_verbatim() {
    let (mut a, mut b, folder_id) = paired("take-remote");
    a.rename_folder(&folder_id, "Work-A");
    b.rename_folder(&folder_id, "Work-B");
    let rep = pull(&mut b, &mut a);
    assert_eq!(rep.conflicts.len(), 1);
    let conflict_id = b.folder_conflicts()[0].conflict_id.clone();
    let remote_snapshot = b.folder_conflicts()[0].remote_snapshot.clone();

    let resets = b
        .session
        .repo_mut()
        .expect("repo")
        .resolve_conflict(&conflict_id, ConflictChoice::TakeRemote, " (copy)")
        .expect("resolve take_remote");

    let landed = b.folder(&folder_id).expect("folder");
    assert_eq!(landed.name, "Work-A", "采用远端 = 原样落地对方快照");
    assert_eq!(
        landed.rev, remote_snapshot.rev,
        "不得高抬 rev（否则双端互采振荡）"
    );
    assert_eq!(landed.updated_at, remote_snapshot.updated_at);
    assert_eq!(resets.len(), 1);
    assert_eq!(resets[0].new_rev, remote_snapshot.rev);
}

#[test]
fn d171_keep_both_rejected_and_conflict_stays_open() {
    let (mut a, mut b, folder_id) = paired("keep-both");
    a.rename_folder(&folder_id, "Work-A");
    b.rename_folder(&folder_id, "Work-B");
    pull(&mut b, &mut a);
    let conflict_id = b.folder_conflicts()[0].conflict_id.clone();

    let err = b
        .session
        .repo_mut()
        .expect("repo")
        .resolve_conflict(&conflict_id, ConflictChoice::KeepBoth, " (copy)")
        .expect_err("分组不支持 keep_both");
    assert_eq!(err.code(), passwordms_core::error::codes::ERR_VALIDATION);
    // 冲突必须仍在（不能被「试一下」点掉）。
    assert_eq!(b.folder_conflicts().len(), 1, "拒绝后冲突保持未解决");
    assert_eq!(b.folder_name(&folder_id).as_deref(), Some("Work-B"));
}

// ── ⑦⑧ 删除-修改拉锯 ──

#[test]
fn d171_remote_delete_vs_local_edit_becomes_conflict() {
    let (mut a, mut b, folder_id) = paired("del-vs-edit");
    b.rename_folder(&folder_id, "Work-B");
    a.session
        .repo_mut()
        .expect("repo")
        .delete_folder(&folder_id, FolderDeleteStrategy::ToUngrouped)
        .expect("delete folder a");

    let rep = pull(&mut b, &mut a);

    assert_eq!(rep.conflicts.len(), 1, "对端删除 + 本端改过 ⇒ 冲突");
    assert_eq!(
        b.folder_name(&folder_id).as_deref(),
        Some("Work-B"),
        "本端修改不得被静默删掉"
    );
    let entry = &b.folder_conflicts()[0];
    assert!(entry.remote_deleted, "远端版本是墓碑");
    assert_eq!(entry.remote_snapshot.name, "Work", "死亡快照是删除前内容");
}

#[test]
fn d171_remote_delete_accepted_drops_folder() {
    let (mut a, mut b, folder_id) = paired("del-accept");
    b.rename_folder(&folder_id, "Work-B");
    a.session
        .repo_mut()
        .expect("repo")
        .delete_folder(&folder_id, FolderDeleteStrategy::ToUngrouped)
        .expect("delete folder a");
    pull(&mut b, &mut a);
    let conflict_id = b.folder_conflicts()[0].conflict_id.clone();

    b.session
        .repo_mut()
        .expect("repo")
        .resolve_conflict(&conflict_id, ConflictChoice::TakeRemote, " (copy)")
        .expect("resolve accept delete");

    assert!(b.folder(&folder_id).is_none(), "采用远端 = 接受删除");
    assert!(b.folder_conflicts().is_empty());
}

#[test]
fn d171_remote_delete_without_local_edit_applies_directly() {
    let (mut a, mut b, folder_id) = paired("del-clean");
    a.session
        .repo_mut()
        .expect("repo")
        .delete_folder(&folder_id, FolderDeleteStrategy::ToUngrouped)
        .expect("delete folder a");

    let rep = pull(&mut b, &mut a);

    assert!(rep.conflicts.is_empty(), "本端没改过 → 直接删，不打扰用户");
    assert!(b.folder(&folder_id).is_none());
}

// ── ⑨ 统一冲突箱 ──

#[test]
fn d171_unified_box_mixes_items_and_folders_by_time() {
    let (mut a, mut b, folder_id) = paired("unified");
    // 分组冲突（先发生）。
    a.rename_folder(&folder_id, "Work-A");
    b.rename_folder(&folder_id, "Work-B");
    pull(&mut b, &mut a);
    // 条目冲突（后发生）：只在 A 上建条目，同步给 B，再双改。
    let item_id = a
        .session
        .repo_mut()
        .expect("repo")
        .create_item(passwordms_core::store::repository::ItemDraft {
            kind: passwordms_core::model::item::ItemKind::Login(
                passwordms_core::model::item::LoginData {
                    title: "Site".to_string(),
                    url: None,
                    username: None,
                    password: None,
                    notes: None,
                    totp_secret: None,
                    custom_fields: Vec::new(),
                },
            ),
            folder_id: None,
            favorite: false,
        })
        .expect("create_item")
        .item_id;
    pull(&mut b, &mut a);
    pull(&mut a, &mut b);
    a.rename_item(&item_id, "Site-A");
    b.rename_item(&item_id, "Site-B");
    pull(&mut b, &mut a);

    let repo = b.session.repo().expect("repo");
    let views = repo.list_all_conflicts();
    assert_eq!(views.len(), 2, "条目冲突与分组冲突同箱");
    assert!(views
        .windows(2)
        .all(|w| w[0].detected_at() >= w[1].detected_at()));
    assert_eq!(repo.list_conflicts().len(), 1, "条目表只报条目");
    assert_eq!(repo.list_folder_conflicts().len(), 1, "分组表只报分组");
}

// ── ⑩ 落盘 ──

#[test]
fn d171_folder_conflict_survives_reopen() {
    let vault_b = temp_vault("reopen-b");
    let folder_id;
    {
        let mut a = Side::create(&temp_vault("reopen-a"), PWD_A, "device-a");
        folder_id = a.create_folder("Work");
        let mut b = side_with_vk(&vault_b, PWD_B, "device-b", &a);
        pull(&mut b, &mut a);
        pull(&mut a, &mut b);
        a.rename_folder(&folder_id, "Work-A");
        b.rename_folder(&folder_id, "Work-B");
        pull(&mut b, &mut a);
        assert_eq!(
            b.session
                .repo()
                .expect("repo")
                .list_folder_conflicts()
                .len(),
            1
        );
        b.session.lock();
    }

    // 冲突发生在 B 上（B 是本端），重开 B 的库验证落盘；A 那边没有冲突。
    let reopened = Session::unlock(&vault_b, PWD_B).expect("reopen");
    let list = reopened.repo().expect("repo").list_folder_conflicts();
    assert_eq!(list.len(), 1, "分组冲突必须落盘（记录类型 0x08）");
    assert_eq!(list[0].folder_id, folder_id);
    assert_eq!(list[0].local_snapshot.name, "Work-B", "本地端名称随库保留");
    assert_eq!(list[0].remote_snapshot.name, "Work-A");
    assert_ne!(
        RECORD_TYPE_CONFLICT, RECORD_TYPE_FOLDER_CONFLICT,
        "分组冲突必须用独立记录类型（旧版本靠未知类型兜底，整库才打得开）"
    );
}

// ── ⑪ 事实解决 ──

#[test]
fn d171_newer_folder_version_expires_stale_conflict() {
    let (mut a, mut b, folder_id) = paired("expire");
    a.rename_folder(&folder_id, "Work-A");
    b.rename_folder(&folder_id, "Work-B");
    pull(&mut b, &mut a);
    assert_eq!(b.folder_conflicts().len(), 1);

    // 本端再改一次、对端也再改一次 → 又一轮冲突；此后 B 直接接受 A 的最新版本
    // （模拟「更新的版本已覆盖分组本体」），旧冲突应被事实解决而不是继续挂着。
    b.rename_folder(&folder_id, "Work-B2");
    a.rename_folder(&folder_id, "Work-A2");
    pull(&mut b, &mut a);
    // 手工模拟一条「更新的分组版本」到达：把 A 的分组 rev 抬到最高后重放。
    let mut newest = a.folder(&folder_id).expect("folder a");
    newest.rev = 99;
    newest.updated_at = now_ms() + 10_000;
    newest.name = "Work-Final".to_string();
    b.session
        .repo_mut()
        .expect("repo")
        .put_folder_direct(newest)
        .expect("put newest");
    let expired = b
        .session
        .repo_mut()
        .expect("repo")
        .expire_folder_conflicts_for(&folder_id)
        .expect("expire");
    assert!(expired >= 1, "旧分组冲突应被收口");
    assert!(b.folder_conflicts().is_empty());
}

// ── ⑫ 摘要口径 ──

#[test]
fn d171_folder_hash_ignores_sync_metadata() {
    let base = folder("f-1", "Work", 1);
    let mut shifted = folder("f-1", "Work", 42);
    shifted.updated_at = 999_999;
    assert_eq!(
        folder_hash(&base).expect("a"),
        folder_hash(&shifted).expect("b"),
        "D-109：分组摘要不得含同步元数据"
    );
    let mut renamed = base.clone();
    renamed.name = "Personal".to_string();
    assert_ne!(
        folder_hash(&base).expect("a"),
        folder_hash(&renamed).expect("b")
    );
    // 钉住是用户动作，属业务内容（换一台设备钉住必须能被判定为「改过」）。
    let mut pinned = base.clone();
    pinned.pinned = true;
    assert_ne!(
        folder_hash(&base).expect("a"),
        folder_hash(&pinned).expect("b"),
        "pinned 属业务内容"
    );
    let _: BaseRev = BaseRev {
        rev: 1,
        hash: [0u8; 32],
    };
}
