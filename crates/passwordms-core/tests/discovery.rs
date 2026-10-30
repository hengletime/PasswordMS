//! 局域网自动发现（D-74）回归。
//!
//! 七条用例，一条比一条接近真实：
//! 1. loopback 往返：请求 → 应答 → 拿到对端**真实 TCP 端口**；
//! 2. 未配对设备：**必须超时得 `None`**（而不是 `Err`，更不是应答）；
//! 3. `q` 不符的应答必须被丢弃（同网段可能有另一台设备正在发现）；
//! 4. 畸形包（版本 / tag 数量 / `q` 长度 / 非 JSON）不得 panic、不得应答；
//! 5. 单源限速：同源连发 10 个合法请求，应答 ≤ 5；
//! 6. **验收核心**：静态地址过期 → `resync_with_peer` 靠发现连上并**把新端口写回**；
//! 7. 自身应答过滤：同机双角色（同一把 `sync_key`）时，本机 responder 的应答必须被丢弃
//!    —— **带对照组**，证明 `None` 是过滤造成的而不是"压根没收到"。
//!
//! CI 上真实广播不可靠，所以全部用例都用 loopback 单播注入目标
//! （`DiscoveryMode::Unicast` / `discover_peer_with`），不依赖任何网络环境。

use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use passwordms_core::crypto::keys::{self, ItemKey};
use passwordms_core::crypto::rng;
use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::model::sync_state::PairedDevice;
use passwordms_core::session::Session;
use passwordms_core::store::repository::{ItemDraft, Repository};
use passwordms_core::sync::client;
use passwordms_core::sync::discovery::{self, DiscoveryRequest, DiscoveryResponse};
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::resync::{self, DiscoveryMode, ResyncServer};
use passwordms_core::sync::server::{device_key_aad, SyncServer};
use zeroize::Zeroizing;

const PWD_A: &str = "Disc#Pw-A1";
const PWD_B: &str = "Disc#Pw-B1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d74-{tag}-{}", uuid::Uuid::new_v4()));
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

fn device_id_of(session: &Session) -> String {
    session.repo().unwrap().device_id().to_string()
}

/// 让 `b` 采用 `a` 的 VK（模拟"已配对"后两端共享同一把 VK）。
fn share_vk(a: &Session, b: &mut Session) {
    let vk = Zeroizing::new(**a.repo().unwrap().vault_key());
    b.adopt_vault_key(&vk).unwrap();
}

/// 在 `session` 的配对表里为 `peer_id` 写一条记录，共享密钥用 `sync_key`。
fn add_device_row(session: &mut Session, peer_id: &str, sync_key: &ItemKey) {
    let repo = session.repo_mut().unwrap();
    let wrapped =
        keys::wrap_item_key(repo.vault_key(), &sync_key[..], &device_key_aad(peer_id)).unwrap();
    repo.upsert_paired_device(PairedDevice {
        device_id: peer_id.to_string(),
        device_name: "Peer".to_string(),
        fingerprint: "fp".to_string(),
        sync_key_wrapped: wrapped,
        paired_at: 1,
        last_sync_at: None,
        peer_ip: Some("127.0.0.1".to_string()),
        peer_port: Some(1),
    })
    .unwrap();
}

/// 在两端各写一条配对记录，共享同一把 `sync_key`；返回该 key。
fn pair_rows(a: &mut Session, b: &mut Session) -> ItemKey {
    let sync_key: ItemKey = Zeroizing::new(rng::random_key().unwrap());
    let a_id = device_id_of(a);
    let b_id = device_id_of(b);
    add_device_row(a, &b_id, &sync_key);
    add_device_row(b, &a_id, &sync_key);
    sync_key
}

/// 起一个带**注入发现 socket** 的 ResyncServer，返回 `(server, 发现口地址, TCP 口)`。
fn server_with_discovery() -> (ResyncServer, std::net::SocketAddr, u16) {
    let mut server = ResyncServer::start(0, "PeerB".to_string()).unwrap();
    let tcp_port = server.port();
    let sock = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let discovery_addr = sock.local_addr().unwrap();
    server.attach_discovery_socket(sock);
    (server, discovery_addr, tcp_port)
}

