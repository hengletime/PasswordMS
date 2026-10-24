//! blob 传输阶段（D-53，架构文档 §3.12）。
//!
//! 在既有 `Push → Push → Done` 交换完成后按需拉取附件密文 blob（纯增量阶段）：
//!
//! - **零解密过网**：发送端直接切片 blob 帧密文（`Frame.ct`），接收端重组后一次
//!   `verify_blob_frame`（AEAD tag 兼作组帧完整性 + 真实性校验）；
//! - **能力协商**：双方 `SyncPayload.blob_support = true` 才进入（serde default=false，
//!   老端不触发、不收新报文，行为与现状逐位一致，§3.12.10）；
//! - **防重放**：全部信封走同一 [`ReplayGuard`]，`seq` 从 Done 之后单调延续，
//!   AAD 仍绑 `session_id‖seq‖ts`（A-06/A-08 不受影响）；
//! - **spool 物理隔离**（§3.12.6）：`<vault>.blobspool/<blob_ref>/` 只存密文切片
//!   （A-01），`blobs` 索引仅在「组帧 → tag 验证通过 → 入库」后写入，
//!   `read_blob` 永不见半成品；断连后按 manifest 续传，只补差额。
//!
//! **线上编码（关键裁定）**：`Data` 的 `ct_slice` 以**二进制**进信封内层 pt
//! （`[tag u8][blob_ref 16B][index u32][slice]`），线上只做一次 Base64——
//! 768 KiB 分片的完整信封 JSON ≈ 1,048,800 B < `SYNC_MAX_FRAME_BYTES`。
//! 若走 JSON-in-JSON 双层 b64（`BlobMessage::Data` 的 serde 形态），同分片
//! 实为 ~1.40 MB 必超限。`Pull`/`Meta`/`End` 体量小，走 serde_json。

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::constants::{
    AEAD_NONCE_LEN, AEAD_TAG_LEN, FLAG_BLOB, FRAME_HEADER_LEN, MAX_VAULT_BYTES, RECORD_TYPE_ITEM,
    SYNC_BLOB_CHUNK_BYTES, SYNC_MAX_FRAME_BYTES, WRAPPED_KEY_CT_LEN,
};
use crate::crypto::{aead, rng};
use crate::error::{stage_io, CoreError, CoreResult};
use crate::model::vault::WrappedKey;
use crate::store::frame::{bytes_to_record_id, record_id_to_bytes, Frame};
use crate::store::repository::{BlobWriteOutcome, Repository};
use crate::sync::access::RepoAccess;
use crate::sync::protocol::{envelope_aad, BlobMessage, SyncEnvelope};
use crate::sync::replay::ReplayGuard;
use crate::sync::transport;

/// 内层 pt 首字节类型标签（Data 用二进制 pt，其余为 serde_json）。
const PT_TAG_PULL: u8 = 0;
const PT_TAG_META: u8 = 1;
const PT_TAG_DATA: u8 = 2;
const PT_TAG_END: u8 = 3;

/// 一个 blob 阶段的结果（拉取方视角；completed/missing 以**本端**为准——
/// 组帧验证、容量预检都发生在本端，对端无法代答）。
#[derive(Debug, Clone, Default)]
pub struct BlobPhaseReport {
    /// 本阶段落地可用（新入库或本已有）的 blob_ref。
    pub completed: Vec<String>,
    /// 仍缺失的 blob_ref（对端没有 / 容量拒绝 / 组帧失败 / 本地字节不同）。
    pub missing: Vec<String>,
    /// 本阶段实际新入库的 blob 数（IPC `blobs` 计数来源）。
    pub appended: usize,
}

/// 能力协商门（§3.12.10）：本端与对端**都**声明支持才进入 blob 阶段。
pub fn blob_phase_enabled(local_support: bool, peer_support: bool) -> bool {
    local_support && peer_support
}

/// 容量预检（Meta 阶段；§3.12.6「上限校验前置」）。
///
/// 预期帧长度 = 帧头 124B + 密文（明文 + 16B tag）+ IK 包裹余量 4 KiB；
/// 超限在 **Meta 阶段**即拒绝，一个分片都不落盘。
pub fn capacity_ok(current_vault_bytes: u64, plaintext_len: u32) -> bool {
    let incoming = FRAME_HEADER_LEN + plaintext_len as usize + AEAD_TAG_LEN;
    current_vault_bytes.saturating_add(incoming as u64) + 4096 <= MAX_VAULT_BYTES
}

