# PasswordMS 局域网同步故障：完整问题描述（供 AI 协查）

> 生成时间：2026-10-30 16:32 · 所有日志/代码证据均来自实际取证，标注「未知」的地方是还没拿到的数据。

## 0. 一句话现状

两端回退到 v1.4.12 后，**手机与电脑现在连配对都配不上**（手机端卸载重装，保险库与配对身份已清空）；此前 1.4.18 双端时，电脑→手机方向同步 12 连败（TCP 直连超时 + UDP 广播发现零应答）。

## 1. 项目与架构要点

- **PasswordMS**：Tauri 2 + Svelte 5 + Rust workspace 零知识密码管理器，Windows（便携版）+ Android 双端。
- 同步机制（纯局域网，无服务器）：
  - **配对**：一方显示二维码（含 ip / sync_port / 设备指纹 fp），另一方扫码，TCP 直连完成握手（XChaCha20-Poly1305 会话密钥 + 双向指纹确认），各自持久化对方地址。
  - **同步**：发起方先连**配对记录的静态地址**（IP:端口，4s 超时），失败则走 **UDP 广播发现**：向本机所有网卡的广播地址发 `255.255.255.255:47823`，应答方（持有同一 sync_key）回单播应答（含自己的 TCP 监听端口，HMAC tag 防伪）。
  - **监听器**：解锁期间常驻，`TcpListener::bind(("0.0.0.0", 0))` 由**系统随机分配端口**，端口号写数据目录 `resync.port`，重启优先复用；锁定即停（自第一版如此）。
  - **地址自愈**：入站同步成功后，接收方把「来源 IP + 对端 HELLO 自报端口」回写进配对记录。
  - **Android 收广播**：需要 `WifiManager.MulticastLock`，否则 WiFi 芯片丢广播包。
- 端口事实：Windows 动态端口范围被 WSL2/Hyper-V 改成 1024–65535；Android 临时端口 32768–60999。代码里 `SYNC_PORT_MIN/MAX = 49152–65535` 只是文档口径，**绑定端口时并没有用这个区间**（就是 bind 0）。

## 2. 时间线（电脑端日志实测，便携目录 D:\Tools\PasswordMS-Portable）

| 时间 | 事件 |
|---|---|
| 10-24 10:42 | 便携版首次建目录；监听端口 rebound → 7615 → 38931（端口被占回退） |
| 10-24 18:27 | 配对成功（applied=8），此后手机→电脑多次同步成功 |
| 10-27～10-29 | 入站同步成功 20 余次（日志全部是 `[resync] completed`，= 手机发起、电脑接收） |
| **10-29 14:36** | **最后一次成功同步**（入站）。电脑记录的手机地址被回写为 `192.168.1.102:46879` |
| 10-30 09:23 | 开发提交 `1b1f602`：Android MainActivity 新增 MulticastLock（onResume 持有 / onPause 释放） |
| 10-30 白天 | 电脑 + 手机都升级到 1.4.18 |
| 10-30 10:36～16:02 | 电脑→手机 `sync_now` **12 次全失败**，模式完全一致（见下） |
| 10-30 16:30 前后 | 主人把两端回退到 v1.4.12；**手机卸载重装 → 保险库、配对身份、resync.port 全部清空** |
| 现在 | **双端 1.4.12 配对失败**（具体报错文案待补充） |

## 3. 1.4.18 时期失败现场（电脑日志原样摘录，12 次全部同模式）

```
[sync] 轨迹: peer: 配对记录地址 192.168.1.102:46879
| connect: 静态地址 192.168.1.102:46879 失败（4s 超时）→ 尝试局域网发现
| discovery: mode=broadcast 开始查找对端
| discovery: start local=0.0.0.0:53542 targets=5 wait=2500ms self_port=Some(38931)
| discovery: 第1轮 广播 sent=5 err=0
| discovery: timeout rounds=5 recv=25 bad_json=0 bad_q=0 bad_tag=0 self=25 → 无应答
[sync_now] failed: err=ERR_IO / err.network_unreachable
```

解读：手机对 TCP **完全静默**（4s 超时=包被丢，不是 RST）；UDP 窗口收到 25 个包**全是电脑自己应答器的回声**（self=25），手机零应答、零 tag 不匹配（bad_tag=0，说明不是密钥对不上，是压根没有包到达电脑）。同网段 ping 实测 2/2 通（39ms）。

## 4. 已核实的事实（代码 diff + 系统取证）

