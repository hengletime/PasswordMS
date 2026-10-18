//! 同步报文 schema（架构文档 §3.3、§3.6）。
//!
//! 二进制字段：JSON 内标准 Base64；二维码/令牌使用 base64url（无填充）。

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::constants::{QR_PREFIX, QR_PROTOCOL_VERSION};
use crate::error::{CoreError, CoreResult};

/// base64（标准）serde 辅助。
pub mod b64 {
    use super::*;

    /// 序列化 `Vec<u8>` 为标准 Base64。
    pub fn serialize<S: serde::Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(bytes))
    }
    /// 反序列化标准 Base64。
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD.decode(s.as_bytes()).map_err(serde::de::Error::custom)
    }

    /// 可选变体。
    pub mod opt {
        use super::*;
        /// 序列化可选 Base64。
        pub fn serialize<S: serde::Serializer>(bytes: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
            match bytes {
                Some(b) => s.serialize_str(&STANDARD.encode(b)),
                None => s.serialize_none(),
            }
        }
        /// 反序列化可选 Base64。
        pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
            let opt = Option::<String>::deserialize(d)?;
            match opt {
                None => Ok(None),
                Some(s) => STANDARD.decode(s.as_bytes()).map(Some).map_err(serde::de::Error::custom),
            }
        }
    }
}

/// base64url（无填充）serde 辅助。
pub mod b64url {
    use super::*;

    /// 序列化为 base64url（无填充）。
    pub fn serialize<S: serde::Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&URL_SAFE_NO_PAD.encode(bytes))
    }
    /// 反序列化 base64url（无填充）。
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        URL_SAFE_NO_PAD.decode(s.as_bytes()).map_err(serde::de::Error::custom)
    }
}

/// 二维码载荷（架构文档 §3.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QRPayload {
    pub v: u32,
    pub ip: String,
    pub port: u16,
    #[serde(with = "b64url")]
    pub token: Vec<u8>,
    pub host_id: String,
    pub host_name: String,
    pub fp: String,
    pub exp: i64,
    /// Host 的持久重同步监听端口（0 = 未提供）。供 Joiner 记录后用于 `sync_now`（D-11）。
    #[serde(default)]
    pub sync_port: u16,
    /// Host 的全部候选局域网 IPv4（D-44）：多网卡机器上逐个尝试，第一个 `ip` 为主选。
    /// `serde(default)` 兼容旧版二维码文本（无此字段时回退到 `ip`）。
    #[serde(default)]
    pub ips: Vec<String>,
}

impl QRPayload {
    /// 编码为 `PMS1:base64url(JSON)` 文本（同时支持扫码与手动输入兜底，U3）。
    pub fn encode_string(&self) -> CoreResult<String> {
        let json = serde_json::to_vec(self)?;
        Ok(format!("{}{}", QR_PREFIX, URL_SAFE_NO_PAD.encode(json)))
    }

    /// 解析文本形式。
    pub fn decode_string(text: &str) -> CoreResult<Self> {
        let trimmed = text.trim();
        let body = trimmed
            .strip_prefix(QR_PREFIX)
            .ok_or_else(|| CoreError::Validation("qr_prefix".to_string()))?;
        let json = URL_SAFE_NO_PAD
            .decode(body.trim().as_bytes())
            .map_err(|_| CoreError::Validation("qr_base64".to_string()))?;
        let payload: QRPayload = serde_json::from_slice(&json)?;
        if payload.v != QR_PROTOCOL_VERSION {
            return Err(CoreError::Validation("qr_version".to_string()));
        }
        Ok(payload)
    }
}

/// 握手 HELLO（Joiner → Host）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub v: u32,
    #[serde(with = "b64url")]
    pub token: Vec<u8>,
    pub joiner_id: String,
    pub joiner_name: String,
    #[serde(with = "b64url")]
    pub joiner_pub: Vec<u8>,
    /// Joiner 的持久重同步监听端口（0 = 未提供）。供 Host 记录后用于 `sync_now`（D-11）。
    #[serde(default)]
    pub sync_port: u16,
}

/// 握手 ACCEPT（Host → Joiner），内含用 `sync_key` 加密的 VK 信封。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accept {
    pub v: u32,
    pub host_id: String,
    pub host_name: String,
    #[serde(with = "b64url")]
    pub host_pub: Vec<u8>,
    #[serde(with = "b64url")]
    pub session_id: Vec<u8>,
    #[serde(with = "b64url")]
    pub session_salt: Vec<u8>,
    /// VK 信封 nonce（24B）。
    #[serde(with = "b64")]
    pub vk_nonce: Vec<u8>,
    /// VK 信封密文（`AEAD(sync_key, nonce, VK, aad=session_id)`）。
    #[serde(with = "b64")]
    pub vk_ct: Vec<u8>,
}

/// 握手 CONFIRM（双向密钥确认）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirm {
    #[serde(with = "b64")]
    pub mac: Vec<u8>,
}

/// 握手 REJECT。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reject {
    pub code: String,
}

