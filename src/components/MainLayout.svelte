<script lang="ts">
  // 主界面骨架（PRD F-03/F-04）：顶栏 + 分区导航 + 三栏内容区。
  import Icon from './common/Icon.svelte';
  import FolderSidebar from './FolderSidebar.svelte';
  import HealthCheck from './HealthCheck.svelte';
  import ImportWizard from './ImportWizard.svelte';
  import ItemDetail from './ItemDetail.svelte';
  import ItemEditor from './ItemEditor.svelte';
  import ItemList from './ItemList.svelte';
  import Settings from './Settings.svelte';
  import Trash from './Trash.svelte';
  import ConflictPanel from './sync/ConflictPanel.svelte';
  import DeviceList from './sync/DeviceList.svelte';
  import PairScan from './sync/PairScan.svelte';
  import PairShow from './sync/PairShow.svelte';
  import SyncStatus from './sync/SyncStatus.svelte';
  import Modal from './common/Modal.svelte';
  import { session } from '../lib/stores/session.svelte';
  import { settings } from '../lib/stores/settings.svelte';
  import { sync } from '../lib/stores/sync.svelte';
  import { ui, type MainSection } from '../lib/stores/ui.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { LOCALES, type Locale } from '../lib/i18n/index';

  type SyncTab = 'pair' | 'devices' | 'status' | 'conflicts';

  let editing = $state(false);
  let creatingKind = $state<'login' | 'secure_note' | null>(null);
  let syncTab = $state<SyncTab>('pair');
  let importOpen = $state(false);

  const NAV: { key: MainSection; icon: string; label: string }[] = [
    { key: 'items', icon: 'key', label: 'nav.items' },
    { key: 'health', icon: 'shield', label: 'nav.health' },
    { key: 'trash', icon: 'trash', label: 'nav.trash' },
    { key: 'sync', icon: 'sync', label: 'nav.sync' },
    { key: 'settings', icon: 'settings', label: 'nav.settings' },
  ];

  // 解锁后加载数据（会话状态由 App 驱动）。
  $effect((): void => {
    if (!session.status.locked) {
      void vault.load();
      void settings.load();
      void sync.loadStatus();
      void sync.loadDevices();
      void sync.loadConflicts();
    }
  });

  function startCreate(kind: 'login' | 'secure_note'): void {
    creatingKind = kind;
    editing = true;
  }

  function startEdit(): void {
    creatingKind = null;
    editing = true;
  }

  function finishEdit(): void {
    editing = false;
    creatingKind = null;
  }

  async function lockNow(): Promise<void> {
    await session.lock();
    vault.reset();
    settings.reset();
    sync.reset();
    ui.setRoute('lock');
  }

  function switchLocale(): void {
    const index = LOCALES.indexOf(ui.locale);
    const next: Locale = LOCALES[(index + 1) % LOCALES.length];
    ui.setLocale(next);
  }

  let detailKind = $derived(
    creatingKind ?? (vault.selectedItem?.kind.type === 'secure_note' ? 'secure_note' : 'login'),
  );
</script>

