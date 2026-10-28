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

/// 生物识别快速解锁状态（`docs/08-biometric-unlock.md` §10 / §23）。
///
/// **只带状态，不带任何密钥材料**（§18.5 约束 6）：IPC 里永远不出现 VK、`sealed_vk`
/// 或 `bind`。前端拿到这几个字段足以决定「按钮显不显示、置不置灰、旁边写哪句文案」。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiometricStatusDto {
    /// 平台可用性：`available` / `not_configured` / `not_present` / `disabled_by_policy`
    /// / `busy` / `unsupported` / `not_implemented`（见 `settings.bio_avail_*`）。
    pub availability: String,
    /// 封印状态：`absent`（未开启 / 别的库 / 未知版本）/ `corrupt`（损坏）/ `ready`
    /// / `stale`（已失效，原因在 [`Self::reason`]）。
    pub state: String,
    /// 失效原因：`master_password_changed` / `vault_key_changed` / `enrollment_changed`
    /// / `tampered`（见 `settings.bio_stale_*`）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 封印创建时间（毫秒，仅 `ready` 时有值），设置页显示「已开启（2026-10-28）」。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_at: Option<i64>,
    /// 当前平台：`windows` / `android` / `other`（§23）。
    ///
    /// 前端据此选择平台专属名词（「Windows Hello」/「指纹 · 人脸」）与风险说明。
    /// **不用 `navigator.userAgent` 嗅探**：UA 随 WebView 版本变化，且同一个 UA
    /// 字符串在不同 Android 厂商 ROM 上并不稳定——由后端说出自己是谁，才是唯一可靠的来源。
    pub platform: String,
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
///
/// #531：`use_count` / `last_used_at` 是**组内条目的聚合值**（Σ`use_count` /
/// max`last_used_at`），由 [`passwordms_core::store::repository::Repository::folder_usage`]
/// 现算 —— 与 `list_folders` 的排序键同一来源，UI 显示的数字与排序看到的必然一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderDto {
    pub folder_id: String,
    pub name: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    /// 遗留字段：#531 起排序不再消费（字段保留仅为兼容，恒为落库值）。
    pub sort_order: i32,
    pub rev: u64,
    pub updated_at: i64,
    /// 是否钉住（#531）。
    pub pinned: bool,
    /// 钉住块内顺序（#531）。
    pub pin_order: i64,
    /// 组内条目 `use_count` 之和（#531 聚合值）。
    pub use_count: i64,
    /// 组内条目 `last_used_at` 最大值（#531 聚合值）。
    pub last_used_at: Option<i64>,
}

impl FolderDto {
    /// 由 `Folder` 与其组内频度聚合构造。
    ///
    /// **不用 `From<Folder>`**：聚合值依赖 `items`（`Folder` 自身不含），任何
    /// "默认 0/None" 的隐式路径都会让 DTO 与排序依据脱节，故构造点必须显式传入
    /// [`FolderUsage`](passwordms_core::store::repository::FolderUsage)。
    pub fn from_folder(f: Folder, usage: passwordms_core::store::repository::FolderUsage) -> Self {
        Self {
            folder_id: f.folder_id,
            name: f.name,
            icon: f.icon,
            color: f.color,
            sort_order: f.sort_order,
            rev: f.rev,
            updated_at: f.updated_at,
            pinned: f.pinned,
            pin_order: f.pin_order,
            use_count: usage.use_count,
            last_used_at: usage.last_used_at,
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
    /// D-90：同名同型条目跳过数。
    pub duplicate_skipped: usize,
    /// D-90：与现有库同名而合并（未新建）的分组数。
    pub merged_folders: usize,
}

impl From<ImportResult> for ImportResultDto {
    fn from(r: ImportResult) -> Self {
        Self {
            imported_items: r.imported_items,
            imported_folders: r.imported_folders,
            skipped: r.skipped,
            duplicate_skipped: r.duplicate_skipped,
            merged_folders: r.merged_folders,
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
    /// D-69：本次配对的**会话码**，`sha256(host_eph 公钥)` 前 8 字节的小写 hex（16 位）。
    ///
    /// 与 `PairResultDto.session_code`（joining 侧由 `Accept.host_pub` 算出）**同值**，
    /// 两屏构造上必然相等 —— 这才是用户能核对的量。
    ///
    /// **不是**「设备指纹」：会话码每次配对都会变（绑的是本次临时密钥），
    /// 而设备指纹（`DeviceDto.fingerprint`，长期身份密钥）稳定不变。两者不可混用。
    pub session_code: String,
    pub expires_at: i64,
}

/// 配对结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairResultDto {
    pub device_id: String,
    pub device_name: String,
    /// Host 的**长期设备指纹**（取自二维码 `fp`，稳定），用于设备列表辨识。
    pub device_fingerprint: String,
    /// 本次配对的**会话码**（`sha256(Accept.host_pub)`），与电脑屏显示的必须是同一个值。
    pub session_code: String,
    pub applied: usize,
    pub conflicts: Vec<String>,
    /// D-53：blob 阶段实际入库的附件数（老前端缺省 0）。
    #[serde(default)]
    pub blobs: usize,
}

/// 同步结果 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResultDto {
    pub applied: usize,
    pub conflicts: Vec<String>,
    /// D-53：blob 阶段实际入库的附件数（老前端缺省 0）。
    #[serde(default)]
    pub blobs: usize,
}

/// 同步状态 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusDto {
    pub last_sync_at: Option<i64>,
    pub pending_changes: usize,
    pub device_count: usize,
}

/// 保险库文件信息 DTO。
///
/// D-108（豆包审查 3.4-8）：原字段 `path` 回传**绝对路径**给前端 JS——
/// 文件名足以让用户辨认（`passwordms.vault`，路径结构本就由应用决定、不可配置），
/// 无谓扩大信息面。字段同步更名 `file_name`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFileInfoDto {
    pub file_name: String,
    pub size_bytes: u64,
}

