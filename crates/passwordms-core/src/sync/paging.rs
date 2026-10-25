//! O1：同步载荷分页与帧预算（协议 `cursor` 字段启用）。
//!
//! 背景（审查 🟠-1）：`SYNC_MAX_FRAME_BYTES ≈ 1.06MB` 单帧上限 vs 200MB 库容量——
//! 旧实现把**全部**待发变更塞进单个 Push 帧，库一大即 `frame_too_large`，且因每轮
//! 全量重发、base 无法推进，同步**永久失败**。
//!
//! 方案（发送方驱动、接收方零改动语义）：
//! - `paginate` 按帧预算把确定性排序的变更清单切块；
//! - 每页一个 Push 信封（seq 单调递增），`cursor` = 本页之后剩余条数（0 = 最后一页）；
//! - 接收方循环读到 `cursor == 0` 为止，聚合后一次 merge（页面间记录互不重叠，
//!   等价旧版单帧语义；老对端单页 Push 的 cursor 恒 0 → 一帧即出，天然兼容）。
//!
//! 兼容性口径：新版↔老版在小库（单页）下互通；大库在旧版本就必然失败
//! （`frame_too_large`），无回归。双端同版发布。

use std::io::Read;

use crate::constants::{SYNC_MAX_RECV_PAGES, SYNC_MAX_RECV_TOTAL_BYTES, SYNC_MAX_FRAME_BYTES};
use crate::error::{CoreError, CoreResult};
use crate::sync::protocol::{
    open_envelope, seal_envelope, ChangeRecord, SyncEnvelope, SyncKind, SyncPayload,
};
use crate::sync::replay::ReplayGuard;
use crate::sync::transport;

/// 单页字节预算：`SYNC_MAX_FRAME_BYTES` 再留 96 KiB 安全余量（覆盖信封 JSON
/// 外壳 + 内层 JSON 字段名/结构开销的估算误差）。
pub const PAGE_BUDGET_BYTES: usize = SYNC_MAX_FRAME_BYTES as usize - 96 * 1024;

/// 估算一条 [`ChangeRecord`] 上线后的近似字节数（保守高估）。
///
/// 密文走「内层 b64（×4/3）→ AEAD → 外层 b64（×4/3）」双重膨胀 ≈ 16/9 ≈ 1.78 倍，
/// 按 **2 倍**计；其余固定字段（UUID、rev、时间戳、三个小 b64 字段）给 512 B 余量。
pub fn estimate_record_size(r: &ChangeRecord) -> usize {
    r.ciphertext.len() * 2 + 512
}

/// 将待发变更按帧预算切成页（确定性排序：`(record_type, record_id)`）。
///
/// 至少返回一页（空库 = 一页空 changes，cursor=0，保持「必有一帧 Push」的旧形状）。
/// 单条即超预算的条目直接报 `Validation("change_too_large:<id>")`——正常内容
/// （SecureNote 上限 256 KiB → 估算 ~516 KiB）远到不了这一步；真出现说明有
/// 异常巨型记录，指名道姓比笼统「同步失败」可用。
pub fn paginate(mut records: Vec<ChangeRecord>) -> CoreResult<Vec<Vec<ChangeRecord>>> {
    records.sort_by(|a, b| (a.record_type, &a.record_id).cmp(&(b.record_type, &b.record_id)));
    let mut pages: Vec<Vec<ChangeRecord>> = Vec::new();
    let mut cur: Vec<ChangeRecord> = Vec::new();
    let mut used = 0usize;
    for r in records {
        let size = estimate_record_size(&r);
        if size > PAGE_BUDGET_BYTES {
            return Err(CoreError::Validation(format!(
                "change_too_large:{}",
                r.record_id
            )));
        }
        if used + size > PAGE_BUDGET_BYTES && !cur.is_empty() {
            pages.push(std::mem::take(&mut cur));
            used = 0;
        }
        used += size;
        cur.push(r);
    }
    pages.push(cur);
    Ok(pages)
}

/// 发送一侧的全部 Push 页（≥1 页；`cursor` = 本页之后剩余条数，0 = 最后一页）。
///
/// `next_seq` 为「下一帧可用序号」（先用后增），返回后指向最后一个已用序号 + 1。
/// 每页都通告 `blob_support` 与 `conflicts`（对端逐帧读，取最后一帧语义即可）。
pub fn send_push_pages<W: std::io::Write>(
    stream: &mut W,
    sync_key: &[u8],
    session_id: &[u8],
    next_seq: &mut u64,
    now: i64,
    device_id: &str,
    pages: &[Vec<ChangeRecord>],
    conflicts: Vec<String>,
) -> CoreResult<()> {
    let total: usize = pages.iter().map(Vec::len).sum();
    let mut sent = 0usize;
    for page in pages {
        sent += page.len();
        let payload = SyncPayload {
            kind: SyncKind::Push,
            device_id: device_id.to_string(),
            cursor: (total - sent) as u64,
            changes: page.clone(),
            conflicts: conflicts.clone(),
            blob_support: true,
        };
        let env = seal_envelope(sync_key, session_id, *next_seq, now, &payload)?;
        transport::send_frame(stream, &serde_json::to_vec(&env)?)?;
        *next_seq += 1;
    }
    Ok(())
}

