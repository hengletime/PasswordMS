//! 同步报文 schema（架构文档 §3.3、§3.6）。
//!
//! 二进制字段：JSON 内标准 Base64；二维码/令牌使用 base64url（无填充）。

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

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
        STANDARD
            .decode(s.as_bytes())
            .map_err(serde::de::Error::custom)
    }

    /// 可选变体。
    pub mod opt {
        use super::*;
        /// 序列化可选 Base64。
        pub fn serialize<S: serde::Serializer>(
            bytes: &Option<Vec<u8>>,
            s: S,
        ) -> Result<S::Ok, S::Error> {
            match bytes {
                Some(b) => s.serialize_str(&STANDARD.encode(b)),
                None => s.serialize_none(),
            }
        }
        /// 反序列化可选 Base64。
        pub fn deserialize<'de, D: serde::Deserializer<'de>>(
            d: D,
        ) -> Result<Option<Vec<u8>>, D::Error> {
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
        URL_SAFE_NO_PAD
            .decode(s.as_bytes())
            .map_err(serde::de::Error::custom)
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
    /// Joiner 的**长期**身份公钥（X25519，32B）——【D-70】。
    ///
    /// 与上面的 `joiner_pub` 是**两把完全不同的密钥**：`joiner_pub` 是每次配对新生成的
    /// 临时 ECDH 公钥（会话结束即作废），拿它算出的指纹每次都不一样，写进
    /// `PairedDevice.fingerprint` 就是「用一次性值冒充设备身份」（D-67 同类错误）。
    /// 这里发的是本机身份文件里的长期公钥，跨配对稳定，电脑端才能真正「认人」。
    ///
    /// `serde(default)` = 空 → 老客户端不发该字段时 Host 读到空 vec，指纹留空串、
    /// UI 不渲染指纹行，行为与 D-70 之前逐位一致（向后兼容，不崩、不退化）。
    #[serde(default, with = "b64url")]
    pub joiner_identity_pub: Vec<u8>,
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

/// 握手 REJECT / 数据阶段「中止帧」（#22）。
///
/// 握手阶段恒用 [`Reject::handshake`] 构造：`v/sid/seq/ts/nonce/mac` 全 `None`，
/// `skip_serializing_if` 让线上字节与 1.6.3 逐字节相同（`{"t":"reject","code":…}`）。
/// 数据阶段的新帧补全六字段并带 HMAC（见 [`build_abort_frame`]）；新版接收方恒
/// Strict，缺任一字段的裸帧（old→new）一律 [`CoreError::SyncAborted`]。
/// `serde(default)` 保证旧版裸帧仍能被新版解析（形状层），认证在验证入口做。
///
/// 【形状不变量（M11）】中止帧相对 [`SyncEnvelope`] 始终至少缺一个**无默认值的必填**
/// 字段，当前恰好缺 `session_id`（帧内用的是 `sid`）与 `ct` 两个，安全边际各只有一
/// 字段宽；改帧形状前先看 M11（把 `sid` 改名 `session_id` 且补 `ct`、或给
/// `SyncEnvelope` 的 `session_id`/`ct` 都加 `#[serde(default)]`，都会让缺口归零，
/// 使中止帧被 `from_slice::<SyncEnvelope>` 误认成正常信封）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Reject {
    pub code: String,
    /// 带认证中止帧的形状版本标记（不进 MAC）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<u32>,
    /// 发送侧填入的 `session_id`（形状完整性；MAC 用接收侧**本地** session_id 重算）。
    #[serde(default, with = "b64::opt", skip_serializing_if = "Option::is_none")]
    pub sid: Option<Vec<u8>>,
    /// 只做 MAC 绑定、不参与单调判定（接收侧不调 `replay.accept`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
    /// 16B 随机数，进 MAC 但不加语义。
    #[serde(default, with = "b64::opt", skip_serializing_if = "Option::is_none")]
    pub nonce: Option<Vec<u8>>,
    /// 32B HMAC-SHA256。
    #[serde(default, with = "b64::opt", skip_serializing_if = "Option::is_none")]
    pub mac: Option<Vec<u8>>,
}

impl Reject {
    /// 握手阶段 REJECT：只带 code，六个认证字段全 None（线上字节与 1.6.3 相同）。
    pub fn handshake(code: &str) -> Self {
        Self {
            code: code.to_string(),
            ..Default::default()
        }
    }
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
    /// 本次 Push 是「只配对、不交换数据」（D-147）。
    ///
    /// 配对通道的握手里本来夹着一次完整双向同步：手机扫码一成功，整库就被拉过去了。
    /// 主人裁定「有时只是想配对」——配对只建立信任关系与密钥采纳，数据由用户自己在
    /// 状态页点「立即同步」。
    ///
    /// **向后兼容**：`serde(default)` = false。老端收到 `pair_only: true` 会忽略该字段
    /// 照常同步（退化为旧行为，不丢数据）；新端收到缺字段 = false，同样是旧行为。
    /// 也就是说新旧混用只会「多同步一次」，绝不会少。
    #[serde(default)]
    pub pair_only: bool,
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
    Pull { want: Vec<String> },
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

/// 数据阶段收到的帧若不是信封，尝尝它是不是对端的**中止帧**（D-199）。
///
/// 只有 `{"t":"reject", …}` 这种握手形状才认；正常信封没有 `t` 字段，不会被误判。
///
/// 【#22】这是**纯形状解析**（不做认证），只保留给形状兼容/回归用例。生产接收路径
/// 一律走 [`verify_abort_frame`]（恒 Strict、先验 MAC 再产生行为），绝不直接调用本函数。
pub fn parse_reject(bytes: &[u8]) -> Option<CoreError> {
    match serde_json::from_slice::<HandshakeMessage>(bytes) {
        Ok(HandshakeMessage::Reject(r)) => Some(reject_code_to_error(&r.code)),
        _ => None,
    }
}

/// 中止帧认证上下文（#22）：用一个结构体打包，别散着加参数；方向写死。
///
/// 发送侧：`sender_id = local_id`，`receiver_id = peer_id`；
/// 接收侧：`sender_id = peer_id`，`receiver_id = local_id`。
/// 两端 `AbortCtx` 天然互为镜像（local/peer 互换）；方向写反两端会算出不同 MAC。
#[derive(Debug, Clone)]
pub struct AbortCtx {
    pub local_id: String,
    pub peer_id: String,
}

/// 中止帧 MAC 输入的固定前缀（纵深域分离；主要域分离由 HKDF_INFO_SYNC_ABORT 承担）。
const ABORT_LABEL: &[u8] = b"passwordms/sync/v1/abort-frame";
/// 带认证中止帧的形状版本标记（不进 MAC 输入域）。
pub const ABORT_VERSION: u32 = 1;
/// 中止帧 nonce 长度（字节）。
const ABORT_NONCE_LEN: usize = 16;
/// 中止帧 MAC 长度（HMAC-SHA256，字节）。
const ABORT_MAC_LEN: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// 中止帧 MAC 输入域的全部字段（打包，避免散着加参数）。
struct AbortMacFields<'a> {
    sid: &'a [u8],
    seq: u64,
    ts: i64,
    nonce: &'a [u8],
    code: &'a str,
    sender_id: &'a str,
    receiver_id: &'a str,
}

/// 长度前缀追加：`u64 LE 长度 ‖ bytes`（全部字段长度前缀，防拼接歧义）。
fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// 中止帧 MAC 输入域（全部长度前缀）：
/// `ABORT_LABEL ‖ sid ‖ seq ‖ ts ‖ nonce ‖ code ‖ sender_id ‖ receiver_id`。
///
/// `code` 走「长度前缀 + UTF-8 原始字节」，**绝不**把 `serde_json` 结果或 JSON 片段进 MAC。
/// `receiver_id` 必进 MAC（否则两端方向可替换）。
fn abort_mac_input(f: &AbortMacFields<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    lp(&mut out, ABORT_LABEL);
    lp(&mut out, f.sid);
    lp(&mut out, &f.seq.to_le_bytes());
    lp(&mut out, &f.ts.to_le_bytes());
    lp(&mut out, f.nonce);
    lp(&mut out, f.code.as_bytes());
    lp(&mut out, f.sender_id.as_bytes());
    lp(&mut out, f.receiver_id.as_bytes());
    out
}

/// 用 `derive_abort_key` 派生的专用密钥计算 HMAC-SHA256。
fn compute_abort_mac(abort_key: &[u8], f: &AbortMacFields<'_>) -> CoreResult<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(abort_key)
        .map_err(|_| CoreError::Crypto("abort hmac key".to_string()))?;
    mac.update(&abort_mac_input(f));
    Ok(mac.finalize().into_bytes().to_vec())
}

/// 构造带认证字段的中止帧（#22，发送侧）。
///
/// `sync_key` 必须是数据阶段 `recv_push_pages`/`recv_done` 收到的那把（`&[u8]`），
/// 与接收侧 [`verify_abort_frame`] 完全同源。`sid` 用本地 `session_id`，方向按
/// [`AbortCtx`] 写死。`seq` 只做 MAC 绑定、不参与单调判定（接收侧不调
/// `replay.accept`），故取任一确定性值即可。
pub fn build_abort_frame(
    sync_key: &[u8],
    session_id: &[u8],
    ctx: &AbortCtx,
    seq: u64,
    ts: i64,
    code: &str,
) -> CoreResult<Vec<u8>> {
    let abort_key = crate::sync::handshake::derive_abort_key(sync_key)?;
    let nonce = crate::crypto::rng::random_bytes(ABORT_NONCE_LEN)?;
    let mac = compute_abort_mac(
        &abort_key[..],
        &AbortMacFields {
            sid: session_id,
            seq,
            ts,
            nonce: &nonce,
            code,
            sender_id: &ctx.local_id,
            receiver_id: &ctx.peer_id,
        },
    )?;
    let frame = Reject {
        code: code.to_string(),
        v: Some(ABORT_VERSION),
        sid: Some(session_id.to_vec()),
        seq: Some(seq),
        ts: Some(ts),
        nonce: Some(nonce),
        mac: Some(mac),
    };
    Ok(serde_json::to_vec(&HandshakeMessage::Reject(frame))?)
}

/// 验证数据阶段收到的「中止帧」（#22，接收侧恒 **Strict**）。
///
/// 验证顺序（crypto 硬约束：先验 MAC 再产生行为）：
/// ① 形状解析 + 字段完整性/长度闸（mac 32B、nonce 16B、`sync_key.len()==KEY_LEN`
///    由 `derive_abort_key` 入口兜）+ 帧内 `sid` 与本地 `session_id` 的诊断一致性检查
///    ——纯解析 / 形状完整性，不产生行为；
/// ② 用**本地** `session_id` / `sender_id=peer_id` / `receiver_id=local_id` 重算 expected MAC；
/// ③ [`crate::sync::handshake::verify_mac`] 常量时间比对（长度不等即 false，
///    截断天然被拒，不用 `==`）；
/// ④ 验过之后才 `check_ts(ts, now)` 与 `reject_code_to_error(code)`；`now` 只在此步消费，
///    第 ①–③ 步绝不读它；
/// ⑤ 任一步失败 → [`CoreError::SyncAborted`]，绝不回显/记录 raw code，绝不落 Internal。
///
/// 返回 `None` = 不是 reject 形状（调用方一律收敛到 `CoreError::SyncAborted`；
/// M10：绝不回落到 `Internal("json")` 那种可区分文案）；`Some` = 中止帧判定结果。
/// 帧内 `sid` 的一致性检查只是**形状完整性、不是第二套认证闸**：认证仍由 MAC
/// 单独承担，帧内 `sid` 绝不进 MAC 输入域（本地 `session_id` 才是唯一会话绑定源，
/// 信任对端值会弱化绑定）；跨会话复制由 MAC 输入域里的本地 `session_id` 兜住。
///
/// 【abort 处理路径不得写入任何持久状态】本函数是纯函数：不 `save_sync_state`、
/// 不 `update_paired_addr`、不写游标、不调 `replay.accept`（只 `check_ts`）、不落日志型
/// 状态机；失败路径维持「整段丢弃、下轮重发」。一旦将来触发状态写入，seq 必须改走
/// `replay.accept`（否则重放护栏与序号语义会对不上）。
pub fn verify_abort_frame(
    bytes: &[u8],
    sync_key: &[u8],
    session_id: &[u8],
    ctx: &AbortCtx,
    replay: &crate::sync::replay::ReplayGuard,
    now: i64,
) -> Option<CoreError> {
    // ① 形状解析：只认 `{"t":"reject", …}`；正常信封/其它形状 → None（不是中止帧）。
    let r = match serde_json::from_slice::<HandshakeMessage>(bytes) {
        Ok(HandshakeMessage::Reject(r)) => r,
        _ => return None,
    };
    // 字段完整性/长度闸：六字段缺一即为「裸帧」（old→new，已接受的单向退化）或
    // 畸形帧，新版恒 Strict。
    let (seq, ts, nonce, mac) = match (r.v, &r.sid, r.seq, r.ts, &r.nonce, &r.mac) {
        (Some(v), Some(sid), Some(seq), Some(ts), Some(nonce), Some(mac))
            if v == ABORT_VERSION
                && mac.len() == ABORT_MAC_LEN
                && nonce.len() == ABORT_NONCE_LEN =>
        {
            // 诊断一致性检查（形状完整性，不是第二套认证闸）：帧内 `sid` 必须与本地
            // `session_id` 常量时间相等，防止这个「死字段」被任意篡改而无人察觉。
            // 认证仍由下面的 MAC 单独承担；帧内 `sid` 绝不进 MAC 输入域（本地
            // `session_id` 才是唯一会话绑定源，信任对端值会弱化绑定）。
            if sid.len() != session_id.len() || !bool::from(session_id.ct_eq(sid.as_slice())) {
                return Some(CoreError::SyncAborted);
            }
            (seq, ts, nonce, mac)
        }
        _ => return Some(CoreError::SyncAborted),
    };

    // ② 用本地 session_id / 方向重算 expected MAC（方向写死，两端镜像）。
    let expected = match crate::sync::handshake::derive_abort_key(sync_key) {
        Ok(k) => match compute_abort_mac(
            &k[..],
            &AbortMacFields {
                sid: session_id,
                seq,
                ts,
                nonce,
                code: &r.code,
                sender_id: &ctx.peer_id,
                receiver_id: &ctx.local_id,
            },
        ) {
            Ok(m) => m,
            Err(_) => return Some(CoreError::SyncAborted),
        },
        Err(_) => return Some(CoreError::SyncAborted),
    };
    // ③ 常量时间比对；长度不等即 false，截断天然被拒，绝不写 `==`。
    if !crate::sync::handshake::verify_mac(&expected, mac) {
        return Some(CoreError::SyncAborted);
    }
    // ④ 验过之后才产生行为：时间窗 + 白名单 code → 具体错误；`now` 只在此步消费。
    if replay.check_ts(ts, now).is_err() {
        return Some(CoreError::SyncAborted);
    }
    Some(reject_code_to_error(&r.code))
}

/// 中止帧的 code → 具体错误（前端据此给出对应文案）。
///
/// **全仓唯一一份映射**：`resync.rs` 与 `client.rs` 各有同名私有函数的年代已经过去——
/// 三份表迟早漂移，而漂移的后果是「同一件事三种说法」。
///
/// 认不出来的 code 一律回 [`CoreError::SyncAborted`]——**绝不**用 `Internal`：
/// 那会把「对方主动中止」说成「程序出错」。
pub fn reject_code_to_error(code: &str) -> CoreError {
    use crate::error::codes::*;
    match code {
        ERR_PAIR_TOKEN_USED => CoreError::PairTokenUsed,
        ERR_PAIR_TOKEN_EXPIRED => CoreError::PairTokenExpired,
        ERR_PAIR_TOKEN_INVALID => CoreError::PairTokenInvalid,
        ERR_NOT_PAIRED => CoreError::NotPaired,
        ERR_FINGERPRINT_MISMATCH => CoreError::FingerprintMismatch,
        ERR_REPLAY_DETECTED => CoreError::ReplayDetected,
        ERR_VAULT_KEY_MISMATCH => CoreError::VaultKeyMismatch,
        _ => CoreError::SyncAborted,
    }
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

    fn sample(ips: Vec<String>) -> QRPayload {
        QRPayload {
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
        let mut v = serde_json::to_value(sample(vec!["192.168.1.10".to_string()])).unwrap();
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
        let modern =
            r#"{"kind":"push","device_id":"dev","changes":[],"conflicts":[],"blob_support":true}"#;
        let payload: SyncPayload = serde_json::from_str(modern).unwrap();
        assert!(payload.blob_support);
    }

    /// BlobMessage serde 形态：tag = "t"，二进制字段 b64（应用层往返）。
    #[test]
    fn blob_message_serde_roundtrip() {
        let msgs = vec![
            BlobMessage::Pull {
                want: vec!["ref-a".into(), "ref-b".into()],
            },
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
            BlobMessage::Data {
                blob_ref: "ref-a".into(),
                index: 1,
                ct_slice: vec![9u8; 32],
            },
            BlobMessage::End {
                completed: vec!["ref-a".into()],
                missing: vec![],
            },
        ];
        for m in msgs {
            let json = serde_json::to_string(&m).unwrap();
            assert!(json.contains(r#""t":"#));
            let back: BlobMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(m, back);
        }
    }
}
