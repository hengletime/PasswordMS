<script lang="ts">
  // 左侧分组栏（PRD F-03）。
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

<aside class="sidebar">
  <nav>
    <button
      class="row"
      class:active={vault.selectedFolderId === null && !vault.favoriteOnly}
      type="button"
      onclick={() => void vault.selectFolder(null)}
    >
      <Icon name="key" size={16} />
      <span>{ui.t('folder.all')}</span>
    </button>
    <button
      class="row"
      class:active={vault.favoriteOnly}
      type="button"
      onclick={() => void vault.toggleFavoriteOnly()}
    >
      <Icon name="star" size={16} />
      <span>{ui.t('nav.favorites')}</span>
    </button>
  </nav>

  <div class="section-head">
    <span>{ui.t('folder.all')}</span>
    <button class="icon-btn" type="button" aria-label={ui.t('folder.new')} onclick={() => (creating = true)}>
      <Icon name="plus" size={16} />
    </button>
  </div>

  {#if creating}
    <form class="inline-form" onsubmit={submitNew}>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        autofocus
        placeholder={ui.t('folder.name_placeholder')}
        bind:value={newName}
        onblur={() => (creating = false)}
      />
    </form>
  {/if}

  <ul class="folders">
    {#each vault.folders as folder (folder.folder_id)}
      <li>
        {#if editingId === folder.folder_id}
          <form class="inline-form" onsubmit={submitEdit}>
            <!-- svelte-ignore a11y_autofocus -->
            <input autofocus bind:value={editingName} onblur={() => (editingId = null)} />
          </form>
        {:else}
          <button
            class="row"
            class:active={vault.selectedFolderId === folder.folder_id && !vault.favoriteOnly}
            type="button"
            ondblclick={() => startEdit(folder.folder_id, folder.name)}
            onclick={() => void vault.selectFolder(folder.folder_id)}
          >
            <Icon name="folder" size={16} />
            <span class="name">{folder.name}</span>
          </button>
          <button
            class="icon-btn subtle"
            type="button"
            aria-label={ui.t('common.delete')}
            onclick={() => void removeFolder(folder.folder_id, folder.name)}
          >
            <Icon name="close" size={14} />
          </button>
        {/if}
      </li>
    {:else}
      <li class="empty">{ui.t('folder.empty')}</li>
    {/each}
  </ul>
</aside>

<style>
  .sidebar {
    width: 220px;
    flex-shrink: 0;
    border-right: 1px solid var(--pms-border);
    padding: 0.75rem;
    overflow: auto;
  }
  nav,
  .folders {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }
  .folders {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .folders li {
    display: flex;
    align-items: center;
  }
  .row {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.5rem;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--pms-fg);
    cursor: pointer;
    font-size: 0.88rem;
    text-align: left;
  }
  .row:hover {
    background: var(--pms-hover);
  }
  .row.active {
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: 0.9rem 0 0.3rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--pms-muted);
  }
  .icon-btn {
    background: transparent;
    border: none;
    color: var(--pms-muted);
    cursor: pointer;
    padding: 0.2rem;
    line-height: 0;
  }
  .icon-btn.subtle {
    opacity: 0;
  }
  li:hover .icon-btn.subtle {
    opacity: 1;
  }
  .inline-form {
    padding: 0.2rem 0.1rem;
  }
  .inline-form input {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--pms-accent);
    border-radius: 6px;
    background: var(--pms-bg);
    color: var(--pms-fg);
    padding: 0.35rem 0.5rem;
    font-size: 0.85rem;
  }
  .empty {
    color: var(--pms-muted);
    font-size: 0.8rem;
    padding: 0.4rem 0.5rem;
  }
</style>
