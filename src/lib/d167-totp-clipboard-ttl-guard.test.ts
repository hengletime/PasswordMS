// D-167 守门：动态口令复制后的剪贴板存活时长，必须**跟着口令寿命走**。
//
// 主人的原话：「动态口令复制后的清空时间不是用 30 秒倒计时，而应该用动态口令的存活时间
// 倒计时。动态口令都变成下一个了，前一个还保存在剪贴板里有什么用？」
//
// 这条链路最容易出的错不是「忘了改」，而是**改错来源**：组件里现成有一个每秒刷新的
// `totpRemaining`，看起来很顺手，但它在新挂载、或 TOTP 效应还没跑第一轮时是 0 ——
// 照它算就是「复制即清空」（D-135 的同一类坑：把「还没算出来」当成「已经到期」）。
// 所以这里既测纯函数的每一个边界，也测组件**有没有按 `Date.now()` 现算**。

import { describe, expect, it } from 'vitest';

import { planSecretCopy, TOTP_EXPIRY_WARN_SECS } from './clipboard';
import { TOTP_PERIOD_SECS } from './constants';
import { enUS } from './i18n/en-US';
import { th } from './i18n/th';
import { zhCN } from './i18n/zh-CN';
import { zhTW } from './i18n/zh-TW';
import { secondsRemaining } from './totp';

