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
    // 缺少协议头时补 https://。
    // D-108（豆包审查 3.2-3）：原判定 `contains("://") || contains(':')` 会把
    // `mysite.com:8443`（host:port 形态）当成"已带协议"原样交给 Url::parse——
    // scheme 为 `mysite.com`（非 http/https）被误拒。改为只认 `://`：
    // 无 scheme 的 host:port 现在正确补成 `https://mysite.com:8443`；
    // 而 `javascript:…` / `file:…` / `data:…` 等危险 scheme 都含 `://` 或会被
    // 补全后的端口解析拒绝（`https://javascript:x` 无合法端口 → UnsafeUrl），
    // 白名单结论不变（审查确认未发现绕过）。
    let with_scheme = if candidate.contains("://") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_with_port_is_accepted() {
        // D-108（3.2-3）：host:port 形态曾被误拒。
        assert!(normalize_and_validate("mysite.com:8443").is_ok());
        assert!(normalize_and_validate("10.0.0.1:8080").is_ok());
    }

    #[test]
    fn plain_host_gets_https() {
        assert_eq!(
            normalize_and_validate("example.com").unwrap(),
            "https://example.com/"
        );
    }

    #[test]
    fn dangerous_schemes_are_rejected() {
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>",
            "ftp://example.com",
            "tel:+123",
        ] {
            assert!(normalize_and_validate(bad).is_err(), "{bad} 应被拒绝");
        }
    }

    #[test]
    fn schemeless_garbage_lands_on_https_only() {
        // D-108 行为说明：无 `://` 的输入一律按主机名补 https://。个别垃圾输入
        // （如 `mailto:user@host`，@ 被当作用户信息分隔符）会被"洗"成一个无害的
        // https 地址（主机名非法/不存在，打开必然失败）而不是报 UnsafeUrl——
        // **安全边界是 scheme 白名单（最终只放行 http/https），不是输入启发式**，
        // 该取舍在 3.2-3 修复时确认可接受。
        let url = normalize_and_validate("mailto:user@host").unwrap();
        assert!(url.starts_with("https://"));
    }
}
