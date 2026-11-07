//! D-205：本端已删 + 远端改名（delete-vs-update）必须进冲突箱，不许静默复活。
//!
//! 事故形态（走查单 3.81/3.84，真机 1.6.6）：A 端删分组 X，B 端把同一个 X 改名成 Y，
//! **B（存活方）先发起同步**——B 的 live 改名先到 A，A 这边
//! `merge.rs` 的 FOLDER-`None` 分支无条件 `put_folder_direct`，分组当场复活成 Y；
//! 随后 A 的墓碑因为「该 id 已活跃 → 历史墓碑不导出」永远发不出去，删除意图被吞。
//!
//! 与 `d171_folder_conflict.rs` 的 ⑦ 方向**相反**：那边是墓碑先到（本端 live rev 更高
//! → `remote_deleted` 冲突，已正确）；这边是 live 先到，旧实现没有任何判定。
//!
//! 本套件锁定：
//! ① 存活方先推 live → 删除方必须进冲突箱（`local_deleted`），且本端保持已删；
//! ② `KeepLocal`（= 确认删除）解完仍然是删，且墓碑仍可导出（删除意图没被吞）；
//! ③ `TakeRemote`（= 接受复活）解完分组复活且名字是远端改名后的 `Work-B`。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use passwordms_core::model::conflict::{ConflictChoice, FolderConflictEntry};
use passwordms_core::model::folder::Folder;
use passwordms_core::model::sync_state::DeviceSyncState;
use passwordms_core::session::Session;
use passwordms_core::store::repository::FolderDeleteStrategy;
use passwordms_core::sync::merge;

const PWD_A: &str = "D205#Vault-A1";
const PWD_B: &str = "D205#Vault-B2";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d205-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("passwordms.vault")
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
        self.session
            .repo_mut()
            .expect("repo")
            .update_folder(folder_id, name.to_string(), None, None)
            .expect("update_folder");
    }

    fn delete_folder(&mut self, folder_id: &str) {
        self.session
            .repo_mut()
            .expect("repo")
            .delete_folder(folder_id, FolderDeleteStrategy::ToUngrouped)
            .expect("delete_folder");
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

    fn resolve(
        &mut self,
        conflict_id: &str,
        choice: ConflictChoice,
    ) -> Vec<passwordms_core::model::conflict::WatermarkReset> {
        self.session
            .repo_mut()
            .expect("repo")
            .resolve_conflict(conflict_id, choice, " (copy)")
            .expect("resolve_conflict")
    }
}

/// 单向交换：sender 按（对 receiver 的）状态导出 → receiver 合并（与 d171 同款）。
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

fn side_with_vk(vault: &Path, pwd: &str, device_id: &str, host: &Side) -> Side {
    let mut side = Side::create(vault, pwd, device_id);
    let vk = host.session.repo().expect("repo").vault_key().clone();
    side.session.adopt_vault_key(&vk).expect("adopt_vault_key");
    side
}

/// 起点：两端同步过一次、分组 rev=1 且双方视角一致（照抄 d171 的 paired）。
fn paired(tag: &str) -> (Side, Side, String) {
    let vault_a = temp_vault(&format!("{tag}-a"));
    let mut a = Side::create(&vault_a, PWD_A, "device-a");
    let folder_id = a.create_folder("Work");
    let mut b = side_with_vk(&temp_vault(&format!("{tag}-b")), PWD_B, "device-b", &a);
    let rep = pull(&mut b, &mut a);
    assert!(rep.conflicts.is_empty(), "起点不该有冲突");
    assert_eq!(b.folder_name(&folder_id).as_deref(), Some("Work"));
    pull(&mut a, &mut b);
    (a, b, folder_id)
}

/// 复现顺序：B（存活方）改名 → A（删除方）删除 → **B 先把 live 改名推给 A**。
fn delete_vs_rename(tag: &str) -> (Side, Side, String, merge::MergeReport) {
    let (mut a, mut b, folder_id) = paired(tag);
    b.rename_folder(&folder_id, "Work-B");
    a.delete_folder(&folder_id);
    let rep = pull(&mut a, &mut b);
    (a, b, folder_id, rep)
}

#[test]
fn d205_local_delete_vs_remote_rename_becomes_conflict() {
    let (mut a, mut b, folder_id) = paired("conflict");
    b.rename_folder(&folder_id, "Work-B");
    a.delete_folder(&folder_id);

    let rep = pull(&mut a, &mut b);

    assert_eq!(
        rep.conflicts.len(),
        1,
        "本端已删 + 远端改名必须进冲突箱，不许静默复活"
    );
    assert_eq!(rep.conflicts[0], folder_id);
    assert!(
        a.folder(&folder_id).is_none(),
        "冲突期间本端保持已删（不得被远端 live 静默复活）"
    );

    let entries = a.folder_conflicts();
    assert_eq!(entries.len(), 1, "冲突必须落进分组冲突箱");
    let entry = &entries[0];
    assert!(entry.local_deleted, "本端侧是墓碑（删除前的死亡快照）");
    assert!(!entry.remote_deleted, "远端侧是活跃改名，不是删除");
    assert_eq!(entry.local_snapshot.name, "Work", "本端侧是删除前最后形态");
    assert_eq!(entry.remote_snapshot.name, "Work-B", "远端侧是改名后的名字");
    assert_eq!(entry.folder_id, folder_id);

    // 反方向仍应保持既有语义：B 那边这轮什么都没收到，分组照旧活着。
    assert_eq!(b.folder_name(&folder_id).as_deref(), Some("Work-B"));
}

