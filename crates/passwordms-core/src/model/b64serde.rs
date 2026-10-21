//! Base64 serde 辅助模块。
//!
//! 架构文档 §9.4：JSON 内一律**标准 Base64（带 `=` 填充）**。
//! 单一模块承载 `Vec<u8>` / `Option<Vec<u8>>` / `[u8; 32]` 三种形态，避免散落重复代码。

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Deserializer, Serializer};

/// `Vec<u8>` ↔ Base64 字符串。
pub mod vec {
    use super::*;

    /// 序列化为 Base64 字符串。
    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    /// 从 Base64 字符串反序列化。
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD
            .decode(s.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}

/// `Option<Vec<u8>>` ↔ Base64 / null。
pub mod opt_vec {
    use super::*;

    /// 序列化可选 Base64。
    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(b) => s.serialize_str(&STANDARD.encode(b)),
            None => s.serialize_none(),
        }
    }

    /// 反序列化可选 Base64。
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let opt = Option::<String>::deserialize(d)?;
        match opt {
            None => Ok(None),
            Some(s) => STANDARD
                .decode(s.as_bytes())
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

/// `[u8; 32]` ↔ Base64 字符串。
pub mod arr32 {
    use super::*;

    /// 序列化 32 字节数组。
    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }

    /// 反序列化 32 字节数组（长度不符即报错）。
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let raw = STANDARD
            .decode(s.as_bytes())
            .map_err(serde::de::Error::custom)?;
        let mut out = [0u8; 32];
        if raw.len() != 32 {
            return Err(serde::de::Error::custom("expected 32 bytes"));
        }
        out.copy_from_slice(&raw);
        Ok(out)
    }
}
