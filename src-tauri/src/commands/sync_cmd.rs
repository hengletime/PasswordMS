//! 同步与配对命令（架构文档 §5.2、F-09、§3）。
//!
//! **配对**：Host 以非阻塞轮询线程等待 Joiner 连接；配对时双方互相通告各自的**持久重同步
//! 监听端口**（随二维码 / HELLO 传递），并写入 `PairedDevice`。
//! **直连重同步**：`sync_now` 读取已配对设备地址 → 建立连接 → 复用 sync 层握手（会话密钥
//! 派生 + 双向确认）→ 拉取/推送变更 → 合并（冲突入冲突箱）→ 更新游标（修复 D-11）。

use passwordms_core::constants::{DISCOVERY_PORT, SYNC_PORT_MAX, SYNC_PORT_MIN};
use passwordms_core::crypto::rng;
use passwordms_core::model::conflict::ConflictChoice;
use passwordms_core::model::now_ms;
use passwordms_core::sync::client;
use passwordms_core::sync::handshake::HostIdentity;
use passwordms_core::sync::protocol::{qr_svg, QRPayload};
use passwordms_core::sync::resync;
use passwordms_core::sync::server::SyncServer;
use tauri::Manager;
use zeroize::Zeroizing;

use crate::dto::{
    ConflictDto, DeviceDto, PairResultDto, PairingDto, SyncNetInfoDto, SyncResultDto, SyncStatusDto,
};
use crate::error::{lock_err, IpcError, IpcResult};
use crate::state::{ensure_resync_listener, identity_path, AppState};

/// F-09 Host：生成一次性令牌并展示二维码。
///
/// D-139：错误**留痕**（stderr + 数据目录 passwordms.log）——此前本命令失败静默，
/// 「加密操作失败」这类错误在 logcat / 日志里零痕迹，排查无从下手（与
/// `sync_join` 的留痕对称）。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_start_pairing(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> IpcResult<PairingDto> {
    let result = sync_start_pairing_impl(&app, &state);
    if let Err(e) = &result {
        // IpcError 的 Debug 只有 code + message_key，无隐私载荷。
        eprintln!("[sync_start_pairing] failed: {e:?}");
        crate::state::append_log(&app, &format!("[sync_start_pairing] failed: {e:?}"));
    }
    result
}

fn sync_start_pairing_impl(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
) -> IpcResult<PairingDto> {
    // 必须处于解锁态；同时取出 VK（身份私钥的包裹密钥，D-96）。
    //
    // `Zeroizing`：`repo.vault_key()` 本来是 `&Zeroizing<[u8; 32]>`，drop 即清零。但
    // `to_vec()` 复制出的是一份**普通 `Vec<u8>`**，不享受这层保护——为绕开
    // `session` 锁的作用域就必须复制，那就用 `Zeroizing` 把保护补回来，
    // 否则明文 VK 会以不受管理的形态留在堆上直到被覆盖（`docs/08` §35.5）。
    let (host_id, host_vk) = {
        let session = state.lock_session().map_err(lock_err)?;
        let repo = session.repo()?;
        (
            repo.device_id().to_string(),
            Zeroizing::new(repo.vault_key().to_vec()),
        )
    };

    // 确保持久重同步监听器在线，并把其端口通告给 Joiner（D-11）。
    let sync_port = ensure_resync_listener(app)?;

    let identity = load_or_create_identity(app, &host_vk[..])?;
    // 若已有配对会话，先作废。
    *state.pairing.lock().map_err(lock_err)? = None;

    let host_name = host_display_name(&host_id);
    let port = random_port()?;
    let server = SyncServer::start(port, host_id, host_name, identity, sync_port, now_ms())?;
    // D-69：会话码（= `sha256(host_eph 公钥)` 前 8 字节 hex）。
    // **不是**设备指纹：它绑的是本次握手的临时密钥，手机端由 `Accept.host_pub` 算出同一个值，
    // 两屏构造上必然相等，用户核对才有意义。临时密钥在 `SyncServer::start` 内一次生成，
    // 因此这里在手机连上之前就能拿到（这正是把生成点从每连接前移到 start 的原因）。
    let session_code = server.session_code();
    // D-44：二维码携带全部候选局域网 IPv4（多网卡机器上主选地址可能手机路由不到）。
    let ips = passwordms_core::sync::transport::local_ipv4s();
    let qr = server.qr_payload(ips.clone())?;
    let qr_text = qr.encode_string()?;
    let qr_markup = qr_svg(&qr_text)?;
    let dto = PairingDto {
        qr_svg: qr_markup,
        qr_text,
        ip: qr.ip.clone(),
        ips,
        port: qr.port,
        session_code,
        expires_at: qr.exp,
    };
    *state.pairing.lock().map_err(lock_err)? = Some(server);

    spawn_pairing_worker(app.clone());
    Ok(dto)
}

