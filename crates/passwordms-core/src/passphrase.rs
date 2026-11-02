//! 口令短语生成（v1.5.29 / B-04）。
//!
//! ## 为什么需要它
//!
//! 随机字符串适合**网站密码**——那类密码存在库里，用户不需要记住。但**主密码**是
//! 唯一必须由用户记住的那一个：用 16 位随机串当主密码，用户记不住，于是退化成
//! `abc123456` 或写在便利贴上。一个点被攻破，全库安全归零。
//!
//! 词表法把「随机」和「可记」拆开：从 7772 个常见词里随机取 4 个，熵约 51.7 bits
//! （远超 `Tr0ub4dor&3` 那类"看起来复杂"的口令），而「correct-horse-battery-staple」
//! 这种句子是能背下来的。
//!
//! ## 关键设计决定
//!
//! 1. **熵按运行时词表条数计算**，不硬编码 7776 / 12.9。内置词表实际是 7772 条
//!    （见 `assets/README.md`），换个词表就换熵——硬编码会在词表被替换后静默失真。
//! 2. **熵按运行时词表条数计算**，不硬编码 7776 / 12.9。内置词表实际是 7772 条
//!    （见 `assets/README.md`），换个词表就换熵——硬编码会在词表被替换后静默失真。
//! 3. **有放回抽取**（允许出现 `apple-apple`）。看起来反直觉，但这是唯一能让
//!    「标称熵」与「实际熵」精确相等的做法：有放回时每个词的取值独立均匀，
//!    熵 = `k × log2(N)`；无放回时真实熵是 `log2(P(N,k))`，比标称低约 0.0036 bits，
//!    属「报高了」。而 `apple-apple-apple-apple` 的概率是 `1/7772³ ≈ 2e-12`，
//!    它的问题是「看着不像随机」（用户体验），不是「熵虚高」。
//! 4. **随机源与密码生成器同源**（[`crate::crypto::rng::below`]）：拒绝采样消除模偏置。
//!    两份各写一遍迟早漂移——一份修了偏置另一份没修，是最难发现的失配。
//! 5. 生成的短语**不进任何日志 / 错误上下文**：它是明文凭据，只经 IPC 返回给前端。

use std::sync::OnceLock;

use crate::crypto::rng;
use crate::error::{CoreError, CoreResult};

/// 内置词表原文，编译期嵌入二进制（运行时不读盘、不依赖安装目录）。
const WORDLIST_RAW: &str = include_str!("../assets/eff_large.wordlist");

/// 词数下限：3 词 ≈ 38.8 bits，低于此不建议作为主密码。
pub const MIN_WORDS: u8 = 3;
/// 词数上限：8 词 ≈ 103.4 bits，再长用户背不下来，收益也趋于饱和。
pub const MAX_WORDS: u8 = 8;
/// 分隔符最大长度（字符数）。够用 `-` `_` `.` 空格 `::` 之类，同时挡住「用 1MB 分隔符
/// 把生成结果撑爆」这条无意义路径。
pub const MAX_SEPARATOR_CHARS: usize = 8;
/// 末尾追加随机数字的最大位数。
pub const MAX_APPEND_DIGITS: u8 = 4;

/// 词表大小写风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStyle {
    /// 全小写（默认；最好背）。
    Lower,
    /// 每个词首字母大写（`Correct-Horse-Battery`）。
    Title,
    /// 全大写。
    Upper,
}

/// 口令短语生成参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseOptions {
    /// 词数，`MIN_WORDS..=MAX_WORDS`。
    pub words: u8,
    /// 词间分隔符（可为空串——此时词会连在一起，不推荐但允许）。
    pub separator: String,
    /// 大小写风格。
    pub case: CaseStyle,
    /// 末尾追加的随机数字位数（0 = 不追加）。补零到固定宽度。
    pub append_digits: u8,
}

