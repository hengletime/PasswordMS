//! D-90 导入合并回归：同名文件夹合并 + 同名同型条目跳过。
//!
//! 背景（主人实测）：重复导入同一份 CSV 每次都多一套同名分组（「Commander 办公」×2）、
//! 同名条目无限堆积。旧 `apply` 用 `put_folder_direct` / `put_item_direct` 直接落库、
//! 从不与现有库比对。本套测试钉死新契约：
//!
//! 1. **同名文件夹合并**：按 `name` 命中现有分组即复用其 folder_id（条目归入既有分组），
//!    预览内条目的 folder_id 全部重映射；
//! 2. **同名同型条目跳过**（title + type 都相同），**绝不覆盖**现有数据；同名不同型照导；
//! 3. **回收站条目不阻碍重导入**（items 集合不含回收站快照）；
//! 4. 结果计数如实：`merged_folders` / `duplicate_skipped` / `imported_*`。

use std::path::PathBuf;

use passwordms_core::crypto::keys::MasterKey;
use passwordms_core::crypto::{kdf, keys, rng, verify};
use passwordms_core::import::{self, ImportFormat};
use passwordms_core::model::item::{CustomField, ItemKind, LoginData, SecureNoteData};
use passwordms_core::model::now_ms;
use passwordms_core::model::vault::Vault;
use passwordms_core::store::repository::{ItemDraft, Repository};
use zeroize::Zeroizing;

const CSV_HEADER: &str =
    "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields";

fn tmp_vault_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "passwordms_test_import_merge_{name}_{}.vault",
        uuid::Uuid::new_v4()
    ));
    p
}

fn build_header(kek: &keys::Kek, vk: &[u8; 32]) -> Vault {
    let salt = rng::random_salt16().expect("salt").to_vec();
    let wrapped = keys::wrap_vault_key(kek, vk).expect("wrap");
    let verify_key = Zeroizing::new([7u8; 32]);
    let mac = verify::compute_verify_mac(&verify_key).expect("mac");
    Vault::new(
        "00000000-0000-4000-8000-000000000000".to_string(),
        salt,
        wrapped,
        mac,
        now_ms(),
    )
}

fn fresh_repo(name: &str) -> Repository {
    let path = tmp_vault_path(name);
    let mk: MasterKey = Zeroizing::new(rng::random_key().expect("mk"));
    let kek = kdf::derive_kek(&mk[..]).expect("kek");
    let vk = rng::random_key().expect("vk");
    Repository::create(
        &path,
        build_header(&kek, &vk),
        Zeroizing::new(vk),
        "qa_test_device".to_string(),
    )
    .expect("create repo")
}

fn login_draft(title: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: None,
            username: Some("qa_test_user".to_string()),
            password: Some("qa_test_password".to_string()),
            notes: None,
            totp_secret: None,
            custom_fields: vec![CustomField {
                key: "qa_test_field".to_string(),
                value: "v".to_string(),
                hidden: false,
            }],
        }),
        folder_id: None,
        favorite: false,
    }
}

/// CSV 行（folder / favorite / type / name / notes / uri / username / password / totp / fields）。
fn csv_row(folder: &str, kind: &str, name: &str) -> String {
    format!("{folder},0,{kind},{name},,,,,,\n")
}

fn parse_csv(rows: &[String]) -> import::ImportPreview {
    let mut csv = String::from(CSV_HEADER);
    csv.push('\n');
    for row in rows {
        csv.push_str(row);
    }
    import::preview_from_str(&csv, ImportFormat::Csv).expect("测试 CSV 必须可解析")
}

#[test]
fn 同名文件夹合并_条目归入既有分组() {
    let mut repo = fresh_repo("merge_folder");
    let existing = repo
        .create_folder("Commander 办公".to_string(), None, None)
        .expect("预置分组");

    let preview = parse_csv(&[
        csv_row("Commander 办公", "login", "条目甲"),
        csv_row("Commander 办公", "login", "条目乙"),
        csv_row("新分组", "login", "条目丙"),
    ]);
    let result = import::apply(&preview, &mut repo).expect("apply");

    assert_eq!(result.merged_folders, 1, "「Commander 办公」应合并不新建");
    assert_eq!(result.imported_folders, 1, "只新建「新分组」");
    assert_eq!(result.duplicate_skipped, 0);
    assert_eq!(result.imported_items, 3);

    let folders = repo.list_folders();
    assert_eq!(folders.len(), 2, "总共只有 2 个分组（预置 1 + 新建 1）");
    assert!(
        folders
            .iter()
            .filter(|f| f.name == "Commander 办公")
            .count()
            == 1,
        "不得出现第二个同名分组"
    );

    // 条目 folder_id 必须重映射到现有分组的 id。
    for title in ["条目甲", "条目乙"] {
        let hit = repo
            .list_items(None, false, None)
            .into_iter()
            .find(|it| it.kind.title() == title)
            .unwrap_or_else(|| panic!("{title} 应已导入"));
        assert_eq!(
            hit.folder_id.as_deref(),
            Some(existing.folder_id.as_str()),
            "{title} 应归入既有分组（folder_id 重映射）"
        );
    }
    let new_folder = folders.iter().find(|f| f.name == "新分组").expect("新分组");
    let bing = repo
        .list_items(None, false, None)
        .into_iter()
        .find(|it| it.kind.title() == "条目丙")
        .expect("条目丙");
    assert_eq!(
        bing.folder_id.as_deref(),
        Some(new_folder.folder_id.as_str())
    );
}