/// F-09 取消配对并作废令牌。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_cancel_pairing(state: tauri::State<'_, AppState>) -> IpcResult<()> {
    let mut guard = state.pairing.lock().map_err(lock_err)?;
    if let Some(server) = guard.as_mut() {
        server.cancel();
    }
    *guard = None;
    Ok(())
}

/// F-09 Joiner：扫码/粘贴二维码文本后发起配对并执行首次同步。
///
/// D-48：**async 命令 + spawn_blocking**——同步命令默认在主线程执行，
/// 直接做 30s 级网络等待会把整个 UI 冻住（手机实测：点其它全部无响应）。
#[tauri::command(rename_all = "snake_case")]
pub async fn sync_join(app: tauri::AppHandle, qr_payload: String) -> IpcResult<PairResultDto> {
    let app_log = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        sync_join_impl(&app, &state, &qr_payload)
    })
    .await
    .map_err(|e| IpcError::internal(&format!("spawn_blocking join: {e}")))?;
    if let Err(e) = &result {
        // 诊断留痕（数据目录 passwordms.log + stderr）：IpcError 的 Debug 只有 code + message_key，无隐私载荷。
        eprintln!("[sync_join] failed: {e:?}");
        crate::state::append_log(&app_log, &format!("[sync_join] failed: {e:?}"));
    }
    result
}

