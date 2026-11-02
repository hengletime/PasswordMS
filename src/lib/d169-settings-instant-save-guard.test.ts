// D-169 守门：设置页不该再有「哪些要按保存」这种需要记住的规则。
//
// 主人的原话：「设置里有点乱。哪些设置的改变要点击保存按钮，哪些设置不要点击保存按钮
// 不知道。我刚刚设置和剪贴板清空时间，以为设置一下数字就好了，结果没有生效。再去看看
// 可能是没有点击保存，而保存按钮离这个设置中间隔了几个卡片。」
//
// 真正的问题不是「按钮位置不好」，而是**同一页里存在三种生效方式且没有区分**：
// ①改了立刻生效（主题、语言、热键、自启、自动锁定、快速解锁）；②要按保存（剪贴板秒数、
// 体检阈值）；③点了就执行（导入、导出、同步、改密码）。
// 修法是消灭第②类：三处输入框失焦即写库，页面上不再有保存按钮。
//
// 所以这里测三件事：纯函数夹紧的边界（空串/小数/越界）、三个输入框**都**接了提交、
// 以及失败必须可见——静默失败正是「以为改生效了」的翻版。

import { describe, expect, it } from 'vitest';

import {
  CLIPBOARD_TTL_MAX,
  CLIPBOARD_TTL_MIN,
  CLIPBOARD_TTL_SECS,
  HEALTH_STALE_DAYS,
  HEALTH_STALE_MAX,
  HEALTH_STALE_MIN,
  HEALTH_WEAK_MAX,
  HEALTH_WEAK_MAX_LEN,
  HEALTH_WEAK_MIN,
} from './constants';
import { enUS } from './i18n/en-US';
import { th } from './i18n/th';
import { zhCN } from './i18n/zh-CN';
import { zhTW } from './i18n/zh-TW';
import { clampClipboardTtl, clampHealthSettings, clampInt } from './settings-clamp';

