// D-76 保险库压缩入口 —— 组件层结构守门。
//
// 规格见 `docs/06-ui-design.md` §6.11（v2.14）。本文件守 §6.11.7 的六条判据里
// **可以离屏判定**的五条（第 6 条窄屏 is-info 由 `settings-row-type-guard.test.ts` 守）。
//
// 为什么用 `?raw` 扫源码而不是渲染快照：本项目没有 DOM 测试环境；而要守的东西
// 全部是「谁包在谁里面」「有没有那次 reload」「失败分支有没有偷偷改数字」这类
// **结构事实**，源码断言恰好能逐条钉死，且改了就会红。
//
// 其中四条是本轮最容易做错、也最难被肉眼看出来的：
//   ① `reclaimable_bytes === 0` 必须是**不渲染按钮**，不是 `disabled` ——
//      一个点了没反应的按钮就是假功能；
//   ② 成功后必须**重新拉取** `fileInfo` 与 `compactionStats` —— 只弹 toast 不刷新，
//      卡片上还是 24.3 MB，用户会以为没生效（正是本次缺陷的翻版）；
//   ③ 失败分支**不许改动任何显示数字** —— 宁可显示旧值，不许显示「已压缩」；
//   ④ 压缩期间 `reloadSideData()` 必须**短路** —— 后端持 `state.session` 锁时，
//      再调 stats IPC 不失败也不超时，是**挂住**（失败隔离救不了「挂住」）。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob('../components/Settings.svelte', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

/**
 * 按键名**后缀**取源码（不依赖 Vite 的键规整规则）。
 *
 * @param map glob 结果
 * @param suffix 键名后缀
 * @returns 源码；未命中返回空串（由「扫描面有效」那条断言判红）
 */
function pickSource(map: Record<string, string>, suffix: string): string {
  const key = Object.keys(map).find((k) => k.endsWith(suffix));
  return key === undefined ? '' : map[key];
}

/**
 * 剥掉注释后再做**结构切片**。
 *
 * 必须剥：markup 里的说明文字会写出被断言的结构本身（例如「见下面的 `{:else}` 分支」），
 * 直接在全文上 `indexOf` 会把**注释**当成代码 —— 这正是 `:has()` 那条守卫踩过的坑
 * （见 `settings-row-type-guard.test.ts` 里 `stripComments` 的注释）。
 * 剥注释只影响切片边界，不改变任何行为断言。
 *
 * @param source 组件全文
 * @returns 去掉块注释与 HTML 注释之后的文本
 */
function stripComments(source: string): string {
  return source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/<!--[\s\S]*?-->/g, '');
}

/** 组件全文（含注释，仅用于「确实读到了」这类粗断言）。 */
const SRC = pickSource(modules, 'components/Settings.svelte');
/** 结构断言一律在「无注释」的源码上做。 */
const SRC_CODE = stripComments(SRC);

/** 取「保险库文件」卡片那一段（从卡片标题到卡片闭合）。 */
function vaultCard(): string {
  const at = SRC_CODE.indexOf("settings.vault_file')");
  expect(at, 'Settings.svelte 应有 settings.vault_file 卡片标题').toBeGreaterThan(-1);
  // 卡片到下一个 `<div class="pms-card set-card">` 或文件末尾为止。
  const next = SRC_CODE.indexOf('<div class="pms-card set-card">', at);
  return next < 0 ? SRC_CODE.slice(at) : SRC_CODE.slice(at, next);
}

/**
 * 取「体积构成读取失败」那个分支（`{#if compactionStatsFailed}` 到 `{:else if compactionStats}`）。
 *
 * 注意必须**切片**后再做负向断言：`settings.vault_file_failed` 在本卡片上方还有一处
 * （`fileInfoFailed` 用的），对整张卡片做 `not.toContain` 会恒红。
 *
 * @returns 失败分支源码
 */
function compactionFailBranch(): string {
  const card = vaultCard();
  const at = card.indexOf('{#if compactionStatsFailed}');
  expect(at, '保险库卡片应有 compactionStatsFailed 分支').toBeGreaterThan(-1);
  const end = card.indexOf('{:else if compactionStats}', at);
  expect(end, 'compactionStatsFailed 分支后应紧跟 {:else if compactionStats}').toBeGreaterThan(at);
  return card.slice(at, end);
}

/** 取 `runCompaction()` 的函数体。 */
function runCompactionBody(): string {
  const at = SRC_CODE.indexOf('async function runCompaction');
  expect(at, 'Settings.svelte 应有 runCompaction()').toBeGreaterThan(-1);
  const end = SRC_CODE.indexOf('\n  }\n', at);
  expect(end).toBeGreaterThan(at);
  return SRC_CODE.slice(at, end);
}

