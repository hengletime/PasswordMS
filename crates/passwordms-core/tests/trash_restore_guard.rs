//! D-91 回归套件（全仓审查 R2 🔴 / C2）。
//!
//! 覆盖：
//! 1. **R2 还原守卫**：同 item_id 已有活跃条目（对端删除后又编辑、同步复活的等价
//!    形态）时，`restore_item` 必须拒绝（`ERR_RESTORE_CONFLICT`），不得用旧快照
//!    rev+1 + updated_at=now 顶掉新数据（LWW 会把覆盖传播回对端，两端丢数据）；
//! 2. **R2 根因修除**：`drop_trash_snapshots` 清除同 item_id 的陈旧回收站快照；
//! 3. **C2 重复快照**：删除冲突仲裁「采用远端」时，live 已不在且回收站已有同
//!    item_id 快照 → 不再制造重复回收站条目；live 在场 → 软删行为不变。

use std::path::PathBuf;

use passwordms_core::error::CoreError;
use passwordms_core::model::conflict::{ConflictChoice, ConflictEntry};
use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;

const PWD: &str = "D91#Restore-Pw1";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d91-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("d91.vault")
}

fn draft(title: &str) -> ItemDraft {
    ItemDraft {
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
    }
}

/// 造「已删条目 + 已复活 live 新版本」的异常并存形态（R2 的前置场景），
/// 返回（session、item_id、trash_id、复活版 rev）。
fn setup_anomaly(tag: &str) -> (Session, String, String, u64) {
    let path = temp_vault(tag);
    let mut s = Session::create(&path, PWD, true).unwrap();
    let item = s.repo_mut().unwrap().create_item(draft("旧标题")).unwrap();
    s.repo_mut()
        .unwrap()
        .delete_item(&item.item_id, false)
        .unwrap();
    let trash_id = s.repo().unwrap().list_trash()[0].trash_id.clone();

    // 对端删除后又编辑、同步复活（rev 更高 + updated_at 更新）的等价形态。
    let mut revived: Item = item.clone();
    revived.rev = item.rev + 5;
    revived.updated_at = now_ms() + 1000;
    s.repo_mut()
        .unwrap()
        .put_item_direct(revived.clone())
        .unwrap();
    (s, item.item_id, trash_id, revived.rev)
}

#[test]
fn restore_is_rejected_when_live_item_exists() {
    let (mut s, item_id, trash_id, revived_rev) = setup_anomaly("guard");
    let err = s.repo_mut().unwrap().restore_item(&trash_id).unwrap_err();
    assert!(
        matches!(err, CoreError::RestoreConflict),
        "必须返回 RestoreConflict，实得 {err:?}"
    );
    // live 新版本未被旧快照顶掉。
    let live = s.repo().unwrap().get_item(&item_id).unwrap();
    assert_eq!(live.rev, revived_rev, "live 条目必须原样保留");
    // 回收站条目仍在（用户可感知，未静默吞掉）。
    assert_eq!(s.repo().unwrap().list_trash().len(), 1);
}

#[test]
fn revival_cleans_stale_trash_snapshots() {
    let (mut s, item_id, trash_id, _) = setup_anomaly("drop");
    // 正常同步复活路径现在会调 drop_trash_snapshots（merge.rs）；此处直接验证
    // 该方法本身的幂等清理语义。
    let n = s
        .repo_mut()
        .unwrap()
        .drop_trash_snapshots(&item_id)
        .unwrap();
    assert_eq!(n, 1, "应清除恰好一条陈旧快照");
    assert!(s.repo().unwrap().list_trash().is_empty());
    // 快照已清：restore 报 ItemNotFound（而非拿着旧快照覆盖 live）。
    let err = s.repo_mut().unwrap().restore_item(&trash_id).unwrap_err();
    assert!(matches!(err, CoreError::ItemNotFound));
    // 幂等：再清一次返回 0。
    let n = s
        .repo_mut()
        .unwrap()
        .drop_trash_snapshots(&item_id)
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn conflict_take_remote_deleted_skips_duplicate_trash_entry() {
    // 场景：条目已不在 live（对端删除已生效），回收站已有同 item_id 快照，
    // 冲突箱里挂着一条 remote_deleted 冲突。仲裁「采用远端」= 接受删除。
    // 场景：条目已不在 live（对端删除已生效），回收站已有同 item_id 快照，
    // 冲突箱里挂着一条 remote_deleted 冲突。仲裁「采用远端」= 接受删除。
    let path = temp_vault("c2dup");
    let mut s = Session::create(&path, PWD, true).unwrap();
    let item = s.repo_mut().unwrap().create_item(draft("已删的")).unwrap();
    s.repo_mut()
        .unwrap()
        .delete_item(&item.item_id, false)
        .unwrap();
    assert!(s.repo().unwrap().get_item_opt(&item.item_id).is_none());
    let item_id = item.item_id.clone();
    let local_snapshot = s.repo().unwrap().list_trash()[0].item_snapshot.clone();
    let remote_snapshot = {
        let mut r = item.clone();
        r.rev += 1;
        r
    };
    let conflict_id = uuid::Uuid::new_v4().to_string();
    s.repo_mut()
        .unwrap()
        .add_conflict(ConflictEntry {
            conflict_id: conflict_id.clone(),
            item_id: item_id.clone(),
            local_updated_at: local_snapshot.updated_at,
            remote_updated_at: now_ms(),
            local_snapshot,
            remote_snapshot,
            detected_at: now_ms(),
            resolved: false,
            remote_deleted: true,
        })
        .unwrap();

    let _resets = s
        .repo_mut()
        .unwrap()
        .resolve_conflict(&conflict_id, ConflictChoice::TakeRemote)
        .unwrap();

    // 关键断言：不产生第二条重复回收站条目。
    assert_eq!(
        s.repo().unwrap().list_trash().len(),
        1,
        "回收站不得出现同 item_id 的重复条目"
    );
    // live 仍不在（删除意图已生效）。
    assert!(s.repo().unwrap().get_item_opt(&item_id).is_none());
}

#[test]
fn conflict_take_remote_deleted_still_soft_deletes_live_item() {
    // 对照组：live 在场 + remote_deleted 冲突 → 软删行为与 D-56 裁定一致。
    let path = temp_vault("c2live");
    let mut s = Session::create(&path, PWD, true).unwrap();
    let item = s.repo_mut().unwrap().create_item(draft("活着的")).unwrap();
    let remote_snapshot = {
        let mut r = item.clone();
        r.rev += 1;
        r
    };
    let conflict_id = uuid::Uuid::new_v4().to_string();
    s.repo_mut()
        .unwrap()
        .add_conflict(ConflictEntry {
            conflict_id: conflict_id.clone(),
            item_id: item.item_id.clone(),
            local_updated_at: item.updated_at,
            remote_updated_at: now_ms(),
            local_snapshot: item.clone(),
            remote_snapshot,
            detected_at: now_ms(),
            resolved: false,
            remote_deleted: true,
        })
        .unwrap();

    let _resets = s
        .repo_mut()
        .unwrap()
        .resolve_conflict(&conflict_id, ConflictChoice::TakeRemote)
        .unwrap();

    assert!(s.repo().unwrap().get_item_opt(&item.item_id).is_none());
    assert_eq!(
        s.repo().unwrap().list_trash().len(),
        1,
        "live 在场时正常软删入回收站"
    );
}
