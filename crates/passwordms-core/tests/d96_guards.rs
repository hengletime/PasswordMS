//! 【D-96】防御加固批次回归套件（全仓复核后主人批准的 10 项修复）。
//!
//! 覆盖：
//! 1. **导入两级上限**：文件 > 64 MB 预检拒绝；条目数 > 10_000 拒绝；
//! 2. **条目统一明文上限 384 KiB**：超限条目 `create_item` / `update_item` 拒绝；
//! 3. **头部计数交叉核对**：`item_count` 与实际扫描不符 → `maintenance_warnings`
//!    留痕（不阻断解锁，业务数据不受影响）；
//! 4. **导入去重回填**：同一预览文件内两条同名同型 → 第二条跳过；
//! 5. **folder_id 存在性校验**：`create_item` / `update_item` 引用不存在的分组 → 拒绝。

use std::path::PathBuf;

use passwordms_core::error::CoreError;
use passwordms_core::import::{self, ImportFormat};
use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;

const PWD: &str = "D96#Guard-Pw!42";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d96-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("d96.vault")
}

fn login_draft(title: &str, notes: Option<String>) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: None,
            username: Some("user".to_string()),
            password: Some("pw".to_string()),
            notes,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
    }
}

// ===== 2. 导入两级上限 =====

#[test]
fn import_file_over_limit_is_rejected_before_read() {
    let dir = std::env::temp_dir().join(format!("pms-d96-imp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("huge.csv");
    // 造一个 64 MB + 1 的文件（重复行填充，写入是一次性顺序 IO，秒级内完成）。
    let limit = passwordms_core::constants::MAX_IMPORT_FILE_BYTES;
    let row = "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields\n";
    let mut f = std::fs::File::create(&path).unwrap();
    use std::io::Write;
    f.write_all(row.as_bytes()).unwrap();
    let mut written = row.len() as u64;
    let chunk = vec![b'x'; 64 * 1024];
    while written <= limit {
        // 写非行内容也无妨——超限判定发生在解析之前。
        let n = (limit + 1 - written).min(chunk.len() as u64) as usize;
        f.write_all(&chunk[..n]).unwrap();
        written += n as u64;
    }
    drop(f);
    let err = import::preview_from_file(&path, ImportFormat::Csv).unwrap_err();
    match err {
        CoreError::Validation(d) => {
            assert!(d.contains("import_file_too_large"), "实得 {d}");
        }
        other => panic!("期望 Validation，实际 {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_item_count_over_limit_is_rejected() {
    // 10_001 条 login 行 → 解析成功但条目数超限（ERR_VALIDATION，带实际数量）。
    let header = "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields\n";
    let mut csv = String::from(header);
    for i in 0..passwordms_core::constants::MAX_IMPORT_ITEMS + 1 {
        csv.push_str(&format!(
            ",,login,item-{i},,https://x.example,user,pw,,\n"
        ));
    }
    let err = import::preview_from_str(&csv, ImportFormat::Csv).unwrap_err();
    match err {
        CoreError::Validation(d) => {
            assert!(d.contains("import_too_many_items"), "实得 {d}");
        }
        other => panic!("期望 Validation，实际 {other:?}"),
    }
}

#[test]
fn import_within_limits_still_works() {
    // 对照组：9999 条（< 10_000）正常通过解析与落库。
    let header = "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields\n";
    let mut csv = String::from(header);
    for i in 0..9999 {
        csv.push_str(&format!(
            ",,login,item-{i},,https://x.example,user,pw,,\n"
        ));
    }
    let preview = import::preview_from_str(&csv, ImportFormat::Csv).unwrap();
    let path = temp_vault("imp-ok");
    let mut s = Session::create(&path, PWD, true).unwrap();
    let result = import::apply(&preview, s.repo_mut().unwrap()).unwrap();
    assert_eq!(result.imported_items, 9999);
    assert_eq!(result.duplicate_skipped, 0);
}

// ===== 6. 条目统一明文上限 =====

#[test]
fn oversized_item_is_rejected_with_title_and_limit() {
    let path = temp_vault("item-big");
    let mut s = Session::create(&path, PWD, true).unwrap();
    // 384 KiB 上限：notes 造 400 KiB（超限），标题可定位。
    let big_notes = "x".repeat(400 * 1024);
    let err = s
        .repo_mut()
        .unwrap()
        .create_item(login_draft("巨型条目", Some(big_notes)))
        .unwrap_err();
    match err {
        CoreError::Validation(d) => {
            assert!(d.contains("item_too_large"), "实得 {d}");
            assert!(d.contains("巨型条目"), "报错应带条目标题，实得 {d}");
            assert!(
                d.contains(&passwordms_core::constants::MAX_ITEM_PLAINTEXT_BYTES.to_string()),
                "报错应带上限值，实得 {d}"
            );
        }
        other => panic!("期望 Validation，实际 {other:?}"),
    }
    // update 同样拒绝。
    let item = s.repo_mut().unwrap().create_item(login_draft("小条目", None)).unwrap();
    let err = s
        .repo_mut()
        .unwrap()
        .update_item(&item.item_id, login_draft("小条目", Some("x".repeat(400 * 1024))))
        .unwrap_err();
    assert!(matches!(err, CoreError::Validation(ref d) if d.contains("item_too_large")));
}

#[test]
fn normal_note_at_legacy_note_limit_still_passes() {
    // 回归锚点：SecureNote 256 KiB 正文（既有 MAX_NOTE_BYTES 上限）+ 标题
    // 仍低于 384 KiB 统一上限，正常创建不受影响。
    let path = temp_vault("note-ok");
    let mut s = Session::create(&path, PWD, true).unwrap();
    let body = "y".repeat(passwordms_core::constants::MAX_NOTE_BYTES);
    let draft = ItemDraft {
        kind: ItemKind::SecureNote(passwordms_core::model::item::SecureNoteData {
            title: "满额笔记".to_string(),
            body,
        }),
        folder_id: None,
        favorite: false,
    };
    let item = s.repo_mut().unwrap().create_item(draft).unwrap();
    assert_eq!(item.title(), "满额笔记");
}

// ===== 7. 头部计数交叉核对 =====

#[test]
fn header_count_mismatch_is_reported_via_maintenance_warnings() {
    let path = temp_vault("count-mismatch");
    {
        let mut s = Session::create(&path, PWD, true).unwrap();
        s.repo_mut().unwrap().create_item(login_draft("甲", None)).unwrap();
        s.repo_mut().unwrap().create_item(login_draft("乙", None)).unwrap();
        // 正常落盘后头部计数 = 2，无警告。
        assert!(s.repo_mut().unwrap().take_open_warnings().is_empty());
    }
    // 直接改文件头部 JSON 的 item_count（2 → 9，等长替换不破坏容器布局）。
    let raw = std::fs::read(&path).unwrap();
    let needle = b"\"item_count\":2";
    let pos = raw
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("item_count 应出现在头部 JSON");
    let mut patched = raw.clone();
    patched[pos..pos + needle.len()].copy_from_slice(b"\"item_count\":9");
    std::fs::write(&path, &patched).unwrap();

    let s = Session::unlock(&path, PWD).unwrap();
    let warnings = s.maintenance_warnings();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("header_item_count_mismatch")
                && w.contains("header=9")
                && w.contains("actual=2")),
        "应有计数不一致警告，实得 {warnings:?}"
    );
    // 不阻断：两条业务数据完好。
    assert_eq!(s.repo().unwrap().list_items(None, false, None).len(), 2);
}

// ===== 9. 导入去重回填 =====

#[test]
fn duplicate_rows_within_same_import_are_deduped() {
    // 同一预览文件内两条同名同型：第二条应计 duplicate_skipped 而非双双落库。
    let header = "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields\n";
    let csv = format!(
        "{header},,login,重复条目,,https://a.example,user,pa,,\n,,login,重复条目,,https://b.example,user,pb,,\n"
    );
    let preview = import::preview_from_str(&csv, ImportFormat::Csv).unwrap();
    assert_eq!(preview.items.len(), 2, "解析器不折叠同名行");
    let path = temp_vault("imp-dedup");
    let mut s = Session::create(&path, PWD, true).unwrap();
    let result = import::apply(&preview, s.repo_mut().unwrap()).unwrap();
    assert_eq!(result.imported_items, 1, "只导首条");
    assert_eq!(result.duplicate_skipped, 1, "第二条应计为重复跳过");
    assert_eq!(
        s.repo()
            .unwrap()
            .list_items(None, false, Some("重复条目"))
            .len(),
        1,
        "库内只能有一条"
    );
}

// ===== 10. folder_id 存在性校验 =====

#[test]
fn create_and_update_reject_missing_folder_id() {
    let path = temp_vault("folder-check");
    let mut s = Session::create(&path, PWD, true).unwrap();
    let ghost = "00000000-0000-0000-0000-00000000dead".to_string();
    // create：不存在的分组 → 拒绝（错误信息带 folder_id）。
    let err = s
        .repo_mut()
        .unwrap()
        .create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "条目".to_string(),
                url: None,
                username: None,
                password: None,
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: Some(ghost.clone()),
            favorite: false,
        })
        .unwrap_err();
    assert!(
        matches!(err, CoreError::Validation(ref d) if d.contains(&ghost) && d.contains("does not exist")),
        "实得 {err:?}"
    );
    // 对照：真实分组正常创建。
    let folder = s
        .repo_mut()
        .unwrap()
        .create_folder("真分组".to_string(), None, None)
        .unwrap();
    let item = s
        .repo_mut()
        .unwrap()
        .create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "条目".to_string(),
                url: None,
                username: None,
                password: None,
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: Some(folder.folder_id.clone()),
            favorite: false,
        })
        .unwrap();
    assert_eq!(item.folder_id.as_deref(), Some(folder.folder_id.as_str()));
    // update：改指到不存在的分组 → 拒绝。
    let err = s
        .repo_mut()
        .unwrap()
        .update_item(&item.item_id, login_draft_with_folder(&ghost))
        .unwrap_err();
    assert!(matches!(err, CoreError::Validation(ref d) if d.contains(&ghost)));
    // 原条目未被半途改动（update 在校验处早退）。
    let unchanged = s.repo().unwrap().get_item(&item.item_id).unwrap();
    assert_eq!(
        unchanged.folder_id.as_deref(),
        Some(folder.folder_id.as_str())
    );
}

fn login_draft_with_folder(folder_id: &str) -> ItemDraft {
    let mut d = login_draft("条目", None);
    d.folder_id = Some(folder_id.to_string());
    d
}