/** 取确认块（`<Modal open={compactConfirmOpen}` 到对应 `</Modal>`）。 */
function confirmModal(): string {
  const at = SRC_CODE.indexOf('<Modal\n  open={compactConfirmOpen}');
  expect(at, 'Settings.svelte 应有压缩二次确认 Modal').toBeGreaterThan(-1);
  const end = SRC_CODE.indexOf('</Modal>', at);
  expect(end).toBeGreaterThan(at);
  return SRC_CODE.slice(at, end);
}

describe('D-76 压缩入口：扫描面与数据接入', () => {
  it('扫描面有效（glob 写错会退化成空串上的空断言假绿）', () => {
    expect(SRC.length).toBeGreaterThan(10000);
    expect(SRC).toContain('settings.compact');
    expect(SRC).toContain('runCompaction');
    expect(confirmModal().length).toBeGreaterThan(200);
  });

  it('体积构成从 loadSettingsPageSide 的第四块取（独立 IPC，不靠 fileInfo 猜）', () => {
    expect(SRC).toContain('loadCompactionStats: (): Promise<CompactionStatsDto> =>');
    expect(SRC).toContain('api.system.vaultCompactionStats()');
    // 负向：不许拿库文件大小「推算」可回收空间（会差几百字节，然后被当成 bug 报上来）。
    expect(SRC).not.toContain('size_bytes - ');
  });

  it('两个数字都走 formatBytes（不许自己拼单位）', () => {
    const card = vaultCard();
    expect(card).toContain('formatBytes(deadAttachmentBytes)');
    expect(card).toContain('formatBytes(reclaimableBytes)');
  });
});

describe('D-76 §6.11.7 判据 1/2：按钮的存在与禁用', () => {
  it('reclaimable_bytes === 0 时按钮**不进 DOM**：按钮包在 {#if canCompact} 内，else 分支给结论句', () => {
    const card = vaultCard();
    const ifAt = card.indexOf('{#if canCompact}');
    const btnAt = card.indexOf('<button');
    expect(ifAt).toBeGreaterThan(-1);
    expect(btnAt, '按钮必须存在（在 canCompact 分支里）').toBeGreaterThan(ifAt);
    // `{:else}` 要从**按钮闭合之后**开始找：按钮内部还有一个 `{#if compacting}…{:else}…`
    // （忙碌态文案切换），从 ifAt 往后找第一个 `{:else}` 会命中它，切错分支。
    const afterBtn = card.indexOf('</button>', btnAt);
    expect(afterBtn).toBeGreaterThan(btnAt);
    const elseAt = card.indexOf('{:else}', afterBtn);
    expect(elseAt, 'canCompact 必须有 {:else} 分支').toBeGreaterThan(afterBtn);
    // else 分支说的是「没有可回收的空间」这个**结论**，而不是留白。
    const elseBranch = card.slice(elseAt, card.indexOf('{/if}', elseAt));
    expect(elseBranch).toContain("ui.t('settings.compact_none')");
    // 负向：不许在 else 分支里还留一个 disabled 的按钮（那就是「假功能」）。
    expect(elseBranch).not.toContain('<button');
  });

  it('canCompact 由 reclaimable_bytes 派生（不是由 file_bytes 或硬编码布尔）', () => {
    expect(SRC).toContain('const canCompact = $derived(reclaimableBytes > 0);');
    expect(SRC).toContain('const reclaimableBytes = $derived(compactionStats?.reclaimable_bytes ?? 0);');
  });

  it('同步进行中禁用 + 出现 compact_wait_sync（后端压缩在 session 锁内，与同步结构互斥）', () => {
    const card = vaultCard();
    expect(card).toContain('disabled={compacting || syncBusy}');
    expect(SRC).toContain('const syncBusy = $derived(sync.syncingDeviceId !== null);');
    // 提示与禁用必须在同一分支内（只禁用不解释，用户会以为卡死）。
    const btnAt = card.indexOf('<button');
    const after = card.slice(btnAt);
    expect(after).toContain('{#if syncBusy}');
    expect(after).toContain("ui.t('settings.compact_wait_sync')");
  });

  it('压缩自身进行中：禁用 + Spinner + 文案换 compacting', () => {
    const card = vaultCard();
    expect(card).toContain('<Spinner');
    expect(card).toContain("ui.t('settings.compacting')");
    // 按钮颜色不许用危险色（压缩不删数据，用 danger 会误导）。
    const btn = card.slice(card.indexOf('<button'), card.indexOf('</button>'));
    expect(btn).not.toContain('is-danger');
    expect(btn).toContain('is-ghost');
  });
});

