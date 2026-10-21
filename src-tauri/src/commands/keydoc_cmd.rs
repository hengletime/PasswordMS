//! 密钥文档命令：附件添加 / 移除 / 导出（架构文档 §5.2、F-12；D-60 §3.14.5）。
//!
//! D-60 语义：附件是登录/安全笔记条目的**附属内容**（父条目内嵌 `attachments` 数组），
//! 不再是独立条目。`import_attachment` 更名 `add_attachment`（按父条目定位），
//! 新增 `remove_attachment`，`export_attachment` / `attachment_blob_ready` 改按
//! `attachment_id` 定位。
//!
//! D-66 根因（Android 附件必失败）：`@tauri-apps/plugin-dialog` 在 Android 上的
//! `open()` 返回的是 **`content://` URI 字符串**（`tauri-plugin-dialog` 的
//! `DialogPlugin.kt`：`uris.add(uri?.toString())`），不是文件系统路径。此前后端把它
//! 直接当路径交给 `Repository::add_attachment`，第一步 `std::fs::metadata("content://…")`
//! 就必然 `NotFound` → `ERR_IO` → toast「文件读写出错。」，手机上永远加不进附件。
//! 修法：`content://` 走 `tauri-plugin-fs` 的 `getFileDescriptor` 读字节 →
//! `Repository::add_attachment_bytes`，文件名由
//! [`passwordms_core::attachment_name::filename_from_source`] 百分号解码得到。

use std::path::Path;

#[cfg(target_os = "android")]
use passwordms_core::attachment_name::filename_from_source;
use passwordms_core::CoreError;

use crate::dto::ItemDto;
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::AppState;

/// 读取 `content://` URI 指向文件的内容（**有上限**，见下）。
///
/// 为什么不能 `std::fs::read`：Android 的文件选择器返回的是 ContentProvider URI，
/// 内核里没有对应路径；必须经 provider 的 file descriptor 才能读。`tauri-plugin-fs`
/// 的 Android 实现正是这么做的（`src/android.rs::resolve_content_uri` →
/// `run_mobile_plugin("getFileDescriptor")`），且插件已在 `lib.rs` 用
/// `tauri_plugin_fs::init()` 注册，Rust 侧直接调插件 API 不经 IPC 权限，**无需改
/// capabilities**。
///
/// 平台说明：函数体**不加 `cfg` 门控**——桌面侧 `tauri-plugin-fs` 的 `Fs::open` 与
/// Android 同名同签名（`FilePath` / `OpenOptions` 都是跨平台类型），因此本函数在桌面
/// 目标下同样参与编译，可被 `cargo check` / `clippy` 验证；只有 Android 才会真正走到
/// 这里（桌面 `cfg` 分支不调用它）。虚拟文件（如云盘）provider 可能不返回 fd，
/// 插件以 `ErrorKind::Unsupported` 报错 → 仍映射 `ERR_IO`，不新增错误码。
///
/// **内存上限**：最多只读 [`passwordms_core::constants::MAX_ATTACHMENT_BYTES`] + 1 字节，
/// 超限直接 `ERR_ATTACHMENT_TOO_LARGE`——SAF 允许用户选中几百 MB 的视频，不能先整份
/// 读进内存再让 core 拒掉（Android 上那是一次 OOM 级别的内存尖峰）。
/// 因此返回的 `data.len()` **只保证**「≤ `MAX_ATTACHMENT_BYTES`」或「超限时已被拒绝」，
/// **不是文件真实大小**：调用方不得据此推断文件尺寸（≤ 20 MB 时二者相同，超过则已提前
/// 报错返回）。真实大小判定仍由 `Repository::insert_attachment` 按同一常量负责。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn read_content_uri(app: &tauri::AppHandle, uri: &str) -> Result<Vec<u8>, IpcError> {
    use std::io::Read as _;
    use tauri_plugin_fs::{FilePath, FsExt as _, OpenOptions};

    // 非 UTF-8 / 非合法 URI → 与文件读写失败同码（ERR_IO），不引入新错误语义。
    let url = tauri::Url::parse(uri).map_err(|_| IpcError::io("invalid_content_uri"))?;
    let mut opts = OpenOptions::new();
    opts.read(true);
    // `take` 按值消费 `File`（`&mut` 只加在 `Take` 适配器上），故 `file` 无需 `mut`。
    let file = app
        .fs()
        .open(FilePath::Url(url), opts)
        .map_err(|e| IpcError::from(CoreError::from(e)))?;
    // D-66：只读到「上限 + 1」字节即停——超限文件在读进内存前就判死，避免手机
    // 选到几百 MB 视频时整份读入触发 OOM（桌面路径由 `add_attachment` 的 metadata
    // 预检承担同一职责，两条路径的内存峰值都被钉在 20 MB + 1 字节）。
    let limit = passwordms_core::constants::MAX_ATTACHMENT_BYTES;
    let mut data = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|e| IpcError::from(CoreError::from(e)))?;
    if data.len() as u64 > limit {
        return Err(IpcError::from(CoreError::AttachmentTooLarge));
    }
    Ok(data)
}

