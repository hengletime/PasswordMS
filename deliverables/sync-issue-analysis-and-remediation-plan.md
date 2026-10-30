# PasswordMS 局域网同步故障分析与处理方案

> 面向 WorkBuddy 的协查与执行文档  
> 日期：2026-10-30  
> 输入依据：`sync-issue-brief-for-kimi.md`、当前仓库源码及 `v1.4.12...v1.4.18` 同步路径差异  
> 目标：先恢复安全配对，再定位 Android 接收路径，最后解决局域网同步的产品可靠性问题

---

## 0. 数据保护约束（最高优先级）

1. **不得删除、重建或覆盖电脑端保险库。**
2. 开始任何修复前，完整备份电脑便携目录，至少包括：
   - `passwordms.vault`
   - `passwordms.device`
   - `passwordms.identity`
   - `biometric.json`
   - `prefs.json`
   - `resync.port`
   - `passwordms.log`
3. 不得把“删除身份文件”“清空配对记录”“再次卸载手机端”作为默认排障手段。
4. 手机旧库已清空，当前可信数据源是电脑端库。配对恢复后，先确认电脑数据完整同步到手机，再做其他破坏性操作。
5. 诊断日志不得记录主密码、VK、sync_key、完整配对 token、条目明文或附件明文。

---

## 1. 核心结论

这组现象应按**两个独立问题**处理，而不是认定为一个同步协议回归：

### 问题一：1.4.18 电脑→手机同步失败

现有证据只能确定：

- 电脑连接配对记录中的手机静态地址超时；
- UDP 广播没有发现有效手机应答；
- 收到的 25 个包均为电脑自身应答；
- 不是已证实的 sync_key 不匹配；
- 不是由 `49152-65535` 端口区间校验导致，因为代码没有把区间外端口当作非法地址；
- `v1.4.12...v1.4.18` 同步路径主要是诊断日志、延长发现和多次重播，没有证据表明新版引入了对应行为回退。

尚需区分：

1. 手机端监听器未运行；
2. 手机 IP 或监听端口已经变化；
3. 手机应用未真正处于前台解锁状态；
4. UDP 47823 未绑定成功；
5. MulticastLock 未实际持有；
6. Android/OEM 冻结或限制应用网络；
7. AP/路由器过滤客户端间 TCP、UDP 或广播。

### 问题二：双端 1.4.12 重新配对失败

旧身份和旧设备记录不是首要嫌疑。手机卸载重装后，必须先：

> **新建空保险库 → 使用主密码解锁 → 保持应用前台 → 扫描电脑端新生成的二维码。**

如果上述前提已经满足，则优先检查配对 TCP 地址、一次性配对端口、Windows 入站规则和二维码生命周期。

### 两个需要纠正的判断

- **Ping 通不能排除 AP 隔离。** Ping 只证明 ICMP 可达，TCP、UDP、广播仍可能被单独过滤。
- **配对不使用 UDP 广播发现。** 回退到没有 MulticastLock 的 1.4.12，本身不会导致配对 TCP 失败。

---

## 2. 配对机制与旧身份记录结论

### 2.1 配对前置条件

加入配对前，代码会：

1. 启动本机重同步监听器；
2. 通过 `session.repo()` 取得本机 `device_id`；
3. 完成一次性 token、ECDH 和双向确认；
4. 仅在本地库为空时采用 Host 下发的 VK；
5. 持久化配对设备并执行首次同步。

关键位置：

- `src-tauri/src/commands/sync_cmd.rs:114-173`
- `crates/passwordms-core/src/sync/client.rs:281-397`
- `crates/passwordms-core/src/session.rs:360-388`

以下状态会导致失败：

- 手机尚未创建保险库；
- 任一端未解锁；
- 手机使用生物识别解锁，当前会话没有 KEK；
- 手机新库已经新增或导入数据；
- 配对过程中发生自动锁定。

### 2.2 旧设备记录不会自然阻止新手机配对

