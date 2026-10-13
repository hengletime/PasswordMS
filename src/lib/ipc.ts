// 类型化 IPC 封装（架构文档 §5.2）。
//
// ⚠️ 命令名与参数键名**必须**与 `src-tauri/src/lib.rs` 的 `generate_handler!` 及
// `#[tauri::command(rename_all = "snake_case")]` 完全一致（参数一律 snake_case）。

import { invoke } from '@tauri-apps/api/core';

import type {
  ConflictChoice,
  ConflictDto,
  DeviceDto,
  FolderDto,
  GeneratorOptions,
  GeneratedPasswordDto,
  HealthConfig,
  HealthReport,
  ImportPreviewDto,
  ImportResultDto,
  ItemDraftDto,
  ItemDto,
  PairingDto,
  PairResultDto,
  StatusDto,
  SyncResultDto,
  SyncStatusDto,
  TotpDto,
  TrashDto,
  VaultFileInfoDto,
  VaultSettings,
} from '../types';

/** 导入源格式。 */
export type ImportFormat = 'json' | 'csv';

/** 系统类。 */
export const system = {
  getStatus: (): Promise<StatusDto> => invoke<StatusDto>('get_status'),
  copySecret: (text: string, ttlSecs: number): Promise<void> =>
    invoke<void>('copy_secret', { text, ttl_secs: ttlSecs }),
  // A-03：仅当剪贴板仍为本应用写入的内容时才清空；返回是否已清空。
  clearSecretIfOwn: (): Promise<boolean> => invoke<boolean>('clear_secret_if_own'),
  openUrl: (url: string): Promise<void> => invoke<void>('open_url', { url }),
  vaultFileInfo: (): Promise<VaultFileInfoDto> => invoke<VaultFileInfoDto>('vault_file_info'),
};

/** 保险库类。 */
export const vault = {
  create: (masterPassword: string, strengthConfirmed: boolean): Promise<StatusDto> =>
    invoke<StatusDto>('create_vault', {
      master_password: masterPassword,
      strength_confirmed: strengthConfirmed,
    }),
  unlock: (masterPassword: string): Promise<StatusDto> =>
    invoke<StatusDto>('unlock', { master_password: masterPassword }),
  lock: (): Promise<void> => invoke<void>('lock'),
  changeMasterPassword: (oldPassword: string, newPassword: string): Promise<void> =>
    invoke<void>('change_master_password', {
      old_password: oldPassword,
      new_password: newPassword,
    }),
  exportVault: (destPath: string, masterPassword: string): Promise<void> =>
    invoke<void>('export_vault', { dest_path: destPath, master_password: masterPassword }),
};

/** 条目类。 */
export const items = {
  list: (folderId?: string | null, favoriteOnly?: boolean, query?: string): Promise<ItemDto[]> =>
    invoke<ItemDto[]>('list_items', {
      folder_id: folderId ?? null,
      favorite_only: favoriteOnly ?? null,
      query: query ?? null,
    }),
  get: (itemId: string): Promise<ItemDto> => invoke<ItemDto>('get_item', { item_id: itemId }),
  create: (draft: ItemDraftDto): Promise<ItemDto> =>
    invoke<ItemDto>('create_item', { draft }),
  update: (itemId: string, draft: ItemDraftDto): Promise<ItemDto> =>
    invoke<ItemDto>('update_item', { item_id: itemId, draft }),
  remove: (itemId: string, permanent: boolean): Promise<void> =>
    invoke<void>('delete_item', { item_id: itemId, permanent }),
  toggleFavorite: (itemId: string, favorite: boolean): Promise<ItemDto> =>
    invoke<ItemDto>('toggle_favorite', { item_id: itemId, favorite }),
};