/// 在 `limit` 内反复轮询应答，返回（累计应答数）。
fn poll_until(
    server: &mut ResyncServer,
    repo: &Repository,
    limit: Duration,
    stop_at: usize,
) -> usize {
    let deadline = Instant::now() + limit;
    let mut total = 0usize;
    while Instant::now() < deadline {
        total += server.poll_discovery_once(repo);
        if stop_at > 0 && total >= stop_at {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    total
}

// ===== ① loopback 往返 =====

#[test]
fn discovery_roundtrip_on_loopback() {
    let dir = temp_dir("rt");
    let mut a = Session::create(&dir.join("a.vault"), PWD_A, true).unwrap();
    let mut b = Session::create(&dir.join("b.vault"), PWD_B, true).unwrap();
    share_vk(&a, &mut b);
    let sync_key = pair_rows(&mut a, &mut b);

    let (mut server, discovery_addr, tcp_port) = server_with_discovery();

    let key_for_client: ItemKey = Zeroizing::new(*sync_key);
    let handle = std::thread::spawn(move || {
        discovery::discover_peer_with(
            &key_for_client,
            &[discovery_addr],
            Duration::from_millis(1500),
            None,
        )
        .unwrap()
    });

    let replied = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_secs(3), 1)
    };
    let found = handle.join().unwrap();

    assert_eq!(replied, 1, "命中已配对设备时必须恰好应答一次");
    let (ip, port) = found.expect("loopback 上应当能发现对端");
    assert_eq!(ip, "127.0.0.1", "源地址即对端本次出口 IP");
    assert_eq!(port, tcp_port, "应答里的端口必须是本端 TCP 监听口");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ② 未配对 → 超时 None（不是 Err） =====

#[test]
fn discovery_no_reply_for_unpaired() {
    let dir = temp_dir("unpaired");
    let mut a = Session::create(&dir.join("a.vault"), PWD_A, true).unwrap();
    let mut b = Session::create(&dir.join("b.vault"), PWD_B, true).unwrap();
    share_vk(&a, &mut b);
    let sync_key = pair_rows(&mut a, &mut b);

    // B2：**有**配对记录，但那条记录用的是另一把 key（模拟"这台机器上和别人配过对"）。
    // 这样断言的是"tag 不匹配 → 不应答"，而不是"配对表为空 → 不应答"。
    let mut b2 = Session::create(&dir.join("b2.vault"), "Disc#Pw-C1", true).unwrap();
    share_vk(&a, &mut b2);
    let other_key: ItemKey = Zeroizing::new(rng::random_key().unwrap());
    let a_id = device_id_of(&a);
    add_device_row(&mut b2, &a_id, &other_key);
    assert_eq!(b2.repo().unwrap().list_paired_devices().len(), 1);

    let (mut server, discovery_addr, _tcp_port) = server_with_discovery();

    let key_for_client: ItemKey = Zeroizing::new(*sync_key);
    let handle = std::thread::spawn(move || {
        discovery::discover_peer_with(
            &key_for_client,
            &[discovery_addr],
            Duration::from_millis(500),
            None,
        )
        .unwrap()
    });
    let replied = {
        let repo = b2.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_millis(300), 0)
    };
    let found = handle.join().unwrap();

    assert_eq!(replied, 0, "不认识 tag 的请求一律不应答");
    assert!(found.is_none(), "没人应答时必须返回 None，而不是 Err");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ③ q 不符的应答必须被丢弃 =====

#[test]
fn discovery_drops_mismatched_q() {
    let dir = temp_dir("wrongq");
    let mut a = Session::create(&dir.join("a.vault"), PWD_A, true).unwrap();
    let mut b = Session::create(&dir.join("b.vault"), PWD_B, true).unwrap();
    share_vk(&a, &mut b);
    let sync_key = pair_rows(&mut a, &mut b);

    // 假应答方：收到请求后回一个 **q 不同但 tag 合法** 的应答（模拟同网段的另一次发现）。
    let fake = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let fake_addr = fake.local_addr().unwrap();
    let responder_key: ItemKey = Zeroizing::new(*sync_key);
    let responder = std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        fake.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        if let Ok((n, src)) = fake.recv_from(&mut buf) {
            let _req: DiscoveryRequest = serde_json::from_slice(&buf[..n]).unwrap();
            let wrong_q = rng::random_bytes(32).unwrap();
            let tag = discovery::response_tag(&responder_key, &wrong_q).unwrap();
            let resp = DiscoveryResponse {
                v: 1,
                q: wrong_q,
                tag,
                port: 12345,
            };
            let bytes = serde_json::to_vec(&resp).unwrap();
            let _ = fake.send_to(&bytes, src);
        }
    });

    let key_for_client: ItemKey = Zeroizing::new(*sync_key);
    let found = discovery::discover_peer_with(
        &key_for_client,
        &[fake_addr],
        Duration::from_millis(600),
        None,
    )
    .unwrap();
    responder.join().unwrap();

    assert!(found.is_none(), "q 不符的应答必须被丢弃，不能当成对端地址");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ④ 畸形包：不 panic、不应答 =====

