//! D-147：配对只建立关系，**不搬数据**。
//!
//! 主人原话：「配对成功马上就同步有点不合适，有时只是想配对，配对成功后应该跳转到状态，
//! 由用户手动点击『立即同步』。」
//!
//! 这条守的是一件很容易被改坏的事：配对通道的握手末尾原本夹着一轮完整双向同步，
//! 去掉之后**两端都不能偷偷推进水位**——否则对端的 `base` 记成「它已持有我的变更」，
//! 而它一个字节都没收到，那些变更此后任何一轮都不会再重发，等于静默丢数据。
//!
//! 所以本文件的核心断言不只是「配对后没数据」，而是「**配对之后还能同步出全部数据**」：
//! 那条才是水位没被动过的证明。

use std::path::{Path, PathBuf};

use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::resync::{self, ResyncServer};
use passwordms_core::sync::server::SyncServer;

const HOST_PWD: &str = "D147#Host-Pw1";
const JOINER_PWD: &str = "D147#Joiner-Pw1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d147-{tag}-{}", uuid::Uuid::new_v4()));
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

fn item_titles(vault: &Path, pwd: &str) -> Vec<String> {
    let s = Session::unlock(vault, pwd).unwrap();
    let repo = s.repo().unwrap();
    let mut out: Vec<String> = repo
        .list_items(None, false, None)
        .into_iter()
        .map(|i| i.kind.title().to_string())
        .collect();
    out.sort();
    out
}

/// 一次重同步（Host 侧起线程 poll，Joiner 主动连）。
fn resync_once(
    host_vault: &Path,
    resync_server: ResyncServer,
    joiner_session: &mut Session,
    host_device_id: &str,
    local_port: u16,
) -> (
    passwordms_core::sync::resync::ResyncOutcome,
    passwordms_core::sync::resync::ResyncOutcome,
) {
    let hv = host_vault.to_path_buf();
    let handle = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&hv, HOST_PWD).unwrap();
        let mut resync_server = resync_server;
        loop {
            let repo = host_session.repo_mut().unwrap();
            match resync_server.poll_once(repo, now_ms()) {
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Ok(Some(outcome)) => return outcome,
                Err(e) => panic!("host 重同步失败: {e:?}"),
            }
        }
    });
    let joiner_outcome = {
        let repo = joiner_session.repo_mut().unwrap();
        resync::resync_with_peer(repo, host_device_id, now_ms(), local_port).unwrap()
    };
    (handle.join().unwrap(), joiner_outcome)
}

#[test]
fn pairing_alone_moves_no_data_and_leaves_the_door_open() {
    let dir = temp_dir("pair-only");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // ── Host：两条条目，等着看它们会不会在配对时被搬走 ──
    let mut host_session = Session::create(&host_vault, HOST_PWD, true).unwrap();
    {
        let repo = host_session.repo_mut().unwrap();
        repo.create_item(login_draft("d147-one")).unwrap();
        repo.create_item(login_draft("d147-two")).unwrap();
    }
    let host_id = host_session.repo().unwrap().device_id().to_string();
    drop(host_session);

    // ── 配对：Joiner 明确声明「这一轮只配对」 ──
    let host_resync = ResyncServer::start(0, "D147Host".to_string()).unwrap();
    let resync_port = host_resync.port();
    let mut server = SyncServer::start(
        0,
        host_id.clone(),
        "D147Host".to_string(),
        HostIdentity::generate().unwrap(),
        resync_port,
        now_ms(),
    )
    .unwrap();
    let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();

    let hv = host_vault.clone();
    let host_handle = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&hv, HOST_PWD).unwrap();
        let repo = host_session.repo_mut().unwrap();
        server.accept_and_sync(repo, now_ms())
    });

    let mut joiner_session = Session::create(&joiner_vault, JOINER_PWD, true).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();
    let mut joiner = client::pair(&qr, &joiner_id, "D147Joiner", 0, now_ms()).expect("握手失败");
    joiner_session
        .adopt_vault_key(joiner.host_vault_key())
        .unwrap();
    let joiner_result = {
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        joiner.mark_pair_only();
        joiner.push_and_sync(repo, now_ms()).unwrap()
    };
    drop(joiner);
    let host_result = host_handle.join().unwrap().expect("Host 侧配对失败");

    // ── 断言 1：两端都没动数据 ──
    assert_eq!(joiner_result.applied, 0, "只配对的一轮不该应用任何变更");
    assert_eq!(host_result.applied, 0, "Host 侧同样不该应用任何变更");
    assert_eq!(
        item_titles(&joiner_vault, JOINER_PWD),
        Vec::<String>::new(),
        "配对后 Joiner 仍是空库——这正是主人要的「先只配对」"
    );
    assert_eq!(
        item_titles(&host_vault, HOST_PWD),
        vec!["d147-one".to_string(), "d147-two".to_string()],
        "Host 的数据一条都不能少"
    );

    // ── 断言 2（关键）：配对关系确实建立，且随后「立即同步」能拿到全部 ──
    // 如果配对那轮把 Host 的 base 推进了，这里 applied 会是 0 —— 那些变更再也不会重发。
    let joiner_listener = ResyncServer::start(0, "D147Joiner".to_string()).unwrap();
    let joiner_port = joiner_listener.port();
    let mut joiner_session = Session::unlock(&joiner_vault, JOINER_PWD).unwrap();
    let (_host_out, joiner_out) = resync_once(
        &host_vault,
        host_resync,
        &mut joiner_session,
        &host_id,
        joiner_port,
    );
    assert_eq!(
        joiner_out.applied, 2,
        "配对后第一次显式同步必须拿到全部 2 条（水位没被配对那轮动过）"
    );
    drop(joiner_session);
    assert_eq!(
        item_titles(&joiner_vault, JOINER_PWD),
        vec!["d147-one".to_string(), "d147-two".to_string()],
        "显式同步之后数据才到位"
    );
}