/// 握手阶段帧体（明文 JSON）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum HandshakeMessage {
    Hello(Hello),
    Accept(Accept),
    Confirm(Confirm),
    Reject(Reject),
}

/// 数据阶段信封（明文外壳 + 密文内层）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEnvelope {
    #[serde(with = "b64url")]
    pub session_id: Vec<u8>,
    pub seq: u64,
    pub ts: i64,
    #[serde(with = "b64")]
    pub nonce: Vec<u8>,
    #[serde(with = "b64")]
    pub ct: Vec<u8>,
}

/// 内层载荷类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncKind {
    Push,
    Pull,
    Ack,
    ConflictNotice,
    Done,
}

/// 内层载荷（被加密）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPayload {
    pub kind: SyncKind,
    pub device_id: String,
    #[serde(default)]
    pub cursor: u64,
    #[serde(default)]
    pub changes: Vec<ChangeRecord>,
    /// 冲突条目 ID 清单（仅 ID，不含明文）。
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// 本端是否支持 blob 同步阶段（D-53）。
    ///
    /// `serde(default)` = false → 老端报文缺字段时视为不支持，新端**不发起** blob 阶段、
    /// 不发任何 `BlobMessage`，行为与 D-53 之前逐位一致（§3.12.10 能力协商）。
    #[serde(default)]
    pub blob_support: bool,
}

/// blob 阶段报文族（D-53，架构文档 §3.12.4；与 [`HandshakeMessage`] 平行）。
///
/// **线上编码说明（重要）**：`Pull` / `Meta` / `End` 体量小，直接以 `serde_json`
/// 进信封内层；`Data` 的 `ct_slice`（≤ 768 KiB）**不走 JSON-b64**——它以二进制
/// 形式进信封内层 pt（`[tag][blob_ref 16B][index u32][slice]`，见
/// `blob_transfer` 的 seal/open），线上只做**一次** Base64。若按本枚举的
/// `#[serde(with = "b64")]` 走 JSON-in-JSON 双层编码，768 KiB 分片的完整信封
/// 实为 ~1.40 MB，必超 `SYNC_MAX_FRAME_BYTES`。本枚举保留为**应用层类型**
/// （构造 / 测试 / 语义权威），Data 的线上编码由 blob_transfer 统一收口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum BlobMessage {
    /// 拉取方 → 对端：缺失清单（空清单 = 无缺，对端立即回 End）。
    Pull {
        want: Vec<String>,
    },
    /// 帧元信息（每个 blob 一次）。
    Meta {
        blob_ref: String,
        item_id: String,
        #[serde(with = "b64")]
        wrapped_key: Vec<u8>,
        #[serde(with = "b64")]
        wrapped_key_nonce: Vec<u8>,
        #[serde(with = "b64")]
        nonce: Vec<u8>,
        plaintext_len: u32,
        chunk_total: u32,
        chunk_size: u32,
    },
    /// 一片一信封（线上为二进制 pt，见模块说明）。
    Data {
        blob_ref: String,
        index: u32,
        #[serde(with = "b64")]
        ct_slice: Vec<u8>,
    },
    /// 阶段收尾（服务方视角：能提供的=completed，提供不了的=missing）。
    End {
        completed: Vec<String>,
        missing: Vec<String>,
    },
}

/// 变更集元素（端到端密文）。
///
/// **偏差 D-1 补充**：除架构文档 §3.6 列出的 `wrapped_key` / `ciphertext` 外，本实现额外
/// 携带 `wrapped_key_nonce` / `nonce` / `plaintext_len`，与容器帧头对齐（否则无法解密）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub record_type: u8,
    pub record_id: String,
    pub rev: u64,
    pub updated_at: i64,
    #[serde(default)]
    pub modified_by: String,
    pub deleted: bool,
    #[serde(with = "b64")]
    pub wrapped_key: Vec<u8>,
    #[serde(with = "b64")]
    pub wrapped_key_nonce: Vec<u8>,
    #[serde(with = "b64")]
    pub nonce: Vec<u8>,
    pub plaintext_len: u32,
    #[serde(with = "b64")]
    pub ciphertext: Vec<u8>,
}

/// 数据阶段信封 AAD = `session_id ‖ seq(8B LE) ‖ ts(8B LE)`（架构文档 §2.3、§3.7）。
pub fn envelope_aad(session_id: &[u8], seq: u64, ts: i64) -> Vec<u8> {
    let mut aad = Vec::with_capacity(session_id.len() + 16);
    aad.extend_from_slice(session_id);
    aad.extend_from_slice(&seq.to_le_bytes());
    aad.extend_from_slice(&ts.to_le_bytes());
    aad
}