// ===== 信封 seal/open（BlobMessage ↔ SyncEnvelope）=====

/// 封装 BlobMessage 为加密信封（AAD = `session_id‖seq‖ts`，与数据阶段一致）。
///
/// Data 走二进制 pt（单次 Base64 预算），其余 serde_json。
pub fn seal_blob_envelope(
    sync_key: &[u8],
    session_id: &[u8],
    seq: u64,
    ts: i64,
    msg: &BlobMessage,
) -> CoreResult<SyncEnvelope> {
    let mut pt: Vec<u8> = Vec::with_capacity(128);
    match msg {
        BlobMessage::Pull { .. } | BlobMessage::Meta { .. } | BlobMessage::End { .. } => {
            pt.push(match msg {
                BlobMessage::Pull { .. } => PT_TAG_PULL,
                BlobMessage::Meta { .. } => PT_TAG_META,
                _ => PT_TAG_END,
            });
            pt.extend_from_slice(&serde_json::to_vec(msg)?);
        }
        BlobMessage::Data {
            blob_ref,
            index,
            ct_slice,
        } => {
            pt.push(PT_TAG_DATA);
            pt.extend_from_slice(&record_id_to_bytes(blob_ref)?);
            pt.extend_from_slice(&index.to_le_bytes());
            pt.extend_from_slice(ct_slice);
        }
    }
    let aad = envelope_aad(session_id, seq, ts);
    let nonce = rng::random_nonce()?;
    let ct = aead::seal(sync_key, &nonce, &aad, &pt)?;
    Ok(SyncEnvelope {
        session_id: session_id.to_vec(),
        seq,
        ts,
        nonce: nonce.to_vec(),
        ct,
    })
}

/// 打开 blob 信封（session_id 不符按重放处理，与数据阶段同语义）。
pub fn open_blob_envelope(
    sync_key: &[u8],
    session_id: &[u8],
    env: &SyncEnvelope,
) -> CoreResult<BlobMessage> {
    if env.session_id != session_id {
        return Err(CoreError::ReplayDetected);
    }
    let aad = envelope_aad(session_id, env.seq, env.ts);
    let pt = aead::open(sync_key, &env.nonce, &aad, &env.ct)?;
    if pt.is_empty() {
        return Err(CoreError::Validation("blob_pt_empty".to_string()));
    }
    let (tag, body) = (pt[0], &pt[1..]);
    match tag {
        PT_TAG_DATA => {
            if body.len() < 20 {
                return Err(CoreError::Validation("blob_data_short".to_string()));
            }
            let mut rid = [0u8; 16];
            rid.copy_from_slice(&body[..16]);
            let index = u32::from_le_bytes([body[16], body[17], body[18], body[19]]);
            Ok(BlobMessage::Data {
                blob_ref: bytes_to_record_id(&rid),
                index,
                ct_slice: body[20..].to_vec(),
            })
        }
        PT_TAG_PULL | PT_TAG_META | PT_TAG_END => {
            let msg: BlobMessage = serde_json::from_slice(body)?;
            let ok = matches!(
                (&msg, tag),
                (BlobMessage::Pull { .. }, PT_TAG_PULL)
                    | (BlobMessage::Meta { .. }, PT_TAG_META)
                    | (BlobMessage::End { .. }, PT_TAG_END)
            );
            if !ok {
                return Err(CoreError::Validation("blob_tag_mismatch".to_string()));
            }
            Ok(msg)
        }
        _ => Err(CoreError::Validation("blob_tag".to_string())),
    }
}