/// 接收对端的全部 Push 页（读到 `cursor == 0` 为止），返回聚合后的变更清单。
///
/// 每帧都过 [`ReplayGuard`]；`blob_support` 取最后一帧。老对端单页 Push 的
/// cursor 恒 0 → 一帧即出，与旧版行为逐位一致。聚合后由调用方一次性 merge
/// （页面间记录互不重叠，等价旧版单帧 merge；中途失败则什么都不落库）。
///
/// 【D-96】接收侧资源夹取（协议兼容：正常分页远低于阈值）：
/// - 累计接收的变更估算字节（`estimate_record_size` 口径）≤ [`SYNC_MAX_RECV_TOTAL_BYTES`]；
/// - 页数 ≤ [`SYNC_MAX_RECV_PAGES`]。
/// 超限报 `Validation("sync_recv_limit_exceeded: pages=…, bytes=…")`（`ERR_VALIDATION`）
/// ——cursor 由对端驱动，恶意/失控对端可以无限发页拖死接收端（内存与循环时间），
/// 这里在聚合缓冲处一刀拦截。
pub fn recv_push_pages<R: Read>(
    stream: &mut R,
    sync_key: &[u8],
    session_id: &[u8],
    replay: &mut ReplayGuard,
    now: i64,
) -> CoreResult<(Vec<ChangeRecord>, bool)> {
    recv_push_pages_with_limits(stream, sync_key, session_id, replay, now, SYNC_MAX_RECV_TOTAL_BYTES, SYNC_MAX_RECV_PAGES)
}

/// [`recv_push_pages`] 的可测内核：上限由调用方注入（生产传 [`SYNC_MAX_RECV_TOTAL_BYTES`]
/// / [`SYNC_MAX_RECV_PAGES`]，测试传极小值验证拒绝路径，不必真传 200 MB）。
fn recv_push_pages_with_limits<R: Read>(
    stream: &mut R,
    sync_key: &[u8],
    session_id: &[u8],
    replay: &mut ReplayGuard,
    now: i64,
    max_total_bytes: usize,
    max_pages: usize,
) -> CoreResult<(Vec<ChangeRecord>, bool)> {
    let mut all: Vec<ChangeRecord> = Vec::new();
    let mut blob_support;
    let mut total_bytes = 0usize;
    let mut page_count = 0usize;
    loop {
        let bytes = transport::recv_frame(stream)?;
        let env: SyncEnvelope = serde_json::from_slice(&bytes)?;
        replay.accept(env.seq, env.ts, now)?;
        let payload = open_envelope(sync_key, session_id, &env)?;
        blob_support = payload.blob_support;
        page_count += 1;
        total_bytes += payload.changes.iter().map(estimate_record_size).sum::<usize>();
        if total_bytes > max_total_bytes || page_count > max_pages {
            return Err(CoreError::Validation(format!(
                "sync_recv_limit_exceeded: pages={page_count}, bytes={total_bytes}"
            )));
        }
        all.extend(payload.changes);
        if payload.cursor == 0 {
            return Ok((all, blob_support));
        }
    }
}

/// 接收并打开对端的 Done 帧（单帧，过 ReplayGuard）。
///
/// 返回完整载荷（调用方主要消费 `blob_support` 做 D-53 阶段判定）。
pub fn recv_done<R: Read>(
    stream: &mut R,
    sync_key: &[u8],
    session_id: &[u8],
    replay: &mut ReplayGuard,
    now: i64,
) -> CoreResult<SyncPayload> {
    let bytes = transport::recv_frame(stream)?;
    let env: SyncEnvelope = serde_json::from_slice(&bytes)?;
    replay.accept(env.seq, env.ts, now)?;
    open_envelope(sync_key, session_id, &env)
}

