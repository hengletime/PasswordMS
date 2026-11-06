//! D-198 复现：**旧配对记录**让「配对」静默跳过 VK 采纳 → 两端 VK 不同 →
//! 点「立即同步」时台式机报「加密操作失败」、笔记本报「文件读写出错」。
//!
//! 主人真机（1.6.0，2026-11-06 早）：笔记本与公司台式机配对**成功**，随后点「立即同步」，
//! 笔记本提示「文件读写错误」，台式机提示「加密操作失败」。本文件把这条故障链逐环钉住：
//!
//! ① 配对放行判据（`src-tauri/src/commands/sync_cmd.rs` 的 `already_paired`）只要
//!    「有这台 Host 的旧记录」就成立，**不核对两端密钥是否真的相同**——于是
//!    `adopt_vault_key` 被跳过，配对照样返回成功；
//! ② 跳过之后两端各用自己的 VK 包裹同一把 `sync_key`：信封层（`seal_envelope`）与
//!    握手 MAC 全部照常通过，握手阶段没有任何检查能发现两条库不是同一条；
//! ③ 一旦开始搬数据，Host 侧 `merge::apply_changes` → `Repository::decrypt_change`
//!    用**本机 VK** 解对端的 `wrapped_key` → AEAD 打不开 → `CoreError::Crypto`
//!    → 用户看到「加密操作失败」；
//! ④ Host 中止后直接关连接，发起方还停在 `recv_frame` 上，收到的是 `UnexpectedEof`，
//!    归一到 `err.io` → 用户看到「文件读写出错」。
//!
//! 两个提示**同源**：不是两个故障，是一个故障的两端显示。
//!
//! 修法（见 `sync_cmd.rs` 的 `already_paired`）：豁免必须由**密钥相等**成立，而不是由
//! 「有过记录」成立；不相等且本机库非空 → 明确报错并说清下一步，不许静默继续。

use std::path::PathBuf;

use passwordms_core::error::CoreError;
use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::{now_ms, PairedDevice, WrappedKey};
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::resync::{self, ResyncServer};
use passwordms_core::sync::server::SyncServer;

const HOST_PWD: &str = "Sync#Host-Pw1";
const JOINER_PWD: &str = "Sync#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d198-{tag}-{}", uuid::Uuid::new_v4()));
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

/// 主人现场的最小复刻：两条**各自有数据**的库 + 一条指向 Host 的旧配对记录。
///
/// 返回 `(host 侧错误, joiner 侧错误, 配对后两端 VK 是否相同)`。
fn reproduce() -> (CoreError, CoreError, bool) {
    let dir = temp_dir("repro");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // Host：非空库（条目用自己的 VK 加密）。
    {
        let mut hs = Session::create(&host_vault, HOST_PWD, true).unwrap();
        hs.repo_mut()
            .unwrap()
            .create_item(login_draft("host-item"))
            .unwrap();
    }
    let host_device_id = {
        let s = Session::unlock(&host_vault, HOST_PWD).unwrap();
        s.repo().unwrap().device_id().to_string()
    };

    let resync_server = ResyncServer::start(0, "SyncHost".to_string()).unwrap();
    let resync_port = resync_server.port();

    // Joiner：也非空（自己的 VK），并带一条「以前和这台 Host 配过」的旧记录。
    // 仅「有记录」这一件事就够让豁免成立——这正是缺口的入口。
    {
        let mut js = Session::create(&joiner_vault, JOINER_PWD, true).unwrap();
        let repo = js.repo_mut().unwrap();
        repo.create_item(login_draft("joiner-item")).unwrap();
        repo.upsert_paired_device(PairedDevice {
            device_id: host_device_id.clone(),
            device_name: "SyncHost".to_string(),
            fingerprint: String::new(),
            // 内容无所谓：`persist_host_device` 会覆盖它。存在的意义是「让记录在」。
            sync_key_wrapped: WrappedKey {
                nonce: vec![0u8; 24],
                ct: vec![0u8; 48],
            },
            paired_at: now_ms() - 86_400_000,
            last_sync_at: None,
            peer_ip: Some("127.0.0.1".to_string()),
            peer_port: Some(resync_port),
        })
        .unwrap();
    }

    // ── 配对：逐字复刻 `src-tauri/src/commands/sync_cmd.rs` 的放行判定。
    let host_vk: [u8; 32];
    {
        let identity = HostIdentity::generate().unwrap();
        let mut server = SyncServer::start(
            0,
            host_device_id.clone(),
            "SyncHost".to_string(),
            identity,
            resync_port,
            now_ms(),
        )
        .unwrap();
        let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();

        let hv = host_vault.clone();
        let handle = std::thread::spawn(move || {
            let mut hs = Session::unlock(&hv, HOST_PWD).unwrap();
            let repo = hs.repo_mut().unwrap();
            server.accept_and_sync(repo, now_ms())
        });

        let mut js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
        let joiner_id = js.repo().unwrap().device_id().to_string();
        let mut joiner = client::pair(&qr, &joiner_id, "SyncJoiner", 0, None, now_ms())
            .expect("回环配对握手应成功");
        host_vk = *joiner.host_vault_key();

        let already_paired = {
            let repo = js.repo().unwrap();
            repo.list_paired_devices()
                .iter()
                .any(|d| d.device_id == host_device_id)
                || repo.matches_vault_key(joiner.host_vault_key())
        };
        assert!(already_paired, "旧配对记录应命中 D-47/D-62 豁免");
        if !already_paired {
            js.adopt_vault_key(joiner.host_vault_key()).unwrap();
        }
        {
            let repo = js.repo_mut().unwrap();
            joiner.persist_host_device(repo).unwrap();
            joiner.mark_pair_only();
            // 配对阶段不搬数据（D-147），所以这一步**不会**暴露密钥不一致。
            joiner
                .push_and_sync(repo, now_ms())
                .expect("pair_only 交换不碰数据，必须通过");
        }
        drop(joiner);
        handle.join().unwrap().expect("Host 侧配对应成功");
    }

    let keys_differ = {
        let js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
        !js.repo().unwrap().matches_vault_key(&host_vk)
    };

    // ── 「立即同步」。
    let hv = host_vault.clone();
    let host_handle = std::thread::spawn(move || {
        let mut hs = Session::unlock(&hv, HOST_PWD).unwrap();
        let mut server = resync_server;
        for _ in 0..300 {
            let repo = hs.repo_mut().unwrap();
            match server.poll_once(repo, now_ms()) {
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                other => return other,
            }
        }
        panic!("Host 未收到重同步连接");
    });

    let mut js = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let joiner_err = {
        let repo = js.repo_mut().unwrap();
        resync::resync_with_peer(repo, &host_device_id, now_ms(), 0)
            .expect_err("两端 VK 不同，交换不可能成功")
    };
    let host_err = match host_handle.join().unwrap() {
        Err(e) => e,
        Ok(v) => panic!("Host 侧本应失败，却返回 {v:?}"),
    };

    (host_err, joiner_err, keys_differ)
}

