//! URL 协议白名单校验（架构文档 §3.3 F-05、§8.1 `url` crate）。
//!
//! 仅放行 `http` / `https`；缺协议头自动补 `https://`；其余协议（`javascript:`、`file:`
//! 等）一律 [`CoreError::UnsafeUrl`]，防钓鱼与本地文件风险。

use url::Url;

use crate::error::{CoreError, CoreResult};

/// 归一化并校验 URL，返回可安全打开的绝对地址。
pub fn normalize_and_validate(input: &str) -> CoreResult<String> {
    let candidate = input.trim();
    if candidate.is_empty() {
        return Err(CoreError::UnsafeUrl);
    }
    // 缺少协议头时补 https://（仅当看起来像主机名时）。
    let with_scheme = if candidate.contains("://") || candidate.contains(':') {
        candidate.to_string()
    } else {
        format!("https://{}", candidate)
    };

    let parsed = Url::parse(&with_scheme).map_err(|_| CoreError::UnsafeUrl)?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        _ => Err(CoreError::UnsafeUrl),
    }
}
