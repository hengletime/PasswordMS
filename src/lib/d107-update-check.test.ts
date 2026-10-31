// D-107：更新检查纯逻辑单测（版本比较 / GitHub 应答解析 / 节流与开关）。
// 网络与 localStorage 全部注入替身，Node 环境直跑。

import { describe, expect, it } from 'vitest';
import {
  AUTO_CHECK_INTERVAL_MS,
  compareVersions,
  fetchLatestVersion,
  isAutoCheckEnabled,
    markAutoChecked,
    pendingUpdateVersion,
    setAutoCheckEnabled,
    setPendingUpdate,
    shouldAutoCheck,
    type UpdateCheckStorage,
  } from './update-check';

/** 内存版 localStorage。 */
function memoryStorage(initial: Record<string, string> = {}): UpdateCheckStorage & { map: Map<string, string> } {
  const map = new Map(Object.entries(initial));
  return {
    map,
    getItem: (k) => (map.has(k) ? (map.get(k) as string) : null),
    setItem: (k, v) => void map.set(k, v),
    removeItem: (k) => void map.delete(k),
  };
}

describe('D-107 compareVersions', () => {
  it('语义化比较：主/次/修订号逐段比', () => {
    expect(compareVersions('1.2.23', '1.2.22')).toBeGreaterThan(0);
    expect(compareVersions('1.2.22', '1.2.23')).toBeLessThan(0);
    expect(compareVersions('1.2.22', '1.2.22')).toBe(0);
    expect(compareVersions('2.0.0', '1.99.99')).toBeGreaterThan(0);
  });

  it('容忍 v 前缀与缺段（缺段按 0）', () => {
    expect(compareVersions('v1.2.23', '1.2.22')).toBeGreaterThan(0);
    expect(compareVersions('1.2', '1.2.0')).toBe(0);
    expect(compareVersions('1.2', '1.2.1')).toBeLessThan(0);
  });

  it('非数字段按 0（容错预发布标记）', () => {
    expect(compareVersions('1.2.3-beta', '1.2.3')).toBe(0);
  });
});

describe('D-107 fetchLatestVersion', () => {
  const okFetch = (tag: string, status = 200) =>
    (async () => ({
      ok: status === 200,
      status,
      json: async () => ({ tag_name: tag, name: 'x' }),
    })) as unknown as typeof fetch;

  it('tag 带新版本 → hasUpdate=true 且去掉 v 前缀', async () => {
    const r = await fetchLatestVersion(okFetch('v1.2.23'), '1.2.22');
    expect(r).not.toBeNull();
    expect(r?.latestVersion).toBe('1.2.23');
    expect(r?.hasUpdate).toBe(true);
  });

  it('同版本 / 旧版本 → hasUpdate=false', async () => {
    expect((await fetchLatestVersion(okFetch('v1.2.22'), '1.2.22'))?.hasUpdate).toBe(false);
    expect((await fetchLatestVersion(okFetch('1.2.21'), '1.2.22'))?.hasUpdate).toBe(false);
  });

  it('HTTP 非 200 / 应答结构异常 / 网络异常 → null（静默）', async () => {
    expect(await fetchLatestVersion(okFetch('v9.9.9', 403), '1.2.22')).toBeNull();
    const badShape = (async () => ({ ok: true, json: async () => ({}) })) as unknown as typeof fetch;
    expect(await fetchLatestVersion(badShape, '1.2.22')).toBeNull();
    const broken = (async () => {
      throw new Error('offline');
    }) as unknown as typeof fetch;
    expect(await fetchLatestVersion(broken, '1.2.22')).toBeNull();
  });
});

describe('D-107 自动检查开关与节流', () => {
  it('默认关闭（方案 B）；开启/关闭按预期落盘，关闭清残留提示', () => {
    const s = memoryStorage();
    expect(isAutoCheckEnabled(s)).toBe(false);
    setAutoCheckEnabled(s, true);
    expect(isAutoCheckEnabled(s)).toBe(true);
    markAutoChecked(s, '1.2.23');
    expect(pendingUpdateVersion(s, '1.2.22')).toBe('1.2.23');
    setAutoCheckEnabled(s, false);
    expect(isAutoCheckEnabled(s)).toBe(false);
    expect(pendingUpdateVersion(s, '1.2.22')).toBeNull();
  });

  it('未查过 → 该查；24h 内查过 → 不查；过期 → 该查', () => {
    const s = memoryStorage();
    setAutoCheckEnabled(s, true);
    expect(shouldAutoCheck(s)).toBe(true);

    markAutoChecked(s, null);
    expect(shouldAutoCheck(s)).toBe(false);

    // 把时间戳拨回 25h 前。
    const stale = memoryStorage({
      update_check_enabled: '1',
      update_check_last_ms: String(Date.now() - AUTO_CHECK_INTERVAL_MS - 60 * 1000),
    });
    expect(shouldAutoCheck(stale)).toBe(true);
  });

  it('开关关闭时恒不查（哪怕从未查过）', () => {
    const s = memoryStorage();
    expect(shouldAutoCheck(s)).toBe(false);
  });
});

// D-142：绿色版升到新版后设置页仍提示「有新版本」，点一次检查更新才隐去。
describe('D-142 暂存版本必须按当前版本过滤', () => {
  it('升级后缓存不再构成更新 → 返回 null 并清掉缓存（提示立即消失）', () => {
    const s = memoryStorage();
    setAutoCheckEnabled(s, true);
    markAutoChecked(s, '1.5.0');
    // 用户把绿色版换成 1.5.0 后重进设置页。
    expect(pendingUpdateVersion(s, '1.5.0')).toBeNull();
    expect(s.map.has('update_check_latest')).toBe(false);
    // 清缓存不许动节流时间戳（那是独立状态，否则会自动重查刷爆 GitHub）。
    expect(shouldAutoCheck(s)).toBe(false);
  });

  it('缓存高于当前版本 → 原样返回且保留缓存', () => {
    const s = memoryStorage();
    markAutoChecked(s, '1.6.0');
    expect(pendingUpdateVersion(s, '1.5.0')).toBe('1.6.0');
    expect(s.map.get('update_check_latest')).toBe('1.6.0');
  });

  it('无缓存 → null（不写垃圾值）', () => {
    const s = memoryStorage();
    expect(pendingUpdateVersion(s, '1.5.0')).toBeNull();
    expect(s.map.size).toBe(0);
  });

  it('手动检查发现已最新会清缓存，重进设置页不再冒提示', () => {
    const s = memoryStorage();
    markAutoChecked(s, '1.5.0');
    // 手动检查：hasUpdate=false → 传 null。
    setPendingUpdate(s, null);
    expect(pendingUpdateVersion(s, '1.4.19')).toBeNull();
  });

  it('v 前缀与缺段按 compareVersions 同一口径（1.5 vs 1.5.0 视为同版）', () => {
    const s = memoryStorage();
    setPendingUpdate(s, 'v1.5');
    expect(pendingUpdateVersion(s, '1.5.0')).toBeNull();
  });
});
