<script lang="ts">
  // 导入向导（PRD F-14）：Bitwarden JSON/CSV，含字段映射预览与跳过计数。
  import { open } from '@tauri-apps/plugin-dialog';
  import { readTextFile } from '@tauri-apps/plugin-fs';
  import Icon from './common/Icon.svelte';
  import Spinner from './common/Spinner.svelte';
  import { toAppError } from '../lib/errors';
  import { api, type ImportFormat } from '../lib/ipc';
  import { mapCsvHeader } from '../lib/import';
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
    <button class="pms-btn is-ghost" type="button" onclick={() => void doPreview()} disabled={sourcePath.length === 0 || busy}>
      {ui.t('import.preview')}
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
