//! D-55 备份恢复闭环回归测试（`restore_backup_from`，验收：恢复后可用备份创建时的
//! 主密码解锁、条目数一致；非 vault 文件拒绝；错主密码解锁失败）。

use std::path::PathBuf;
use std::time::SystemTime;

use passwordms_core::constants::{CONTAINER_PREFIX_LEN, FRAME_HEADER_LEN};
use passwordms_core::error::CoreError;
use passwordms_core::model::item::{CustomField, ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::container::restore_backup_from;
use passwordms_core::store::repository::ItemDraft;

/// 测试主密码（≥ 8 位）。
const MASTER: &str = "qa-restore-master-pw";

fn tmp_path(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "passwordms_restore_{tag}_{}.vault",
        uuid::Uuid::new_v4()
    ));
    p
}

fn login_draft(title: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: Some("https://example.invalid".to_string()),
            username: Some("qa_restore_user".to_string()),
            password: Some("qa_restore_password".to_string()),
            notes: None,
            totp_secret: None,
            custom_fields: vec![CustomField {
                key: "qa_restore_field".to_string(),
                value: "v".to_string(),
                hidden: false,
            }],
        }),
        folder_id: None,
        favorite: false,
    }
}

/// ① 建库（2 条）→ 导出备份 → 把本机库换成垃圾字节 → 恢复 → 原主密码解锁成功
/// 且记录数与恢复前一致。同时验证 mtime 语义：恢复后文件的修改时间 = 备份文件的
/// 修改时间（fs::copy 保留 LastWriteTime，客户要求保留）。
#[test]
fn restore_roundtrip_unlocks_with_original_password_and_keeps_mtime() {
    let vault_path = tmp_path("main");
    let backup_path = tmp_path("backup");

    // 建库 + 写入 2 条条目。
    let mut session = Session::create(&vault_path, MASTER, true).expect("create");
    session
        .repo_mut()
        .expect("repo")
        .create_item(login_draft("qa_restore_alpha"))
        .expect("item a");
    session
        .repo_mut()
        .expect("repo")
        .create_item(login_draft("qa_restore_beta"))
        .expect("item b");
    let records_before = session.repo().expect("repo").record_count();
    assert!(records_before >= 2, "至少应有 2 条记录");

    // 导出 = 字节级加密副本（unlock_check 校验主密码后 fs::copy）。
    session
        .repo()
        .expect("repo")
        .export_vault(&backup_path, MASTER)
        .expect("export");
    drop(session);

    // 模拟「本机库损坏」：库文件内容换成垃圾字节。
    std::fs::write(&vault_path, b"corrupted junk bytes, definitely not a vault").expect("corrupt");

    // 恢复（magic 校验 + 复制）。
    restore_backup_from(&backup_path, &vault_path).expect("restore");

    // mtime 语义：恢复后的文件修改时间必须等于备份文件的修改时间。
    let src_mtime: SystemTime = std::fs::metadata(&backup_path)
        .expect("src meta")
        .modified()
        .expect("src mtime");
    let dest_mtime: SystemTime = std::fs::metadata(&vault_path)
        .expect("dest meta")
        .modified()
        .expect("dest mtime");
    assert_eq!(src_mtime, dest_mtime, "恢复必须保留备份的修改时间");

    // 原主密码解锁成功，记录数与恢复前一致。
    let session = Session::unlock(&vault_path, MASTER).expect("unlock with original password");
    assert_eq!(
        session.repo().expect("repo").record_count(),
        records_before,
        "恢复后记录数应与备份时一致"
    );

    let _ = std::fs::remove_file(&vault_path);
    let _ = std::fs::remove_file(&backup_path);
}