/// 发送一个 blob 信封（`next_seq` 单调递增；调用方在阶段结束后持久化游标）。
fn send_blob(
    stream: &mut std::net::TcpStream,
    sync_key: &[u8],
    session_id: &[u8],
    next_seq: &mut u64,
    now: i64,
    msg: &BlobMessage,
) -> CoreResult<()> {
    let seq = *next_seq;
    *next_seq += 1;
    let env = seal_blob_envelope(sync_key, session_id, seq, now, msg)?;
    let wire = serde_json::to_vec(&env)?;
    // 强制单测同步断言：768 KiB 分片的完整信封 JSON < SYNC_MAX_FRAME_BYTES（D-53）。
    debug_assert!(wire.len() <= SYNC_MAX_FRAME_BYTES as usize, "blob 信封超限");
    transport::send_frame(stream, &wire).map_err(|e| stage_io(e, "blob_send"))
}

/// 接收并打开一个 blob 信封（重放 / 时间窗校验在先）。
fn recv_blob(
    stream: &mut std::net::TcpStream,
    sync_key: &[u8],
    session_id: &[u8],
    replay: &mut ReplayGuard,
    now: i64,
) -> CoreResult<BlobMessage> {
    let bytes = transport::recv_frame(stream).map_err(|e| stage_io(e, "blob_recv"))?;
    let env: SyncEnvelope = serde_json::from_slice(&bytes)?;
    replay.accept(env.seq, env.ts, now)?;
    open_blob_envelope(sync_key, session_id, &env)
}

// ===== spool（断点续传；§3.12.6）=====

/// spool 记账文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpoolManifest {
    blob_ref: String,
    chunk_total: u32,
    chunk_size: u32,
    received: Vec<u32>,
}

/// 单个 blob 的接收暂存区（只存**密文**切片，A-01）。
struct Spool {
    dir: PathBuf,
    manifest: SpoolManifest,
    /// 明文长度 + tag = 完整密文长度（用于尾片长度校验）。
    ct_len: usize,
}

impl Spool {
    /// spool 根目录：`<vault>.blobspool/`。
    pub fn root(vault_path: &std::path::Path) -> PathBuf {
        let mut s = vault_path.as_os_str().to_os_string();
        s.push(".blobspool");
        PathBuf::from(s)
    }

    /// 打开或新建 spool；已有 manifest 与本次 Meta 不一致（对端重发了不同尺寸的
    /// blob，或上次异常中断留下脏档）时丢弃重建。
    pub fn open_or_create(root: &std::path::Path, meta: &BlobMeta) -> CoreResult<Self> {
        let dir = root.join(&meta.blob_ref);
        // D-91（审查 R1）纵深防御：规范化 UUID 目录的父目录必为 root 本身。
        // 即便上游校验被绕过（绝对路径整体替换 / `..` 穿越），在此一票拦截。
        if dir.parent() != Some(root) {
            return Err(CoreError::Validation("blob_ref_path".to_string()));
        }
        let ct_len = meta.plaintext_len as usize + AEAD_TAG_LEN;
        let mut spool = Self {
            dir,
            manifest: SpoolManifest {
                blob_ref: meta.blob_ref.clone(),
                chunk_total: meta.chunk_total,
                chunk_size: meta.chunk_size,
                received: Vec::new(),
            },
            ct_len,
        };
        let manifest_path = spool.dir.join("manifest.json");
        if let Ok(raw) = std::fs::read(&manifest_path) {
            if let Ok(saved) = serde_json::from_slice::<SpoolManifest>(&raw) {
                let compatible = saved.blob_ref == meta.blob_ref
                    && saved.chunk_total == meta.chunk_total
                    && saved.chunk_size == meta.chunk_size;
                if compatible {
                    spool.manifest = saved;
                }
            }
        }
        std::fs::create_dir_all(&spool.dir)?;
        spool.persist()?;
        Ok(spool)
    }

    /// 第 `index` 片的期望长度（尾片可短）。
    fn expected_len(&self, index: u32) -> usize {
        let full = self.manifest.chunk_size as usize;
        let last = self.manifest.chunk_total - 1;
        if index >= last {
            self.ct_len - last as usize * full
        } else {
            full
        }
    }

