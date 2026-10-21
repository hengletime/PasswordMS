// `loadSettingsPageSide` 的行为级测试（D-73）。
//
// 守的是**缺陷形状**，不是「跑通就行」：原实现把三件互不相关的事塞在同一个 `$effect`，
// 第一行对 `$state` 代理做 `structuredClone` 抛 `DataCloneError` → 后两块从不执行。
// 所以本文件里「一个 reject，另外两个仍有值」的用例才是核心，
// 只测三者全成功的话，原缺陷照样能全绿通过 —— 那才是真正的假绿。

import { describe, expect, it, vi } from 'vitest';

import { loadSettingsPageSide, type SettingsPageSideDeps } from './settings-page';
import type { VaultFileInfoDto } from '../types';

/** 库文件信息的样例值（路径形如 Windows 真机返回）。 */
const FILE_INFO: VaultFileInfoDto = {
  path: 'C:\\Users\\owner\\AppData\\Roaming\\PasswordMS\\passwordms.vault',
  size_bytes: 4096,
};

/** 兜底档位：真实调用点传 `AUTO_LOCK_OPTIONS[1]`，当前即 5 分钟。 */
const FALLBACK = 5;

/** 构造替身依赖；默认三者全部成功。 */
function makeDeps(overrides: Partial<SettingsPageSideDeps> = {}): SettingsPageSideDeps {
  return {
    loadVaultFileInfo: vi.fn(async (): Promise<VaultFileInfoDto> => FILE_INFO),
    loadAutoLock: vi.fn(async (): Promise<number> => 15),
    loadVersion: vi.fn(async (): Promise<string> => '1.2.6'),
    ...overrides,
  };
}

describe('loadSettingsPageSide —— D-73 设置页旁路数据失败隔离', () => {
  it('三者全成功：三块都有值，两个 failed 标记均为 false', async () => {
    const side = await loadSettingsPageSide(makeDeps(), FALLBACK);

    expect(side.fileInfo).toEqual(FILE_INFO);
    expect(side.fileInfoFailed).toBe(false);
    expect(side.autoLockMinutes).toBe(15);
    expect(side.autoLockFailed).toBe(false);
    expect(side.version).toBe('1.2.6');
  });

  it('库文件信息 reject：该项失败标记为 true，**版本与档位仍有值**（核心回归锚点）', async () => {
    const side = await loadSettingsPageSide(
      makeDeps({
        loadVaultFileInfo: vi.fn(async (): Promise<VaultFileInfoDto> => {
          throw { code: 'ERR_IO', message_key: 'err.io' };
        }),
      }),
      FALLBACK,
    );

    expect(side.fileInfo).toBeNull();
    expect(side.fileInfoFailed).toBe(true);
    // ↓ 这两条就是原缺陷的症状：一个炸了，另外两个也被带塌。
    expect(side.version).toBe('1.2.6');
    expect(side.autoLockMinutes).toBe(15);
    expect(side.autoLockFailed).toBe(false);
  });

  it('版本号 reject：version 为 null，库文件信息仍有值（对称锚点）', async () => {
    const side = await loadSettingsPageSide(
      makeDeps({
        loadVersion: vi.fn(async (): Promise<string> => {
          throw new Error('acl: allow-version missing');
        }),
      }),
      FALLBACK,
    );

    expect(side.version).toBeNull();
    expect(side.fileInfo).toEqual(FILE_INFO);
    expect(side.fileInfoFailed).toBe(false);
    expect(side.autoLockMinutes).toBe(15);
  });

  it('自动锁定档位 reject：回落到传入的兜底值并标记失败，另两项仍有值', async () => {
    const side = await loadSettingsPageSide(
      makeDeps({
        loadAutoLock: vi.fn(async (): Promise<number> => {
          throw new Error('locked');
        }),
      }),
      FALLBACK,
    );

    // 这条是主人还没发现的隐藏症状的回归点：读失败时显示兜底档，且必须**标记出来**，
    // 否则用户会以为库里存的就是 5 分钟（静默说谎）。
    expect(side.autoLockMinutes).toBe(FALLBACK);
    expect(side.autoLockFailed).toBe(true);
    expect(side.fileInfo).toEqual(FILE_INFO);
    expect(side.version).toBe('1.2.6');
  });

  it('档位返回非有限数（NaN）：按失败处理并回落，不让 <select> 变成「没选中项」', async () => {
    const side = await loadSettingsPageSide(
      makeDeps({ loadAutoLock: vi.fn(async (): Promise<number> => Number.NaN) }),
      FALLBACK,
    );

    expect(side.autoLockMinutes).toBe(FALLBACK);
    expect(side.autoLockFailed).toBe(true);
  });
});
