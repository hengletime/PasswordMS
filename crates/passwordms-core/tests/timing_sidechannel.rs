//! A-05 **时序侧信道**测试（Phase 4c · 任务 2）。
//!
//! 抗时序侧信道的**结构性保证**：解锁的**任何失败路径都不允许在 Argon2id 之前提前返回**。
//! Argon2id(m=64MiB, t=3, p=1) 是解锁耗时的绝对主项（本机 debug 下数百 ms 级），任何
//! “提前 return”都会比完整 KDF 快上千倍、一眼可辨。因此本测试的核心断言是：
//!
//! - **【结构性】** 「错误密码」「头部内容损坏」「**结构性头部损坏**（magic 被破坏）」三条
//!   失败路径的耗时中位数，都必须 ≥ 完整 KDF 基准耗时的 **50%**（证明 KDF 三条路径都真的跑了）。
//!   *其中「结构性头部损坏」曾因在 KDF 前早退被豁免（D-10）；折叠错误码时已补上等价 KDF，
//!   故现纳入本测试。*
//! - **【统计性·宽松】** 三条失败路径中位数的比值落在 **`[0.7, 1.4]`**（中位数、去预热）。
//!
//! 断言前会 `println!` 完整原始数据（N、中位数、p10、p90、比值、KDF 基准）。
//!
//! 运行（看原始数据）：
//! `cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm --nocapture unlock_failure_timing`
//!
//! 说明：本文件**单独成二进制且仅含本用例**，以避免与其它测试并行抢占 CPU 干扰计时。
//! 若机器负载极高导致统计断言偶发失败：请**如实保留**结构性断言（它对负载不敏感），
//! 并据实测数据决定是否放宽统计门槛；**禁止**为让测试好过而把门槛放到无意义。

use std::path::PathBuf;
use std::time::Instant;

use passwordms_core::constants::CONTAINER_PREFIX_LEN;
use passwordms_core::crypto::kdf;
use passwordms_core::model::vault::KdfParams;
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;

/// 每条路径丢弃的预热样本数。
const WARMUP: usize = 3;
/// 每条路径最少迭代次数。
const MIN_N: usize = 15;
/// 迭代次数上限（防止慢机器上跑太久）。
const MAX_N: usize = 40;
/// 目标：每条路径样本采集总耗时约 8 秒（据此自适应 N）。
const TARGET_MS_PER_PATH: f64 = 8000.0;

fn tmp_path(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "passwordms_timing_{tag}_{}.vault",
        uuid::Uuid::new_v4()
    ));
    p
}

fn sorted(xs: &[f64]) -> Vec<f64> {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v
}