#[test]
fn discovery_rejects_bad_version_and_oversized_tags() {
    let dir = temp_dir("malformed");
    let mut a = Session::create(&dir.join("a.vault"), PWD_A, true).unwrap();
    let mut b = Session::create(&dir.join("b.vault"), PWD_B, true).unwrap();
    share_vk(&a, &mut b);
    let sync_key = pair_rows(&mut a, &mut b);

    let (mut server, discovery_addr, _tcp_port) = server_with_discovery();

    let sender = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    sender
        .set_read_timeout(Some(Duration::from_millis(250)))
        .unwrap();

    let q = rng::random_bytes(32).unwrap();
    let good_tag = discovery::request_tag(&sync_key, &q).unwrap();

    // ① 版本不对（其余字段都合法）
    let bad_v = DiscoveryRequest {
        v: 2,
        q: q.clone(),
        tags: vec![good_tag.clone()],
    };
    // ② tag 数量远超上限（内容合法，但 100 条里前 8 条都不是正确 tag）
    let many_tags = DiscoveryRequest {
        v: 1,
        q: q.clone(),
        tags: vec!["A".repeat(43); 100],
    };
    // ③ q 长度不对
    let short_q = DiscoveryRequest {
        v: 1,
        q: q[..31].to_vec(),
        tags: vec![good_tag],
    };
    sender
        .send_to(&serde_json::to_vec(&bad_v).unwrap(), discovery_addr)
        .unwrap();
    sender
        .send_to(&serde_json::to_vec(&many_tags).unwrap(), discovery_addr)
        .unwrap();
    sender
        .send_to(&serde_json::to_vec(&short_q).unwrap(), discovery_addr)
        .unwrap();
    // ④ 根本不是 JSON
    sender.send_to(b"not-json-at-all", discovery_addr).unwrap();

    let replied = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_millis(400), 0)
    };
    assert_eq!(replied, 0, "畸形包不得触发应答");

    // 客户端侧也确认收不到任何东西（且不 panic）。
    let mut buf = [0u8; 512];
    assert!(
        sender.recv_from(&mut buf).is_err(),
        "畸形包不应换来任何回包"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑤ 单源限速 =====

#[test]
fn discovery_rate_limited_per_source() {
    let dir = temp_dir("ratelimit");
    let mut a = Session::create(&dir.join("a.vault"), PWD_A, true).unwrap();
    let mut b = Session::create(&dir.join("b.vault"), PWD_B, true).unwrap();
    share_vk(&a, &mut b);
    let sync_key = pair_rows(&mut a, &mut b);

    let (mut server, discovery_addr, _tcp_port) = server_with_discovery();

    let sender = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    sender
        .set_read_timeout(Some(Duration::from_millis(150)))
        .unwrap();

    // 同一 socket（= 同一源 IP）连发 10 个**完全合法**的请求。
    for _ in 0..10 {
        let q = rng::random_bytes(32).unwrap();
        let req = DiscoveryRequest {
            v: 1,
            q: q.clone(),
            tags: vec![discovery::request_tag(&sync_key, &q).unwrap()],
        };
        sender
            .send_to(&serde_json::to_vec(&req).unwrap(), discovery_addr)
            .unwrap();
    }

    let replied = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_secs(1), 0)
    };

    let mut received = 0usize;
    let mut buf = [0u8; 512];
    while sender.recv_from(&mut buf).is_ok() {
        received += 1;
    }

    assert!(replied >= 1, "限速不能把正常请求也全挡掉");
    assert!(
        replied <= 5,
        "单源每秒最多 5 次应答，实际 {replied}（收到 {received} 个回包）"
    );
    assert_eq!(received, replied, "客户端收到的回包数应等于服务端应答数");

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑥ 验收核心：地址过期 → 靠发现自愈 =====

