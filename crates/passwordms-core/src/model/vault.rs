//! 保险库头部与偏好模型（架构文档 §5.1）。

use serde::{Deserialize, Serialize};

use crate::constants::{
    AUTO_LOCK_MINUTES, CHARSET_SYMBOLS, CLIPBOARD_TTL_SECS, ENCRYPTION_SCHEME, FORMAT_VERSION,
    GENERATOR_DEFAULT_LEN, HEALTH_STALE_DAYS, HEALTH_WEAK_MAX_LEN, KDF_ALGORITHM, KDF_ITERATIONS,
    KDF_MEMORY_KIB, KDF_PARALLELISM, KDF_SALT_LEN,
};

/// KDF 参数（随库存储，解锁时按文件内参数执行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt_len: u16,
}

impl Default for KdfParams {
    fn default() -> Self {
        // 默认即架构文档 §2.2 冻结参数：Argon2id m=64MiB t=3 p=1 salt=16。
        Self {
            algorithm: KDF_ALGORITHM.to_string(),
            memory_kib: KDF_MEMORY_KIB,
            iterations: KDF_ITERATIONS,
            parallelism: KDF_PARALLELISM,
            salt_len: KDF_SALT_LEN as u16,
        }
    }
}

/// 被包裹的密钥（XChaCha20-Poly1305 key wrap 输出）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKey {
    #[serde(with = "crate::model::b64serde::vec")]
    pub nonce: Vec<u8>,
    #[serde(with = "crate::model::b64serde::vec")]
    pub ct: Vec<u8>,
}

/// 密码生成器偏好（与 IPC `GeneratorOptions` 同构）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratorOptions {
    pub length: u8,
    pub uppercase: bool,
    pub lowercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
    /// 用户自选的符号集（D-50）：`None` / 空 = 用默认精编集 [`crate::constants::CHARSET_SYMBOLS`]。
    /// 仅接受可打印 ASCII 非字母数字字符，其余字符在生效时被过滤（见 [`Self::effective_symbol_set`]）。
    /// `serde(default)` 兼容旧库文件 / 旧前端（无此字段 = 默认集）。
    #[serde(default)]
    pub symbol_set: Option<String>,
}

impl GeneratorOptions {
    /// 生效的符号集（D-50）：自选集去重、过滤为「可打印 ASCII 且非字母数字」，
    /// 过滤后非空则采用；否则回退默认精编集 [`crate::constants::CHARSET_SYMBOLS`]。
    pub fn effective_symbol_set(&self) -> String {
        let filtered: String = match &self.symbol_set {
            Some(s) if !s.is_empty() => {
                let mut seen: Vec<char> = Vec::new();
                for c in s.chars() {
                    if c.is_ascii_graphic() && !c.is_ascii_alphanumeric() && !seen.contains(&c) {
                        seen.push(c);
                    }
                }
                seen.into_iter().collect()
            }
            _ => String::new(),
        };
        if filtered.is_empty() {
            crate::constants::CHARSET_SYMBOLS.to_string()
        } else {
            filtered
        }
    }
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        Self {
            length: GENERATOR_DEFAULT_LEN,
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
            // D-52：默认自描述为精编集（None 在核心层等价回退默认集，但序列化出去的设置
            // 应能自描述，避免前端/旧版本把「未设置」与「显式全选精编集」混淆）。
            symbol_set: Some(CHARSET_SYMBOLS.to_string()),
        }
    }
}

/// 体检配置（与 IPC `HealthConfig` 同构）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthConfig {
    pub weak_max_len: u8,
    pub stale_days: u32,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            weak_max_len: HEALTH_WEAK_MAX_LEN,
            stale_days: HEALTH_STALE_DAYS,
        }
    }
}

/// 保险库偏好设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultSettings {
    #[serde(default)]
    pub generator: GeneratorOptions,
    #[serde(default)]
    pub health: HealthConfig,
    #[serde(default = "default_clipboard_ttl")]
    pub clipboard_clear_secs: u16,
    #[serde(default)]
    pub auto_sync: bool,
    #[serde(default)]
    pub sync_on_start: bool,
}

fn default_clipboard_ttl() -> u16 {
    CLIPBOARD_TTL_SECS
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            generator: GeneratorOptions::default(),
            health: HealthConfig::default(),
            clipboard_clear_secs: CLIPBOARD_TTL_SECS,
            auto_sync: false,
            sync_on_start: false,
        }
    }
}

/// 保险库头部（明文元数据，**禁止含任何凭据**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    pub vault_id: String,
    pub format_version: u16,
    pub kdf: KdfParams,
    #[serde(with = "crate::model::b64serde::vec")]
    pub kdf_salt: Vec<u8>,
    pub wrapped_vault_key: WrappedKey,
    #[serde(with = "crate::model::b64serde::vec")]
    pub verify_mac: Vec<u8>,
    pub encryption_scheme: String,
    pub item_count: u32,
    pub folder_count: u32,
    pub created_at: i64,
    pub updated_at: i64,
    pub auto_lock_minutes: u16,
    #[serde(default)]
    pub settings: VaultSettings,
}

impl Vault {
    /// 构造一个新的空库头部（加密字段由调用方填充）。
    pub fn new(
        vault_id: String,
        kdf_salt: Vec<u8>,
        wrapped_vault_key: WrappedKey,
        verify_mac: Vec<u8>,
        now: i64,
    ) -> Self {
        Self {
            vault_id,
            format_version: FORMAT_VERSION,
            kdf: KdfParams::default(),
            kdf_salt,
            wrapped_vault_key,
            verify_mac,
            encryption_scheme: ENCRYPTION_SCHEME.to_string(),
            item_count: 0,
            folder_count: 0,
            created_at: now,
            updated_at: now,
            auto_lock_minutes: AUTO_LOCK_MINUTES,
            settings: VaultSettings::default(),
        }
    }
}