#[test]
fn d205_keep_local_confirms_deletion_and_bumps_tombstone_over_the_line() {
    let (mut a, _b, folder_id, rep) = delete_vs_rename("keep-local");
    assert_eq!(rep.conflicts.len(), 1);
    let remote_rev = a.folder_conflicts()[0].remote_snapshot.rev;
    let conflict_id = a.folder_conflicts()[0].conflict_id.clone();

    let resets = a.resolve(&conflict_id, ConflictChoice::KeepLocal);

    assert!(
        a.folder(&folder_id).is_none(),
        "KeepLocal = 确认删除，解完仍然是删"
    );
    assert!(a.folder_conflicts().is_empty(), "仲裁后冲突应已解决");

    // 「确认删除」必须落成**再删一次、rev 压过输的那一方**。
    // 不能省这一步：`tombstone()` 不推进 rev，沿用删前那枚墓碑的话，per-peer base
    // 早已等值，墓碑会被 `base.rev > rev` 之外的过滤永远扣下 —— 删除决定传不到对端，
    // 对端那张卡原地不动，两端静默分叉。
    assert_eq!(resets.len(), 1, "确认删除要产生一次水位修正（新墓碑 rev）");
    assert_eq!(resets[0].record_id, folder_id);
    assert!(
        resets[0].new_rev > remote_rev,
        "新墓碑 rev 必须压过远端活版本：got {} vs remote {}",
        resets[0].new_rev,
        remote_rev
    );

    // 删除意图不能被吞：墓碑必须仍能导出给对端（下一轮同步才能传到 B）。
    let changes = a.session.repo_mut().expect("repo").export_changes(&a.state);
    let tomb = changes
        .iter()
        .find(|c| {
            c.record_type == passwordms_core::constants::RECORD_TYPE_FOLDER
                && c.record_id == folder_id
                && c.deleted
        })
        .expect("确认删除后墓碑必须仍可导出（否则两侧永久静默分叉）");
    assert_eq!(tomb.rev, resets[0].new_rev, "导出的必须是抬过线的新墓碑");
}

#[test]
fn d205_take_remote_revives_folder_with_remote_name() {
    let (mut a, _b, folder_id, rep) = delete_vs_rename("take-remote");
    assert_eq!(rep.conflicts.len(), 1);
    let conflict_id = a.folder_conflicts()[0].conflict_id.clone();
    let remote_snapshot = a.folder_conflicts()[0].remote_snapshot.clone();

    let resets = a.resolve(&conflict_id, ConflictChoice::TakeRemote);

    let landed = a.folder(&folder_id).expect("采用远端 = 接受复活");
    assert_eq!(landed.name, "Work-B", "复活后是远端改名后的名字");
    assert_eq!(
        landed.rev, remote_snapshot.rev,
        "采用远端原样落地，不高抬 rev"
    );
    assert_eq!(landed.updated_at, remote_snapshot.updated_at);
    assert!(a.folder_conflicts().is_empty());
    assert_eq!(resets.len(), 1);
    assert_eq!(resets[0].record_id, folder_id);
    assert_eq!(resets[0].new_rev, remote_snapshot.rev);
}

/// D-205 收敛性（删除方仲裁）：在 A 上选「确认删除」之后，这个决定必须**真的落到** B，
/// 且 B 端不该被再开一张一模一样的卡。
///
/// 为什么非要有这一条：上面那条 `d205_keep_local_confirms_deletion_and_tombstone_still_exports`
/// 只证明「墓碑还导出得出去」（单端视角）。导出得出去 ≠ 对端会接受。把 `KeepLocal` 改成
/// 什么都不做，那条用例照样全绿——它是**装饰**，这条才是判据。
///
/// 机理：墓碑的 rev 冻结在删除前那一版（`tombstone()` 不推进 rev），而 B 手里是改名后的
/// 更高 rev。若仲裁时不把这个决定抬到过线，B 收到墓碑只会再判一次 delete-vs-update，
/// **每轮同步重新开一张卡**，永远收敛不了。
#[test]
fn d205_keep_local_decision_reaches_peer_without_another_conflict() {
    let (mut a, mut b, folder_id, rep) = delete_vs_rename("converge");
    assert_eq!(rep.conflicts.len(), 1, "起点必须已经有一张卡");

    let conflict_id = a.folder_conflicts()[0].conflict_id.clone();
    a.resolve(&conflict_id, ConflictChoice::KeepLocal);
    assert!(a.folder(&folder_id).is_none(), "仲裁后本端仍然是删");

    // 真实同步是双向的：A 先把「确认删除」推给 B。
    let rep_ab = pull(&mut b, &mut a);

    assert!(
        b.folder(&folder_id).is_none(),
        "裁决必须真的落地到对端：B 端该分组应当已被删除"
    );
    assert!(
        rep_ab.conflicts.is_empty(),
        "对端收到已仲裁的删除不该再开一张卡（否则每轮同步都重开，永不收敛）：{:?}",
        rep_ab.conflicts
    );

    // 回声再走一轮，两端都不许冒出新卡。
    let rep_ba = pull(&mut a, &mut b);
    assert!(
        a.folder(&folder_id).is_none() && b.folder(&folder_id).is_none(),
        "两端都应保持已删"
    );
    assert!(rep_ba.conflicts.is_empty(), "回声不该再开卡");
    assert!(
        a.folder_conflicts().is_empty() && b.folder_conflicts().is_empty(),
        "仲裁完成后两端冲突箱都该是空的"
    );
}

