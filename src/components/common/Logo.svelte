<script lang="ts" module>
  // 每个实例一个独立渐变 id：SVG 的 url(#id) 是全文档解析的，
  // 多实例共用同一 id 虽然通常也能画出来，但一旦挂载顺序变化就可能引用到
  // 别的实例的 defs —— 干脆一人一份，彻底避开。
  let seq = 0;
</script>

<script lang="ts">
  // 品牌标：与 src-tauri/icons 的应用图标**同一份矢量**（2026-10-14 客户选定 A · 盾与钥匙孔）。
  // ⚠️ 改这里的形状 = 改应用图标，两处必须同步：
  //   应用图标源图 = %TEMP%\pms-logo-source.html（同一份 path），用无头 Edge 截成
  //   1024×1024 透明 PNG 后跑 `npx tauri icon <png>`，会同时产出
  //   Windows icons/、安卓 mipmap、iOS AppIcon（本仓库只用前两者）。
  interface Props {
    size?: number;
    /** plate = 带渐变圆角方牌（与应用图标完全一致，默认）；mark = 仅标识本身，颜色跟随 currentColor */
    variant?: 'plate' | 'mark';
  }
  const { size = 44, variant = 'plate' }: Props = $props();

  const gid = `pms-logo-g${++seq}`;
  // 用 $derived 而不是初始化时求值：variant 是响应式 prop，直接在顶层读取
  // 只会捕获初始值（svelte-check 会报 state_referenced_locally）。
  const ink = $derived(variant === 'plate' ? '#fff' : 'currentColor');
  // 盾 + 钥匙孔（48 视箱，方牌 44×44 / 圆角 13，与图标源图逐字相同）
  const SHIELD =
    'M24 12c-2.6 0-6.2 2.2-9 2.6v7.7c0 5.4 3.9 9.4 9 11.4 5.1-2 9-6 9-11.4v-7.7c-2.8-.4-6.4-2.6-9-2.6z';
  const SLOT = 'M22.6 22.2h2.8l.8 4.6h-4.4z';
</script>

<svg width={size} height={size} viewBox="0 0 48 48" role="img" aria-label="PasswordMS">
  <defs>
    <linearGradient id={gid} x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#5EEAD4" />
      <stop offset=".5" stop-color="#2DD4BF" />
      <stop offset="1" stop-color="#0F8B80" />
    </linearGradient>
  </defs>
  {#if variant === 'plate'}
    <rect x="2" y="2" width="44" height="44" rx="13" fill={`url(#${gid})`} />
  {/if}
  <path d={SHIELD} fill="none" stroke={ink} stroke-width="2.4" stroke-linejoin="round" />
  <circle cx="24" cy="20.8" r="2.6" fill={ink} />
  <path d={SLOT} fill={ink} />
</svg>