/// 用真实配对流程建好两端，返回 Host 的 `(device_id, 已绑定的 ResyncServer)`。
///
/// 与 `resync_addr_heal.rs` 的做法一致：配对时先把 `ResyncServer` 的端口通告出去，
/// 这样 Joiner 的配对记录里就有一个"曾经正确"的 TCP 端口。
fn pair_real_host(host_vault: &Path, joiner_vault: &Path) -> (String, ResyncServer) {
    let mut host = Session::create(host_vault, PWD_A, true).unwrap();
    host.repo_mut()
        .unwrap()
        .create_item(login_draft("pre-pair"))
        .unwrap();
    let host_id = device_id_of(&host);
    drop(host);

    let resync_server = ResyncServer::start(0, "RealHost".to_string()).unwrap();
    let identity = HostIdentity::generate().unwrap();
    let mut sync_server = SyncServer::start(
        0,
        host_id.clone(),
        "RealHost".to_string(),
        identity,
        resync_server.port(),
        now_ms(),
    )
    .unwrap();
    let qr = sync_server
        .qr_payload(vec!["127.0.0.1".to_string()])
        .unwrap();

    let hv = host_vault.to_path_buf();
    let host_thread = std::thread::spawn(move || {
        let mut s = Session::unlock(&hv, PWD_A).unwrap();
        let repo = s.repo_mut().unwrap();
        sync_server.accept_and_sync(repo, now_ms())
    });

    let mut joiner = Session::create(joiner_vault, PWD_B, true).unwrap();
    let joiner_id = device_id_of(&joiner);
    let mut paired = client::pair(&qr, &joiner_id, "RealJoiner", 0, now_ms()).unwrap();
    joiner.adopt_vault_key(paired.host_vault_key()).unwrap();
    {
        let repo = joiner.repo_mut().unwrap();
        paired.persist_host_device(repo).unwrap();
        paired.push_and_sync(repo, now_ms()).unwrap();
    }
    host_thread.join().unwrap().unwrap();

    (host_id, resync_server)
}

#[test]
fn resync_heals_stale_addr_via_discovery() {
    let dir = temp_dir("heal");
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    let (host_id, mut server) = pair_real_host(&host_vault, &joiner_vault);
    let real_port = server.port();

    // 交换后（模拟"手机换了 IP / 端口 rebind"）：Joiner 的记录里端口是错的。
    // 刻意用 1（特权口，本机绝不会有服务在听），而不是 `real_port + 1`——并行跑用例时
    // "+1" 有可能正好是**另一条用例的监听口**，那样就会连到错误的服务器上，用例随机失败。
    let stale_port: u16 = 1;
    assert_ne!(stale_port, real_port);
    {
        let mut joiner = Session::unlock(&joiner_vault, PWD_B).unwrap();
        let repo = joiner.repo_mut().unwrap();
        let mut dev = repo.get_paired_device(&host_id).unwrap();
        dev.peer_ip = Some("127.0.0.1".to_string());
        dev.peer_port = Some(stale_port);
        repo.upsert_paired_device(dev).unwrap();
    }

    // 发现 socket：测试注入任意端口，避免抢 47823。
    let sock = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let discovery_addr = sock.local_addr().unwrap();
    server.attach_discovery_socket(sock);

    // Host 再写一条，让本轮同步确实有东西可应用。
    {
        let mut hs = Session::unlock(&host_vault, PWD_A).unwrap();
        hs.repo_mut()
            .unwrap()
            .create_item(login_draft("post-pair"))
            .unwrap();
    }

    // Host 侧：同时轮询发现应答与入站连接（与 200ms 轮询线程同构）。
    let hv = host_vault.clone();
    let host_thread = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&hv, PWD_A).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            let repo = host_session.repo_mut().unwrap();
            let _ = server.poll_discovery_once(repo);
            match server.poll_once(repo, now_ms()) {
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Ok(Some(outcome)) => return outcome,
                Err(e) => panic!("host 重同步失败: {e:?}"),
            }
        }
        panic!("host 在 15s 内没有收到重同步连接（发现没把地址交给客户端？）");
    });

    let outcome = {
        let mut joiner = Session::unlock(&joiner_vault, PWD_B).unwrap();
        let repo = joiner.repo_mut().unwrap();
        resync::resync_with_peer_opts(
            repo,
            &host_id,
            now_ms(),
            0,
            &DiscoveryMode::Unicast(vec![discovery_addr]),
        )
        .expect("静态端口过期时，应通过发现连上对端")
    };
    host_thread.join().unwrap();

    assert!(
        outcome.applied >= 1,
        "发现连上后必须完成真实同步（applied = {}）",
        outcome.applied
    );

    // 验收断言：新端口已写回 Joiner 的配对记录。
    let joiner = Session::unlock(&joiner_vault, PWD_B).unwrap();
    let dev = joiner.repo().unwrap().get_paired_device(&host_id).unwrap();
    assert_eq!(
        dev.peer_port,
        Some(real_port),
        "旧端口 {stale_port} 应被自愈为真实端口 {real_port}"
    );
    assert_eq!(dev.peer_ip.as_deref(), Some("127.0.0.1"));

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑦ 自身应答过滤（规格漏的洞：两端同一把 sync_key，tag 拦不住"自己"） =====

