//! 冲突检测与三选一仲裁（架构文档 §8.4、§3.9，Q3 回归）。
//!
//! 状态：已在本机实跑通过（`cargo test -p passwordms-core`，见 docs/03b-verification.md）。

use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::sync_state::BaseRev;
use passwordms_core::store::repository::{hash32, item_hash};
use passwordms_core::sync::merge;

fn login(id: &str, title: &str, rev: u64) -> Item {
    Item {
        item_id: id.to_string(),
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
        created_at: 0,
        updated_at: rev as i64,
        rev,
        modified_by: "qa_test".to_string(),
        attachments: Vec::new(),
    }
}

#[test]
fn no_conflict_when_only_one_side_changed() {
    let base = BaseRev {
        rev: 1,
        hash: item_hash(&login("id", "v1", 1)).expect("hash"),
    };
    let local = login("id", "v2", 2);
    let remote = login("id", "v1", 1);
    assert!(!merge::detect_conflict(&local, &remote, Some(&base)).expect("detect"));
}

#[test]
fn no_conflict_when_both_changed_to_same_content() {
    let v1 = login("id", "v1", 1);
    let base = BaseRev {
        rev: 1,
        hash: item_hash(&v1).expect("hash"),
    };
    let same = login("id", "v2", 2);
    assert!(!merge::detect_conflict(&same, &same.clone(), Some(&base)).expect("detect"));
}

#[test]
fn conflict_when_both_changed_differently() {
    let base = BaseRev {
        rev: 1,
        hash: item_hash(&login("id", "v1", 1)).expect("hash"),
    };
    let local = login("id", "local_v2", 2);
    let remote = login("id", "remote_v2", 2);
    assert!(merge::detect_conflict(&local, &remote, Some(&base)).expect("detect"));
}

#[test]
fn conflict_choice_serde() {
    use passwordms_core::model::conflict::ConflictChoice;
    let json = serde_json::to_string(&ConflictChoice::KeepBoth).expect("ser");
    assert_eq!(json, "\"keep_both\"");
}

#[test]
fn hash_is_stable() {
    let a = hash32(b"qa_test_bytes");
    let b = hash32(b"qa_test_bytes");
    let c = hash32(b"qa_test_other");
    assert_eq!(a, b);
    assert_ne!(a, c);
}
