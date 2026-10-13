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

<div class="wizard">
  <div class="row">
    <button class="btn" type="button" onclick={() => void choose()}>
      <Icon name="file" size={15} />{ui.t('import.choose_file')}
    </button>
    {#if sourcePath}<code class="path">{sourcePath}</code>{/if}
  </div>

  <div class="row">
    <span class="label">{ui.t('import.format')}</span>
    <select bind:value={format}>
      <option value="json">{ui.t('import.format_json')}</option>
      <option value="csv">{ui.t('import.format_csv')}</option>
    </select>
    <button class="btn ghost" type="button" onclick={() => void doPreview()} disabled={sourcePath.length === 0 || busy}>
      {ui.t('import.preview')}
    </button>
  </div>

  {#if format === 'csv' && (mapped.length > 0 || customFields.length > 0)}
    <div class="mapping">
      <div class="group">
        <span class="label">{ui.t('import.mapping_known')}</span>
        <div class="chips">
          {#each mapped as col (col)}<span class="chip">{col}</span>{/each}
        </div>
      </div>
      <div class="group">
        <span class="label">{ui.t('import.mapping_custom')}</span>
        <div class="chips">
          {#each customFields as col (col)}<span class="chip muted">{col}</span>{/each}
        </div>
      </div>
    </div>
  {/if}

  {#if busy}
    <Spinner label={ui.t('common.loading')} />
  {/if}

  {#if preview}
    <div class="counts">
      <div><span>{ui.t('import.items')}</span><strong>{preview.item_count}</strong></div>
      <div><span>{ui.t('import.folders')}</span><strong>{preview.folder_count}</strong></div>
      <div><span>{ui.t('import.skipped')}</span><strong>{preview.skipped}</strong></div>
    </div>
    {#if preview.skipped > 0}
      <div class="skips">
        {#each Object.entries(preview.skipped_by_type) as [label, count] (label)}
          <span class="chip muted">{label}: {count}</span>
        {/each}
      </div>
    {/if}
    <button class="btn primary" type="button" onclick={() => void apply()} disabled={busy}>
      {ui.t('import.apply')}
    </button>
  {/if}

  {#if resultText}
    <p class="result">{resultText}</p>
  {/if}
</div>

<style>
  .wizard {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .label {
    font-size: 0.78rem;
    color: var(--pms-muted);
  }
  .path {
    font-family: var(--pms-mono);
    font-size: 0.76rem;
    color: var(--pms-muted);
    word-break: break-all;
  }
  select {
    border: 1px solid var(--pms-border);
    border-radius: 8px;
    background: var(--pms-bg);
    color: var(--pms-fg);
    padding: 0.4rem 0.55rem;
    font-size: 0.85rem;
  }
  .mapping {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    border: 1px solid var(--pms-border);
    border-radius: 10px;
    padding: 0.7rem;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .chip {
    font-size: 0.74rem;
    padding: 0.15rem 0.45rem;
    border-radius: 6px;
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .chip.muted {
    background: var(--pms-hover);
    color: var(--pms-muted);
  }
  .counts {
    display: flex;
    gap: 1.25rem;
  }
  .counts div {
    display: flex;
    flex-direction: column;
  }
  .counts span {
    font-size: 0.74rem;
    color: var(--pms-muted);
  }
  .counts strong {
    font-size: 1.15rem;
  }
  .skips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .result {
    font-size: 0.85rem;
    color: var(--pms-success);
  }
</style>