fn sync_join_impl(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
    qr_payload: &str,
) -> IpcResult<PairResultDto> {
    let qr = QRPayload::decode_string(qr_payload)?;
    let now = now_ms();

    // 确保持久重同步监听器在线，并把本机端口随 HELLO 通告给 Host（D-11）。
    let sync_port = ensure_resync_listener(app)?;

    // ── 锁内段①（O2，毫秒级）：本端设备身份。
    //
    // 【D-70】同时取本机**长期**身份公钥，随 HELLO 发给电脑端——电脑「设备」页
    // 才能显示一个跨配对稳定的指纹（认人，而不是认一次性的临时会话）。
    // 加载失败**不阻断配对**：降级为 None = 不发该字段 = 老客户端行为（对端留空），
    // 宁可没有指纹，也不能因为一个可选的身份文件让主人配不上对。
    let (device_id, device_name, identity_pub) = {
        let session = state.lock_session().map_err(lock_err)?;
        let id = {
            let repo = session.repo()?;
            repo.device_id().to_string()
        };
        let name = host_display_name(&id);
        // VK 要先拷出来：`load_or_create_identity` 会写身份文件，不能与 `repo()` 的
        // 不可变借用同时存在（且 VK 本身是短命临时值，用完即清）。
        let vk = Zeroizing::new(session.repo()?.vault_key().to_vec());
        let pubkey = match load_or_create_identity(app, &vk[..]) {
            Ok(identity) => Some(identity.public_bytes()),
            Err(e) => {
                let msg = format!("[identity] load for hello failed (fingerprint empty): {e:?}");
                eprintln!("{msg}");
                crate::state::append_log(app, &msg);
                None
            }
        };
        (id, name, pubkey)
    };

    // 网络段（O2，不持锁）：1) 握手（不接触本地数据）。
    // 行为变更说明：握手期间若自动锁定触发，后续锁内段②将失败并整体放弃本次配对
    // （旧实现持锁横跨全程无此窗口；换来的是配对全程 UI 可响应）。
    let mut joiner = client::pair(&qr, &device_id, &device_name, sync_port, identity_pub, now)?;

    // ── 锁内段②（O2，毫秒级）：2) 采用 Host 的 VK + 3) 持久化配对关系（含对端地址）。
    //    U1/U2 裁定：本地库非空则拒绝，由前端处理提示。
    //    D-47：**同组重配对豁免**——本机已与该 Host 配对 = 本地 VK 已在首次配对时采纳
    //    （与对端一致）。此时重复扫码（重装、重复操作、想手动触发反向同步）不应再被
    //    「非空即拒」拦下，直接复用既有密钥完成本次交换；真正的异组保险库仍被拒绝。
    //    D-62：豁免条件增强——本机「删除配对设备」（remove_paired_device）只清
    //    PairedDevice/sync_state、**保留库密钥**（本地数据仍以其加密），上面那条判定
    //    随之失配 → 重新扫码被 U1/U2 误拦（主人实测：解绑后无法重配对）。但此时本机
    //    库的加密密钥**就是**握手下发的 Host VK，采纳是零风险幂等（同值重包裹）。
    //    故补「本机当前 VK == 对端 VK」豁免：相等 = 同一逻辑库（安全跳过）；
    //    不等 = 真正的异组保险库（仍拒绝，U1/U2 红线不破）。
    {
        let mut session = state.lock_session().map_err(lock_err)?;
        let already_paired = {
            let repo = session.repo()?;
            repo.list_paired_devices()
                .iter()
                .any(|d| d.device_id == joiner.host_id())
                || repo.matches_vault_key(joiner.host_vault_key())
        };
        if !already_paired {
            // D-139：身份文件是「本机旧 VK」包裹的。采纳对端 VK 后若不重包裹，
            // 旧 VK 随采纳永久消失，这份私钥再也解不开——之后本机每次当 Host
            // （生成二维码）都报「加密操作失败」，且永不自愈（主人实测）。
            // 趁旧 VK 还在内存里：采纳**前**先解开身份私钥，采纳**后**用新 VK
            // 包回去——长期身份保住，对端无需重新配对。
            // 文件不存在 / 已无法用旧 VK 解开（历史遗留的损坏态）→ 跳过：
            // 下次 Host 加载时按 D-139 的「损坏等价重置」自愈重建。
            let old_vk = {
                let repo = session.repo()?;
                Zeroizing::new(repo.vault_key().to_vec())
            };
            session.adopt_vault_key(joiner.host_vault_key())?;
            let ident_path = identity_path(app)?;
            match passwordms_core::sync::identity::load_or_create_identity_file(
                &ident_path,
                &old_vk[..],
            ) {
                Ok(secret) => {
                    let new_vk: &[u8] = &session.repo()?.vault_key()[..];
                    if let Err(e) = passwordms_core::sync::identity::store_identity_file(
                        &ident_path,
                        &secret,
                        new_vk,
                    ) {
                        // 重包裹落盘失败不阻断配对：下次 Host 加载按 D-139 自愈重建。
                        let msg = format!("[identity] rewrap after adopt failed: {e:?}");
                        eprintln!("{msg}");
                        crate::state::append_log(app, &msg);
                    }
                }
                Err(e) => {
                    let msg = format!(
                        "[identity] unreadable before adopt (will reset on next host): {e:?}"
                    );
                    eprintln!("{msg}");
                    crate::state::append_log(app, &msg);
                }
            }
            // D-113（`docs/08` §18.5 约束 2）：采纳对端 VK 之后，现有的生物识别封印封的是
            // **换 VK 之前那个 VK**。core 的 `bind` 已让它必然被判失效（不会交出错误密钥），
            // 但一条「已失效」的封印留在盘上只会让用户困惑，且是多余的旧密钥密文。
            // `Session` 不持有 `SealStore`（core 刻意不碰文件系统之外的旁路），故撤销点在此。
            let seal_path = session.repo()?.path();
            if let Err(e) = passwordms_core::biometric::SealStore::new(&seal_path).remove() {
                crate::state::append_log(
                    app,
                    &format!("[biometric] remove seal after adopt failed: {e:?}"),
                );
            }
        }
        joiner.persist_host_device(session.repo_mut()?)?;
    }

    // 网络段 + 锁内段（O2 三段式）：4) 双向同步交换——`push_and_sync_access` 内部
    // 按「导出/合并/持久化」三个锁内段进出锁，传输全程不持会话锁。
    //
    // 【D-147】这一轮**只配对、不搬数据**：主人裁定「有时只是想配对」，配对成功后由
    // 用户自己在状态页点「立即同步」。`mark_pair_only` 让两端都不发变更、不合并、
    // 不推进任何同步水位——但要照常走完 Push/Done 这轮报文，否则对端会一直等。
    // 配对关系（含对端地址）已由上面的 `persist_host_device` 落盘，与数据无关。
    joiner.mark_pair_only();
    let mut access = crate::state::SessionAccess {
        session: &state.session,
    };
    let result = joiner.push_and_sync_access(&mut access, now)?;

    Ok(PairResultDto {
        device_id: joiner.host_id().to_string(),
        device_name: joiner.host_name().to_string(),
        device_fingerprint: joiner.device_fingerprint().to_string(),
        // 与电脑屏 `PairingDto.session_code` 同一把临时密钥算出的同一个值（D-69）。
        session_code: joiner.session_code().to_string(),
        applied: result.applied,
        conflicts: result.conflicts,
        blobs: result.blobs,
    })
}

