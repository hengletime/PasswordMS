//! 锁定后会话内存清零辅助检查（架构文档 §8.4，验收点 A-04）。
//!
//! 状态：已在本机实跑通过（`cargo test -p passwordms-core`，见 docs/03b-verification.md）。
//! 真正的内存取证需外部工具；此处验证清零原语与锁定状态语义。

use passwordms_core::model::{
    AttachmentData, CustomField, Folder, Item, ItemKind, LoginData, SecureNoteData,
};
use passwordms_core::securemem::{secret32, wipe, Secret32};
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use zeroize::Zeroize;

/// 构造一条带已知 password 的 Login 条目草稿（供仓储级接线测试使用）。
fn login_draft_with_password(title: &str, password: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: Some("https://example.invalid".to_string()),
            username: Some("qa_test_user".to_string()),
            password: Some(password.to_string()),
            notes: Some("qa_test_note".to_string()),
            totp_secret: None,
            custom_fields: vec![CustomField {
                key: "qa_test_field".to_string(),
                value: "qa_test_value".to_string(),
                hidden: false,
            }],
        }),
        folder_id: None,
        favorite: false,
    }
}

/// `Option<String>` 视「为 `None`」或「为空串」为已擦除（zeroize 两种语义均视为已清）。
fn is_wiped(field: &Option<String>) -> bool {
    match field {
        None => true,
        Some(s) => s.is_empty(),
    }
}

#[test]
fn wipe_zeroes_buffer() {
    let mut buf = vec![0xAAu8; 64];
    wipe(&mut buf);
    assert!(buf.iter().all(|b| *b == 0), "wipe 后缓冲区应全 0");
}

#[test]
fn secret32_zeroizes() {
    let mut s: Secret32 = secret32([0x11u8; 32]);
    assert_eq!(s[0], 0x11);
    s.zeroize();
    assert!(s.iter().all(|b| *b == 0), "zeroize 后密钥应全 0");
}

#[test]
fn session_lock_transitions_and_cascades() {
    let mut path = std::env::temp_dir();
    path.push(format!("passwordms_test_zeroize_{}.vault", uuid::Uuid::new_v4()));

    let mut session =
        Session::create(&path, "qa_test_master_password", true).expect("create");
    assert!(session.is_unlocked());

    // 锁定 → 状态切换、仓储不可访问（旧 Unlocked Drop → Zeroizing 级联清零）。
    session.lock();
    assert!(!session.is_unlocked());
    assert_eq!(session.repo().unwrap_err().code(), "ERR_NOT_UNLOCKED");

    // 重新解锁可用（上一会话明文不可见）。
    let mut reopened = Session::unlock(&path, "qa_test_master_password").expect("unlock");
    assert!(reopened.is_unlocked());
    reopened.lock();

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("device"));
}

#[test]
fn item_wipe_plaintext_clears_sensitive_fields() {
    // D-1 / A-04：锁定前必须显式擦除条目明文（`Drop` 只释放、不覆写）。
    let mut login = LoginData {
        title: "qa_test_title".to_string(),
        url: Some("https://example.invalid".to_string()),
        username: Some("qa_test_user".to_string()),
        password: Some("qa_test_secret".to_string()),
        notes: Some("qa_test_note".to_string()),
        totp_secret: Some("JBSWY3DPEHPK3PXP".to_string()),
        custom_fields: vec![CustomField {
            key: "qa_test_key".to_string(),
            value: "qa_test_value".to_string(),
            hidden: true,
        }],
    };
    login.wipe_plaintext();
    assert!(login.title.is_empty(), "title 应被清空");
    assert!(is_wiped(&login.url), "url 应被擦除");
    assert!(is_wiped(&login.username), "username 应被擦除");
    assert!(is_wiped(&login.password), "password 应被擦除");
    assert!(is_wiped(&login.notes), "notes 应被擦除");
    assert!(is_wiped(&login.totp_secret), "totp_secret 应被擦除");
    assert!(login.custom_fields[0].key.is_empty(), "自定义字段 key 应被擦除");
    assert!(login.custom_fields[0].value.is_empty(), "自定义字段 value 应被擦除");

    let mut note = SecureNoteData {
        title: "qa_test_note_title".to_string(),
        body: "qa_test_body".to_string(),
    };
    note.wipe_plaintext();
    assert!(note.title.is_empty() && note.body.is_empty(), "笔记标题与正文应被擦除");

    let mut att = AttachmentData {
        title: "qa_test_att".to_string(),
        original_filename: "qa_test.bin".to_string(),
        mime_type: "application/octet-stream".to_string(),
        size_bytes: 16,
        blob_ref: "qa_test_blob".to_string(),
        blob_size_bytes: 32,
    };
    att.wipe_plaintext();
    assert!(att.title.is_empty(), "附件标题应被擦除");
    assert!(att.original_filename.is_empty(), "原始文件名应被擦除");
    assert_eq!(att.mime_type, "application/octet-stream", "非隐私元数据应保留");
}

