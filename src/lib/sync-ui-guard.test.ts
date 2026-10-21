// D-66 / D-67 / D-69 的**组件层**防回归（源码结构断言）。
//
// 为什么光有 `device-label.test.ts` 不够：那三个纯函数本身对，但组件可以不用它们、
// 用错方向、或者把 `device_id` 另外再渲染一份。这类「纯函数绿、界面仍错」的漂移
// 编译器和单测都发现不了，因此照 `legal-ui.test.ts` 的先例用 `?raw` 读组件源码断言。
//
// 守五条：
// 1. D-67 / D-69：设备行的指纹文本必须来自 `fingerprintText(device.fingerprint)`（**单参**，
//    不再收 `device_id`），且整行只在 `hasSessionCode(device.fingerprint)` 为真时渲染；
//    `device_id` 只能作为 `title` 悬停线索出现，不得再以文本节点身份冒充指纹；
// 2. D-66：`SyncStatus.svelte` 的按钮忙碌态必须走 `syncButtonState()`，**不得**再碰
//    store 级 `sync.busy`（那正是「点一台两台一起转圈」的成因）；
// 3. D-69 方向：`PairShow.svelte`（电脑/主机屏）显示 `sync.pairing.session_code`（本次会话码），
//    `PairScan.svelte`（手机/加入屏）显示 `sync.lastPairSessionCode`（同一值）——不可互换；
//    **正向与负向断言必须在同一个 it 里成对**，否则整块被删也能绿（见该 describe 注释）；
// 4. D-69 守卫：两个核对块都必须由 `hasSessionCode()` 判空后再渲染，
//    不允许「无条件渲染 + 空值留白」；
// 5. D-69 文案：两屏不得再引用 `sync.fingerprint`（那是**设备**指纹，与本次会话无关）。

import { describe, expect, it } from 'vitest';