    /// 落一片（密文）。已收过（idx 在账 + part 文件长度正确）则跳过——
    /// 断连续传时「只补差额」的关键路径。
    pub fn write_chunk(&mut self, index: u32, slice: &[u8]) -> CoreResult<bool> {
        if index >= self.manifest.chunk_total {
            return Err(CoreError::Validation("blob_chunk_index".to_string()));
        }
        if slice.len() != self.expected_len(index) {
            return Err(CoreError::Validation("blob_chunk_len".to_string()));
        }
        if self.manifest.received.contains(&index) && self.part_len(index) == Some(slice.len()) {
            return Ok(false); // 已收，跳过
        }
        std::fs::write(self.dir.join(format!("{index}.part")), slice)?;
        if !self.manifest.received.contains(&index) {
            self.manifest.received.push(index);
        }
        self.persist()?;
        Ok(true)
    }

    fn part_len(&self, index: u32) -> Option<usize> {
        std::fs::metadata(self.dir.join(format!("{index}.part")))
            .ok()
            .map(|m| m.len() as usize)
    }

    /// 收齐校验：每个分片都在账且文件长度正确。
    pub fn is_complete(&self) -> bool {
        (0..self.manifest.chunk_total).all(|i| {
            self.manifest.received.contains(&i) && self.part_len(i) == Some(self.expected_len(i))
        })
    }

    /// 组帧：按序拼接全部密文切片。缺片 / 长度不符 → `ERR_BLOB_INCOMPLETE`。
    pub fn assemble(&self) -> CoreResult<Vec<u8>> {
        let mut ct = Vec::with_capacity(self.ct_len);
        for i in 0..self.manifest.chunk_total {
            let expected = self.expected_len(i);
            let raw = std::fs::read(self.dir.join(format!("{i}.part")))
                .map_err(|_| CoreError::BlobIncomplete)?;
            if raw.len() != expected {
                return Err(CoreError::BlobIncomplete);
            }
            ct.extend_from_slice(&raw);
        }
        Ok(ct)
    }

    fn persist(&self) -> CoreResult<()> {
        let raw = serde_json::to_vec(&self.manifest)?;
        std::fs::write(self.dir.join("manifest.json"), raw)?;
        Ok(())
    }

