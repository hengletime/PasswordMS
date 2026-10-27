//! `Repository::add_attachment_bytes` 契约回归（D-66）。
//!
//! 为什么这个文件是必需的：D-66 的修法是「Android 上 `content://` 走 `add_attachment_bytes`」，
//! 也就是说**手机上每一次加附件都只经过这一条入口**。改动前它零测试覆盖 —— 它只在真机上
//! 被调用，CI 里从没被调用过，属于「关键路径无测试」。本文件把它钉在 CI 里。
//!
//! 覆盖：
//! a) 正常字节落库：父 `attachments` +1、`size_bytes` / `blob_size_bytes` 正确、
//!    `rev` +1、文件名与 MIME 保留、blob 可原样读回、重新解锁后仍在；
//! b) 超 [`MAX_ATTACHMENT_BYTES`] 的字节 → `AttachmentTooLarge`；
//! c) 超 [`MAX_ATTACHMENT_FILENAME_BYTES`] 的文件名 → `Validation("filename_too_long")`；
//! d) 父条目不存在 → `ItemNotFound`；
//! e) 校验**顺序**与 `add_attachment(source_path)` 一致：同时「超大 + 超长名」时报
//!    `AttachmentTooLarge`（大小排在文件名之前），与 `insert_attachment` 文档的分歧点
//!    逐条对齐；空字节附件是合法的（0 字节文件在手机上真实存在）。
//!
//! 说明：c/d/e 里都没有真正写 20 MB 以上的数据以外的大对象，唯一的大分配是 b 的
//! `MAX_ATTACHMENT_BYTES + 1`，与真机路径的内存量级一致。

use std::path::{Path, PathBuf};

use passwordms_core::constants::{
    AEAD_TAG_LEN, MAX_ATTACHMENT_BYTES, MAX_ATTACHMENT_FILENAME_BYTES,
};
use passwordms_core::error::CoreError;
use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;

const VAULT_PWD: &str = "Bytes#Att-1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-attbytes-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
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

/// 建库并返回 `(dir, vault_path)`。
fn setup(tag: &str) -> (PathBuf, PathBuf) {
    let dir = temp_dir(tag);
    let vault = dir.join("vault.vault");
    Session::create(&vault, VAULT_PWD, true).unwrap();
    (dir, vault)
}

#[test]
fn bytes_entry_persists_blob_and_bumps_parent_rev() {
    let (dir, vault) = setup("ok");
    let payload = b"report body \xF0\x9F\x93\x84 with non-ascii".to_vec();
    let item_id;
    let attachment_id;
    {
        let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
        let repo = session.repo_mut().unwrap();
        let parent = repo.create_item(login_draft("bytes-parent")).unwrap();
        assert_eq!(parent.attachments.len(), 0);
        assert_eq!(parent.rev, 1, "新建条目起始 rev 应为 1");

        let updated = repo
            .add_attachment_bytes(&parent.item_id, "报告.pdf", &payload)
            .unwrap();

        assert_eq!(updated.attachments.len(), 1, "父条目 attachments 应 +1");
        assert_eq!(updated.rev, parent.rev + 1, "父条目 rev 必须 +1");
        assert_eq!(updated.item_id, parent.item_id, "item_id 不可变");

        let meta = &updated.attachments[0];
        assert_eq!(meta.filename, "报告.pdf", "文件名必须原样保留");
        assert_eq!(
            meta.size_bytes,
            payload.len() as u64,
            "size_bytes 必须是明文长度"
        );
        assert_eq!(
            meta.blob_size_bytes,
            (payload.len() + AEAD_TAG_LEN) as u64,
            "blob_size_bytes 必须是明文 + AEAD tag"
        );
        assert_eq!(meta.mime_type, "application/pdf", "MIME 由扩展名推断");
        assert!(
            repo.has_blob(&meta.attachment_id),
            "blob 帧必须已落库（否则导出/同步都会 BlobMissing）"
        );
        assert_eq!(
            *repo.read_blob(&meta.attachment_id).unwrap(),
            payload,
            "读回的明文必须与写入字节逐字节一致"
        );
        item_id = parent.item_id.clone();
        attachment_id = meta.attachment_id.clone();
    }

    // 落盘验证：重新解锁后元数据与 blob 都还在。
    let reopened = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = reopened.repo().unwrap();
    let item = repo.get_item(&item_id).unwrap();
    assert_eq!(item.attachments.len(), 1);
    assert_eq!(item.attachments[0].attachment_id, attachment_id);
    assert_eq!(*repo.read_blob(&attachment_id).unwrap(), payload);
    cleanup(&dir);
}

#[test]
fn bytes_entry_accepts_empty_payload() {
    // 0 字节文件在真机上真实存在（空日志、空占位文件），不能被误拒。
    let (dir, vault) = setup("empty");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    let parent = repo.create_item(login_draft("empty-att")).unwrap();
    let updated = repo
        .add_attachment_bytes(&parent.item_id, "empty.txt", &[])
        .unwrap();
    assert_eq!(updated.attachments.len(), 1);
    let meta = &updated.attachments[0];
    assert_eq!(meta.size_bytes, 0);
    assert_eq!(meta.blob_size_bytes, AEAD_TAG_LEN as u64);
    assert_eq!(
        *repo.read_blob(&meta.attachment_id).unwrap(),
        Vec::<u8>::new()
    );
    cleanup(&dir);
}