/// ② 源文件不是保险库（magic 不符 / 过短）→ `Validation("not_a_vault_backup")`，
/// 且目标文件不得被创建或改动。
#[test]
fn restore_rejects_non_vault_file_without_touching_dest() {
    let src = tmp_path("junk");
    let dest = tmp_path("dest");

    // 乱写字节（长度足够但 magic 不符）。
    std::fs::write(&src, b"this is definitely not a PMSVAULT file").expect("write junk");
    let err = restore_backup_from(&src, &dest).expect_err("应拒绝非保险库文件");
    assert!(
        matches!(err, CoreError::Validation(ref m) if m == "not_a_vault_backup"),
        "期望 Validation(not_a_vault_backup)，实际 {err:?}"
    );
    assert!(!dest.exists(), "校验失败时不得触碰目标文件");

    // 过短文件（不足容器前缀 16 字节）同样拒绝。
    std::fs::write(&src, b"PM").expect("write short");
    let err = restore_backup_from(&src, &dest).expect_err("应拒绝过短文件");
    assert!(
        matches!(err, CoreError::Validation(ref m) if m == "not_a_vault_backup"),
        "期望 Validation(not_a_vault_backup)，实际 {err:?}"
    );
    assert!(!dest.exists(), "校验失败时不得触碰目标文件");

    let _ = std::fs::remove_file(&src);
}

/// ③ 恢复成功后用**错误主密码**解锁 → `UnlockFailed`（A-05 统一报错，
/// unlock_check 校验的是备份创建时的主密码）。
#[test]
fn restore_then_wrong_password_unlock_fails() {
    let vault_path = tmp_path("wrong");
    let backup_path = tmp_path("wrongbak");

    // 建库（主密码 = MASTER）+ 1 条条目 + 导出备份。
    let mut session = Session::create(&vault_path, MASTER, true).expect("create");
    session
        .repo_mut()
        .expect("repo")
        .create_item(login_draft("qa_restore_gamma"))
        .expect("item g");
    session
        .repo()
        .expect("repo")
        .export_vault(&backup_path, MASTER)
        .expect("export");
    drop(session);

    // 删掉本机库，从备份恢复一份（与 ① 完全独立的环境）。
    std::fs::remove_file(&vault_path).expect("remove vault");
    restore_backup_from(&backup_path, &vault_path).expect("restore");

    // 错误主密码 → 统一 UnlockFailed，不得泄露具体原因。
    // （Session 未实现 Debug，不能用 expect_err。）
    let err = match Session::unlock(&vault_path, "definitely-wrong-password") {
        Err(e) => e,
        Ok(_) => panic!("错主密码应解锁失败"),
    };
    assert!(
        matches!(err, CoreError::UnlockFailed),
        "期望 UnlockFailed，实际 {err:?}"
    );

    let _ = std::fs::remove_file(&vault_path);
    let _ = std::fs::remove_file(&backup_path);
}

/// 计算备份文件的记录区起点（`CONTAINER_PREFIX_LEN + header_len`，header_len 在 [10..12)）。
fn record_start_of(bytes: &[u8]) -> usize {
    assert!(bytes.len() >= CONTAINER_PREFIX_LEN, "文件短于容器前缀");
    let header_len = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
    CONTAINER_PREFIX_LEN + header_len
}

/// ④ 帧体截断（QA caseA 形态：截断点落在第一帧帧体内）→ restore 返回
/// `Validation("corrupt_vault_backup")`，且目标文件不得被创建。
/// （恢复入口提前拦截后，存量 unlock 路径对截断帧报 ERR_INTERNAL 的行为不再可达。）
#[test]
fn restore_rejects_backup_with_truncated_frame_body() {
    let vault_path = tmp_path("casea");
    let backup_path = tmp_path("caseabak");
    let dest = tmp_path("caseadest");

    // 建库（1 条，确保记录区至少有一帧）+ 导出备份。
    let mut session = Session::create(&vault_path, MASTER, true).expect("create");
    session
        .repo_mut()
        .expect("repo")
        .create_item(login_draft("qa_restore_casea"))
        .expect("item");
    session
        .repo()
        .expect("repo")
        .export_vault(&backup_path, MASTER)
        .expect("export");
    drop(session);

    // 把备份截断到「第一帧帧头之后、帧体之内」。
    let mut bytes = std::fs::read(&backup_path).expect("read backup");
    let start = record_start_of(&bytes);
    assert!(
        start + FRAME_HEADER_LEN + 8 < bytes.len(),
        "测试前提：第一帧帧体足够长"
    );
    bytes.truncate(start + FRAME_HEADER_LEN + 8);
    std::fs::write(&backup_path, &bytes).expect("write truncated");

    // restore 必须拒绝（corrupt_vault_backup），且不创建目标文件。
    let err = restore_backup_from(&backup_path, &dest).expect_err("帧体截断应被拒绝");
    assert!(
        matches!(err, CoreError::Validation(ref m) if m == "corrupt_vault_backup"),
        "期望 Validation(corrupt_vault_backup)，实际 {err:?}"
    );
    assert!(!dest.exists(), "校验失败时不得触碰目标文件");

    let _ = std::fs::remove_file(&vault_path);
    let _ = std::fs::remove_file(&backup_path);
}