/// 保险库体积构成 DTO（D-76，只读统计）。
///
/// 全部字段是字节数/帧数，**不含任何路径、ID、明文或密文**。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStatsDto {
    /// 容器文件当前字节数。
    pub file_bytes: u64,
    /// 头部区字节数（压缩后不变）。
    pub header_bytes: u64,
    /// 实际有效数据字节数（压缩后保留的部分）。
    pub live_bytes: u64,
    /// 墓碑帧总字节数（死数据总量，含必须保留的少量墓碑）。
    pub tombstone_bytes: u64,
    /// 其中必须保留的墓碑字节数。
    pub kept_tombstone_bytes: u64,
    /// 压缩可回收字节数。
    pub reclaimable_bytes: u64,
    /// 压缩后预计文件字节数。
    pub after_bytes: u64,
    /// 已删/无引用附件密文字节数（回答「删掉的附件还在不在」）。
    pub attachment_dead_bytes: u64,
    /// 活跃附件密文字节数。
    pub attachment_live_bytes: u64,
    /// 保留帧数。
    pub live_frames: u32,
    /// 将被丢弃的帧数。
    pub dead_frames: u32,
}

impl From<passwordms_core::store::repository::CompactionStats> for CompactionStatsDto {
    fn from(s: passwordms_core::store::repository::CompactionStats) -> Self {
        Self {
            file_bytes: s.file_bytes,
            header_bytes: s.header_bytes,
            live_bytes: s.live_bytes,
            tombstone_bytes: s.tombstone_bytes,
            kept_tombstone_bytes: s.kept_tombstone_bytes,
            reclaimable_bytes: s.reclaimable_bytes,
            after_bytes: s.after_bytes,
            attachment_dead_bytes: s.attachment_dead_bytes,
            attachment_live_bytes: s.attachment_live_bytes,
            live_frames: s.live_frames,
            dead_frames: s.dead_frames,
        }
    }
}

/// 压缩结果 DTO（D-76）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResultDto {
    /// 压缩前文件字节数。
    pub before_bytes: u64,
    /// 压缩后文件字节数。
    pub after_bytes: u64,
    /// 实际回收字节数（`before_bytes - after_bytes`，恒 ≥ 0）。
    pub reclaimed_bytes: u64,
    /// 丢弃的帧数。
    pub dropped_frames: u32,
}

impl From<passwordms_core::store::repository::CompactionOutcome> for CompactionResultDto {
    fn from(o: passwordms_core::store::repository::CompactionOutcome) -> Self {
        Self {
            before_bytes: o.before_bytes,
            after_bytes: o.after_bytes,
            reclaimed_bytes: o.reclaimed_bytes,
            dropped_frames: o.dropped_frames,
        }
    }
}
