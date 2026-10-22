//! 记录帧编解码（架构文档 §2.6）。
//!
//! # 与架构文档的偏差（D-1）
//!
//! 架构文档 §2.6 的帧表**未包含被包裹的条目密钥**，但 §2.1/§3.6 明确「IK 由 VK 包裹、
//! 随条目记录存储」，`ChangeRecord` 亦同时携带 `wrapped_key` 与 `ciphertext`。为使设计自洽
//! （否则无法从帧内恢复 IK），本实现将 `wrapped_key`（nonce 24B + ct 48B）纳入帧头。
//!
//! # 帧字节布局（v1）
//!
//! | 偏移 | 长度 | 字段 |
//! |------|------|------|
//! | 0    | 1    | record_type |
//! | 1    | 1    | flags（bit0 tombstone / bit1 blob） |
//! | 2    | 2    | reserved（0） |
//! | 4    | 16   | record_id（UUID 原始 16B） |
//! | 20   | 24   | wrapped_key.nonce |
//! | 44   | 48   | wrapped_key.ct（32B IK + 16B tag） |
//! | 92   | 4    | payload_len = 24 + ct_len |
//! | 96   | 4    | plaintext_len |
//! | 100  | 24   | nonce |
//! | 124  | ct_len | ct（含 16B tag） |
//!
//! 固定头部长度 = 124B（[`crate::constants::FRAME_HEADER_LEN`]）。
//!
//! **单帧总长 = `FRAME_HEADER_LEN(124) + ct_len` = `100 + payload_len`**。
//! `payload_len` 已含 24 字节 AEAD nonce，且 nonce 存在头部 `[100..124)`
//! → 不能再算 `124 + payload_len`（那是把 nonce 算了两次，写独立解析器时极易踩到：
//! 用 `124 + payload_len` 逐帧跳，第一帧就多跳 24 B，第二帧起字段全错位）。

use crate::constants::{
    AEAD_NONCE_LEN, FLAG_BLOB, FLAG_TOMBSTONE, FRAME_HEADER_LEN, WRAPPED_KEY_CT_LEN,
};
use crate::error::{CoreError, CoreResult};
use crate::model::vault::WrappedKey;

/// 一条记录帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub record_type: u8,
    pub flags: u8,
    pub record_id: [u8; 16],
    pub wrapped_key: WrappedKey,
    pub nonce: [u8; AEAD_NONCE_LEN],
    pub plaintext_len: u32,
    pub ct: Vec<u8>,
}

impl Frame {
    /// 密文长度（含 16B tag）。
    pub fn ct_len(&self) -> usize {
        self.ct.len()
    }

    /// 编码后的总字节数。
    pub fn encoded_len(&self) -> usize {
        FRAME_HEADER_LEN + self.ct.len()
    }

    /// 是否为墓碑帧。
    pub fn is_tombstone(&self) -> bool {
        self.flags & FLAG_TOMBSTONE != 0
    }

    /// 是否为附件密文 blob 帧。
    pub fn is_blob(&self) -> bool {
        self.flags & FLAG_BLOB != 0
    }

    /// 记录 ID 的 UUID 字符串形式。
    pub fn record_id_string(&self) -> String {
        bytes_to_record_id(&self.record_id)
    }

    /// 编码为字节流。
    pub fn encode(&self) -> CoreResult<Vec<u8>> {
        if self.wrapped_key.nonce.len() != AEAD_NONCE_LEN
            || self.wrapped_key.ct.len() != WRAPPED_KEY_CT_LEN
        {
            return Err(CoreError::Internal("frame wrapped_key shape".to_string()));
        }
        let ct_len = self.ct.len();
        if ct_len != self.plaintext_len as usize + crate::constants::AEAD_TAG_LEN {
            return Err(CoreError::Internal("frame ct_len mismatch".to_string()));
        }
        let payload_len = AEAD_NONCE_LEN + ct_len;
        if payload_len > u32::MAX as usize {
            return Err(CoreError::Internal("frame too large".to_string()));
        }

        let mut buf = Vec::with_capacity(FRAME_HEADER_LEN + ct_len);
        buf.push(self.record_type);
        buf.push(self.flags);
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&self.record_id);
        buf.extend_from_slice(&self.wrapped_key.nonce);
        buf.extend_from_slice(&self.wrapped_key.ct);
        buf.extend_from_slice(&(payload_len as u32).to_le_bytes());
        buf.extend_from_slice(&self.plaintext_len.to_le_bytes());
        buf.extend_from_slice(&self.nonce);
        buf.extend_from_slice(&self.ct);
        debug_assert_eq!(buf.len(), FRAME_HEADER_LEN + ct_len);
        Ok(buf)
    }

    /// 从字节流起始位置解析一帧，返回 `(frame, 消耗字节数)`。
    pub fn decode(buf: &[u8]) -> CoreResult<(Frame, usize)> {
        if buf.len() < FRAME_HEADER_LEN {
            return Err(CoreError::Internal("short frame header".to_string()));
        }
        let record_type = buf[0];
        let flags = buf[1];
        let mut record_id = [0u8; 16];
        record_id.copy_from_slice(&buf[4..20]);

        let mut wk_nonce = [0u8; AEAD_NONCE_LEN];
        wk_nonce.copy_from_slice(&buf[20..20 + AEAD_NONCE_LEN]);
        let wk_ct = buf[44..44 + WRAPPED_KEY_CT_LEN].to_vec();

        let payload_len = u32::from_le_bytes([buf[92], buf[93], buf[94], buf[95]]) as usize;
        let plaintext_len = u32::from_le_bytes([buf[96], buf[97], buf[98], buf[99]]);
        if payload_len < AEAD_NONCE_LEN {
            return Err(CoreError::Internal("bad payload_len".to_string()));
        }
        let ct_len = payload_len - AEAD_NONCE_LEN;
        let total = FRAME_HEADER_LEN + ct_len;
        if buf.len() < total {
            return Err(CoreError::Internal("truncated frame".to_string()));
        }
        if ct_len != plaintext_len as usize + crate::constants::AEAD_TAG_LEN {
            return Err(CoreError::Internal(
                "ct_len/plaintext_len mismatch".to_string(),
            ));
        }

        let mut nonce = [0u8; AEAD_NONCE_LEN];
        nonce.copy_from_slice(&buf[100..100 + AEAD_NONCE_LEN]);
        let ct = buf[FRAME_HEADER_LEN..total].to_vec();

        Ok((
            Frame {
                record_type,
                flags,
                record_id,
                wrapped_key: WrappedKey {
                    nonce: wk_nonce.to_vec(),
                    ct: wk_ct,
                },
                nonce,
                plaintext_len,
                ct,
            },
            total,
        ))
    }
}

/// UUID 字符串 → 原始 16 字节。
pub fn record_id_to_bytes(id: &str) -> CoreResult<[u8; 16]> {
    let parsed =
        uuid::Uuid::parse_str(id).map_err(|_| CoreError::Validation("uuid".to_string()))?;
    Ok(*parsed.as_bytes())
}

/// 原始 16 字节 → UUID 字符串（小写带连字符）。
pub fn bytes_to_record_id(bytes: &[u8; 16]) -> String {
    uuid::Uuid::from_bytes(*bytes).to_string()
}
