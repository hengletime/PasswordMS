<script lang="ts">
  // 密码生成器（PRD F-07）。生成在 Rust 侧完成（CSPRNG），前端只做参数与展示。
  // 设计规范 §5.4：结果用 --pms-mono 大字 + .pms-input-group 承载复制/刷新按钮；选项区两列网格。
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

<div class="gen">
  <div class="pms-input-group gen-output">
    <code>{password}</code>
    <button class="pms-icon-btn" type="button" aria-label={ui.t('gen.copy')} onclick={() => void copy()}>
      <Icon name="copy" size={16} />
    </button>
    <button class="pms-icon-btn" type="button" aria-label={ui.t('gen.generate')} onclick={() => void generate()}>
      <Icon name="refresh" size={16} />
    </button>
  </div>

  <label class="gen-length" for="gen-range">
    <span class="pms-label">{ui.t('gen.length')}: {opts.length}</span>
    <input
      id="gen-range"
      type="range"
      min={GENERATOR_MIN_LEN}
      max={GENERATOR_MAX_LEN}
      value={opts.length}
      oninput={(event: Event): void => {
        opts.length = Number((event.target as HTMLInputElement).value);
      }}
    />
  </label>

  <div class="gen-toggles">
    {#each toggles as toggle (toggle.key)}
      <label class="pms-check">
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
    <button class="pms-btn is-primary" type="button" onclick={() => onuse(password)}>
      {ui.t('gen.use')}
    </button>
  {/if}
</div>

<style>
  .gen {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .gen-output {
    padding-left: 0.8rem;
  }
  .gen-output code {
    flex: 1;
    min-width: 0;
    font-family: var(--pms-mono);
    font-size: 1.05rem;
    font-weight: 600;
    letter-spacing: 0.03em;
    word-break: break-all;
  }
  .gen-length {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .gen-length input {
    width: 100%;
    accent-color: var(--pms-accent);
  }
  .gen-toggles {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.5rem 0.85rem;
  }
</style>
