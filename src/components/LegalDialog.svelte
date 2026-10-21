<script lang="ts">
  // 法律条款弹窗（D-63）：免责声明 / 用户协议通用，复用通用 Modal（设计规范 §5.4）。
  // 语言跟随 `ui.locale`（zh-CN → 中文文本；en-US → 英文文本），文本源见 src/lib/legal-content.ts。
  // props：`open` 可绑定（父组件 bind:open），`kind` 选择文档；底部「我知道了」关闭。
  import Modal from './common/Modal.svelte';
  import { getDisclaimer, getTerms, type LegalDoc, type LegalLang } from '../lib/legal-content';
  import { ui } from '../lib/stores/ui.svelte';

  interface Props {
    open?: boolean;
    kind: 'disclaimer' | 'terms';
  }
  let { open = $bindable(false), kind }: Props = $props();

  const lang = $derived<LegalLang>(ui.locale === 'en-US' ? 'en' : 'zh');
  const doc = $derived<LegalDoc>(kind === 'terms' ? getTerms(lang) : getDisclaimer(lang));
  const titleKey = $derived(kind === 'terms' ? 'legal.terms_title' : 'legal.disclaimer_title');

  function close(): void {
    open = false;
  }
</script>

<Modal {open} {titleKey} onclose={close}>
  <div class="legal">
    <p class="pms-meta legal-updated">{ui.t('legal.updated')}: {doc.updated}</p>
    {#each doc.sections as section, index (section.heading)}
      <h4 class="legal-heading">{index + 1}. {section.heading}</h4>
      {#each section.paragraphs as paragraph, pIndex (pIndex)}
        <p class="legal-para">{paragraph}</p>
      {/each}
    {/each}
  </div>
  {#snippet footer()}
    <button class="pms-btn is-primary" type="button" onclick={close}>{ui.t('legal.understood')}</button>
  {/snippet}
</Modal>

<style>
  .legal {
    max-height: min(60vh, 420px);
    overflow-y: auto;
    /* 右侧留出滚动条槽位，长文滚动时不抖动。 */
    padding-right: 0.4rem;
  }
  .legal-updated {
    margin: 0 0 0.75rem;
  }
  .legal-heading {
    margin: 1rem 0 0.35rem;
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--pms-fg);
  }
  .legal-heading:first-of-type {
    margin-top: 0.5rem;
  }
  .legal-para {
    margin: 0 0 0.5rem;
    font-size: 0.85rem;
    line-height: 1.65;
    color: var(--pms-muted);
  }
</style>
