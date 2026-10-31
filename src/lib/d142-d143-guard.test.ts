// D-142 / D-143 的源码结构守卫。
//
// 这两条都不是「算错了」，而是「少了一步判断」——单测能守住算法，守不住调用点，
// 所以这里钉的是**接线**：
//
// ① D-142（更新提示不消失）：读缓存必须带当前版本号过一遍。若有人把它改回
//    「先摆提示、下次检查再纠正」，绿色版每次升级都会重现同一句假提示。
// ② D-143（卸载即清库）：`hasFragileUserData`（系统卸载框的保留数据选项）与
//    「导出卡片顶部那条警示」必须同时在，且 `allowBackup` 不许被顺手打开
//    —— 后者会把加密库送进第三方云，是比「数据被删」更严重的方向错误。
//
// 读文件统一走 `import.meta.glob(?raw)`：与其余源码断言测试同一条路
// （tsconfig 没有 node 类型，`node:fs` 会让 `npm run check` 直接红）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  ['../components/Settings.svelte', '../../src-tauri/gen/android/app/src/main/AndroidManifest.xml'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const settingsSrc = Object.entries(raw).find(([p]) => p.endsWith('Settings.svelte'))?.[1] ?? '';
const manifest = Object.entries(raw).find(([p]) => p.endsWith('AndroidManifest.xml'))?.[1] ?? '';

describe('D-142 更新提示必须按当前版本过滤', () => {
  it('两类源码都读到了（守门本身别空跑）', () => {
    expect(settingsSrc.length).toBeGreaterThan(1000);
    expect(manifest.length).toBeGreaterThan(500);
  });
  it('进入设置页读暂存版本时带当前版本（禁止裸读缓存）', () => {
    expect(settingsSrc).toContain('pendingUpdateVersion(updateStorage(), appVersion)');
    // 旧 API 已移除：它返回的是「上次查到过什么」，不是「现在还有没有新版」。
    expect(settingsSrc).not.toContain('storedLatest');
  });

  it('手动检查结果也写回缓存（否则当场隐去、重进又冒）', () => {
    const manualBranch = settingsSrc.slice(settingsSrc.indexOf('} else if (result !== null) {'));
    expect(manualBranch.slice(0, 400)).toContain('setPendingUpdate');
  });
});

describe('D-143 卸载不许悄悄带走密码库', () => {
  it('Manifest 声明 hasFragileUserData（系统卸载框才会问「保留数据吗」）', () => {
    expect(manifest).toContain('android:hasFragileUserData="true"');
  });

  it('allowBackup 必须仍为 false：保险库不进第三方云', () => {
    expect(manifest).toContain('android:allowBackup="false"');
    expect(manifest).not.toContain('android:allowBackup="true"');
  });

  it('导出卡片顶部有常驻的卸载警示（在「加密副本」说明之前）', () => {
    const cardStart = settingsSrc.indexOf("ui.t('settings.export_vault')");
    expect(cardStart).toBeGreaterThan(-1);
    const card = settingsSrc.slice(cardStart, cardStart + 900);
    const warnAt = card.indexOf("ui.t('settings.uninstall_wipes_vault')");
    const copyAt = card.indexOf("ui.t('settings.export_confirm_body')");
    expect(warnAt).toBeGreaterThan(-1);
    expect(copyAt).toBeGreaterThan(-1);
    // 先读到「为什么必须导出」，再读「导出的是什么」。
    expect(warnAt).toBeLessThan(copyAt);
  });
});