1. **v1.4.12 → 1.4.18 同步路径无行为回退**：`git diff v1.4.12 HEAD` 对 `sync/` 全是诊断日志（trace）+ 发现等待 1.5s→2.5s（放宽）+ 窗口内重播 5 轮（增强）。
2. 「锁定即停监听」从项目第一个提交（f7464c1）就存在，不是新引入。
3. **旧版手机 APK（1.4.12 及更早）没有 MulticastLock** —— 理论上**收不到 UDP 广播**，即「发现」在手机端从未真正通过；此前成功的同步全是**手机发起**（静态直连电脑 38931 + 电脑常驻接收），`46879` 这个记录是入站成功回写的**真实端口**（Android 临时端口区间 32768 起，合法）。
4. Windows 防火墙：开启、默认入站 Block、当前网络 Public；`D:\tools\passwordms-portable\passwordms.exe` 有 Inbound Allow 规则（10-29 手机→电脑入站成功与此相符）。
5. 10-30 上午（手机升级 1.4.18 **之前**）实测扫手机全部 TCP 端口：open=0 refused=0 **silent=16405**——连 RST 都没有，按 UID 静默丢包（当时手机疑似锁屏/休眠）。

## 5. 未解之谜（按嫌疑排序）

- **谜 A（当前卡点）**：双端 1.4.12 重新配对失败。未知项：①失败的报错文案；②哪一方显示二维码、哪一方扫；③手机端是否已重新建库/配对入口在哪一步失败。注意：电脑端 1.4.12 的数据目录里有**旧的配对身份**（.identity/.device）和**旧设备记录**（那台已消失的旧手机身份），新旧身份不匹配会不会导致配对/同步协议拒绝？
- **谜 B**：1.4.18 手机（持有组播锁、解锁、亮屏）为何对 UDP 广播零应答、对 TCP 静默？——同网段 ping 通排除 AP 隔离；防火墙规则已放行电脑侧。是厂商省电策略按 UID 丢包，还是解锁判定/监听器没起来（手机端无日志设施，无法取证）？
- **谜 C**：10-24～10-29 电脑→手机方向有没有成功过？日志无成功记录（旧版成功不打日志），无失败记录。主人记忆是「之前手动同步好多次没问题」——需要确认他当时点的是哪一端。

## 6. 数据现状（重要，别再丢）

- 电脑端库完好：`D:\Tools\PasswordMS-Portable\passwordms.vault`（202,326 B，10-30 13:43）+ .device/.identity/biometric.json/prefs.json/resync.port 五件套。
- 手机端：**已清空**（卸载重装 1.4.12）。手机上原来那份库的数据，只存在于电脑端库的镜像里（最后一次双向同步 10-29 14:36）。
- 回收站取证过：无 `%APPDATA%` 路径的 vault 删除记录；1.2.14 旧安装版的 AppData 目录创建于 10-29 19:44（主人记忆中的旧测试库在 10-23～10-29 间被整体清除过，成因未定，与本次故障无直接关联）。

## 7. 请协查的问题

1. 双端 1.4.12 重新配对失败，最可能的技术原因排序？（结合 §5 谜 A 的未知项，先补报错文案再定性）
2. Android 12+ 上 `MulticastLock`（onResume/onPause 持有）+ `CHANGE_WIFI_MULTICAST_STATE` 权限，是否足以让前台应用收到 `255.255.255.255:47823` 广播？各厂商 ROM（华为/小米等省电策略）有没有已知的「按 UID 静默丢 TCP+UDP、仅放 ICMP」行为？如何用 adb 取证（logcat / dumpsys netpolicy / battery optimization 白名单）？
3. 局域网直连同步产品形态下，手机端「必须解锁才能应答」与「5 分钟自动锁定」的组合是否注定让电脑发起的同步不可靠？业界通行做法（如 Syncthing/KDE Connect）如何在 Android 上保活发现与监听？
4. 配对身份模型：电脑端保留旧手机身份记录时，新手机重新扫码配对是否应被允许覆盖/并存？有没有已知的身份复用陷阱？

## 附：关键代码位置（仓库 github.com/hengletime/PasswordMS，分支 main）

- `crates/passwordms-core/src/sync/discovery.rs` —— UDP 广播发现 + 应答（LABEL_REQ/RESP 域分离、q 随机数防重放）
- `crates/passwordms-core/src/sync/resync.rs` —— 静态直连 → 发现兜底；`ResyncServer::start(0)` 随机端口；入站成功回写地址（~L843）
- `crates/passwordms-core/src/sync/server.rs` —— HELLO 处理、`hello.sync_port==0` 分支（L313）
- `src-tauri/src/state.rs` —— `ensure_resync_listener`（resync.port 复用，L266 起）、锁定即停
- `src-tauri/gen/android/app/src/main/java/com/passwordms/app/MainActivity.kt` —— MulticastLock（1b1f602，10-30 新增）
- 电脑端日志：`D:\Tools\PasswordMS-Portable\passwordms.log`