#[test]
fn 同名同型跳过_同名不同型照导_不覆盖现有数据() {
    let mut repo = fresh_repo("dup_items");
    let seeded = repo.create_item(login_draft("条目A")).expect("预置条目");

    let preview = parse_csv(&[
        csv_row("", "login", "条目A"),       // 同名同型 → 跳过
        csv_row("", "secure note", "条目A"), // 同名不同型 → 照导
        csv_row("", "login", "条目B"),       // 新条目 → 导入
    ]);
    let result = import::apply(&preview, &mut repo).expect("apply");

    assert_eq!(result.duplicate_skipped, 1, "只有同名同型那行被跳过");
    assert_eq!(result.imported_items, 2);

    let items = repo.list_items(None, false, None);
    let logins_a: Vec<_> = items
        .iter()
        .filter(|it| it.kind.title() == "条目A" && it.kind.type_name() == "login")
        .collect();
    assert_eq!(logins_a.len(), 1, "登录「条目A」不得堆积");
    assert_eq!(
        logins_a[0].item_id, seeded.item_id,
        "现有条目必须原封不动（绝不覆盖）"
    );
    assert_eq!(logins_a[0].kind.title(), "条目A",);
    assert!(
        items
            .iter()
            .any(|it| it.kind.type_name() == "secure_note" && it.kind.title() == "条目A"),
        "同名不同型必须照常导入"
    );
}

#[test]
fn 重复导入同一文件_幂等不堆积() {
    let mut repo = fresh_repo("idempotent");
    let preview = parse_csv(&[
        csv_row("Commander 办公", "login", "条目甲"),
        csv_row("Commander 办公", "secure note", "条目乙"),
    ]);

    let first = import::apply(&preview, &mut repo).expect("第一次 apply");
    assert_eq!(first.imported_items, 2);
    assert_eq!(first.merged_folders, 0);

    // 第二次导入同一文件：全部条目同名同型 → 全跳过；分组同名 → 全合并；库不膨胀。
    let second = import::apply(&preview, &mut repo).expect("第二次 apply");
    assert_eq!(second.imported_items, 0, "重复导入不得新增条目");
    assert_eq!(second.duplicate_skipped, 2);
    assert_eq!(second.merged_folders, 1);
    assert_eq!(second.imported_folders, 0);

    assert_eq!(repo.list_items(None, false, None).len(), 2, "条目总数不变");
    assert_eq!(repo.list_folders().len(), 1, "分组总数不变");
}

#[test]
fn 回收站同名条目不阻碍重导入() {
    let mut repo = fresh_repo("trash_reimport");
    let seeded = repo.create_item(login_draft("条目A")).expect("预置");
    repo.delete_item(&seeded.item_id, false).expect("进回收站");

    let preview = parse_csv(&[csv_row("", "login", "条目A")]);
    let result = import::apply(&preview, &mut repo).expect("apply");

    assert_eq!(result.imported_items, 1, "回收站条目不应算作现存同名");
    assert_eq!(result.duplicate_skipped, 0);
    assert_eq!(repo.list_items(None, false, None).len(), 1);
}

#[test]
fn 笔记正文与登录标题混合场景_类型名判定正确() {
    // 确认 type_name 判定与 SecureNoteData 构造无隐性耦合（防将来 enum 变体改名漏改）。
    let note = ItemKind::SecureNote(SecureNoteData {
        title: "n".to_string(),
        body: "b".to_string(),
    });
    let login = ItemKind::Login(LoginData {
        title: "n".to_string(),
        url: None,
        username: None,
        password: None,
        notes: None,
        totp_secret: None,
        custom_fields: vec![],
    });
    assert_eq!(note.type_name(), "secure_note");
    assert_eq!(login.type_name(), "login");
    assert_ne!(
        note.type_name(),
        login.type_name(),
        "同名不同型判定依赖 type_name 区分"
    );
}
