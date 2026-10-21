//! Bitwarden **CSV** 导入解析与字段映射（架构文档 §7.3、§10 U8）。
//!
//! 【已裁定 U8】CSV 仅映射同名基础列 + `folder` 列，其余列归入 `custom_fields`。
//! 因依赖白名单未包含 csv 库，此处实现最小 RFC 4180 解析器（引号转义 + 跨行字段）。

use std::collections::HashMap;

use crate::error::{CoreError, CoreResult};
use crate::import::ImportPreview;
use crate::model::folder::Folder;
use crate::model::item::{CustomField, Item, ItemKind, LoginData, SecureNoteData};
use crate::model::now_ms;

/// 已知基础列（其余列进入 `custom_fields`）。
///
/// D-72：改为 `pub` —— 内置 CSV 模板（[`crate::import::template::CSV_TEMPLATE`]）的
/// 表头顺序必须与本数组**逐字一致**，该约束由 `tests/import_template.rs` 断言守住，
/// 否则「模板改了、解析器没跟上」会静默把示例值映射到别的列。
pub const KNOWN_COLUMNS: [&str; 10] = [
    "folder",
    "favorite",
    "type",
    "name",
    "notes",
    "login_uri",
    "login_username",
    "login_password",
    "login_totp",
    "fields",
];

/// 解析 Bitwarden CSV 导出内容。
pub fn parse(content: &str) -> CoreResult<ImportPreview> {
    let rows = parse_csv(content);
    if rows.is_empty() {
        return Err(CoreError::ImportParse("csv_empty".to_string()));
    }
    let header = rows[0].clone();
    let index: HashMap<String, usize> = header
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim().to_lowercase(), i))
        .collect();

    let mut preview = ImportPreview {
        folders: Vec::new(),
        items: Vec::new(),
        total: 0,
        skipped: 0,
        skipped_by_type: HashMap::new(),
    };
    let mut folder_map: HashMap<String, String> = HashMap::new();
    let mut sort_order = 0i32;

    for row in rows.iter().skip(1) {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        preview.total += 1;

        let get = |key: &str| -> Option<String> {
            index
                .get(key)
                .and_then(|i| row.get(*i))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let type_val = get("type").unwrap_or_default().to_lowercase();
        let is_login = matches!(type_val.as_str(), "login" | "1");
        let is_note = matches!(type_val.as_str(), "secure note" | "note" | "2");

        if !is_login && !is_note {
            let label = match type_val.as_str() {
                "card" | "3" => "card",
                "identity" | "4" => "identity",
                _ if type_val.is_empty() => "unknown",
                _ => "unknown",
            };
            *preview
                .skipped_by_type
                .entry(label.to_string())
                .or_insert(0) += 1;
            continue;
        }

        let title = get("name").unwrap_or_default();
        if title.trim().is_empty() {
            *preview
                .skipped_by_type
                .entry("unknown".to_string())
                .or_insert(0) += 1;
            continue;
        }

        // 分组：按名称惰性创建。
        let folder_id = match get("folder") {
            Some(name) => Some(ensure_folder(
                &name,
                &mut folder_map,
                &mut preview.folders,
                &mut sort_order,
            )),
            None => None,
        };

        let favorite = get("favorite")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);

        // 其余非已知列归入 custom_fields。
        let mut custom_fields: Vec<CustomField> = Vec::new();
        for (key, i) in &index {
            if KNOWN_COLUMNS.contains(&key.as_str()) {
                continue;
            }
            if let Some(value) = row.get(*i) {
                let value = value.trim();
                if !value.is_empty() {
                    custom_fields.push(CustomField {
                        key: key.clone(),
                        value: value.to_string(),
                        hidden: false,
                    });
                }
            }
        }

        let now = now_ms();
        let kind = if is_login {
            ItemKind::Login(LoginData {
                title,
                url: get("login_uri"),
                username: get("login_username"),
                password: get("login_password"),
                notes: get("notes"),
                totp_secret: get("login_totp"),
                custom_fields,
            })
        } else {
            ItemKind::SecureNote(SecureNoteData {
                title,
                body: get("notes").unwrap_or_default(),
            })
        };

        preview.items.push(Item {
            item_id: uuid::Uuid::new_v4().to_string(),
            kind,
            folder_id,
            favorite,
            created_at: now,
            updated_at: now,
            rev: 1,
            modified_by: "import".to_string(),
            attachments: Vec::new(),
        });
    }

    preview.skipped = preview.skipped_by_type.values().sum();
    Ok(preview)
}

fn ensure_folder(
    name: &str,
    folder_map: &mut HashMap<String, String>,
    folders: &mut Vec<Folder>,
    sort_order: &mut i32,
) -> String {
    if let Some(id) = folder_map.get(name) {
        return id.clone();
    }
    let folder = Folder {
        folder_id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        icon: None,
        color: None,
        sort_order: *sort_order,
        rev: 1,
        updated_at: now_ms(),
    };
    *sort_order += 1;
    folder_map.insert(name.to_string(), folder.folder_id.clone());
    folders.push(folder.clone());
    folder.folder_id
}

/// 最小 CSV 解析器（RFC 4180 子集）：支持双引号包裹、内嵌逗号/换行与 `""` 转义。
fn parse_csv(content: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    // 收尾最后一列/行（文件末尾无换行时）。
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}
