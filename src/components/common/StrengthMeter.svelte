<script lang="ts">
  // 主密码强度指示（仅提示，不强制混合字符类型 —— U6）。
  // 设计规范 §5.4：4 段彩色（弱=红 / 一般=橙 / 强=青绿 / 很强=青绿加粗）+ 右侧文字标签。
  //
  // 修正记录（2026-10-14）：原实现用「未输入时整条压暗（opacity:.5）」+ 空 label 仍占
  //   min-width:2.6rem，实测暗色下条带 #18222E 叠在卡片 #121A24 上、RGB 差仅 24，肉眼
  //   几乎不可见；右侧还凭空多出 47px 留白。现改为：轨道用中性可见色（--pms-track 已提亮），
  //   空态直接隐藏 label 让条带占满整行。
  import { ui } from '../../lib/stores/ui.svelte';

  interface Props {
    password: string;
  }
  const { password }: Props = $props();

  /** 强度分级：0（空）~ 4（很强）。 */
  function scoreOf(value: string): number {
    if (value.length === 0) {
      return 0;
    }
    let classes = 0;
    if (/[a-z]/.test(value)) classes += 1;
    if (/[A-Z]/.test(value)) classes += 1;
    if (/[0-9]/.test(value)) classes += 1;
    if (/[^A-Za-z0-9]/.test(value)) classes += 1;
    let base = classes;
    if (value.length >= 12) base += 1;
    if (value.length >= 20) base += 1;
    return Math.max(1, Math.min(4, base));
  }

  const LABEL_KEYS: Record<number, string> = {
    1: 'meter.weak',
    2: 'meter.fair',
    3: 'meter.strong',
    4: 'meter.very_strong',
  };

  let score = $derived(scoreOf(password));
  let label = $derived(score === 0 ? '' : ui.t(LABEL_KEYS[score]));
</script>

<div class="meter" data-score={score}>
  <div class="bars">
    {#each [1, 2, 3, 4] as level (level)}
      <span class="bar" class:filled={score >= level}></span>
    {/each}
  </div>
  <span class="label" class:is-strong={score === 4}>{label}</span>
</div>

<style>
  .meter {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin-top: 0.45rem;
  }
  .bars {
    display: flex;
    flex: 1;
    gap: 0.25rem;
  }
  .bar {
    height: 5px;
    flex: 1;
    border-radius: var(--pms-r-pill);
    background: var(--pms-track);
    transition: background var(--pms-dur) var(--pms-ease);
  }
  /* 未输入：不要压暗到看不见，也不要用空 label 占位 */
  .meter[data-score='0'] .label {
    display: none;
  }
  .meter[data-score='1'] .bar.filled { background: var(--pms-danger); }
  .meter[data-score='2'] .bar.filled { background: var(--pms-warning); }
  .meter[data-score='3'] .bar.filled { background: var(--pms-success); }
  .meter[data-score='4'] .bar.filled { background: var(--pms-success); }

  .label {
    min-width: 2.6rem;
    text-align: right;
    font-size: 0.74rem;
    font-weight: 600;
    color: var(--pms-muted);
    white-space: nowrap;
  }
  .meter[data-score='1'] .label { color: var(--pms-danger); }
  .meter[data-score='2'] .label { color: var(--pms-warning); }
  .meter[data-score='3'] .label { color: var(--pms-success); }
  /* 很强 = 青绿加粗 */
  .meter[data-score='4'] .label,
  .label.is-strong { color: var(--pms-success); font-weight: 700; }
</style>
