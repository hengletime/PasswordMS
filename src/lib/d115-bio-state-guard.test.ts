// D-115：主人真机走查报出的两件事的守门。
//
// 1. 开启快速解锁时主密码输错 → 提示「解锁失败，请重试」。
//    根因：`Session::verify_master_password` 把失败折叠成 `CoreError::UnlockFailed`，
//    而那句文案是**锁屏入口**专用（A-05：「密码错 / 头部坏」不可区分）。
//    本 API 只在**已解锁会话**里可达，调用者已认证且手里就有容器文件，
//    「密码对不对」对它不是新信息 —— 继续折叠纯属误导用户。
//
// 2. Hello 被关掉后，状态行「已开启（日期）」与红字「未检测到可用的 Windows Hello」
//    同屏，自相矛盾；而重新开通 Hello 后快速解锁**不需要重输主密码**就恢复了。
//    这不是漏洞（`biometric::unlock` 先查可用性，Hello 关着时一次都用不了），
//    但界面没解释「为什么开关还在」「撤销到底靠什么」，必须写清楚。
//
// 组件没有单测夹具，按本项目惯例（d81 / d85 / d100 / d103 / d108 / d114）做源码级守门。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(
  ['../components/Settings.svelte', './errors.ts', './i18n/*.ts'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const settings = modules['../components/Settings.svelte'] ?? '';
const errors = modules['./errors.ts'] ?? '';
const zh = modules['./i18n/zh-CN.ts'] ?? '';
const zhTw = modules['./i18n/zh-TW.ts'] ?? '';
const en = modules['./i18n/en-US.ts'] ?? '';
const th = modules['./i18n/th.ts'] ?? '';

/** 截出 `<script>` 里某个顶层 `async function` 的函数体。 */
function fnBody(src: string, name: string): string {
  const start = src.indexOf(`async function ${name}(`);
  if (start < 0) {
    return '';
  }
  const end = src.indexOf('\n  }\n', start);
  return end < 0 ? src.slice(start) : src.slice(start, end);
}

describe('D-115 主密码错就是「主密码错」', () => {
  it('前端认识 ERR_WRONG_MASTER_PASSWORD，且兜底到 err.wrong_master_password', () => {
    expect(errors).toContain('ERR_WRONG_MASTER_PASSWORD');
    expect(errors).toContain("'err.wrong_master_password'");
  });

  it('开启快速解锁失败时**就地**显示在输入框下，不再只弹右下角 toast', () => {
    const body = fnBody(settings, 'submitEnableBiometric');
    expect(body).not.toBe('');
    // 不允许再退回 toast：用户的眼睛在输入框上。
    expect(body).not.toContain('toast(\'error\'');
    // D-120 起这句还要按平台取词，但「就地显示」的意图不变——仍然不许退回 toast。
    expect(body).toContain('bioErrorKey = localizeBioMessageKey(toAppError(err).messageKey');
    // 输入框下方要有 role="alert" 的一行。
    expect(settings).toMatch(/\{#if bioErrorKey !== ''\}[\s\S]{0,400}role="alert"/);
    // 改输入即清（避免「上一次错了」与「这一次在输」同屏）。
    expect(settings).toContain("oninput={() => (bioErrorKey = '')}");
  });

  it('改密失败也常驻显示（同一张卡片上成功留、失败不能只闪 3 秒）', () => {
    const body = fnBody(settings, 'changePassword');
    expect(body).toContain('pwdErrorKey = toAppError(err).messageKey');
    // 顶部的「两次新密码不一致」仍可 toast（那是输入校验，不是后端失败）；
    // 这里禁的是把**后端错误**也丢进 toast。
    expect(body).not.toContain('toast(\'error\', toAppError(err)');
    expect(settings).toMatch(/\{#if pwdErrorKey !== ''\}[\s\S]{0,300}role="alert"/);
  });

  it('四语言都补齐主密码错文案，且不得再复用「解锁失败」那句', () => {
    for (const dict of [zh, zhTw, en, th]) {
      expect(dict).toContain('wrong_master_password:');
    }
    // zh-CN 的「解锁失败，请重试」只属于锁屏；新增的那条必须点名「主密码」。
    expect(zh).toContain('主密码错误');
  });
});

describe('D-115 「已开启」不许与「不可用」同屏打架', () => {
  it('降级态要单独成一种显示状态（封印完好 ≠ 平台现在可用）', () => {
    expect(settings).toContain('const bioDegraded');
    expect(settings).toContain("bioEnabled && bio !== null && !bioAvailable");
    // 状态行必须先判降级，再判普通「已开启」——顺序反了就永远走不到降级分支。
    expect(settings.indexOf("ui.t('settings.bio_state_on_unavailable'")).toBeLessThan(
      settings.indexOf("ui.t('settings.bio_state_on',"),
    );
  });

  it('降级时必须解释「为什么开关还在 / 撤销靠什么」', () => {
    // §23 起键名带平台后缀（`${bioSuffix}`），这里断言的是「渲染点仍然引用这个键」，
    // 而不是某个固定的字面量写法——否则每加一个平台就要改一次测试。
    expect(settings).toContain('settings.bio_degraded_hint${bioSuffix}');
    // 这句话必须明确「撤销要按关闭」，不能只说「重新开通就恢复」。
    expect(zh).toContain('要彻底撤销请点「关闭」');
    // 也必须承认「关闭期间一次也用不了」（这才是真实安全语义）。
    expect(zh).toContain('一次也用不了');
  });

  it('四语言都补齐两个新键（键以 zh-CN 为唯一事实来源）', () => {
    for (const dict of [zh, zhTw, en, th]) {
      for (const key of ['bio_state_on_unavailable', 'bio_degraded_hint']) {
        expect(dict, `缺少 ${key}`).toContain(`${key}:`);
      }
    }
    // `{date}` 占位必须在——否则「已开启，但当前不可用」会丢掉时间信息。
    for (const dict of [zh, zhTw, en, th]) {
      const line = dict.split('\n').find((l) => l.includes('bio_state_on_unavailable:')) ?? '';
      expect(line).toContain('{date}');
    }
  });
});