describe('D-76 §6.11.7 判据 3：确认块四个数字 + 三条提示', () => {
  it('四个数字齐全（当前 / 压缩后约 / 可回收 / 已删附件占）', () => {
    const modal = confirmModal();
    for (const key of [
      'settings.compact_label_before',
      'settings.compact_label_after',
      'settings.compact_label_reclaim',
      'settings.compact_label_dead',
    ]) {
      expect(modal, `确认块缺少「${key}」`).toContain(key);
    }
    // 「压缩后约」取 stats.after_bytes（不是 file_bytes - reclaimable_bytes 推算）。
    expect(modal).toContain('compactionStats?.after_bytes');
  });

  it('三条提示缺一不可（不改数据/不要主密码 · 回收站不回收 · 2 倍磁盘别关程序）', () => {
    const modal = confirmModal();
    for (const key of [
      'settings.compact_hint_no_password',
      'settings.compact_hint_trash',
      'settings.compact_hint_disk',
    ]) {
      expect(modal, `确认块缺少「${key}」`).toContain(key);
    }
  });

  it('确认块有两个动作：取消 / 开始压缩', () => {
    const modal = confirmModal();
    expect(modal).toContain("ui.t('common.cancel')");
    expect(modal).toContain("ui.t('settings.compact_start')");
    expect(modal).toContain('void runCompaction()');
  });
});

describe('D-76 §6.11.7 判据 4/5：成功重拉、失败不改数字', () => {
  it('成功后重新拉取 fileInfo 与 compactionStats（断言那次调用，不只看 toast）', () => {
    const body = runCompactionBody();
    const callAt = body.indexOf('api.system.compactVault()');
    const reloadAt = body.indexOf('await reloadSideData(true)');
    expect(callAt).toBeGreaterThan(-1);
    expect(reloadAt, '压缩成功后必须 reload，否则卡片还是旧大小').toBeGreaterThan(callAt);
    // 被重拉的两个字段由 reloadSideData 负责写回。
    // D-107 起文件里有两个 $effect：终点必须找 reloadSideData **之后**的第一个。
    const startAt = SRC_CODE.indexOf('async function reloadSideData');
    const reloader = SRC_CODE.slice(startAt, SRC_CODE.indexOf('$effect(', startAt));
    expect(reloader).toContain('fileInfo = side.fileInfo;');
    expect(reloader).toContain('compactionStats = side.compactionStats;');
  });

  it('成功 toast 带上实际回收量（settings.compact_done 的 {size}）', () => {
    const body = runCompactionBody();
    expect(body).toContain("ui.toast('success', 'settings.compact_done'");
    expect(body).toContain('result.reclaimed_bytes');
  });

  it('失败分支不改动任何显示数字（只 toast），也不自动重试', () => {
    const body = runCompactionBody();
    const catchAt = body.indexOf('catch (err)');
    expect(catchAt).toBeGreaterThan(-1);
    // 切到 `} finally` 而不是 `finally`：try 块里的注释写了「复位在 finally」，
    // 用裸 `finally` 会切在注释上 → catchBody 变成空串 → 负向断言全部恒绿（空串陷阱）。
    const catchBody = body.slice(catchAt, body.indexOf('} finally'));
    expect(catchBody).toContain('toAppError(err)');
    // 负向：失败时不许写任何数据状态 —— 那会让界面显示「已压缩」而库里没变。
    expect(catchBody).not.toContain('fileInfo =');
    expect(catchBody).not.toContain('compactionStats =');
    expect(catchBody).not.toContain('compactVault()'); // 不许自动重试
  });

  it('防重入：压缩进行中直接返回（重操作，双击不该跑两遍）', () => {
    const body = runCompactionBody();
    const guardAt = body.indexOf('if (compacting)');
    const callAt = body.indexOf('api.system.compactVault()');
    expect(guardAt).toBeGreaterThan(-1);
    expect(guardAt).toBeLessThan(callAt);
  });
});