/// 封装内层载荷为加密信封（`AEAD(sk, nonce, payload, aad=session_id‖seq‖ts)`）。
pub fn seal_envelope(
    sync_key: &[u8],
    session_id: &[u8],
    seq: u64,
    ts: i64,
    payload: &SyncPayload,
) -> CoreResult<SyncEnvelope> {
    let aad = envelope_aad(session_id, seq, ts);
    let nonce = crate::crypto::rng::random_nonce()?;
    let pt = serde_json::to_vec(payload)?;
    let ct = crate::crypto::aead::seal(sync_key, &nonce, &aad, &pt)?;
    Ok(SyncEnvelope {
        session_id: session_id.to_vec(),
        seq,
        ts,
        nonce: nonce.to_vec(),
        ct,
    })
}

/// 打开加密信封，返回内层载荷。
pub fn open_envelope(
    sync_key: &[u8],
    session_id: &[u8],
    env: &SyncEnvelope,
) -> CoreResult<SyncPayload> {
    if env.session_id != session_id {
        return Err(CoreError::ReplayDetected);
    }
    let aad = envelope_aad(session_id, env.seq, env.ts);
    let pt = crate::crypto::aead::open(sync_key, &env.nonce, &aad, &env.ct)?;
    Ok(serde_json::from_slice(&pt)?)
}

/// 将文本渲染为二维码 SVG（纯矢量，无图像编解码依赖；§8.1 `qrcode` 仅启用 `svg` 特性）。
pub fn qr_svg(text: &str) -> CoreResult<String> {
    let code = qrcode::QrCode::new(text.as_bytes())
        .map_err(|_| CoreError::Crypto("qrcode".to_string()))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .build())
}

#[cfg(test)]
mod qr_ips_tests {
    use super::*;

    fn sample(ips: Vec<String>) -> QRPayload {        QRPayload {
            v: QR_PROTOCOL_VERSION,
            ip: ips.first().cloned().unwrap_or_default(),
            port: 40001,
            token: vec![7u8; 32],
            host_id: "host-id".to_string(),
            host_name: "host-name".to_string(),
            fp: "fp".to_string(),
            exp: 1_700_000_000_000,
            sync_port: 40002,
            ips,
        }
    }

    #[test]
    fn ips_roundtrip_preserves_order() {
        let p = sample(vec!["192.168.1.10".to_string(), "172.25.16.1".to_string()]);
        let text = p.encode_string().unwrap();
        let back = QRPayload::decode_string(&text).unwrap();
        assert_eq!(back.ips, vec!["192.168.1.10", "172.25.16.1"]);
        assert_eq!(back.ip, "192.168.1.10");
    }

    #[test]
    fn legacy_payload_without_ips_decodes() {
        // 旧版二维码文本没有 ips 字段 → 解码为空列表，Joiner 回退到 ip（D-44 向后兼容）。
        let mut v = serde_json::to_value(&sample(vec!["192.168.1.10".to_string()])).unwrap();
        v.as_object_mut().unwrap().remove("ips");
        let text = format!(
            "{}{}",
            QR_PREFIX,
            URL_SAFE_NO_PAD.encode(v.to_string().as_bytes())
        );
        let back = QRPayload::decode_string(&text).unwrap();
        assert!(back.ips.is_empty());
        assert_eq!(back.ip, "192.168.1.10");
    }
}

/// D-53 协议层单测：能力协商兼容性与 BlobMessage serde 形态。
#[cfg(test)]
mod blob_protocol_tests {
    use super::*;

    /// 老端 SyncPayload（无 blob_support 字段）反序列化 → false（不发起 blob 阶段）。
    #[test]
    fn legacy_sync_payload_without_blob_support_defaults_false() {
        let legacy = r#"{"kind":"push","device_id":"dev","changes":[],"conflicts":[]}"#;
        let payload: SyncPayload = serde_json::from_str(legacy).unwrap();
        assert!(!payload.blob_support);
        // 新端显式 true 正常往返。
        let modern = r#"{"kind":"push","device_id":"dev","changes":[],"conflicts":[],"blob_support":true}"#;
        let payload: SyncPayload = serde_json::from_str(modern).unwrap();
        assert!(payload.blob_support);
    }

    /// BlobMessage serde 形态：tag = "t"，二进制字段 b64（应用层往返）。
    #[test]
    fn blob_message_serde_roundtrip() {
        let msgs = vec![
            BlobMessage::Pull { want: vec!["ref-a".into(), "ref-b".into()] },
            BlobMessage::Meta {
                blob_ref: "ref-a".into(),
                item_id: "item-a".into(),
                wrapped_key: vec![1u8; 48],
                wrapped_key_nonce: vec![2u8; 24],
                nonce: vec![3u8; 24],
                plaintext_len: 1234,
                chunk_total: 2,
                chunk_size: 768 * 1024,
            },
            BlobMessage::Data { blob_ref: "ref-a".into(), index: 1, ct_slice: vec![9u8; 32] },
            BlobMessage::End { completed: vec!["ref-a".into()], missing: vec![] },
        ];
        for m in msgs {
            let json = serde_json::to_string(&m).unwrap();
            assert!(json.contains(r#""t":"#));
            let back: BlobMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(m, back);
        }
    }
}
