//! 密码生成器（架构文档 §3.3 F-07）。
//!
//! 随机源为 CSPRNG（§2.4）；保证结果至少包含每个被选中字符集的一个字符，并做均匀洗牌。

use crate::constants::{
    AMBIGUOUS_CHARS, CHARSET_DIGITS, CHARSET_LOWERCASE, CHARSET_UPPERCASE, GENERATOR_MAX_LEN,
    GENERATOR_MIN_LEN,
};
use crate::crypto::rng;
use crate::error::{CoreError, CoreResult};
use crate::model::vault::GeneratorOptions;

/// 生成密码。
pub fn generate(opts: &GeneratorOptions) -> CoreResult<String> {
    if !opts.uppercase && !opts.lowercase && !opts.digits && !opts.symbols {
        return Err(CoreError::Validation("charset_empty".to_string()));
    }
    if opts.length < GENERATOR_MIN_LEN || opts.length > GENERATOR_MAX_LEN {
        return Err(CoreError::Validation("length_range".to_string()));
    }

    let mut pools: Vec<Vec<char>> = Vec::new();
    if opts.lowercase {
        pools.push(pool(CHARSET_LOWERCASE, opts.exclude_ambiguous));
    }
    if opts.uppercase {
        pools.push(pool(CHARSET_UPPERCASE, opts.exclude_ambiguous));
    }
    if opts.digits {
        pools.push(pool(CHARSET_DIGITS, opts.exclude_ambiguous));
    }
    if opts.symbols {
        // D-50：符号池改用「生效符号集」——用户自选集（过滤/去重后非空）优先，
        // 否则回退默认精编集。见 GeneratorOptions::effective_symbol_set。
        pools.push(pool(&opts.effective_symbol_set(), opts.exclude_ambiguous));
    }
    if pools.iter().any(|p| p.is_empty()) {
        return Err(CoreError::Validation("charset_empty".to_string()));
    }

    let all: Vec<char> = pools.iter().flatten().copied().collect();
    let mut chars: Vec<char> = Vec::with_capacity(opts.length as usize);

    // 每个选中字符集至少取一个，保证强度语义。
    for p in &pools {
        let idx = rng::below(p.len() as u32)? as usize;
        chars.push(p[idx]);
    }
    while chars.len() < opts.length as usize {
        let idx = rng::below(all.len() as u32)? as usize;
        chars.push(all[idx]);
    }

    shuffle(&mut chars)?;
    Ok(chars.into_iter().collect())
}

/// 过滤易混字符后的字符池。
fn pool(source: &str, exclude_ambiguous: bool) -> Vec<char> {
    source
        .chars()
        .filter(|c| !exclude_ambiguous || !AMBIGUOUS_CHARS.contains(*c))
        .collect()
}

/// Fisher-Yates 洗牌（CSPRNG）。
fn shuffle(chars: &mut [char]) -> CoreResult<()> {
    if chars.len() <= 1 {
        return Ok(());
    }
    for i in (1..chars.len()).rev() {
        let j = rng::below((i + 1) as u32)? as usize;
        chars.swap(i, j);
    }
    Ok(())
}
