//! 自动输入序列解析器（1.5.39，纯逻辑零依赖）。
//!
//! 把用户可配置的序列模板（如 `{USERNAME}{TAB}{PASSWORD}{ENTER}`）切成 Token 列表，
//! 供外壳层（`src-tauri/src/commands/autotype_cmd.rs`）逐 Token 转成按键注入。
//!
//! ## 支持的占位符（大小写不敏感）
//!
//! | 占位符 | 含义 |
//! |---|---|
//! | `{USERNAME}` | 登录用户名（**字段值由调用方解析**，见下） |
//! | `{PASSWORD}` | 登录密码（同上） |
//! | `{URL}` | 登录条目网址（同上） |
//! | `{TOTP}` | 当前 6 位动态码（**由调用方现算**，同上） |
//! | `{TAB}` | Tab 键 |
//! | `{ENTER}` | 回车键 |
//! | `{DELAY n}` | 停顿 n 毫秒（0 ≤ n ≤ 10000） |
//!
//! 其余文本原样保留为 [`Token::Text`]；未知占位符、空占位符 `{}`、未闭合的 `{`
//! 一律报错——宁可拒绝整条序列，也不把用户没料到的字面量打进别的应用。
//!
//! ## 为什么 `{USERNAME}` / `{PASSWORD}` / `{URL}` / `{TOTP}` 解析成 `Text` 而不是独立变体
//!
//! Token 枚举是**跨端冻结契约**（`Text(String) / Tab / Enter / DelayMs(u32)`）。
//! 四个字段占位符被解析成**保留其占位符标记**的 `Text`（payload 恒为
//! `"{USERNAME}"` / `"{PASSWORD}"` / `"{URL}"` / `"{TOTP}"`，大小写已归一），由调用方
//! 按字面值比对后换成真实内容。这样设计的好处是**明文与动态码完全不经过解析器**：
//! 密码从库里取出后直接进调用方的注入缓冲，不进任何 `String` 中间层之外的副本；
//! 含 `{` / `}` 的密码也不会被解析器误当成占位符（它根本不参与解析）；TOTP 也是
//! 调用方现算现用，解析器见不到。
//!
//! `Token::Text` 因此有两种来源：字面文本与字段占位符。调用方区分二者靠
//! payload 是否恰好等于归一后的占位符标记——模板由外壳层机器拼装（不含用户自由
//! 文本），不存在歧义。

/// 解析产物的一个动作单元。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// 字面文本，或保留占位符标记的字段占位（`"{USERNAME}"` / `"{PASSWORD}"` /
    /// `"{URL}"` / `"{TOTP}"`，见模块文档）。payload 是**字符**序列（注入时按
    /// UTF-16 码元逐个发）。
    Text(String),
    /// Tab 键。
    Tab,
    /// 回车键。
    Enter,
    /// 停顿 n 毫秒（上限 10000）。
    DelayMs(u32),
}

/// 解析失败的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeqError {
    /// `{...}` 里是不认识的占位符（含空占位符 `{}`）。携带原始内文（非敏感）。
    UnknownPlaceholder(String),
    /// `{` 没有配对的 `}`。
    UnclosedBrace,
    /// `{DELAY n}` 的 n 不是非负整数，或超出 0..=10000。
    InvalidDelay(String),
}

impl std::fmt::Display for SeqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPlaceholder(s) => write!(f, "unknown placeholder {{{s}}}"),
            Self::UnclosedBrace => write!(f, "unclosed '{{'"),
            Self::InvalidDelay(s) => write!(f, "invalid delay value {s:?} (0..=10000)"),
        }
    }
}

impl std::error::Error for SeqError {}

/// `{DELAY n}` 允许的最大毫秒数。
pub const MAX_DELAY_MS: u32 = 10_000;

