# PasswordMS 局域网同步故障：代码级分析与解决方案

- 分析日期：2026-10-30
- 输入：`deliverables/sync-issue-brief-for-kimi.md`（workbuddy 取证简报）
- 方法：对照仓库 main 分支实际代码逐行核实（`sync/server.rs`、`sync/client.rs`、`sync/discovery.rs`、`sync_cmd.rs`、`state.rs`、`MainActivity.kt`、`AndroidManifest.xml`、`constants.rs`），所有结论附代码位置。
- 范围：只分析、给方案，不改代码。

---

## 0. 先纠正两个前提（影响后续所有判断）

1. **配对失败与 UDP 广播 / MulticastLock 无关。** 配对（QR 握手）是 Joiner 扫码后直接 TCP 连 QR 里通告的 `ip:port`（`client.rs::pair` L288-322），全程不经过广播发现。MulticastLock 只服务于「已配对设备换 IP 后的地址自愈」。所以"手机收不到广播"解释不了"重新配对失败"。
2. **电脑端残留的旧手机身份记录不会导致配对被拒绝。** `server.rs::handle_connection` 全程不查设备列表；`persist_device` 是 `upsert_paired_device(device_id)`（`server.rs:489-511`）。新手机重装 = 新建库 = 新 `device_id`（建库时生成的随机 UUID，存于库内），配对时**新增**一条记录，旧记录原样保留。代码里不存在"新旧身份不匹配就 REJECT"的分支。谜 A 的这个怀疑可以排除。

---

## 1. 问题一：双端回退 1.4.12 后重新配对失败——原因排序

先补两个关键事实（brief 里标注"待补充"，是定性前必须拿到的）：

- **F1**：手机端配对失败的完整报错文案（或手机上看到的错误提示原文）。
- **F2**：配对时哪一端展示二维码、哪一端扫码。

以下按嫌疑从高到低排序，每条给出验证方法。

### H1（最可能）：Windows 防火墙入站规则的 Profile 范围与当前"公用网络"不匹配

**代码/事实链**：

- 配对时若电脑展示 QR（Host），手机要主动连电脑。电脑侧配对监听器绑定随机端口（1.4.12 为 `bind(0)`，main 分支为 49152–65535 区间随机，`sync_cmd.rs:449-454`）。
- Windows 防火墙按 exe 路径放行，规则在 10-24 首次 bind 时创建。Windows 创建入站规则时会绑定当时的网络 Profile（专用/公用/域）。
- **静默丢包、无 RST、4s 超时**正是 Windows 防火墙丢弃 SYN 的典型特征——brief 中 `discovery: timeout ... self=25 → 无应答` 和 `connect 失败（4s 超时）`都是这个指纹。
- 时间线吻合：10-29 14:36 最后一次入站成功时规则还生效；10-30 白天失败。中间发生了什么？brief 记载"1.2.14 旧安装版的 AppData 目录创建于 10-29 19:44"——晚间装过另一个安装包/改动过网络配置；且当前网络 Profile 已是 Public。规则很可能只在"专用" Profile 下生效。

**验证（电脑 PowerShell）**：

```powershell
# 当前网络 Profile
Get-NetConnectionProfile
# 找到 PasswordMS 规则，看它的 Profile 范围
Get-NetFirewallRule -DisplayName "*assword*" | Select-Object DisplayName,Enabled,Profile,Direction
```

若规则 `Profile=Private`（或 Private+Domain，不含 Public）而当前网络是 Public，即确诊。

**修复**（Profile 放开到 Any，或重建）：

```powershell
New-NetFirewallRule -DisplayName "PasswordMS" -Direction Inbound -Action Allow `
  -Program "D:\Tools\PasswordMS-Portable\passwordms.exe" -Profile Any