    /// 丢弃整个 spool 目录（finalize 失败 / 容量拒绝 / 条目被删）。
    pub fn discard(self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// 清理孤儿 spool 目录：blob_ref 不再对应任何 attachment 条目（条目中途被删）。
/// 在每次同步的 blob 阶段开始时调用（§3.12.6「孤儿清理」）。
pub fn cleanup_orphan_spools(repo: &Repository) -> CoreResult<()> {
    let root = Spool::root(&repo.path());
    if !root.is_dir() {
        return Ok(());
    }
    let valid: HashSet<String> = repo.attachment_blob_refs().into_iter().collect();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !valid.contains(&name) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    // 根目录已空（上轮收尾后遗留）则一并移除；非空时 remove_dir 自然失败，无副作用。
    let _ = std::fs::remove_dir(&root);
    Ok(())
}

// ===== Meta 解析与 finalize =====

/// 一次 Meta 报文的校验后形态。
#[derive(Debug, Clone)]
struct BlobMeta {
    blob_ref: String,
    wrapped_key: Vec<u8>,
    wrapped_key_nonce: Vec<u8>,
    nonce: [u8; AEAD_NONCE_LEN],
    plaintext_len: u32,
    chunk_total: u32,
    chunk_size: u32,
}

impl BlobMeta {
    /// 校验字段形状与分片数学；不符返回 Validation（视为对端协议错误）。
    fn from_message(m: BlobMessage) -> CoreResult<Self> {
        let BlobMessage::Meta {
            blob_ref,
            item_id: _,
            wrapped_key,
            wrapped_key_nonce,
            nonce,
            plaintext_len,
            chunk_total,
            chunk_size,
        } = m
        else {
            return Err(CoreError::Validation("expected_meta".to_string()));
        };
        // D-91（全仓审查 R1，安全审计 F-001 🔴）：blob_ref 必须是**规范 UUID**——
        // 先经 UUID 解析校验（绝对路径 / `..` / 含路径分隔符的串一律拒绝），再回写
        // 规范化形式（小写连字符）。此前 blob_ref 为自由 String 直接 `root.join()`，
        // 恶意配对对端可借 Meta 报文把 spool 目录落到宿主机任意路径（路径穿越 →
        // 任意位置写受控内容）。uuid 解析失败的串到不了这里之后的任何 join。
        let rid = record_id_to_bytes(&blob_ref)?;
        let blob_ref = bytes_to_record_id(&rid);
        if wrapped_key.len() != WRAPPED_KEY_CT_LEN
            || wrapped_key_nonce.len() != AEAD_NONCE_LEN
            || nonce.len() != AEAD_NONCE_LEN
        {
            return Err(CoreError::Validation("blob_meta_shape".to_string()));
        }
        if chunk_size == 0 || chunk_size as usize > SYNC_BLOB_CHUNK_BYTES || chunk_total == 0 {
            return Err(CoreError::Validation("blob_chunk_math".to_string()));
        }
        let ct_len = plaintext_len as usize + AEAD_TAG_LEN;
        let last = chunk_total - 1;
        let covers = last as usize * (chunk_size as usize) < ct_len
            && ct_len <= chunk_total as usize * (chunk_size as usize);
        if !covers {
            return Err(CoreError::Validation("blob_chunk_math".to_string()));
        }
        let mut nonce_arr = [0u8; AEAD_NONCE_LEN];
        nonce_arr.copy_from_slice(&nonce);
        Ok(Self {
            blob_ref,
            wrapped_key,
            wrapped_key_nonce,
            nonce: nonce_arr,
            plaintext_len,
            chunk_total,
            chunk_size,
        })
    }

    /// 组装完整 blob 帧（不验证；验证在 [`Repository::verify_blob_frame`]）。
    fn into_frame(self, ct: Vec<u8>) -> CoreResult<Frame> {
        if ct.len() != self.plaintext_len as usize + AEAD_TAG_LEN {
            return Err(CoreError::BlobIncomplete);
        }
        Ok(Frame {
            record_type: RECORD_TYPE_ITEM,
            flags: FLAG_BLOB,
            record_id: record_id_to_bytes(&self.blob_ref)?,
            wrapped_key: WrappedKey {
                nonce: self.wrapped_key_nonce,
                ct: self.wrapped_key,
            },
            nonce: self.nonce,
            plaintext_len: self.plaintext_len,
            ct,
        })
    }
}

/// 由本端已有的 blob 帧构造 Meta（发送端零解密：字段全部取自帧头/帧体）。
fn meta_of(blob_ref: &str, frame: &Frame) -> BlobMessage {
    let ct_len = frame.ct.len();
    let chunk_total = ct_len.div_ceil(SYNC_BLOB_CHUNK_BYTES);
    BlobMessage::Meta {
        blob_ref: blob_ref.to_string(),
        item_id: blob_ref.to_string(),
        wrapped_key: frame.wrapped_key.ct.clone(),
        wrapped_key_nonce: frame.wrapped_key.nonce.clone(),
        nonce: frame.nonce.to_vec(),
        plaintext_len: frame.plaintext_len,
        chunk_total: chunk_total as u32,
        chunk_size: SYNC_BLOB_CHUNK_BYTES as u32,
    }
}

/// finalize 一个收齐的 blob：组帧 → tag 验证 → 入库（幂等，不覆盖本地）。
///
/// 成败都清 spool：成功已入库；失败（缺片/tag 不符）重同步时全量重传。
fn finalize_blob(
    repo: &mut Repository,
    meta: BlobMeta,
    spool: Spool,
) -> CoreResult<FinalizeResult> {
    // 收齐校验（manifest 在账 + part 文件长度双重校验）先行，缺片早退。
    let outcome = if !spool.is_complete() {
        Err(CoreError::BlobIncomplete)
    } else {
        spool
            .assemble()
            .and_then(|ct| meta.into_frame(ct))
            .and_then(|frame| {
                repo.verify_blob_frame(&frame)
                    .and_then(|_| repo.write_blob_frame(frame))
            })
    };
    spool.discard();
    match outcome? {
        BlobWriteOutcome::Appended => Ok(FinalizeResult::Appended),
        BlobWriteOutcome::Skipped => Ok(FinalizeResult::AlreadyPresent),
        // 字节不同（理论罕见）→ 保留本地、报缺失（§3.12.5 异常路径）。
        BlobWriteOutcome::KeptLocal => Ok(FinalizeResult::KeptLocal),
    }
}

/// finalize 结果（报告聚合用）。
enum FinalizeResult {
    /// 新入库。
    Appended,
    /// 本地已有（幂等跳过）。
    AlreadyPresent,
    /// 本地已有但字节不同（保留本地，计入缺失）。
    KeptLocal,
}

// ===== 阶段主体 =====

/// 拉取方：向对端请求缺失的 blob 并落库（双向「各拉各缺」的一侧）。
///
/// 调用时机：`Done` 之后、blob 协商通过（[`blob_phase_enabled`]）。无论 want 是否
/// 为空都必须发 Pull（对端按收到的 Pull 决定是否供片，空清单 = 立即 End），
/// 保证两侧阶段次序确定；返回 [`BlobPhaseReport`]。
///
/// **O2 三段式**：锁内段①取缺失清单/容量快照，网络段收片落 spool（不需要 repo），
/// 锁内段②批量 finalize。容量预检用锁内段①的文件大小**快照**——交换中途本端库
/// 只会被本次交换自己改变（同一时刻仅一场同步），偏差以字节计、可忽略。
pub fn pull_missing(
    stream: &mut std::net::TcpStream,
    sync_key: &[u8],
    session_id: &[u8],
    replay: &mut ReplayGuard,
    access: &mut dyn RepoAccess,
    next_seq: &mut u64,
    now: i64,
) -> CoreResult<BlobPhaseReport> {
    // 锁内段①：孤儿 spool 清理（条目中途被删的场景；§3.12.6）+ 缺失清单 + 容量快照。
    let mut prep: Option<(Vec<String>, PathBuf, u64)> = None;
    access.with_repo(Box::new(|repo| {
        cleanup_orphan_spools(repo)?;
        let mut want: Vec<String> = repo
            .attachment_blob_refs()
            .into_iter()
            .filter(|r| !repo.has_blob(r))
            .collect();
        // 排序保证确定性。
        want.sort();
        let size = repo.file_size()?;
        prep = Some((want, Spool::root(&repo.path()), size));
        Ok(())
    }))?;
    let (want, spool_root, vault_size) = prep.ok_or(CoreError::SyncAborted)?;
    send_blob(
        stream,
        sync_key,
        session_id,
        next_seq,
        now,
        &BlobMessage::Pull { want },
    )?;

    let mut report = BlobPhaseReport::default();
    let peer_missing: Vec<String>;
    // 当前在接收的 blob（分片按 ref 串行到达：Meta ref₁ → Data ref₁ ×n → Meta ref₂ → …）。
    // O2：收齐的 blob 暂存到 `finished`（spool 在盘上，不占内存），End 后批量 finalize。
    let mut finished: Vec<(BlobMeta, Spool)> = Vec::new();
    let mut current: Option<(BlobMeta, Spool)> = None;
    // 容量拒绝时剩余需消费丢弃的 Data 信封数（保持流同步，不落盘）。
    let mut skip_remaining: u32 = 0;

    loop {
        let msg = recv_blob(stream, sync_key, session_id, replay, now)?;
        if matches!(msg, BlobMessage::Meta { .. }) {
            // 收到新 Meta：上一轮必然已收齐，先入队等待批量 finalize。
            if let Some(pair) = current.take() {
                finished.push(pair);
            }
            // 解析并校验本次 Meta；容量预检不过 → 拒绝（不落半个分片，§3.12.6）。
            let meta = BlobMeta::from_message(msg)?;
            if !capacity_ok(vault_size, meta.plaintext_len) {
                skip_remaining = meta.chunk_total;
                report.missing.push(meta.blob_ref.clone());
                current = None;
            } else {
                let spool = Spool::open_or_create(&spool_root, &meta)?;
                current = Some((meta, spool));
            }
            continue;
        }
        match msg {
            BlobMessage::Data {
                blob_ref,
                index,
                ct_slice,
            } => {
                if skip_remaining > 0 {
                    // 容量拒绝的 ref：消费并丢弃，保持两侧报文序对齐。
                    skip_remaining = skip_remaining.saturating_sub(1);
                    continue;
                }
                let (_, spool) = current
                    .as_mut()
                    .filter(|(m, _)| m.blob_ref == blob_ref)
                    .ok_or_else(|| CoreError::Validation("blob_chunk_orphan".to_string()))?;
                spool.write_chunk(index, &ct_slice)?;
            }
            BlobMessage::End {
                completed: _,
                missing,
            } => {
                peer_missing = missing;
                break;
            }
            BlobMessage::Pull { .. } | BlobMessage::Meta { .. } => {
                return Err(CoreError::Validation("blob_phase_order".to_string()));
            }
        }
    }
    // 结算最后一轮。
    if let Some(pair) = current.take() {
        finished.push(pair);
    }

    // 锁内段②：批量 finalize（组帧 → tag 验证 → 入库，幂等不覆盖本地）。
    access.with_repo(Box::new(|repo| {
        for (meta, spool) in finished.drain(..) {
            let blob_ref = meta.blob_ref.clone();
            match finalize_blob(repo, meta, spool)? {
                FinalizeResult::Appended => {
                    report.completed.push(blob_ref);
                    report.appended += 1;
                }
                FinalizeResult::AlreadyPresent => report.completed.push(blob_ref),
                FinalizeResult::KeptLocal => report.missing.push(blob_ref),
            }
        }
        Ok(())
    }))?;
    // 对端无法提供的 ref（本地必然缺失）。
    for r in peer_missing {
        if !report.completed.contains(&r) && !report.missing.contains(&r) {
            report.missing.push(r);
        }
    }
    Ok(report)
}

/// 供片方单批发送的字节预算（O2：批量克隆帧出锁后发送，限制内存峰值）。
const SERVE_BATCH_BYTES: usize = 32 * 1024 * 1024;

/// 供片方：收对端 Pull，逐 ref 发 Meta + Data×n，收尾 End（能供/不能供分列）。
///
/// 返回实际供出的 blob 数（日志用）。对端容量不足 / 组帧失败由对端自担——
/// 供片方只保证「能供的必发全」。
///
/// **O2 三段式**：锁内段只做「读一批 blob 帧出锁」（按 [`SERVE_BATCH_BYTES`]
/// 分批，限制内存峰值），网络发送全程不持锁。
pub fn serve_pull(
    stream: &mut std::net::TcpStream,
    sync_key: &[u8],
    session_id: &[u8],
    replay: &mut ReplayGuard,
    access: &mut dyn RepoAccess,
    next_seq: &mut u64,
    now: i64,
) -> CoreResult<usize> {
    let msg = recv_blob(stream, sync_key, session_id, replay, now)?;
    let want = match msg {
        BlobMessage::Pull { want } => want,
        _ => return Err(CoreError::Validation("expected_pull".to_string())),
    };
    let mut served: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut queue: std::collections::VecDeque<String> = want.into();
    while !queue.is_empty() {
        // 锁内段：读下一批帧出锁（首条即便超预算也带走，避免死循环）。
        let mut batch: Vec<(String, Frame)> = Vec::new();
        let mut batch_bytes = 0usize;
        access.with_repo(Box::new(|repo| {
            while let Some(r) = queue.front().cloned() {
                match repo.blob_frame(&r) {
                    Some(frame) => {
                        if batch_bytes > 0 && batch_bytes + frame.ct.len() > SERVE_BATCH_BYTES {
                            break;
                        }
                        batch_bytes += frame.ct.len();
                        batch.push((r.clone(), frame));
                    }
                    None => missing.push(r.clone()),
                }
                queue.pop_front();
            }
            Ok(())
        }))?;
        // 网络段：发送本批（不持锁）。
        for (r, frame) in &batch {
            send_blob(
                stream,
                sync_key,
                session_id,
                next_seq,
                now,
                &meta_of(r, frame),
            )?;
            for (i, slice) in frame.ct.chunks(SYNC_BLOB_CHUNK_BYTES).enumerate() {
                send_blob(
                    stream,
                    sync_key,
                    session_id,
                    next_seq,
                    now,
                    &BlobMessage::Data {
                        blob_ref: r.clone(),
                        index: i as u32,
                        ct_slice: slice.to_vec(),
                    },
                )?;
            }
        }
        served.extend(batch.into_iter().map(|(r, _)| r));
    }
    let served_count = served.len();
    send_blob(
        stream,
        sync_key,
        session_id,
        next_seq,
        now,
        &BlobMessage::End {
            completed: served,
            missing,
        },
    )?;
    Ok(served_count)
}

// ===== D-91（审查 R1）回归单测：blob_ref 路径穿越防护 =====

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造一个分片数学合法的最小 Meta 报文（其余字段校验与本测试无关）。
    fn meta_msg(blob_ref: &str) -> BlobMessage {
        BlobMessage::Meta {
            blob_ref: blob_ref.to_string(),
            item_id: blob_ref.to_string(),
            wrapped_key: vec![0u8; WRAPPED_KEY_CT_LEN],
            wrapped_key_nonce: vec![0u8; AEAD_NONCE_LEN],
            nonce: vec![0u8; AEAD_NONCE_LEN],
            plaintext_len: 0,
            chunk_total: 1,
            chunk_size: 64,
        }
    }

    #[test]
    fn meta_rejects_path_traversal_blob_refs() {
        for evil in [
            "C:\\Users\\victim\\AppData\\Roaming\\evil",
            "..\\..\\..\\Windows\\Temp\\evil",
            "../../../etc/passwd",
            "/abs/path/evil",
            "normal/../escaped",
            "with\\backslash",
            "with/slash",
        ] {
            let err = BlobMeta::from_message(meta_msg(evil)).unwrap_err();
            assert!(
                matches!(err, CoreError::Validation(ref d) if d == "uuid"),
                "blob_ref {evil:?} 必须被 UUID 解析拒绝，实得 {err:?}"
            );
        }
    }

    #[test]
    fn meta_accepts_uuid_and_normalizes_form() {
        let id = uuid::Uuid::new_v4().to_string();
        let meta = BlobMeta::from_message(meta_msg(&id)).unwrap();
        assert_eq!(meta.blob_ref, id);
        // 大写形式（uuid crate 可解析）回写为规范化小写连字符形式。
        let upper = id.to_uppercase();
        let meta = BlobMeta::from_message(meta_msg(&upper)).unwrap();
        assert_eq!(meta.blob_ref, id, "非规范形式必须归一化，杜绝大小写旁路");
    }

    #[test]
    fn spool_rejects_join_escape_even_if_upstream_bypassed() {
        // 纵深防御：直接构造 BlobMeta（绕过 from_message 的 UUID 校验），
        // open_or_create 的 parent 检查必须兜底拦截。
        let root = std::env::temp_dir().join(format!("pms-d91-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let meta = BlobMeta {
            blob_ref: "..\\evil".to_string(),
            wrapped_key: vec![0u8; WRAPPED_KEY_CT_LEN],
            wrapped_key_nonce: vec![0u8; AEAD_NONCE_LEN],
            nonce: [0u8; AEAD_NONCE_LEN],
            plaintext_len: 0,
            chunk_total: 1,
            chunk_size: 64,
        };
        let err = match Spool::open_or_create(&root, &meta) {
            Err(e) => e,
            Ok(_) => panic!("穿越型 blob_ref 必须被 spool 纵深校验拒绝"),
        };
        assert!(matches!(err, CoreError::Validation(ref d) if d == "blob_ref_path"));
        // 根目录未被写穿（evil 目录不存在于 root 之外）。
        assert!(!root.parent().unwrap().join("evil").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spool_accepts_canonical_uuid_dir() {
        let root = std::env::temp_dir().join(format!("pms-d91-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let meta = BlobMeta {
            blob_ref: uuid::Uuid::new_v4().to_string(),
            wrapped_key: vec![0u8; WRAPPED_KEY_CT_LEN],
            wrapped_key_nonce: vec![0u8; AEAD_NONCE_LEN],
            nonce: [0u8; AEAD_NONCE_LEN],
            plaintext_len: 0,
            chunk_total: 1,
            chunk_size: 64,
        };
        let spool = Spool::open_or_create(&root, &meta).unwrap();
        assert_eq!(spool.dir.parent(), Some(root.as_path()));
        spool.discard();
        let _ = std::fs::remove_dir_all(&root);
    }
}
