<script lang="ts">
  // 密码生成器（PRD F-07）。生成在 Rust 侧完成（CSPRNG），前端只做参数与展示。
  // 设计规范 §5.4：结果用 --pms-mono 大字 + .pms-input-group 承载复制/重新生成按钮；选项区两列网格。
  //
  // 本轮升级（客户："可以自定义生成位数、包含哪些字符等"）：
  //   ① 位数改为「滑块 + 数字输入框」双向联动，越界钳制到 [MIN, MAX]，非法输入绝不写 NaN；
  //   ② 4 类字符至少选一类：全不选时禁用「生成」「使用此密码」并提示 gen.need_one，
  //      且 **先判空再请求** —— 0 字符集不向后端发请求（否则后端报错并弹 toast）；
  //   ③ 复用 common/StrengthMeter 展示当前结果强度，让"生成"更有价值感。
  import Icon from './common/Icon.svelte';
  import StrengthMeter from './common/StrengthMeter.svelte';
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
  // 数字输入框的"正在编辑文本"：与 opts.length 解耦，避免中途输入（如先敲 "1"）被立刻钳制回写成 "8"。
  // svelte-ignore state_referenced_locally
  let lengthText = $state<string>(String(opts.length));

  /** 4 类字符至少选一类（exclude_ambiguous 不计入）。 */
  let hasCharset = $derived(opts.uppercase || opts.lowercase || opts.digits || opts.symbols);

  /** 把任意数值钳制到合法区间 [GENERATOR_MIN_LEN, GENERATOR_MAX_LEN] 并取整。 */
  function clampLength(value: number): number {
    return Math.min(GENERATOR_MAX_LEN, Math.max(GENERATOR_MIN_LEN, Math.round(value)));
  }

  /** 滑块输入：值恒为合法数字，直接采用并回写文本框（实现"滑块 → 数字框"联动）。 */
  function onRangeInput(event: Event): void {
    const n = Number((event.target as HTMLInputElement).value);
    if (!Number.isFinite(n)) {
      return;
    }
    opts.length = clampLength(n);
    lengthText = String(opts.length);
  }

  /**
   * 数字框输入：先原样保留文本（允许用户连续输入多位），仅当数值落在合法区间内才同步给参数/滑块。
   * 空值或非法值一律不改 opts —— 从源头杜绝把参数写成 NaN。越界值留到 commitLength 统一钳制。
   */
  function onLengthInput(event: Event): void {
    const raw = (event.target as HTMLInputElement).value;
    lengthText = raw;
    const n = Number(raw);
    if (raw.trim() === '' || !Number.isFinite(n) || n < GENERATOR_MIN_LEN || n > GENERATOR_MAX_LEN) {
      return;
    }
    opts.length = Math.round(n);
  }

  /** 数字框提交（失焦 / 回车）：越界值钳制到范围内并回写；非法输入还原为当前值。 */
  function commitLength(): void {
    const n = Number(lengthText);
    if (lengthText.trim() === '' || !Number.isFinite(n)) {
      lengthText = String(opts.length);
      return;
    }
    opts.length = clampLength(n);
    lengthText = String(opts.length);
  }

  async function generate(): Promise<void> {
    // 双保险：任何路径下 0 字符集都不请求后端。
    if (!hasCharset) {
      return;
    }
    try {
      const result = await api.tools.generatePassword($state.snapshot(opts));
      password = result.password;
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  $effect((): void => {
    // 参数变化即重新生成（首帧也会执行一次）。
    // ⚠️ 4 类字符全不选时字符集为空：后端会返回错误并弹 toast —— 因此**先判空、直接返回**，不发请求。
    if (!hasCharset) {
      return;
    }
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
    <button
      class="pms-icon-btn"
      type="button"
      aria-label={ui.t('gen.generate')}
      disabled={!hasCharset}
      onclick={() => void generate()}
    >
      <Icon name="refresh" size={16} />
    </button>
  </div>

  <StrengthMeter password={password} />

  <div class="gen-length">
    <label class="pms-label" for="gen-range">{ui.t('gen.length')}</label>
    <div class="gen-length-row">
      <input
        id="gen-range"
        class="gen-slider"
        type="range"
        min={GENERATOR_MIN_LEN}
        max={GENERATOR_MAX_LEN}
        value={opts.length}
        oninput={onRangeInput}
      />
      <input
        class="pms-input gen-number"
        type="number"
        min={GENERATOR_MIN_LEN}
        max={GENERATOR_MAX_LEN}
        step="1"
        aria-label={ui.t('gen.length')}
        value={lengthText}
        oninput={onLengthInput}
        onchange={commitLength}
        onblur={commitLength}
      />
    </div>
  </div>

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

  {#if !hasCharset}
    <p class="pms-hint is-danger">{ui.t('gen.need_one')}</p>
  {/if}

  {#if onuse}
    <button class="pms-btn is-primary" type="button" disabled={!hasCharset} onclick={() => onuse(password)}>
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
  .gen-length-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }
  .gen-slider {
    flex: 1 1 auto;
    min-width: 0;
    accent-color: var(--pms-accent);
  }
  .gen-number {
    flex: 0 0 5rem;
    width: 5rem;
    text-align: center;
  }
  .gen-toggles {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.5rem 0.85rem;
  }
</style>
