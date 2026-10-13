<script lang="ts">
  // 主密码强度指示（仅提示，不强制混合字符类型 —— U6）。
  import { MIN_MASTER_PASSWORD_LEN } from '../../lib/constants';
  import { ui } from '../../lib/stores/ui.svelte';

  interface Props {
    password: string;
  }
  const { password }: Props = $props();

  /** 强度分级：0（空）~ 4（强）。 */
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

  let score = $derived(scoreOf(password));
  let tooShort = $derived(password.length > 0 && password.length < MIN_MASTER_PASSWORD_LEN);
</script>

<div class="meter" data-score={score}>
  <div class="bars">
    {#each [1, 2, 3, 4] as level (level)}
      <span class="bar" class:filled={score >= level}></span>
    {/each}
  </div>
  <span class="hint">
    {#if tooShort}
      {ui.t('create.too_short', { n: MIN_MASTER_PASSWORD_LEN })}
    {:else if score > 0 && score < 4}
      {ui.t('create.weak_hint')}
    {/if}
  </span>
</div>

<style>
  .meter {
    margin-top: 0.4rem;
  }
  .bars {
    display: flex;
    gap: 0.25rem;
  }
  .bar {
    height: 4px;
    flex: 1;
    border-radius: 2px;
    background: var(--pms-border);
  }
  .meter[data-score='1'] .bar.filled {
    background: var(--pms-danger);
  }
  .meter[data-score='2'] .bar.filled {
    background: var(--pms-warning);
  }
  .meter[data-score='3'] .bar.filled {
    background: #3b82f6;
  }
  .meter[data-score='4'] .bar.filled {
    background: var(--pms-success);
  }
  .hint {
    display: block;
    min-height: 1rem;
    margin-top: 0.25rem;
    font-size: 0.78rem;
    color: var(--pms-muted);
  }
</style>
