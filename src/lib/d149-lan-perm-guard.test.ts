// D-149 的源码结构守卫：Android 17 的局域网权限必须有一条**能到达用户**的通道。
//
// 这条缺陷的形态是「全链路都编译通过、单测全绿、真机就是不同步」：`MainActivity` 从
// D-141 起就在申请 `ACCESS_LOCAL_NETWORK`，但结果只写 logcat。系统对未授权 UID 的
// AppOps 恒为 `ignore`，包被**静默丢弃**（无 RST），于是配对/同步表现成「永远超时」，
// 而用户能想到的排查方向（WiFi、路由器、防火墙、省电）全部无效。
//
// 这里钉住四类「写错不会报错、只会静默失效」的接线：
//
//   ① 插件注册名 ↔ Kotlin 包名/类名（不一致 → `run_mobile_plugin` 报插件不存在）；
//   ② 权限字符串在 Manifest / MainActivity / 插件三处逐字相同；
//   ③ 版本闸门（37）两处一致 —— 一处写 36、一处写 37 会让「支持」与「能申请」对不上；
//   ④ 前端判据必须是 `supported && !granted`：只判 `!granted` 会在桌面与旧版 Android 上
//      顶一条**永远消不掉**的警示，把用户赶去找一个不存在的开关。
//
// 另外钉住主人 2026-10-31 明确要求删掉的两处：状态页不再出现「无法扫码？」那句
// （配对页已有，状态页没有配对文本可复制，是死的指引）。

import { describe, expect, it } from 'vitest';

const raw = import.meta.glob(
  [
    '../../src-tauri/src/lib.rs',
    '../../src-tauri/src/commands/perm_cmd.rs',
    '../../src-tauri/gen/android/app/src/main/java/com/passwordms/app/MainActivity.kt',
    '../../src-tauri/gen/android/app/src/main/java/com/passwordms/app/LanPermissionPlugin.kt',
    '../../src-tauri/gen/android/app/src/main/AndroidManifest.xml',
    '../components/sync/SyncStatus.svelte',
    '../components/sync/PairShow.svelte',
    './ipc.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

/** 取源码；**必须归一化换行**——这些文件是 CRLF，带 `\n` 的字面量会恒不命中（D-147 踩过）。 */
function src(suffix: string): string {
  const text = Object.entries(raw).find(([p]) => p.endsWith(suffix))?.[1] ?? '';
  return text.replace(/\r\n/g, '\n');
}

describe('D-149：局域网权限通道的跨语言接线', () => {
  it('Rust 注册的插件类名与 Kotlin 的 package/class 逐字一致', () => {
    const libRs = src('/lib.rs');
    const kotlin = src('/LanPermissionPlugin.kt');

    // lib.rs 的注册点：register_android_plugin("<包名>", "<类名>")
    const reg = libRs.match(/register_android_plugin\("([^"]+)",\s*"([^"]+)"\)/g) ?? [];
    expect(reg.length, 'lib.rs 里应有 2 个 register_android_plugin（生物识别 + 局域网权限）').toBe(2);

    expect(kotlin).toContain('package com.passwordms.app');
    expect(kotlin).toContain('class LanPermissionPlugin');
    // 注册的是 "com.passwordms.app" + "LanPermissionPlugin"，两段必须分别对上。
    expect(libRs).toContain('register_android_plugin("com.passwordms.app", "LanPermissionPlugin")');
  });

  it('权限字符串在 Manifest / MainActivity / 插件三处逐字相同', () => {
    const perm = 'android.permission.ACCESS_LOCAL_NETWORK';
    expect(src('/AndroidManifest.xml')).toContain(`<uses-permission android:name="${perm}" />`);
    expect(src('/MainActivity.kt')).toContain(`private const val PERM_LOCAL_NETWORK = "${perm}"`);
    expect(src('/LanPermissionPlugin.kt')).toContain(
      `private const val PERM_LOCAL_NETWORK = "${perm}"`,
    );
  });

  it('版本闸门 37 在 MainActivity 与插件里一致（不许一处 36 一处 37）', () => {
    const main = src('/MainActivity.kt');
    const plugin = src('/LanPermissionPlugin.kt');
    expect(main).toContain('const val MIN_SDK_FOR_PERM = 37');
    expect(plugin).toContain('const val MIN_SDK_FOR_PERM = 37');
    // 两边都必须用常量判定，不许再出现裸 `SDK_INT < 37`（漂移风险点）。
    expect(main).not.toMatch(/SDK_INT\s*<\s*37/);
    expect(plugin).not.toMatch(/SDK_INT\s*<\s*37/);
  });

  it('插件不自己申请权限，而是转交 MainActivity 的已注册通道', () => {
    const main = src('/MainActivity.kt');
    const plugin = src('/LanPermissionPlugin.kt');
    // MainActivity 在 onCreate 里登记；插件按需取用，取不到必须 reject（不许假装成功）。
    expect(main).toMatch(/lanPermRequester\s*=\s*\{\s*cb\s*->/);
    expect(plugin).toContain('MainActivity.lanPermRequester');
    expect(plugin).toMatch(/requester == null[\s\S]{0,200}invoke\.reject/);
  });

  it('命令名两端一致，且桌面返回「不适用」而不是报错', () => {
    expect(src('/perm_cmd.rs')).toContain('pub fn lan_permission_status');
    expect(src('/perm_cmd.rs')).toContain('pub fn lan_permission_request');
    expect(src('/ipc.ts')).toContain(`invoke<LanPermissionDto>('lan_permission_status')`);
    expect(src('/ipc.ts')).toContain(`invoke<LanPermissionDto>('lan_permission_request')`);
    // 桌面分支：supported=false / granted=true（「这个问题在本平台不存在」）。
    expect(src('/perm_cmd.rs')).toMatch(
      /supported:\s*false,\s*\n\s*granted:\s*true/,
    );
  });
});

describe('D-149：同步状态页的判据与文案', () => {
  it('只有 supported 且未授予才渲染警示', () => {
    const svelte = src('/SyncStatus.svelte');
    // 判据必须落在**模板的 `{#if}`** 上，不能只要求「文件里出现过这个字符串」——
    // 首版就是被 `grantPerm()` 里那句同名判据喂饱了：把模板退化成 `!granted` 仍然全绿
    // （变异实测）。这里直接匹配模板行，退化成只判 `granted` 立刻红。
    expect(svelte).toMatch(/\{#if lanPerm !== null && lanPerm\.supported && !lanPerm\.granted\}/);
    // 只判 `!granted` 会让桌面 / Android 16 顶一条永远消不掉的警示。
    expect(svelte).toContain(`ui.t('sync.lan_perm_missing')`);
    expect(svelte).toContain(`api.sync.requestLanPermission()`);
  });

  it('状态页不再出现「无法扫码？」——它属于配对页，那里才有可复制的配对文本', () => {
    expect(src('/SyncStatus.svelte')).not.toContain('sync.manual_hint');
    // 配对页必须保留：那句话在那里是有效指引。
    expect(src('/PairShow.svelte')).toContain('sync.manual_hint');
  });
});