/// D-205 去重（防御）：同一个远端 live 版本被**重复投递**（重新配对 / 水位重置 /
/// resync 重放）时，不能每投递一次就多开一张卡——否则用户会看到同一个分组挂一串卡，/// 而用户在冲突箱里看到一叠一模一样的卡，只会以为软件坏了。
#[test]
fn d205_repeated_apply_does_not_pile_up_conflicts() {
    let (mut a, mut b, folder_id) = paired("dedupe");
    b.rename_folder(&folder_id, "Work-B");
    a.delete_folder(&folder_id);

    // 同一批变更投递两次（第一次之后 B 的 base 会推进，这里直接把 B 的 base 清掉
    // 来模拟"水位被重置/重新配对"，让第二批必须重发）。
    let rep1 = pull(&mut a, &mut b);
    assert_eq!(rep1.conflicts.len(), 1);
    b.state.base.clear();
    b.state.sent_watermark.clear();
    let rep2 = pull(&mut a, &mut b);

    assert_eq!(
        a.folder_conflicts().len(),
        1,
        "同一个分组的 delete-vs-update 冲突只许有一条，实际 {}",
        a.folder_conflicts().len()
    );
    assert!(
        rep2.conflicts.is_empty(),
        "第二条同内容的变更不该再报一次冲突：{:?}",
        rep2.conflicts
    );
    assert!(a.folder(&folder_id).is_none(), "重复投递不许把分组复活");
}

/// D-205 走查单 3.81 的真实路径：用户在 **B（存活方）** 上点「采用远端」接受删除，
/// A（删除方）那张卡必须跟着自动收口，两端都收敛到「已删」。
///
/// 为什么单独钉这一条：3.81 的判据是「在 B 上同步 → 看卡片 → 点采用远端」，
/// 用户**不会**特意跑去 A 再点一次。若 A 那张卡挂在那里不动，用户下次打开 A 会看到
/// 一条早就处理完的陈旧冲突，只能自己猜该不该再点一遍。
#[test]
fn d205_resolving_on_peer_side_closes_both_ends() {
    let (mut a, mut b, folder_id) = paired("peer-resolve");
    b.rename_folder(&folder_id, "Work-B");
    a.delete_folder(&folder_id);

    // 第一轮 B → A：改名先到，A（删除方）开卡且保持已删。
    let rep_ab = pull(&mut a, &mut b);
    assert_eq!(rep_ab.conflicts.len(), 1, "A 端应开一张卡（本端已删）");
    assert!(a.folder(&folder_id).is_none());

    // 第二轮 A → B：墓碑到，B 开自己那张（远端已删除）——3.81 里用户看到的就是这张。
    let rep_ba = pull(&mut b, &mut a);
    assert_eq!(
        rep_ba.conflicts.len(),
        1,
        "B 端应同时有一张卡（远端已删除）"
    );
    assert_eq!(a.folder_conflicts().len(), 1, "两端各一条，不是第三条");
    assert_eq!(b.folder_conflicts().len(), 1);

    // 用户在 B 上点「采用远端」= 接受删除。
    let b_conflict = b.folder_conflicts()[0].conflict_id.clone();
    assert!(
        b.folder_conflicts()[0].remote_deleted,
        "B 侧必须是「远端已删除」形态，否则按钮语义就反了"
    );
    b.resolve(&b_conflict, ConflictChoice::TakeRemote);
    assert!(b.folder(&folder_id).is_none(), "B 接受删除后分组应消失");

    // 决定同步回 A：A 那张卡必须自动收口。
    let rep_back = pull(&mut a, &mut b);
    assert!(a.folder(&folder_id).is_none(), "A 端仍是已删，不许被复活");
    assert!(
        rep_back.conflicts.is_empty(),
        "对端不该再开新卡：{:?}",
        rep_back.conflicts
    );
    assert!(
        a.folder_conflicts().is_empty(),
        "A 端那张卡必须随对端裁决自动收口，否则用户会在 A 上看到一条陈旧冲突"
    );
}