/// ⑤ 记录区 <124B 尾料（QA caseB 形态：截断点在记录区内，遗留不足一帧头）→
/// 同样拒绝。**不允许出现 restore Ok 后 unlock 成功但 record_count=0 的静默丢数据路径。**
#[test]
fn restore_rejects_backup_with_tiny_tail_fragment() {
    let vault_path = tmp_path("caseb");
    let backup_path = tmp_path("casebbak");
    let dest = tmp_path("casebdest");

    let mut session = Session::create(&vault_path, MASTER, true).expect("create");
    session
        .repo_mut()
        .expect("repo")
        .create_item(login_draft("qa_restore_caseb"))
        .expect("item");
    session
        .repo()
        .expect("repo")
        .export_vault(&backup_path, MASTER)
        .expect("export");
    drop(session);

    // 把备份截断到「记录区起点 + 50B」——尾料不足 124B 帧头（QA 实测：这种备份
    // 此前 restore Ok → unlock 成功 → record_count=0，静默丢数据，最糟形态）。
    let mut bytes = std::fs::read(&backup_path).expect("read backup");
    let start = record_start_of(&bytes);
    assert!(
        start + 50 < bytes.len(),
        "测试前提：记录区应有足够内容可截断"
    );
    bytes.truncate(start + 50);
    std::fs::write(&backup_path, &bytes).expect("write truncated");

    let err = restore_backup_from(&backup_path, &dest).expect_err("尾料应被拒绝");
    assert!(
        matches!(err, CoreError::Validation(ref m) if m == "corrupt_vault_backup"),
        "期望 Validation(corrupt_vault_backup)，实际 {err:?}"
    );
    assert!(!dest.exists(), "校验失败时不得触碰目标文件");

    let _ = std::fs::remove_file(&vault_path);
    let _ = std::fs::remove_file(&backup_path);
}

/// ④【D-115】导出时主密码输错 → `WrongMasterPassword`，不是 `UnlockFailed`。
///
/// 语义分界：`UnlockFailed` 是**锁屏入口**的答案（A-05：不区分「密码错 / 头部坏」）；
/// `export_vault` 只可能在**已解锁会话**里被调用，调用者已认证、手里就有库文件，
/// 「密码对不对」不是它能多拿到的信息。继续回「解锁失败，请重试」会让用户
/// 在导出卡片里以为自己被锁在门外（主人真机走查 D-115）。
#[test]
fn export_with_wrong_password_reports_wrong_master_password() {
    let vault_path = tmp_path("export-wrong-pw");
    let backup_path = tmp_path("export-wrong-pw-out");
    let session = Session::create(&vault_path, MASTER, true).expect("create");

    let err = session
        .repo()
        .expect("repo")
        .export_vault(&backup_path, "not-the-master-pw")
        .expect_err("错主密码必须失败");
    assert!(
        matches!(err, CoreError::WrongMasterPassword),
        "期望 WrongMasterPassword，实际 {err:?}"
    );
    // 失败时不得落半个备份文件。
    assert!(!backup_path.exists(), "校验失败时不得产出备份文件");

    let _ = std::fs::remove_file(&vault_path);
}
