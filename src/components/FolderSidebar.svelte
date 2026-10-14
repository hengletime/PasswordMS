<script lang="ts">
  // 左侧分组栏（PRD F-03）。设计规范 §5.3：240px、底色 --pms-bg-2、40px 行、选中左侧 2px 青绿竖条。
  import Icon from './common/Icon.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { ui } from '../lib/stores/ui.svelte';

  let creating = $state(false);
  let newName = $state('');
  let editingId = $state<string | null>(null);
  let editingName = $state('');

  function submitNew(event: SubmitEvent): void {
    event.preventDefault();
    const name = newName.trim();
    if (name.length === 0) {
      creating = false;
      return;
    }
    void vault.createFolder(name);
    newName = '';
    creating = false;
  }

  function startEdit(folderId: string, name: string): void {
    editingId = folderId;
    editingName = name;
  }

  function submitEdit(event: SubmitEvent): void {
    event.preventDefault();
    const id = editingId;
    const name = editingName.trim();
    editingId = null;
    if (id !== null && name.length > 0) {
      void vault.updateFolder(id, name);
    }
  }

  async function removeFolder(folderId: string, name: string): Promise<void> {
    const ok = await ui.confirm('folder.delete_title', 'folder.delete_body', { name });
    if (ok) {
      await vault.deleteFolder(folderId, 'to_ungrouped');
    }
  }
</script>

<aside class="side">
  <nav class="side-nav">
    <button
      class="side-row"
      class:active={vault.selectedFolderId === null && !vault.favoriteOnly}
      type="button"
      onclick={() => void vault.selectFolder(null)}
    >
      <Icon name="key" size={16} />
      <span>{ui.t('folder.all')}</span>
    </button>
    <button
      class="side-row"
      class:active={vault.favoriteOnly}
      type="button"
      onclick={() => void vault.toggleFavoriteOnly()}
    >
      <Icon name="star" size={16} />
      <span>{ui.t('nav.favorites')}</span>
    </button>
  </nav>

  <div class="side-head">
    <span>{ui.t('folder.all')}</span>
    <button class="pms-icon-btn side-add" type="button" aria-label={ui.t('folder.new')} onclick={() => (creating = true)}>
      <Icon name="plus" size={16} />
    </button>
  </div>

  {#if creating}
    <form class="side-form" onsubmit={submitNew}>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="pms-input"
        autofocus
        placeholder={ui.t('folder.name_placeholder')}
        bind:value={newName}
        onblur={() => (creating = false)}
      />
    </form>
  {/if}

  <ul class="side-folders">
    {#each vault.folders as folder (folder.folder_id)}
      <li>
        {#if editingId === folder.folder_id}
          <form class="side-form" onsubmit={submitEdit}>
            <!-- svelte-ignore a11y_autofocus -->
            <input class="pms-input" autofocus bind:value={editingName} onblur={() => (editingId = null)} />
          </form>
        {:else}
          <button
            class="side-row"
            class:active={vault.selectedFolderId === folder.folder_id && !vault.favoriteOnly}
            type="button"
            ondblclick={() => startEdit(folder.folder_id, folder.name)}
            onclick={() => void vault.selectFolder(folder.folder_id)}
          >
            <Icon name="folder" size={16} />
            <span class="side-name">{folder.name}</span>
          </button>
          <button
            class="pms-icon-btn side-del"
            type="button"
            aria-label={ui.t('common.delete')}
            onclick={() => void removeFolder(folder.folder_id, folder.name)}
          >
            <Icon name="close" size={14} />
          </button>
        {/if}
      </li>
    {:else}
      <li class="side-empty">{ui.t('folder.empty')}</li>
    {/each}
  </ul>
</aside>

<style>
  .side {
    width: 240px;
    flex-shrink: 0;
    border-right: 1px solid var(--pms-border);
    background: var(--pms-bg-2);
    padding: 0.75rem;
    overflow: auto;
  }
  .side-nav,
  .side-folders {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .side-folders {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .side-folders li {
    display: flex;
    align-items: center;
  }
  .side-row {
    position: relative;
    flex: 1;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-height: 40px;
    padding: 0 0.55rem;
    border: none;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-fg);
    cursor: pointer;
    font-size: 0.88rem;
    font-family: inherit;
    text-align: left;
    transition: background var(--pms-dur) var(--pms-ease), color var(--pms-dur) var(--pms-ease);
  }
  .side-row:hover {
    background: var(--pms-elevated);
  }
  .side-row.active {
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .side-row.active::before {
    content: '';
    position: absolute;
    left: 0;
    top: 6px;
    bottom: 6px;
    width: 2px;
    border-radius: var(--pms-r-pill);
    background: var(--pms-accent);
  }
  .side-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .side-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0.9rem 0 0.3rem;
    padding-left: 0.55rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--pms-muted);
  }
  .side-add {
    width: 26px;
    height: 26px;
    flex: 0 0 26px;
  }
  .side-del {
    width: 26px;
    height: 26px;
    flex: 0 0 26px;
    opacity: 0;
  }
  li:hover .side-del {
    opacity: 1;
  }
  .side-form {
    padding: 0.2rem 0.1rem;
  }
  .side-form .pms-input {
    min-height: 34px;
    font-size: 0.85rem;
  }
  .side-empty {
    color: var(--pms-faint);
    font-size: 0.8rem;
    padding: 0.4rem 0.55rem;
  }
</style>
