//! 附件展示名解析（D-66）。
//!
//! 背景：桌面端文件选择器返回的是**文件系统路径**（`C:\a\b.pdf`、`/home/a/b.pdf`），
//! 而 Android 端 `@tauri-apps/plugin-dialog` 的 `open()` 返回的是 **`content://` URI**
//! （`tauri-plugin-dialog` 的 `DialogPlugin.kt`：`uris.add(uri?.toString())`）。
//! 两者都不能直接当 `Path` 用，且 URI 里的文件名是**百分号编码**的 doc id：
//!
//! ```text
//! content://com.android.externalstorage.documents/document/primary%3ADownload%2Freport.pdf
//!                                                      └── 解码后 = primary:Download/report.pdf
//! ```
//!
//! 本模块只做一件事：把「来源字符串」翻译成**可展示的文件名**。
//! 它**不做**任何 I/O，也不校验长度/大小（那是 [`crate::store::repository`] 的职责），
//! 因此可纯函数单测、可被任意外壳复用。
//!
//! 设计取舍：
//! - **不伪造扩展名**。`content://media/external/images/media/image%3A1000000123` 这类
//!   MediaStore 项没有真实文件名，解码后就是 `image:1000000123`，原样保留——猜一个
//!   `.jpg` 会让用户误以为拿到了原始文件名（且 MIME 推断也会随之出错）。
//! - **不引入第三方 percent-encoding 依赖**：URI 只需处理 `%XX` 三字节形态，
//!   手写 10 行足够，且避免为一个文件名解析新增供应链面。

/// 无法解析出文件名时的兜底名（与 D-53 时代 `add_attachment` 的兜底一致）。
const FALLBACK_NAME: &str = "attachment";

/// `content://` URI 的 scheme 前缀。
const CONTENT_SCHEME: &str = "content://";

/// Android 文档 provider 在 doc id 里附加的定位前缀（解码后需剥掉才能看到真实文件名）。
///
/// - `document/`：部分 provider 把类型段塞进 doc id；
/// - `primary:`：外部存储「主卷」（`Download`、`DCIM` 都在它下面）；
/// - `raw:`：`raw:` 类型的真实路径包装；
/// - `home:`：Files 应用的「主目录」别名。
const PROVIDER_PREFIXES: [&str; 4] = ["document/", "primary:", "raw:", "home:"];

/// 从「文件来源」提取可展示文件名：兼容 Windows/POSIX 路径与 Android `content://` URI。
///
/// 规则：
/// 1. `content://` 开头 → 取最后一个 `/` 之后的段 → **百分号解码** → 再按 `/` 取最后
///    一段 → 剥掉 provider 前缀（[`PROVIDER_PREFIXES`]）；
/// 2. 其它情况按路径处理：取最后一个 `/` 或 `\` 之后的部分；
/// 3. 结果为空（含 `""`、`"content://"`、纯目录结尾）→ 返回 `"attachment"`。
///
/// 不做文件系统访问，也不读 `Path`——入参就是前端/插件给的原始字符串。
///
/// # Examples
///
/// ```
/// use passwordms_core::attachment_name::filename_from_source;
///
/// assert_eq!(filename_from_source(r"C:\Users\a\报告.pdf"), "报告.pdf");
/// assert_eq!(filename_from_source("/home/a/report.pdf"), "report.pdf");
/// assert_eq!(
///     filename_from_source("content://com.android.externalstorage.documents/document/primary%3ADownload%2Freport.pdf"),
///     "report.pdf"
/// );
/// assert_eq!(filename_from_source(""), "attachment");
/// ```
pub fn filename_from_source(source: &str) -> String {
    let raw = source.trim();
    if raw.is_empty() {
        return FALLBACK_NAME.to_string();
    }

    let candidate = if is_content_uri(raw) {
        // URI 的 doc id 段（最后一个 `/` 之后）→ 解码 → 再去一次 `/` 段与 provider 前缀。
        let encoded_segment = last_segment(raw, |c| c == '/');
        let decoded = percent_decode(encoded_segment);
        let tail = last_segment(&decoded, |c| c == '/');
        strip_provider_prefix(tail).to_string()
    } else {
        // 普通路径：Windows 反斜杠与 POSIX 斜杠都算分隔符。
        last_segment(raw, |c| c == '/' || c == '\\').to_string()
    };

    if candidate.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        candidate
    }
}

