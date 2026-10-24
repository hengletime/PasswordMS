//! Android `content://` URI 的读写收敛点（D-66 / D-71）。
//!
//! 背景：Android 的文件选择器 / 保存对话框返回的都是 **ContentProvider URI**
//! （`content://…`），内核里没有对应路径，`std::fs::read` / `std::fs::write` 必然失败：
//!
//! - 读：`@tauri-apps/plugin-dialog` 的 `open()` → `DialogPlugin.kt`：`uris.add(uri?.toString())`；
//! - 写：`saveFileDialog` 用 `Intent.ACTION_CREATE_DOCUMENT`，
//!   `DialogPlugin.kt`：`callResult.put("file", uri.toString())`。
//!
//! 唯一可行通道是 provider 的 file descriptor —— `tauri-plugin-fs` 的 Android 实现
//! （`src/android.rs::resolve_content_uri` → `run_mobile_plugin("getFileDescriptor")`
//! → `FsPlugin.kt` 的 `openAssetFileDescriptor(uri, mode)`）就是干这个的，插件已在
//! `lib.rs` 用 `tauri_plugin_fs::init()` 注册；Rust 侧直接调插件 API **不经 IPC 权限**，
//! 因此 **capabilities 无需改动**。
//!
//! ## 平台说明
//!
//! 两个函数**都不加 `cfg` 门控**：桌面侧 `tauri_plugin_fs` 的 `Fs::open` 与 Android
//! 同名同签名（`FilePath` / `OpenOptions` 都是跨平台类型），所以它们在桌面目标下同样
//! 参与编译，可被 `cargo check` / `clippy` 覆盖（否则「没编过等于没写过」）。
//! 只有 Android 才会真正调用（各命令的 `content://` 分支都在 `cfg(target_os = "android")` 内）。

use std::io::{Read as _, Write as _};

use tauri::AppHandle;
use tauri_plugin_fs::{FilePath, FsExt as _, OpenOptions};

use crate::error::IpcError;

/// 读取 `content://` URI 指向文件的内容（**有上限**，见下）。
///
/// 为什么不能 `std::fs::read`：见模块文档——ContentProvider URI 没有内核路径，
/// 必须经 provider 的 file descriptor 才能读。
///
/// **内存上限**：最多只读 [`passwordms_core::constants::MAX_ATTACHMENT_BYTES`] + 1 字节，
/// 超限直接 `ERR_ATTACHMENT_TOO_LARGE`——SAF 允许用户选中几百 MB 的视频，不能先整份
/// 读进内存再让 core 拒掉（Android 上那是一次 OOM 级别的内存尖峰）。
/// 因此返回的 `data.len()` **只保证**「≤ `MAX_ATTACHMENT_BYTES`」或「超限时已被拒绝」，
/// **不是文件真实大小**：调用方不得据此推断文件尺寸（≤ 20 MB 时二者相同，超过则已提前
/// 报错返回）。真实大小判定仍由 `Repository::insert_attachment` 按同一常量负责。
///
/// 虚拟文件（云盘类 provider 不返回 fd）→ 插件报 `ErrorKind::Unsupported`，仍映射
/// `ERR_IO`（不新增错误码）。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn read_to_bytes(app: &AppHandle, uri: &str) -> Result<Vec<u8>, IpcError> {
    // 非 UTF-8 / 非合法 URI → 与文件读写失败同码（ERR_IO），不引入新错误语义。
    let url = tauri::Url::parse(uri).map_err(|_| IpcError::io("invalid_content_uri"))?;
    let mut opts = OpenOptions::new();
    opts.read(true);
    // `take` 按值消费 `File`（`&mut` 只加在 `Take` 适配器上），故 `file` 无需 `mut`。
    let file = app
        .fs()
        .open(FilePath::Url(url), opts)
        .map_err(|e| IpcError::from(passwordms_core::CoreError::from(e)))?;
    let limit = passwordms_core::constants::MAX_ATTACHMENT_BYTES;
    let mut data = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|e| IpcError::from(passwordms_core::CoreError::from(e)))?;
    if data.len() as u64 > limit {
        return Err(IpcError::from(
            passwordms_core::CoreError::AttachmentTooLarge,
        ));
    }
    Ok(data)
}

