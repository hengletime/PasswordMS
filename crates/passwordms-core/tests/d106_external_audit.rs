//! D-106 回归套件：豆包外部审查（2026-10-27）P2 修复守门。
//!
//! 背景（docs/PasswordMS-审查报告-2026-10-27.md）：六路静态审查发现同步接收路径
//! 缺大小校验（3.1-5/3.1-6）、还原路径缺分组存在性校验（3.1-3）。本套件锁死三条：
//!
//! 1. **同步 blob 帧大小守卫**：`write_blob_frame` 拒绝明文 > 20MB 的帧（与本地
//!    `insert_attachment` 同口径），防止异常配对端把库撑爆；
//! 2. **同步条目大小守卫**：`put_item_direct` 拒绝序列化 > 384KiB 的条目（与本地
//!    create/update 的 `validate_item_kind` 同口径）；
//! 3. **还原悬空分组**：回收站期间分组被删，还原后 `folder_id` 必须置 None
//!    （移入未分组），不得落进悬空分组导致条目在分组视图不可见。
//!
//! 说明（3.2-1 / 3.1-1 / 3.1-2 / 3.1-7 无专属用例的理由）：解锁错误统一化的
//! 篡改头场景需伪造头部完整结构（KDF 参数明文段）且依赖耗时断言，环境噪声大，
//! 由代码审查保证；`sync_all` / `place_frame` / `tombstone` 顺序改动无 IO 失败
//! 注入设施，同上。这些点由 `cargo test -p passwordms-core` 全量 212 例间接回归。

use std::path::PathBuf;

use passwordms_core::constants::{FLAG_BLOB, RECORD_TYPE_ITEM};
use passwordms_core::error::CoreError;
use passwordms_core::model::item::{ItemKind, LoginData, SecureNoteData};
use passwordms_core::model::vault::WrappedKey;
use passwordms_core::session::Session;
use passwordms_core::store::frame::Frame;
use passwordms_core::store::repository::{FolderDeleteStrategy, ItemDraft};

const PWD: &str = "D106#Guard-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d106-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn login_draft(title: &str) -> ItemDraft {
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

/// 3.1-3：回收站期间删除分组 → 还原后 folder_id 置 None（不落悬空分组）。
#[test]
fn restore_moves_item_to_ungrouped_when_folder_deleted() {
    let dir = temp_dir("restore-dangling");
    let path = dir.join("passwordms.vault");
    let mut session = Session::create(&path, PWD, true).unwrap();

    let folder = session
        .repo_mut()
        .unwrap()
        .create_folder("组A".to_string(), None, None)
        .unwrap();
    let mut draft = login_draft("条目");
    draft.folder_id = Some(folder.folder_id.clone());
    let item = session.repo_mut().unwrap().create_item(draft).unwrap();
    assert_eq!(item.folder_id.as_deref(), Some(folder.folder_id.as_str()));

    // 软删 → 删分组（内容移未分组）→ 还原。
    {
        let repo = session.repo_mut().unwrap();
        repo.delete_item(&item.item_id, false).unwrap();
        repo.delete_folder(&folder.folder_id, FolderDeleteStrategy::ToUngrouped)
            .unwrap();
        assert!(
            repo.list_folders()
                .iter()
                .all(|f| f.folder_id != folder.folder_id),
            "分组应已删除"
        );
    }

    let trash_id = {
        let repo = session.repo_mut().unwrap();
        repo.list_trash()
            .iter()
            .find(|t| t.item_snapshot.item_id == item.item_id)
            .expect("软删条目应进回收站")
            .trash_id
            .clone()
    };
    let restored = session.repo_mut().unwrap().restore_item(&trash_id).unwrap();
    assert!(
        restored.folder_id.is_none(),
        "分组已删，还原后必须移入未分组（3.1-3），实际 = {:?}",
        restored.folder_id
    );
}

/// 3.1-5：同步接收的 blob 帧明文 > 20MB 必须拒绝（与本地 insert_attachment 同口径）。
///
/// 守卫在解密之前（`write_blob_frame` 先查大小再落盘），手工构造的帧无需真实密文。
#[test]
fn write_blob_frame_rejects_oversized_plaintext() {
    let dir = temp_dir("blob-too-large");
    let path = dir.join("passwordms.vault");
    let mut session = Session::create(&path, PWD, true).unwrap();

    let oversized = vec![0u8; 20 * 1024 * 1024 + 1];
    let frame = Frame {
        record_type: RECORD_TYPE_ITEM,
        flags: FLAG_BLOB,
        record_id: *uuid::Uuid::new_v4().as_bytes(),
        wrapped_key: WrappedKey {
            nonce: Vec::new(),
            ct: Vec::new(),
        },
        nonce: [0u8; 24],
        plaintext_len: oversized.len() as u32,
        ct: Vec::new(),
    };
    let err = session
        .repo_mut()
        .unwrap()
        .write_blob_frame(frame)
        .expect_err("超限 blob 帧必须被拒");
    assert!(
        matches!(err, CoreError::AttachmentTooLarge),
        "应报 AttachmentTooLarge，实际 = {err:?}"
    );

    // 边界内（1KB）不应触发大小守卫（会在后续 AEAD 步骤失败——但那不是本守卫）。
    // 不做此正向断言：需要真实 VK 加密帧，既有 attach/blob 同步测试已覆盖正向路径。
}

/// 3.1-6：同步直写条目序列化 > 384KiB 必须拒绝（与本地 validate_item_kind 同口径）。
#[test]
fn put_item_direct_rejects_oversized_item() {
    let dir = temp_dir("item-too-large");
    let path = dir.join("passwordms.vault");
    let mut session = Session::create(&path, PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();

    // 正常条目先通过（守卫不误伤）。
    let ok = repo.create_item(login_draft("正常")).unwrap();
    repo.put_item_direct(ok).unwrap();

    // ≈400KB 笔记 → 序列化超 384KiB。
    let big_note = "好".repeat(140 * 1024); // UTF-8 每字 3 字节 ≈ 420KB
    let mut item = repo.create_item(login_draft("巨大")).unwrap();
    item.kind = ItemKind::SecureNote(SecureNoteData {
        title: "巨大笔记".to_string(),
        body: big_note,
    });
    item.rev += 1;
    let err = repo
        .put_item_direct(item)
        .expect_err("超限同步条目必须被拒");
    assert!(
        matches!(err, CoreError::Validation(_)),
        "应报 Validation(item_too_large)，实际 = {err:?}"
    );
}
