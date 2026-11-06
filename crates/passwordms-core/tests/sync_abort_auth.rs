//! #22 数据阶段「中止帧」会话认证回归套件（2026-11-06，裁定路线 B）。
//!
//! 新发送方恒发「带认证字段」的中止帧；新接收方恒 Strict（不接受裸帧）。
//! 本套件把帧格式、MAC 输入域、验证顺序、跨版本兼容与「空 changes 语义」全部钉住，
//! 并为变异验证提供会红的观测点（见 `target/` 下的临时 harness，不进库）。
//!
//! 两端 `AbortCtx` 互为镜像：`sender_ctx` = 发送方（local=S, peer=R），
//! `receiver_ctx` = 接收方（local=R, peer=S）。方向写死，写反两端会算出不同 MAC。

use std::io::Cursor;

use base64::Engine as _;
use passwordms_core::constants::{
    HKDF_INFO_SYNC_ABORT, HKDF_INFO_SYNC_CONFIRM, HKDF_INFO_SYNC_KEY, HKDF_INFO_SYNC_SESSION,
    KEY_LEN, SYNC_TIME_WINDOW_SECS,
};
use passwordms_core::crypto::kdf;
use passwordms_core::error::{codes, CoreError};
use passwordms_core::sync::paging;
use passwordms_core::sync::protocol::{
    self, AbortCtx, HandshakeMessage, Reject, SyncEnvelope, SyncKind, SyncPayload,
};
use passwordms_core::sync::replay::ReplayGuard;
use passwordms_core::sync::resync;
use serde::Deserialize;

const CODE: &str = codes::ERR_VAULT_KEY_MISMATCH;
/// 毫秒时间戳（`ReplayGuard::check_ts` 用秒×1000 的窗口）。
const NOW: i64 = 1_700_000_000_000;
const SEQ: u64 = 7;

fn sync_key() -> [u8; 32] {
    [0x42u8; 32]
}

fn session_id() -> Vec<u8> {
    b"sess-abort-auth-2026".to_vec()
}

/// 发送方（中止方）上下文：sender_id=local="S"，receiver_id=peer="R"。
fn sender_ctx() -> AbortCtx {
    AbortCtx {
        local_id: "S".to_string(),
        peer_id: "R".to_string(),
    }
}

/// 接收方上下文（与发送方镜像）：sender_id=peer="S"，receiver_id=local="R"。
fn receiver_ctx() -> AbortCtx {
    AbortCtx {
        local_id: "R".to_string(),
        peer_id: "S".to_string(),
    }
}

fn b64dec(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}

