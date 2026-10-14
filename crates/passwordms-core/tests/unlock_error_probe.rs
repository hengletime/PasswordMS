//! A-05 解锁**错误码一致性**探针（Phase 4c · 任务 1）。
//!
//! 目的：独立验证 `docs/01-security-req.md:155`（A-05）所述
//! 「解锁失败提示不区分『密码错 / 数据损坏』」是否对**结构性头部损坏**也成立。
//!
//! 运行：`cargo test -p passwordms-core --target x86_64-pc-windows-gnullvm --nocapture probe_unlock_error`
//! （不加 `--nocapture` 也能跑，但看不到打印的原始错误码表。）
//!
//! 本文件**只新增测试**，不改生产代码。D-10 修复（2026-10）后，断言为：
//! - 错误密码 / 头部内容被篡改（verify_mac）→ `ERR_UNLOCK_FAILED`；
//! - **结构性头部损坏**（magic / version / header_len=0 / 截断 / JSON 反序列化）
//!   → 亦统一为 `ERR_UNLOCK_FAILED`（D-10 已折叠，原先的「特征化断言」已被正确期望取代）；
//! - **不存在的路径** → 仍为 `ERR_NO_VAULT`（主理人裁定：`NoVault` 属本地 UI 状态，不折叠）。

use std::path::PathBuf;

use passwordms_core::constants::CONTAINER_PREFIX_LEN;
use passwordms_core::session::Session;

fn tmp_path(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "passwordms_probe_{tag}_{}.vault",
        uuid::Uuid::new_v4()
    ));
    p
}

/// 把 `Session::unlock` 的结果折叠成可打印的字符串。
fn code_of(r: Result<Session, passwordms_core::CoreError>) -> String {
    match r {
        Ok(_) => "OK(意外成功)".to_string(),
        Err(e) => e.code().to_string(),
    }
}

/// 复制一份库字节、施加字节级改动、写到新路径。
fn variant(ok_bytes: &[u8], tag: &str, mutate: impl Fn(&mut Vec<u8>)) -> PathBuf {
    let mut b = ok_bytes.to_vec();
    mutate(&mut b);
    let p = tmp_path(tag);
    std::fs::write(&p, &b).expect("write variant");
    p
}

/// 造一个「容器结构合法、但头部 `verify_mac` 被篡改」的副本：`Container::open` 仍成功，
/// 从而解锁会走到完整 KDF + `unlock_check` 统一报错。
fn content_corrupted(ok_bytes: &[u8], tag: &str) -> PathBuf {
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

#[test]
fn probe_unlock_error_codes_across_failure_modes() {
    let pw = "qa_test_master_password";
    let base = tmp_path("base");
    drop(Session::create(&base, pw, true).expect("create"));
    let ok_bytes = std::fs::read(&base).expect("read base");

    // ① 错误密码（内容合法）。
    let c_wrong = code_of(Session::unlock(&base, "definitely_wrong_password"));

    // ② 头部内容被篡改（verify_mac 翻转）——结构合法。
    let p_content = content_corrupted(&ok_bytes, "content");
    let c_content = code_of(Session::unlock(&p_content, pw));

    // ③ 结构性损坏：magic / format_version / header_len=0 / 截断 / JSON 反序列化。
    let p_magic = variant(&ok_bytes, "magic", |b| b[0] ^= 0xff);
    let c_magic = code_of(Session::unlock(&p_magic, pw));

    let p_ver = variant(&ok_bytes, "version", |b| b[9] ^= 0xff);
    let c_ver = code_of(Session::unlock(&p_ver, pw));

    let p_len = variant(&ok_bytes, "hlen0", |b| {
        b[10] = 0;
        b[11] = 0;
    });
    let c_len = code_of(Session::unlock(&p_len, pw));

    let p_trunc = variant(&ok_bytes, "trunc", |b| b.truncate(20));
    let c_trunc = code_of(Session::unlock(&p_trunc, pw));

    // ③-b JSON 反序列化失败：magic / version / header_len 均合法，但头部首字节被破坏为
    //      非 JSON（0x00），令 `serde_json::from_slice` 在 `Container::open` 内失败。
    let p_json = variant(&ok_bytes, "json", |b| b[CONTAINER_PREFIX_LEN] = 0x00);
    let c_json = code_of(Session::unlock(&p_json, pw));

    // ④ 不存在的路径：必须仍为 ERR_NO_VAULT（不得被折叠）。
    let p_missing = tmp_path("missing");
    let c_missing = code_of(Session::unlock(&p_missing, pw));

    println!("==== A-05 / D-10 解锁错误码实测（除①外均用正确主密码去解）====");
    println!("① 错误密码                 -> {c_wrong}");
    println!("② 头部内容篡改(verify_mac)  -> {c_content}");
    println!("③ 结构性损坏 magic         -> {c_magic}");
    println!("   结构性损坏 version       -> {c_ver}");
    println!("   结构性损坏 header_len=0  -> {c_len}");
    println!("   结构性损坏 截断文件      -> {c_trunc}");
    println!("   结构性损坏 JSON 反序列化 -> {c_json}");
    println!("④ 不存在的路径             -> {c_missing}");

    // —— 统一为 ERR_UNLOCK_FAILED 的全部路径（含结构性损坏，D-10 已折叠）——
    assert_eq!(c_wrong, "ERR_UNLOCK_FAILED", "错误密码应统一为 ERR_UNLOCK_FAILED");
    assert_eq!(
        c_content, "ERR_UNLOCK_FAILED",
        "头部内容篡改（走到 unlock_check）应统一为 ERR_UNLOCK_FAILED"
    );
    for (label, c) in [
        ("magic", &c_magic),
        ("version", &c_ver),
        ("header_len=0", &c_len),
        ("truncated", &c_trunc),
        ("json-deserialize", &c_json),
    ] {
        assert_eq!(
            *c, "ERR_UNLOCK_FAILED",
            "[{label}] 结构性损坏应被折叠为 ERR_UNLOCK_FAILED（D-10），实际得到 {c}"
        );
    }

    // —— NoVault 必须独立，不得被误折叠 ——
    assert_eq!(
        c_missing, "ERR_NO_VAULT",
        "不存在的路径应仍返回 ERR_NO_VAULT（未被折叠）；实际得到 {c_missing}"
    );

    // 清理
    for p in [
        &base, &p_content, &p_magic, &p_ver, &p_len, &p_trunc, &p_json, &p_missing,
    ] {
        let _ = std::fs::remove_file(p);
        let _ = std::fs::remove_file(p.with_extension("device"));
    }
}
