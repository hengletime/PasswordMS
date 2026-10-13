<script lang="ts">
  // 条目编辑器（PRD F-04 / F-13）：三种 kind 的表单 + 自定义字段增删 + URL 校验 + 生成器。
  import Icon from './common/Icon.svelte';
  import PasswordGenerator from './PasswordGenerator.svelte';
  import { MAX_NOTE_BYTES } from '../lib/constants';
  import { isSafeUrl } from '../lib/url-safety';
  import { ui } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import type { CustomField, ItemDraftDto, ItemDto } from '../types';

  interface Props {
    item?: ItemDto | null;
    kind: 'login' | 'secure_note';
    ondone: () => void;
    oncancel: () => void;
  }
  const { item, kind, ondone, oncancel }: Props = $props();

  // 打开编辑器时对条目做一次性快照（表单为本地可编辑副本，父组件后续变化不覆盖）。
  // svelte-ignore state_referenced_locally
  const login = item?.kind.type === 'login' ? item.kind : null;
  // svelte-ignore state_referenced_locally
  const note = item?.kind.type === 'secure_note' ? item.kind : null;

  let title = $state(login?.title ?? note?.title ?? '');
  let url = $state(login?.url ?? '');
  let username = $state(login?.username ?? '');
  let password = $state(login?.password ?? '');
  let notes = $state(login?.notes ?? '');
  let totp = $state(login?.totp_secret ?? '');
  let body = $state(note?.body ?? '');
  // svelte-ignore state_referenced_locally
  let folderId = $state<string | null>(item?.folder_id ?? null);
  // svelte-ignore state_referenced_locally
  let favorite = $state(item?.favorite ?? false);
  let fields = $state<CustomField[]>(
    login ? login.custom_fields.map((f): CustomField => ({ ...f })) : [],
  );
  let showPassword = $state(false);
  let showGenerator = $state(false);

  let noteBytes = $derived(new Blob([body]).size);
  let bodyTooLong = $derived(kind === 'secure_note' && noteBytes > MAX_NOTE_BYTES);
  let urlUnsafe = $derived(kind === 'login' && url.trim().length > 0 && !isSafeUrl(url));
  let canSave = $derived(
    title.trim().length > 0 && !bodyTooLong && !urlUnsafe && !vault.busy,
  );

  function addField(): void {
    fields = [...fields, { key: '', value: '', hidden: false }];
  }

  function removeField(index: number): void {
    fields = fields.filter((_value, i): boolean => i !== index);
  }

  async function save(): Promise<void> {
    if (!canSave) {
      return;
    }
    let draft: ItemDraftDto;
    if (kind === 'login') {
      draft = {
        kind: {
          type: 'login',
          title: title.trim(),
          url: url.trim() || null,
          username: username.trim() || null,
          password: password.length > 0 ? password : null,
          notes: notes.trim() || null,
          totp_secret: totp.trim() || null,
          custom_fields: fields.filter((f): boolean => f.key.trim().length > 0),
        },
        folder_id: folderId,
        favorite,
      };
    } else {
      draft = {
        kind: { type: 'secure_note', title: title.trim(), body },
        folder_id: folderId,
        favorite,
      };
    }
    const existing = item;
    const result =
      existing !== null && existing !== undefined && existing.kind.type === kind
        ? await vault.updateItem(existing.item_id, draft)
        : await vault.createItem(draft);
    if (result !== null) {
      ondone();
    }
  }
</script>

<form
  class="editor"
  onsubmit={(event: SubmitEvent): void => {
    event.preventDefault();
    void save();
  }}
