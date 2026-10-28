// D-114：主人真机实测报出的三条 UX 缺陷的守门。
//
// 1. 改主密码成功后只 `ui.toast('success', 'common.ok')` —— `common.ok` 是按钮标签
//    「确定」，等于什么也没说，而且 3 秒自动消失 ⇒ 主人原话「右下角一闪而过」。
// 2. 命令层在改密成功后按 §18.5 约束 2 删掉快速解锁封印，前端既不重查也不解释
//    ⇒ 用户只看到「快速解锁悄悄没了」，不知道是自己操作导致的。
// 3. Windows Hello 从未配置时，`UserConsentVerifier` 回报的是 `DeviceNotPresent`
//    （不是 `NotConfiguredForUser`），文案却断言「本机没有可用的硬件」⇒ 主人以为坏了，
//    真正的下一步「去开通 Hello」被彻底藏住。
//
// 这三条都是**文案/流程**层面的修复，没有可断言的运行时行为（组件没有单测夹具），
// 所以按本项目既有惯例（d81 / d85 / d100 / d103 / d108）做源码级守门。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(['../components/Settings.svelte', './i18n/*.ts', './bio-platform.ts'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

const settings = modules['../components/Settings.svelte'] ?? '';
const bioPlatform = modules['./bio-platform.ts'] ?? '';
const zh = modules['./i18n/zh-CN.ts'] ?? '';
const zhTw = modules['./i18n/zh-TW.ts'] ?? '';
const en = modules['./i18n/en-US.ts'] ?? '';
const th = modules['./i18n/th.ts'] ?? '';

/** 截出 `<script>` 里某个顶层 `async function` 的函数体（到下一个 2 空格缩进的 `}` 为止）。 */
function fnBody(src: string, name: string): string {
  const start = src.indexOf(`async function ${name}(`);
  if (start < 0) {
    return '';
  }
  const end = src.indexOf('\n  }\n', start);
  return end < 0 ? src.slice(start) : src.slice(start, end);
}

describe('D-114 改密回执不能只有一句「确定」', () => {
  it('changePassword 不再把 `common.ok` 当成功文案（那就是按钮上的「确定」）', () => {
    const body = fnBody(settings, 'changePassword');
    expect(body).not.toBe('');
    expect(body).not.toContain("'common.ok'");
    // 新文案必须点名「主密码已修改」，并且区分「快速解锁因此关闭」这一档。
    expect(body).toContain('settings.password_changed');
    expect(body).toContain('settings.password_changed_bio_off');
  });

  it('改密成功后必须重查快速解锁状态（否则卡片还谎报「已开启」）', () => {
    const body = fnBody(settings, 'changePassword');
    expect(body).toContain('session.refreshBiometric()');
    // 是否开着要**改密前**取：改密之后封印已被删除，再取永远是 false。
    expect(body).toContain('const bioWasOn = bioEnabled;');
    expect(body.indexOf('const bioWasOn')).toBeLessThan(
      body.indexOf('changeMasterPassword'),
    );
  });

  it('回执是常驻结果条（`role="status"` + is-success），不是一闪而过的 toast', () => {
    expect(settings).toContain('pwd-result');
    // 用 `.pms-alert.is-success`（带底色的一条）而不是 `.pms-hint`（0.78rem 小字）。
    expect(settings).toMatch(/class="pms-alert is-success[^"]*"\s+role="status"/);
    // 三个输入框任一改动都要清掉上一条回执，避免「已修改」与「修改失败」同屏。
    // D-115 起失败回执也走同一处清理，故三个 `oninput` 统一调 `clearPwdReceipt`
    // （它内部同时清 `pwdChanged` 与 `pwdErrorKey`）——计数仍是 3，语义比原来更强。
    const clears = settings.match(/oninput=\{clearPwdReceipt\}/g) ?? [];
    expect(clears).toHaveLength(3);
  });
});

describe('D-114 「没有硬件」不许替系统断言', () => {
  it('not_present 文案不再声称缺少硬件，而是给出下一步', () => {
    expect(zh).toContain('bio_avail_not_present');
    expect(zh).not.toContain('本机没有可用的 Windows Hello 硬件');
    expect(zh).toContain('不一定缺少硬件');
  });

  it('两种「没配好」都要有可点的出口：引导语 + 打开登录选项按钮', () => {
    // §23 起这段判断搬进了 `bioShouldOfferSettings()`，由 Settings 与测试共用同一份真相
    // （Android 复用同一条逻辑，只是目标页面换成系统生物识别设置）。所以这里断言两件事：
    // 调用点用的就是这个函数，而那个函数本身覆盖了两种「没配好」。
    expect(settings).toContain('bioShouldOfferSettings(bio.availability)');
    expect(bioPlatform).toContain("availability === 'not_present'");
    expect(bioPlatform).toContain("availability === 'not_configured'");
    expect(settings).toContain('settings.bio_avail_hint');
    expect(settings).toContain('settings.bio_open_settings');
    expect(settings).toContain('api.vault.openBiometricSettings()');
  });

  it('窗口重新获得焦点要重查状态（去系统设置开通后切回来立刻生效）', () => {
    expect(settings).toContain("window.addEventListener('focus'");
    // 必须解绑，否则每次进出设置页都会叠一个监听。
    expect(settings).toContain("window.removeEventListener('focus'");
  });

  it('四语言都补齐 5 个键（键以 zh-CN 为唯一事实来源）', () => {
    const keys = [
      'bio_avail_not_present',
      'bio_avail_hint',
      'bio_open_settings',
      'password_changed',
      'password_changed_bio_off',
    ];
    for (const dict of [zh, zhTw, en, th]) {
      for (const key of keys) {
        expect(dict, `缺少 ${key}`).toContain(`${key}:`);
      }
    }
  });
});
