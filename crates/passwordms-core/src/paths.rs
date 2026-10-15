//! 便携模式数据目录判定（发布渠道：便携绿色版）。
//!
//! 规则：exe 同目录存在标记文件 `passwordms.portable`（内容不限，可为空）时，
//! 全部数据文件（保险库 / 同步身份）落在 exe 同目录，实现「拷走即用、数据随包」；
//! 否则走系统应用数据目录（Windows: `%APPDATA%\com.passwordms.app`）。
//!
//! 为什么放 core：判定逻辑是纯路径运算、可单测；`src-tauri` 的 `data_dir()`
//! 只负责「拿 exe 目录 + 按判定结果二选一」的接线。

use std::path::{Path, PathBuf};

/// 便携模式标记文件名。
pub const PORTABLE_MARKER: &str = "passwordms.portable";

/// 判定数据目录。
///
/// * `exe_dir` —— `Some(exe 所在目录)`；`None` 表示拿不到 exe 路径（理论少见，走系统目录）。
///
/// 返回 `Some(目录)` = 便携模式（目录即 exe 所在目录，必定已存在，无需创建）；
/// `None` = 常规模式，调用方使用系统应用数据目录。
pub fn portable_data_dir(exe_dir: Option<&Path>) -> Option<PathBuf> {
    let dir = exe_dir?;
    let marker = dir.join(PORTABLE_MARKER);
    if marker.is_file() {
        Some(dir.to_path_buf())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_present_uses_exe_dir() {
        let base = std::env::temp_dir().join("pms-portable-test-present");
        let _ = std::fs::create_dir_all(&base);
        let marker = base.join(PORTABLE_MARKER);
        std::fs::write(&marker, b"").unwrap();
        let resolved = portable_data_dir(Some(&base));
        assert_eq!(resolved, Some(base.clone()));
        let _ = std::fs::remove_file(&marker);
        let _ = std::fs::remove_dir(&base);
    }

    #[test]
    fn marker_absent_falls_back_to_none() {
        let base = std::env::temp_dir().join("pms-portable-test-absent");
        let _ = std::fs::create_dir_all(&base);
        assert_eq!(portable_data_dir(Some(&base)), None);
    }

    #[test]
    fn no_exe_dir_is_none() {
        assert_eq!(portable_data_dir(None), None);
    }

    #[test]
    fn marker_is_directory_not_file_ignored() {
        // 防呆：标记必须是文件；同名目录不算数。
        let base = std::env::temp_dir().join("pms-portable-test-dir");
        let marker = base.join(PORTABLE_MARKER);
        let _ = std::fs::create_dir_all(&marker);
        assert_eq!(portable_data_dir(Some(&base)), None);
        let _ = std::fs::remove_dir(&marker);
        let _ = std::fs::remove_dir(&base);
    }
}