const components = import.meta.glob('../components/Settings.svelte', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const settings = Object.values(components)[0] ?? '';

describe('D-169 剪贴板秒数的夹紧（行为判据）', () => {
  it('正常值原样通过', () => {
    expect(clampClipboardTtl(30)).toBe(30);
    expect(clampClipboardTtl('45')).toBe(45);
  });

  it('空串回落到默认 30，而不是 0', () => {
    // `Number('')` 是 **0**：不先判空串就会把「复制即清空」写进库，而界面看着像没问题。
    expect(clampClipboardTtl('')).toBe(CLIPBOARD_TTL_SECS);
    expect(clampClipboardTtl('   ')).toBe(CLIPBOARD_TTL_SECS);
    expect(clampClipboardTtl(null)).toBe(CLIPBOARD_TTL_SECS);
  });

  it('0 / 负数被抬到下限（0 秒 = 复制即清空）', () => {
    expect(clampClipboardTtl(0)).toBe(CLIPBOARD_TTL_MIN);
    expect(clampClipboardTtl(-5)).toBe(CLIPBOARD_TTL_MIN);
  });

  it('超大值被压到上限（密码不该长期留在剪贴板）', () => {
    expect(clampClipboardTtl(9999)).toBe(CLIPBOARD_TTL_MAX);
  });

  it('小数向下取整，非数字回落', () => {
    expect(clampClipboardTtl(31.9)).toBe(31);
    expect(clampClipboardTtl('abc')).toBe(CLIPBOARD_TTL_SECS);
    expect(clampClipboardTtl(Number.NaN)).toBe(CLIPBOARD_TTL_SECS);
  });
});

describe('D-169 体检阈值的夹紧', () => {
  it('两项各自夹紧、互不影响', () => {
    expect(clampHealthSettings({ weak_max_len: 10, stale_days: 90 })).toEqual({
      weak_max_len: 10,
      stale_days: 90,
    });
    expect(clampHealthSettings({ weak_max_len: 1, stale_days: 9 })).toEqual({
      weak_max_len: HEALTH_WEAK_MIN,
      stale_days: HEALTH_STALE_MIN,
    });
    expect(clampHealthSettings({ weak_max_len: 99, stale_days: 9999 })).toEqual({
      weak_max_len: HEALTH_WEAK_MAX,
      stale_days: HEALTH_STALE_MAX,
    });
  });

  it('空值回落各自的默认值（不是互相顶替）', () => {
    const out = clampHealthSettings({ weak_max_len: '' as unknown as number, stale_days: '' as unknown as number });
    expect(out.weak_max_len).toBe(HEALTH_WEAK_MAX_LEN);
    expect(out.stale_days).toBe(HEALTH_STALE_DAYS);
  });

  it('通用夹紧函数本身：边界与回落都独立可测', () => {
    expect(clampInt(5, 5, 120, 30)).toBe(5);
    expect(clampInt(120, 5, 120, 30)).toBe(120);
    expect(clampInt('', 5, 120, 30)).toBe(30);
    expect(clampInt('7.8', 5, 120, 30)).toBe(7);
  });
});

describe('D-169 设置页不再有「保存」按钮', () => {
  it('页面上不出现 common.save', () => {
    // 保留一个只服务三处输入框、还隔着两张卡的按钮，正是「用户不知道该不该点」的来源。
    expect(settings).not.toContain("ui.t('common.save')");
  });

  it('顶部明确告知「改动立即生效」', () => {
    expect(settings).toContain("ui.t('settings.instant_hint')");
  });
});

describe('D-169 三处输入框都接了提交', () => {
  it('剪贴板 / 弱口令 / 超期 三个 onchange 都调 applySettings', () => {
    const calls = settings.match(/onchange=\{\(\) => void applySettings\(\)\}/g) ?? [];
    expect(calls).toHaveLength(3);
  });

  it('三个输入框的 id 都还在（否则等于悄悄删了一项设置）', () => {
    for (const id of ['set-ttl', 'set-weak', 'set-stale']) {
      expect(settings, id).toContain(`id="${id}"`);
    }
  });

  it('min/max 与常量同源，不再硬编码数字', () => {
    expect(settings).toContain('min={CLIPBOARD_TTL_MIN}');
    expect(settings).toContain('max={CLIPBOARD_TTL_MAX}');
    expect(settings).toContain('min={HEALTH_WEAK_MIN}');
    expect(settings).toContain('max={HEALTH_STALE_MAX}');
    // 反面判据：任何一个输入框上出现写死的 min="5" 都算回退。
    expect(settings).not.toMatch(/type="number" min="\d+"/);
  });
});

describe('D-169 写库走静默保存，但失败必须可见', () => {
  it('用 saveQuiet（逐项提交不该连弹「已保存」）', () => {
    expect(settings).toContain('settings.saveQuiet(');
  });

  it('保存失败要报错，不能静默', () => {
    // 静默失败 = 用户以为改生效了，正是本轮要消灭的那类误导。
    expect(settings).toContain("'settings.save_failed'");
    const block = settings.slice(
      settings.indexOf('async function applySettings'),
      settings.indexOf('async function changePassword'),
    );
    expect(block).toMatch(/if \(!ok\)/);
    expect(block).toContain("ui.toast('error'");
  });

  it('写成功后以库里的值为准（不在界面上留着「我以为的值」）', () => {
    const block = settings.slice(
      settings.indexOf('async function applySettings'),
      settings.indexOf('async function changePassword'),
    );
    expect(block).toContain('draft = $state.snapshot(settings.value)');
  });
});

describe('D-169 四种语言的说明与失败文案', () => {
  it('instant_hint / save_failed 四语齐备且非空', () => {
    for (const [name, dict] of [
      ['zh-CN', zhCN],
      ['zh-TW', zhTW],
      ['en-US', enUS],
      ['th', th],
    ] as const) {
      const bag = (dict as unknown as { settings: Record<string, string> }).settings;
      for (const key of ['instant_hint', 'save_failed']) {
        expect(bag[key], `${name}:settings.${key}`).toBeTruthy();
        expect((bag[key] ?? '').trim().length, `${name}:settings.${key}`).toBeGreaterThan(0);
      }
    }
  });
});
