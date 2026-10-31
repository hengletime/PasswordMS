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
///
/// **D-134**：`peer_ip` / `peer_port` 上屏。手动同步失败时最需要回答的问题是
/// 「我记的地址和对方现在用的地址是不是同一个」——这两个值原来只存在于 core 的
/// `PairedDevice` 和日志里，界面上看不到，用户只能看到一句「网络不可达」。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDto {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub paired_at: i64,
    pub last_sync_at: Option<i64>,
    /// 配对记录里保存的对端 IPv4（`None` = 从未记下地址）。
    pub peer_ip: Option<String>,
    /// 配对记录里保存的对端重同步端口（`None` = 从未协商过）。
    pub peer_port: Option<u16>,
}

impl From<PairedDevice> for DeviceDto {
    fn from(d: PairedDevice) -> Self {
        Self {
            device_id: d.device_id,
            device_name: d.device_name,
            fingerprint: d.fingerprint,
            paired_at: d.paired_at,
            last_sync_at: d.last_sync_at,
            peer_ip: d.peer_ip,
            peer_port: d.peer_port,
        }
    }
}

/// 本机同步网络信息（**只读诊断**，D-134）。
///
/// 为什么要新开一个命令而不是塞进 `sync_status`：这个值要在**未解锁**时也能回答
/// 「我在不在监听」吗？——不需要，同步本来就只在解锁期存在。但 `sync_status`
/// 的语义是「库里的同步事实」，而这份是「本机此刻的网络事实」，混在一起会让
/// 调用方在缓存/刷新时取到两个不同时钟的量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncNetInfoDto {
    /// 本机全部局域网 IPv4（多网卡机器会有多条）。
    pub local_ips: Vec<String>,
    /// 当前重同步 TCP 监听端口（`None` = 监听器未启动/已随锁定停止）。
    pub listen_port: Option<u16>,
    /// UDP 发现应答器是否绑定成功（`false` = 端口被占/无权限，未启用的机器不会应答别人的发现广播）。
    pub discovery_enabled: bool,
    /// 配对时的固定发现端口（供用户核对两端一致）。
    pub discovery_port: u16,
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
    /// D-151：「待同步」这个数字由哪台设备的落后程度决定（`None` = 无配对设备）。
    ///
    /// 没有归因时，用户面对一个恒定的 58 完全无从下手——他不知道是「同步失败」还是
    /// 「有条历史残影设备一条都没同步过」。给出名字，他就能去设备页找到那一行处理。
    #[serde(default)]
    pub pending_device_name: Option<String>,
    /// D-151：该设备是否**从未同步过**。真 = 这 58 条不是「同步失败」，而是
    /// 「这台设备一条都没拿到」（残影/刚配对未同步）。
    #[serde(default)]
    pub pending_device_never_synced: bool,
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

/// 应用级配置（v1.4.10）：热键 / 开机自启 / 托盘提示。
///
/// 与 `VaultSettings` 的区别：这份**不随保险库加密**，锁屏态与冷启动即可用
/// （详见 `src-tauri/src/prefs.rs` 模块文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefsDto {
    /// 当前平台是否具备系统集成能力（桌面 true / 移动端 false）。
    ///
    /// 前端据此**整卡不渲染**——与 D-116 同一口径：「还没有的功能不做广告」。
    /// 平台身份由后端自报，前端不做 UA 嗅探（`bio-platform` 的教训）。
    pub supported: bool,
    /// 规范化后的热键串（`ctrl+alt+KeyP`）。**空串 = 用户主动禁用热键**。
    pub hotkey: String,
    /// 该热键此刻在系统里是否真的注册着。
    ///
    /// `false` 有两种含义，靠 `hotkey` 是否为空区分：空 = 用户关掉的（正常）；
    /// 非空 = 注册失败（被占用），设置页要显示红字。
    pub hotkey_ok: bool,
    /// 开机自启**系统实际状态**（读注册表，不是读我们的配置）。
    ///
    /// 用户在任务管理器里禁用了启动项时，这里如实反映——界面不该撒谎。
    pub autostart: bool,
}
