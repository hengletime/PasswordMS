<script lang="ts">
  // 加密附件视图（PRD F-12）：展示元信息 + 导出（明文风险由二次确认覆盖）。
  import { save } from '@tauri-apps/plugin-dialog';
  import Icon from './common/Icon.svelte';
  import { formatBytes, MAX_ATTACHMENT_BYTES, MB } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { api } from '../lib/ipc';
  import { ui } from '../lib/stores/ui.svelte';
  import type { ItemDto } from '../types';

  interface Props {
    item: ItemDto;
  }
  const { item }: Props = $props();

  let info = $derived(
    item.kind.type === 'attachment'
      ? item.kind
      : { original_filename: '', mime_type: '', size_bytes: 0, blob_size_bytes: 0, title: '', blob_ref: '' },
  );

  async function exportFile(): Promise<void> {
    const confirmed = await ui.confirm('attach.export_confirm_title', 'attach.export_confirm_body');
    if (!confirmed) {
      return;
    }
    try {
      const dest = await save({ defaultPath: info.original_filename || 'attachment.bin' });
      if (dest === null) {
        return;
      }
      await api.keydocs.exportAttachment(item.item_id, dest);
      ui.toast('success', 'common.ok');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }
</script>

<div class="attachment">
  <div class="rows">
    <div class="row">
      <span class="label">{ui.t('attach.filename')}</span>
      <span class="value">{info.original_filename}</span>
    </div>
    <div class="row">
      <span class="label">{ui.t('attach.mime')}</span>
      <span class="value">{info.mime_type || ui.t('common.unknown')}</span>
    </div>
    <div class="row">
      <span class="label">{ui.t('attach.size')}</span>
      <span class="value">{formatBytes(info.size_bytes)}</span>
    </div>
  </div>

  {#if info.size_bytes > MAX_ATTACHMENT_BYTES}
    <p class="warn">{ui.t('attach.too_large', { n: MAX_ATTACHMENT_BYTES / MB })}</p>
  {/if}

  <button class="btn ghost" type="button" onclick={() => void exportFile()}>
    <Icon name="download" size={15} />{ui.t('attach.export')}
  </button>
</div>

<style>
  .attachment {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    font-size: 0.88rem;
    border-bottom: 1px solid var(--pms-border);
    padding-bottom: 0.4rem;
  }
  .label {
    color: var(--pms-muted);
  }
  .value {
    word-break: break-all;
    text-align: right;
  }
  .warn {
    color: var(--pms-danger);
    font-size: 0.82rem;
  }
  .btn {
    align-self: flex-start;
  }
</style>