impl Default for PassphraseOptions {
    /// 默认：4 词、`-` 分隔、全小写、不追加数字。
    ///
    /// 4 词 ≈ 51.7 bits 是「好记」与「够强」的交点；追加数字留给需要过
    /// 「必须含数字」这类网站校验的场景（口令短语主要用于主密码，两者都不需要，
    /// 所以默认关闭）。
    fn default() -> Self {
        Self {
            words: 4,
            separator: "-".to_string(),
            case: CaseStyle::Lower,
            append_digits: 0,
        }
    }
}

/// 解析后的词表（进程内只解析一次）。
fn wordlist() -> &'static [&'static str] {
    static CACHE: OnceLock<Vec<&'static str>> = OnceLock::new();
    CACHE.get_or_init(|| {
        WORDLIST_RAW
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect()
    })
}

/// 内置词表的实际条数。
pub fn word_count() -> usize {
    wordlist().len()
}

/// 每词熵（bits），`log2(词表条数)`。
pub fn bits_per_word() -> f64 {
    (word_count() as f64).log2()
}

/// 该参数下的标称熵（bits）。
///
/// **精确值，不是上界**：有放回抽取下每个词独立均匀取自 `N = word_count()` 个词，
/// 熵恰为 `k × log2(N)`；末尾数字固定宽度补零，熵恰为 `digits × log2(10)`。
/// （无放回抽样才会出现「标称高于实际」的偏差，这也是本模块选择有放回的原因之一。）
pub fn entropy_bits(words: u8, append_digits: u8) -> f64 {
    words as f64 * bits_per_word() + append_digits as f64 * 10f64.log2()
}

/// 生成一条口令短语。
///
/// 失败只会来自参数越界或 CSPRNG 不可用；两者都是调用方的程序错误，
/// 不做静默回退（回退到"默认参数"会让用户以为自己选的是别的）。
pub fn generate(opts: &PassphraseOptions) -> CoreResult<String> {
    if opts.words < MIN_WORDS || opts.words > MAX_WORDS {
        return Err(CoreError::Validation("passphrase_words_range".to_string()));
    }
    if opts.append_digits > MAX_APPEND_DIGITS {
        return Err(CoreError::Validation("passphrase_digits_range".to_string()));
    }
    if opts.separator.chars().count() > MAX_SEPARATOR_CHARS {
        return Err(CoreError::Validation(
            "passphrase_separator_too_long".to_string(),
        ));
    }
    // 控制字符会把结果变成「看起来一样、复制出来不一样」的东西（换行尤其致命：
    // 用户粘贴到主密码框时被截断，之后再也解不开库）。
    if opts.separator.chars().any(char::is_control) {
        return Err(CoreError::Validation(
            "passphrase_separator_control".to_string(),
        ));
    }

    let list = wordlist();
    if list.is_empty() {
        // 词表被清空/打包失败——这是打包事故，不是用户输入问题。
        return Err(CoreError::Internal("passphrase_wordlist_empty".to_string()));
    }

    // 有放回抽样：每个词独立取自全词表。这样标称熵 `k × log2(N)` 与实际熵精确相等
    // （无放回会略低，见模块文档第 3 条）。重复词的概率 1/7772³ ≈ 2e-12，可忽略。
    let mut picked: Vec<&'static str> = Vec::with_capacity(opts.words as usize);
    for _ in 0..opts.words {
        let idx = rng::below(list.len() as u32)? as usize;
        picked.push(list[idx]);
    }

    let words: Vec<String> = picked
        .into_iter()
        .map(|word| apply_case(word, opts.case))
        .collect();

    let mut out = words.join(&opts.separator);

    if opts.append_digits > 0 {
        let digits = opts.append_digits as usize;
        // 10^4 = 10000，远小于 u32 上限；`digits == 0` 已被上面的分支排除。
        let max = 10u32.pow(opts.append_digits as u32);
        let value = rng::below(max)?;
        // 补零到固定宽度：`7` → `07`。不补零会让「两位数字」的实际取值空间
        // 从 100 塌缩成 100 个不等宽的字符串，熵计算也就对不上了。
        out.push_str(&format!("{value:0digits$}"));
    }

    Ok(out)
}

