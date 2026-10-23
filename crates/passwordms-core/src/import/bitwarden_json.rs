//! Bitwarden **JSON** 导入解析与字段映射（架构文档 §7.1/§7.2）。

use std::collections::HashMap;

use serde_json::Value;

use crate::error::{CoreError, CoreResult};
use crate::import::ImportPreview;
use crate::model::folder::Folder;
use crate::model::item::{CustomField, Item, ItemKind, LoginData, SecureNoteData};
use crate::model::now_ms;

/// 解析 Bitwarden JSON 导出内容。
pub fn parse(content: &str) -> CoreResult<ImportPreview> {
    let root: Value =
        serde_json::from_str(content).map_err(|_| CoreError::ImportParse("json".to_string()))?;

    // 加密导出（含 encrypted=true）不支持。
    if root
        .get("encrypted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Err(CoreError::ImportEncryptedExport);
    }

    let mut preview = ImportPreview {
        folders: Vec::new(),
        items: Vec::new(),
        total: 0,
        skipped: 0,
        skipped_by_type: HashMap::new(),
    };

    // 分组：Bitwarden id → 新 Folder。
    let mut folder_map: HashMap<String, String> = HashMap::new();
    let mut sort_order = 0i32;
    if let Some(folders) = root.get("folders").and_then(|v| v.as_array()) {
        for f in folders {
            let old_id = f
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = f
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let folder = Folder {
                folder_id: uuid::Uuid::new_v4().to_string(),
                name,
                icon: None,
                color: None,
                sort_order,
                rev: 1,
                updated_at: now_ms(),
                pinned: false,
                pin_order: 0,
            };
            sort_order += 1;
            folder_map.insert(old_id, folder.folder_id.clone());
            preview.folders.push(folder);
        }
    }

    let items = root
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    preview.total = items.len();

    for raw in items {
        let type_id = raw.get("type").and_then(|v| v.as_i64()).unwrap_or(-1);
        match type_id {
            1 => {
                if let Some(item) = build_login(&raw, &folder_map) {
                    preview.items.push(item);
                } else {
                    preview.skipped += 1;
                }
            }
            2 => {
                if let Some(item) = build_secure_note(&raw, &folder_map) {
                    preview.items.push(item);
                } else {
                    preview.skipped += 1;
                }
            }
            3 => bump(&mut preview.skipped_by_type, "card"),
            4 => bump(&mut preview.skipped_by_type, "identity"),
            _ => bump(&mut preview.skipped_by_type, "unknown"),
        }
    }
    preview.skipped = preview.skipped_by_type.values().sum();

    Ok(preview)
}

fn build_login(raw: &Value, folder_map: &HashMap<String, String>) -> Option<Item> {
    let title = str_field(raw, "name").unwrap_or_default();
    if title.trim().is_empty() {
        return None;
    }
    let login = raw.get("login");

    // 多 URI 策略【已裁定 Q2】：取第一条。
    let url = login
        .and_then(|l| l.get("uris"))
        .and_then(|u| u.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("uri"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let username = login.and_then(|l| str_field(l, "username"));
    let password = login.and_then(|l| str_field(l, "password"));
    let totp_secret = login.and_then(|l| str_field(l, "totp"));

    let custom_fields = map_fields(raw.get("fields"));

    let now = now_ms();
    Some(Item {
        item_id: uuid::Uuid::new_v4().to_string(),
        kind: ItemKind::Login(LoginData {
            title,
            url,
            username,
            password,
            notes: str_field(raw, "notes"),
            totp_secret,
            custom_fields,
        }),
        folder_id: map_folder(raw, folder_map),
        favorite: raw
            .get("favorite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        created_at: now,
        updated_at: now,
        rev: 1,
        modified_by: "import".to_string(),
        attachments: Vec::new(),
        pinned: false,
        pin_order: 0,
        use_count: 0,
        last_used_at: None,
    })
}

fn build_secure_note(raw: &Value, folder_map: &HashMap<String, String>) -> Option<Item> {
    let title = str_field(raw, "name").unwrap_or_default();
    if title.trim().is_empty() {
        return None;
    }
    let now = now_ms();
    Some(Item {
        item_id: uuid::Uuid::new_v4().to_string(),
        kind: ItemKind::SecureNote(SecureNoteData {
            title,
            body: str_field(raw, "notes").unwrap_or_default(),
        }),
        folder_id: map_folder(raw, folder_map),
        favorite: raw
            .get("favorite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        created_at: now,
        updated_at: now,
        rev: 1,
        modified_by: "import".to_string(),
        attachments: Vec::new(),
        pinned: false,
        pin_order: 0,
        use_count: 0,
        last_used_at: None,
    })
}

/// `fields[]` → `custom_fields`（type=1 表示隐藏）。
fn map_fields(fields: Option<&Value>) -> Vec<CustomField> {
    let mut out = Vec::new();
    if let Some(arr) = fields.and_then(|v| v.as_array()) {
        for f in arr {
            let key = f
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let value = f
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let hidden = f.get("type").and_then(|v| v.as_i64()).unwrap_or(0) == 1;
            if key.is_empty() && value.is_empty() {
                continue;
            }
            out.push(CustomField { key, value, hidden });
        }
    }
    out
}

fn map_folder(raw: &Value, folder_map: &HashMap<String, String>) -> Option<String> {
    raw.get("folderId")
        .and_then(|v| v.as_str())
        .and_then(|id| folder_map.get(id).cloned())
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn bump(map: &mut HashMap<String, usize>, key: &str) {
    *map.entry(key.to_string()).or_insert(0) += 1;
}
