//! P2 新增的两个会话级 API 守门（`docs/08-biometric-unlock.md` §18.5 约束 9/10）。
//!
//! 这两条 API 是为「命令层不许把封印身份拼错、也不许连开两次容器」而加的，
//! 本身没有独立业务语义，但**一旦退化就会静默失效**：
//!
//! 1. [`Session::verify_master_password`] —— 开启快速解锁的「证明你知道主密码」那一关。
//!    退化（恒 Ok / 生物会话直接拒）会让「解锁后离开座位被旁人开一条免密通道」成立，
//!    而 UI 上完全看不出异常。
//! 2. [`Session::unlock_with_opened_container`] —— 与 `unlock_with_vault_key` 必须**行为等价**。
//!    它是 TOCTOU 修复的落点：若有人在这里少了一帧判据，错误 VK 就能建出会话。

use std::path::PathBuf;

use passwordms_core::error::CoreError;
use passwordms_core::model::{ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::container::Container;
use passwordms_core::store::repository::ItemDraft;
use zeroize::Zeroizing;

const PW: &str = "correct-horse-battery";
const WRONG: &str = "wrong-horse-battery";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-biosess-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn vault_file(tag: &str) -> PathBuf {
    temp_dir(tag).join("passwordms.vault")
}

fn draft(title: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: None,
            username: Some("user".to_string()),
            password: Some("pw".to_string()),
            notes: None,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
    }
}

/// 建一个有**至少一帧**的库（有帧才能证明 VK 属于本库，见 `decrypted_frames`）。
fn seed_vault(tag: &str) -> (PathBuf, [u8; 32]) {
    let path = vault_file(tag);
    let mut session = Session::create(&path, PW, true).unwrap();
    session
        .repo_mut()
        .unwrap()
        .create_item(draft("条目"))
        .unwrap();
    let vk: [u8; 32] = **session.repo().unwrap().vault_key();
    session.lock();
    (path, vk)
}

// ───────────────────────── verify_master_password ─────────────────────────

/// 正确主密码通过；错误主密码 → `UnlockFailed`（不泄露「密码错 vs 头部坏」）。
#[test]
fn verify_master_password_checks_the_password() {
    let path = vault_file("verify");
    let session = Session::create(&path, PW, true).unwrap();

    session.verify_master_password(PW).expect("正确密码应通过");
    match session.verify_master_password(WRONG) {
        Err(CoreError::UnlockFailed) => {}
        other => panic!("错误密码应为 UnlockFailed，实际 {other:?}"),
    }
    // 校验不得改变会话状态：仍然解锁、仍然持有 KEK。
    assert!(session.is_unlocked());
    assert!(session.has_kek());
}

/// 锁定态没有可比对的 VK ⇒ `NotUnlocked`（而不是「密码正确就 Ok」）。
#[test]
fn verify_master_password_requires_unlocked_session() {
    let path = vault_file("locked");
    let mut session = Session::create(&path, PW, true).unwrap();
    session.lock();
    match session.verify_master_password(PW) {
        Err(CoreError::NotUnlocked) => {}
        other => panic!("锁定态应为 NotUnlocked，实际 {other:?}"),
    }
}

/// **关键**：生物识别会话（`kek = None`）照样能校验主密码。
///
/// 用户可能是用 Hello 快速进来的；开启快速解锁时仍必须证明「知道主密码」，
/// 否则「用 Hello 进来 → 再开一条 Hello 通道」等于一次 Hello 就永久免密。
/// 这条实现只用到头部 + KDF，不依赖会话 KEK——若哪天有人给它加个 `has_kek()` 前置，
/// 生物会话就会永远开不了快速解锁，而正常用户（主密码进来）测不出来。
#[test]
fn verify_master_password_works_on_biometric_session() {
    let (path, vk) = seed_vault("biosess");
    let session = Session::unlock_with_vault_key(&path, Zeroizing::new(vk)).expect("生物解锁");
    assert!(!session.has_kek(), "生物解锁路径不应有 KEK");

    session
        .verify_master_password(PW)
        .expect("生物会话必须能校验主密码");
    match session.verify_master_password(WRONG) {
        Err(CoreError::UnlockFailed) => {}
        other => panic!("错误密码应为 UnlockFailed，实际 {other:?}"),
    }
}

// ──────────────────── unlock_with_opened_container ────────────────────

/// 与 `unlock_with_vault_key` 等价：同一份 VK 进来的会话"看起来完全一样"。
#[test]
fn opened_container_entry_matches_path_entry() {
    let (path, vk) = seed_vault("opened");
    let via_path = Session::unlock_with_vault_key(&path, Zeroizing::new(vk)).expect("路径入口");
    let vault_id_path = via_path.repo().unwrap().header().vault_id.clone();

    let container = Container::open(&path).expect("open");
    let via_container = Session::unlock_with_opened_container(&path, container, Zeroizing::new(vk))
        .expect("容器入口");
    let vault_id_container = via_container.repo().unwrap().header().vault_id.clone();

    assert_eq!(vault_id_path, vault_id_container);
    assert_eq!(
        via_path.repo().unwrap().record_count(),
        via_container.repo().unwrap().record_count(),
        "两条入口打开的应是同一份数据"
    );
    assert!(!via_container.has_kek(), "都走生物路径，KEK 必须为 None");
}

/// **一帧都没解开就必须拒绝**——这条判据是 D-113 的 fix 2，容器入口不得绕过它。
///
/// 反例代价：假 VK 建出会话后，用户新建的条目全部用错密钥加密，主密码再也解不开。
#[test]
fn opened_container_entry_still_rejects_unverifiable_vault_key() {
    // 空库：create 之后没有任何业务帧，`decrypted_frames == 0`。
    let path = vault_file("empty");
    {
        let mut session = Session::create(&path, PW, true).unwrap();
        let vk: [u8; 32] = **session.repo().unwrap().vault_key();
        session.lock();
        let container = Container::open(&path).expect("open");
        match Session::unlock_with_opened_container(&path, container, Zeroizing::new(vk)) {
            Err(CoreError::NeedMasterPassword) => {}
            // `Session` 刻意不实现 `Debug`（内含密钥载体），所以只能分臂断言。
            Err(_) => panic!("空库应返回 NeedMasterPassword，实际是其它错误"),
            Ok(_) => panic!("空库不得建出会话"),
        }
    }

    // 非空库 + 错 VK：AEAD 会当场失败，绝不能"看起来成功了"。
    let (path, vk) = seed_vault("wrongvk");
    let mut wrong = vk;
    wrong[0] ^= 0xFF;
    let container = Container::open(&path).expect("open");
    assert!(
        Session::unlock_with_opened_container(&path, container, Zeroizing::new(wrong)).is_err(),
        "错 VK 不得建出会话"
    );
}
