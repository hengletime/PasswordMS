// D-133 / D-134 的**回退与上屏**双守卫（源码结构断言）。
//
// 这一轮要防的不是「新功能写错」，而是两类方向相反的漂移：
//
// ① **回退不彻底**：`auto_sync` / `sync_on_start` 两个开关的执行层（`lib/auto-sync.ts`）
//    已随 v1.4.16 删除。若设置页的勾选框或 `VaultSettings` 里的字段又长回来，
//    就精确复现了 D-129 的病根——**UI 替不存在的功能做承诺**（勾上了、保存了、什么都不发生）。
//    所以负向断言必须与「正向锚点」成对：只写 `not.toContain`，整块被删时也是绿的。
//
// ② **诊断被悄悄删掉**：`SyncStatus` 上的「本机网络 / 配对记录地址」是手动同步失败时
//    用户唯一能自己对照的两个数（D-134）。它们若被删，界面又回到只剩一句「网络不可达」。

import { describe, expect, it } from 'vitest';

const components = import.meta.glob('../components/{Settings,MainLayout}.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const syncComponents = import.meta.glob('../components/sync/SyncStatus.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

/** Rust 侧设置结构（`VaultSettings` 字段的唯一事实来源）。 */
const rustModules = import.meta.glob('../../crates/passwordms-core/src/model/vault.rs', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const settingsSrc = components['../components/Settings.svelte'] ?? '';
const mainLayoutSrc = components['../components/MainLayout.svelte'] ?? '';
const syncStatusSrc = syncComponents['../components/sync/SyncStatus.svelte'] ?? '';
const vaultRs = rustModules['../../crates/passwordms-core/src/model/vault.rs'] ?? '';

describe('文件均可读取（否则下面的负向断言全是假绿）', () => {
  it('四个源文件都非空', () => {
    expect(settingsSrc.length).toBeGreaterThan(500);
    expect(mainLayoutSrc.length).toBeGreaterThan(500);
    expect(syncStatusSrc.length).toBeGreaterThan(500);
    expect(vaultRs.length).toBeGreaterThan(500);
  });
});

describe('D-133：自动同步回退彻底 —— 开关、接线、字段三处都不许留下', () => {
  it('设置页不再绑定 auto_sync / sync_on_start，且「同步」组卡片已整体移除', () => {
    // 负向：两个字段名在设置页里彻底消失（含 checkbox 的 bind:checked）。
    expect(settingsSrc).not.toContain('auto_sync');
    expect(settingsSrc).not.toContain('sync_on_start');
    // D-136（v1.4.18，主人实测判定）：v1.4.16 回退后留下的「同步」卡片只剩一句
    // 「同步只在你点立即同步时发生」，没有任何可操作控件——主人原话「放着已经没有意义了」。
    // 删掉卡片后，这里**不能再用「必须出现那句文案」作正向锚点**：那样会把空壳文案
    // 钉死在代码里，下次谁想删都得先改测试（等于用测试保护一个没人要的东西）。
    // 改为钉住「组标题键与说明键都不许回来」。
    expect(settingsSrc).not.toContain("settings.group_sync");
    expect(settingsSrc).not.toContain("settings.sync_manual_only");
    // 正向锚点：删的是同步组，不是整个设置页——其他设置组必须还在。
    expect(settingsSrc).toContain("ui.t('settings.group_health')");
  });

  it('主界面不再挂自动同步调度器，但仍保留解锁后的五路加载', () => {
    // 负向：调度器模块与实例都不许回来（回来就等于给已删功能重新接上心跳）。
    expect(mainLayoutSrc).not.toContain('auto-sync');
    expect(mainLayoutSrc).not.toContain('createAutoSync');
    // 正向锚点：删的是同步调度，不是解锁加载——`loadAfterUnlock()` 必须还在。
    expect(mainLayoutSrc).toContain('loadAfterUnlock()');
  });

  it('Rust `VaultSettings` 里不再有这两个字段（留着 = 库里永远存着一个没人读的开关）', () => {
    // 负向：结构体字段名。
    expect(vaultRs).not.toContain('pub auto_sync');
    expect(vaultRs).not.toContain('pub sync_on_start');
    // 正向锚点：证明读到的是真的设置结构，而不是一个空文件。
    expect(vaultRs).toContain('pub clipboard_clear_secs');
    expect(vaultRs).toContain('pub struct VaultSettings');
  });

  it('同步状态页不再渲染旧自动同步回执（autoLast / 开关状态行）', () => {
    expect(syncStatusSrc).not.toContain('autoLast');
    expect(syncStatusSrc).not.toContain('sync.auto_');
  });
});

describe('D-134：手动同步的网络信息必须真上屏', () => {
  it('设备行渲染配对记录地址（peer_ip + peer_port）', () => {
    const tpl = syncStatusSrc.slice(syncStatusSrc.indexOf('</script>'));
    expect(tpl).toContain('device.peer_ip');
    expect(tpl).toContain('device.peer_port');
    // 地址缺失时不得留白——留白会被读成「地址是空的」而不是「没记录」。
    expect(syncStatusSrc).toContain("ui.t('sync.addr_unknown')");
  });

  it('页面渲染本机网络（地址 + 监听端口），且「未在监听」有单独文案', () => {
    const tpl = syncStatusSrc.slice(syncStatusSrc.indexOf('</script>'));
    expect(tpl).toContain("ui.t('sync.local_net')");
    expect(tpl).toContain('net.local_ips');
    expect(tpl).toContain('net.listen_port');
    // `listen_port === null` 与「端口是 0」在界面上必须可区分。
    expect(syncStatusSrc).toContain("ui.t('sync.not_listening')");
    expect(syncStatusSrc).toContain("ui.t('sync.listen_port', { port: net.listen_port })");
  });

  it('本机网络来自 sync_net_info 命令（不是就地拼的假数据）', () => {
    expect(syncStatusSrc).toContain('api.sync.netInfo()');
  });
});