describe('D-76 竞态：压缩期间不许重拉旁路数据', () => {
  // 后端 `compact_vault` 持 `state.session` 锁，压缩期间再调 `vault_compaction_stats`
  // **不失败、也不超时，会一直挂住**。D-73 的逐块失败隔离救不了「挂住」——
  // 它不是失败。触发路径：压缩中离开设置页再回来 → `$effect` 重跑 `reloadSideData()`。
  it('reloadSideData 开头短路（且短路在读 IPC 之前）', () => {
    // D-107 起文件里有两个 $effect：终点必须找 reloadSideData **之后**的第一个。
    const startAt = SRC_CODE.indexOf('async function reloadSideData');
    const reloader = SRC_CODE.slice(startAt, SRC_CODE.indexOf('$effect(', startAt));
    const guardAt = reloader.indexOf('if (!force && compacting)');
    const callAt = reloader.indexOf('loadSettingsPageSide(');
    // 正向锚点：短路必须真的存在。`!force` 写在前面，强制调用才不会被自己的守卫拦掉。
    expect(guardAt, 'reloadSideData 开头必须有 if (!force && compacting) 短路').toBeGreaterThan(-1);
    expect(callAt).toBeGreaterThan(-1);
    // 且必须在发 IPC 之前 —— 写在 await 之后等于没写（照样挂住）。
    expect(guardAt).toBeLessThan(callAt);
    // 负向：短路分支里不能再调任何 IPC（`return;` 就是全部动作）。
    const guardBody = reloader.slice(guardAt, guardAt + 80);
    expect(guardBody).toContain('return;');
    expect(guardBody).not.toContain('vaultCompactionStats()');
  });

  it('成功后那次 reload 走 force（否则是一条永不执行的语句）', () => {
    const body = runCompactionBody();
    // 正向锚点：显式刷新必须带 force —— 此刻 compacting 仍是 true（复位在 finally）。
    expect(body, '成功路径必须 await reloadSideData(true)').toContain('await reloadSideData(true)');
    // 负向 1：不许在 try 里复位后再 reload（那样 reload 与 effect 会并发打两组四块读取，
    // 且按钮在 reload 窗口内已可点，理论上能点出第二次 compact_vault）。
    const tryBody = body.slice(body.indexOf('try {'), body.indexOf('catch (err)'));
    expect(tryBody).not.toContain('compacting = false');
    // 负向 2：不许退化成不带参数的 reload（会被守卫直接返回 → 判据 4 失效）。
    expect(body).not.toContain('await reloadSideData();');
    // finally 里那次复位是唯一的复位点，必须留着。
    expect(body.slice(body.indexOf('} finally'))).toContain('compacting = false;');
  });
});

describe('D-76 压缩失败：说清「库没被写坏」', () => {
  it('通用内部错误换成 compact_failed，具体原因仍透出', () => {
    const body = runCompactionBody();
    const catchAt = body.indexOf('catch (err)');
    // 切到 `} finally` 而不是 `finally`：try 块里的注释写了「复位在 finally」，
    // 用裸 `finally` 会切在注释上 → catchBody 变成空串 → 负向断言全部恒绿（空串陷阱）。
    const catchBody = body.slice(catchAt, body.indexOf('} finally'));
    // 正向锚点：新文案必须真的被用到（整块删掉时下面的负向会恒绿）。
    expect(catchBody).toContain("'settings.compact_failed'");
    // 只替换通用兜底，不吞掉后端给的具体原因。
    expect(catchBody).toContain("appErr.messageKey === 'err.internal'");
    expect(catchBody).toContain('appErr.messageKey');
    // 负向：不许直接拿通用错误码当压缩失败文案（那就是「内部错误」，会让用户以为库坏了）。
    expect(catchBody).not.toContain("ui.toast('error', 'err.internal')");
  });
});

describe('D-76 体积构成读取失败：说「读不到」，不说别的结论', () => {
  // 这一组守的是「失败伪装成结论」的同族坑（D-27 / D-73）：统计拿不到时
  // 既不能留白，也不能说「没有可回收的空间」——后者是我们**不知道**的结论。
  it('失败分支用专门的 compact_stats_failed 文案', () => {
    const branch = compactionFailBranch();
    // 正向锚点：整块被删时下面那条负向断言会恒绿，所以先钉住它必须说点什么。
    expect(branch).toContain("ui.t('settings.compact_stats_failed')");
    // 负向 1：不许复用「保险库文件大小读取失败」——主语不对，把失败归错因。
    expect(branch).not.toContain('settings.vault_file_failed');
    // 负向 2：不许说成「没有可回收的空间」——那是结论，此刻并不知道。
    expect(branch).not.toContain('settings.compact_none');
  });

  it('失败分支不渲染压缩按钮（量不出来就不给动作）', () => {
    expect(compactionFailBranch()).not.toContain('<button');
  });
});
