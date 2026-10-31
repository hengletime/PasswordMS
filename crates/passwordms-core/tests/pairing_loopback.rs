//! 配对回环集成测试（D-43 取证实验）：Host 与 Joiner 在 127.0.0.1 完整走一遍
//! 「展示二维码 → 连接 → 握手 → 采纳 VK → 双向同步」。
//!
//! 背景：主人真机扫码失败——手机端读到「对端关闭」（EOF），证明 Host 在握手/交换中途断开；
//! 但 GUI 程序无法看 stderr，真机日志难取。本测试在**无防火墙、无网段差异**的回环环境
//! 完整复现配对：若这里失败，直接拿到 Host 端断开点（决定性）；若成功，根因指向网络环境层。

use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::server::SyncServer;

#[test]
fn pairing_loopback_handshake_and_sync() {
    let dir = std::env::temp_dir().join(format!("pms-loopback-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // ---- Host：建库、写一条条目、起配对监听 ----
    let host_session = Session::create(&host_vault, "Loopback#Master1", true).unwrap();
    let host_id = host_session.repo().unwrap().device_id().to_string();
    let identity = HostIdentity::generate().unwrap();

    let mut server = SyncServer::start(
        0, // 随机端口（0 = 内核分配；若 start 不支持 0 则失败后改固定高位端口）
        host_id.clone(),
        "LoopbackHost".to_string(),
        identity,
        0, // 无持久重同步端口
        now_ms(),
    )
    .unwrap();

    let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();

    // Host accept 放独立线程（阻塞式 accept_and_sync）。
    let host_handle = {
        let mut host_session = host_session;
        std::thread::spawn(move || {
            let repo = host_session.repo_mut().unwrap();
            server.accept_and_sync(repo, now_ms())
        })
    };

    // ---- Joiner：建库（空）→ client::pair 完整握手 → 采纳 VK → 同步 ----
    let mut joiner_session = Session::create(&joiner_vault, "Loopback#Joiner1", true).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();

    let mut joiner = client::pair(&qr, &joiner_id, "LoopbackJoiner", 0, None, now_ms())
        .expect("回环配对握手失败 —— Host 断开点见本测试的 host 线程结果");
    joiner_session
        .adopt_vault_key(joiner.host_vault_key())
        .unwrap();
    {
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        let result = joiner.push_and_sync(repo, now_ms()).unwrap();
        let _ = result;
    }
    drop(joiner);

    // ---- 收割 Host 线程结果：任何 Err 都会在这里原样暴露 ----
    let host_result = host_handle.join().unwrap();
    assert!(
        host_result.is_ok(),
        "Host accept_and_sync 失败: {:?}",
        host_result.err()
    );

    // 双端都能解锁（Joiner 已采纳 Host 的 VK）。host_session 已移入线程，此处不再持有。
    drop(joiner_session);
    Session::unlock(&host_vault, "Loopback#Master1").unwrap();
    assert!(
        Session::unlock(&joiner_vault, "Loopback#Master1").is_err(),
        "Joiner 不应被 Host 主密码解锁"
    );
    Session::unlock(&joiner_vault, "Loopback#Joiner1").unwrap();

    let _ = std::fs::remove_dir_all(&dir);
}

/// D-46 回归测试：`poll_accept_and_sync` 接受的连接必须是**阻塞模式**。
///
/// 平台缺陷（仅 Windows 触发）：accept 出的 socket 继承 listener 的非阻塞模式，
/// 后续 recv 在对端数据未到达时立即返回 WouldBlock → 配对必败。
/// 复现手法：客户端连接后**延迟 600ms** 才发帧——
///   - 有缺陷：host recv 立即 WouldBlock（错误里含 "would block"）；
///   - 已修复：host 阻塞等待 600ms，收到的是垃圾帧 → serde 错误（Internal/Validation），
///     且从 accept 到出错的耗时 ≥ 500ms（证明真的阻塞等待了）。
///
/// 回环投递零延迟，所以既有回环测试从不触发此竞态——本测试用人为延迟补上这个盲区。
#[test]
fn poll_accept_stream_is_blocking_after_accept() {
    use std::io::Write;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let dir = std::env::temp_dir().join(format!("pms-nonblock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let host_vault = dir.join("host.vault");

    let host_session = Session::create(&host_vault, "NonBlock#Master1", true).unwrap();
    let host_id = host_session.repo().unwrap().device_id().to_string();
    let identity = HostIdentity::generate().unwrap();

    let mut server = SyncServer::start(
        0,
        host_id,
        "NonBlockHost".to_string(),
        identity,
        0,
        now_ms(),
    )
    .unwrap();
    let port = server.local_port().unwrap();

    // Host：poll 路径（真实 App 用的就是这个）放独立线程。
    // ⚠️ poll_accept_and_sync 是**单次**轮询（无连接时返回 Ok(None)），真实 App 的 worker
    // 外层有循环；测试同样必须循环，否则 server 被立即 drop、监听关闭，客户端必被拒。
    let host_handle = {
        let mut host_session = host_session;
        std::thread::spawn(move || {
            let repo = host_session.repo_mut().unwrap();
            loop {
                match server.poll_accept_and_sync(repo, now_ms()) {
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    other => return other,
                }
            }
        })
    };

    // 客户端：连上后睡 600ms 再发一个垃圾帧，随后关闭。
    // 带重试：容忍 host 线程尚未调度到位的启动竞态。
    let client_handle = std::thread::spawn(move || {
        // 等待 host 进入 poll（监听已在 start 里就绪，直接连即可）。
        let mut stream = None;
        for _ in 0..60 {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => panic!("连接失败（非拒绝类）: {e:?}"),
            }
        }
        let mut stream = stream.expect("重试 3 秒仍被拒绝：监听端口不可达");
        std::thread::sleep(Duration::from_millis(600));
        // 长度前缀帧（u32 LE）+ 垃圾载荷：不是合法 JSON。
        let payload = b"not-a-valid-frame";
        stream
            .write_all(&(payload.len() as u32).to_le_bytes())
            .unwrap();
        stream.write_all(payload).unwrap();
        stream.flush().unwrap();
        // 保持连接片刻再关，避免 host 读 Done 时收到对端关闭混淆断言。
        std::thread::sleep(Duration::from_millis(200));
    });

    let started = Instant::now();
    let host_result = match host_handle.join() {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            panic!("Host 线程 panic：{msg}");
        }
    };
    client_handle.join().unwrap();
    let elapsed = started.elapsed();

    match host_result {
        // 有缺陷时的直接证据：WouldBlock 立即返回（几乎无耗时）。
        Err(e) => {
            let debug = format!("{e:?}");
            assert!(
                !debug.contains("would block"),
                "D-46 回归：poll_accept 后的 stream 仍是非阻塞模式（recv 返回 WouldBlock）。错误：{debug}"
            );
            assert!(
                elapsed >= Duration::from_millis(500),
                "host 未阻塞等待数据就出错（耗时 {elapsed:?}），疑似非阻塞残留"
            );
            // 收到垃圾帧 → serde 反序列化错误（Internal("json")），这是预期分支。
        }
        Ok(_) => panic!("垃圾帧不应被当成成功配对"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// D-47 回归：**同组重配对**——两端首次配对并同步后（Joiner 本地库已非空、VK 已采纳），
/// 再次完整走一遍「Host 起会话 → Joiner 握手 → 持久化 → 双向同步」，**全程不调用
/// adopt_vault_key**，协议层必须成功。对应真实场景：已配对设备重复扫码触发反向同步
/// （此前被 U1/U2「非空即拒」拦下，见 sync_cmd 的 already_paired 豁免）。
#[test]
fn repair_already_paired_vault_without_adopt() {
    let dir = std::env::temp_dir().join(format!("pms-repair-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // ---- 第一次配对（与 pairing_loopback_handshake_and_sync 相同流程）----
    let host_session = Session::create(&host_vault, "Repair#Master1", true).unwrap();
    let host_id = host_session.repo().unwrap().device_id().to_string();
    let identity = HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_id.clone(),
        "RepairHost".to_string(),
        identity,
        0,
        now_ms(),
    )
    .unwrap();
    let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();
    let host_handle = {
        let mut host_session = host_session;
        std::thread::spawn(move || {
            let repo = host_session.repo_mut().unwrap();
            server.accept_and_sync(repo, now_ms())
        })
    };
    let mut joiner_session = Session::create(&joiner_vault, "Repair#Joiner1", true).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();
    {
        let mut joiner = client::pair(&qr, &joiner_id, "RepairJoiner", 0, None, now_ms()).unwrap();
        joiner_session
            .adopt_vault_key(joiner.host_vault_key())
            .unwrap();
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        joiner.push_and_sync(repo, now_ms()).unwrap();
    }
    let _first = host_handle.join().unwrap().expect("首次配对失败");

    // ---- 第二次配对：Joiner 本地库**非空**（有首次同步进来的数据），不走采纳 ----
    let identity2 = HostIdentity::generate().unwrap();
    let mut server2 = SyncServer::start(
        0,
        // ⚠️ 必须用 Host 自己的 device_id（HostSyncResult.device_id 是对端 Joiner 的 id）。
        // 传错会导致 Joiner 的 sync_state 查错 key → seq 重置 → 正确地触发重放判定。
        host_id.clone(),
        "RepairHost".to_string(),
        identity2,
        0,
        now_ms(),
    )
    .unwrap();
    let qr2 = server2.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();
    let host_handle2 = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&host_vault, "Repair#Master1").unwrap();
        let repo = host_session.repo_mut().unwrap();
        server2.accept_and_sync(repo, now_ms())
    });

    let mut joiner2_session = Session::unlock(&joiner_vault, "Repair#Joiner1").unwrap();
    let joiner2_id = joiner2_session.repo().unwrap().device_id().to_string();
    // Joiner 侧全流程（配对 → 持久化 → 双向同步）必须**先于** join(Host) 完成：
    // Host 的 accept_and_sync 在 run_exchange 里等 Joiner 的 Push（30s 超时），
    // 若先把线程 join 了就构成测试自身的死锁（不是被测代码的问题）。
    let joiner_result = (|| -> Result<(), passwordms_core::CoreError> {
        let mut joiner2 = client::pair(&qr2, &joiner2_id, "RepairJoiner", 0, None, now_ms())?;
        // ⚠️ 关键：这里**故意不调用** adopt_vault_key（等价于后端 already_paired 豁免）。
        let repo = joiner2_session.repo_mut().unwrap();
        joiner2.persist_host_device(repo)?;
        joiner2.push_and_sync(repo, now_ms())?;
        Ok(())
    })();

    let second = match host_handle2.join() {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            panic!("Host 线程 panic：{msg}");
        }
    };
    assert!(
        second.is_ok(),
        "重配对（已配对保险库、跳过采纳）Host 侧失败: {:?}",
        second.err()
    );
    assert!(
        joiner_result.is_ok(),
        "重配对（已配对保险库、跳过采纳）Joiner 侧失败: {:?}",
        joiner_result.err()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// D-62 回归：**解绑后重配对**——Joiner 首次配对采纳 VK 并同步进数据后，在本机
/// 「删除配对设备」（`remove_paired_device`：只清 PairedDevice/sync_state，**库密钥
/// 保留**，本地数据仍以其加密）。D-47 豁免的判定依据（配对记录含该 Host）随之消失，
/// 重新扫码被 U1/U2「非空即拒」误拦（主人实测：手机解绑后无法与电脑重配对）。
/// sync_cmd 的 D-62 增强豁免 = 配对记录含该 Host **或 本机当前 VK == 握手下发的
/// Host VK**（相等 ⇒ 本机库本就用该密钥加密 ⇒ 采纳幂等零风险）。本用例在 core 层
/// 模拟命令层豁免语义：解绑 → 重配对 → 断言 VK 相等成立 → 不调用 adopt → 全流程
/// 成功且本地数据完好。
#[test]
fn repair_repair_after_unbind_with_matching_vk() {
    let dir = std::env::temp_dir().join(format!("pms-unbind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let host_vault = dir.join("host.vault");
    let joiner_vault = dir.join("joiner.vault");

    // ---- 第一次配对：Host 建 1 条数据，Joiner 空库采纳 + 同步进来 ----
    let mut host_session = Session::create(&host_vault, "Unbind#Master1", true).unwrap();
    let host_id = host_session.repo().unwrap().device_id().to_string();
    host_session
        .repo_mut()
        .unwrap()
        .create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "unbind-item".to_string(),
                url: None,
                username: None,
                password: Some("pw".to_string()),
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        })
        .unwrap();
    let identity = HostIdentity::generate().unwrap();
    let mut server = SyncServer::start(
        0,
        host_id.clone(),
        "UnbindHost".to_string(),
        identity,
        0,
        now_ms(),
    )
    .unwrap();
    let qr = server.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();
    let host_handle = {
        let mut host_session = host_session;
        std::thread::spawn(move || {
            let repo = host_session.repo_mut().unwrap();
            server.accept_and_sync(repo, now_ms())
        })
    };
    let mut joiner_session = Session::create(&joiner_vault, "Unbind#Joiner1", true).unwrap();
    let joiner_id = joiner_session.repo().unwrap().device_id().to_string();
    {
        let mut joiner = client::pair(&qr, &joiner_id, "UnbindJoiner", 0, None, now_ms()).unwrap();
        joiner_session
            .adopt_vault_key(joiner.host_vault_key())
            .unwrap();
        let repo = joiner_session.repo_mut().unwrap();
        joiner.persist_host_device(repo).unwrap();
        joiner.push_and_sync(repo, now_ms()).unwrap();
    }
    let first = host_handle.join().unwrap();
    assert!(first.is_ok(), "首次配对 Host 侧失败: {:?}", first.err());
    assert_eq!(
        joiner_session
            .repo()
            .unwrap()
            .list_items(None, false, None)
            .len(),
        1,
        "首次配对后 Joiner 应有 1 条同步进来的数据"
    );

    // ---- 主人操作：Joiner 侧删除配对设备（D-47 豁免判定依据消失，VK 保留）----
    joiner_session
        .repo_mut()
        .unwrap()
        .remove_paired_device(&host_id)
        .unwrap();
    assert!(
        joiner_session
            .repo()
            .unwrap()
            .list_paired_devices()
            .is_empty(),
        "解绑后配对记录应为空"
    );

    // ---- 重新配对：非空库 + 无配对记录 → 命中 D-62 的 VK 相等豁免（不 adopt）----
    let identity2 = HostIdentity::generate().unwrap();
    let mut server2 = SyncServer::start(
        0,
        host_id.clone(),
        "UnbindHost".to_string(),
        identity2,
        0,
        now_ms(),
    )
    .unwrap();
    let qr2 = server2.qr_payload(vec!["127.0.0.1".to_string()]).unwrap();
    let host_handle2 = std::thread::spawn(move || {
        let mut host_session = Session::unlock(&host_vault, "Unbind#Master1").unwrap();
        let repo = host_session.repo_mut().unwrap();
        server2.accept_and_sync(repo, now_ms())
    });

    let joiner_result = (|| -> Result<(), passwordms_core::CoreError> {
        let mut joiner2 = client::pair(&qr2, &joiner_id, "UnbindJoiner", 0, None, now_ms())?;
        // 模拟 sync_cmd 的 D-62 增强豁免判定：VK 相等 ⇒ 跳过 adopt。
        // 变异锚点：若豁免缺失，命令层会对非空库调用 adopt_vault_key → VkAdoptRequiresEmpty。
        assert_eq!(
            joiner_session.repo().unwrap().vault_key()[..],
            joiner2.host_vault_key()[..],
            "解绑不清密钥：本机 VK 必须仍等于握手下发的 Host VK（豁免前提）"
        );
        let repo = joiner_session.repo_mut().unwrap();
        joiner2.persist_host_device(repo)?;
        joiner2.push_and_sync(repo, now_ms())?;
        Ok(())
    })();

    let second = match host_handle2.join() {
        Ok(r) => r,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown panic".to_string());
            panic!("Host 线程 panic：{msg}");
        }
    };
    assert!(
        second.is_ok(),
        "解绑后重配对（VK 相等豁免）Host 侧失败: {:?}",
        second.err()
    );
    assert!(
        joiner_result.is_ok(),
        "解绑后重配对（VK 相等豁免）Joiner 侧失败: {:?}",
        joiner_result.err()
    );
    // 数据完好：条目仍在（密钥未被动过，可正常枚举）。
    assert_eq!(
        joiner_session
            .repo()
            .unwrap()
            .list_items(None, false, None)
            .len(),
        1,
        "重配对不得触碰本地数据"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