当前模型中：

- 手机正常卸载重装后会生成新的 `.device`；
- 电脑按新的 `joiner_id` 新增 `PairedDevice`；
- 如果 ID 相同，完整重新配对会更新设备记录，并重置该配对通道的接收序号基线；
- 配对首先验证一次性 token 和本次 ECDH 握手，不检查旧手机是否仍然在线；
- `.identity` 是电脑自己的 Host 长期身份，不是允许连接的手机名单。

关键位置：

- `crates/passwordms-core/src/sync/server.rs:305-350`
- `crates/passwordms-core/src/sync/server.rs:489-510`

因此：**不要为了恢复配对删除电脑端 `.device`、`.identity` 或保险库。**

### 2.3 身份复用的真实风险边界

需要避免的是异常手工操作，例如：

- 把旧 `.device` 复制到另一份不同 VK 的库；
- 只恢复数据目录中的一部分文件；
- 让同一个设备 ID 对应另一份不相容的保险库。

普通卸载重装生成新设备 ID，不属于上述情况。

---

## 3. 配对失败的错误分类

拿到具体错误后，优先按下表定位，不要统一归为“网络问题”。

| 报错或表现 | 优先判断 | 下一步取证 |
|---|---|---|
| `NO_VAULT`／仍停留创建页 | 手机尚未建库 | 创建空库并用主密码解锁 |
| `NOT_UNLOCKED` | 任一端未解锁，或配对中自动锁定 | 记录两端锁定时间、自动锁定档位 |
| `NEED_MASTER_PASSWORD` | 生物识别会话没有 KEK | 使用主密码重新解锁后复测 |
| `VK_ADOPT_REQUIRES_EMPTY` | 手机本地库非空 | 检查是否新增、导入或残留条目 |
| `PAIR_TOKEN_EXPIRED` | 二维码过期或系统时间异常 | 重新生成二维码，核对两端时间 |
| `PAIR_TOKEN_USED` | 二维码已使用 | 重新生成，不复用旧截图或文本 |
| `PAIR_TOKEN_INVALID` | 配对文本损坏或不属于当前会话 | 核对扫码/粘贴内容和当前 Host 会话 |
| 连接超时／网络不可达 | IP、配对端口、防火墙或网络过滤 | 核对二维码地址并抓取配对日志 |
| Connection refused | 配对监听未启动、已结束或端口错误 | 检查 Host 是否仍显示当前二维码 |
| `FINGERPRINT_MISMATCH` | 握手报文、版本或协议状态异常 | 保留完整日志，不归因于旧设备记录 |

当前仍缺少三项关键证据：

1. 配对失败的原文提示；
2. 哪一端显示二维码、哪一端扫码；
3. 手机是否已新建空库并用主密码解锁。

---

## 4. Android 广播与 TCP 接收分析

### 4.1 MulticastLock 是必要条件，不是充分条件

当前实现：

- Manifest 声明 `CHANGE_WIFI_MULTICAST_STATE` 和 `ACCESS_WIFI_STATE`；
- `MainActivity.onResume()` 获取 MulticastLock；
- `onPause()` 释放；
- Rust 发现应答器绑定 `0.0.0.0:47823`；
- 只有保险库解锁期间，轮询线程才处理发现请求。

关键位置：

- `src-tauri/gen/android/app/src/main/AndroidManifest.xml`
- `src-tauri/gen/android/app/src/main/java/com/passwordms/app/MainActivity.kt:48-80`
- `crates/passwordms-core/src/sync/resync.rs:559-613`
- `src-tauri/src/state.rs:369-419`

前台接收 `255.255.255.255:47823` 需要同时满足：

> 权限有效＋MulticastLock 已持有＋应用前台＋保险库解锁＋UDP 绑定成功＋同一 Wi-Fi＋AP 允许广播。

MulticastLock 不会保证：