#[test]
fn bytes_entry_rejects_over_max_size() {
    let (dir, vault) = setup("toolarge");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    let parent = repo.create_item(login_draft("big-att")).unwrap();

    // 恰好等于上限：允许。
    let at_limit = vec![0u8; MAX_ATTACHMENT_BYTES as usize];
    assert!(
        repo.add_attachment_bytes(&parent.item_id, "at-limit.bin", &at_limit)
            .is_ok(),
        "恰好 20 MB 必须被接受（边界包含）"
    );

    // 上限 + 1 字节：拒绝。
    let over = vec![0u8; MAX_ATTACHMENT_BYTES as usize + 1];
    assert!(
        matches!(
            repo.add_attachment_bytes(&parent.item_id, "over.bin", &over),
            Err(CoreError::AttachmentTooLarge)
        ),
        "超 1 字节必须 AttachmentTooLarge"
    );
    // 拒绝后父条目状态不变（仍只有 1 个附件、rev 未再增长）。
    let after = repo.get_item(&parent.item_id).unwrap();
    assert_eq!(after.attachments.len(), 1, "失败的写入不得留下元数据");
    cleanup(&dir);
}

#[test]
fn bytes_entry_rejects_over_long_filename() {
    let (dir, vault) = setup("toolongname");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    let parent = repo.create_item(login_draft("long-name")).unwrap();

    // 255 字节 = 上限，允许。
    let ok_name = format!("{}.txt", "a".repeat(MAX_ATTACHMENT_FILENAME_BYTES - 4));
    assert_eq!(
        ok_name.len(),
        MAX_ATTACHMENT_FILENAME_BYTES,
        "构造必须恰为 255 字节"
    );
    assert!(
        repo.add_attachment_bytes(&parent.item_id, &ok_name, b"x")
            .is_ok(),
        "255 字节文件名必须被接受"
    );

    // 256 字节 = 拒绝。
    let bad_name = "a".repeat(MAX_ATTACHMENT_FILENAME_BYTES + 1);
    assert!(
        matches!(
            repo.add_attachment_bytes(&parent.item_id, &bad_name, b"x"),
            Err(CoreError::Validation(ref k)) if k == "filename_too_long"
        ),
        "超 255 字节文件名必须 Validation(\"filename_too_long\")"
    );
    // 中文按字节计：86 个汉字 = 258 字节 → 拒绝。
    let cjk = "名".repeat(86);
    assert!(matches!(
        repo.add_attachment_bytes(&parent.item_id, &cjk, b"x"),
        Err(CoreError::Validation(ref k)) if k == "filename_too_long"
    ));
    cleanup(&dir);
}

#[test]
fn bytes_entry_on_unknown_parent_is_item_not_found() {
    let (dir, vault) = setup("noparent");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    let ghost = uuid::Uuid::new_v4().to_string();
    assert!(
        matches!(
            repo.add_attachment_bytes(&ghost, "a.txt", b"hello"),
            Err(CoreError::ItemNotFound)
        ),
        "父条目不存在必须 ItemNotFound（先于任何大小/名称判定）"
    );
    cleanup(&dir);
}

#[test]
fn bytes_entry_keeps_validation_order_of_path_entry() {
    // 本节守 `insert_attachment` 文档承诺的顺序：④ 大小 → ⑤ 库容量 → ⑥ 文件名长度。
    // 同时违反「超大」与「超长名」时必须先报大小，否则错误码语义在两条入口间漂移。
    let (dir, vault) = setup("order");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    let parent = repo.create_item(login_draft("order-att")).unwrap();

    let big = vec![0u8; MAX_ATTACHMENT_BYTES as usize + 1];
    let long = "b".repeat(MAX_ATTACHMENT_FILENAME_BYTES + 1);
    assert!(
        matches!(
            repo.add_attachment_bytes(&parent.item_id, &long, &big),
            Err(CoreError::AttachmentTooLarge)
        ),
        "大小校验先于文件名校验（与 insert_attachment 的 ④/⑥ 顺序一致）"
    );

    // 附件数上限（③）先于大小（④）：塞满 16 个后再传超大文件必须报 AttachmentLimit。
    let max = passwordms_core::constants::MAX_ATTACHMENTS_PER_ITEM;
    for i in 0..max {
        repo.add_attachment_bytes(&parent.item_id, &format!("f{i}.txt"), b"x")
            .unwrap();
    }
    assert!(
        matches!(
            repo.add_attachment_bytes(&parent.item_id, "over.bin", &big),
            Err(CoreError::AttachmentLimit)
        ),
        "附件数上限必须先于大小判定（③ → ④）"
    );
    cleanup(&dir);
}
