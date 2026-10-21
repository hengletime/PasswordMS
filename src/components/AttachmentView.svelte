<script lang="ts">
  // 父条目附件区（D-60 §3.14.6）：附件是父条目（登录/安全笔记）内嵌 `attachments` 数组的成员，
  // 不再是独立条目。区头「添加附件」+ 附件行（文件名 / 类型 / 大小 + 导出 + 移除）。
  //
  // - 添加：系统文件对话框（单选）→ `add_attachment` IPC → 返回父条目（rev+1）→ 刷新；
  //   流程逻辑复用 src/lib/attachment-import.ts 纯逻辑层（守卫/取消/成功/失败四分支有单测守门）。
  // - 导出：明文风险二次确认（attach.export_confirm_*）+ `ERR_BLOB_MISSING` 待同步态禁用（D-53 语义保留）。
  // - 移除：二次确认（attach.remove_confirm_*）→ `remove_attachment` IPC → blob 交孤儿 GC。
  // - blob 就绪态按 attachment_id 逐个查询（D-60：不再是 item_id）。
  import { open, save } from '@tauri-apps/plugin-dialog';
  import Icon from './common/Icon.svelte';
  import { formatBytes } from '../lib/constants';
  import { toAppError } from '../lib/errors';
  import { runAttachmentImport } from '../lib/attachment-import';
  import { api } from '../lib/ipc';
  import { ui } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import type { AttachmentMetaDto, ItemDto } from '../types';

  interface Props {
    item: ItemDto;
  }
  const { item }: Props = $props();

  // 每个附件的 blob 就绪态（undefined = 查询中/未知，false = 待同步，true = 可导出）。
  let ready = $state<Record<string, boolean>>({});

  $effect(() => {
    // 依赖 item（父条目刷新即重查）；不读 ready，避免写读回路。
    const ids = item.attachments.map((a): string => a.attachment_id);
    ready = {};
    for (const id of ids) {
      void api.keydocs
        .attachmentBlobReady(id)
        .then((isReady): void => {
          ready[id] = isReady;
        })
        .catch((): void => {
          // 后端不可达时保守按「待同步」处理（导出禁用，与 D-53 语义一致）。
          ready[id] = false;
        });
    }
  });

  /** 添加附件：守卫/取消/成功/失败四分支在纯逻辑层，成功后刷新父条目。 */
  async function addAttachment(): Promise<void> {
    const result = await runAttachmentImport(
      {
        pickFile: async (): Promise<string | null> => {
          try {
            const picked = await open({ multiple: false, directory: false });
            return typeof picked === 'string' ? picked : null;
          } catch {
            // D-66：Android 上 dialog 插件在用户取消时是 reject("File picker cancelled")，
            // 桌面端返回 null。统一按「用户取消」处理（不弹错误 toast）。
            return null;
          }
        },
        addAttachment: (itemId: string, sourcePath: string, title: string): Promise<ItemDto> =>
          api.keydocs.addAttachment(itemId, sourcePath, title),
        toast: (kind, messageKey): void => {
          ui.toast(kind, messageKey);
        },
      },
      item.item_id,
    );
    if (result.item !== null) {
      await vault.load(); // 父条目 rev+1，列表与详情一并刷新
    }
  }

  /** 导出单个附件（明文风险二次确认；待同步态禁用）。 */
  async function exportAttachment(att: AttachmentMetaDto): Promise<void> {
    const confirmed = await ui.confirm('attach.export_confirm_title', 'attach.export_confirm_body');
    if (!confirmed) {
      return;
    }
    try {
      const dest = await save({ defaultPath: att.filename || 'attachment.bin' });
      if (dest === null) {
        return;
      }
      await api.keydocs.exportAttachment(item.item_id, att.attachment_id, dest);
      ui.toast('success', 'common.ok');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }

  /** 移除单个附件（二次确认；blob 交孤儿 GC）。 */
  async function removeAttachment(att: AttachmentMetaDto): Promise<void> {
    const confirmed = await ui.confirm('attach.remove_confirm_title', 'attach.remove_confirm_body', {
      name: att.filename,
    });
    if (!confirmed) {
      return;
    }
    try {
      await api.keydocs.removeAttachment(item.item_id, att.attachment_id);
      await vault.load();
      ui.toast('success', 'common.ok');
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    }
  }
</script>

<div class="attach">
  <div class="attach-head">
    <span class="attach-title">
      <Icon name="paperclip" size={14} />
      <span class="pms-label">{ui.t('attach.section_title')}</span>
    </span>
    <button class="pms-btn is-ghost is-sm" type="button" onclick={() => void addAttachment()}>
      <Icon name="plus" size={14} />{ui.t('attach.add')}
    </button>
  </div>

  {#each item.attachments as att (att.attachment_id)}
    <div class="attach-row">
      <span class="attach-icon"><Icon name="paperclip" size={15} /></span>
      <span class="attach-meta">
        <span class="attach-name">{att.filename}</span>
        <span class="pms-meta">{att.mime_type || ui.t('common.unknown')} · {formatBytes(att.size_bytes)}</span>
        {#if ready[att.attachment_id] === false}
          <span class="pms-meta attach-pending">{ui.t('attach.pending_sync')}</span>
        {/if}
      </span>
      <span class="attach-actions">
        <button
          class="pms-icon-btn"
          type="button"
          title={ui.t('attach.export')}
          aria-label={ui.t('attach.export')}
          disabled={ready[att.attachment_id] === false}
          onclick={() => void exportAttachment(att)}
        >
          <Icon name="download" size={15} />
        </button>
        <button
          class="pms-icon-btn"
          type="button"
          title={ui.t('attach.remove')}
          aria-label={ui.t('attach.remove')}
          onclick={() => void removeAttachment(att)}
        >
          <Icon name="trash" size={15} />
        </button>
      </span>
    </div>
  {/each}
</div>

<style>
  .attach {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
  }
  .attach-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
  }
  .attach-title {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    color: var(--pms-muted);
  }
  .attach-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    min-height: 40px;
    padding: 0.3rem 0.4rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r-sm);
    background: var(--pms-bg-2);
  }
  .attach-icon {
    display: flex;
    color: var(--pms-muted);
    flex-shrink: 0;
  }
  .attach-meta {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
    flex: 1;
  }
  .attach-name {
    font-size: 0.85rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .attach-pending {
    color: var(--pms-warning);
  }
  .attach-actions {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    flex-shrink: 0;
  }
</style>