/// F-09 列出已配对设备。
#[tauri::command(rename_all = "snake_case")]
pub fn list_paired_devices(state: tauri::State<'_, AppState>) -> IpcResult<Vec<DeviceDto>> {
    let session = state.lock_session().map_err(lock_err)?;
    Ok(session
        .repo()?
        .list_paired_devices()
        .into_iter()
        .map(DeviceDto::from)
        .collect())
}

/// O-07 解除配对（废弃共享密钥）。
#[tauri::command(rename_all = "snake_case")]
pub fn unpair(state: tauri::State<'_, AppState>, device_id: String) -> IpcResult<()> {
    let mut session = state.lock_session().map_err(lock_err)?;
    session.repo_mut()?.remove_paired_device(&device_id)?;
    Ok(())
}

/// D-66：重命名已配对设备（返回 `void`，前端据 `list_paired_devices` 刷新）。
///
/// 背景：默认展示名由 `resync::device_display_name` 生成（`<平台>-<8 位短 ID>`），
/// 两台同型号手机的短 ID 也可能被用户看成同一个，设备列表无法分辨 → 提供手动改名。
/// 校验（空 / 超 64 字节 / 未配对）全部在 core：`Repository::rename_paired_device`。
/// 契约：前端 `invoke('rename_paired_device', { device_id, name })` → `void`。
#[tauri::command(rename_all = "snake_case")]
pub fn rename_paired_device(
    state: tauri::State<'_, AppState>,
    device_id: String,
    name: String,
) -> IpcResult<()> {
    let mut session = state.lock_session().map_err(lock_err)?;
    session
        .repo_mut()?
        .rename_paired_device(&device_id, &name)?;
    Ok(())
}

