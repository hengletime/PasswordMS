//! 密码体检（架构文档 §3.3 F-16）。
//!
//! 三条规则（仅对 `login` 型条目计分）：
//! - **弱口令**：长度 < `weak_max_len`，或仅含单一字符类型；
//! - **重复使用**：同一密码被 ≥ 2 条登录条目使用；
//! - **超期未改**：`updated_at` 早于 `now - stale_days` 天。
//!
//! 全部计算在本地完成，明文字符串从不外传、不落盘。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::constants::MS_PER_DAY;
use crate::model::item::{Item, ItemKind};
use crate::model::vault::HealthConfig;

/// 体检报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthReport {
    /// 总分（0-100）。
    pub total_score: u32,
    /// 参与评分的登录条目数。
    pub login_count: usize,
    /// 弱口令条目 ID。
    pub weak: Vec<String>,
    /// 重复使用密码的条目 ID。
    pub reused: Vec<String>,
    /// 超期未改条目 ID。
    pub stale: Vec<String>,
    /// 整改建议（可执行文案）。
    pub suggestions: Vec<String>,
}

/// 执行体检。
pub fn check(items: &[Item], cfg: &HealthConfig, now_ms: i64) -> HealthReport {
    let logins: Vec<&Item> = items
        .iter()
        .filter(|i| matches!(i.kind, ItemKind::Login(_)))
        .collect();

    let mut weak: Vec<String> = Vec::new();
    let mut stale: Vec<String> = Vec::new();
    let mut password_users: HashMap<String, Vec<String>> = HashMap::new();

    let stale_threshold = now_ms - (cfg.stale_days as i64) * MS_PER_DAY;

    for item in &logins {
        if let ItemKind::Login(login) = &item.kind {
            let password = login.password.as_deref().unwrap_or("");
            if is_weak(password, cfg.weak_max_len) {
                weak.push(item.item_id.clone());
            }
            if !password.is_empty() {
                password_users
                    .entry(password.to_string())
                    .or_default()
                    .push(item.item_id.clone());
            }
            if item.updated_at < stale_threshold {
                stale.push(item.item_id.clone());
            }
        }
    }

    // 重复使用：同一密码被 ≥ 2 条登录条目使用。
    let mut reused: Vec<String> = Vec::new();
    for ids in password_users.values() {
        if ids.len() >= 2 {
            reused.extend(ids.iter().cloned());
        }
    }
    reused.sort();
    reused.dedup();

    let total = logins.len().max(1) as u32;
    let mut penalties: u32 = 0;
    penalties += (weak.len() as u32) * 100 / total;
    penalties += (reused.len() as u32) * 100 / total;
    penalties += (stale.len() as u32) * 60 / total;
    let total_score = 100u32.saturating_sub(penalties.min(100));

    let mut suggestions: Vec<String> = Vec::new();
    if !weak.is_empty() {
        suggestions.push(format!(
            "有 {} 条条目使用弱口令，建议改为 12 位以上的混合字符密码。",
            weak.len()
        ));
    }
    if !reused.is_empty() {
        suggestions.push(format!(
            "有 {} 条条目重复使用同一密码，建议为每站设置独立密码。",
            reused.len()
        ));
    }
    if !stale.is_empty() {
        suggestions.push(format!(
            "有 {} 条条目超过 {} 天未更换密码，建议定期轮换。",
            stale.len(),
            cfg.stale_days
        ));
    }
    if suggestions.is_empty() {
        suggestions.push("密码健康状况良好，继续保持。".to_string());
    }

    HealthReport {
        total_score,
        login_count: logins.len(),
        weak,
        reused,
        stale,
        suggestions,
    }
}

/// 弱口令判定：长度不足 或 仅含单一字符类型。
fn is_weak(password: &str, weak_max_len: u8) -> bool {
    if password.is_empty() {
        return true;
    }
    if password.chars().count() < weak_max_len as usize {
        return true;
    }
    has_lower(password) as u8
        + has_upper(password) as u8
        + has_digit(password) as u8
        + has_symbol(password) as u8
        <= 1
}

fn has_lower(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_lowercase())
}
fn has_upper(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_uppercase())
}
fn has_digit(s: &str) -> bool {
    s.chars().any(|c| c.is_ascii_digit())
}
fn has_symbol(s: &str) -> bool {
    s.chars().any(|c| !c.is_alphanumeric())
}