#[test]
fn repository_wipe_plaintext_clears_item_read_from_repo() {
    // D-1 / A-04（**擦除覆盖面**验证）：证明 [`Repository::wipe_plaintext`] 的擦除覆盖
    // 到了「从库读回的条目明文」——调用后同一条目读回的敏感字段被清空。
    //
    // 说明（如实修正含义）：本用例**不**证明「`Session::lock` 一定调用了 wipe」。
    // 真兜底是 `impl Drop for Repository`（QA 对抗实验：注释掉 `lock` 内调用后本用例仍绿）。
    // 故其价值在于「擦除覆盖面包含条目字段」，而非「接线被调用」。
    let mut path = std::env::temp_dir();
    path.push(format!(
        "passwordms_test_repo_wipe_{}.vault",
        uuid::Uuid::new_v4()
    ));

    let mut session =
        Session::create(&path, "qa_test_master_password", true).expect("create");
    assert!(session.is_unlocked());

    // 1) 建库后写入一条 Login 条目，password 设为已知值。
    let item = session
        .repo_mut()
        .expect("unlocked")
        .create_item(login_draft_with_password("qa_test_alpha", "qa_test_password"))
        .expect("create_item");

    // 2) 断言能从 repo 读回该 password（明文确实驻留于内存 items 映射中）。
    let read = session
        .repo()
        .expect("unlocked")
        .get_item(&item.item_id)
        .expect("get_item");
    match &read.kind {
        ItemKind::Login(login) => {
            assert_eq!(
                login.password.as_deref(),
                Some("qa_test_password"),
                "写入后应能从仓储读回明文密码"
            );
        }
        other => panic!("应为 Login 条目，实际为 {other:?}"),
    }

    // 3) 调用根级擦除（`Session::lock` 亦走此入口）。
    session.repo_mut().expect("unlocked").wipe_plaintext();

    // 4) 断言同一条目读回时 password 已被清空（其余明文字段一并清空）。
    let after = session
        .repo()
        .expect("unlocked")
        .get_item(&item.item_id)
        .expect("get_item");
    match &after.kind {
        ItemKind::Login(login) => {
            assert!(is_wiped(&login.password), "仓储擦除后 password 应被清空");
            assert!(login.title.is_empty(), "仓储擦除后 title 应被清空");
            assert!(is_wiped(&login.url), "仓储擦除后 url 应被清空");
            assert!(is_wiped(&login.username), "仓储擦除后 username 应被清空");
            assert!(is_wiped(&login.notes), "仓储擦除后 notes 应被清空");
            assert!(
                login.custom_fields[0].key.is_empty(),
                "仓储擦除后自定义字段 key 应被清空"
            );
            assert!(
                login.custom_fields[0].value.is_empty(),
                "仓储擦除后自定义字段 value 应被清空"
            );
        }
        other => panic!("应为 Login 条目，实际为 {other:?}"),
    }

    // 5) 走完整 `Session::lock` 路径：锁定后仓储不可再访问（明文已随状态切换丢弃）。
    session.lock();
    assert!(!session.is_unlocked());
    assert_eq!(session.repo().unwrap_err().code(), "ERR_NOT_UNLOCKED");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("device"));
}

