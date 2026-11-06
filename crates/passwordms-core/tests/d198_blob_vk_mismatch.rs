//! D-198「同因两文案」收口回归：blob 同步路径 `open_frame` 的包裹层解不开
//! 必须与 merge 路径 `decrypt_change` 一样映射成 `VaultKeyMismatch`。
//!
//! 根因：接收端 finalize（`sync/blob_transfer.rs:470`）调
//! `repo.verify_blob_frame(&frame)` → `open_frame`；过去 `unwrap_item_key(...)`
//! 是裸 `?`，两端 VK 不同时仍报 `Crypto`「加密操作失败」，与 merge 路径
//! 「这两台设备里的数据不是同一个保险库」自相矛盾。
//!
//! 边界（本文件两条用例一正一反钉死）：映射**只到包裹层**。
//! `aead::open` 失败（密文本身损坏 / 分片缺失 / 错序 / tag 不符）必须继续报 `Crypto`，
//! 绝不能被一起映射成 VaultKeyMismatch。
//!
//! 变异验证：① 把 `.map_err(|_| CoreError::VaultKeyMismatch)?` 改回裸 `?` → 用例 1 红；
//! ② 把 `aead::open(...)` 也套同样的 map_err → 用例 2（对照组）红。

use std::path::PathBuf;

use passwordms_core::error::{codes, CoreError};
use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::store::Frame;

const PWD_A: &str = "D198#Blob-A-Pw1";
const PWD_B: &str = "D198#Blob-B-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d198-blob-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn login_draft(title: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: None,
            username: None,
            password: None,
            notes: None,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
    }
}

/// 取会话本机 VK（拷成定长数组，便于「两库 VK 不同」断言）。
fn vault_key(session: &Session) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&session.repo().unwrap().vault_key()[..]);
    out
}

/// 用给定会话的 VK 造一个**合法** blob Frame（真密文，走 add_attachment_bytes → blob_frame）。
fn make_blob_frame(session: &mut Session) -> Frame {
    let repo = session.repo_mut().unwrap();
    let parent = repo.create_item(login_draft("blob-parent")).unwrap();
    let parent = repo
        .add_attachment_bytes(&parent.item_id, "att.bin", b"d198-blob-payload")
        .unwrap();
    let att_id = parent.attachments[0].attachment_id.clone();
    repo.blob_frame(&att_id).expect("应能取出刚写入的 blob 帧")
}

/// ① 两库 VK 不同：用库 A 的 VK 包出合法 blob Frame，交给库 B 的 `verify_blob_frame`。
///    包裹层 `wrapped_key` 解不开 → 必须报 `VaultKeyMismatch`，且不再是 `Crypto`。
///    变异 ①（改回裸 `?`）：此处会变 `ERR_CRYPTO` → 红。
#[test]
fn wrong_vk_reports_vault_key_mismatch_not_crypto() {
    let dir = temp_dir("mismatch");
    let vault_a = dir.join("a.vault");
    let vault_b = dir.join("b.vault");

    // 库 A：造一个合法 blob Frame（wrapped_key / ct 均由 VK_A 包裹）。
    let mut s_a = Session::create(&vault_a, PWD_A, true).unwrap();
    let frame = make_blob_frame(&mut s_a);
    let vk_a = vault_key(&s_a);

    // 库 B：另一把 VK（Session::create 每次随机生成，两库必然不同）。
    let s_b = Session::create(&vault_b, PWD_B, true).unwrap();
    assert_ne!(
        vk_a,
        vault_key(&s_b),
        "两库 VK 必须不同，否则没复现到目标路径"
    );

    // 用库 B 的 repo 验证库 A 的帧 → unwrap_item_key 失败。
    let err = s_b
        .repo()
        .unwrap()
        .verify_blob_frame(&frame)
        .expect_err("VK 不同，包裹层不可能解开");

    assert_eq!(
        err.code(),
        codes::ERR_VAULT_KEY_MISMATCH,
        "应点名「两端不是同一个保险库」，实际 {err:?}"
    );
    assert_eq!(err.message_key(), "err.vault_key_mismatch");
    assert_ne!(
        err.code(),
        codes::ERR_CRYPTO,
        "不得再说「加密操作失败」——这正是 D-198 要收的口"
    );
    assert!(matches!(err, CoreError::VaultKeyMismatch));

    let _ = std::fs::remove_dir_all(&dir);
}

/// ② 对照组：同一把**正确** VK 下，仅把 `ct` 翻 1 bit（密文/tag 损坏）。
///    此时 `wrapped_key` 解得开、只有 `aead::open` 失败 → 必须**仍**是 `Crypto`，
///    证明没把「数据损坏」误标成「库密钥不同」。
///    变异 ②（给 `aead::open` 也套 map_err）：此处会变 `ERR_VAULT_KEY_MISMATCH` → 红。
#[test]
fn corrupted_ct_stays_crypto_not_vault_key_mismatch() {
    let dir = temp_dir("corrupt");
    let vault_a = dir.join("a.vault");

    let mut s_a = Session::create(&vault_a, PWD_A, true).unwrap();
    let frame = make_blob_frame(&mut s_a);

    // 未破坏的帧在本库（正确 VK）下应验过：包裹层 + ct 都合法。
    s_a.repo()
        .unwrap()
        .verify_blob_frame(&frame)
        .expect("合法帧应通过校验");

    // 翻 ct 首字节 1 bit：密钥仍对、wrapped_key 不变，只有密文认证会失败。
    let mut corrupted = frame.clone();
    corrupted.ct[0] ^= 0x01;
    let err = s_a
        .repo()
        .unwrap()
        .verify_blob_frame(&corrupted)
        .expect_err("密文损坏，AEAD tag 校验必然失败");

    assert_eq!(
        err.code(),
        codes::ERR_CRYPTO,
        "数据损坏不得误标为库密钥不同，实际 {err:?}"
    );
    assert_ne!(err.code(), codes::ERR_VAULT_KEY_MISMATCH);

    let _ = std::fs::remove_dir_all(&dir);
}
