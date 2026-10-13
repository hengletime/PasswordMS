<script lang="ts">
  // 密码生成器（PRD F-07）。生成在 Rust 侧完成（CSPRNG），前端只做参数与展示。
  import Icon from './common/Icon.svelte';
  import { copySecretWithAutoClear } from '../lib/clipboard';
  import { GENERATOR_MAX_LEN, GENERATOR_MIN_LEN } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { ui } from '../lib/stores/ui.svelte';
  import type { GeneratorOptions } from '../types';

  interface Props {
    initial?: GeneratorOptions;
    onuse?: (password: string) => void;
  }
  const { initial, onuse }: Props = $props();

  const DEFAULTS: GeneratorOptions = {
    length: 16,
    uppercase: true,
    lowercase: true,
    digits: true,
    symbols: true,
    exclude_ambiguous: true,
  };

  // 仅在打开时快照一次初始参数（有意为之：后续父组件变化不覆盖用户已调参数）。
  // svelte-ignore state_referenced_locally
  let opts = $state<GeneratorOptions>({ ...(initial ?? DEFAULTS) });
  let password = $state('');

  async function generate(): Promise<void> {
    try {
      const result = await api.tools.generatePassword($state.snapshot(opts));
      password = result.password;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  $effect((): void => {
    // 参数变化即重新生成（首帧也会执行一次）。
    void generate();
  });

  async function copy(): Promise<void> {
    if (password.length === 0) {
      return;
    }
    const ttl = 30;
    await copySecretWithAutoClear(password, ttl, (remaining: number): void => {
      if (remaining === ttl) {
        ui.toast('success', 'common.copied_countdown', { n: ttl });
      }
    });
  }

  const toggles: { key: keyof GeneratorOptions; labelKey: string }[] = [
    { key: 'uppercase', labelKey: 'gen.uppercase' },
    { key: 'lowercase', labelKey: 'gen.lowercase' },
    { key: 'digits', labelKey: 'gen.digits' },
    { key: 'symbols', labelKey: 'gen.symbols' },
    { key: 'exclude_ambiguous', labelKey: 'gen.exclude_ambiguous' },
  ];
</script>

<div class="generator">
  <div class="output">
    <code>{password}</code>
    <button class="icon-btn" type="button" aria-label={ui.t('gen.copy')} onclick={() => void copy()}>
      <Icon name="copy" size={16} />
    </button>
    <button class="icon-btn" type="button" aria-label={ui.t('gen.generate')} onclick={() => void generate()}>
      <Icon name="refresh" size={16} />
    </button>
  </div>

  <label class="length">
    <span>{ui.t('gen.length')}: {opts.length}</span>
    <input
      type="range"
      min={GENERATOR_MIN_LEN}
      max={GENERATOR_MAX_LEN}
      value={opts.length}
      oninput={(event: Event): void => {
        opts.length = Number((event.target as HTMLInputElement).value);
      }}
    />
  </label>

  <div class="toggles">
    {#each toggles as toggle (toggle.key)}
      <label class="check">
        <input
          type="checkbox"
          checked={opts[toggle.key] as boolean}
          onchange={(event: Event): void => {
            (opts[toggle.key] as boolean) = (event.target as HTMLInputElement).checked;
          }}
        />
        <span>{ui.t(toggle.labelKey)}</span>
      </label>
    {/each}
  </div>

  {#if onuse}
    <button class="btn primary" type="button" onclick={() => onuse(password)}>
      {ui.t('gen.use')}
    </button>
  {/if}
</div>

<style>
  .generator {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .output {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    background: var(--pms-bg);
    padding: 0.5rem 0.6rem;
  }
  .output code {
    flex: 1;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.95rem;
    word-break: break-all;
  }
  .icon-btn {
    background: transparent;
    border: none;
    color: var(--pms-muted);
    cursor: pointer;
    padding: 0.2rem;
    line-height: 0;
  }
  .length {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    font-size: 0.82rem;
    color: var(--pms-muted);
  }
  .length input {
    width: 100%;
  }
  .toggles {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.4rem;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
  }
</style>