fn b64enc(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// 长度前缀帧（`u32 LE` 长度 + 帧体），与 `transport::send_frame` 一致。
fn framed(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// 合法「带认证字段」中止帧（每次调用 nonce 不同，MAC 随帧自洽）。
fn valid_frame() -> Vec<u8> {
    protocol::build_abort_frame(&sync_key(), &session_id(), &sender_ctx(), SEQ, NOW, CODE).unwrap()
}

/// 改一个 JSON 字段（保留其余字段与 mac 原样），用于篡改类用例。
fn with_field(frame: &[u8], key: &str, value: serde_json::Value) -> Vec<u8> {
    let mut v: serde_json::Value = serde_json::from_slice(frame).unwrap();
    v[key] = value;
    serde_json::to_vec(&v).unwrap()
}

fn json_frame(frame: &[u8]) -> serde_json::Value {
    serde_json::from_slice(frame).unwrap()
}

/// 走真实接收链：`recv_push_pages` 先试 `SyncEnvelope`，失败才回落认证入口。
fn recv_via_push_pages(frame: &[u8], sk: &[u8], sid: &[u8], rx: &AbortCtx, now: i64) -> CoreError {
    let mut stream = Cursor::new(framed(frame));
    let mut replay = ReplayGuard::with_state(0, SYNC_TIME_WINDOW_SECS);
    paging::recv_push_pages(&mut stream, sk, sid, &mut replay, now, rx).unwrap_err()
}

/// 走 `recv_done` 接收分支（paging.rs 的第二个 `unwrap_or` 落地处），供 T12 覆盖
/// 「两处都收敛到同一个 SyncAborted」，防任一分支单独被改回 `?` 透传而漏网。
fn recv_via_done(frame: &[u8], sk: &[u8], sid: &[u8], rx: &AbortCtx, now: i64) -> CoreError {
    let mut stream = Cursor::new(framed(frame));
    let mut replay = ReplayGuard::with_state(0, SYNC_TIME_WINDOW_SECS);
    paging::recv_done(&mut stream, sk, sid, &mut replay, now, rx).unwrap_err()
}

fn is_aborted(err: &CoreError) -> bool {
    matches!(err, CoreError::SyncAborted)
        && err.code() == codes::ERR_SYNC_ABORTED
        && err.message_key() == "err.sync_aborted"
        && err.code() != codes::ERR_INTERNAL
}

// ===== T1–T11：帧格式 / MAC 域 / 验证顺序 / 兼容 =====

/// T1：发送侧构造 → 接收侧验证，round-trip 得到具体原因 `VaultKeyMismatch`。
#[test]
fn t1_round_trip_maps_to_vault_key_mismatch() {
    let frame = valid_frame();
    let replay = ReplayGuard::with_state(0, SYNC_TIME_WINDOW_SECS);
    // 低层认证入口。
    assert!(matches!(
        protocol::verify_abort_frame(
            &frame,
            &sync_key(),
            &session_id(),
            &receiver_ctx(),
            &replay,
            NOW,
        ),
        Some(CoreError::VaultKeyMismatch)
    ));
    // 真实接收链。
    let err = recv_via_push_pages(&frame, &sync_key(), &session_id(), &receiver_ctx(), NOW);
    assert!(matches!(err, CoreError::VaultKeyMismatch), "实际 {err:?}");
    assert_eq!(err.code(), codes::ERR_VAULT_KEY_MISMATCH);
}

/// T2：换 `sync_key` → `SyncAborted`，且 raw code 不出现在结果/日志、绝不落 Internal。
#[test]
fn t2_wrong_sync_key_is_sync_aborted_without_raw_code() {
    let mut other = sync_key();
    other[0] ^= 0xff;
    let err = recv_via_push_pages(&valid_frame(), &other, &session_id(), &receiver_ctx(), NOW);
    assert!(is_aborted(&err), "实际 {err:?}");
    let rendered = format!("{err:?}");
    assert!(
        !rendered.contains(CODE),
        "结果/日志不得回显 raw code：{rendered}"
    );
    assert!(!err.code().contains("VAULT_KEY_MISMATCH"));
    assert_eq!(err.message_key(), "err.sync_aborted");
}

/// T3：跨会话复制（帧在会话 A 生成，拿到会话 B 验证）→ `SyncAborted`。
/// MAC 输入域用**本地** session_id 重算，换会话必失败。
#[test]
fn t3_cross_session_copy_is_rejected() {
    let other_sid = b"another-session-id".to_vec();
    let err = recv_via_push_pages(
        &valid_frame(),
        &sync_key(),
        &other_sid,
        &receiver_ctx(),
        NOW,
    );
    assert!(is_aborted(&err), "实际 {err:?}");
}

/// T4：只改 code 保留 mac → `SyncAborted`，且绝不返回被改成的安全语义码。
#[test]
fn t4_code_tamper_is_rejected_and_never_returns_tampered_code() {
    let tampered = with_field(
        &valid_frame(),
        "code",
        serde_json::json!(codes::ERR_PAIR_TOKEN_USED),
    );
    let err = recv_via_push_pages(&tampered, &sync_key(), &session_id(), &receiver_ctx(), NOW);
    assert!(is_aborted(&err), "实际 {err:?}");
    assert_ne!(err.code(), codes::ERR_PAIR_TOKEN_USED);
    assert_ne!(err.code(), codes::ERR_VAULT_KEY_MISMATCH);
}

/// T5：mac 变异 / 截断 / 长度不等 → 一律 `SyncAborted`（常量时间比对，长度不等即 false）。
#[test]
fn t5_mac_mutation_truncation_and_length_are_rejected() {
    let frame = valid_frame();
    let mac = b64dec(json_frame(&frame)["mac"].as_str().unwrap());
    assert_eq!(mac.len(), 32);

    let mut flipped = mac.clone();
    flipped[0] ^= 1;
    let cases = vec![
        ("bit-flip", b64enc(&flipped)),
        ("truncated-16", b64enc(&mac[..16])),
        ("extended-33", b64enc(&[mac.as_slice(), &[0u8]].concat())),
    ];
    for (tag, value) in cases {
        let f = with_field(&frame, "mac", serde_json::json!(value));
        let err = recv_via_push_pages(&f, &sync_key(), &session_id(), &receiver_ctx(), NOW);
        assert!(is_aborted(&err), "{tag} 实际 {err:?}");
    }
}

/// T6：ts 越窗（±121s，窗口 = 120s×1000ms）→ `SyncAborted`。
/// ts 进 MAC 且值一致，故 MAC 先过、随后 `check_ts` 才拒——钉住「先验 MAC 再产生行为」。
#[test]
fn t6_ts_out_of_window_is_rejected() {
    for delta in [-121_000i64, 121_000i64] {
        let frame = protocol::build_abort_frame(
            &sync_key(),
            &session_id(),
            &sender_ctx(),
            SEQ,
            NOW + delta,
            CODE,
        )
        .unwrap();
        let err = recv_via_push_pages(&frame, &sync_key(), &session_id(), &receiver_ctx(), NOW);
        assert!(is_aborted(&err), "delta={delta} 实际 {err:?}");
    }
}

/// T7：seq / nonce / sender_id / receiver_id 任一变异 → `SyncAborted`（全部进 MAC 域）。
#[test]
fn t7_seq_nonce_and_direction_tampering_are_rejected() {
    let frame = valid_frame();
    let sk = sync_key();
    let sid = session_id();

    // seq 变异。
    let f = with_field(&frame, "seq", serde_json::json!(999u64));
    assert!(is_aborted(&recv_via_push_pages(
        &f,
        &sk,
        &sid,
        &receiver_ctx(),
        NOW
    )));

    // nonce 变异。
    let nonce = b64dec(json_frame(&frame)["nonce"].as_str().unwrap());
    assert_eq!(nonce.len(), 16);
    let mut flipped = nonce.clone();
    flipped[0] ^= 1;
    let f = with_field(&frame, "nonce", serde_json::json!(b64enc(&flipped)));
    assert!(is_aborted(&recv_via_push_pages(
        &f,
        &sk,
        &sid,
        &receiver_ctx(),
        NOW
    )));

    // sender_id 变异：用不同发送方上下文生成帧（MAC 用错的 sender 算）。
    let bad_sender = AbortCtx {
        local_id: "EVIL".to_string(),
        peer_id: "R".to_string(),
    };
    let f = protocol::build_abort_frame(&sk, &sid, &bad_sender, SEQ, NOW, CODE).unwrap();
    assert!(is_aborted(&recv_via_push_pages(
        &f,
        &sk,
        &sid,
        &receiver_ctx(),
        NOW
    )));

    // receiver_id 变异：接收侧本地 id 换掉（重算 MAC 的 receiver 变了）。
    let bad_receiver = AbortCtx {
        local_id: "EVIL".to_string(),
        peer_id: "S".to_string(),
    };
    assert!(is_aborted(&recv_via_push_pages(
        &frame,
        &sk,
        &sid,
        &bad_receiver,
        NOW
    )));
}

/// T8：正常信封不被误判（同步成功）；认证中止帧也绝不会被认成 `SyncEnvelope`。
#[test]
fn t8_normal_envelope_ok_and_abort_frame_is_not_envelope() {
    let sk = sync_key();
    let sid = session_id();
    let payload = SyncPayload {
        kind: SyncKind::Push,
        device_id: "peer".to_string(),
        cursor: 0,
        changes: Vec::new(),
        conflicts: Vec::new(),
        blob_support: false,
        pair_only: false,
    };
    let env = protocol::seal_envelope(&sk, &sid, 1, NOW, &payload).unwrap();
    let envelope_bytes = serde_json::to_vec(&env).unwrap();
    // 正常信封能被认成信封。
    assert!(serde_json::from_slice::<SyncEnvelope>(&envelope_bytes).is_ok());
    let mut stream = Cursor::new(framed(&envelope_bytes));
    let mut replay = ReplayGuard::with_state(0, SYNC_TIME_WINDOW_SECS);
    assert!(
        paging::recv_push_pages(&mut stream, &sk, &sid, &mut replay, NOW, &receiver_ctx()).is_ok()
    );

    // 带认证的中止帧缺 session_id / ct，绝不会被认成信封。
    assert!(
        serde_json::from_slice::<SyncEnvelope>(&valid_frame()).is_err(),
        "中止帧不得被 from_slice::<SyncEnvelope> 认成信封"
    );
}

/// T9：握手阶段 REJECT 线形逐字节不变（钉住 `skip_serializing_if`）。
/// `HandshakeMessage`（配对）与 `ResyncMessage`（重同步）两个发送侧都覆盖。
#[test]
fn t9_handshake_reject_wire_bytes_are_unchanged() {
    let expected = br#"{"t":"reject","code":"ERR_VAULT_KEY_MISMATCH"}"#;
    let handshake = serde_json::to_vec(&HandshakeMessage::Reject(Reject::handshake(CODE))).unwrap();
    assert_eq!(handshake, expected);
    let resync =
        serde_json::to_vec(&resync::ResyncMessage::Reject(Reject::handshake(CODE))).unwrap();
    assert_eq!(resync, expected);
}

/// T10：新版恒 Strict，收到 legacy 裸帧 `{"t":"reject","code":…}` → `SyncAborted`
/// （old→new 的单向退化，是已接受的代价）。
#[test]
fn t10_strict_rejects_legacy_naked_frame() {
    let naked = br#"{"t":"reject","code":"ERR_VAULT_KEY_MISMATCH"}"#;
    let err = recv_via_push_pages(naked, &sync_key(), &session_id(), &receiver_ctx(), NOW);
    assert!(is_aborted(&err), "实际 {err:?}");
}

/// 模拟 1.6.3 的「旧形状枚举」：只有 `Reject { code }`，且不加 `deny_unknown_fields`。
#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
enum LegacyHandshake {
    Reject { code: String },
}

/// T11：兼容性钉子——旧形状枚举反序列化带 v/sid/seq/ts/nonce/mac 的新帧必须成功且
/// code 正确（把「serde 忽略未知字段」从静态推断升级成实测）。
#[test]
fn t11_legacy_shape_enum_ignores_new_fields() {
    let frame = valid_frame();
    let parsed: LegacyHandshake = serde_json::from_slice(&frame).unwrap();
    match parsed {
        LegacyHandshake::Reject { code } => assert_eq!(code, CODE),
    }
}

// ===== 本轮补充：M10 错误响应不可区分 / 帧内 sid 一致性 / M2 红锚 =====

/// T12：失败响应**不可区分**（M10）。覆盖：非 JSON 垃圾字节 / 合法 JSON 但非 Reject
/// 形状 / reject 形状但字段不全 / 带认证字段但 mac 长度错 / mac 不符 / mac 正确但
/// sync_key 错 / ts 越窗且 MAC 合法。全部 `is_aborted()`、`code()==ERR_SYNC_ABORTED`
/// 且 `!= ERR_INTERNAL`；并额外断言 `Debug` 渲染**两两相等**（比各自 `is_aborted()`
/// 更强）。前几类走 `recv_push_pages`，畸形帧再补喂 `recv_done`，覆盖两个
/// `unwrap_or` 落地处：**任一处**把 `unwrap_or(SyncAborted)` 改回 `?` 透传，
/// 畸形帧落成 `Internal("json")`、与其余类可区分，本用例立刻红。
#[test]
fn t12_three_failure_classes_are_indistinguishable() {
    let sk = sync_key();
    let sid = session_id();
    let rx = receiver_ctx();

    // ① 非 JSON 垃圾字节（连 serde 都解析不了）。
    let garbage = b"\xff\xfe\x00not-json";
    // ② 合法 JSON 但非 Reject 形状（既不是 SyncEnvelope 也不是 reject → 入口返回 None）。
    let malformed = br#"{"garbage":123}"#;
    // ③ reject 形状但字段不全（裸帧）。
    let incomplete = br#"{"t":"reject","code":"ERR_VAULT_KEY_MISMATCH"}"#;
    // ④ 带认证字段但 mac 长度错（长度闸）。
    let short_mac = {
        let frame = valid_frame();
        with_field(&frame, "mac", serde_json::json!(b64enc(&[0u8; 16])))
    };
    // ⑤ mac 不符（合法帧翻转一个 bit，保留其余字段）。
    let bad_mac = {
        let frame = valid_frame();
        let mac = b64dec(json_frame(&frame)["mac"].as_str().unwrap());
        let mut flipped = mac.clone();
        flipped[0] ^= 1;
        with_field(&frame, "mac", serde_json::json!(b64enc(&flipped)))
    };
    // ⑥ ts 越窗且 MAC 合法（越窗帧 MAC 自洽，故 MAC 先过、check_ts 才拒）。
    let out_of_window =
        protocol::build_abort_frame(&sk, &sid, &sender_ctx(), SEQ, NOW + 121_000, CODE).unwrap();
    // ⑦ mac 正确但 sync_key 错（派生 abort_key 不同 → MAC 不符）。
    let mut wrong_sk = sk;
    wrong_sk[0] ^= 1;

    let mut errs: Vec<CoreError> = vec![
        recv_via_push_pages(garbage, &sk, &sid, &rx, NOW),
        recv_via_push_pages(malformed, &sk, &sid, &rx, NOW),
        recv_via_push_pages(incomplete, &sk, &sid, &rx, NOW),
        recv_via_push_pages(&short_mac, &sk, &sid, &rx, NOW),
        recv_via_push_pages(&bad_mac, &sk, &sid, &rx, NOW),
        recv_via_push_pages(&out_of_window, &sk, &sid, &rx, NOW),
        recv_via_push_pages(&valid_frame(), &wrong_sk, &sid, &rx, NOW),
    ];
    // 畸形帧再补喂 `recv_done`（第二个 `unwrap_or` 落地处），使「两处任一被改回
    // ? 透传」都会红。
    errs.push(recv_via_done(malformed, &sk, &sid, &rx, NOW));

    for (i, err) in errs.iter().enumerate() {
        assert!(is_aborted(err), "第 {i} 类失败实际 {err:?}");
        assert_eq!(err.code(), codes::ERR_SYNC_ABORTED, "第 {i} 类 code");
        assert_ne!(err.code(), codes::ERR_INTERNAL, "第 {i} 类不得落 Internal");
        assert_eq!(err.message_key(), "err.sync_aborted", "第 {i} 类文案");
    }
    // 关键：全部 `Debug` 渲染两两相等——证明不是各自成立的 `is_aborted()`，
    // 而是「同一错误、同一文案」的不可区分性（与 Auto-Type M4 互为镜像）。
    let rendered: Vec<String> = errs.iter().map(|e| format!("{e:?}")).collect();
    for i in 1..rendered.len() {
        assert_eq!(
            rendered[0], rendered[i],
            "第 0 类与第 {i} 类给出可区分文案：{:?} vs {:?}",
            rendered[0], rendered[i]
        );
    }
}

/// T13：帧内 `sid` 是形状完整性字段——改帧内 `sid`、保留合法 mac，接收侧必须拒绝
/// （诊断一致性检查）。认证仍由本地 `session_id` 进 MAC 单独承担，帧内 `sid` 绝不进
/// MAC 输入域；删掉一致性检查，本用例立刻红。
#[test]
fn t13_frame_sid_mismatch_is_rejected() {
    let frame = valid_frame();
    let other = b64enc(b"attacker-chosen-sid");
    let tampered = with_field(&frame, "sid", serde_json::json!(other));
    let err = recv_via_push_pages(&tampered, &sync_key(), &session_id(), &receiver_ctx(), NOW);
    assert!(is_aborted(&err), "帧内 sid 被篡改却放行：{err:?}");
}

/// T3b / M2 红锚：跨会话复制且把帧内 `sid` 对齐到接收侧本地 `session_id`。
/// 此时一致性检查（形状完整性）放行，剩下的会话绑定只能靠 MAC 输入域里的**本地**
/// `session_id`：帧在会话 A 生成（MAC 绑 A），拿到会话 B 验证（MAC 重算绑 B）
/// → MAC 必不相符 → `SyncAborted`。把 `sid` 从 MAC 输入域拿掉（M2），本用例立刻红
/// （返回具体码而非 `SyncAborted`）。这是新增一致性检查后 M2 的新红锚：原 T3
/// （帧内 sid 未对齐）会被一致性检查独立兜住，单靠它 M2 不再会红。
#[test]
fn t3_cross_session_with_aligned_sid_is_rejected() {
    let frame = valid_frame();
    let other_sid = b"another-session-id".to_vec();
    // 把帧内死字段 sid 对齐到接收侧本地值（一致性检查会放行）。
    let aligned = with_field(&frame, "sid", serde_json::json!(b64enc(&other_sid)));
    let err = recv_via_push_pages(&aligned, &sync_key(), &other_sid, &receiver_ctx(), NOW);
    assert!(is_aborted(&err), "实际 {err:?}");
}

// ===== 域分离 / 守门 / Q3.4 语义判定 =====

/// M8 观测点：`HKDF_INFO_SYNC_ABORT` 与三把会话密钥 info 互不冲突，且同一
/// ikm+salt 换 info 必得不同密钥（绝不让同一把 sync_key 兼做 AEAD 与 HMAC）。
#[test]
fn abort_key_domain_separation() {
    assert_ne!(HKDF_INFO_SYNC_ABORT, HKDF_INFO_SYNC_KEY);
    assert_ne!(HKDF_INFO_SYNC_ABORT, HKDF_INFO_SYNC_CONFIRM);
    assert_ne!(HKDF_INFO_SYNC_ABORT, HKDF_INFO_SYNC_SESSION);
    let sk = sync_key();
    assert_eq!(sk.len(), KEY_LEN);
    let prk = kdf::hkdf_extract(&[], &sk);
    let alt = kdf::hkdf_expand(&prk, HKDF_INFO_SYNC_KEY.as_bytes(), 32).unwrap();
    let abort = passwordms_core::sync::handshake::derive_abort_key(&sk).unwrap();
    assert_ne!(&abort[..], &alt[..], "换 info 必须得到不同密钥");
    // 长度闸：非 32B 一律 CoreError::Crypto。
    assert!(matches!(
        passwordms_core::sync::handshake::derive_abort_key(&[0u8; 31]),
        Err(CoreError::Crypto(_))
    ));
}

/// M1 源码守门：验证入口必须命中 `handshake::verify_mac`（删调用 / 换成 `==` 立刻红）。
#[test]
fn guard_abort_verify_uses_handshake_verify_mac() {
    let src = include_str!("../src/sync/protocol.rs");
    let hits = src.matches("crate::sync::handshake::verify_mac(").count();
    assert!(
        hits >= 1,
        "验证入口命中 handshake::verify_mac ≥ 1，实际 {hits}"
    );
}

/// 守门断言：abort 处理路径不得写入任何持久状态（游标/水位）。
/// 一旦将来触发状态写入，seq 必须改走 `replay.accept`（见 protocol.rs 注释）。
#[test]
fn guard_no_persistent_state_on_abort() {
    let sk = sync_key();
    let sid = session_id();
    let frame = valid_frame();
    // 验证成功帧与验证失败帧都不许推进 last_recv_seq。
    for f in [
        frame.clone(),
        with_field(&frame, "seq", serde_json::json!(999u64)),
    ] {
        let mut stream = Cursor::new(framed(&f));
        let mut replay = ReplayGuard::with_state(0, SYNC_TIME_WINDOW_SECS);
        let before = replay.last_recv_seq();
        let _ = paging::recv_push_pages(&mut stream, &sk, &sid, &mut replay, NOW, &receiver_ctx());
        assert_eq!(
            replay.last_recv_seq(),
            before,
            "abort 路径不得写入任何持久状态；一旦触发状态写入，seq 必须改走 replay.accept"
        );
    }
}

/// M9a + M9c（Q3.4 语义判定）：构造「已有 SyncKind + 空 changes」的入站帧喂进接收链。
/// M9a 断言落在**可观察后果**：返回 Ok（同步成功）+ `last_recv_seq` 真的推进（状态推进），
/// 而不是断言「源码里 cursor 缺省是 0」。
/// M9c 用「本轮实际进入接收函数的帧数 ≥ 1」命中数断言防空转（Cursor 读出字节数）。
#[test]
fn q34_empty_changes_really_processed() {
    let sk = sync_key();
    let sid = session_id();
    let payload = SyncPayload {
        kind: SyncKind::Push,
        device_id: "peer".to_string(),
        cursor: 0,
        changes: Vec::new(),
        conflicts: Vec::new(),
        blob_support: false,
        pair_only: false,
    };
    let env = protocol::seal_envelope(&sk, &sid, 1, NOW, &payload).unwrap();
    let wire = framed(&serde_json::to_vec(&env).unwrap());
    let total = wire.len();
    let mut stream = Cursor::new(wire);
    let mut replay = ReplayGuard::with_state(0, SYNC_TIME_WINDOW_SECS);

    let got = paging::recv_push_pages(&mut stream, &sk, &sid, &mut replay, NOW, &receiver_ctx())
        .expect("空 changes 的合法 Push 帧必须被当成同步成功（Ok）");
    assert!(got.changes.is_empty());
    // M9a：可观察后果——接收链真的处理了这一帧并推进了水位。
    assert_eq!(replay.last_recv_seq(), 1, "状态推进：这一帧真的被处理");
    // M9c：命中数断言——本轮实际进入接收函数的帧数 ≥ 1（至少读走长度前缀+帧体）。
    assert!(
        stream.position() > 4 && stream.position() <= total as u64,
        "本轮实际进入接收函数的帧数 ≥ 1，position={}",
        stream.position()
    );
}