- TCP 监听器仍在运行；
- UDP socket 已成功绑定；
- 应用未被 OEM 冻结；
- AP 转发广播；
- 息屏或 Doze 时及时收包；
- 配对记录中的旧 TCP 端口仍有效。

### 4.2 对“按 UID 静默丢 TCP＋UDP”的判断

“应用明确前台、进程存活、socket 已绑定，但所有 TCP/UDP 都按 UID 静默丢弃，仅 ICMP 可达”不是标准 Android 12 行为，不能在没有系统证据时直接认定为华为/小米的普遍机制。

当前优先怀疑顺序：

1. 测试时监听器未运行或端口已改变；
2. 应用不是实际前台解锁状态；
3. OEM 冻结或回收了应用执行环境；
4. AP/路由器过滤广播或客户端间流量；
5. 使用了旧的 `192.168.1.102:46879`，而手机地址已变化。

手机锁屏/休眠时的全端口静默扫描，不能证明解锁前台时也发生 UID 丢包，因为锁定后应用本来就会停止监听。

---

## 5. ADB 取证方案

以下命令均为只读诊断，不修改系统设置。

### 5.1 应用、UID、权限和实际 IP

```bash
adb shell pm list packages -U com.passwordms.app
adb shell dumpsys package com.passwordms.app
adb shell ip addr show wlan0
adb shell ip route
```

核对：

- 实际版本和 UID；
- `INTERNET`；
- `CHANGE_WIFI_MULTICAST_STATE`；
- `ACCESS_WIFI_STATE`；
- 手机当前 IP 是否仍为配对记录中的地址。

### 5.2 MulticastLock、Doze 和网络策略

```bash
adb shell dumpsys wifi
adb shell dumpsys connectivity
adb shell dumpsys netpolicy
adb shell dumpsys deviceidle
adb shell dumpsys deviceidle whitelist
adb shell am get-standby-bucket com.passwordms.app
adb shell am get-inactive com.passwordms.app
adb shell appops get com.passwordms.app RUN_IN_BACKGROUND
adb shell appops get com.passwordms.app RUN_ANY_IN_BACKGROUND
```

重点检查：

- `MulticastLock` 或 `passwordms-lan-discovery`；
- 应用 UID 是否处于 Doze、Power Save 或后台限制规则；
- 是否处于 `IDLE`、受限 bucket 或后台冻结状态。

电池优化白名单只能作为后续 A/B 实验，不应在未取证前当成确定修复。

### 5.3 TCP/UDP socket 是否真实存在

```bash
adb shell cat /proc/net/tcp
adb shell cat /proc/net/tcp6
adb shell cat /proc/net/udp
adb shell cat /proc/net/udp6
```

部分设备可尝试：

```bash
adb shell ss -lntup
```

核对：

- 手机当前 TCP 监听端口；
- UDP 47823 是否绑定；
- socket UID 是否与应用 UID 一致。

优先使用 1.4.18 的 `sync_net_info` 获取当前监听端口和 `discovery_enabled`，不要继续根据旧日志猜端口。

### 5.4 应用和系统异常

```bash
adb logcat -v threadtime
```

重点保留：

- `SecurityException`
- `EACCES`
- `Permission Denied`
- `SocketException`
- `AndroidRuntime`
- Tauri/Rust 输出
- `sync_join`
- `resync`
- `discovery`

当前 MulticastLock 获取失败基本静默处理，因此“没有异常日志”不等于“锁已成功持有”。

### 5.5 TCP 与 UDP 分开验证

手机保持前台、解锁，电脑执行：

```powershell
Test-NetConnection <手机当前IP> -Port <手机当前监听端口>
```

结果解释：

| 结果 | 结论方向 |
|---|---|
| `TcpTestSucceeded: True` | 手机 TCP 接收路径可用，继续查握手或同步状态 |
| 超时且手机无监听 socket | 应用监听器未启动或已退出 |
| 超时但手机确有监听 socket | 查 Android/OEM、AP 或中间网络过滤 |
| Connection refused | 端口错误或对端没有监听，通常不是防火墙静默丢包 |

