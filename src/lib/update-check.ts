// D-107：更新检查（方案 B：默认关 + 手动按钮）。
//
// 设计要点（docs/06 §6.21、架构文档 D-107 段）：
// - **只提醒、不下载不安装**：查出新版本后给一句话提示 + 跳转 Releases 页按钮，
//   用户自行下载——不引入自动更新的签名/供应链复杂度；
// - **零 Rust 依赖**：走 WebView `fetch`（CSP `connect-src` 已放行
//   `https://api.github.com`），不加 HTTP crate（守住 §8.1 的 23 个上限与体积门禁）；
// - **隐私**：单向 GET，不携带任何本机信息（连本机版本号都不发送）；
//   自动检查**默认关闭**，关闭时零网络请求（用户协议已披露）；
// - 节流：自动检查每 24h 最多一次（localStorage 记录上次时间）；
// - 请求失败静默返回 null（GitHub 不可达不该打扰用户）。

/** GitHub Releases 最新版 API。 */
export const UPDATE_API_URL = 'https://api.github.com/repos/hengletime/PasswordMS/releases/latest';
/** Releases 下载页（走 open_url 的 http/https 白名单链路打开）。 */
export const UPDATE_RELEASES_URL = 'https://github.com/hengletime/PasswordMS/releases/latest';

/** localStorage：自动检查开关（'1' / 缺省 = 关）。 */
const KEY_ENABLED = 'update_check_enabled';
/** localStorage：上次自动检查的 Unix 毫秒。 */
const KEY_LAST_MS = 'update_check_last_ms';
/** localStorage：上次查到的新版本号（自动检查命中后暂存，供 UI 显示）。 */
const KEY_LATEST = 'update_check_latest';

/** 自动检查的节流间隔：24 小时。 */
export const AUTO_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

/** 一次更新检查的结果。 */
export interface UpdateCheckResult {
  /** 仓库最新版本（不含 v 前缀，如 `1.2.23`）。 */
  latestVersion: string;
  /** true = 有比 `currentVersion` 更新的版本。 */
  hasUpdate: boolean;
}

/** localStorage 存取（测试可注入替身）。 */
export interface UpdateCheckStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * 语义化版本比较（纯函数，有单测）。
 *
 * 支持 `v` 前缀与任意段数（`1.2` vs `1.2.3`：缺段按 0）。非数字段视为 0
 * （容错预发布标记 `1.2.3-beta`：按 1.2.3 处理，对本工具的用途足够）。
 */
export function compareVersions(a: string, b: string): number {
  const pa = a.trim().replace(/^v/i, '').split('.').map((s) => {
    const n = parseInt(s, 10);
    return Number.isNaN(n) ? 0 : n;
  });
  const pb = b.trim().replace(/^v/i, '').split('.').map((s) => {
    const n = parseInt(s, 10);
    return Number.isNaN(n) ? 0 : n;
  });
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i++) {
    const x = pa[i] ?? 0;
    const y = pb[i] ?? 0;
    if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}

/** 一次检查失败的原因分类（D-212）。 */
export type UpdateFailure = 'network' | 'rate_limited' | 'other';

/** 带重试的检查结果。 */
export interface UpdateAttempt {
  /** 成功时的结果；失败为 `null`。 */
  result: UpdateCheckResult | null;
  /** 失败原因；成功为 `null`。 */
  failure: UpdateFailure | null;
  /** 实际发出的请求次数（1..=1+retries）。 */
  attempts: number;
}

/** 默认重试次数（不含首次）与间隔。 */
export const UPDATE_RETRY_TIMES = 2;
export const UPDATE_RETRY_DELAY_MS = 1000;

/** 单次请求的结果：要么拿到版本，要么拿到失败分类。 */
type AttemptOutcome = { result: UpdateCheckResult } | { failure: UpdateFailure };

/**
 * 发一次请求并分类结果。
 *
 * 分类的意义（D-212）：界面上「检查更新失败」这一句什么都不说明——用户不知道该等、
 * 该换网、还是该点下载页。三类里**只有网络抖动值得重试**；限流按小时恢复，重试既
 * 没用又多打请求。
 */
async function attemptOnce(
  fetchFn: typeof fetch,
  currentVersion: string,
): Promise<AttemptOutcome> {
  let res: Response;
  try {
    res = await fetchFn(UPDATE_API_URL, {
      headers: { Accept: 'application/vnd.github+json' },
    });
  } catch {
    // fetch 抛异常 = 连不上（超时 / DNS / TLS / 代理整条断掉）。
    return { failure: 'network' };
  }
  if (!res.ok) {
    // 403/429 = GitHub 未认证限流（60 次/小时/IP）；其余 4xx/5xx 归 other。
    return { failure: res.status === 403 || res.status === 429 ? 'rate_limited' : 'other' };
  }
  let data: unknown;
  try {
    data = await res.json();
  } catch {
    return { failure: 'other' };
  }
  const tag =
    typeof data === 'object' && data !== null && 'tag_name' in data
      ? (data as { tag_name: unknown }).tag_name
      : null;
  if (typeof tag !== 'string' || !/^\d/.test(tag.replace(/^v/i, ''))) {
    return { failure: 'other' };
  }
  const latestVersion = tag.replace(/^v/i, '');
  return {
    result: {
      latestVersion,
      hasUpdate: compareVersions(latestVersion, currentVersion) > 0,
    },
  };
}

