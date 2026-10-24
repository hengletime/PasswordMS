//! Bitwarden 导入（架构文档 §3.3 F-14、§7 映射表、§10 U8）。
//!
//! 统一流程：`preview`（解析 + 字段映射，生成新 UUID 与分组映射）→ `apply`（落库）。
//! 不支持类型（card / identity）跳过并计数，不阻塞整体导入。

pub mod bitwarden_csv;
pub mod bitwarden_json;
pub mod template;

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
    let bytes = std::fs::read(path)?;
    let content = decode_source(&bytes)?;
    preview_from_str(&content, format)
}

/// 导入源字节 → UTF-8 文本（D-87 编码容错）。
///
/// 背景：中文 Windows 上用 Excel / WPS 编辑 CSV 再保存，默认写成 ANSI（GBK）；
/// 旧实现 `std::fs::read_to_string` 严格要求 UTF-8，GBK 字节抛 `InvalidData`
/// → 被笼统映射为 `ERR_IO`「文件读写出错」，与真实原因（编码）不符。
///
/// 探测顺序（Excel / WPS / 记事本三种真实产源全覆盖）：
/// 1. UTF-8 BOM（`EF BB BF`）→ 按 UTF-8 解码并**剥掉 BOM**（BOM 留在串首会污染
///    首列表名 `folder`，导致映射失败）；
/// 2. UTF-16 LE / BE BOM → 按对应端序解码（Excel「Unicode 文本」产源）；
/// 3. 无 BOM：先按 UTF-8 严格解码（纯 ASCII / 真正的 UTF-8 走快路径，行为与旧版一致）；
/// 4. UTF-8 失败 → 按 GBK 解码；仍有无法映射的字节（二进制垃圾）→
///    `ImportParse("encoding")`（`err.import_parse`，不再误报 IO）。
pub fn decode_source(bytes: &[u8]) -> CoreResult<String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let text = std::str::from_utf8(&bytes[3..]).map_err(|_| {
            CoreError::ImportParse("encoding".to_string())
        })?;
        return Ok(text.to_string());
    }
    // encoding_rs 的 `decode` 只在 UTF-8 时做 BOM 嗅探；显式指定 UTF-16 端序时
    // BOM 会原样留在输出首（U+FEFF 污染 CSV 首列名）——先手动剥掉再解。
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (text, _, had_errors) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        if had_errors {
            return Err(CoreError::ImportParse("encoding".to_string()));
        }
        return Ok(text.into_owned());
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, had_errors) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        if had_errors {
            return Err(CoreError::ImportParse("encoding".to_string()));
        }
        return Ok(text.into_owned());
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_string()),
        Err(_) => {
            let (text, _, had_errors) = encoding_rs::GBK.decode(bytes);
            if had_errors {
                return Err(CoreError::ImportParse("encoding".to_string()));
            }
            Ok(text.into_owned())
        }
    }
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