电脑侧 Wireshark 过滤：

```text
udp.port == 47823
```

用于区分：

1. 请求未离开电脑；
2. 请求已发出但手机无应答；
3. 手机已应答但应答未回到电脑。

Android 上直接运行 `tcpdump` 通常需要 root 或额外权限，不作为普通 ADB 环境的默认步骤。

---

## 6. 立即恢复配对的执行顺序

1. 完整备份电脑便携目录。
2. 保留电脑旧手机记录和全部身份文件。
3. 电脑作为 Host，保持解锁并生成新的配对二维码。
4. 手机新建空库，使用主密码解锁，保持应用前台。
5. 两端连接同一非访客 Wi-Fi，暂时排除 VPN、代理影响。
6. 手机立即扫描新二维码。
7. 记录完整错误文案、发生时间和所处阶段。
8. 如果报网络错误，核对二维码中的**一次性配对端口**，不要把 38931、46879 或 UDP 47823 当作配对端口。
9. 配对成功后，先确认电脑数据完整同步到手机，再清理旧设备记录。
10. 后续验证广播发现时，双端使用同一带诊断和 MulticastLock 的版本；回退 1.4.12 对广播发现没有帮助。
11. 如果静态直连恢复而广播仍失败，将其作为独立发现兼容性问题处理，不再重新配对或清空保险库。

---

## 7. WorkBuddy 分阶段处理任务

### P0：先补可观测性，不盲改协议

目标：下一次失败能够直接判断是“未监听、未持锁、未收到、未匹配还是未应答”。

建议在 Android/同步路径增加脱敏诊断：

1. `MainActivity`：
   - `onResume`/`onPause`；
   - MulticastLock 获取成功、失败及 `isHeld`；
   - 异常类型，不记录网络敏感载荷。
2. `ensure_resync_listener`：
   - 实际 TCP 绑定端口；
   - 重绑定事件；
   - UDP 47823 绑定成功或失败原因。
3. `DiscoveryResponder`：
   - 收到请求计数；
   - JSON/版本/形状失败计数；
   - 未匹配配对 key 计数；
   - 应答发送成功或失败计数。
4. `ResyncServer`：
   - 接受到 TCP 连接；
   - 握手拒绝阶段及错误码；
   - 锁定状态导致的拒绝。

验收标准：

- 一次电脑→手机失败后，手机日志能明确区分：
  - Activity 非前台；
  - 未持有 MulticastLock；
  - TCP 未监听；
  - UDP 未绑定；
  - 未收到广播；
  - 收到但 key 不匹配；
  - 已发送应答但电脑未收到；
  - TCP 已接受但握手/同步失败。

### P0：恢复配对流程的前置状态提示

在 UI/命令错误呈现中明确区分：

- 未建库；
- 未解锁；
- 生物识别会话需要主密码；
- 本地库非空；
- 二维码过期/已用/无效；
- 配对 TCP 连接失败；
- 配对过程中自动锁定。

验收标准：用户不再只看到笼统的网络或 IO 错误，能够知道下一步该检查哪一个前提。

### P1：广播之外增加可用路径

广播不是所有局域网都可靠，建议按以下优先级处理：

1. 当前真实地址的 TCP 直连；
2. 向上次手机 IP 的 UDP 47823 发送**单播发现请求**；
3. 提供手动核对或更新对端 IP/端口的入口；
4. 最后才要求重新扫码配对。

代码已有 `DiscoveryMode::Unicast`，可复用现有带 HMAC 的发现协议，不需要发明新的无认证发现报文。

验收标准：

- AP 禁止广播但允许单播时，仍能通过上次 IP 找到手机；
- 手机端口变化时，发现返回的新端口可用于重连；
- 错误应答、错误 key、过期地址不能覆盖有效配对地址；
- 只有认证且同步成功的路径才能持久化地址变更。

### P1：同步页显示真实接收状态

