// #531 使用埋点守卫：复制成功 → record_item_usage 被调。
//
// 无 DOM 测试环境，组件行为用**源码结构断言**钉（与 d81-ui-guard / vault-compact-ui 同族）：
// 埋点必须挂在 `ItemDetail.copyText` 的**成功路径**上（密码 / 用户名 / TOTP 三个复制
// 全部走 copyText 一个出口），且必须在复制成功之后（失败不算「使用」）；
// SecureNoteView 的正文复制是第四个出口（team-lead 追加裁定），同样钉成功路径。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob('../components/*.svelte', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

function pickSource(suffix: string): string {
  const key = Object.keys(modules).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : modules[key];
}

describe('#531 复制埋点（record_item_usage）', () => {
  it('扫描面有效', () => {
    expect(pickSource('ItemDetail.svelte').length).toBeGreaterThan(5000);
  });

  it('copyText 成功路径上有 recordItemUsage，且在复制调用之后（失败不算使用）', () => {
    const src = pickSource('ItemDetail.svelte');
    const fnAt = src.indexOf('async function copyText');
    expect(fnAt, 'ItemDetail 应有 copyText').toBeGreaterThan(-1);
    const usageAt = src.indexOf('vault.recordItemUsage(item.item_id)');
    expect(usageAt, 'copyText 成功后必须埋点').toBeGreaterThan(fnAt);
    // 埋点必须在两条复制分支之后（copySecretWithAutoClear / copySecret 都已 await）。
    const branchAt = src.indexOf('await api.system.copySecret(text, 0)', fnAt);
    expect(branchAt).toBeGreaterThan(fnAt);
    expect(usageAt).toBeGreaterThan(branchAt);
  });

  it('埋点走 vault store（ipc.ts 的 record_item_usage 由 ipc-contract 锚点与 store 透传测试守）', () => {
    expect(pickSource('ItemDetail.svelte')).toContain('vault.recordItemUsage');
  });

  it('SecureNoteView 正文复制成功路径上有 recordItemUsage，且在复制之后（team-lead 追加裁定）', () => {
    const src = pickSource('SecureNoteView.svelte');
    expect(src.length, '扫描面有效').toBeGreaterThan(500);
    const fnAt = src.indexOf('async function copyBody');
    expect(fnAt, 'SecureNoteView 应有 copyBody').toBeGreaterThan(-1);
    const copyAt = src.indexOf('await copySecretWithAutoClear(body)', fnAt);
    expect(copyAt).toBeGreaterThan(fnAt);
    const usageAt = src.indexOf('vault.recordItemUsage(item.item_id)', copyAt);
    expect(usageAt, '复制成功后必须埋点（失败不算使用）').toBeGreaterThan(copyAt);
  });

  it('SecureNoteView 埋点同样走 vault store', () => {
    expect(pickSource('SecureNoteView.svelte')).toContain('vault.recordItemUsage');
  });
});
