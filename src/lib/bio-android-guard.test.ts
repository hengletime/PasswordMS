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

import { localizeBioMessageKey } from './bio-platform';
import { APP_RESUMED_EVENT, shouldRefreshOnResume } from './app-resume';

const modules = import.meta.glob(
  [
    '../App.svelte',
    '../components/Settings.svelte',
    '../components/LockScreen.svelte',
    '../../src-tauri/gen/android/app/src/main/java/com/passwordms/app/BiometricPlugin.kt',
    '../../src-tauri/gen/android/app/src/main/res/values*/strings.xml',
    '../../src-tauri/src/commands/biometric_cmd.rs',
    '../../src-tauri/src/platform/mod.rs',
    '../../src-tauri/src/platform/android_keystore.rs',
    '../../src-tauri/src/lib.rs',
    '../../crates/passwordms-core/src/biometric.rs',
    './app-resume.ts',
    './stores/session.svelte.ts',
    './i18n/*.ts',
  ],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const appSvelte = modules['../App.svelte'] ?? '';
const appResumeSrc = modules['./app-resume.ts'] ?? '';

const settings = modules['../components/Settings.svelte'] ?? '';
const lockScreen = modules['../components/LockScreen.svelte'] ?? '';
const cmds = modules['../../src-tauri/src/commands/biometric_cmd.rs'] ?? '';
const platformMod = modules['../../src-tauri/src/platform/mod.rs'] ?? '';
const keystoreRs = modules['../../src-tauri/src/platform/android_keystore.rs'] ?? '';
const libRs = modules['../../src-tauri/src/lib.rs'] ?? '';

const kotlinRaw =
  Object.entries(modules).find(([k]) => k.endsWith('/BiometricPlugin.kt'))?.[1] ?? '';

const coreBio = modules['../../crates/passwordms-core/src/biometric.rs'] ?? '';
const sessionStore = modules['./stores/session.svelte.ts'] ?? '';

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
    // 四条（D-122 之后 `openSettings` 已删；原本五条）。
    expect(kotlinCmds.size).toBeGreaterThanOrEqual(4);

    // 只扫**代码**：注释里提到命令名（比如解释 D-122 为什么撤掉它）不算调用。
    const callSites = codeOnly([platformMod, keystoreRs].join('\n'));
    const rustCmds = [...callSites.matchAll(/run_mobile_plugin(?:::<[^>]*>)?\(\s*"([A-Za-z_]+)"/g)].map(
      (m) => m[1],
    );
    // 四条命令都被真正调到了（少一条说明有分支写错或没接上）。
    // D-122：`openSettings` 已从两侧一起删除——这条命令点了没反应且无从检测，撤得干净
    // 比留着「以后可能好使」更诚实。
    expect(new Set(rustCmds)).toEqual(new Set(['availability', 'seal', 'unseal', 'remove']));
    expect(rustCmds, 'Rust 还在调 openSettings，Kotlin 已经没有这个命令了').not.toContain(
      'openSettings',
    );
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
      'bio_manage_hint_android',
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

// ─────────────────────────────────────────────────────────────────────────────
// D-119 / D-120：主人 2026-10-28 真机实测暴露的两条
// ─────────────────────────────────────────────────────────────────────────────

describe('D-119 平台密钥永久失效必须当场作废封印', () => {
  it('core 只认 EnrollmentChanged 可撤销', () => {
    expect(coreBio).toContain('pub fn is_platform_revoked');
    const at = coreBio.indexOf('pub fn is_platform_revoked');
    const rest = coreBio.slice(at);
    const body = codeOnly(rest.slice(0, rest.indexOf('\n}\n') + 3));
    expect(body).toContain('InvalidatedReason::BiometricEnrollmentChanged');
    // 反向：其余三种理由一次都不许进判据——判成可撤销 = 删掉一条还能用的封印。
    for (const bad of ['Tampered', 'MasterPasswordChanged', 'VaultKeyChanged']) {
      expect(body, `${bad} 不该触发撤销`).not.toContain(bad);
    }
  });

  it('解锁失败时清平台钥匙并照原错误上报，且**不删**封印文件（墓碑留痕）', () => {
    const at = cmds.indexOf('pub fn biometric_unlock');
    expect(at).toBeGreaterThan(-1);
    const body = cmds.slice(at);
    const iCheck = body.indexOf('is_platform_revoked(&err)');
    const iPure = body.indexOf('purge_platform_key(&sealer, &bind)');
    const iErr = body.indexOf('return Err(err.into())');
    for (const [name, i] of [
      ['is_platform_revoked', iCheck],
      ['purge_platform_key', iPure],
      ['return Err', iErr],
    ] as const) {
      expect(i, `biometric_unlock 里找不到 ${name}——D-119 的收尾断了`).toBeGreaterThan(-1);
    }
    expect(iCheck).toBeLessThan(iPure);
    // 撤销之后仍然要把原因报上去（静默成功会让用户以为解锁成功）。
    expect(iErr).toBeGreaterThan(iCheck);
    expect(body).toContain('[biometric] platform key invalidated');
    // 反向：**不许**删文件。删了状态就退化成 Absent，「刚刚失效」与「从未开启」分不开，
    // 设置页既看不到原因、也没有 D-121 那个入口的触发条件。
    expect(body).not.toContain('store.remove()');
    // 墓碑由 core 写（命令层拿不到 rec）。
    expect(coreBio).toContain('invalidated: Some(InvalidatedMarker');
  });
});

describe('D-120 失效 / 取消文案跟着平台取词', () => {
  it('改写函数：android 加后缀，其余平台原样返回', () => {
    expect(localizeBioMessageKey('err.bio_invalidated_enrollment', 'android')).toBe(
      'err.bio_invalidated_enrollment_android',
    );
    expect(localizeBioMessageKey('err.biometric_canceled', 'android')).toBe(
      'err.biometric_canceled_android',
    );
    // Windows 自己就是那套名词，不该被改写。
    expect(localizeBioMessageKey('err.bio_invalidated_enrollment', 'windows')).toBe(
      'err.bio_invalidated_enrollment',
    );
    // 与平台无关的句子不进清单——多一套同义键只会让翻译漂移。
    expect(localizeBioMessageKey('err.bio_invalidated_password', 'android')).toBe(
      'err.bio_invalidated_password',
    );
    expect(localizeBioMessageKey('err.bio_invalidated_tampered', 'android')).toBe(
      'err.bio_invalidated_tampered',
    );
  });

  it('锁屏与设置页都走改写，不许直接吐后端的 messageKey', () => {
    expect(sessionStore).toContain('localizeBioMessageKey(appErr.messageKey');
    // 只看快速解锁那一段：主密码解锁的 catch 用的是同一句 toast，不该被这条断言牵连。
    const at = sessionStore.indexOf('async unlockWithBiometric');
    expect(at).toBeGreaterThan(-1);
    const body = sessionStore.slice(at, sessionStore.indexOf('\n  }', at));
    expect(body).not.toContain("ui.toast('error', appErr.messageKey)");
    expect(settings).toContain('localizeBioMessageKey(toAppError(err).messageKey, bioPlatform)');
  });

  it('四语言两套键齐全（缺键时 ui.t 会把 key 本身画到屏幕上）', () => {
    for (const [name, dict] of [
      ['zh-CN', zh],
      ['zh-TW', zhTw],
      ['en-US', en],
      ['th', th],
    ] as const) {
      for (const key of [
        'bio_invalidated_enrollment:',
        'bio_invalidated_enrollment_android:',
        'biometric_canceled:',
        'biometric_canceled_android:',
      ]) {
        expect(dict, `${name} 缺少 ${key}`).toContain(key);
      }
    }
  });

  it('android 变体里不出现 Windows Hello', () => {
    for (const [name, dict] of [
      ['zh-CN', zh],
      ['zh-TW', zhTw],
      ['en-US', en],
      ['th', th],
    ] as const) {
      for (const key of ['bio_invalidated_enrollment_android:', 'biometric_canceled_android:']) {
        const at = dict.indexOf(key);
        const line = dict.slice(at, dict.indexOf('\n', at));
        expect(line, `${name} 的 ${key} 里还有 Windows Hello`).not.toContain('Windows Hello');
      }
    }
  });
});

describe('D-121（B 案）系统生物识别设置入口的显示条件', () => {
  it('判据独立于「平台不可用」——失效可能发生在平台完全可用的时候', () => {
    const at = settings.indexOf('const bioSealBroken');
    expect(at, '缺少 bioSealBroken').toBeGreaterThan(-1);
    const sealLine = settings.slice(at, settings.indexOf('\n', at));
    expect(sealLine).toContain("'stale'");
    expect(sealLine).toContain("'corrupt'");

    const at2 = settings.indexOf('const bioShowSettingsEntry');
    expect(at2, '缺少 bioShowSettingsEntry').toBeGreaterThan(-1);
    const line = settings.slice(at2, settings.indexOf('\n', at2));
    // 两个条件缺一不可：平台不可用（去开通）**或**封印已失效（去看看指纹）。
    expect(line).toContain('!bioAvailable');
    expect(line).toContain('bioSealBroken');
  });

  it('入口由独立判据渲染，不再嵌在「平台不可用」块里', () => {
    const iCond = settings.indexOf('{#if bioShowSettingsEntry}');
    expect(iCond, '缺少 {#if bioShowSettingsEntry} 块').toBeGreaterThan(-1);
    const iBtn = settings.indexOf("ui.t('settings.bio_open_settings')");
    expect(iBtn).toBeGreaterThan(iCond);
    // 判据与按钮之间只允许夹一层平台判断，不许出现 {/if}
    // ——出现就说明按钮落进了别的块（旧写法就是这么把它埋掉的）。
    const between = settings.slice(iCond, iBtn);
    expect(between).toContain('bioCanOpenSettings(bioPlatform)');
    expect(between).not.toContain('{/if}');
    // 正常态（available + ready）两个条件都为假 ⇒ 不渲染，不打扰。
  });
});

describe('D-122 安卓撤掉跳转按钮，改为纯文案（主人 2026-10-28 真机「点了完全没反应」）', () => {
  it('Android 分支里是段落不是按钮', () => {
    const iCond = settings.indexOf('{#if bioShowSettingsEntry}');
    const iElse = settings.indexOf('{:else}', iCond);
    expect(iElse, '缺少 {:else} 分支').toBeGreaterThan(iCond);
    const iEnd = settings.indexOf('{/if}', iElse);
    const branch = settings.slice(iElse, iEnd);
    expect(branch, 'Android 分支必须给文案').toContain('bio_manage_hint_android');
    expect(branch, 'Android 分支不许再有按钮——点了没反应比没按钮更糟').not.toContain('<button');
  });

  it('Kotlin / Rust 里的跳转链路已删干净，不留死代码', () => {
    expect(kotlinRaw, 'Kotlin 还留着 openSettings').not.toContain('fun openSettings');
    // 查**代码**不查注释：注释里留着这条 action 名是为了说明为什么撤掉它，属正常。
    expect(codeOnly(kotlinRaw), 'Kotlin 还在用 ACTION_BIOMETRIC_ENROLL').not.toContain(
      'ACTION_BIOMETRIC_ENROLL',
    );
    expect(codeOnly(platformMod), 'Rust 还在调 run_mobile_plugin("openSettings")').not.toContain(
      '"openSettings"',
    );
    // Windows 那条留着：ms-settings 是文档化协议，稳定可用。
    expect(platformMod).toContain('open_hello_settings');
  });

  it('两个已撤销的键从词典里同步删掉（留着会让人以为按钮还在）', () => {
    for (const [name, dict] of [
      ['zh-CN', zh],
      ['zh-TW', zhTw],
      ['en-US', en],
      ['th', th],
    ] as const) {
      for (const dead of ['bio_avail_hint_android:', 'bio_open_settings_android:']) {
        expect(dict, `${name} 还留着已废弃的 ${dead}`).not.toContain(dead);
      }
      expect(dict, `${name} 缺少 bio_manage_hint_android`).toContain('bio_manage_hint_android:');
    }
  });

  it('文案只说「去设置的哪一类页面」，不列品牌、不写死菜单层级', () => {
    // 三条禁令，任何一条被破都说明文案又开始「假装知道对方的手机长什么样」：
    //   1. 不列品牌——全球机型太多，列三家等于告诉另外几百家「没为你考虑」；
    //   2. 不写死菜单层级——旧文案的「设置 → 安全 → 指纹、人脸与密码」抄的是 iOS 路由，
    //      安卓上没有这一层，等于把用户送去不存在的页面；
    //   3. 不承诺具体位置——各家 ROM 自己改，写死了就是下一次「点了没反应」。
    const dicts = [
      ['zh-CN', zh, ['华为', '小米', '各品牌']],
      ['zh-TW', zhTw, ['華為', '小米', '各品牌']],
      ['en-US', en, ['Huawei', 'Xiaomi', 'brand', 'stock Android']],
      ['th', th, ['Huawei', 'Xiaomi', 'แบรนด์']],
    ] as const;
    for (const [name, dict, banned] of dicts) {
      const at = dict.indexOf('bio_manage_hint_android:');
      // 键行 + 下一行（文案可能写在续行上）。用 fromCharCode 取换行，避免转义在传递中被改写。
      const line = dict.slice(at).split(String.fromCharCode(10)).slice(0, 2).join('');
      for (const word of banned) {
        expect(line, `${name} 的安卓提示又列品牌了：${word}`).not.toContain(word);
      }
      expect(line, `${name} 的安卓提示没写「设置」`).toMatch(/设置|設定|Settings|การตั้งค่า/);
    }
  });

  it('不承诺「切回来自动重新检测」（Android 上做不到，主人 2026-10-28 真机否掉）', () => {
    // 设置页确实挂了 `window.addEventListener('focus', …)` → `refreshBiometric()`，
    // 但那套在 **Windows** 上成立：窗口真的丢了焦点又拿回来。
    // Android 是 Activity 进后台再回来，WebView 里的 document 从未失去焦点，
    // `focus` 事件不会触发 ⇒ 状态停在旧值，用户照做后发现没变化，只会觉得这 App 在瞎说。
    //
    // 所以文案只说「去哪里做」，不承诺做完之后本窗口会怎样。
    // 想让它真成立，得在 Rust 侧接 `RunEvent::Resumed` 发事件再让前端重查（另开一条），
    // 不是靠改文案能补上的。
    const dicts = [
      ['zh-CN', zh, ['自动重新检测', '自动检测', '重新检测', '会自动']],
      ['zh-TW', zhTw, ['自動重新偵測', '自動偵測', '重新偵測', '會自動']],
      ['en-US', en, ['re-checks automatically', 'automatically', 'auto-refresh', 'auto ']],
      ['th', th, ['อัตโนมัติ', 'ตรวจสอบใหม่']],
    ] as const;
    for (const [name, dict, banned] of dicts) {
      const at = dict.indexOf('bio_manage_hint_android:');
      const line = dict.slice(at).split(String.fromCharCode(10)).slice(0, 2).join('');
      for (const word of banned) {
        expect(line, `${name} 的安卓提示又在承诺自动重查了：${word}`).not.toContain(word);
      }
    }
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// D-123：回前台重查——把上一轮删掉的那句承诺做成真的
// ─────────────────────────────────────────────────────────────────────────────

describe('D-123 宿主回前台（RunEvent::Resumed）驱动状态重查', () => {
  /** 取 `marker` 之后第一对引号里的内容（刻意不用正则，免得反斜杠在传递中被吞）。 */
  function quotedAfter(src: string, marker: string): string | null {
    const at = src.indexOf(marker);
    if (at < 0) return null;
    const dq = src.indexOf('"', at);
    const sq = src.indexOf("'", at);
    let start = -1;
    let quote = '';
    if (dq >= 0 && (sq < 0 || dq < sq)) {
      start = dq;
      quote = '"';
    } else if (sq >= 0) {
      start = sq;
      quote = "'";
    }
    if (start < 0) return null;
    const end = src.indexOf(quote, start + 1);
    if (end < 0) return null;
    return src.slice(start + 1, end);
  }

  it('事件名两侧逐字一致（跨语言字面值，写错不报错只静默失效）', () => {
    const rust = quotedAfter(libRs, 'APP_RESUMED_EVENT:');
    expect(rust, 'lib.rs 里找不到 APP_RESUMED_EVENT 常量').not.toBeNull();
    const ts = quotedAfter(appResumeSrc, 'APP_RESUMED_EVENT =');
    expect(ts, 'app-resume.ts 里找不到 APP_RESUMED_EVENT 常量').not.toBeNull();
    expect(rust).toBe(ts);
    // 运行时导入的那份值也必须一致（防止有人改了字面量却漏改常量导出）。
    expect(APP_RESUMED_EVENT).toBe(ts);
  });

  it('Rust 在 Resumed 分支里发事件，且该分支只编进移动端', () => {
    const code = codeOnly(libRs);
    const iResumed = code.indexOf('RunEvent::Resumed');
    expect(iResumed, 'lib.rs 没有处理 RunEvent::Resumed').toBeGreaterThan(-1);

    // 变体只在移动端存在；桌面目标不加 cfg 会直接编译失败（这条是那条编译期约束的护栏）。
    const before = code.slice(0, iResumed);
    const lastCfg = before.lastIndexOf('#[cfg(');
    expect(lastCfg, 'Resumed 分支前面没有 cfg 门').toBeGreaterThan(-1);
    const cfgLine = before.slice(lastCfg, before.indexOf(']', lastCfg) + 1);
    expect(cfgLine).toContain('target_os = "android"');
    expect(cfgLine).toContain('target_os = "ios"');

    // 必须 emit 那个常量，而不是手写字符串——手写就失去了上面的逐字比对保护。
    const arm = code.slice(iResumed, code.indexOf('_ => {}', iResumed));
    expect(arm).toContain('emit(APP_RESUMED_EVENT');
    expect(arm).not.toContain('"app://resumed"');
    expect(arm, 'emit 需要 Emitter trait').toContain('use tauri::Emitter');
  });

  it('前端在根组件注册监听（锁屏按钮也靠它），并只在 Tauri 宿主里注册', () => {
    const code = codeOnly(appSvelte);
    expect(code).toContain('registerResumeListener()');
    expect(code).toContain('listen(APP_RESUMED_EVENT');
    // 无宿主时 listen() 会抛错；Node 测试环境必须跳过。
    expect(code).toContain('hasTauriHost()');
    // 重查走 store 的既有入口，不许自己再造一条 IPC。
    expect(code).toContain('session.refreshBiometric()');
    expect(code, 'App.svelte 不许直接调 biometric_status').not.toContain('biometricStatus');
    // busy 时不插一脚：否则会把进行中的解锁/开关状态搅乱。
    expect(code).toContain('shouldRefreshOnResume(');
  });

  it('该不该重查：busy / 创建向导两个例外', () => {
    expect(shouldRefreshOnResume({ busy: false, route: 'lock' })).toBe(true);
    expect(shouldRefreshOnResume({ busy: false, route: 'main' })).toBe(true);
    // 正在解锁或正在开关快速解锁——重查会把按钮状态与进行中的操作打架。
    expect(shouldRefreshOnResume({ busy: true, route: 'lock' })).toBe(false);
    // 还没建库，`biometric_status` 必然失败，重查纯噪音。
    expect(shouldRefreshOnResume({ busy: false, route: 'create' })).toBe(false);
  });
});
