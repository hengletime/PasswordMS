<script lang="ts">
  // 安全笔记只读视图（PRD F-13）。
  import Icon from './common/Icon.svelte';
  import { copySecretWithAutoClear } from '../lib/clipboard';
  import { ui } from '../lib/stores/ui.svelte';
  import type { ItemDto } from '../types';

  interface Props {
    item: ItemDto;
  }
  const { item }: Props = $props();

  let body = $derived(item.kind.type === 'secure_note' ? item.kind.body : '');

  async function copyBody(): Promise<void> {
    if (body.length === 0) {
      return;
    }
    await copySecretWithAutoClear(body);
    ui.toast('success', 'common.copied');
  }
</script>

<div class="note">
  <article>{body}</article>
  <button class="pms-btn is-ghost note-copy" type="button" onclick={() => void copyBody()}>
    <Icon name="copy" size={15} />{ui.t('common.copy')}
  </button>
</div>

<style>
  .note {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  article {
    white-space: pre-wrap;
    word-break: break-word;
    background: var(--pms-bg-2);
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    padding: 0.85rem;
    font-size: 0.9rem;
    line-height: 1.6;
    min-height: 120px;
  }
  .note-copy {
    align-self: flex-start;
  }
</style>
