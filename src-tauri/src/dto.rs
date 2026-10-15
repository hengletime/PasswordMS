//! IPC 传输 DTO（架构文档 §5.2）。wire 格式统一 `snake_case`。
//!
//! 说明：复杂类型（`Item`）直接复用核心模型（其 serde 形态与 DTO 一致），
//! 其余以显式结构体定义并提供 `From` 转换，保证前后端契约稳定。

use serde::{Deserialize, Serialize};

use passwordms_core::health::HealthReport;
use passwordms_core::import::{ImportPreview, ImportResult};
use passwordms_core::model::conflict::ConflictEntry;
use passwordms_core::model::folder::Folder;
use passwordms_core::model::item::{Item, ItemKind};
use passwordms_core::model::sync_state::PairedDevice;
use passwordms_core::model::trash::TrashEntry;
use passwordms_core::model::vault::{GeneratorOptions, HealthConfig, VaultSettings};
use passwordms_core::totp::TotpCode;

/// 启动 / 解锁状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusDto {
    pub has_vault: bool,
    pub locked: bool,
}

/// 条目 DTO（直接复用核心 `Item`：字段与 `ItemKind` 内部标签均符合契约）。
pub type ItemDto = Item;

/// 条目草稿 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDraftDto {
    pub kind: ItemKind,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub favorite: bool,
}

/// 分组 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderDto {
    pub folder_id: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub sort_order: i32,
    pub rev: u64,
    pub updated_at: i64,
}

impl From<Folder> for FolderDto {
    fn from(f: Folder) -> Self {
        Self {
            folder_id: f.folder_id,
            name: f.name,
            icon: f.icon,
            color: f.color,
            sort_order: f.sort_order,
            rev: f.rev,
            updated_at: f.updated_at,
        }
    }
}

/// 回收站 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashDto {
    pub trash_id: String,
    pub item_id: String,
    pub title: String,
    pub original_folder_id: Option<String>,
    pub deleted_at: i64,
    pub expire_at: i64,
}

impl From<TrashEntry> for TrashDto {
    fn from(e: TrashEntry) -> Self {
        Self {
            trash_id: e.trash_id,
            item_id: e.item_snapshot.item_id.clone(),
            title: e.item_snapshot.title().to_string(),
            original_folder_id: e.original_folder_id,
            deleted_at: e.deleted_at,
            expire_at: e.expire_at,
        }
    }
}

/// 冲突 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictDto {
    pub conflict_id: String,
    pub item_id: String,
    pub local_title: String,
    pub remote_title: String,
    pub local_updated_at: i64,
    pub remote_updated_at: i64,
    pub detected_at: i64,
}

impl From<ConflictEntry> for ConflictDto {
    fn from(c: ConflictEntry) -> Self {
        Self {
            conflict_id: c.conflict_id,
            item_id: c.item_id,
            local_title: c.local_snapshot.title().to_string(),
            remote_title: c.remote_snapshot.title().to_string(),
            local_updated_at: c.local_updated_at,
            remote_updated_at: c.remote_updated_at,
            detected_at: c.detected_at,
        }
    }
}

/// 已配对设备 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDto {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub paired_at: i64,
    pub last_sync_at: Option<i64>,
}

impl From<PairedDevice> for DeviceDto {
    fn from(d: PairedDevice) -> Self {
        Self {
            device_id: d.device_id,
            device_name: d.device_name,
            fingerprint: d.fingerprint,
            paired_at: d.paired_at,
            last_sync_at: d.last_sync_at,
        }
    }
}

/// 体检报告 DTO（复用核心结构）。
pub type HealthReportDto = HealthReport;

/// 生成器选项 DTO（复用核心结构）。
pub type GeneratorOptionsDto = GeneratorOptions;

/// 体检配置 DTO（复用核心结构）。
pub type HealthConfigDto = HealthConfig;

/// 设置 DTO（复用核心结构）。
pub type VaultSettingsDto = VaultSettings;

/// TOTP DTO（复用核心结构）。
pub type TotpDto = TotpCode;

/// 生成结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedPasswordDto {
    pub password: String,
}

/// 导入预览 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreviewDto {
    pub preview_token: String,
    pub item_count: usize,
    pub folder_count: usize,
    pub skipped: usize,
    pub skipped_by_type: std::collections::HashMap<String, usize>,
    pub total: usize,
}

impl ImportPreviewDto {
    /// 由核心预览构造（`token` 由调用方生成）。
    pub fn from_preview(token: String, preview: &ImportPreview) -> Self {
        Self {
            preview_token: token,
            item_count: preview.items.len(),
            folder_count: preview.folders.len(),
            skipped: preview.skipped,
            skipped_by_type: preview.skipped_by_type.clone(),
            total: preview.total,
        }
    }
}

/// 导入结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResultDto {
    pub imported_items: usize,
    pub imported_folders: usize,
    pub skipped: usize,
}

impl From<ImportResult> for ImportResultDto {
    fn from(r: ImportResult) -> Self {
        Self {
            imported_items: r.imported_items,
            imported_folders: r.imported_folders,
            skipped: r.skipped,
        }
    }
}

/// Host 配对信息 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingDto {
    /// 二维码内联 SVG（不使用图标字体 / 图像编解码依赖）。
    pub qr_svg: String,
    /// 二维码文本（`PMS1:base64url(JSON)`），支持手动输入兜底（U3）。
    pub qr_text: String,
    pub ip: String,
    /// 全部候选局域网 IPv4（多网卡时主选在前，D-44）。
    pub ips: Vec<String>,
    pub port: u16,
    pub expires_at: i64,
}

/// 配对结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairResultDto {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub applied: usize,
    pub conflicts: Vec<String>,
}

/// 同步结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResultDto {
    pub applied: usize,
    pub conflicts: Vec<String>,
}

/// 同步状态 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusDto {
    pub last_sync_at: Option<i64>,
    pub pending_changes: usize,
    pub device_count: usize,
}

/// 保险库文件信息 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFileInfoDto {
    pub path: String,
    pub size_bytes: u64,
}
