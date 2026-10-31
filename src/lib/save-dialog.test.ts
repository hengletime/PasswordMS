// `runSaveToDestination` 的行为级测试（D-71 / D-72）。
//
// 守的是什么：三处「选目标 + 写盘」的取消/失败归一，必须**同一口径**。
// 缺陷前史（三处、三种错法）：
//   ① vault-export 把对话框放在 try 外 → Android 取消 = 未捕获 rejection；
//   ② AttachmentView 用 try 包住「对话框 + 写盘」→ 点取消弹红「内部错误」；
//   ③ ImportWizard 空 catch → 真失败也静默。
// 所以下面五个用例刻意覆盖「取消的三条路（reject / null / 空串）」与
// 「失败的唯一路（write 抛）」——只测 happy path 就正好漏掉全部三个缺陷。

import { describe, expect, it, vi } from 'vitest';

import { runSaveToDestination, type SaveDialogDeps } from './save-dialog';

/**
 * 构造替身依赖；`pickDestination` 默认放行到固定路径。
 *
 * 返回类型显式声明成「依赖 + 可观测 mock」：`overrides` 里可能传的是普通函数而非 `vi.fn`，
 * 所以内部联合类型必须靠一次断言收窄（与 `vault-export.test.ts` 同款写法），
 * 否则 `Mock & Function` 的交叉类型无法从 `??` 的结果推出。
 */
function makeDeps(overrides: Partial<SaveDialogDeps> = {}): SaveDialogDeps & {
  pickDestination: ReturnType<typeof vi.fn>;
  write: ReturnType<typeof vi.fn>;
} {
  const pickDestination =
    overrides.pickDestination ?? vi.fn(async (): Promise<string | null> => '/tmp/passwordms-out.bin');
  const write = overrides.write ?? vi.fn(async (_dest: string): Promise<unknown> => undefined);
  return { pickDestination, write } as unknown as SaveDialogDeps & {
    pickDestination: ReturnType<typeof vi.fn>;
    write: ReturnType<typeof vi.fn>;
  };
}

describe('runSaveToDestination —— 取消/失败归一（D-71 / D-72）', () => {
  it('对话框 reject（Android 取消）→ cancelled，且 write 一次都没被调用', async () => {
    // 这条对应缺陷 ①：取消抛出的异常绝不能再逃出调用链，也绝不能触发写盘。
    const deps = makeDeps({
      pickDestination: vi.fn(async (): Promise<string | null> => {
        throw new Error('File picker cancelled');
      }),
    });

    const outcome = await runSaveToDestination(deps);

    expect(outcome).toEqual({ kind: 'cancelled' });
    expect(deps.write).not.toHaveBeenCalled();
  });

  it('对话框 resolve null（桌面取消）→ cancelled，write 未调用', async () => {
    const deps = makeDeps({ pickDestination: vi.fn(async (): Promise<string | null> => null) });

    const outcome = await runSaveToDestination(deps);

    expect(outcome).toEqual({ kind: 'cancelled' });
    expect(deps.write).not.toHaveBeenCalled();
  });

  it('对话框 resolve 空串 → cancelled，write 未调用（锚点：不许退化回 dest === null）', async () => {
    // 空串 = 没选到位置。若有人把 `typeof dest !== 'string' || dest.length === 0`
    // 改回 `dest === null`，空串会一路走到 write('')，变成一条莫名其妙的写盘失败。
    const deps = makeDeps({ pickDestination: vi.fn(async (): Promise<string | null> => '') });

    const outcome = await runSaveToDestination(deps);

    expect(outcome).toEqual({ kind: 'cancelled' });
    expect(deps.write).not.toHaveBeenCalled();
  });

  it('写盘失败（IPC 形错误）→ failed 且透出后端文案键（真实失败必须冒出来）', async () => {
    // 这条是「空 catch 吞真错」那个疑问的正面回答：取消静默，失败不静默。
    const deps = makeDeps({
      write: vi.fn(async (_dest: string): Promise<unknown> => {
        throw { code: 'ERR_IO', message_key: 'err.io' };
      }),
    });

    const outcome = await runSaveToDestination(deps);

    expect(outcome).toEqual({ kind: 'failed', messageKey: 'err.io' });
    expect(deps.write).toHaveBeenCalledTimes(1);
  });

  it('写盘成功 → saved，且 write 收到对话框返回的同一个位置字符串', async () => {
    const dest = 'content://com.android.providers.downloads/document/42';
    const deps = makeDeps({ pickDestination: vi.fn(async (): Promise<string | null> => dest) });

    const outcome = await runSaveToDestination(deps);

    expect(outcome).toEqual({ kind: 'saved', data: undefined });
    // 关键：Android 的 content:// URI 必须**原样**交给后端，前端不做任何解析/拼接。
    expect(deps.write).toHaveBeenCalledWith(dest);
  });

  it('D-144：write 的返回值原样透传（导出的字节数靠它送达调用点）', async () => {
    const dest = '/home/u/passwordms-export.vault';
    const deps = makeDeps({
      pickDestination: vi.fn(async (): Promise<string | null> => dest),
      write: vi.fn(async (): Promise<number> => 8_208),
    });

    const outcome = await runSaveToDestination(deps);

    expect(outcome).toEqual({ kind: 'saved', data: 8_208 });
  });
});