/// 自动输入动作用到的序列模板（1.5.39 增强：选择窗可指定"打什么内容"）。
pub const TEMPLATE_DEFAULT: &str = "{USERNAME}{TAB}{PASSWORD}{ENTER}";
/// 只打用户名（单字段一律不带回车——用户按完动作后自己决定下一步）。
pub const TEMPLATE_USERNAME: &str = "{USERNAME}";
/// 只打密码。
pub const TEMPLATE_PASSWORD: &str = "{PASSWORD}";
/// 只打条目网址。
pub const TEMPLATE_URL: &str = "{URL}";
/// 只打当前 TOTP 验证码。
pub const TEMPLATE_TOTP: &str = "{TOTP}";

/// action 取值不认识。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionError(pub String);

impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown autotype action {:?}", self.0)
    }
}

impl std::error::Error for ActionError {}

/// action → 序列模板（纯函数，可单测；大小写与首尾空白容忍）。
///
/// 合法 action（与前端选择窗的键逐字一致）：
/// `default` / `username` / `password` / `url` / `totp`。
/// 其余一律 [`ActionError`]——宁可拒绝，也不猜用户想打什么。
pub fn template_for(action: &str) -> Result<&'static str, ActionError> {
    match action.trim().to_ascii_lowercase().as_str() {
        "default" => Ok(TEMPLATE_DEFAULT),
        "username" => Ok(TEMPLATE_USERNAME),
        "password" => Ok(TEMPLATE_PASSWORD),
        "url" => Ok(TEMPLATE_URL),
        "totp" => Ok(TEMPLATE_TOTP),
        other => Err(ActionError(other.to_string())),
    }
}

/// 解析自动输入序列。
///
/// 空串 / 全空白返回空表（= 注入一个空序列，调用方自行决定是否视为配置错误）。
pub fn parse(seq: &str) -> Result<Vec<Token>, SeqError> {
    let mut tokens: Vec<Token> = Vec::new();
    // 字面文本累积缓冲：连续的普通字符合并成一个 Text（少发一倍的 SendInput 轮次）。
    let mut literal = String::new();

    let mut chars = seq.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch != '{' {
            literal.push(ch);
            continue;
        }
        // 找配对的 '}'（占位符内文里不允许嵌套 '{'：再遇 '{' 按未闭合报错最直观）。
        let body_start = idx + ch.len_utf8();
        let candidate = seq[body_start..]
            .find('}')
            .map(|pos| (&seq[body_start..body_start + pos], body_start + pos + 1))
            // 第一个 '}' 之前若又出现 '{'，说明这个 '{' 永远等不到自己的 '}'。
            .filter(|(body, _)| !body.contains('{'));
        let Some((body, consumed_until)) = candidate else {
            return Err(SeqError::UnclosedBrace);
        };
        let cursor = consumed_until;

        // 冲刷累积的字面文本。
        if !literal.is_empty() {
            tokens.push(Token::Text(std::mem::take(&mut literal)));
        }

        let name = body.trim();
        match name.to_ascii_lowercase().as_str() {
            // 字段占位符：保留归一后的标记，由调用方解析成真实内容。
            "username" => tokens.push(Token::Text("{USERNAME}".to_string())),
            "password" => tokens.push(Token::Text("{PASSWORD}".to_string())),
            "url" => tokens.push(Token::Text("{URL}".to_string())),
            "totp" => tokens.push(Token::Text("{TOTP}".to_string())),
            "tab" => tokens.push(Token::Tab),
            "enter" => tokens.push(Token::Enter),
            "delay" => {
                // `{DELAY}` 不带参数同样视为非法——停顿多少毫秒必须写明白。
                return Err(SeqError::InvalidDelay(String::new()));
            }
            _ if name.starts_with_ascii_ignore_case("delay ") => {
                let raw = name["delay ".len()..].trim();
                let ms: u32 = raw
                    .parse()
                    .map_err(|_| SeqError::InvalidDelay(raw.into()))?;
                if ms > MAX_DELAY_MS {
                    return Err(SeqError::InvalidDelay(raw.into()));
                }
                tokens.push(Token::DelayMs(ms));
            }
            _ => return Err(SeqError::UnknownPlaceholder(name.to_string())),
        }
        // 跳过已消费的占位符（含 '}'）；下一个字符从 `cursor` 处继续。
        while chars.next_if(|(i, _)| *i < cursor).is_some() {}
    }

    if !literal.is_empty() {
        tokens.push(Token::Text(literal));
    }
    Ok(tokens)
}

