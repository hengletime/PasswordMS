<script lang="ts">
  // 加密附件视图（PRD F-12）：展示元信息 + 导出（明文风险由二次确认覆盖）。
  // D-53：blob 未同步到本机时呈现「附件内容待同步」占位态，导出禁用（导出会报 ERR_BLOB_MISSING）。
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

  // D-53：blob 是否已就绪（仅 attachment 条目查询；后端不可达时保守按未同步处理）。
  let blobReady = $state<boolean | null>(null);
  $effect(() => {
    if (item.kind.type !== 'attachment') {
      blobReady = true;
      return;
    }
    blobReady = null;
    void api.keydocs
      .attachmentBlobReady(item.item_id)
      .then((ready): void => {
        blobReady = ready;
      })
      .catch((): void => {
        blobReady = false;
      });
  });

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

<div class="attach">
  <div class="attach-rows">
    <div class="attach-row">
      <span class="pms-label">{ui.t('attach.filename')}</span>
      <span class="attach-value">{info.original_filename}</span>
    </div>
    <div class="attach-row">
      <span class="pms-label">{ui.t('attach.mime')}</span>
      <span class="attach-value">{info.mime_type || ui.t('common.unknown')}</span>
    </div>
    <div class="attach-row">
      <span class="pms-label">{ui.t('attach.size')}</span>
      <span class="attach-value">{formatBytes(info.size_bytes)}</span>
    </div>
  </div>

  {#if info.size_bytes > MAX_ATTACHMENT_BYTES}
    <p class="pms-alert is-danger">
      <Icon name="alert" size={16} />
      <span>{ui.t('attach.too_large', { n: MAX_ATTACHMENT_BYTES / MB })}</span>
    </p>
  {/if}

  {#if blobReady === false}
    <p class="pms-alert is-warning">
      <Icon name="alert" size={16} />
      <span>{ui.t('attach.pending_sync')}</span>
    </p>
  {/if}

  <button
    class="pms-btn is-ghost attach-export"
    type="button"
    disabled={blobReady === false}
    onclick={() => void exportFile()}
  >
    <Icon name="download" size={15} />{ui.t('attach.export')}
  </button>
</div>

<style>
  .attach {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .attach-rows {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .attach-row {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    font-size: 0.88rem;
    border-bottom: 1px solid var(--pms-border);
    padding-bottom: 0.45rem;
  }
  .attach-value {
    word-break: break-all;
    text-align: right;
  }
  .attach-export {
    align-self: flex-start;
  }
</style>