/// F-09 手动同步：直连已配对设备执行一次增量重同步（修复 D-11）。
///
/// 失败路径映射到既定错误码：未配对 → `ERR_NOT_PAIRED`；密钥确认失败 → `ERR_FINGERPRINT_MISMATCH`；
/// 序号/时间窗异常 → `ERR_REPLAY_DETECTED`；连接/协议中断 → `ERR_SYNC_ABORTED`。
/// D-48：async + spawn_blocking，同 `sync_join`——网络 I/O 不进主线程。
#[tauri::command(rename_all = "snake_case")]
pub async fn sync_now(app: tauri::AppHandle, device_id: String) -> IpcResult<SyncResultDto> {
    use std::sync::atomic::Ordering;
    let app_log = app.clone();
    // D-132：日志里必须带设备 id，否则 2 台对端连续失败时无法分辨是哪台、
    // 也无法对应「设备」页里的那一行。
    let dev_for_log = device_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        // v1.2.4（根因 B）：本机持久重同步监听端口要随 HELLO 通告给对端，
        // 对端据此刷新它记录的本机地址（地址自愈）。
        // **必须在取 session 锁之前**：`ensure_resync_listener` 要拿 resync 锁，
        // 而全局锁顺序恒为 resync → session；反过来写就是死锁。
        let sync_port = ensure_resync_listener(&app)?;
        // O2：同一时刻仅一场同步（与被动 worker 互斥，防双场交换的 seq/base 竞态）。
        if state
            .sync_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // D-108（豆包审查 3.4-7）：`SyncBusy` 入 core 错误码表（编译期约束），
            // 不再在命令层手工拼 `ERR_SYNC_BUSY`。
            return Err(IpcError::from(passwordms_core::CoreError::SyncBusy));
        }
        let run = || -> IpcResult<SyncResultDto> {
            // O2：三段式——锁内段仅在「读写库的瞬间」短暂持有 session 锁，
            // 连接/握手/传输全程锁外，同步期间 UI 的一切 IPC 不再排队等待。
            let mut access = crate::state::SessionAccess {
                session: &state.session,
            };
            let outcome = resync::resync_with_peer_access(
                &mut access,
                &device_id,
                now_ms(),
                sync_port,
                &resync::DiscoveryMode::Broadcast,
            )?;
            Ok(SyncResultDto {
                applied: outcome.applied,
                conflicts: outcome.conflicts,
                blobs: outcome.blobs,
            })
        };
        passwordms_core::sync::discovery::clear_trace();
        let result = run();
        state.sync_busy.store(false, Ordering::SeqCst);
        // D-132：把发现轨迹落盘。成功时也有价值——"靠发现把地址换成了新的"
        // 是要写进文档的事实；失败时它是唯一能区分「手机没收到广播」与
        // 「手机回了但应答到不了」的依据。合并成一行，避免几十次文件开关。
        let traces = passwordms_core::sync::discovery::drain_trace();
        if !traces.is_empty() {
            crate::state::append_log(&app, &format!("[sync] 轨迹: {}", traces.join(" | ")));
        }
        result
    })
    .await
    .map_err(|e| IpcError::internal(&format!("spawn_blocking sync_now: {e}")))?;
    if let Err(e) = &result {
        eprintln!("[sync_now] failed: {e:?}");
        crate::state::append_log(
            &app_log,
            &format!("[sync_now] failed: device={dev_for_log} err={e:?}"),
        );
    }
    result
}

/// D-134：本机同步网络信息（只读诊断）。
///
/// 为什么必须把它上屏：手动同步失败时用户手上只有一句「网络不可达」，而真正要判断的
/// 只有一件事——**电脑记的地址和手机此刻的地址/端口是不是同一个**。这两个字
/// （`local_ips` 与 `listen_port`）原来只埋在日志和 core 结构里，两端各看一眼就能定性。
///
/// 刻意**不取 session 锁**：它不读库、只读网络状态，且要在「同步失败后立刻刷新」
/// 这种时序下可用；这些信息（局域网 IP）本来就在配对二维码里明文展示，无新增暴露面。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_net_info(app: tauri::AppHandle) -> IpcResult<SyncNetInfoDto> {
    let (listen_port, discovery_enabled) = crate::state::resync_net_info(&app);
    Ok(SyncNetInfoDto {
        local_ips: passwordms_core::sync::transport::local_ipv4s(),
        listen_port,
        discovery_enabled,
        discovery_port: DISCOVERY_PORT,
    })
}

