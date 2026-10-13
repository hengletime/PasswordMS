//! 密码生成器（架构文档 §3.3 F-07）。
//!
//! 随机源为 CSPRNG（§2.4）；保证结果至少包含每个被选中字符集的一个字符，并做均匀洗牌。

use crate::constants::{
    AMBIGUOUS_CHARS, CHARSET_DIGITS, CHARSET_LOWERCASE, CHARSET_SYMBOLS, CHARSET_UPPERCASE,
    GENERATOR_MAX_LEN, GENERATOR_MIN_LEN,
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
        pools.push(pool(CHARSET_SYMBOLS, opts.exclude_ambiguous));
    }
    if pools.iter().any(|p| p.is_empty()) {
        return Err(CoreError::Validation("charset_empty".to_string()));
    }

    let all: Vec<char> = pools.iter().flatten().copied().collect();
    let mut chars: Vec<char> = Vec::with_capacity(opts.length as usize);

    // 每个选中字符集至少取一个，保证强度语义。
    for p in &pools {
        let idx = rand_below(p.len() as u32)? as usize;
        chars.push(p[idx]);
    }
    while chars.len() < opts.length as usize {
        let idx = rand_below(all.len() as u32)? as usize;
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

/// 无模偏置地生成 `[0, n)` 区间内的随机数。
fn rand_below(n: u32) -> CoreResult<u32> {
    if n == 0 {
        return Err(CoreError::Internal("rand_below(0)".to_string()));
    }
    // 拒绝采样：丢弃落在 [2^32 - (2^32 % n), 2^32) 的值以消除模偏置。
    let limit = u32::MAX - (u32::MAX % n);
    loop {
        let bytes = rng::random_bytes(4)?;
        let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if v < limit {
            return Ok(v % n);
        }
    }
}

/// Fisher-Yates 洗牌（CSPRNG）。
fn shuffle(chars: &mut [char]) -> CoreResult<()> {
    if chars.len() <= 1 {
        return Ok(());
    }
    for i in (1..chars.len()).rev() {
        let j = rand_below((i + 1) as u32)? as usize;
        chars.swap(i, j);
    }
    Ok(())
}
