<script lang="ts">
  // 导入向导（PRD F-14）：Bitwarden JSON/CSV，含字段映射预览与跳过计数。
  // D-72：补「支持范围说明 + CSV 模板下载」。模板由后端写盘 —— Android 的 `save()` 返回
  // `content://…` URI，WebView 侧没有能直接写它的 API（同一个原因导致 D-71 导出失败）。
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { readTextFile } from '@tauri-apps/plugin-fs';
  import Icon from './common/Icon.svelte';
  import Spinner from './common/Spinner.svelte';
  import { toAppError } from '../lib/errors';
  import { api, type ImportFormat } from '../lib/ipc';
  import { mapCsvHeader } from '../lib/import';
  import { runSaveToDestination } from '../lib/save-dialog';
  import { ui } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import type { ImportPreviewDto } from '../types';

  interface Props {
    ondone: () => void;
  }
  const { ondone }: Props = $props();

  let sourcePath = $state('');
  let format = $state<ImportFormat>('json');
  let preview = $state<ImportPreviewDto | null>(null);
  let mapped = $state<string[]>([]);
  let customFields = $state<string[]>([]);
  let busy = $state(false);
  let resultText = $state('');

  function detectFormat(path: string): ImportFormat {
    return path.toLowerCase().endsWith('.csv') ? 'csv' : 'json';
  }

  async function readHeader(path: string): Promise<void> {
    if (detectFormat(path) !== 'csv') {
      mapped = [];
      customFields = [];
      return;
    }
    try {
      const text = await readTextFile(path);
      const firstLine = text.split(/\r?\n/, 1)[0] ?? '';
      const mapping = mapCsvHeader(firstLine.split(','));
      mapped = mapping.mapped;
      customFields = mapping.customFields;
    } catch {
      mapped = [];
      customFields = [];
    }
  }

  async function choose(): Promise<void> {
    const picked = await open({ multiple: false, directory: false });
    if (typeof picked !== 'string') {
      return;
    }
    sourcePath = picked;
    format = detectFormat(picked);
    preview = null;
    await readHeader(picked);
  }

  async function doPreview(): Promise<void> {
    if (sourcePath.length === 0) {
      // D-73：不能静默 return。旧实现把「预览」按钮 `disabled` 掉，界面上又不解释为什么 →
      // 主人卡在这里（真机反馈：「预览这里点不动，也没提示」）。现在按钮可点，
      // 未选文件时给出明确指令，用户至少知道下一步做什么。
      ui.toast('info', 'import.choose_first');
      return;
    }
    busy = true;
    try {
      preview = await api.importer.preview(sourcePath, format);
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      busy = false;
    }
  }

  async function apply(): Promise<void> {
    if (preview === null) {
      return;
    }
    busy = true;
    try {
      const result = await api.importer.apply(sourcePath, format, preview.preview_token);
      resultText = `${ui.t('import.done')}: ${ui.t('common.items_count', { n: result.imported_items })}`;
      ui.toast('success', 'import.done');
      await vault.load();
      ondone();
    } catch (err) {
      ui.toast('error', toAppError(err).messageKey);
    } finally {
      busy = false;
    }
  }

  /**
   * D-72：下载（另存）内置 CSV 导入模板。
   *
   * 取消/失败归一交给 `src/lib/save-dialog.ts`（D-71/D-72 三处同源缺陷的统一出口）：
   * 桌面取消 `null`、**Android 取消 reject**、Android 确认是 `content://…` URI ——
   * 三条平台差异都在那个模块里吃掉，这里只按结局分流文案。
   *
   * ⚠️ `failed` **必须** toast：旧实现是空 catch，把「写盘失败 / invalid args /
   * 命令未注册」一起吞掉，表现就是「点下载没反应、再点还是没反应」
   * （`docs/03-impl.md` §14.3 描述的形态），真机上极难定位。
   */
  async function downloadTemplate(): Promise<void> {
    const outcome = await runSaveToDestination({
      pickDestination: async (): Promise<string | null> =>
        save({
          defaultPath: 'passwordms-import-template.csv',
          filters: [{ name: 'CSV', extensions: ['csv'] }],
        }),
      write: (dest: string): Promise<void> => api.importer.saveTemplate(dest),
    });
    if (outcome.kind === 'saved') {
      ui.toast('success', 'import.template_saved');
    } else if (outcome.kind === 'failed') {
      ui.toast('error', outcome.messageKey);
    }
    // cancelled：用户主动放弃，什么都不做（不是错误，弹红会让人以为坏了）。
  }