```

### H2：手机端配对前置条件不满足（未解锁新库 / U1-U2 拦截 / QR 过期）

- **未解锁**：`sync_join_impl` 第一步就是 `state.lock_session()`（`sync_cmd.rs:127`）。手机卸载重装后必须先**创建新库并解锁**，否则命令直接 NOT_UNLOCKED 失败。
- **U1/U2 前端拦截**：手机新库若不是空的（建库时手输了任何测试条目），前端会弹窗拒绝 adopt 电脑端 VK（`sync_cmd.rs:149-157` 的豁免条件：未与该 Host 配对过且 VK 不相等 → 核心 adopt 后由前端提示）。用户容易把这个弹窗读成"配对失败"。
- **QR 120 秒过期**：`PAIR_TOKEN_TTL_SECS = 120`（`constants.rs:145`）。用户在手机上忙建库、找扫码入口，两分钟后扫码 → 手机收到 `ERR_PAIR_TOKEN_EXPIRED` REJECT（`server.rs:245-248`）。

**验证**：F1 报错文案。出现 `pair_token_expired` / `not_unlocked` / 前端弹窗文案即可定性。

### H3：QR 主选 IP 是手机路由不到的虚拟网卡地址

- `local_ipv4s()` 枚举全部网卡，主选排在 `ips[0]`（`sync_cmd.rs:63`、`server.rs:140-156`）。WSL2/Hyper-V 虚拟网卡（172.x、169.254）若排前，手机先连它 4s 超时；代码会逐个候选 fallback（`client.rs:295-318`），但每个候选 4s，3~5 个候选累计 12–20s，容易撞上 QR 120s TTL，或被用户当成卡死。

**验证**：配对界面/QR 数据里展示的 IP 列表顺序；`sync_net_info` 诊断命令（`sync_cmd.rs:314-322`）。

### H4：方向反了——手机展示 QR、电脑扫码

- 若手机是 Host，电脑连手机：Android 默认无主机防火墙，理论上应通。但 QR 里手机自报的 IP 由 `local_ipv4s()` 在 Android 上枚举，可能误带蜂窝接口/127.0.0.1；且手机 App 必须全程前台亮屏。
- F2 补上即可排除。

### 已排除的项

- 协议版本不匹配：两端都回退到 1.4.12，同版本，不成立。
- 旧设备记录拦截：见 §0 第 2 条，不成立。
- MulticastLock 缺失：配对不走广播，见 §0 第 1 条，不成立。

---

## 2. 问题二：Android 端 MulticastLock 够不够 / 厂商 ROM 静默丢包 / adb 取证

### 2.1 MulticastLock 本身：实现正确，是必要不充分条件

`MainActivity.kt` 的实现（`onResume` acquire / `onPause` release、`setReferenceCounted(false)`、权限 `CHANGE_WIFI_MULTICAST_STATE` 已在 `AndroidManifest.xml:11` 声明）是教科书式正确做法。它解决的是唯一一件事：**WiFi 芯片是否把广播包交给 App**。

它不解决：

1. **锁屏即 onPause → 锁释放**（设计如此，省电）。意味着锁屏后手机既不收广播、TCP 监听也随 lock 死掉（见 §3）。
2. **Doze / OEM 省电按 UID 静默丢包**。这正是 1.4.18 白天 12 连败（TCP 静默超时 + UDP 零应答）的头号嫌疑。用户机器用户名 HUAWEI——若手机是华为/荣耀 EMUI/MagicOS，"电池优化/应用启动管理"会后台冻结网络：ICMP 例外放行（所以 ping 通），TCP/UDP 按 UID 静默丢弃（连 RST 都不回）。
3. 还有一个未验证假设：12 次失败时手机是否真的"解锁+前台+亮屏"。brief 只记录了"10-30 上午扫描时手机锁屏/休眠"那次（那次静默完全正常），白天 12 次失败时的手机状态未取证。

### 2.2 adb 取证命令清单

```bash
# ① App 是否在跑、UID
adb shell dumpsys package com.passwordms.app | findstr userId

# ② Doze 白名单（手机端 App 不在白名单 = 锁屏后网络冻结）
adb shell dumpsys deviceidle whitelist
adb shell dumpsys deviceidle get deep

# ③ 网络策略：该 UID 是否被限制后台网络
adb shell dumpsys netpolicy

# ④ logcat 抓配对/同步窗口（在手机前台开着 App 时执行）
adb logcat -c
adb logcat | findstr /i "passwordms multicastlock networkpolicy connectivity"

# ⑤ 手机前台解锁状态下，看监听器端口是否真的 LISTEN
adb shell netstat -an | findstr LISTEN