/**
 * 查询 GitHub 最新版本（单次）。任何失败（网络 / 限流 / 结构变化）都返回 `null`——
 * 更新检查是增强路径，绝不能变成新的报错源。
 *
 * @param fetchFn 网络函数（组件注入真实 `fetch`，测试注入替身）
 * @param currentVersion 本机版本（`getVersion()`）
 */
export async function fetchLatestVersion(
  fetchFn: typeof fetch,
  currentVersion: string,
): Promise<UpdateCheckResult | null> {
  const outcome = await attemptOnce(fetchFn, currentVersion);
  return 'result' in outcome ? outcome.result : null;
}

/**
 * 查询最新版本，**失败自动重试**，并带回失败原因（D-212）。
 *
 * 为什么需要：本机到 `api.github.com` 的连通性是抖的（2026-11-24 实测 6 次里 4 次
 * 在 10–20 秒上超时，而同一天成功的那次配额还剩 58/60）。发布脚本靠重试能把同一份
 * 包传上去，检查更新也该有同样的耐心。
 *
 * 限流不重试：配额按小时恢复，重试只会多打请求、还让用户多等两秒。
 */
export async function fetchLatestVersionWithRetry(
  fetchFn: typeof fetch,
  currentVersion: string,
  opts: {
    retries?: number;
    delayMs?: number;
    sleep?: (ms: number) => Promise<void>;
  } = {},
): Promise<UpdateAttempt> {
  const retries = opts.retries ?? UPDATE_RETRY_TIMES;
  const delayMs = opts.delayMs ?? UPDATE_RETRY_DELAY_MS;
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  let attempts = 0;
  let lastFailure: UpdateFailure = 'other';
  for (let i = 0; i <= retries; i += 1) {
    attempts += 1;
    const outcome = await attemptOnce(fetchFn, currentVersion);
    if ('result' in outcome) {
      return { result: outcome.result, failure: null, attempts };
    }
    lastFailure = outcome.failure;
    if (outcome.failure === 'rate_limited') {
      break;
    }
    if (i < retries) {
      await sleep(delayMs);
    }
  }
  return { result: null, failure: lastFailure, attempts };
}

/** 是否开启自动检查（默认关，方案 B）。 */
export function isAutoCheckEnabled(storage: UpdateCheckStorage): boolean {
  return storage.getItem(KEY_ENABLED) === '1';
}

/** 设置自动检查开关。关闭时清掉暂存的新版本号（UI 不应残留旧提示）。 */
export function setAutoCheckEnabled(storage: UpdateCheckStorage, enabled: boolean): void {
  if (enabled) {
    storage.setItem(KEY_ENABLED, '1');
  } else {
    storage.removeItem(KEY_ENABLED);
    storage.removeItem(KEY_LATEST);
  }
}

/**
 * 是否该做自动检查（节流 24h）。开关关闭恒 false；
 * 从未查过（无时间戳）视为该查。
 */
export function shouldAutoCheck(storage: UpdateCheckStorage): boolean {
  if (!isAutoCheckEnabled(storage)) {
    return false;
  }
  const last = Number(storage.getItem(KEY_LAST_MS) ?? '0');
  if (!Number.isFinite(last)) {
    return true;
  }
  return Date.now() - last >= AUTO_CHECK_INTERVAL_MS;
}

/** 写入/清除「暂存的新版本号」，不动节流时间戳。 */
export function setPendingUpdate(storage: UpdateCheckStorage, latestVersion: string | null): void {
  if (latestVersion !== null) {
    storage.setItem(KEY_LATEST, latestVersion);
  } else {
    storage.removeItem(KEY_LATEST);
  }
}

/** 记录一次自动检查完成（节流时间戳 + 最新版本暂存）。 */
export function markAutoChecked(storage: UpdateCheckStorage, latestVersion: string | null): void {
  storage.setItem(KEY_LAST_MS, String(Date.now()));
  setPendingUpdate(storage, latestVersion);
}

/**
 * 上次检查暂存、且**对本机当前版本仍构成更新**的版本号（无则 null）。
 *
 * D-142：这里必须带 `currentVersion` 判一次。缓存不会随升级自动失效 ——
 * 典型现场：1.4.19 自动检查暂存了 `1.5.0`，用户换绿色版升到 1.5.0，
 * 打开设置页仍看到「有新版本 1.5.0」，直到手动点一次「检查更新」才消失；
 * 而那个提示是错的，本机已经是最新。所以只要缓存不再构成更新就**顺手清掉**
 * （节流时间戳不动，那是独立状态）。
 */
export function pendingUpdateVersion(
  storage: UpdateCheckStorage,
  currentVersion: string,
): string | null {
  const cached = storage.getItem(KEY_LATEST);
  if (cached === null) {
    return null;
  }
  if (compareVersions(cached, currentVersion) > 0) {
    return cached;
  }
  storage.removeItem(KEY_LATEST);
  return null;
}