</script>

<div class="imp">
  <div class="imp-row">
    <button class="pms-btn" type="button" onclick={() => void choose()}>
      <Icon name="file" size={15} />{ui.t('import.choose_file')}
    </button>
    {#if sourcePath}<code class="imp-path">{sourcePath}</code>{/if}
  </div>

  <div class="imp-row">
    <label class="pms-label" for="imp-format">{ui.t('import.format')}</label>
    <select id="imp-format" class="pms-input imp-format" bind:value={format}>
      <option value="json">{ui.t('import.format_json')}</option>
      <option value="csv">{ui.t('import.format_csv')}</option>
    </select>
    <button class="pms-btn is-ghost" type="button" onclick={() => void doPreview()} disabled={busy}>
      {ui.t('import.preview')}
    </button>
  </div>

  <!-- D-72：支持范围 + CSV 列清单**常驻**显示。放在选文件之前，是因为这两条决定
       「我这个文件能不能导」—— 等导完再看一堆跳过计数才知道，就晚了。
       `.pms-meta` = 中性说明（支持范围），`.pms-hint` = 细则（列清单与 type 取值）。 -->
  <p class="pms-meta">{ui.t('import.supported_hint')}</p>
  <p class="pms-hint">{ui.t('import.template_hint')}</p>
  <div class="imp-row">
    <button class="pms-btn is-ghost" type="button" onclick={() => void downloadTemplate()}>
      <Icon name="download" size={15} />{ui.t('import.download_template')}
    </button>
  </div>

  {#if format === 'csv' && (mapped.length > 0 || customFields.length > 0)}
    <div class="imp-mapping">
      <div class="imp-group">
        <span class="pms-label">{ui.t('import.mapping_known')}</span>
        <div class="imp-chips">
          {#each mapped as col (col)}<span class="pms-badge is-accent">{col}</span>{/each}
        </div>
      </div>
      <div class="imp-group">
        <span class="pms-label">{ui.t('import.mapping_custom')}</span>
        <div class="imp-chips">
          {#each customFields as col (col)}<span class="imp-badge-muted">{col}</span>{/each}
        </div>
      </div>
    </div>
  {/if}

  {#if busy}
    <Spinner label={ui.t('common.loading')} />
  {/if}

  {#if preview}
    <div class="imp-counts">
      <div><span class="pms-label">{ui.t('import.items')}</span><strong>{preview.item_count}</strong></div>
      <div><span class="pms-label">{ui.t('import.folders')}</span><strong>{preview.folder_count}</strong></div>
      <div><span class="pms-label">{ui.t('import.skipped')}</span><strong>{preview.skipped}</strong></div>
    </div>
    {#if preview.skipped > 0}
      <div class="imp-chips">
        {#each Object.entries(preview.skipped_by_type) as [label, count] (label)}
          <span class="imp-badge-muted">{label}: {count}</span>
        {/each}
      </div>
    {/if}
    <button class="pms-btn is-primary" type="button" onclick={() => void apply()} disabled={busy}>
      {ui.t('import.apply')}
    </button>
  {/if}

  {#if resultText}
    <p class="imp-result">{resultText}</p>
  {/if}
</div>

<style>
  .imp {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .imp-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .imp-path {
    font-family: var(--pms-mono);
    font-size: 0.76rem;
    color: var(--pms-muted);
    word-break: break-all;
  }
  .imp-format {
    width: auto;
    min-height: 34px;
    flex: 0 0 auto;
  }
  .imp-mapping {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    padding: 0.75rem;
    border: 1px solid var(--pms-border);
    border-radius: var(--pms-r);
    background: var(--pms-surface);
  }
  .imp-group {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .imp-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .imp-badge-muted {
    display: inline-flex;
    align-items: center;
    padding: 0.1rem 0.5rem;
    border-radius: var(--pms-r-pill);
    font-size: 0.74rem;
    font-weight: 600;
    background: var(--pms-bg-2);
    color: var(--pms-muted);
  }
  .imp-counts {
    display: flex;
    gap: 1.5rem;
  }
  .imp-counts div {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .imp-counts strong {
    font-size: 1.2rem;
    font-weight: 700;
  }
  .imp-result {
    color: var(--pms-success);
  }
</style>
