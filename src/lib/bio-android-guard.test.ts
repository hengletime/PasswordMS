// §23（D-113 P3）：Android Keystore 适配器的结构化守门。
//
// 这一轮改动有三条「写错了也能编译、能跑，但会在真机上以最难看的方式失败」的纪律，
// 单测夹具覆盖不到（要在真机按指纹），所以按本仓惯例（d81 / d100 / d103 / d108 / d114）
// 做源码级守门：
//
// 1. **Kotlin 侧绝不能阻塞主线程**。Tauri 的 `run_command` 是通过 `run_on_android_context`
//    把 JNI 调用投递到**主线程**的，`@Command` 就跑在主线程上；一旦在那里等对话框
//    （latch / sleep / runBlocking），BiometricPrompt 的窗口永远画不出来 → 死锁。
// 2. **四个命令必须 `(async)`**。Tauri 的同步命令直接在调用线程上跑函数体（`body_blocking`
//    不派发），而 Rust 侧 `run_mobile_plugin` 会阻塞等 Kotlin 回复——留在主线程就是同一个死锁。
// 3. **加密与在场验证必须绑在一起**（`CryptoObject`），否则「认证过了但用另一把钥匙开」
//    就重新变成可能，Android 相对 Windows 的那点优势荡然无存。
//
// 读源码用 `import.meta.glob('?raw')`：本仓禁止 `node:fs`（svelte-check 不认 node types）。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(
  [
    '../components/Settings.svelte',
    '../components/LockScreen.svelte',
    '../../src-tauri/gen/android/app/src/main/java/com/passwordms/app/BiometricPlugin.kt',
    '../../src-tauri/gen/android/app/src/main/res/values*/strings.xml',
    '../../src-tauri/src/commands/biometric_cmd.rs',
    '../../src-tauri/src/platform/mod.rs',
    '../../src-tauri/src/platform/android_keystore.rs',
    '../../src-tauri/src/lib.rs',
    './i18n/*.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const settings = modules['../components/Settings.svelte'] ?? '';
const lockScreen = modules['../components/LockScreen.svelte'] ?? '';
const cmds = modules['../../src-tauri/src/commands/biometric_cmd.rs'] ?? '';
const platformMod = modules['../../src-tauri/src/platform/mod.rs'] ?? '';
const keystoreRs = modules['../../src-tauri/src/platform/android_keystore.rs'] ?? '';
const libRs = modules['../../src-tauri/src/lib.rs'] ?? '';

const kotlinRaw =
  Object.entries(modules).find(([k]) => k.endsWith('/BiometricPlugin.kt'))?.[1] ?? '';

const zh = modules['./i18n/zh-CN.ts'] ?? '';
const zhTw = modules['./i18n/zh-TW.ts'] ?? '';
const en = modules['./i18n/en-US.ts'] ?? '';
const th = modules['./i18n/th.ts'] ?? '';

/** 去掉注释后的 Kotlin 代码：注释里会**故意**提到被禁用的 API（说明为什么不许用）。 */
function codeOnly(src: string): string {
  return src
    .split('\n')
    .filter((line) => {
      const t = line.trim();
      return !(t.startsWith('//') || t.startsWith('*') || t.startsWith('/*'));
    })
    .join('\n');
}

describe('§23 Kotlin 插件存在且被正确注册', () => {
  it('Kotlin 源码读到（glob 写错会退化成假绿）', () => {
    expect(kotlinRaw.length).toBeGreaterThan(2000);
    expect(kotlinRaw).toContain('class BiometricPlugin');
  });

  it('Tauri 以正确的类名注册插件，并把句柄存进托管状态', () => {
    // 第二参必须是**类名**（register_android_plugin 会拼成 com/passwordms/app/BiometricPlugin），
    // 与 Builder::new 的插件名不是一回事，写反了会在真机上才炸。
    expect(libRs).toContain('register_android_plugin("com.passwordms.app", "BiometricPlugin")');
    expect(libRs).toContain('BiometricHandle');
    expect(libRs).toContain('.manage(BiometricHandle(');
  });
});

describe('§23 纪律一：Kotlin 侧绝不阻塞主线程', () => {
  it('代码里不出现任何阻塞等待（注释提到它们是被允许的）', () => {
    const code = codeOnly(kotlinRaw);
    for (const forbidden of ['CountDownLatch', 'Thread.sleep', 'runBlocking', '.join()', '.await(']) {
      expect(code, `Kotlin 里出现了阻塞等待：${forbidden}（会让 BiometricPrompt 画不出来）`).not.toContain(
        forbidden,
      );
    }
  });

  it('@Command 里不做加解密，只在对话框回调里 resolve', () => {
    const code = codeOnly(kotlinRaw);
    // 加密/解密的实际动作必须在 `authenticate(...)` 的 action 闭包里（即回调内），
    // 所以 doFinal 的调用次数应为 2（seal 一处、unseal 一处），且都在 authenticate 之后。
    const doFinalIdx = [...code.matchAll(/\.doFinal\(/g)].map((m) => m.index ?? 0);
    const authIdx = [...code.matchAll(/authenticate\(invoke, cipher\)/g)].map((m) => m.index ?? 0);
    expect(doFinalIdx.length).toBe(2);
    expect(authIdx.length).toBe(2);
    for (const d of doFinalIdx) {
      // 每个 doFinal 都必须落在某个 authenticate 调用之后（同一函数内、按源码顺序）。
      expect(authIdx.some((a) => a < d)).toBe(true);
    }
  });
});

describe('§23 纪律二：命令必须 (async)，否则 Android 必死锁', () => {
  it('五个受影响的命令都带 async 标记', () => {
    const marked = cmds.match(/#\[tauri::command\(rename_all = "snake_case", async\)\]/g) ?? [];
    // biometric_status / _enable / _disable / _unlock / open_biometric_settings
    expect(marked.length).toBe(5);
    // 反向：不许留下不带 async 的生物识别命令（会漏掉死锁防线）。
    expect(cmds).not.toContain('#[tauri::command(rename_all = "snake_case")]\npub fn biometric');
  });

  it('platform_sealer 吃 AppHandle（Android 要从 state 取 PluginHandle）', () => {
    expect(platformMod).toContain('pub fn platform_sealer(app: &tauri::AppHandle)');
    expect(platformMod).toContain('KeystoreSealer::new(app.state::<crate::BiometricHandle>()');
  });
});

describe('§23 纪律三：加解密与在场验证必须绑定', () => {
  it('用 CryptoObject 把 cipher 交给对话框', () => {
    expect(kotlinRaw).toContain('BiometricPrompt.CryptoObject(cipher)');
  });

  it('bind 走 GCM 的 AAD（与 Windows 的 DPAPI entropy 语义对齐）', () => {
    const code = codeOnly(kotlinRaw);
    // `updateAAD(bind)` 必须出现两次（加密一次、解密一次），且都用同一个 bind 参数——
    // 若某个方向漏传，换库/换 VK 之后仍能解开，绑定就形同虚设。
    expect((code.match(/updateAAD\(bind\)/g) ?? []).length).toBe(2);
  });

  it('新增/删除指纹必须让密钥失效（安全语义，禁止被改成 false）', () => {
    expect(codeOnly(kotlinRaw)).toContain('.setInvalidatedByBiometricEnrollment(true)');
    expect(codeOnly(kotlinRaw)).not.toContain('.setInvalidatedByBiometricEnrollment(false)');
  });

  it('密钥失效后必须删掉旧别名再重建（否则「重新开启」永远失败）', () => {
    const code = codeOnly(kotlinRaw);
    const seal = code.slice(code.indexOf('fun seal(invoke: Invoke)'), code.indexOf('fun unseal(invoke: Invoke)'));
    const inval = seal.indexOf('catch (e: KeyPermanentlyInvalidatedException)');
    const del = seal.indexOf('deleteKey(alias)', inval);
    const redo = seal.indexOf('cipherForSeal(alias)', del);
    expect(inval).toBeGreaterThan(-1);
    // 顺序必须是「捕获失效 → 删别名 → 重建」：只捕获不删，那个废掉的别名会一直占着坑，
    // 用户点「重新开启」永远抛同一个异常（表现为「点了没反应」）。
    expect(del).toBeGreaterThan(inval);
    expect(redo).toBeGreaterThan(del);
    // 写封印这条路径**不该**再报 invalidated：失效在那里是「重建」而不是「死路」。
    expect(seal).not.toContain('"invalidated"');
  });

  it('Rust 侧调用的插件命令名与 Kotlin 的 @Command 函数名逐字一致', () => {
    // Tauri 的 `PluginHandle` 是 `commands[method.name] = ...` —— 命令名就是 Kotlin
    // 函数名的字面值，**不做 snake_case 转换**。所以 Rust 写 `open_settings` 而 Kotlin
    // 写 `openSettings` 时：编译过、跑得动、按钮点了没反应（错误还被吞成「打不开」）。
    const kotlinCmds = new Set(
      [...codeOnly(kotlinRaw).matchAll(/@Command\s+fun\s+(\w+)/g)].map((m) => m[1]),
    );
    expect(kotlinCmds.size).toBeGreaterThanOrEqual(5);

    const callSites = [platformMod, keystoreRs].join('\n');
    const rustCmds = [...callSites.matchAll(/run_mobile_plugin(?:::<[^>]*>)?\(\s*"([A-Za-z_]+)"/g)].map(
      (m) => m[1],
    );
    // 五条命令都被真正调到了（少一条说明有分支写错或没接上）。
    expect(new Set(rustCmds)).toEqual(new Set(['availability', 'seal', 'unseal', 'remove', 'openSettings']));
    for (const cmd of rustCmds) {
      expect(kotlinCmds.has(cmd), `Rust 调了 ${cmd}，但 Kotlin 没有同名 @Command 函数`).toBe(true);
    }
  });

  it('「未认证」不能谎报成用户取消（静默会变成一次无解释的失败）', () => {
    const code = codeOnly(kotlinRaw);
    const i = code.indexOf('UserNotAuthenticatedException');
    expect(i).toBeGreaterThan(-1);
    // 取该 catch 后面一小段，断言它落到 platform 而不是 canceled。
    const seg = code.slice(i, i + 260);
    expect(seg).toContain('"platform"');
    expect(seg).not.toContain('"canceled"');
  });

  it('每次使用都要认证，不是「认证后一段时间内有效」', () => {
    const code = codeOnly(kotlinRaw);
    // API 30+：timeout = 0（auth-per-use）；API 24–29：-1（每次都要）。
    expect(code).toContain('setUserAuthenticationParameters(0,');
    expect(code).toContain('setUserAuthenticationValidityDurationSeconds(-1)');
  });
});

describe('§23 撤销必须彻底', () => {
  it('关闭快速解锁时删掉平台密钥，而不是只删文件', () => {
    expect(cmds).toContain('purge_platform_key');
    // bind 必须在删文件**之前**算好（删完读不到头部）。
    const bindIdx = cmds.indexOf('biometric::bind_of(');
    const removeIdx = cmds.indexOf('SealStore::new(&path).remove()');
    expect(bindIdx).toBeGreaterThan(0);
    expect(removeIdx).toBeGreaterThan(bindIdx);
  });

  it('platform 层为「非 Android」提供空实现（DPAPI 没有可删的密钥句柄）', () => {
    expect(platformMod).toContain('pub fn purge_platform_key(_sealer: &PlatformSealer, _bind: &[u8]) {');
  });
});

describe('§23 前端按平台取词，不再把 Windows 名词搬给安卓', () => {
  it('两个入口都引入平台判定', () => {
    expect(settings).toContain('bioTextSuffix');
    expect(settings).toContain('bioPlatformOf');
    expect(lockScreen).toContain('bioTextSuffix');
  });

  it('卡片的主文案与可用性文案都带平台后缀', () => {
    for (const key of ['bio_title', 'bio_desc', 'bio_risk', 'bio_degraded_hint']) {
      expect(settings, `${key} 必须带平台后缀`).toContain(`settings.${key}\${bioSuffix}`);
    }
    // 通用句（「本机尚未设置 {noun}」）用插值而非后缀——名词由 bioNoun 提供。
    expect(settings).toContain('{ noun: bioNoun }');
  });

  it('锁屏按钮同样按平台取词', () => {
    expect(lockScreen).toContain('lock.biometric_unlock${bioSuffix}');
    expect(lockScreen).toContain('lock.biometric_waiting${bioSuffix}');
  });

  it('四语言都补齐 §23 的新键（键以 zh-CN 为唯一事实来源）', () => {
    const keys = [
      'bio_noun',
      'bio_noun_android',
      'bio_title_android',
      'bio_desc_android',
      'bio_risk_android',
      'bio_avail_hint_android',
      'bio_open_settings_android',
      'bio_degraded_hint_android',
      'bio_state_stale_enrollment_android',
      'password_changed_bio_off_android',
      'biometric_unlock_android',
      'biometric_waiting_android',
    ];
    for (const dict of [zh, zhTw, en, th]) {
      for (const key of keys) {
        expect(dict, `缺少 ${key}`).toContain(`${key}:`);
      }
    }
  });

  it('Android 那句风险说明讲的是「新增指纹即失效」，不是 Windows 的凭据绕过', () => {
    expect(zh).toContain('新增或删除指纹 / 人脸记录后，已保存的密钥会永久失效');
    // 反向：Android 文案里不许再出现 Windows 专属名词。
    const androidRisk = zh.slice(zh.indexOf('bio_risk_android'));
    expect(androidRisk.slice(0, 200)).not.toContain('Windows Hello');
  });
});

describe('§23 对话框文案四语言齐全（系统语言跟随，与应用内语言无关）', () => {
  it('四个 values 目录都有 bio_prompt_title / subtitle / cancel', () => {
    const resFiles = Object.entries(modules).filter(([k]) => k.endsWith('/strings.xml'));
    expect(resFiles.length).toBeGreaterThanOrEqual(4);
    for (const [path, content] of resFiles) {
      for (const key of ['bio_prompt_title', 'bio_prompt_subtitle', 'bio_prompt_cancel']) {
        expect(content, `${path} 缺少 ${key}`).toContain(`name="${key}"`);
      }
    }
  });
});
