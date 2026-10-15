//! 配对回环集成测试（D-43 取证实验）：Host 与 Joiner 在 127.0.0.1 完整走一遍
//! 「展示二维码 → 连接 → 握手 → 采纳 VK → 双向同步」。
//!
//! 背景：主人真机扫码失败——手机端读到「对端关闭」（EOF），证明 Host 在握手/交换中途断开；
//! 但 GUI 程序无法看 stderr，真机日志难取。本测试在**无防火墙、无网段差异**的回环环境
//! 完整复现配对：若这里失败，直接拿到 Host 端断开点（决定性）；若成功，根因指向网络环境层。

use passwordms_core::model::now_ms;
use passwordms_core::session::Session;
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

    let mut joiner = client::pair(&qr, &joiner_id, "LoopbackJoiner", 0, now_ms())
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
