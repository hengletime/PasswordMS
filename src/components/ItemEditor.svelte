<script lang="ts">
  // 条目编辑器（PRD F-04 / F-13）：三种 kind 的表单 + 自定义字段增删 + URL 校验 + 生成器。
  // 设计规范 §5.4：全部换用 .pms-field / .pms-input-group；自定义字段区用 .pms-card 内嵌；底部操作条 sticky。
  import Icon from './common/Icon.svelte';
  import PasswordGeneratorModal from './PasswordGeneratorModal.svelte';
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
  <div class="pms-field">
    <label class="pms-label" for="ed-title">{ui.t('editor.field_title')}</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="ed-title" class="pms-input" bind:value={title} autofocus />
  </div>

  {#if kind === 'login'}
    <div class="pms-field">
      <label class="pms-label" for="ed-url">{ui.t('editor.field_url')}</label>
      <input id="ed-url" class="pms-input" bind:value={url} type="url" placeholder="https://" aria-invalid={urlUnsafe} />
      {#if urlUnsafe}
        <p class="pms-hint is-danger">{ui.t('err.unsafe_url')}</p>
      {/if}
    </div>

    <div class="pms-field">
      <label class="pms-label" for="ed-username">{ui.t('editor.field_username')}</label>
      <input id="ed-username" class="pms-input" bind:value={username} autocomplete="off" />
    </div>

    <div class="pms-field">
      <label class="pms-label" for="ed-password">{ui.t('editor.field_password')}</label>
      <div class="pms-input-group">
        <input
          id="ed-password"
          class="pms-input"
          bind:value={password}
          type={showPassword ? 'text' : 'password'}
          autocomplete="new-password"
        />
        <button
          class="pms-icon-btn"
          type="button"
          title={showPassword ? ui.t('editor.hide_password') : ui.t('editor.show_password')}
          aria-label={showPassword ? ui.t('editor.hide_password') : ui.t('editor.show_password')}
          onclick={() => (showPassword = !showPassword)}
        >
          <Icon name={showPassword ? 'eye' : 'eye_off'} size={16} />
        </button>
        <button
          class="pms-icon-btn"
          type="button"
          title={ui.t('gen.title')}
          aria-label={ui.t('gen.title')}
          onclick={() => (showGenerator = true)}
        >
          <Icon name="dice" size={16} />
        </button>
      </div>
      <PasswordGeneratorModal
        open={showGenerator}
        onclose={() => (showGenerator = false)}
        onuse={(value: string): void => {
          password = value;
          showGenerator = false;
        }}
      />
    </div>

    <div class="pms-field">
      <label class="pms-label" for="ed-totp">{ui.t('editor.field_totp')}</label>
      <input id="ed-totp" class="pms-input" bind:value={totp} autocomplete="off" placeholder="BASE32" />
    </div>

    <div class="pms-field">
      <span class="pms-label">{ui.t('editor.custom_fields')}</span>
      <div class="pms-card editor-custom">
        {#each fields as field, index (index)}
          <div class="editor-custom-row">
            <input
              class="pms-input editor-key"
              placeholder={ui.t('editor.field_key')}
              aria-label={ui.t('editor.field_key')}
              bind:value={field.key}
            />
            <input
              class="pms-input editor-val"
              placeholder={ui.t('editor.field_value')}
              aria-label={ui.t('editor.field_value')}
              bind:value={field.value}
            />
            <button
              class="pms-icon-btn"
              type="button"
              aria-label={ui.t('common.remove')}
              onclick={() => removeField(index)}
            >
              <Icon name="close" size={14} />
            </button>
          </div>
        {/each}
        <button class="pms-btn is-ghost is-sm editor-add" type="button" onclick={addField}>
          <Icon name="plus" size={14} />{ui.t('editor.add_field')}
        </button>
      </div>
    </div>
  {:else}
    <div class="pms-field">
      <label class="pms-label" for="ed-body">{ui.t('note.body')}</label>
      <textarea
        id="ed-body"
        class="pms-input"
        rows="12"
        placeholder={ui.t('note.placeholder')}
        bind:value={body}
        aria-invalid={bodyTooLong}
      ></textarea>
      {#if bodyTooLong}
        <p class="pms-hint is-danger">{ui.t('note.too_long', { n: Math.floor(MAX_NOTE_BYTES / 1024) })}</p>
      {/if}
    </div>
  {/if}

  <div class="pms-field">
    <label class="pms-label" for="ed-folder">{ui.t('editor.folder')}</label>
    <select id="ed-folder" class="pms-input" bind:value={folderId}>
      <option value={null}>{ui.t('folder.ungrouped')}</option>
      {#each vault.folders as folder (folder.folder_id)}
        <option value={folder.folder_id}>{folder.name}</option>
      {/each}
    </select>
  </div>

  <label class="pms-check">
    <input type="checkbox" bind:checked={favorite} />
    <span>{ui.t('editor.favorite')}</span>
  </label>

  <div class="editor-actions">
    <button class="pms-btn is-ghost" type="button" onclick={oncancel}>{ui.t('editor.cancel')}</button>
    <button class="pms-btn is-primary" type="submit" disabled={!canSave}>{ui.t('editor.save')}</button>
  </div>
</form>

<style>
  .editor {
    padding: 1.1rem;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }
  .editor-custom {
    padding: 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .editor-custom-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .editor-key {
    flex: 0 0 34%;
    min-height: 34px;
  }
  .editor-val {
    flex: 1;
    min-height: 34px;
  }
  .editor-add {
    align-self: flex-start;
  }
  .editor-actions {
    position: sticky;
    bottom: 0;
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.5rem;
    padding: 0.85rem 0 0.2rem;
    border-top: 1px solid var(--pms-border);
    background: var(--pms-bg);
  }
  /* 移动端：编辑器独占整屏（`flex:1` 撑满单栏 pane），避免内容宽度按内容收缩。 */
  @media (max-width: 767px) {
    .editor {
      flex: 1;
      min-width: 0;
    }
  }
</style>
