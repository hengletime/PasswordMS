<script lang="ts">
  // 主题切换按钮（共用）：在「显式浅色」与「显式深色」之间切换。
  // 图标反映**当前实际渲染**的主题：`system` 模式跟随系统偏好，
  // 因此判断依据用 `theme.effective` 而非仅 `theme.mode`，避免系统为深色时图标显示错误。
  // 样式复用共享类 `.pms-icon-btn`（见 global.css §1 命名规定，不再定义同名局部类）。
  import Icon from './Icon.svelte';
  import { theme } from '../../lib/stores/theme.svelte';
  import { ui } from '../../lib/stores/ui.svelte';

  // 当前实际生效的是否为深色。
  let isDark = $derived(theme.effective === 'dark');

  // 点击：当前是浅色 → 显式深色；否则 → 显式浅色。（'system' 只在设置页选，不做三态循环。）
  function toggle(): void {
    theme.setMode(theme.effective === 'dark' ? 'light' : 'dark');
  }
</script>

<button
  class="pms-icon-btn"
  type="button"
  aria-label={isDark ? ui.t('theme.to_light') : ui.t('theme.to_dark')}
  title={isDark ? ui.t('theme.to_light') : ui.t('theme.to_dark')}
  onclick={toggle}
>
  <Icon name={isDark ? 'sun' : 'moon'} />
</button>
