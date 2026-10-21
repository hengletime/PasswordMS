//! 已配对设备改名回归（D-66）。
//!
//! 背景：主人真机反馈「两台手机在设备列表里都叫 `Android-3dd4390a`，分不清」。
//! 修法两层：① 默认名加平台前缀（`resync::device_display_name`，单测在 resync.rs 内）；
//! ② 允许手动改名（`Repository::rename_paired_device`）。
//!
//! 本文件守 ② 的五条契约：
//! a) 正常改名（首尾空格被 trim）；
//! b) 空名拒绝（`Validation("device_name_empty")`）；
//! c) 超 64 字节拒绝（`Validation("device_name_too_long")`）；
//! d) 未配对设备拒绝（`NotPaired`，且不得凭空创建记录）；
//! e) 改名不得触碰 `peer_ip` / `peer_port` / `last_sync_at` / 指纹 / 共享密钥 / 配对时间，
//!    且改名结果要落盘（重新解锁后仍在）；trim 后与现值相同则幂等。

use std::path::{Path, PathBuf};

use passwordms_core::error::CoreError;
use passwordms_core::model::sync_state::PairedDevice;
use passwordms_core::model::vault::WrappedKey;
use passwordms_core::session::Session;

const VAULT_PWD: &str = "Rename#Pw-1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-rename-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 构造一条已配对设备记录（`device_id` 必须是 UUID 形态：`record_id_to_bytes` 会校验）。
fn paired_device(device_id: &str, name: &str) -> PairedDevice {
    PairedDevice {
        device_id: device_id.to_string(),
        device_name: name.to_string(),
        fingerprint: "fp-abc".to_string(),
        sync_key_wrapped: WrappedKey {
            nonce: vec![7u8; 12],
            ct: vec![9u8; 32],
        },
        paired_at: 1_700_000_000_000,
        last_sync_at: Some(1_700_000_111_000),
        peer_ip: Some("192.168.1.42".to_string()),
        peer_port: Some(47821),
    }
}

/// 建库 + 写一条配对记录，返回 `(dir, vault, peer_id)`。
fn setup(tag: &str) -> (PathBuf, PathBuf, String) {
    let dir = temp_dir(tag);
    let vault = dir.join("vault.vault");
    let peer_id = uuid::Uuid::new_v4().to_string();
    let mut session = Session::create(&vault, VAULT_PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();
    repo.upsert_paired_device(paired_device(&peer_id, "Android-3dd4390a"))
        .unwrap();
    drop(session);
    (dir, vault, peer_id)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rename_trims_and_persists() {
    let (dir, vault, peer_id) = setup("trim");
    {
        let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
        let repo = session.repo_mut().unwrap();
        let updated = repo
            .rename_paired_device(&peer_id, "  我的小米手机  ")
            .unwrap();
        assert_eq!(updated.device_name, "我的小米手机", "首尾空格必须被 trim");
        assert_eq!(updated.device_id, peer_id, "device_id 不可变");
    }
    // 落盘：重新解锁后仍是新名字（device 帧被重写过）。
    let reopened = Session::unlock(&vault, VAULT_PWD).unwrap();
    let dev = reopened
        .repo()
        .unwrap()
        .get_paired_device(&peer_id)
        .unwrap();
    assert_eq!(dev.device_name, "我的小米手机");
    cleanup(&dir);
}

#[test]
fn rename_rejects_empty_name() {
    let (dir, vault, peer_id) = setup("empty");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    for bad in ["", "   ", "\t\n "] {
        assert!(
            matches!(
                repo.rename_paired_device(&peer_id, bad),
                Err(CoreError::Validation(ref k)) if k == "device_name_empty"
            ),
            "空白名 {bad:?} 必须被拒绝"
        );
    }
    // 拒绝后原值不变。
    assert_eq!(
        repo.get_paired_device(&peer_id).unwrap().device_name,
        "Android-3dd4390a"
    );
    cleanup(&dir);
}

#[test]
fn rename_rejects_over_64_bytes() {
    let (dir, vault, peer_id) = setup("toolong");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();

    // 64 字节 = 上限，允许。
    let ok_name = "a".repeat(64);
    assert!(repo.rename_paired_device(&peer_id, &ok_name).is_ok());

    // 65 字节 = 拒绝。
    let bad_name = "a".repeat(65);
    assert!(
        matches!(
            repo.rename_paired_device(&peer_id, &bad_name),
            Err(CoreError::Validation(ref k)) if k == "device_name_too_long"
        ),
        "超 64 字节必须被拒绝"
    );
    // 中文按**字节**计：22 个汉字 = 66 字节 → 拒绝（不是按字符数）。
    let cjk = "手".repeat(22);
    assert!(matches!(
        repo.rename_paired_device(&peer_id, &cjk),
        Err(CoreError::Validation(ref k)) if k == "device_name_too_long"
    ));
    // 21 个汉字 = 63 字节 → 允许。
    assert!(repo
        .rename_paired_device(&peer_id, &"手".repeat(21))
        .is_ok());
    cleanup(&dir);
}

#[test]
fn rename_unknown_device_is_not_paired() {
    let (dir, vault, _peer_id) = setup("unpaired");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    let ghost = uuid::Uuid::new_v4().to_string();
    assert!(matches!(
        repo.rename_paired_device(&ghost, "新名字"),
        Err(CoreError::NotPaired)
    ));
    assert!(
        repo.get_paired_device(&ghost).is_none(),
        "不得凭空创建配对记录"
    );
    cleanup(&dir);
}

#[test]
fn rename_touches_only_device_name() {
    let (dir, vault, peer_id) = setup("onlyname");
    let mut session = Session::unlock(&vault, VAULT_PWD).unwrap();
    let repo = session.repo_mut().unwrap();
    let before = repo.get_paired_device(&peer_id).unwrap();
    let after = repo.rename_paired_device(&peer_id, "办公室电脑").unwrap();

    assert_eq!(after.device_name, "办公室电脑");
    assert_eq!(after.peer_ip, before.peer_ip, "peer_ip 不得改动");
    assert_eq!(after.peer_port, before.peer_port, "peer_port 不得改动");
    assert_eq!(
        after.last_sync_at, before.last_sync_at,
        "last_sync_at 不得改动"
    );
    assert_eq!(after.paired_at, before.paired_at, "paired_at 不得改动");
    assert_eq!(after.fingerprint, before.fingerprint, "指纹不得改动");
    assert_eq!(
        after.sync_key_wrapped, before.sync_key_wrapped,
        "共享同步密钥不得改动"
    );

    // trim 后与现值相同 → 幂等成功（不报错、不改变任何字段）。
    let again = repo
        .rename_paired_device(&peer_id, "  办公室电脑  ")
        .unwrap();
    assert_eq!(again.device_name, "办公室电脑");
    assert_eq!(again.last_sync_at, before.last_sync_at);
    cleanup(&dir);
}