/// F-09 同步状态查询。
#[tauri::command(rename_all = "snake_case")]
pub fn sync_status(state: tauri::State<'_, AppState>) -> IpcResult<SyncStatusDto> {
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let devices = repo.list_paired_devices();
    let last_sync_at = devices.iter().filter_map(|d| d.last_sync_at).max();
    // D-56：待发变更按「对端 base 过滤后」的口径统计，取最落后对端的数量
    // （墓碑经免重发过滤；无已配对设备时无可同步对象，计 0）。
    // D-151：口径不变，但**归因到具体设备**——否则数字被历史残影顶住时，
    // 用户看到的是一个没有主语、永远不动的 58（真机 2026-10-31）。
    let pending = passwordms_core::sync::status::pending_summary(repo);
    Ok(SyncStatusDto {
        last_sync_at,
        pending_changes: pending.count,
        device_count: devices.len(),
        pending_device_name: pending.worst_device_name,
        pending_device_never_synced: pending.worst_never_synced,
    })
}

/// 列出未仲裁冲突。
#[tauri::command(rename_all = "snake_case")]
pub fn list_conflicts(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ConflictDto>> {
    let session = state.lock_session().map_err(lock_err)?;
    Ok(session
        .repo()?
        .list_conflicts()
        .into_iter()
        .map(ConflictDto::from)
        .collect())
}

/// 三选一仲裁（`keep_local` / `take_remote` / `keep_both`）。
///
/// D-112：`copy_suffix` 为 KeepBoth 副本标题后缀，由前端按当前 UI 语言传入
/// （如「 (冲突副本)」）——core 不硬编码文案。注意保留前导空格。
#[tauri::command(rename_all = "snake_case")]
pub fn resolve_conflict(
    state: tauri::State<'_, AppState>,
    conflict_id: String,
    choice: String,
    copy_suffix: String,
) -> IpcResult<()> {
    let parsed = match choice.as_str() {
        "keep_local" => ConflictChoice::KeepLocal,
        "take_remote" => ConflictChoice::TakeRemote,
        "keep_both" => ConflictChoice::KeepBoth,
        _ => {
            return Err(IpcError::from(passwordms_core::CoreError::Validation(
                "choice".to_string(),
            )))
        }
    };
    let mut session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo_mut()?;
    let resets = repo.resolve_conflict(&conflict_id, parsed, &copy_suffix)?;
    // D-109/D-111：对每个对端应用仲裁的状态修正——
    // ① 水位回落（TakeRemote）：落地 rev 回落后，min 回落防「rev ≤ 水位免重发」
    //    永久吞掉本端后续修改；
    // ② base 推进（KeepLocal，D-111）：胜出版本落地后各对端 base 推进到
    //    {new_rev, new_hash}，否则双 KeepLocal 场景每轮重新报冲突（新形态振荡）；
    //    TakeRemote 的 base 冲突时已推进为同值，insert 幂等无害。
    if !resets.is_empty() {
        for peer in repo.list_paired_devices() {
            let mut st = repo.sync_state(&peer.device_id);
            let mut changed = false;
            for r in &resets {
                if let Some(wm) = st.sent_watermark.get_mut(&r.record_id) {
                    if *wm > r.new_rev {
                        *wm = r.new_rev;
                        changed = true;
                    }
                }
                let needs_base = match st.base.get(&r.record_id) {
                    Some(b) => b.rev != r.new_rev || b.hash != r.new_hash,
                    None => true,
                };
                if needs_base {
                    st.base.insert(
                        r.record_id.clone(),
                        passwordms_core::model::sync_state::BaseRev {
                            rev: r.new_rev,
                            hash: r.new_hash,
                        },
                    );
                    changed = true;
                }
            }
            if changed {
                repo.save_sync_state(st)?;
            }
        }
    }
    Ok(())
}

// ===== 内部辅助 =====

/// 加载或创建 Host 长期身份密钥对（持久化于应用数据目录）。
///
/// 【D-96】私钥不再以 Base64 裸明文落盘：改用**保险库主密钥（VK）**经
/// XChaCha20-Poly1305 包裹（新格式带 `PMSIDENT1` 魔数，旧格式首次加载即自动迁移，
/// 详见 core `sync::identity` 模块）。调用点 `sync_start_pairing` 必然已先
/// `session.repo()?` 解锁，VK 必在——锁定态本命令入口就被 `NOT_UNLOCKED` 拦下。
fn load_or_create_identity(app: &tauri::AppHandle, vault_key: &[u8]) -> IpcResult<HostIdentity> {
    let path = identity_path(app)?;
    let secret = passwordms_core::sync::identity::load_or_create_identity_file(&path, vault_key)
        .map_err(IpcError::from)?;
    Ok(HostIdentity::from_secret_bytes(&secret))
}

/// 便于用户识别的本机展示名（不含任何隐私信息）。
///
/// D-66：改为委托 core 的 `resync::device_display_name`（`<平台>-<8 位短 ID>`），
/// 让「配对时通告给对端的名字」与「重同步 HELLO 里的 client_name」**同源同格式**，
/// 否则对端设备列表里的本机名仍是旧的无平台前缀形态。
fn host_display_name(device_id: &str) -> String {
    resync::device_display_name(device_id)
}

/// 在协议端口范围内选取随机端口。
///
/// D-108（豆包审查 3.4-13）：RNG 失败不再退化到固定端口 49152——可预测端口
/// 让同网段的恶意进程可以抢先占位/预测监听点；失败就该让配对显式报错。
fn random_port() -> IpcResult<u16> {
    let span = (SYNC_PORT_MAX - SYNC_PORT_MIN) as u32 + 1;
    let bytes = rng::random_bytes(4)?;
    let v = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Ok(SYNC_PORT_MIN + (v % span) as u16)
}

/// 清空当前配对会话（worker 收场路径共用）。
///
/// D-108（豆包审查 3.4-9）：原实现 `if let Ok(mut g) = ...` 中毒即静默跳过——
/// `SyncServer` 无人清理变成僵尸监听（端口持续接受连接）。改为 `into_inner()`
/// 兜底照常清空并留痕。
fn clear_pairing_state(app: &tauri::AppHandle, reason: &str) {
    use tauri::Manager;
    let state = app.state::<AppState>();
    // match 以分号结尾：让 lock() 的临时值在 state drop 之前释放（E0597，同 state.rs 口径）。
    match state.pairing.lock() {
        Ok(mut guard) => *guard = None,
        Err(poisoned) => {
            *poisoned.into_inner() = None;
            crate::state::append_log(
                app,
                &format!("[pairing] clear state poisoned ({reason}); cleared via into_inner"),
            );
        }
    };
}

/// 后台轮询线程：非阻塞等待 Joiner 连接，处理一次配对 + 同步后退出。
///
/// **O2 三段式**：`poll_accept` 不持 session 锁；连接建立后由 `SessionAccess` 在
/// 「读写库的瞬间」短暂取锁（`handle_connection` 内部），握手与传输全程锁外——
/// 配对等待期间 UI 的一切 IPC（列表/复制/自动锁定）不再排队等待。
/// 会话中途被自动锁定 ⇒ `handle_connection` 锁内段失败 ⇒ 配对会话作废（对端报错），
/// 不再出现旧实现的「二维码还挂着但 worker 已退出」悬死态。
fn spawn_pairing_worker(app: tauri::AppHandle) {
    use std::sync::atomic::Ordering;
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));

        let state = app.state::<AppState>();

        // O2：accept 不持 session 锁（pairing 锁内只做存续检查 + 非阻塞接受，毫秒级）。
        let accepted = {
            let mut guard = match state.pairing.lock() {
                Ok(g) => g,
                // D-91（审查 C5）：配对会话锁中毒留痕（与 resync worker 同口径）。
                Err(_) => {
                    crate::state::append_log(
                        &app,
                        "[pairing] worker exiting: pairing lock poisoned",
                    );
                    break;
                }
            };
            match guard.as_mut() {
                Some(server) => match server.poll_accept() {
                    Ok(a) => a,
                    Err(e) => {
                        drop(guard);
                        crate::state::append_log(&app, &format!("[pairing] accept failed: {e:?}"));
                        continue;
                    }
                },
                None => break,
            }
        };
        let Some((stream, peer_ip)) = accepted else {
            continue;
        };

        // O2：同一时刻仅一场同步（与被动 worker / 主动 sync_now 互斥，防 seq/base 竞态）。
        if state
            .sync_busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            crate::state::append_log(&app, "[pairing] skipped: another sync is in flight");
            continue;
        }

        let outcome = {
            let mut guard = match state.pairing.lock() {
                Ok(g) => g,
                Err(_) => {
                    state.sync_busy.store(false, Ordering::SeqCst);
                    crate::state::append_log(
                        &app,
                        "[pairing] worker exiting: pairing lock poisoned",
                    );
                    break;
                }
            };
            match guard.as_mut() {
                Some(server) => {
                    let mut access = crate::state::SessionAccess {
                        session: &state.session,
                    };
                    server.handle_connection(stream, &peer_ip, now_ms(), &mut access)
                }
                None => {
                    state.sync_busy.store(false, Ordering::SeqCst);
                    break;
                }
            }
        };
        state.sync_busy.store(false, Ordering::SeqCst);

        match outcome {
            Ok(result) => {
                eprintln!(
                    "[pairing] host session completed: applied={}",
                    result.applied
                );
                crate::state::append_log(
                    &app,
                    &format!(
                        "[pairing] host session completed: applied={}",
                        result.applied
                    ),
                );
                clear_pairing_state(&app, "session completed");
                // D-44：通知前端会话已结束，收回二维码（否则"二维码还挂着但会话已死"）。
                let _ = tauri::Emitter::emit(&app, "pairing://ended", "completed");
                break;
            }
            Err(e) => {
                // 诊断留痕：CoreError 的 Debug 只有错误码（Io 额外带 stage|kind），无隐私载荷。落盘到数据目录 passwordms.log。
                eprintln!("[pairing] host session aborted: {e:?}");
                crate::state::append_log(&app, &format!("[pairing] host session aborted: {e:?}"));
                // D-108（历史 S6）：错令牌 / 重放等**连接级**失败不作废配对会话——
                // REJECT 已由 server.rs 发给对端，二维码与核对码保持有效，正确用户
                // 无须重新生成。原实现收任意 Err 即清会话 + break，一次错令牌连接
                // 就逼用户重开二维码。
                if matches!(
                    e,
                    passwordms_core::CoreError::PairTokenInvalid
                        | passwordms_core::CoreError::PairTokenExpired
                        | passwordms_core::CoreError::PairTokenUsed
                        | passwordms_core::CoreError::FingerprintMismatch
                ) {
                    crate::state::append_log(&app, "[pairing] rejected a peer; session kept");
                    continue;
                }
                clear_pairing_state(&app, "session aborted");
                let _ = tauri::Emitter::emit(&app, "pairing://ended", "aborted");
                break;
            }
        }
    });
}
