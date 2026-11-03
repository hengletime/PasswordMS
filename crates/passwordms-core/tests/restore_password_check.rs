//! `verify_backup_password` 契约回归（B-03 / P0-3）。
//!
//! 为什么必须有这个文件：恢复是**整容器覆盖**（`restore_backup_from` 把源文件改名到
//! 目标）。密码校验一旦被挪到覆盖之后（或者被谁"优化"掉），后果是原库被换掉、用户
//! 被挡在门外，唯一能救他的 `passwordms.vault.bak-<ms>` 在界面上没有任何入口。
//! 这条路径只在真机点「从备份恢复」时才会走到，CI 里不钉住就等于没测。
//!
//! 覆盖：
//! a) 正确密码 → `Ok`；
//! b) 错误密码 → `WrongMasterPassword`（且**不碰**目标库，见下）；
//! c) 不是本产品备份 → 结构错误，不是密码错误（两者的处置完全不同：一个是换文件，
//!    一个是换密码）；
//! d) 头部能解析、帧流坏掉的备份 → 仍要报错，不能因为"密码对了"就放行。

use std::path::{Path, PathBuf};

use passwordms_core::error::CoreError;
use passwordms_core::session::Session;
use passwordms_core::store::container::verify_backup_password;

const VAULT_PWD: &str = "Restore#Check-1";
const WRONG_PWD: &str = "Restore#Check-2";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-restpwd-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// 建库并导出一份真实备份，返回 (库文件, 备份文件)。
fn vault_with_export(dir: &Path) -> (PathBuf, PathBuf) {
    let vault = dir.join("passwordms.vault");
    let backup = dir.join("export.vault");
    let session = Session::create(&vault, VAULT_PWD, true).unwrap();
    session
        .repo()
        .unwrap()
        .export_vault(&backup, VAULT_PWD)
        .unwrap();
    drop(session);
    (vault, backup)
}

#[test]
fn 正确密码通过错误密码被拒且不碰目标库() {
    let dir = temp_dir("ok");
    let (vault, backup) = vault_with_export(&dir);

    // 目标库的字节先记下来：这条测试要证明"验不过时一个字节都不动"。
    let vault_before = std::fs::read(&vault).unwrap();

    assert!(
        verify_backup_password(&backup, VAULT_PWD).is_ok(),
        "正确密码必须通过"
    );
    match verify_backup_password(&backup, WRONG_PWD) {
        Err(CoreError::WrongMasterPassword) => {}
        other => panic!("错误密码必须报 WrongMasterPassword，实得 {other:?}"),
    }

    // 校验本身是只读的：它不该改动备份，更不该改动任何库文件。
    assert_eq!(
        std::fs::read(&vault).unwrap(),
        vault_before,
        "密码预检不许碰目标库"
    );
    cleanup(&dir);
}

#[test]
fn 非备份文件报结构错误而不是密码错误() {
    let dir = temp_dir("notvault");
    let junk = dir.join("notes.txt");
    std::fs::write(&junk, b"this is not a vault at all, definitely long enough").unwrap();

    match verify_backup_password(&junk, VAULT_PWD) {
        Err(CoreError::Validation(code)) => {
            assert_eq!(code, "not_a_vault_backup", "应报结构错误");
        }
        other => panic!("非备份文件必须报 not_a_vault_backup，实得 {other:?}"),
    }
    cleanup(&dir);
}

#[test]
fn 帧流损坏的备份即使密码正确也不放行() {
    let dir = temp_dir("corrupt");
    let (_vault, backup) = vault_with_export(&dir);

    // 尾部截掉一段：头部完好（密码能算出同一个 KEK），但整库帧流解不完整。
    // 这正是"只验密码不验帧"会漏掉的那一类——恢复过去就是一份坏库。
    let bytes = std::fs::read(&backup).unwrap();
    let cut = bytes.len() - 16;
    std::fs::write(&backup, &bytes[..cut]).unwrap();

    assert!(
        verify_backup_password(&backup, VAULT_PWD).is_err(),
        "帧流不完整的备份不许因为密码正确就放行"
    );
    cleanup(&dir);
}