/// 按风格转换单个词（词表本身全小写，所以 Lower 是恒等变换）。
fn apply_case(word: &str, case: CaseStyle) -> String {
    match case {
        CaseStyle::Lower => word.to_string(),
        CaseStyle::Upper => word.to_uppercase(),
        CaseStyle::Title => {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let mut out = String::with_capacity(word.len());
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                    out
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 词表条数与本文件记录的熵口径一致() {
        // 守门：词表被替换 / 截断 / 多加了行，这里立刻红。
        // 7772 是这份 EFF 词表的实际条数（官方文档说 7776，实测不一致——见 assets/README.md）。
        assert_eq!(word_count(), 7772);
        // 每词 12.9241 bits（log2(7772)）。
        assert!((bits_per_word() - 12.9241).abs() < 1e-3);
    }

    #[test]
    fn 词表工件字节级锁定() {
        // 为什么钉 sha256 而不是只钉条数：条数相同、内容被换掉（同义词表替换、大小写调整、
        // 删掉敏感词）时条数断言照样绿，而熵口径的**前提**已经变了。
        // 这个值来自 `crates/passwordms-core/assets/eff_large.wordlist`（62,122 B）。
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(WORDLIST_RAW.as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex, "df895130803573b14caa684e8cba982a9ed21f4898c98c711f204acf600efedd",
            "词表被改动过——若是有意替换，必须同时更新 assets/README.md 与熵口径断言"
        );
        assert_eq!(WORDLIST_RAW.len(), 62_122);
    }

    #[test]
    fn 词表无重复词且全部为纯小写字母() {
        // 重复词会把实际熵压到 log2(去重后条数) 以下，于是 `k × log2(N)` 从"精确值"
        // 变成"高报"——这正是选择有放回抽样要避免的那种失真。
        // 非 [a-z] 字符会让"大写转换"和"首字母大写"出现意外形态（如数字开头的词）。
        let list = wordlist();
        let unique: std::collections::BTreeSet<&str> = list.iter().copied().collect();
        assert_eq!(unique.len(), list.len(), "词表存在重复词");
        for word in list {
            assert!(!word.is_empty(), "词表存在空词");
            assert!(
                word.chars().all(|c| c.is_ascii_lowercase()),
                "词表含非小写字母字符：{word}"
            );
        }
    }

    #[test]
    fn 每个词都出自内置词表() {
        // 有放回抽样允许重复词（见模块文档第 3 条），所以不能断言"不重复"。
        // 能断言的是：产物里的每一段都必须是词表里的词，且段数等于请求词数。
        let list = wordlist();
        let known: std::collections::BTreeSet<&str> = list.iter().copied().collect();
        let opts = PassphraseOptions {
            words: 6,
            ..PassphraseOptions::default()
        };
        let out = generate(&opts).expect("生成应成功");
        let parts: Vec<&str> = out.split('-').collect();
        assert_eq!(parts.len(), 6);
        for word in parts {
            assert!(known.contains(word), "出现词表外的词：{word}");
        }
    }

    #[test]
    fn 默认参数生成四词并以连字符连接() {
        let out = generate(&PassphraseOptions::default()).expect("生成应成功");
        let parts: Vec<&str> = out.split('-').collect();
        assert_eq!(parts.len(), 4, "默认应为 4 个词：{out}");
        assert!(parts.iter().all(|w| !w.is_empty()), "不应出现空词：{out}");
        assert!(
            out.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
            "默认全小写 + 连字符：{out}"
        );
    }

    #[test]
    fn 大小写风格按选择生效() {
        let base = PassphraseOptions {
            words: 3,
            separator: "_".to_string(),
            ..PassphraseOptions::default()
        };

        let upper = generate(&PassphraseOptions {
            case: CaseStyle::Upper,
            ..base.clone()
        })
        .expect("生成应成功");
        assert!(upper.chars().all(|c| !c.is_ascii_lowercase()), "{upper}");

        let title = generate(&PassphraseOptions {
            case: CaseStyle::Title,
            ..base
        })
        .expect("生成应成功");
        for word in title.split('_') {
            let mut chars = word.chars();
            let first = chars.next().expect("词不为空");
            assert!(first.is_uppercase(), "每个词首字母应大写：{title}");
            assert!(
                chars.all(|c| !c.is_ascii_uppercase()),
                "其余字母应保持小写：{title}"
            );
        }
    }

    #[test]
    fn 追加数字补零到固定宽度() {
        // 跑 200 轮，位数必须恒等于请求值——不能出现 "7" 顶替 "07"。
        for _ in 0..200 {
            let out = generate(&PassphraseOptions {
                words: 3,
                separator: "-".to_string(),
                case: CaseStyle::Lower,
                append_digits: 2,
            })
            .expect("生成应成功");
            let tail: String = out.chars().rev().take(2).collect();
            assert!(
                tail.chars().all(|c| c.is_ascii_digit()),
                "末两位应为数字：{out}"
            );
            // 倒数第三位**不能**是数字，否则说明数字是词的一部分而不是追加段。
            // 注意这里不能断言「倒三是分隔符」——数字直接接在最后一个词后面
            // （`shamrock14`），分隔符在更前面一位。
            assert!(
                !out.chars().nth_back(2).expect("非空").is_ascii_digit(),
                "数字应是一个独立片段，而不是接在词后面成为词的一部分：{out}"
            );
        }
    }

    #[test]
    fn 参数越界一律报校验错误() {
        let too_few = PassphraseOptions {
            words: MIN_WORDS - 1,
            ..PassphraseOptions::default()
        };
        assert!(matches!(generate(&too_few), Err(CoreError::Validation(_))));

        let too_many = PassphraseOptions {
            words: MAX_WORDS + 1,
            ..PassphraseOptions::default()
        };
        assert!(matches!(generate(&too_many), Err(CoreError::Validation(_))));

        let long_sep = PassphraseOptions {
            separator: "----------".to_string(),
            ..PassphraseOptions::default()
        };
        assert!(matches!(generate(&long_sep), Err(CoreError::Validation(_))));

        let control_sep = PassphraseOptions {
            separator: "\n".to_string(),
            ..PassphraseOptions::default()
        };
        assert!(matches!(
            generate(&control_sep),
            Err(CoreError::Validation(_))
        ));

        let bad_digits = PassphraseOptions {
            append_digits: MAX_APPEND_DIGITS + 1,
            ..PassphraseOptions::default()
        };
        assert!(matches!(
            generate(&bad_digits),
            Err(CoreError::Validation(_))
        ));
    }

    #[test]
    fn 空分隔符允许但词会连着() {
        let out = generate(&PassphraseOptions {
            words: 3,
            separator: String::new(),
            case: CaseStyle::Lower,
            append_digits: 0,
        })
        .expect("空分隔符不应报错");
        assert!(!out.is_empty());
        assert!(!out.contains('-'));
    }

    #[test]
    fn 熵随词数与位数单调增加() {
        let three = entropy_bits(3, 0);
        let four = entropy_bits(4, 0);
        let four_with_digits = entropy_bits(4, 2);
        assert!(four > three);
        assert!(four_with_digits > four);
        // 3 词 ≈ 38.8 bits，4 词 ≈ 51.7 bits，4 词 + 2 位数字 ≈ 58.3 bits。
        assert!((three - 38.77).abs() < 0.05, "3 词熵：{three}");
        assert!((four - 51.70).abs() < 0.05, "4 词熵：{four}");
        assert!(
            (four_with_digits - 58.34).abs() < 0.05,
            "4 词+2 位：{four_with_digits}"
        );
    }

    #[test]
    fn 两次生成不同() {
        // 不是概率证明，只是抓「忘了接随机源、返回固定串」这类低级错误。
        let a = generate(&PassphraseOptions::default()).expect("生成应成功");
        let b = generate(&PassphraseOptions::default()).expect("生成应成功");
        assert_ne!(a, b);
    }
}