手机端同步页至少显示：

- 当前本机 IP；
- 当前 TCP 监听端口；
- UDP 47823 是否绑定；
- MulticastLock 是否持有；
- 是否前台、是否解锁；
- 配对记录中的对端地址与本机实际地址是否一致。

验收标准：电脑发起同步前，用户能直接确认手机是否具备接收条件。

### P2：明确产品同步模型

当前“解锁才监听＋自动锁定即停止”意味着：

> 电脑随时主动发起同步天然不可靠，这是安全模型结果，不完全是网络缺陷。

近期应明确手动同步前提：

- 两端均解锁；
- Android 保持应用前台；
- 必要时由用户主动开启时间受限的“接收同步”状态。

如果产品要求真正后台随时同步，则需要：

- 前台服务及持续通知；
- 相应后台运行和网络条件；
- 后台可使用的同步密钥。

这与“锁定后无法取得 VK”的当前零知识模型存在直接冲突，不能只靠 MulticastLock 或电池白名单解决。

WorkManager 不适合维持实时 TCP/UDP 监听。

Syncthing、KDE Connect 的后台服务模型不能直接照搬：它们的设备密钥可供后台服务使用，没有 PasswordMS“保险库锁定即失去解密能力”的等价约束。

---

## 8. 建议的回归测试矩阵

### 配对

1. 手机无库时加入，给出明确提示；
2. 手机空库＋主密码解锁，配对成功；
3. 手机非空异组库，拒绝采用 Host VK；
4. 生物识别会话，提示需要主密码；
5. 二维码过期、已用、无效分别可区分；
6. 电脑保留旧手机记录，新手机可并存配对；
7. 同设备 ID 完整重配对，不出现旧 replay 基线误判；
8. 配对中自动锁定，失败且状态可恢复。

### Android 网络

1. 前台＋解锁＋持锁，TCP 直连成功；
2. 前台＋解锁＋持锁，广播发现成功；
3. 锁定后 TCP/UDP 均停止，UI 状态一致；
4. `onPause` 后释放锁，`onResume` 后重新获取；
5. UDP 47823 被占用时明确显示发现不可用，但 TCP 直连仍可用；
6. AP 禁止广播时，单播发现可用；
7. OEM 后台限制只影响后台，不应被误判为协议失败；
8. 手机换 IP、换端口后能够经发现或手动入口恢复；
9. 旧地址连接失败不会破坏现有配对记录；
10. 新手机重装后不会要求清除电脑 Host 身份。

### 数据安全

1. 所有失败路径不修改 VK；
2. 配对失败不删除本地条目；
3. 地址自愈仅在认证和同步成功后落盘；
4. 日志不包含秘密值或业务明文；
5. 电脑库始终是本次恢复过程中的受保护数据源。

---

## 9. 不建议采取的处理方式

- 继续盲目回退或升级版本，而不记录具体错误阶段；
- 把 `46879` 等区间外端口当成非法端口；
- 把 Ping 通当作排除 AP 隔离的证据；
- 把 MulticastLock 当作后台保活或 TCP 监听保证；
- 未备份就删除电脑端身份、设备或保险库文件；
- 通过永久关闭自动锁定来掩盖同步模型问题；
- 用无认证明文广播替代现有 HMAC 发现协议；
- 将电池优化白名单当作所有 ROM 的统一修复；
- 在没有手机端日志前直接认定是厂商按 UID 丢包。

---

## 10. 最终执行优先级

1. **备份电脑数据。**
2. **按正确前置状态恢复新手机配对，并取得准确错误文案。**
3. **补 Android 监听、锁、UDP 收发包和握手阶段诊断。**
4. **分离验证 TCP 静态直连与 UDP 广播。**
5. **增加单播发现和手动地址恢复路径。**
6. **明确手动同步需要双端解锁、Android 前台的产品约束。**
7. 只有在确实要求后台随时同步时，才单独评估前台服务与密钥可用性的安全模型变更。