<div class="layout">
  <header class="topbar">
    <div class="topbar-brand">
      <span class="topbar-logo"><Icon name="shield" size={18} /></span>
      <span class="topbar-title">{ui.t('app.title')}</span>
    </div>
    <nav class="topbar-nav">
      {#each NAV as entry (entry.key)}
        <button
          class="topbar-tab"
          class:active={ui.section === entry.key}
          type="button"
          onclick={() => ui.setSection(entry.key)}
        >
          <Icon name={entry.icon} size={16} />
          <span>{ui.t(entry.label)}</span>
        </button>
      {/each}
    </nav>
    <div class="topbar-right">
      <button class="pms-btn is-ghost is-sm" type="button" onclick={switchLocale}>
        <Icon name="globe" size={15} />{ui.locale}
      </button>
      <button class="pms-btn is-ghost is-sm" type="button" onclick={() => (importOpen = true)}>
        <Icon name="upload" size={15} />{ui.t('import.title')}
      </button>
      <button class="pms-btn is-ghost is-sm" type="button" onclick={() => void lockNow()}>
        <Icon name="lock" size={15} />{ui.t('nav.lock')}
      </button>
    </div>
  </header>

  <main class="body">
    {#if ui.section === 'items'}
      <FolderSidebar />
      <ItemList onnew={startCreate} />
      {#if editing}
        <ItemEditor
          item={creatingKind === null ? vault.selectedItem : null}
          kind={detailKind}
          ondone={finishEdit}
          oncancel={finishEdit}
        />
      {:else}
        <ItemDetail onedit={startEdit} />
      {/if}
    {:else if ui.section === 'health'}
      <HealthCheck />
    {:else if ui.section === 'trash'}
      <Trash />
    {:else if ui.section === 'settings'}
      <Settings />
    {:else}
      <section class="sync-view">
        <nav class="sync-tabs">
          <button
            class="sync-tab"
            class:active={syncTab === 'pair'}
            type="button"
            onclick={() => (syncTab = 'pair')}
          >
            {ui.t('sync.tab_pair')}
          </button>
          <button
            class="sync-tab"
            class:active={syncTab === 'devices'}
            type="button"
            onclick={() => (syncTab = 'devices')}
          >
            {ui.t('sync.tab_devices')}
          </button>
          <button
            class="sync-tab"
            class:active={syncTab === 'status'}
            type="button"
            onclick={() => (syncTab = 'status')}
          >
            {ui.t('sync.tab_status')}
          </button>
          <button
            class="sync-tab"
            class:active={syncTab === 'conflicts'}
            type="button"
            onclick={() => (syncTab = 'conflicts')}
          >
            {ui.t('sync.tab_conflicts')}
          </button>
        </nav>

        {#if syncTab === 'pair'}
          <div class="sync-grid">
            <PairShow />
            <PairScan />
          </div>
        {:else if syncTab === 'devices'}
          <DeviceList />
        {:else if syncTab === 'status'}
          <SyncStatus />
        {:else}
          <ConflictPanel />
        {/if}
      </section>
    {/if}
  </main>

  <Modal open={importOpen} titleKey="import.title" onclose={() => (importOpen = false)}>
    <ImportWizard ondone={() => (importOpen = false)} />
  </Modal>
</div>

<style>
  .layout {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 0.85rem;
    min-height: 56px;
    border-bottom: 1px solid var(--pms-border);
    background: var(--pms-surface);
  }
  .topbar-brand {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  .topbar-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--pms-r-sm);
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .topbar-title {
    font-size: 1rem;
    font-weight: 600;
  }
  .topbar-nav {
    display: flex;
    gap: 0.2rem;
  }
  .topbar-tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    min-height: 40px;
    padding: 0 0.7rem;
    border: none;
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-muted);
    cursor: pointer;
    font-size: 0.84rem;
    font-family: inherit;
    transition: background var(--pms-dur) var(--pms-ease), color var(--pms-dur) var(--pms-ease);
  }
  .topbar-tab:hover {
    background: var(--pms-bg-2);
    color: var(--pms-fg);
  }
  .topbar-tab.active {
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .topbar-tab.active::after {
    content: '';
    position: absolute;
    left: 0.4rem;
    right: 0.4rem;
    bottom: -0.5rem;
    height: 2px;
    border-radius: var(--pms-r-pill);
    background: var(--pms-accent);
  }
  .topbar-right {
    margin-left: auto;
    display: flex;
    gap: 0.35rem;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
    background: var(--pms-bg);
  }
  .sync-view {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: auto;
  }
  .sync-tabs {
    display: flex;
    gap: 0.3rem;
    padding: 0.7rem 0.85rem;
    border-bottom: 1px solid var(--pms-border);
  }
  .sync-tab {
    min-height: 34px;
    padding: 0 0.8rem;
    border: 1px solid var(--pms-border-strong);
    border-radius: var(--pms-r-sm);
    background: transparent;
    color: var(--pms-muted);
    cursor: pointer;
    font-size: 0.82rem;
    font-family: inherit;
    transition: background var(--pms-dur) var(--pms-ease), border-color var(--pms-dur) var(--pms-ease),
      color var(--pms-dur) var(--pms-ease);
  }
  .sync-tab:hover {
    background: var(--pms-bg-2);
  }
  .sync-tab.active {
    border-color: var(--pms-accent);
    background: var(--pms-accent-soft);
    color: var(--pms-accent);
  }
  .sync-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 1rem;
    padding: 1rem;
  }
</style>