const modules = import.meta.glob(
  '../components/sync/{DeviceList,SyncStatus,PairShow,PairScan}.svelte',
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>;

const deviceList = modules['../components/sync/DeviceList.svelte'] ?? '';
const syncStatus = modules['../components/sync/SyncStatus.svelte'] ?? '';
const pairShow = modules['../components/sync/PairShow.svelte'] ?? '';
const pairScan = modules['../components/sync/PairScan.svelte'] ?? '';

/** 取模板部分（`</script>` 之后），只看真正上屏的结构。 */
function template(source: string): string {
  const at = source.indexOf('</script>');
  expect(at, '组件应有 </script> 结束脚本块').toBeGreaterThan(-1);
  return source.slice(at);
}

describe('组件源码可读取', () => {
  it('四个同步相关组件都被 import.meta.glob 读到', () => {
    expect(deviceList.length).toBeGreaterThan(500);
    expect(syncStatus.length).toBeGreaterThan(500);
    expect(pairShow.length).toBeGreaterThan(500);
    expect(pairScan.length).toBeGreaterThan(500);
  });
});

describe('D-67 / D-69：设备行指纹必须是真指纹，不得用 device_id 冒充', () => {
  it('DeviceList 从 lib/device-label 引入 fingerprintText 与 hasSessionCode', () => {
    expect(deviceList).toContain("import { fingerprintText, hasSessionCode } from '../../lib/device-label'");
  });

  it('指纹那一段用单参 fingerprintText(device.fingerprint) 渲染，device_id 只做 title', () => {
    const tpl = template(deviceList);
    // 同一个 <span> 内既要带 title 悬停线索，又要用纯函数取文本。
    const span = tpl.match(/<span[^>]*class="sync-mono"[^>]*>[\s\S]*?<\/span>/);
    expect(span, 'DeviceList 应有指纹文本的 <span class="sync-mono">').not.toBeNull();
    expect(span?.[0]).toContain('title={device.device_id}');
    expect(span?.[0]).toContain('fingerprintText(device.fingerprint)');
    // D-69 负向：第二参（device_id）不许回到调用点 —— 那个形参正是「空则回退 UUID」的载体。
    expect(span?.[0]).not.toContain('fingerprintText(device.fingerprint, device.device_id)');
  });

  it('指纹整行只在 hasSessionCode(device.fingerprint) 为真时渲染（空指纹不留 UUID 兜底）', () => {
    const tpl = template(deviceList);
    // 正向：判空守卫必须存在。
    expect(tpl).toMatch(/\{#if\s+hasSessionCode\(device\.fingerprint\)\}/);
    // 尾随校验：指纹副行必须被包在该守卫内（守卫与副行之间不得先闭合 {/if}）。
    const at = tpl.indexOf('{#if hasSessionCode(device.fingerprint)}');
    expect(at).toBeGreaterThan(-1);
    const guarded = tpl.slice(at, tpl.indexOf('{/if}', at));
    expect(guarded, '指纹副行必须落在 hasSessionCode 守卫块内').toContain("ui.t('sync.fingerprint')");
  });

  it('device_id 绝不以文本节点身份出现（不得再冒充指纹/名称）', () => {
    const tpl = template(deviceList);
    // `>…{device.device_id}…<` 这种形态 = 直接把 UUID 当可见文本渲染。
    expect(tpl).not.toMatch(/>\s*\{device\.device_id\}\s*</);
    // 反向锚点：真指纹函数必须在模板里被调用过一次。
    expect(tpl).toContain('fingerprintText(');
  });
});

describe('D-66：立即同步按钮的忙碌态不得回退到 store 级 busy', () => {
  it('SyncStatus 用 syncButtonState 解算每行的 { syncing, disabled }', () => {
    expect(syncStatus).toContain("import { syncButtonState } from '../../lib/device-label'");
    expect(template(syncStatus)).toContain('syncButtonState(sync.syncingDeviceId, device.device_id)');
  });

  it('SyncStatus 模板里不再出现 sync.busy（回归到「全局一个布尔」就会红）', () => {
    expect(template(syncStatus)).not.toContain('sync.busy');
    // 转圈条件只认 st.syncing。
    expect(template(syncStatus)).toContain('{#if st.syncing}');
  });
});

// D-69：两屏显示的是**同一个会话码**（`sha256(本次临时公钥)` 前 8 字节的小写 hex）。
// 方向不可反：主机屏取 `sync.pairing.session_code`，加入屏取 `sync.lastPairSessionCode`。
//
// ⚠️ 正向与负向断言**必须成对出现在同一个 it 里**。理由：单看一条
// `not.toContain(对端名字)`，它在「组件整块被删掉」时也是绿的 —— 负向断言天然对
// 「什么都不做」放行。只有与正向同处一个 it，「用自己的」+「不用对方的」两条加起来
// 才叫方向守门。D-68 时代把负向单独放一个 it，改名之后那两条
// `not.toContain(旧名字)` 就恒绿了（本轮 D-69 修的就是这个静默失效）。
describe('D-69：配对两屏的会话码方向不可反', () => {
  it('主机屏（PairShow）：用自己的 sync.pairing.session_code，不碰加入侧的 lastPairSessionCode', () => {
    const tpl = template(pairShow);
    // 正向：显示本次会话码，标签是「会话码」。
    expect(tpl).toContain('sync.pairing.session_code');
    expect(tpl).toContain("ui.t('sync.session_code')");
    // 负向：不得取加入侧才有的事件结果，也不得用「对端会话码」这个标签。
    expect(tpl).not.toContain('lastPairSessionCode');
    expect(tpl).not.toContain("ui.t('sync.peer_session_code')");
  });

  it('加入屏（PairScan）：用自己的 sync.lastPairSessionCode，不碰主机侧的 pairing.session_code', () => {
    const tpl = template(pairScan);
    // 正向：显示 join 结果里的会话码，标签是「对端会话码」。
    expect(tpl).toContain('sync.lastPairSessionCode');
    expect(tpl).toContain("ui.t('sync.peer_session_code')");
    // 负向：`pairing` 只在主机侧被赋值，加入屏读到它永远是 null（两屏必然一空一有）。
    expect(tpl).not.toContain('pairing.session_code');
    expect(tpl).not.toContain("ui.t('sync.session_code')");
  });

  it('两个核对块都由 hasSessionCode() 判空后才渲染', () => {
    for (const [name, src] of [
      ['PairShow', pairShow],
      ['PairScan', pairScan],
    ] as const) {
      const tpl = template(src);
      expect(
        tpl,
        `${name} 的会话码块必须包在 {#if hasSessionCode(...)} 里`,
      ).toMatch(/\{#if[\s\S]{0,80}hasSessionCode\(/);
      expect(tpl).toContain("ui.t('sync.session_code_hint')");
    }
  });

  it('两屏都不得再引用 sync.fingerprint（那是设备指纹，不是本次会话码）', () => {
    for (const [name, src] of [
      ['PairShow', pairShow],
      ['PairScan', pairScan],
    ] as const) {
      const tpl = template(src);
      expect(tpl, `${name} 不得再显示「设备指纹」`).not.toContain("ui.t('sync.fingerprint')");
    }
  });
});

describe('§5.4 / §5.6：设备行副信息顺序 = 指纹 → 配对时间 → 上次同步', () => {
  it('三个副行在模板中的出现顺序与规范一致', () => {
    const tpl = template(deviceList);
    const fp = tpl.indexOf("ui.t('sync.fingerprint')");
    const paired = tpl.indexOf("ui.t('sync.paired_at')");
    const last = tpl.indexOf("ui.t('sync.last_sync')");
    expect(fp).toBeGreaterThan(-1);
    expect(paired).toBeGreaterThan(-1);
    expect(last).toBeGreaterThan(-1);
    expect(fp).toBeLessThan(paired);
    expect(paired).toBeLessThan(last);
  });

  it('改名输入框复用全局 .pms-input，且长度上限常量与后端同值（64 字节）', () => {
    expect(template(deviceList)).toContain('class="pms-input"');
    expect(deviceList).toContain('const MAX_NAME_BYTES = 64');
    // 字节口径（TextEncoder），不是字符数（.length）口径。
    expect(deviceList).toContain('new TextEncoder().encode(text).length');
  });

  it('空名 / 超长时保存按钮被禁用，且超长时有 sync.rename_too_long 提示', () => {
    const tpl = template(deviceList);
    expect(tpl).toContain('disabled={!canSave}');
    expect(deviceList).toContain('trimmed.length > 0 && !tooLong && !saving');
    expect(tpl).toContain("ui.t('sync.rename_too_long')");
  });
});