/// 同机双角色时，本机 responder 会应答自己的广播 —— 必须有 `self_port` 过滤。
///
/// 本用例的两个步骤缺一不可：
/// - **过滤组**：`self_port = Some(本机 TCP 监听口)` → 应答被丢弃 → `None`；
/// - **对照组**：同一份应答、`self_port = None` → 被接受 → `Some(127.0.0.1, 本机口)`。
///
/// 只测过滤组无法区分"被过滤"和"压根没收到"，所以对照组是这条用例价值的一半。
/// 谁把 `discover_peer_with` 里那段 `resp.port == own && is_local_ip(src.ip())` 删了，
/// 过滤组会立刻变红。
#[test]
fn discovery_ignores_self_reply() {
    let dir = temp_dir("selfreply");
    let mut a = Session::create(&dir.join("a.vault"), PWD_A, true).unwrap();
    let mut b = Session::create(&dir.join("b.vault"), PWD_B, true).unwrap();
    share_vk(&a, &mut b);
    let sync_key = pair_rows(&mut a, &mut b);

    // responder 与 requester 在**同一台机器**上、共用**同一把** sync_key —— 正是生产形态
    // （应用进程自己既发起发现、又在后台应答发现）。
    let (mut server, discovery_addr, tcp_port) = server_with_discovery();
    let self_port = tcp_port;

    // ① 过滤组
    let key_filtered: ItemKey = Zeroizing::new(*sync_key);
    let handle = std::thread::spawn(move || {
        discovery::discover_peer_with(
            &key_filtered,
            &[discovery_addr],
            Duration::from_millis(600),
            Some(self_port),
        )
        .unwrap()
    });
    let replied = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_millis(400), 1)
    };
    let filtered = handle.join().unwrap();
    assert!(
        replied >= 1,
        "构造前提：responder 确实应答了（否则下面的 None 无法归因）"
    );
    assert!(
        filtered.is_none(),
        "本机自己的应答必须被 self_port 过滤掉，否则客户端会把自己当成对端（实际：{filtered:?}）"
    );

    // ② 对照组：同一份应答，去掉过滤条件 → 必须被接受
    let key_control: ItemKey = Zeroizing::new(*sync_key);
    let handle = std::thread::spawn(move || {
        discovery::discover_peer_with(
            &key_control,
            &[discovery_addr],
            Duration::from_millis(600),
            None,
        )
        .unwrap()
    });
    let replied_control = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_millis(400), 1)
    };
    let accepted = handle.join().unwrap();
    assert!(replied_control >= 1, "对照组前提：responder 仍应答应答");
    assert_eq!(
        accepted,
        Some(("127.0.0.1".to_string(), tcp_port)),
        "不过滤时同一份应答必须被接受 —— 否则说明第一步的 None 是「没收到」而不是「被过滤」"
    );

    // ③ 复合判断的**另一半**：来源是本机、但端口不是我的 → 必须接受。
    //    只测 ①② 挡不住"把条件弱化成 only-is_local_ip"：那种改法下 ①② 仍然绿
    //    （同机同端口本来就该丢），但同一台机器上跑两个库、用不同端口的合法场景会
    //    被一起丢掉。这一组就是给"只判 IP"那条错路钉的钉子。
    //    （"只判端口"那条错路在回环上构造不出来：本机来源必然是本机 IP，
    //     需要一台真远端主机同端口应答才可分辨，此处只能留注释说明。）
    let key_other_port: ItemKey = Zeroizing::new(*sync_key);
    let other_port = tcp_port.wrapping_add(1); // 只作为过滤条件传入，不 bind，无端口冲突风险
    let handle = std::thread::spawn(move || {
        discovery::discover_peer_with(
            &key_other_port,
            &[discovery_addr],
            Duration::from_millis(600),
            Some(other_port),
        )
        .unwrap()
    });
    let replied_other = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_millis(400), 1)
    };
    let accepted_other = handle.join().unwrap();
    assert!(replied_other >= 1, "前提：responder 仍应答应答");
    assert_eq!(
        accepted_other,
        Some(("127.0.0.1".to_string(), tcp_port)),
        "端口不是本机监听口时，本机来源的应答必须被接受（同机多库场景）—— \
         若这里是 None，说明过滤条件被弱化成了「只判 IP」"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ===== ⑧ 多轮重播：窗口内必须真的反复广播（D-132）=====

/// v1.4.14 真机现象：两端都在同一网段、版本一致、防火墙放行，但每次 `sync_now`
/// 都是「4s 连接超时 + 1.5s 发现空等、零应答」。当时只发**一轮**广播，丢一次就
/// 彻底救不回来。本用例钉住「窗口内持续重播」这个行为本身。
///
/// 怎么让请求方**收不到应答却继续重播**：用 `self_port` 自过滤——应答方照常应答
/// （`replied` 能数到），但请求方按"D-74 自己应答自己"规则丢弃，于是它会一直等到
/// 窗口结束，中途按轮次重播。这样就能用「应答方收到几次请求」直接量出广播轮数。
#[test]
fn discovery_rebroadcast_repeats_within_window() {
    let dir = temp_dir("rebcast");
    let mut a = Session::create(&dir.join("a.vault"), PWD_A, true).unwrap();
    let mut b = Session::create(&dir.join("b.vault"), PWD_B, true).unwrap();
    share_vk(&a, &mut b);
    let sync_key = pair_rows(&mut a, &mut b);

    let (mut server, discovery_addr, tcp_port) = server_with_discovery();

    // ① 单轮（`discover_peer_with`）：一次请求只广播一次 → 应答方只收到 1 个请求。
    let key_single: ItemKey = Zeroizing::new(*sync_key);
    let single = std::thread::spawn(move || {
        discovery::discover_peer_with(
            &key_single,
            &[discovery_addr],
            Duration::from_millis(1000),
            Some(tcp_port),
        )
        .unwrap()
    });
    let n_single = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_millis(1000), 0)
    };
    let single_result = single.join().unwrap();
    assert!(
        single_result.is_none(),
        "构造前提：自应答必须被过滤（否则请求方拿到结果就停了，下面数不出轮数）"
    );
    assert_eq!(
        n_single, 1,
        "单轮入口在一个窗口内只能广播一次（这是测试与『单发语义』的契约，实际收到 {n_single} 次）"
    );

    // ② 多轮（`discover_peer_retrying`）：同窗口内应收到多次请求。
    //    窗口 1000ms、间隔 400ms → 预期 3 轮；断言取 >= 2 是为了不被限速窗口误伤，
    //    同时仍能与单轮的 1 次明确区分开。
    let key_multi: ItemKey = Zeroizing::new(*sync_key);
    let multi = std::thread::spawn(move || {
        discovery::discover_peer_retrying(
            &key_multi,
            &[discovery_addr],
            Duration::from_millis(1000),
            Some(tcp_port),
        )
        .unwrap()
    });
    let n_multi = {
        let repo = b.repo().unwrap();
        poll_until(&mut server, repo, Duration::from_millis(1000), 0)
    };
    let multi_result = multi.join().unwrap();
    assert!(multi_result.is_none(), "构造前提：多轮组的自应答同样被过滤");
    assert!(
        n_multi >= 2,
        "窗口内必须持续重播（单轮只 1 次；实际收到 {n_multi} 次）—— \
         若这里是 1，说明重播没生效，v1.4.14 的『丢一轮即永久失联』会原样回来"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