# ⑥ 对照：电脑 telnet 手机当前 IP:监听端口，看前台时通不通
```

### 2.3 判定树

| 手机前台亮屏时电脑能否连通手机监听端口 | 结论 |
|---|---|
| 通 | 网络层 OK；问题是锁屏/后台后的保活策略 → 走 §3 的架构改造 |
| 不通 | 手机监听器根本没起（unlock 后 `ensure_resync_listener` 未在 Android 被触发）——这是产品缺口：**手机端无日志设施**，建议 Android 侧补 app 私有目录文件日志 |

### 2.4 手机侧手动补救（用户立即可做）

- 华为：手机管家 → 应用启动管理 → PasswordMS → 改为"手动管理"，全部允许（尤其后台活动）。
- 设置 → 电池 → 应用启动管理/电池优化 → PasswordMS → 不限制。
- 配对与同步全程：App 保持前台、屏幕常亮。

---

## 3. 问题三：「必须解锁才应答 + 自动锁定」组合是否注定让电脑发起的同步不可靠？

**结论：在当前架构下，"电脑发起"本质就是尽力而为，不可靠是设计内生的；"手机发起"才是主路径。**

代码事实链：

1. 重同步监听器（TCP + UDP 发现应答）在 `ensure_resync_listener` 解锁期间启动（`state.rs:266`），`lock` 时 `stop_resync_listener` 整个销毁（`state.rs:327-342`）；UDP 发现应答 `poll_discovery_once` 跑在同一个 resync worker 里（`state.rs:410-414`）。
2. 电脑 `sync_now` = 静态地址 4s 超时 → 广播发现等 2.5s（`sync_cmd.rs:268-274`）。手机一锁：SYN 打到死端口（Android 侧要么 RST 要么 OEM 静默丢），UDP responder 也死了 → 零应答。
3. 这正好解释了观测：10-24~10-29 的 20+ 次成功**全部是手机发起**——用户手动拿起手机、开 App、点同步，此时手机必然解锁前台、listener 活着、静态直连电脑 38931 一次就通。

### 业界通行做法（Syncthing / KDE Connect）

| 手段 | 说明 | 对本项目的适用性 |
|---|---|---|
| 前台常驻 Foreground Service | TCP 监听 + UDP 应答挪进常驻 Service，与 Activity 生命周期解耦；挂常驻通知，用户划掉 App 才停 | **最值得做**：手机开着 App 就一直可应答，不因 onPause 死 |
| 监听活着、vault 锁定时明确回 `locked` 拒绝 | 对端拿到明确错误而不是静默 4s 超时 | **必做的产品改进**：UI 提示"手机未解锁"，用户不再瞎猜 |
| 电池优化白名单引导 | 首次启动弹窗引导用户加白 | 低成本，立即缓解 OEM 丢包 |
| FCM 高优先级推送唤醒 | 锁屏也能唤醒 App 应答 | **不建议**：需要服务器，违反"零服务器局域网"架构红线 |

### 产品层面建议

- UI 引导改为"请在手机上点同步"（已验证可靠的方向），电脑发起降级为便利项。
- 电脑发起失败时 toast 明确区分"手机离线/未解锁"与"网络错误"，而不是统一一句 `network_unreachable`。

---

## 4. 问题四：配对身份模型——旧手机记录残留时新手机扫码会不会冲突？

**代码事实（已核实）**：

- Host 端 `upsert_paired_device` 按 `device_id` upsert。新手机 = 新库 = 新 `device_id`（建库随机生成，存于 vault）→ **新增记录，旧记录不被覆盖、不被拒绝**（`server.rs:489-511`、`sync_cmd.rs:208-212`）。
- Joiner 端豁免逻辑（`sync_cmd.rs:150-157`）："删除配对设备后重扫同一 Host"允许 adopt（D-62 故意设计）。
- 旧记录无害：`discovery.rs::match_paired_device` 遍历所有已配对设备的 sync_key 校验 tag，旧手机的 key 只是永远等不到匹配请求。

**真实存在的身份陷阱（都在 UI 层，不在协议层）**：

1. **幽灵设备累积**：旧手机丢失/卸载后，电脑端设备列表里那条记录永远留着，除非手动 `unpair`。建议设备列表按 `last_sync_at` 提示"30 天未同步，是否移除"。
2. **手机重装后电脑端认不出"同一台手机"**：device_id 变了，配对后列表里出现两台手机。建议 UI 按设备名 + 最近同步时间辅助清理。
3. **HostIdentity 文件**：电脑长期身份只存在本机数据目录（`.identity`，VK 包裹存储，D-96），与配对设备列表无关，无复用风险。

---

## 5. 立即可执行的排查顺序（行动清单）

1. **补两个事实**：①手机端配对失败的完整报错文案；②哪端展示 QR。
2. **电脑防火墙**：跑 §1-H1 两条 PowerShell 命令，确认规则 Profile 范围；若不含 Public，按给出的 `New-NetFirewallRule` 重建。
3. **手机电池白名单**：华为手机管家把 PasswordMS 设为手动管理/无限制。
4. **配对姿势**：电脑展示 QR → 手机先创建并解锁新库（保持空库）→ 扫码；QR 出现后 120 秒内完成。
5. **验证方向**：配对成功后先在手机上点同步（已验证可靠方向），再测电脑发起。
6. 若仍失败：按 §2.2 的 adb 取证清单抓一轮，重点看手机前台时监听端口是否 LISTEN。

---

## 6. 中期改进建议（排期，不在本轮做）

| 优先级 | 项 | 对应问题 |
|---|---|---|
| P1 | 手机端补文件日志（app 私有目录），结束"手机黑屏无法取证" | 问题二 |
| P1 | 电脑发起失败时前端区分"手机未解锁/离线"与"网络错误"文案 | 问题三 |
| P2 | Android 把 TCP 监听 + UDP 应答挪进前台 Foreground Service，与 Activity 解耦 | 问题二/三 |
| P2 | 设备列表幽灵设备提示（last_sync_at > 30 天） | 问题四 |
| P3 | QR 主选 IP 排序把虚拟网卡（172.x/169.254）排到候选末尾 | H3 |