/// 把字节写入 `content://` URI 指向的文件（D-71：Android 导出附件 / 写导入模板）。
///
/// 需求来自真机反馈：`saveFileDialog` 返回 `content://com.android.providers.…`，
/// 被当路径交给 `std::fs::write` → `EINVAL` → `ERR_IO` →「文件读写出错」。
///
/// ## 打开模式为什么是 `"wt"`
///
/// `tauri-plugin-fs` 的 Android 侧把 `OpenOptions` 映射成 `ParcelFileDescriptor`
/// 的 mode 字符串（`lib.rs::OpenOptions::android_mode`）：
///
/// | read | write | truncate | append | android mode |
/// |------|-------|----------|--------|--------------|
/// | false | true | **true** | false | **`"wt"`**（写 + 截断） |
/// | false | true | false | false | `"w"`（写，不截断，旧字节会残留） |
/// | true | true | true | false | `"rwt"`（多要一个读权限，SAF 不一定给） |
///
/// 选 `"wt"` 的理由：
///
/// - SAF 的 `ACTION_CREATE_DOCUMENT` 已经**建好**目标文件，我们只需覆盖内容——
///   不截断会在「新文件比旧文件短」时留下尾部残字节；
/// - 不需要读权限（`"rwt"` 在部分 provider 上会因读权限被拒）；
/// - 不设 `create`（文件由系统创建，`"w"`/`"wt"` 都不带 `O_CREAT` 语义）。
///
/// 对应代码即 `OpenOptions` 的 `read=false`、`write=true`、`truncate=true`、
/// `create=false`、`append=false` 这一组合（`OpenOptions::new()` 后只打开需要的项，
/// 因为插件把字段设为私有，外部只能用 builder 方法设置）。
///
/// 写入完成后由 `File` 的 `Drop` 关闭 fd（SAF 在 close 时提交），`write_all` 本身
/// 不经用户态缓冲，无需额外 `flush`。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn write_bytes(app: &AppHandle, uri: &str, data: &[u8]) -> Result<(), IpcError> {
    let url = tauri::Url::parse(uri).map_err(|_| IpcError::io("invalid_content_uri"))?;
    // read=false, write=true, truncate=true, create=false, append=false → android_mode() = "wt"
    let mut opts = OpenOptions::new();
    opts.write(true).truncate(true);
    let mut file = app
        .fs()
        .open(FilePath::Url(url), opts)
        .map_err(|e| IpcError::from(passwordms_core::CoreError::from(e)))?;
    file.write_all(data)
        .map_err(|e| IpcError::from(passwordms_core::CoreError::from(e)))?;
    // 不显式 drop：`file` 在函数返回时销毁，其内部 fd 随 `OwnedFd` 关闭
    // （SAF 于 close 时提交内容）；`write_all` 不经用户态缓冲，无需额外 flush。
    Ok(())
}

/// 把 `content://` URI 指向的文件**流式拷贝**到本地路径（D-94：Android「从备份恢复」）。
///
/// 需求来自真机反馈：锁屏页「从备份恢复」是全仓最后一个没接 `content://` 分支的
/// 选文件入口——SAF `open()` 返回 `content://…` 被当路径交给 `std::fs` 必失败
/// → `ERR_IO` →「文件读写错误」。
///
/// 为什么不用 [`read_to_bytes`]：库文件没有 20 MB 附件上限可依——超限库会被
/// `ERR_ATTACHMENT_TOO_LARGE` 误拒（实测存在 24 MB 的未压缩库），且整库进内存是
/// 一次无谓的尖峰；`io::copy` 走 fd 流式，内存占用与文件大小无关、没有大小上限。
/// 备份的合法性（尺寸 / magic / 整库帧流校验）仍由调用方在本地临时文件上跑
/// [`passwordms_core::store::container::restore_backup_from`]，本函数只负责搬运字节。
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn copy_to_file(
    app: &AppHandle,
    uri: &str,
    dest: &std::path::Path,
) -> Result<(), IpcError> {
    let url = tauri::Url::parse(uri).map_err(|_| IpcError::io("invalid_content_uri"))?;
    let mut opts = OpenOptions::new();
    opts.read(true);
    let mut src = app
        .fs()
        .open(FilePath::Url(url), opts)
        .map_err(|e| IpcError::from(passwordms_core::CoreError::from(e)))?;
    let mut out = std::fs::File::create(dest)
        .map_err(|e| IpcError::from(passwordms_core::CoreError::from(e)))?;
    std::io::copy(&mut src, &mut out)
        .map_err(|e| IpcError::from(passwordms_core::CoreError::from(e)))?;
    // `out` 返回时 Drop 落盘关闭；`src` 的 fd 随 Drop 关闭（SAF close 即提交）。
    Ok(())
}