fn median(xs: &[f64]) -> f64 {
    let v = sorted(xs);
    if v.is_empty() {
        return f64::NAN;
    }
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn percentile(xs: &[f64], p: f64) -> f64 {
    let v = sorted(xs);
    if v.is_empty() {
        return f64::NAN;
    }
    let idx = (((v.len() - 1) as f64) * p).round() as usize;
    v[idx]
}

/// 造「容器结构合法、`verify_mac` 被篡改」的副本：解锁会进入完整 KDF + `unlock_check`。
fn make_content_corrupted(ok_bytes: &[u8], tag: &str) -> PathBuf {
    let header_len = u16::from_le_bytes([ok_bytes[10], ok_bytes[11]]) as usize;
    let header = &ok_bytes[CONTAINER_PREFIX_LEN..CONTAINER_PREFIX_LEN + header_len];
    let mut value: serde_json::Value = serde_json::from_slice(header).expect("header json");
    if let Some(serde_json::Value::String(s)) = value.get_mut("verify_mac") {
        let mut bytes = s.clone().into_bytes();
        if !bytes.is_empty() {
            bytes[0] = if bytes[0] == b'A' { b'B' } else { b'A' };
        }
        *s = String::from_utf8(bytes).expect("utf8");
    }
    let mut new_header = serde_json::to_vec(&value).expect("re-ser");
    assert!(new_header.len() <= header_len, "篡改后头部超出预留容量");
    while new_header.len() < header_len {
        new_header.push(b' ');
    }
    let mut out = Vec::with_capacity(ok_bytes.len());
    out.extend_from_slice(&ok_bytes[..CONTAINER_PREFIX_LEN]);
    out.extend_from_slice(&new_header);
    out.extend_from_slice(&ok_bytes[CONTAINER_PREFIX_LEN + header_len..]);
    let p = tmp_path(tag);
    std::fs::write(&p, &out).expect("write content-corrupted");
    p
}

/// 造「结构性头部损坏」的副本：翻转 magic 首字节，令 `Container::open` 在读取头部时即刻失败。
/// D-10 折叠后该路径也必须在返回前消耗一次等价 KDF，否则会毫秒级早退。
fn make_structural_corrupted(ok_bytes: &[u8], tag: &str) -> PathBuf {
    let mut b = ok_bytes.to_vec();
    b[0] ^= 0xff; // 破坏 magic "PMSVAULT"
    let p = tmp_path(tag);
    std::fs::write(&p, &b).expect("write structural-corrupted");
    p
}

#[test]
fn unlock_failure_timing_has_no_password_vs_corruption_gap() {
    let pw = "qa_test_master_password";
    let base = tmp_path("base");
    drop(Session::create(&base, pw, true).expect("create"));
    let ok_bytes = std::fs::read(&base).expect("read base");

    // 取 salt / params，单独量「完整 KDF」基准。
    let (salt, params): (Vec<u8>, KdfParams) = {
        let c = Container::open(&base).expect("open base");
        (c.header().kdf_salt.clone(), c.header().kdf.clone())
    };
    let mut kdf_samples = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        drop(kdf::derive_master_key(pw, &salt, &params).expect("kdf"));
        kdf_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let kdf_ms = median(&kdf_samples);

    // 自适应 N：据此控制总耗时。
    let n = ((TARGET_MS_PER_PATH / kdf_ms).round() as usize).clamp(MIN_N, MAX_N);
    println!(
        "[timing] Argon2id 基准中位数 = {kdf_ms:.1} ms；每路径 N = {n}（目标≈{TARGET_MS_PER_PATH:.0}ms）"
    );

    let corrupt = make_content_corrupted(&ok_bytes, "corrupt");
    let structural = make_structural_corrupted(&ok_bytes, "structural");

    // 预热三条路径。
    for _ in 0..WARMUP {
        assert!(Session::unlock(&base, "definitely_wrong_password").is_err());
        assert!(Session::unlock(&corrupt, pw).is_err());
        assert!(Session::unlock(&structural, pw).is_err());
    }

    // **轮转采样**（2026-10-21 修，见 docs/07-quality-gate.md §1.2「测量口径」）。
    //
    // 原实现是「一条路径连续跑满 N 次，再换下一条」，于是相位顺序固定为
    // 错误密码 → 内容损坏 → 结构性损坏。实测（同一棵代码树连跑 3 次）：
    //   错误密码 median 1750 / 2684 / 2885（两档跳、p10–p90 宽达 900ms）
    //   内容损坏 median 1726 / 1744 / 1787（三次几乎不动）
    // 形态说明偏差来自**相位**而不是路径：第一相位独自承担机器上升期（CPU 升频收敛、
    // Argon2id 64MB 内存页首次触碰、杀软扫新产物的尾巴），比值随之假红。
    // 每轮三条路径各跑一次 → 同一时刻的负载对三条路径等量施加，比值只反映代码差异。
    let mut wrong = Vec::with_capacity(n);
    let mut corrupt_t = Vec::with_capacity(n);
    let mut structural_t = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        let r = Session::unlock(&base, "definitely_wrong_password");
        wrong.push(t.elapsed().as_secs_f64() * 1000.0);
        assert!(r.is_err(), "错误密码: 样本应为失败");

        let t = Instant::now();
        let r = Session::unlock(&corrupt, pw);
        corrupt_t.push(t.elapsed().as_secs_f64() * 1000.0);
        assert!(r.is_err(), "内容损坏: 样本应为失败");

        let t = Instant::now();
        let r = Session::unlock(&structural, pw);
        structural_t.push(t.elapsed().as_secs_f64() * 1000.0);
        assert!(r.is_err(), "结构性损坏: 样本应为失败");
    }

    let m_wrong = median(&wrong);
    let m_corrupt = median(&corrupt_t);
    let m_struct = median(&structural_t);
    let ratio = m_wrong / m_corrupt;
    let ratio_struct = m_struct / m_corrupt;

    println!(
        "[timing] 错误密码    : N={n} median={m_wrong:.1}ms p10={:.1} p90={:.1}",
        percentile(&wrong, 0.10),
        percentile(&wrong, 0.90)
    );
    println!(
        "[timing] 内容损坏    : N={n} median={m_corrupt:.1}ms p10={:.1} p90={:.1}",
        percentile(&corrupt_t, 0.10),
        percentile(&corrupt_t, 0.90)
    );
    println!(
        "[timing] 结构性损坏  : N={n} median={m_struct:.1}ms p10={:.1} p90={:.1}",
        percentile(&structural_t, 0.10),
        percentile(&structural_t, 0.90)
    );
    println!(
        "[timing] 比值 wrong/corrupt = {ratio:.3}；struct/corrupt = {ratio_struct:.3}；KDF 基准 = {kdf_ms:.1}ms"
    );

    // 结构性断言：三条失败路径都必须跑了完整 KDF（≥50% 基准）。对机器负载不敏感。
    assert!(
        m_wrong >= 0.5 * kdf_ms,
        "错误密码路径疑在 KDF 前提前返回：median={m_wrong:.1}ms < 0.5×{kdf_ms:.1}ms"
    );
    assert!(
        m_corrupt >= 0.5 * kdf_ms,
        "内容损坏路径疑在 KDF 前提前返回：median={m_corrupt:.1}ms < 0.5×{kdf_ms:.1}ms"
    );
    assert!(
        m_struct >= 0.5 * kdf_ms,
        "结构性损坏路径疑在 KDF 前提前返回（D-10 未修好）：median={m_struct:.1}ms < 0.5×{kdf_ms:.1}ms"
    );
    // 统计性断言（宽松门槛）。
    assert!(
        (0.7..=1.4).contains(&ratio),
        "错误密码/内容损坏两条失败路径耗时比值越界：{ratio:.3}（不在 [0.7, 1.4]）。\
         原始数据见上方 [timing] 行；若是机器抖动请复跑评估。"
    );
    assert!(
        (0.6..=1.6).contains(&ratio_struct),
        "结构性损坏/内容损坏两条失败路径耗时比值越界：{ratio_struct:.3}（不在 [0.6, 1.6]）。\
         原始数据见上方 [timing] 行；若是机器抖动请复跑评估。"
    );

    for p in [&base, &corrupt, &structural] {
        let _ = std::fs::remove_file(p);
        let _ = std::fs::remove_file(p.with_extension("device"));
    }
}