const components = import.meta.glob('../components/ItemDetail.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const detail = Object.values(components)[0] ?? '';

/** 取 `[from, to)` 之间的片段；找不到标记直接失败（而不是悄悄返回空串）。 */
function section(source: string, from: string, to: string, label: string): string {
  const start = source.indexOf(from);
  expect(start, `${label}：起点标记 ${from} 不存在`).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(to, start);
  expect(end, `${label}：终点标记 ${to} 不存在`).toBeGreaterThan(start);
  return source.slice(start, end);
}

/** 构造一个「距下一周期还剩 n 秒」的时刻（n ∈ 1..period）。 */
function nowWithSecondsLeft(n: number): number {
  const elapsed = TOTP_PERIOD_SECS - n; // 本周期已经走过的秒数
  // 整体后移一个周期：既让 n = period 落在「刚跨进新窗口」而不是 epoch 0，
  // 也让「nowMs = 0」那条用例测的是另一个时刻。
  return (TOTP_PERIOD_SECS + elapsed) * 1000;
}

describe('D-167 口令剩余寿命的计算口径', () => {
  it('构造的时刻确实到位（否则下面的用例在测空气）', () => {
    // 这条是自检：`nowWithSecondsLeft` 写错的话，后面所有断言都会「碰巧」通过或失败，
    // 而失败原因看起来像业务逻辑错。
    for (const n of [1, 5, 12, 25, 30]) {
      expect(secondsRemaining(nowWithSecondsLeft(n)), `n=${n}`).toBe(n);
    }
  });

  it('口令只剩 12 秒、设置是 30 秒 → 按 12 秒清空', () => {
    // 这就是主人指出的核心场景：口令先死，剪贴板陪着一起死。
    expect(planSecretCopy('totp', 30, nowWithSecondsLeft(12)).ttlSecs).toBe(12);
  });

  it('口令还剩 25 秒、但用户把 TTL 设成 10 秒 → 仍按 10 秒（更严的那一个说了算）', () => {
    // min 而非「永远取口令寿命」：用户主动收紧的偏好不能被放松。
    expect(planSecretCopy('totp', 10, nowWithSecondsLeft(25)).ttlSecs).toBe(10);
  });

  it('用户把 TTL 放宽到 120 秒也拦得住：仍旧 30 秒封顶', () => {
    expect(planSecretCopy('totp', 120, nowWithSecondsLeft(30)).ttlSecs).toBe(TOTP_PERIOD_SECS);
  });

  it('口令只剩 1 秒 → 就是 1 秒，不许回退成整周期', () => {
    expect(planSecretCopy('totp', 30, nowWithSecondsLeft(1)).ttlSecs).toBe(1);
  });

  it('刚跨进新窗口（整 30 秒）→ 按完整 30 秒', () => {
    expect(planSecretCopy('totp', 30, nowWithSecondsLeft(30)).ttlSecs).toBe(30);
  });

  it('nowMs = 0（挂载瞬间）不得被当成「已到期」', () => {
    // 这一条专门挡 `totpRemaining` 那个坑：若实现读的是组件 state，刚挂载时它等于 0，
    // 这里就会算出 0 → startCountdown(0) 立刻清空 → 用户粘出空串。
    expect(planSecretCopy('totp', 30, 0).ttlSecs).toBe(TOTP_PERIOD_SECS);
  });

  it('用户名 / 密码不受口令影响：哪怕口令只剩 2 秒，也照用户的 TTL', () => {
    const near = nowWithSecondsLeft(2);
    expect(planSecretCopy('username', 30, near).ttlSecs).toBe(30);
    expect(planSecretCopy('password', 30, near).ttlSecs).toBe(30);
    // 用户把 TTL 设成 120 时同样原样返回（不做任何裁剪）。
    expect(planSecretCopy('password', 120, near).ttlSecs).toBe(120);
  });
});

describe('D-167 短寿提醒的触发条件', () => {
  it('只剩 5 秒及以下才提醒，6 秒不提醒', () => {
    // 边界两侧各测一次：写成 < 而不是 <= 时，剩 5 秒那条会漏掉提醒，
    // 而剩 6 秒那条无论如何都该安静。
    expect(planSecretCopy('totp', 30, nowWithSecondsLeft(TOTP_EXPIRY_WARN_SECS)).warnExpiring).toBe(
      true,
    );
    expect(
      planSecretCopy('totp', 30, nowWithSecondsLeft(TOTP_EXPIRY_WARN_SECS + 1)).warnExpiring,
    ).toBe(false);
  });

  it('用户把 TTL 收到 5 秒时，即使口令是满周期也只提醒不报错', () => {
    // ttl 被压到 5 秒是用户自己的选择，这里只要求判定与「实际剩几秒」一致。
    expect(planSecretCopy('totp', 5, nowWithSecondsLeft(30)).warnExpiring).toBe(true);
  });

  it('用户名 / 密码永远不提醒（它们没有「换码」这回事）', () => {
    expect(planSecretCopy('username', 5, nowWithSecondsLeft(1)).warnExpiring).toBe(false);
    expect(planSecretCopy('password', 5, nowWithSecondsLeft(1)).warnExpiring).toBe(false);
  });

  it('阈值比一个周期短（否则每条口令复制都会走提醒分支）', () => {
    expect(TOTP_EXPIRY_WARN_SECS).toBeGreaterThan(0);
    expect(TOTP_EXPIRY_WARN_SECS).toBeLessThan(TOTP_PERIOD_SECS);
  });
});

describe('D-167 组件接线', () => {
  const copyFn = section(detail, 'async function copyText(', 'async function openUrl(', 'copyText');

  it('存活时长由 planSecretCopy 决定，不是直接拿设置值', () => {
    expect(copyFn).toContain('planSecretCopy(');
    expect(copyFn).toContain('settings.value.clipboard_clear_secs');
    // 反面判据：不许再出现「ttl = 设置值」这条直通写法。
    expect(copyFn).not.toMatch(/const ttl = settings\.value\.clipboard_clear_secs;/);
  });

  it('剩余寿命按 Date.now() 现算，且算在 IPC 之前', () => {
    expect(copyFn).toContain('Date.now()');
    const nowAt = copyFn.indexOf('Date.now()');
    const awaitAt = copyFn.indexOf('await copySecretWithAutoClear');
    expect(nowAt).toBeGreaterThan(0);
    expect(awaitAt).toBeGreaterThan(nowAt);
  });

  it('不得拿组件里那份每秒刷新的 totpRemaining 当剩余寿命', () => {
    // 这是本轮唯一真正危险的错法：能编译、能跑、大部分时候还「看着对」，
    // 只在刚挂载或切换条目后的第一次复制时把剪贴板立刻清空。
    expect(copyFn).not.toContain('totpRemaining');
  });

  it('算出来的 ttl 真正传进了 copySecretWithAutoClear（而不是算完丢掉）', () => {
    // 先要求「计划里的秒数真的被取出来」，再要求「真的交给 IPC」——
    // 只查后者的话，`ttl` 改回设置值也能通过。
    expect(copyFn).toContain('plan.ttlSecs');
    expect(copyFn).toContain('await copySecretWithAutoClear(text, ttl');
  });

  it('提醒分支由 plan.warnExpiring 驱动，两条提示用的是**实际** ttl', () => {
    // 连方向一起钉住：只查「plan.warnExpiring 出现过」的话，把条件反转成
    // `!plan.warnExpiring` 照样通过，而界面行为正好相反（快到期时反而报「30 秒后清空」）。
    expect(copyFn).toContain('if (plan.warnExpiring) {');
    expect(copyFn).toContain("'common.copied_totp_expiring'");
    expect(copyFn).toContain("'common.copied_countdown'");
    // n 必须是算出来的 ttl，否则「剩 12 秒」的提示会写成设置值 30。
    expect(copyFn.match(/n: ttl/g) ?? []).toHaveLength(2);
  });
});

describe('D-167 四种语言的短寿提示', () => {
  it('common.copied_totp_expiring 四语齐备、非空、且都带 {n} 占位符', () => {
    for (const [name, dict] of [
      ['zh-CN', zhCN],
      ['zh-TW', zhTW],
      ['en-US', enUS],
      ['th', th],
    ] as const) {
      const text = (dict as unknown as { common: { copied_totp_expiring?: string } }).common
        .copied_totp_expiring;
      expect(text, `${name}:common.copied_totp_expiring`).toBeTruthy();
      expect((text ?? '').trim().length, `${name}:common.copied_totp_expiring`).toBeGreaterThan(0);
      expect(text, `${name} 缺 {n} 占位符`).toContain('{n}');
    }
  });
});
