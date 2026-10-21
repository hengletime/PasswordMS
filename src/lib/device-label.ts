// 设备行「立即同步」按钮的状态解算（D-66）。
//
// 缺陷回顾（主人真机实测）：`SyncStatus.svelte` 用 store 级**单一**布尔 `sync.busy`
// 控制 `{#each}` 里**所有**设备行 → 点其中一台，「状态」页两台设备同时显示
// 「正在同步…」并同时转圈；用户无法判断到底是哪一台在工作。
//
// 修法：忙碌态从「全局一个布尔」升级为「哪一台正在同步」（`sync.syncingDeviceId`）。
// 本模块把「某一行该显示转圈还是该置灰」抽成纯函数，使行为可被 vitest 直接覆盖
// （不依赖 Svelte 组件渲染）。
//
// 为什么「非当前台也要 disabled」：同步是排他操作（后端 `sync_now` 走同一条直连通道，
// 并发会把冲突箱搅乱），所以整体忙碌期间**所有**同步按钮都不可点；但**只有被点的那台**
// 显示 Spinner + `sync.syncing`，其余台保持 `sync.sync_now` 文案、仅置灰 —— 这样
// 用户一眼能看出「我在等哪一台」。

/** 单个设备行「立即同步」按钮的显示状态。 */
export interface SyncButtonState {
  /** 是否显示 Spinner + `sync.syncing`（仅被点的那台为 true）。 */
  syncing: boolean;
  /** 是否禁用按钮（整体忙碌期间全部禁用，禁止并发重同步）。 */
  disabled: boolean;
}

/** 空闲态（可点、不转圈）。 */
const IDLE: SyncButtonState = { syncing: false, disabled: false };
/** 忙碌但非本机态（不可点、不转圈）。 */
const BLOCKED: SyncButtonState = { syncing: false, disabled: true };
/** 本机正在同步态（不可点、转圈）。 */
const ACTIVE: SyncButtonState = { syncing: true, disabled: true };

/**
 * D-66：「立即同步」按钮的忙碌态按设备隔离。
 *
 * @param syncingDeviceId store 里正在同步的设备 id（null = 全空闲）。
 * @param deviceId 当前行的设备 id。
 * @returns 该行按钮的 `{ syncing, disabled }`。
 *
 * 规则：
 * - 全空闲（`syncingDeviceId` 为 null 或空串）→ `{ syncing: false, disabled: false }`；
 * - 有设备正在同步 → 被点的那台 `{ syncing: true, disabled: true }`，
 *   其它台 `{ syncing: false, disabled: true }`（期间全部禁点，防并发重同步）。
 *
 * 空字符串防御：`deviceId` 为空串时**永不判为命中**——空 id 不是合法设备，
 * 让它跟着转圈只会把「查不到设备」伪装成「正在同步」。
 */
export function syncButtonState(syncingDeviceId: string | null, deviceId: string): SyncButtonState {
  const busy = syncingDeviceId !== null && syncingDeviceId.length > 0;
  if (!busy) {
    return IDLE;
  }
  return deviceId.length > 0 && deviceId === syncingDeviceId ? ACTIVE : BLOCKED;
}

/**
 * D-67 / D-69：设备行「指纹」该显示什么。
 *
 * 缺陷回顾：`DeviceList.svelte` 曾把 `device_id`（36 位 UUID）冒充「设备指纹」显示。
 * 而真指纹来自 `handshake::fingerprint_of()` = **sha256(对端公钥) 前 8 字节的小写 hex（16 位）**，
 * 配对时写入 `PairedDevice.fingerprint`。拿 UUID 冒充等于**安全提示在说谎**：
 * 字长 36、无自校验含义，就算两端真被中间人隔开，用户照着 UUID 也永远「一致」。
 *
 * **D-69 收紧了这里**：原实现带「指纹为空则回退 `device_id`」的分支，理由是
 * `PairedDevice.fingerprint` 带 `#[serde(default)]`、老记录读出来是空串，「不想让行里留白」。
 * 但那个分支复活的恰恰是 D-67 判死的错误行为 —— 空指纹 = **不知道对端公钥**，
 * 此时显示 UUID 等于凭空造一个「看起来很安全」的值。正确做法是**整行不渲染**
 * （由 `hasSessionCode()` 判空，见 `DeviceList.svelte`）。
 *
 * @param fingerprint 真指纹（`DeviceDto.fingerprint`，16 位小写 hex）。
 * @returns 裁掉前后空白后的指纹；缺失时返回空串（调用方**必须**先判 `hasSessionCode()`）。
 *
 * 为什么不再收 `deviceId` 参数：留着它而没有回退分支，等于给后人留一个「反正 id 在手边」
 * 的诱惑 —— 删掉参数，这段代码就**结构上无法**再渲染 `device_id`。`device_id` 仍以
 * `title={device.device_id}` 的形式留在模板里，供悬停排查。
 */
export function fingerprintText(fingerprint: string | null | undefined): string {
  if (typeof fingerprint !== 'string') {
    return '';
  }
  return fingerprint.trim();
}

/**
 * D-69：核对块 / 指纹行的「有没有值可显示」判定。
 *
 * 命名按 D-69 的调用点定（配对两屏显示的是**会话码**）。语义是通用的「非空文本」：
 * 非空 → 渲染；`null` / `undefined` / 空串 / 纯空白 → **整块不渲染**。
 *
 * 为什么必须整块不渲染而不是留白：屏上出现「对端会话码：」后面什么都没有时，
 * 用户会读成「码是空的，那应该没事吧」—— 比不显示更危险。
 * 同样的道理适用于设备列表：空指纹时那一行副信息整体不渲染（设备名 / 配对时间 /
 * 上次同步照常显示），而不是退化成显示 UUID。
 *
 * @param value 待核对的文本（会话码 / 设备指纹均可）。
 * @returns `true` = 有可显示的值。
 */
export function hasSessionCode(value: string | null | undefined): boolean {
  return typeof value === 'string' && value.trim().length > 0;
}
