// 导出保险库守卫（D-51 / Q-1）的行为级测试。
// QA Phase 5 登记缺口：守卫原先内联在 Settings.svelte，grep *.test.ts 零命中——
// 现已抽到 src/lib/vault-export.ts（依赖注入纯逻辑层），本文件证明：
//   1. exportPassword 为空 → 导出 IPC（exportVault）**零调用** + 就地 error toast；
//   2. 非空 → IPC 被调用且参数携带该密码；成功后 cleared=true（组件据此清空输入框）+ success toast；
//   3. 用户取消（确认框/保存对话框）→ 不发 IPC；
//   4. IPC 失败 → error toast、不清空。
// 守卫若被误删：空密码会一路走到 exportVault 替身，用例 1 断言 `invoked === false` 即红。

import { describe, expect, it, vi } from 'vitest';

import {
  EXPORT_OK_KEY,
  EXPORT_PASSWORD_REQUIRED_KEY,
  runVaultExport,
  type VaultExportDeps,
} from './vault-export';

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
    vi.fn(async (_dest: string, _password: string): Promise<void> => undefined);
  const toast = vi.fn<(kind: 'info' | 'success' | 'error', messageKey: string) => void>();
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
    expect(result).toEqual({ invoked: false, cleared: false });
  });

  it('非空密码：IPC 被调用且参数携带该密码；成功后 cleared=true + success toast', async () => {
    const deps = makeDeps({ exportPassword: 'qa-master-password' });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).toHaveBeenCalledTimes(1);
    expect(deps.exportVault).toHaveBeenCalledWith('/tmp/passwordms-export.vault', 'qa-master-password');
    expect(deps.toast).toHaveBeenCalledWith('success', EXPORT_OK_KEY);
    expect(result).toEqual({ invoked: true, cleared: true });
  });

  it('确认框取消：不发 IPC、无 toast', async () => {
    const deps = makeDeps({ confirm: async (): Promise<boolean> => false });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, cleared: false });
  });

  it('保存对话框取消：不发 IPC、无 toast', async () => {
    const deps = makeDeps({ pickDestination: async (): Promise<string | null> => null });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).not.toHaveBeenCalled();
    expect(deps.toast).not.toHaveBeenCalled();
    expect(result).toEqual({ invoked: false, cleared: false });
  });

  it('IPC 失败：error toast（统一错误码映射）、不清空', async () => {
    const deps = makeDeps({
      exportVault: vi.fn(
        async (_dest: string, _password: string): Promise<void> => {
          throw new Error('unlock failed');
        },
      ),
    });

    const result = await runVaultExport(deps);

    expect(deps.exportVault).toHaveBeenCalledTimes(1);
    expect(deps.toast).toHaveBeenCalledWith('error', 'err.internal');
    expect(result).toEqual({ invoked: true, cleared: false });
  });
});