/// O1 协议层单测：预算切块、单条超限指名报错、cursor 编解码与老报文兼容。
#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: &str, ct_len: usize) -> ChangeRecord {
        ChangeRecord {
            record_type: crate::constants::RECORD_TYPE_ITEM,
            record_id: id.to_string(),
            rev: 1,
            updated_at: 0,
            modified_by: String::new(),
            deleted: false,
            wrapped_key: vec![0u8; 48],
            wrapped_key_nonce: vec![0u8; 24],
            nonce: vec![0u8; 24],
            plaintext_len: ct_len as u32,
            ciphertext: vec![0u8; ct_len],
        }
    }

    #[test]
    fn empty_list_yields_single_empty_page() {
        let pages = paginate(Vec::new()).unwrap();
        assert_eq!(pages.len(), 1);
        assert!(pages[0].is_empty());
    }

    #[test]
    fn chunks_by_budget_with_deterministic_order() {
        // 每条估算 2KB；预算 ~1MB → 约每页 500 条。
        let records: Vec<ChangeRecord> = (0..1500)
            .map(|i| rec(&format!("{:032x}", i), 256))
            .collect();
        let pages = paginate(records).unwrap();
        assert!(pages.len() >= 2, "1500 条至少切 2 页，实际 {}", pages.len());
        // 页内不超预算。
        for page in &pages {
            let used: usize = page.iter().map(estimate_record_size).sum();
            assert!(used <= PAGE_BUDGET_BYTES);
        }
        // 确定性排序：跨页记录 ID 单调不减。
        let flat: Vec<&str> = pages.iter().flatten().map(|r| r.record_id.as_str()).collect();
        let mut sorted = flat.clone();
        sorted.sort();
        assert_eq!(flat, sorted);
        // 无丢失无重复。
        assert_eq!(flat.len(), 1500);
    }

    #[test]
    fn oversized_single_record_rejected_with_id() {
        let err = paginate(vec![rec("big-one", PAGE_BUDGET_BYTES)]).unwrap_err();
        match err {
            CoreError::Validation(d) => assert!(d.contains("change_too_large:big-one")),
            other => panic!("期望 Validation，实际 {:?}", other),
        }
    }

    #[test]
    fn legacy_payload_cursor_zero_is_single_page() {
        // 老端 SyncPayload（无 cursor 字段）→ cursor=0 → 接收循环一帧即出。
        let legacy = r#"{"kind":"push","device_id":"dev","changes":[],"conflicts":[]}"#;
        let payload: SyncPayload = serde_json::from_str(legacy).unwrap();
        assert_eq!(payload.cursor, 0);
    }

    fn rng_key() -> [u8; 32] {
        crate::crypto::rng::random_key().unwrap()
    }

    /// 构造「发端写满 cursor 分页 Push」的内存流，供接收侧测试回放。
    fn feed_pages(
        sync_key: &[u8],
        session_id: &[u8],
        pages: &[Vec<ChangeRecord>],
        next_seq: &mut u64,
    ) -> std::io::Cursor<Vec<u8>> {
        let mut buf = Vec::new();
        send_push_pages(
            &mut buf,
            sync_key,
            session_id,
            next_seq,
            0,
            "test-dev",
            pages,
            Vec::new(),
        )
        .unwrap();
        std::io::Cursor::new(buf)
    }

    #[test]
    fn recv_push_pages_page_limit_rejects() {
        // 【D-96】页数超限：max_pages=2 时第三页必须被拒，报错信息带页数。
        let sync_key = rng_key();
        let session_id = b"sess-recv-pages".to_vec();
        let mut next_seq = 1u64;
        let pages: Vec<Vec<ChangeRecord>> = (0..3)
            .map(|_| vec![rec("a", 32)])
            .collect();
        let mut stream = feed_pages(&sync_key, &session_id, &pages, &mut next_seq);
        let mut replay = crate::sync::replay::ReplayGuard::new(120);
        let err = recv_push_pages_with_limits(&mut stream, &sync_key, &session_id, &mut replay, 0, 2_000_000_000, 2)
            .unwrap_err();
        match err {
            CoreError::Validation(d) => {
                assert!(d.contains("sync_recv_limit_exceeded"), "实得 {d}");
                assert!(d.contains("pages=3"), "报错应带实际页数，实得 {d}");
            }
            other => panic!("期望 Validation，实际 {other:?}"),
        }
    }

    #[test]
    fn recv_push_pages_byte_limit_rejects() {
        // 【D-96】累计字节超限：max_bytes 设为 0（任何带记录的页都超）。
        let sync_key = rng_key();
        let session_id = b"sess-recv-bytes".to_vec();
        let mut next_seq = 1u64;
        let pages = vec![vec![rec("big", 256)]];
        let mut stream = feed_pages(&sync_key, &session_id, &pages, &mut next_seq);
        let mut replay = crate::sync::replay::ReplayGuard::new(120);
        let err = recv_push_pages_with_limits(&mut stream, &sync_key, &session_id, &mut replay, 0, 0, 4096)
            .unwrap_err();
        match err {
            CoreError::Validation(d) => {
                assert!(d.contains("sync_recv_limit_exceeded"), "实得 {d}");
                assert!(d.contains("bytes="), "报错应带累计字节，实得 {d}");
            }
            other => panic!("期望 Validation，实际 {other:?}"),
        }
    }

    #[test]
    fn recv_push_pages_normal_flow_within_limits() {
        // 对照组：限额充足时分页收发语义不变（3 页全收、停在 cursor=0）。
        let sync_key = rng_key();
        let session_id = b"sess-recv-ok".to_vec();
        let mut next_seq = 1u64;
        let pages: Vec<Vec<ChangeRecord>> = (0..3)
            .map(|i| vec![rec(&format!("{i}"), 32), rec(&format!("{i}-b"), 32)])
            .collect();
        let mut stream = feed_pages(&sync_key, &session_id, &pages, &mut next_seq);
        let mut replay = crate::sync::replay::ReplayGuard::new(120);
        let (all, blob_support) =
            recv_push_pages_with_limits(&mut stream, &sync_key, &session_id, &mut replay, 0, 2_000_000_000, 4096)
                .unwrap();
        assert_eq!(all.len(), 6);
        assert!(blob_support);
    }
}