/** 分组类。 */
export const folders = {
  list: (): Promise<FolderDto[]> => invoke<FolderDto[]>('list_folders'),
  create: (name: string, icon?: string | null, color?: string | null): Promise<FolderDto> =>
    invoke<FolderDto>('create_folder', { name, icon: icon ?? null, color: color ?? null }),
  update: (
    folderId: string,
    name: string,
    icon?: string | null,
    color?: string | null,
  ): Promise<FolderDto> =>
    invoke<FolderDto>('update_folder', {
      folder_id: folderId,
      name,
      icon: icon ?? null,
      color: color ?? null,
    }),
  remove: (folderId: string, strategy: 'to_ungrouped' | 'to_trash'): Promise<void> =>
    invoke<void>('delete_folder', { folder_id: folderId, strategy }),
  reorder: (orderedIds: string[]): Promise<void> =>
    invoke<void>('reorder_folders', { ordered_ids: orderedIds }),
};

/** 密钥文档（附件）类。 */
export const keydocs = {
  importAttachment: (sourcePath: string, title: string): Promise<ItemDto> =>
    invoke<ItemDto>('import_attachment', { source_path: sourcePath, title }),
  exportAttachment: (itemId: string, destPath: string): Promise<void> =>
    invoke<void>('export_attachment', { item_id: itemId, dest_path: destPath }),
};

/** 生成器 / TOTP。 */
export const tools = {
  generatePassword: (opts: GeneratorOptions): Promise<GeneratedPasswordDto> =>
    invoke<GeneratedPasswordDto>('generate_password', { opts }),
  totpNow: (secret: string): Promise<TotpDto> => invoke<TotpDto>('totp_now', { secret }),
};

/** 导入类。 */
export const importer = {
  preview: (sourcePath: string, format: ImportFormat): Promise<ImportPreviewDto> =>
    invoke<ImportPreviewDto>('import_preview', { source_path: sourcePath, format }),
  apply: (
    sourcePath: string,
    format: ImportFormat,
    previewToken: string,
  ): Promise<ImportResultDto> =>
    invoke<ImportResultDto>('import_apply', {
      source_path: sourcePath,
      format,
      preview_token: previewToken,
    }),
};

/** 回收站类。 */
export const trash = {
  list: (): Promise<TrashDto[]> => invoke<TrashDto[]>('list_trash'),
  restore: (trashId: string): Promise<ItemDto> =>
    invoke<ItemDto>('restore_item', { trash_id: trashId }),
  purge: (trashId: string): Promise<void> => invoke<void>('purge_item', { trash_id: trashId }),
  empty: (): Promise<void> => invoke<void>('empty_trash'),
};

/** 体检类。 */
export const health = {
  check: (config?: HealthConfig | null): Promise<HealthReport> =>
    invoke<HealthReport>('health_check', { config: config ?? null }),
};

/** 同步 / 配对类。 */
export const sync = {
  startPairing: (): Promise<PairingDto> => invoke<PairingDto>('sync_start_pairing'),
  cancelPairing: (): Promise<void> => invoke<void>('sync_cancel_pairing'),
  join: (qrPayload: string): Promise<PairResultDto> =>
    invoke<PairResultDto>('sync_join', { qr_payload: qrPayload }),
  acceptPair: (sessionId: string): Promise<void> =>
    invoke<void>('sync_accept_pair', { session_id: sessionId }),
  listDevices: (): Promise<DeviceDto[]> => invoke<DeviceDto[]>('list_paired_devices'),
  unpair: (deviceId: string): Promise<void> => invoke<void>('unpair', { device_id: deviceId }),
  now: (deviceId: string): Promise<SyncResultDto> =>
    invoke<SyncResultDto>('sync_now', { device_id: deviceId }),
  status: (): Promise<SyncStatusDto> => invoke<SyncStatusDto>('sync_status'),
  listConflicts: (): Promise<ConflictDto[]> => invoke<ConflictDto[]>('list_conflicts'),
  resolveConflict: (conflictId: string, choice: ConflictChoice): Promise<void> =>
    invoke<void>('resolve_conflict', { conflict_id: conflictId, choice }),
};

/** 设置类。 */
export const settings = {
  get: (): Promise<VaultSettings> => invoke<VaultSettings>('get_settings'),
  update: (value: VaultSettings): Promise<VaultSettings> =>
    invoke<VaultSettings>('update_settings', { settings: value }),
};

/** 全部命令的聚合入口。 */
export const api = { system, vault, items, folders, keydocs, tools, importer, trash, health, sync, settings };