#[test]
fn repository_wipe_plaintext_clears_folder_fields() {
    // D-1b / A-04（**擦除覆盖面**验证）：锁定须擦除分组明文（名称 / 图标 / 颜色），
    // 否则「银行」「公司 VPN」等分组主题会以明文残留在内存（01-security-req.md:70）。
    // 未加 `Repository::wipe_plaintext` 内 folders 循环前，本用例应**变红**。
    let mut path = std::env::temp_dir();
    path.push(format!(
        "passwordms_test_folder_wipe_{}.vault",
        uuid::Uuid::new_v4()
    ));

    let mut session =
        Session::create(&path, "qa_test_master_password", true).expect("create");
    session
        .repo_mut()
        .expect("unlocked")
        .create_folder(
            "qa_test_bank".to_string(),
            Some("icon_qa".to_string()),
            Some("#123456".to_string()),
        )
        .expect("create_folder");

    // 擦除前：从库读回分组，应可见明文；并记录数值元数据用于对照。
    let before = session.repo().expect("unlocked").list_folders();
    assert_eq!(before.len(), 1, "应存在 1 个分组");
    assert_eq!(before[0].name, "qa_test_bank");
    assert_eq!(before[0].icon.as_deref(), Some("icon_qa"));
    assert_eq!(before[0].color.as_deref(), Some("#123456"));

    // 擦除。
    session.repo_mut().expect("unlocked").wipe_plaintext();

    // 擦除后：分组名 / 图标 / 颜色均被清空（`zeroize` 对 `Option<String>` 置空串，
    // `is_wiped` 兼容 None / 空串两种语义）。
    let after = session.repo().expect("unlocked").list_folders();
    assert_eq!(after.len(), 1, "擦除不应删除分组本身");
    assert!(after[0].name.is_empty(), "分组 name 应被清空");
    assert!(is_wiped(&after[0].icon), "分组 icon 应被清空");
    assert!(is_wiped(&after[0].color), "分组 color 应被清空");
    // 数值元数据保留（不可反推主题）。
    assert_eq!(after[0].sort_order, before[0].sort_order, "sort_order 应保留");
    assert_eq!(after[0].rev, before[0].rev, "rev 应保留");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("device"));
}

#[test]
fn import_preview_wipe_plaintext_clears_items_and_folders() {
    // F-3 / A-04：导入预览缓存（`AppState.pending_imports`）持有完整条目明文，
    // 锁定须能擦除之；本用例验证 [`ImportPreview::wipe_plaintext`] 的覆盖面。
    use passwordms_core::import::ImportPreview;

    let folder = Folder {
        folder_id: "11111111-1111-4111-8111-111111111111".to_string(),
        name: "qa_test_import_folder".to_string(),
        icon: Some("icon_import".to_string()),
        color: Some("#abcdef".to_string()),
        sort_order: 0,
        rev: 1,
        updated_at: 0,
    };
    let item = Item {
        item_id: "22222222-2222-4222-8222-222222222222".to_string(),
        kind: ItemKind::Login(LoginData {
            title: "qa_test_import_login".to_string(),
            url: Some("https://example.invalid".to_string()),
            username: Some("qa_test_user".to_string()),
            password: Some("qa_test_import_password".to_string()),
            notes: Some("qa_test_note".to_string()),
            totp_secret: None,
            custom_fields: vec![CustomField {
                key: "qa_test_field".to_string(),
                value: "qa_test_value".to_string(),
                hidden: false,
            }],
        }),
        folder_id: Some(folder.folder_id.clone()),
        favorite: false,
        created_at: 0,
        updated_at: 0,
        rev: 1,
        modified_by: "qa_test".to_string(),
    };
    let mut preview = ImportPreview {
        folders: vec![folder],
        items: vec![item],
        total: 1,
        skipped: 0,
        skipped_by_type: std::collections::HashMap::new(),
    };

    preview.wipe_plaintext();

    assert_eq!(preview.folders.len(), 1, "擦除不应删除预览分组");
    assert!(preview.folders[0].name.is_empty(), "预览分组 name 应被清空");
    assert!(is_wiped(&preview.folders[0].icon), "预览分组 icon 应被清空");
    assert!(is_wiped(&preview.folders[0].color), "预览分组 color 应被清空");
    assert_eq!(preview.items.len(), 1, "擦除不应删除预览条目");
    match &preview.items[0].kind {
        ItemKind::Login(login) => {
            assert!(login.title.is_empty(), "预览条目 title 应被清空");
            assert!(is_wiped(&login.password), "预览条目 password 应被清空");
            assert!(is_wiped(&login.username), "预览条目 username 应被清空");
            assert!(login.custom_fields[0].value.is_empty(), "预览自定义字段应被清空");
        }
        other => panic!("应为 Login 条目，实际为 {other:?}"),
    }
}