/// 是否为 Android `content://` URI（大小写不敏感，`Content://` 也接受）。
fn is_content_uri(source: &str) -> bool {
    source
        .get(..CONTENT_SCHEME.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(CONTENT_SCHEME))
}

/// 取按 `sep` 切分后的最后一段（无分隔符时返回整体）。
fn last_segment<F: Fn(char) -> bool>(source: &str, sep: F) -> &str {
    source.rsplit(sep).next().unwrap_or("")
}

/// 反复剥掉 [`PROVIDER_PREFIXES`] 里的前缀（`primary:` 与 `document/` 可能叠加）。
fn strip_provider_prefix(segment: &str) -> &str {
    let mut out = segment;
    loop {
        let mut changed = false;
        for prefix in PROVIDER_PREFIXES {
            if let Some(rest) = out.strip_prefix(prefix) {
                out = rest;
                changed = true;
            }
        }
        if !changed {
            return out;
        }
    }
}

/// `%XX` 百分号解码（非法转义原样保留；非 UTF-8 字节用替换符兜底，不 panic）。
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push(high * 16 + low);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 单个十六进制字符 → 数值。
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_path_yields_file_name() {
        assert_eq!(filename_from_source(r"C:\Users\a\报告.pdf"), "报告.pdf");
        assert_eq!(filename_from_source(r"C:\a\b\note.txt"), "note.txt");
    }

    #[test]
    fn posix_path_yields_file_name() {
        assert_eq!(filename_from_source("/home/a/report.pdf"), "report.pdf");
        assert_eq!(filename_from_source("/report.pdf"), "report.pdf");
        // 裸文件名（无分隔符）原样返回。
        assert_eq!(filename_from_source("report.pdf"), "report.pdf");
    }

    #[test]
    fn external_storage_content_uri_decodes_doc_id() {
        assert_eq!(
            filename_from_source(
                "content://com.android.externalstorage.documents/document/primary%3ADownload%2Freport.pdf"
            ),
            "report.pdf"
        );
        // `home:` 卷同理。
        assert_eq!(
            filename_from_source(
                "content://com.android.externalstorage.documents/document/home%3Areport.pdf"
            ),
            "report.pdf"
        );
    }

    #[test]
    fn media_store_uri_without_real_name_is_kept_verbatim() {
        // provider 不给真名时原样保留，不伪造扩展名。
        assert_eq!(
            filename_from_source("content://media/external/images/media/image%3A1000000123"),
            "image:1000000123"
        );
    }

    #[test]
    fn percent_encoded_space_is_decoded() {
        assert_eq!(
            filename_from_source("content://com.android.providers.downloads.documents/document/msf%3A12%2Fmy%20file.txt"),
            "my file.txt"
        );
        // 中文（UTF-8 三字节百分号编码）也要解出来。
        assert_eq!(
            filename_from_source(
                "content://x/document/primary%3ADownload%2F%E6%8A%A5%E5%91%8A.pdf"
            ),
            "报告.pdf"
        );
    }

    #[test]
    fn empty_or_degenerate_source_falls_back() {
        assert_eq!(filename_from_source(""), "attachment");
        assert_eq!(filename_from_source("   "), "attachment");
        assert_eq!(filename_from_source("content://"), "attachment");
        // 目录结尾（最后一段为空）。
        assert_eq!(filename_from_source("/home/a/"), "attachment");
        assert_eq!(filename_from_source(r"C:\Users\a\"), "attachment");
    }

    #[test]
    fn invalid_escape_is_left_untouched() {
        // `%2` 截断、`%ZZ` 非法 → 原样保留，不 panic、不吞字符。
        assert_eq!(filename_from_source("content://x/document/a%2"), "a%2");
        assert_eq!(
            filename_from_source("content://x/document/a%ZZ.txt"),
            "a%ZZ.txt"
        );
    }
}