/// 复现本体：把「两个提示同源」钉成断言。
///
/// 关键在**两端都失败、但只有一端说的话有用**：
/// - 合并方（这里是 Host）现在能当场点名「不是同一个保险库」（D-198 修好之前它是
///   `Crypto`，用户看到的正是「加密操作失败」）；
/// - 发起方**永远只能**看到 `err.io`——它只是被掐断了连接，本身没有任何判断依据。
///   这也是为什么必须由合并方把话说清楚。
#[test]
fn vk_mismatch_shows_as_key_mismatch_on_merger_and_io_on_peer() {
    let (host_err, joiner_err, keys_differ) = reproduce();

    assert!(
        keys_differ,
        "跳过采纳后两端 VK 必须不同——否则本用例没复现到目标路径"
    );

    // 合并方：`merge` 用本机 VK 解对端条目的 `wrapped_key` → 包裹层打不开。
    assert!(
        matches!(host_err, CoreError::VaultKeyMismatch),
        "合并方应点名「两端不是同一个保险库」，实际 {host_err:?}"
    );
    assert_eq!(host_err.code(), "ERR_VAULT_KEY_MISMATCH");
    assert_eq!(host_err.message_key(), "err.vault_key_mismatch");

    // 发起方：对端已关连接，自己还停在等回包上 → UnexpectedEof → `err.io`。
    assert_eq!(
        joiner_err.code(),
        "ERR_IO",
        "发起方只会看到笼统的 io 错误，实际 {joiner_err:?}"
    );
    assert_eq!(
        joiner_err.message_key(),
        "err.io",
        "笔记本提示「文件读写出错」"
    );
}

/// D-198 的准入判定：**只有密钥相等才豁免采纳**。
///
/// 红色条件（把修复改回去必然红）：把 `admit_pairing` 的第一条判定改回
/// 「有过记录就 SameVault」，下面第二条用例会拿到 `SameVault` → 断言失败。
#[test]
fn admit_pairing_refuses_when_record_alive_but_key_changed() {
    let dir = temp_dir("admit");
    let path = dir.join("joiner.vault");
    let host_id = uuid::Uuid::new_v4().to_string();
    let host_id = host_id.as_str();

    let mut js = Session::create(&path, JOINER_PWD, true).unwrap();
    // 本机自己的 VK（记下来当「Host 的 VK」的对立面）。
    let repo = js.repo_mut().unwrap();
    repo.upsert_paired_device(PairedDevice {
        device_id: host_id.to_string(),
        device_name: "SyncHost".to_string(),
        fingerprint: String::new(),
        sync_key_wrapped: WrappedKey {
            nonce: vec![0u8; 24],
            ct: vec![0u8; 48],
        },
        paired_at: now_ms() - 86_400_000,
        last_sync_at: None,
        peer_ip: None,
        peer_port: None,
    })
    .unwrap();

    // ① 记录在 + 密钥不同 → 必须拒绝（这就是主人真机踩到的那条路）。
    assert_eq!(
        client::admit_pairing(repo, host_id, &[0x5a; 32]),
        client::PairAdmission::RefuseMismatch,
        "有过记录不代表还是同一个保险库——不许静默跳过采纳"
    );

    // ② 记录在 + 密钥相同 → 豁免（D-47 重配对 / D-62 解绑后重扫，两条都不能破）。
    let mut own_vk = [0u8; 32];
    own_vk.copy_from_slice(&repo.vault_key()[..]);
    assert_eq!(
        client::admit_pairing(repo, host_id, &own_vk),
        client::PairAdmission::SameVault,
        "密钥真的相同就该跳过采纳（D-47/D-62 豁免不许被一起删掉）"
    );

    // ③ 没有记录 → 走采纳（库空则成功，库非空由 adopt_vault_key 自己拒绝）。
    assert_eq!(
        client::admit_pairing(repo, "another-host", &[0x5a; 32]),
        client::PairAdmission::AdoptHostKey,
        "陌生 Host 必须走采纳，不能被上一台的记录影响"
    );
}