/// ASCII 前缀匹配（大小写不敏感），供 `{DELAY n}` 判定用。
trait StartsWithIgnoreCase {
    fn starts_with_ascii_ignore_case(&self, prefix: &str) -> bool;
}

impl StartsWithIgnoreCase for str {
    fn starts_with_ascii_ignore_case(&self, prefix: &str) -> bool {
        self.len() >= prefix.len() && self[..prefix.len()].eq_ignore_ascii_case(prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(seq: &str) -> Vec<Token> {
        parse(seq).expect("应解析成功")
    }

    #[test]
    fn classic_placeholders() {
        let toks = parse_ok("{USERNAME}{PASSWORD}{TAB}{ENTER}{DELAY 5000}");
        assert_eq!(
            toks,
            vec![
                Token::Text("{USERNAME}".into()),
                Token::Text("{PASSWORD}".into()),
                Token::Tab,
                Token::Enter,
                Token::DelayMs(5000),
            ]
        );
    }

    #[test]
    fn literal_text_mixed_in() {
        let toks = parse_ok("user{TAB}hello world{ENTER}");
        assert_eq!(
            toks,
            vec![
                Token::Text("user".into()),
                Token::Tab,
                Token::Text("hello world".into()),
                Token::Enter,
            ]
        );
        // 相邻字面字符合并成一个 Text。
        assert_eq!(parse_ok("abc"), vec![Token::Text("abc".into())]);
    }

    #[test]
    fn delay_boundary_values() {
        assert_eq!(parse_ok("{DELAY 0}"), vec![Token::DelayMs(0)]);
        assert_eq!(parse_ok("{DELAY 10000}"), vec![Token::DelayMs(10_000)]);
        // 10001 超上限。
        assert_eq!(
            parse("{DELAY 10001}"),
            Err(SeqError::InvalidDelay("10001".into()))
        );
        assert_eq!(
            parse("{DELAY 20000}"),
            Err(SeqError::InvalidDelay("20000".into()))
        );
        // 非数字。
        assert_eq!(
            parse("{DELAY abc}"),
            Err(SeqError::InvalidDelay("abc".into()))
        );
        // 负数不是合法毫秒值。
        assert_eq!(
            parse("{DELAY -5}"),
            Err(SeqError::InvalidDelay("-5".into()))
        );
        // 不带参数同样非法。
        assert_eq!(parse("{DELAY}"), Err(SeqError::InvalidDelay(String::new())));
    }

    #[test]
    fn unknown_placeholder_rejected() {
        assert_eq!(
            parse("{FOO}"),
            Err(SeqError::UnknownPlaceholder("FOO".into()))
        );
        // 空占位符也是未知。
        assert_eq!(
            parse("{}"),
            Err(SeqError::UnknownPlaceholder(String::new()))
        );
    }

    #[test]
    fn unclosed_brace_rejected() {
        assert_eq!(parse("{USERNAME"), Err(SeqError::UnclosedBrace));
        assert_eq!(parse("abc{"), Err(SeqError::UnclosedBrace));
        // 内文里再开 '{' 一样按未闭合处理。
        assert_eq!(parse("{DE{LAY 5}"), Err(SeqError::UnclosedBrace));
    }

    #[test]
    fn empty_sequence_is_empty_vec() {
        assert_eq!(parse_ok(""), Vec::new());
        // 空白是**字面文本**（会被原样打出去），不是空序列——只有空串才是空表。
        assert_eq!(parse_ok("   "), vec![Token::Text("   ".into())]);
    }

    #[test]
    fn placeholders_are_case_insensitive() {
        assert_eq!(
            parse_ok("{tab}{Enter}{USERNAME}{pAssWoRd}{delay 100}"),
            parse_ok("{TAB}{ENTER}{USERNAME}{PASSWORD}{DELAY 100}")
        );
        // 内文空白容忍。
        assert_eq!(
            parse_ok("{ TAB }{ delay  50 }"),
            vec![Token::Tab, Token::DelayMs(50)]
        );
    }

    #[test]
    fn field_placeholders_keep_marker_so_secrets_bypass_parser() {
        // 关键不变量：字段占位符解析成保留标记的 Text——真实明文由调用方注入，
        // 解析器永远接触不到密码本体（含 '{' 的密码也不会被误解析）。
        let toks = parse_ok("{USERNAME}{TAB}{PASSWORD}{ENTER}");
        assert_eq!(toks[0], Token::Text("{USERNAME}".into()));
        assert_eq!(toks[2], Token::Text("{PASSWORD}".into()));
    }

    #[test]
    fn url_and_totp_placeholders() {
        assert_eq!(parse_ok("{URL}"), vec![Token::Text("{URL}".into())]);
        assert_eq!(parse_ok("{TOTP}"), vec![Token::Text("{TOTP}".into())]);
        // 大小写同样不敏感。
        assert_eq!(
            parse_ok("{url}{Totp}"),
            vec![Token::Text("{URL}".into()), Token::Text("{TOTP}".into())]
        );
    }

    #[test]
    fn url_and_totp_mixed_with_other_tokens() {
        let toks = parse_ok("open {URL}{ENTER}{TOTP}{TAB}");
        assert_eq!(
            toks,
            vec![
                Token::Text("open ".into()),
                Token::Text("{URL}".into()),
                Token::Enter,
                Token::Text("{TOTP}".into()),
                Token::Tab,
            ]
        );
    }

    #[test]
    fn template_for_actions() {
        assert_eq!(template_for("default"), Ok(TEMPLATE_DEFAULT));
        assert_eq!(template_for("username"), Ok(TEMPLATE_USERNAME));
        assert_eq!(template_for("password"), Ok(TEMPLATE_PASSWORD));
        assert_eq!(template_for("url"), Ok(TEMPLATE_URL));
        assert_eq!(template_for("totp"), Ok(TEMPLATE_TOTP));
        // 空白与大小写容忍（wire 值由前端常量给出，这里只是防御）。
        assert_eq!(template_for(" URL "), Ok(TEMPLATE_URL));
        // 非法值一律拒绝，不猜。
        assert_eq!(template_for("bogus"), Err(ActionError("bogus".into())));
        assert_eq!(template_for(""), Err(ActionError(String::new())));
    }

    #[test]
    fn every_template_parses_and_only_has_markers() {
        // 模板与解析器必须自洽：模板串过 parse 不报错，且字段占位符保持标记形态。
        for template in [
            TEMPLATE_DEFAULT,
            TEMPLATE_USERNAME,
            TEMPLATE_PASSWORD,
            TEMPLATE_URL,
            TEMPLATE_TOTP,
        ] {
            let toks = parse(template).expect("模板必须可解析");
            assert!(!toks.is_empty());
            for tok in toks {
                if let Token::Text(text) = tok {
                    assert!(
                        matches!(
                            text.as_str(),
                            "{USERNAME}" | "{PASSWORD}" | "{URL}" | "{TOTP}"
                        ),
                        "模板里出现了未知文本 {text:?}"
                    );
                }
            }
        }
        // 单字段模板不带回车（产品约定：按完动作由用户自己决定下一步）。
        for template in [
            TEMPLATE_USERNAME,
            TEMPLATE_PASSWORD,
            TEMPLATE_URL,
            TEMPLATE_TOTP,
        ] {
            assert_eq!(
                parse(template).unwrap(),
                vec![Token::Text(template.to_string())]
            );
        }
        // default 模板尾部是回车。
        assert_eq!(parse(TEMPLATE_DEFAULT).unwrap().last(), Some(&Token::Enter));
    }

    #[test]
    fn error_display_is_sanitized() {
        // 错误信息只含占位符名（模板不是秘密），但 delay 非法值照实携带。
        assert_eq!(
            parse("{FOO}").unwrap_err().to_string(),
            "unknown placeholder {FOO}"
        );
        assert_eq!(parse("{TAB").unwrap_err().to_string(), "unclosed '{'");
    }
}
