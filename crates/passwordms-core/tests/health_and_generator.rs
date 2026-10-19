//! 体检规则与生成器字符集（架构文档 §8.4、F-07、F-16）。
//!
//! 状态：已在本机实跑通过（`cargo test -p passwordms-core`，见 docs/03b-verification.md）。

use passwordms_core::generator;
use passwordms_core::health;
use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::vault::{GeneratorOptions, HealthConfig};

fn login(id: &str, password: &str, updated_at: i64, rev: u64) -> Item {
    Item {
        item_id: id.to_string(),
        kind: ItemKind::Login(LoginData {
            title: format!("qa_test_{id}"),
            url: None,
            username: None,
            password: Some(password.to_string()),
            notes: None,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
        created_at: 0,
        updated_at,
        rev,
        modified_by: "qa_test".to_string(),
        attachments: Vec::new(),
    }
}

#[test]
fn generator_respects_charset_and_length() {
    let opts = GeneratorOptions {
        length: 16,
        uppercase: true,
        lowercase: true,
        digits: true,
        symbols: false,
        exclude_ambiguous: true,
        symbol_set: None,
    };
    let pw = generator::generate(&opts).expect("gen");
    assert_eq!(pw.chars().count(), 16);
    assert!(pw.chars().all(|c| c.is_ascii_alphanumeric()));
    // 排除易混字符。
    for ch in ['0', 'O', '1', 'l', 'I'] {
        assert!(!pw.contains(ch), "不应包含易混字符 {ch}");
    }
}

#[test]
fn generator_rejects_empty_charset_and_bad_length() {
    let none = GeneratorOptions {
        length: 16,
        uppercase: false,
        lowercase: false,
        digits: false,
        symbols: false,
        exclude_ambiguous: false,
        symbol_set: None,
    };
    assert_eq!(generator::generate(&none).unwrap_err().code(), "ERR_VALIDATION");

    let too_short = GeneratorOptions {
        length: 4,
        uppercase: true,
        lowercase: true,
        digits: true,
        symbols: false,
        exclude_ambiguous: false,
        symbol_set: None,
    };
    assert_eq!(generator::generate(&too_short).unwrap_err().code(), "ERR_VALIDATION");
}

#[test]
fn generator_guarantees_each_selected_class() {
    let opts = GeneratorOptions {
        length: 32,
        uppercase: true,
        lowercase: true,
        digits: true,
        symbols: true,
        exclude_ambiguous: false,
        symbol_set: None,
    };
    let pw = generator::generate(&opts).expect("gen");
    assert!(pw.chars().any(|c| c.is_ascii_uppercase()));
    assert!(pw.chars().any(|c| c.is_ascii_lowercase()));
    assert!(pw.chars().any(|c| c.is_ascii_digit()));
    assert!(pw.chars().any(|c| !c.is_ascii_alphanumeric()));
}

#[test]
fn health_detects_weak_reused_and_stale() {
    let cfg = HealthConfig {
        weak_max_len: 8,
        stale_days: 180,
    };
    let now = 10_000_000_000i64;
    let day = 24 * 60 * 60 * 1000i64;

    let items = vec![
        // 弱口令：长度不足。
        login("a", "short", now, 1),
        // 弱口令：单一字符类型。
        login("b", "aaaaaaaaaaaa", now, 1),
        // 重复使用：与 c 同密码。
        login("d", "qa_test_shared_pw_1", now, 1),
        login("e", "qa_test_shared_pw_1", now, 1),
        // 超期未改（200 天前）。
        login("f", "qa_test_str0ng_Pw!", now - 200 * day, 1),
    ];

    let report = health::check(&items, &cfg, now);
    assert!(report.weak.contains(&"a".to_string()));
    assert!(report.weak.contains(&"b".to_string()));
    assert!(report.reused.contains(&"d".to_string()));
    assert!(report.reused.contains(&"e".to_string()));
    assert!(report.stale.contains(&"f".to_string()));
    assert!(report.total_score <= 100);
    assert!(!report.suggestions.is_empty());
}

#[test]
fn health_ignores_non_login_items() {
    let cfg = HealthConfig::default();
    let note = Item {
        item_id: "n".to_string(),
        kind: ItemKind::SecureNote(passwordms_core::model::item::SecureNoteData {
            title: "qa_test_note".to_string(),
            body: "qa_test_body".to_string(),
        }),
        folder_id: None,
        favorite: false,
        created_at: 0,
        updated_at: 0,
        rev: 1,
        modified_by: "qa_test".to_string(),
        attachments: Vec::new(),
    };
    let report = health::check(&[note], &cfg, 1_000_000);
    assert_eq!(report.login_count, 0);
    assert!(report.weak.is_empty());
}

/// D-49 回归：符号集精编后，生成结果**不得**再出现任何 shell / 编程语法高危符号。
/// 200 次 × 32 位扫描，任一命中即失败——防止将来有人把符号集改回去而无人察觉。
#[test]
fn generated_symbols_exclude_syntax_meaningful_chars() {
    let hazardous: Vec<char> = "=@$&^;#%!?*/\\'\"`()[]{}<>|~`".chars().collect();
    let opts = GeneratorOptions {
        length: 32,
        uppercase: true,
        lowercase: true,
        digits: true,
        symbols: true,
        exclude_ambiguous: false,
        symbol_set: None,
    };
    for _ in 0..200 {
        let pw = generator::generate(&opts).expect("gen");
        for c in &hazardous {
            assert!(
                !pw.contains(*c),
                "生成结果出现高危符号 {c:?}（D-49 精编被破坏）：{pw}"
            );
        }
    }
}

/// D-50：用户自选符号集生效——自定义集内的符号参与生成，集外符号绝不出现。
#[test]
fn custom_symbol_set_restricts_output() {
    let opts = GeneratorOptions {
        length: 32,
        uppercase: true,
        lowercase: true,
        digits: true,
        symbols: true,
        exclude_ambiguous: false,
        symbol_set: Some("=@#".to_string()),
    };
    let allowed: Vec<char> = "=@#".chars().collect();
    for _ in 0..200 {
        let pw = generator::generate(&opts).expect("gen");
        assert!(pw.chars().any(|c| allowed.contains(&c)), "自选集内符号应参与生成：{pw}");
        for c in pw.chars() {
            assert!(
                c.is_ascii_alphanumeric() || allowed.contains(&c),
                "出现自选集外字符 {c:?}：{pw}"
            );
        }
    }
}

/// D-50：自选集的过滤与兜底——字母数字/空格被滤掉、去重保持稳定；空集回退默认精编集。
#[test]
fn symbol_set_filter_and_fallback() {
    use passwordms_core::model::vault::GeneratorOptions as Opts;

    // 字母/数字/空格被过滤，重复去重：'@@aa 5' → 生效集 = {'@'}。
    let o = Opts { symbol_set: Some("@@aa 5".to_string()), ..Opts::default() };
    assert_eq!(o.effective_symbol_set(), "@");

    // 空字符串 → 回退默认精编集。
    let o = Opts { symbol_set: Some(String::new()), ..Opts::default() };
    assert_eq!(o.effective_symbol_set(), "+-_.,:");

    // None → 默认精编集。
    let o = Opts { symbol_set: None, ..Opts::default() };
    assert_eq!(o.effective_symbol_set(), "+-_.,:");
}
