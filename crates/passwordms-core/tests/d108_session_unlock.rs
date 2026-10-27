//! D-108 回归套件：会话解锁（审查 3.2-6「session.rs 零测试」）+ 归一化/TOCTOU 修复守门。
//!
//! 审查报告 `docs/PasswordMS-审查报告-2026-10-27.md` 3.2-6：A-05 统一化 / burn-KDF /
//! 锁态清零 / 改密流程此前无 core 级回归网（3.2-1 的逃逸口正是因此长期存在）。
//! 本套件覆盖（每例都真实走 Argon2id，耗时换覆盖）：
//!
//! 1. 错误主密码 → 统一 `UnlockFailed`（A-05）；
//! 2. 库文件不存在 → `NoVault` 原样透出（主理人裁定：不折叠）；
//! 3. 头部结构性损坏（截断 / 垃圾覆写）→ 统一 `UnlockFailed`，不得逃逸成
//!    `ERR_IO` / `ERR_CRYPTO`（D-106 3.2-1 折叠的回归守门）；
//! 4. `lock()` 后状态复位、明文不可再访问（A-04）；
//! 5. 改主密码：旧密码错 → `UnlockFailed`；成功后旧密码失效、新密码可解锁；
//! 6. D-108（3.2-8）：`Repository::open_with_container` 复用容器的解锁路径等价
//!    （本文件所有 unlock 用例都走该路径，即回归本身）。

use std::path::PathBuf;

use passwordms_core::error::CoreError;
use passwordms_core::session::Session;

const PWD: &str = "D108#Session-Pw1";
const PWD2: &str = "D108#Session-Pw2";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d108-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 解锁并取回错误（`Session` 未实现 Debug，`unwrap_err` 用不了）。
fn unlock_err(path: &std::path::Path, pw: &str) -> CoreError {
    match Session::unlock(path, pw) {
        Err(e) => e,
        Ok(_) => panic!("unlock 应失败：{pw}"),
    }
}

/// 1. 错误主密码 → UnlockFailed（不是 Validation / Crypto / Io）。
#[test]
fn unlock_wrong_password_is_unified_error() {
    let dir = temp_dir("wrong-pw");
    let path = dir.join("passwordms.vault");
    Session::create(&path, PWD, true).unwrap();

    let err = unlock_err(&path, "totally-wrong-pw");
    assert!(
        matches!(err, CoreError::UnlockFailed),
        "应报 UnlockFailed，实际 = {err:?}"
    );
}

/// 2. 库文件不存在 → NoVault（主理人裁定：本地 UI 状态，不折叠）。
#[test]
fn unlock_missing_vault_is_no_vault() {
    let dir = temp_dir("no-vault");
    let path = dir.join("passwordms.vault");
    let err = unlock_err(&path, PWD);
    assert!(
        matches!(err, CoreError::NoVault),
        "应报 NoVault，实际 = {err:?}"
    );
}

/// 3. 头部结构性损坏 → 统一 UnlockFailed（D-106 3.2-1 折叠的回归守门）。
#[test]
fn unlock_corrupted_header_is_unified_error() {
    // 3a：截断到不足一帧头。
    let dir = temp_dir("header-truncated");
    let path = dir.join("passwordms.vault");
    Session::create(&path, PWD, true).unwrap();
    let full = std::fs::read(&path).unwrap();
    std::fs::write(&path, &full[..full.len() / 2]).unwrap();
    let err = unlock_err(&path, PWD);
    assert!(
        matches!(err, CoreError::UnlockFailed),
        "截断库应报 UnlockFailed，实际 = {err:?}"
    );

    // 3b：头部区被垃圾覆写（magic 保留、JSON 段破坏）。
    let dir = temp_dir("header-garbage");
    let path = dir.join("passwordms.vault");
    Session::create(&path, PWD, true).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let mid = bytes.len() / 2;
    for b in &mut bytes[10..mid] {
        *b = 0xA5;
    }
    std::fs::write(&path, &bytes).unwrap();
    let err = unlock_err(&path, PWD);
    assert!(
        matches!(err, CoreError::UnlockFailed),
        "垃圾头部应报 UnlockFailed，实际 = {err:?}"
    );
}

/// 4. lock() 后状态复位（A-04：锁定即清零）。
#[test]
fn lock_resets_state() {
    let dir = temp_dir("lock-reset");
    let path = dir.join("passwordms.vault");
    let mut session = Session::create(&path, PWD, true).unwrap();
    assert!(session.is_unlocked());
    assert!(session.repo().is_ok());

    session.lock();
    assert!(!session.is_unlocked(), "锁定后不得仍处于解锁态");
    assert!(session.repo().is_err(), "锁定后 repo 访问必须被拒");
    assert!(session.vault_path().is_err());
    assert!(session.maintenance_warnings().is_empty());
}

/// 5. 改主密码全流程：旧密码错拒绝；成功后旧密码失效、新密码可解锁。
#[test]
fn change_master_password_flow() {
    let dir = temp_dir("change-pw");
    let path = dir.join("passwordms.vault");
    let mut session = Session::create(&path, PWD, true).unwrap();

    // 旧密码错误 → UnlockFailed，密钥材料不变。
    let err = session
        .change_master_password("not-the-old-pw", PWD2)
        .unwrap_err();
    assert!(
        matches!(err, CoreError::UnlockFailed),
        "旧密码错误应报 UnlockFailed，实际 = {err:?}"
    );

    // 正确改密 → 锁定后旧密码必败、新密码必成。
    session.change_master_password(PWD, PWD2).unwrap();
    session.lock();
    assert!(matches!(unlock_err(&path, PWD), CoreError::UnlockFailed));
    let reopened = Session::unlock(&path, PWD2);
    assert!(
        reopened.is_ok(),
        "新密码必须能解锁，实际 = {:?}",
        reopened.err()
    );
}
