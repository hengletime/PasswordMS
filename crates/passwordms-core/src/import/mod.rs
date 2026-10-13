//! Bitwarden 导入（架构文档 §3.3 F-14、§7 映射表、§10 U8）。
//!
//! 统一流程：`preview`（解析 + 字段映射，生成新 UUID 与分组映射）→ `apply`（落库）。
//! 不支持类型（card / identity）跳过并计数，不阻塞整体导入。

pub mod bitwarden_csv;
pub mod bitwarden_json;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::model::folder::Folder;
use crate::model::item::Item;
use crate::store::repository::Repository;

/// 导入源格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Json,
    Csv,
}

impl ImportFormat {
    /// 解析前端传入的格式字符串。
    pub fn parse(s: &str) -> CoreResult<Self> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(ImportFormat::Json),
            "csv" => Ok(ImportFormat::Csv),
            _ => Err(CoreError::Validation("import_format".to_string())),
        }
    }
}

/// 导入预览（已生成新 UUID；`items[].folder_id` 指向预览内新分组）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub folders: Vec<Folder>,
    pub items: Vec<Item>,
    /// 源文件中的条目总数（含被跳过）。
    pub total: usize,
    /// 被跳过的条目数。
    pub skipped: usize,
    /// 按类型统计的跳过数（如 `card` / `identity` / `unknown`）。
    pub skipped_by_type: HashMap<String, usize>,
}

impl ImportPreview {
    /// 擦除预览中的全部条目 / 分组明文（A-04：锁定后不得残留会话明文）。
    pub fn wipe_plaintext(&mut self) {
        for item in self.items.iter_mut() {
            item.kind.wipe_plaintext();
        }
        for folder in self.folders.iter_mut() {
            folder.wipe_plaintext();
        }
    }
}

/// 导入执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub imported_items: usize,
    pub imported_folders: usize,
    pub skipped: usize,
}

/// 从文件解析并生成预览。
pub fn preview_from_file(path: &Path, format: ImportFormat) -> CoreResult<ImportPreview> {
    let content = std::fs::read_to_string(path)?;
    preview_from_str(&content, format)
}

/// 从字符串解析并生成预览。
pub fn preview_from_str(content: &str, format: ImportFormat) -> CoreResult<ImportPreview> {
    match format {
        ImportFormat::Json => bitwarden_json::parse(content),
        ImportFormat::Csv => bitwarden_csv::parse(content),
    }
}

/// 将预览落库（直接写入。
pub fn apply(preview: &ImportPreview, repo: &mut Repository) -> CoreResult<ImportResult> {
    for folder in &preview.folders {
        repo.put_folder_direct(folder.clone())?;
    }
    for item in &preview.items {
        repo.put_item_direct(item.clone())?;
    }
    repo.persist_counts()?;
    Ok(ImportResult {
        imported_items: preview.items.len(),
        imported_folders: preview.folders.len(),
        skipped: preview.skipped,
    })
}