>
  <label class="field">
    <span>{ui.t('editor.field_title')}</span>
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={title} autofocus />
  </label>

  {#if kind === 'login'}
    <label class="field">
      <span>{ui.t('editor.field_url')}</span>
      <input bind:value={url} type="url" placeholder="https://" aria-invalid={urlUnsafe} />
      {#if urlUnsafe}
        <small class="err">{ui.t('err.unsafe_url')}</small>
      {/if}
    </label>

    <label class="field">
      <span>{ui.t('editor.field_username')}</span>
      <input bind:value={username} autocomplete="off" />
    </label>

    <div class="field">
      <span>{ui.t('editor.field_password')}</span>
      <div class="field-row">
        <input bind:value={password} type={showPassword ? 'text' : 'password'} autocomplete="new-password" />
        <button
          class="icon-btn"
          type="button"
          aria-label={showPassword ? ui.t('editor.hide_password') : ui.t('editor.show_password')}
          onclick={() => (showPassword = !showPassword)}
        >
          <Icon name={showPassword ? 'eye_off' : 'eye'} size={16} />
        </button>
        <button
          class="icon-btn"
          type="button"
          aria-label={ui.t('gen.title')}
          onclick={() => (showGenerator = !showGenerator)}
        >
          <Icon name="refresh" size={16} />
        </button>
      </div>
      {#if showGenerator}
        <div class="generator-box">
          <PasswordGenerator
            onuse={(value: string): void => {
              password = value;
              showGenerator = false;
            }}
          />
        </div>
      {/if}
    </div>

    <label class="field">
      <span>{ui.t('editor.field_totp')}</span>
      <input bind:value={totp} autocomplete="off" placeholder="BASE32" />
    </label>

    <div class="field">
      <span>{ui.t('editor.custom_fields')}</span>
      {#each fields as field, index (index)}
        <div class="field-row custom">
          <input class="key" placeholder={ui.t('editor.field_key')} bind:value={field.key} />
          <input class="value" placeholder={ui.t('editor.field_value')} bind:value={field.value} />
          <button
            class="icon-btn"
            type="button"
            aria-label={ui.t('common.remove')}
            onclick={() => removeField(index)}
          >
            <Icon name="close" size={14} />
          </button>
        </div>
      {/each}
      <button class="btn ghost small" type="button" onclick={addField}>
        <Icon name="plus" size={14} />{ui.t('editor.add_field')}
      </button>
    </div>
  {:else}
    <label class="field">
      <span>{ui.t('note.body')}</span>
      <textarea
        rows="12"
        placeholder={ui.t('note.placeholder')}
        bind:value={body}
        aria-invalid={bodyTooLong}
      ></textarea>
      {#if bodyTooLong}
        <small class="err">{ui.t('note.too_long', { n: Math.floor(MAX_NOTE_BYTES / 1024) })}</small>
      {/if}
    </label>
  {/if}

  <label class="field">
    <span>{ui.t('editor.folder')}</span>
    <select bind:value={folderId}>
      <option value={null}>{ui.t('folder.ungrouped')}</option>
      {#each vault.folders as folder (folder.folder_id)}
        <option value={folder.folder_id}>{folder.name}</option>
      {/each}
    </select>
  </label>

  <label class="check">
    <input type="checkbox" bind:checked={favorite} />
    <span>{ui.t('editor.favorite')}</span>
  </label>

  <div class="actions">
    <button class="btn ghost" type="button" onclick={oncancel}>{ui.t('editor.cancel')}</button>
    <button class="btn primary" type="submit" disabled={!canSave}>{ui.t('editor.save')}</button>
  </div>
</form>

<style>
  .editor {
    padding: 1rem;
    overflow: auto;
  }
  .generator-box {
    margin-top: 0.6rem;
    border: 1px solid var(--pms-border);
    border-radius: 10px;
    padding: 0.85rem;
    background: var(--pms-surface);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1rem;
    padding-top: 0.85rem;
    border-top: 1px solid var(--pms-border);
  }
  .err {
    color: var(--pms-danger);
    font-size: 0.76rem;
  }
  .field-row.custom .key {
    flex: 0 0 34%;
  }
  .field-row.custom .value {
    flex: 1;
  }
</style>