/// F-12 向父条目添加附件（§3.14.5）。
///
/// 校验链（core 层）：父条目存在且非遗留 attachment 型 → 附件数 ≤ 16
/// （`ERR_ATTACHMENT_LIMIT`）→ 文件名 ≤ 255B（`ERR_VALIDATION`）→ 单文件 ≤ 20 MB
/// （`ERR_ATTACHMENT_TOO_LARGE`）→ 库容量 ≤ 200 MB（`ERR_VAULT_CAPACITY_EXCEEDED`）。
/// 返回**父条目**（rev+1），前端据此刷新详情。
///
/// D-66：Android 上 `source_path` 可能是 `content://…`，此时走
/// `add_attachment_bytes`（文件名百分号解码）；其余情况（桌面任意路径、Android 上的
/// 普通路径）仍走 `add_attachment`，行为与修复前完全一致。
#[tauri::command(rename_all = "snake_case")]
pub fn add_attachment(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    item_id: String,
    source_path: String,
    title: String,
) -> IpcResult<ItemDto> {
    // 桌面目标不使用 `app`（content:// 只出现在 Android）；显式忽略避免 unused 告警。
    #[cfg(not(target_os = "android"))]
    let _ = &app;

    #[cfg(target_os = "android")]
    if source_path.starts_with("content://") {
        // 先在锁外把字节读出来：ContentProvider 读取可能慢，不占会话锁。
        let data = read_content_uri(&app, &source_path)?;
        let name = filename_from_source(&source_path);
        let mut session = state.session.lock().map_err(lock_err)?;
        let repo = session.repo_mut()?;
        return Ok(repo.add_attachment_bytes(&item_id, &name, &data)?);
    }

    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.add_attachment(&item_id, Path::new(&source_path), &title)?)
}

/// F-12 从父条目移除附件（blob 交孤儿 GC，§3.14.4）。返回父条目（rev+1）。
#[tauri::command(rename_all = "snake_case")]
pub fn remove_attachment(
    state: tauri::State<'_, AppState>,
    item_id: String,
    attachment_id: String,
) -> IpcResult<ItemDto> {
    let mut session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    Ok(repo.remove_attachment(&item_id, &attachment_id)?)
}

/// F-12 导出附件（明文风险提示与二次确认由前端负责；导出后不自动打开）。
///
/// D-53 语义保留：附件元数据在但 blob 未同步到本机时报 `ERR_BLOB_MISSING`，
/// 经 `From<CoreError>` 原样透传给前端。
#[tauri::command(rename_all = "snake_case")]
pub fn export_attachment(
    state: tauri::State<'_, AppState>,
    item_id: String,
    attachment_id: String,
    dest_path: String,
) -> IpcResult<()> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    repo.export_attachment(&item_id, &attachment_id, Path::new(&dest_path))?;
    Ok(())
}

/// D-53/D-60：附件 blob 是否已同步到本机（附件行「内容待同步」态的判定来源）。
#[tauri::command(rename_all = "snake_case")]
pub fn attachment_blob_ready(
    state: tauri::State<'_, AppState>,
    attachment_id: String,
) -> IpcResult<bool> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    Ok(repo.has_blob(&attachment_id))
}
