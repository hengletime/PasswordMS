//! 同步状态模型（架构文档 §5.1）。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::vault::WrappedKey;

/// 单条记录的同步基准（用于冲突「双改」探测）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseRev {
    pub rev: u64,
    #[serde(with = "crate::model::b64serde::arr32")]
    pub hash: [u8; 32],
}

/// 已配对设备。
///
/// 【偏差 D-13】在架构 §5.1 基础上新增 `peer_ip` / `peer_port`：`sync_now` 直连重同步
/// （修复 D-11）需要重新获知对端地址，而原结构未含地址字段。二者均为**非密钥元信息**
/// （仅局域网地址），随整条记录在容器内 AEAD 加密存储，明文头部不写入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    #[serde(default)]
    pub fingerprint: String,
    /// 由 VK 包裹的共享同步密钥。
    pub sync_key_wrapped: WrappedKey,
    pub paired_at: i64,
    #[serde(default)]
    pub last_sync_at: Option<i64>,
    /// 对端最近一次已知的局域网 IPv4（用于直连重同步）。
    #[serde(default)]
    pub peer_ip: Option<String>,
    /// 对端持久重同步监听端口（配对时由对端通告）。
    #[serde(default)]
    pub peer_port: Option<u16>,
}

/// 每设备同步状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSyncState {
    pub device_id: String,
    #[serde(default)]
    pub base: HashMap<String, BaseRev>,
    #[serde(default)]
    pub last_recv_seq: u64,
    #[serde(default)]
    pub last_sent_seq: u64,
    /// O1 发送水位：已成功推送给该对端的最高 rev（`record_id → rev`）。
    ///
    /// `export_changes` 只导出 `rev > 水位` 的 live 记录（墓碑走 base 过滤不变），
    /// 砍掉「每轮全量重发」。失败安全与 [`note_sent_tombstones`](crate::sync::merge)
    /// 同款语义：发送成功后才推进；同步中途失败则 `save_sync_state` 不执行，
    /// 水位随会话丢弃，下一轮自动重发（宁多勿丢，对端 merge 幂等）。
    /// `serde(default)` 兼容既有状态帧（无字段 = 从未发过 = 全量导出）。
    #[serde(default)]
    pub sent_watermark: HashMap<String, u64>,
}
