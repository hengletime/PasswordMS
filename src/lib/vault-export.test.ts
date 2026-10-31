// 导出保险库守卫（D-51 / Q-1）的行为级测试。
// QA Phase 5 登记缺口：守卫原先内联在 Settings.svelte，grep *.test.ts 零命中——
// 现已抽到 src/lib/vault-export.ts（依赖注入纯逻辑层），本文件证明：
//   1. exportPassword 为空 → 导出 IPC（exportVault）**零调用** + 就地 error toast；
//   2. 非空 → IPC 被调用且参数携带该密码；成功后 cleared=true（组件据此清空输入框）+ success toast；
//   3. 用户取消（确认框/保存对话框）→ 不发 IPC；
//   4. IPC 失败 → error toast、不清空；
//   5. **D-144**：后端回报的字节数决定「成功」是真的还是假的 —— 正数才念体积并记录，
//      0 字节一律判失败（2026-10-31 真机事故：SAF 留下 0 字节占位文件 + 一句「确定」）。
// 守卫若被误删：空密码会一路走到 exportVault 替身，用例 1 断言 `invoked === false` 即红。

import { describe, expect, it, vi } from 'vitest';

import {
  EXPORT_EMPTY_KEY,
  EXPORT_OK_KEY,
  EXPORT_OK_WITH_SIZE_KEY,
  EXPORT_PASSWORD_REQUIRED_KEY,
  runVaultExport,
  type VaultExportDeps,
} from './vault-export';

/** 正常保险库的量级（8 KB 出头的真实小库），用来断言体积文案。 */
const DEFAULT_BYTES = 8_208;

/** 构造一套可观测替身依赖；confirm/对话框默认放行，密码默认非空。 */
function makeDeps(overrides: Partial<VaultExportDeps> = {}): VaultExportDeps & {
  exportVault: ReturnType<typeof vi.fn>;
  toast: ReturnType<typeof vi.fn>;
} {
  // exportVault/toast 必须从 overrides 中**取出**后再注入（不能被后面的简写覆盖回去），
  // 否则 override 的替身会被默认 mock 顶掉，断言观测不到真实调用。
  const { exportVault: exportVaultOverride, ...rest } = overrides;
  const exportVault =
    exportVaultOverride ??
    vi.fn(async (_dest: string, _password: string): Promise<number> => DEFAULT_BYTES);
  const toast = vi.fn<
    (
      kind: 'info' | 'success' | 'error',
      messageKey: string,
      params?: Record<string, string | number>,
    ) => void
  >();
  const deps: VaultExportDeps = {
    exportPassword: 'qa-master-password',
    confirm: async (): Promise<boolean> => true,
    pickDestination: async (): Promise<string | null> => '/tmp/passwordms-export.vault',
    exportVault,
    toast: toast as unknown as VaultExportDeps['toast'],
    ...rest,
  };
  return { ...deps, exportVault, toast } as VaultExportDeps & {
    exportVault: ReturnType<typeof vi.fn>;
    toast: ReturnType<typeof vi.fn>;
  };
}

describe('runVaultExport —— D-51 导出守卫', () => {
  it('空密码：不发 IPC，就地 error toast（守卫被删则此用例变红）', async () => {
    const deps = makeDeps({ exportPassword: '' });

    const result = await runVaultExport(deps);

    // 核心断言：导出 IPC 零调用（守卫若被删，空密码会走到 exportVault）。
    expect(deps.exportVault).not.toHaveBeenCalled();
    expect(deps.toast).toHaveBeenCalledTimes(1);
    expect(deps.toast).toHaveBeenCalledWith('error', EXPORT_PASSWORD_REQUIRED_KEY);
    expect(result).toEqual({ invoked: false, cleared: false, bytes: null });
  });

  it('非空密码：IPC 被调用且参数携带该密码；成功后 cleared=true + 带体积的 success toast', async () => {
    const deps = makeDeps({ exportPassword: 'qa-master-password' });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).toHaveBeenCalledTimes(1);
    expect(deps.exportVault).toHaveBeenCalledWith('/tmp/passwordms-export.vault', 'qa-master-password');
    // D-144：成功文案必须带上实际字节数（8,208 B → "8.0 KB"）。
    expect(deps.toast).toHaveBeenCalledWith('success', EXPORT_OK_WITH_SIZE_KEY, {
      size: '8.0 KB',
    });
    expect(result).toEqual({ invoked: true, cleared: true, bytes: DEFAULT_BYTES });
  });

  it('D-144：后端回报 0 字节 → 判失败（error toast、不清空、bytes=0）', async () => {
    // 这正是 2026-10-31 的事故形态：写盘"成功"但一个字节都没落地。
    // 若实现把 0 当成功，用户会再次相信一份空备份 —— 本用例就是那道闸。
    const deps = makeDeps({
      exportVault: vi.fn(async (): Promise<number> => 0),
    });

    const result = await runVaultExport(deps);

    expect(deps.toast).toHaveBeenCalledWith('error', EXPORT_EMPTY_KEY);
    expect(result).toEqual({ invoked: true, cleared: false, bytes: 0 });
  });

  it('D-144：拿不到字节数（旧包/替身返回非数字）→ 沿用旧成功文案，不误判失败', async () => {
    const deps = makeDeps({
      exportVault: vi.fn(async (): Promise<number> => undefined as unknown as number),
    });

    const result = await runVaultExport(deps);

    expect(deps.toast).toHaveBeenCalledWith('success', EXPORT_OK_KEY);
    expect(result).toEqual({ invoked: true, cleared: true, bytes: null });
  });

  it('确认框取消：不发 IPC、无 toast', async () => {
    const deps = makeDeps({ confirm: async (): Promise<boolean> => false });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, cleared: false, bytes: null });
  });

  it('保存对话框取消：不发 IPC、无 toast', async () => {
    const deps = makeDeps({ pickDestination: async (): Promise<string | null> => null });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, cleared: false, bytes: null });
  });

  it('保存对话框 cancel（Android reject）→ 正常返回、不发 IPC（未捕获 rejection 回归锚点）', async () => {
    // D-71/D-72：Android 上 dialog 插件取消是 reject（桌面是 resolve null）。
    // 旧实现把 `pickDestination()` 放在 try 外面 → 异常逃出 runVaultExport →
    // `Settings.svelte` 的 `await runVaultExport(...)` 无 try/catch → 未捕获 rejection，
    // 用户点取消毫无反应。本用例**故意不在外层加 try/catch**：
    // 一旦实现又把异常抛出来，这里会直接失败（而不是变成一条安静的红）。
    const deps = makeDeps({
      pickDestination: vi.fn(async (): Promise<string | null> => {
        throw new Error('File picker cancelled');
      }),
    });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, cleared: false, bytes: null });
  });

  it('IPC 失败：error toast（统一错误码映射）、不清空', async () => {
    const deps = makeDeps({
      exportVault: vi.fn(
        async (_dest: string, _password: string): Promise<number> => {
          throw new Error('unlock failed');
        },
      ),
    });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).toHaveBeenCalledTimes(1);
    expect(deps.toast).toHaveBeenCalledWith('error', 'err.internal');
    expect(result).toEqual({ invoked: true, cleared: false, bytes: null });
  });
});
