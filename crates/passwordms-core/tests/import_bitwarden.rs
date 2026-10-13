//! Bitwarden JSON / CSV 映射与跳过计数（架构文档 §8.4、§7）。
//!
//! 状态：已在本机实跑通过（`cargo test -p passwordms-core`，见 docs/03b-verification.md）。

use passwordms_core::error::CoreError;
use passwordms_core::import::{bitwarden_csv, bitwarden_json};

const JSON_FIXTURE: &str = r#"{
  "folders": [ { "id": "f1", "name": "qa_test_folder" } ],
  "items": [
    { "id": "i1", "type": 1, "name": "qa_test_login", "folderId": "f1", "favorite": true,
      "notes": "qa_test_note",
      "login": { "username": "qa_test_user", "password": "qa_test_pass",
                 "uris": [ { "uri": "https://example.invalid/one" }, { "uri": "https://example.invalid/two" } ],
                 "totp": "JBSWY3DPEHPK3PXP" },
      "fields": [ { "name": "qa_test_field", "value": "qa_test_value", "type": 1 } ] },
    { "id": "i2", "type": 2, "name": "qa_test_note_item", "notes": "qa_test_body" },
    { "id": "i3", "type": 3, "name": "qa_test_card" },
    { "id": "i4", "type": 4, "name": "qa_test_identity" }
  ]
}"#;

#[test]
fn json_mapping_and_skip_count() {
    let preview = bitwarden_json::parse(JSON_FIXTURE).expect("parse");
    assert_eq!(preview.folders.len(), 1);
    assert_eq!(preview.folders[0].name, "qa_test_folder");
    assert_eq!(preview.items.len(), 2);
    assert_eq!(preview.total, 4);
    assert_eq!(preview.skipped, 2);
    assert_eq!(preview.skipped_by_type.get("card").copied(), Some(1));
    assert_eq!(preview.skipped_by_type.get("identity").copied(), Some(1));

    // 首条登录：多 URI 取第一条（Q2）、字段映射、字段隐藏。
    let login = &preview.items[0];
    match &login.kind {
        passwordms_core::model::item::ItemKind::Login(d) => {
            assert_eq!(d.title, "qa_test_login");
            assert_eq!(d.url.as_deref(), Some("https://example.invalid/one"));
            assert_eq!(d.username.as_deref(), Some("qa_test_user"));
            assert_eq!(d.totp_secret.as_deref(), Some("JBSWY3DPEHPK3PXP"));
            assert_eq!(d.custom_fields.len(), 1);
            assert!(d.custom_fields[0].hidden);
        }
        _ => panic!("期望 login 类型"),
    }
    assert!(login.favorite);
    // folder 映射到预览内新分组 ID。
    assert_eq!(login.folder_id.as_deref(), Some(preview.folders[0].folder_id.as_str()));
}

#[test]
fn encrypted_export_is_rejected() {
    let fixture = r#"{"encrypted": true, "items": []}"#;
    let err = bitwarden_json::parse(fixture).unwrap_err();
    assert_eq!(err.code(), "ERR_IMPORT_ENCRYPTED_EXPORT");
    assert!(matches!(err, CoreError::ImportEncryptedExport));
}

#[test]
fn csv_mapping_and_skip_count() {
    let csv = "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,extra_col\n\
qa_test_folder,true,login,qa_test_login,qa_test_note,https://example.invalid,qa_test_user,qa_test_pass,,\
qa_test_extra\n\
,,card,qa_test_card,,,,,,\n\
,,identity,qa_test_identity,,,,,,\n";
    let preview = bitwarden_csv::parse(csv).expect("parse");
    assert_eq!(preview.folders.len(), 1);
    assert_eq!(preview.items.len(), 1);
    assert_eq!(preview.total, 3);
    assert_eq!(preview.skipped, 2);
    match &preview.items[0].kind {
        passwordms_core::model::item::ItemKind::Login(d) => {
            assert_eq!(d.title, "qa_test_login");
            assert_eq!(d.password.as_deref(), Some("qa_test_pass"));
            // 未知列归入 custom_fields（U8）。
            assert!(d.custom_fields.iter().any(|f| f.key == "extra_col"));
        }
        _ => panic!("期望 login 类型"),
    }
}
