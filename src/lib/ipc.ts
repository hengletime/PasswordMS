// 类型化 IPC 封装（架构文档 §5.2）。
//
// ⚠️ 命令名与参数键名**必须**与 `src-tauri/src/lib.rs` 的 `generate_handler!` 及
// `#[tauri::command(rename_all = "snake_case")]` 完全一致（参数一律 snake_case）。

import { invoke } from '@tauri-apps/api/core';

import type {
  CompactionResultDto,
  CompactionStatsDto,
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
  // D-76：保险库体积构成（只读：不解密 blob、不写任何字节）+ 压缩（重写容器，只留活跃帧）。
  // 两条命令都不带参数 —— 后端签名是 `vault_compaction_stats(state)` / `compact_vault(app)`；
  // 若将来加参数，键名必须 snake_case（§14 不变量）。
  vaultCompactionStats: (): Promise<CompactionStatsDto> =>
    invoke<CompactionStatsDto>('vault_compaction_stats'),
  compactVault: (): Promise<CompactionResultDto> =>
    invoke<CompactionResultDto>('compact_vault'),
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
  // D-61：自动锁定档位读写（`Vault.auto_lock_minutes` 明文头部；仅 1/5/15 合法）。
  getAutoLock: (): Promise<number> => invoke<number>('get_auto_lock'),
  setAutoLock: (minutes: number): Promise<void> => invoke<void>('set_auto_lock', { minutes }),
  changeMasterPassword: (oldPassword: string, newPassword: string): Promise<void> =>
    invoke<void>('change_master_password', {
      old_password: oldPassword,
      new_password: newPassword,
    }),
  exportVault: (destPath: string, masterPassword: string): Promise<void> =>
    invoke<void>('export_vault', { dest_path: destPath, master_password: masterPassword }),
  // D-55 从备份恢复：source_path 必须是 PasswordMS 导出的 .vault 备份（后端校验 magic）。
  restoreBackup: (sourcePath: string): Promise<void> =>
    invoke<void>('restore_backup', { source_path: sourcePath }),
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
  // #531：排序模型 = 钉住块（pin_order）→ 未钉按频度。以下命令后端并行注册中（53→59），
  // 命令名与参数键已按契约钉死，逐字一致。
  // 复制成功后埋点（后端 60s 节流，前端每处复制成功调一次即可）。
  recordUsage: (itemId: string): Promise<void> =>
    invoke<void>('record_item_usage', { item_id: itemId }),
  // 拖拽松手 = 把该条钉住（目标位由 reorder_pinned_items 表达）。
  setPin: (itemId: string, pinned: boolean): Promise<void> =>
    invoke<void>('set_item_pin', { item_id: itemId, pinned }),
  // 已钉住集合的新顺序（仅钉住集合内的相对顺序）。
  reorderPinned: (orderedIds: string[]): Promise<void> =>
    invoke<void>('reorder_pinned_items', { ordered_ids: orderedIds }),
  // 跨分组移动；folderId 传 null = 移到未分组。
  moveToFolder: (itemId: string, folderId: string | null): Promise<void> =>
    invoke<void>('move_item_to_folder', { item_id: itemId, folder_id: folderId ?? null }),
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
  // #531：分组钉住与钉住排序（旧 `reorder_folders` 的排序语义作废，命令仍在册）。
  setPin: (folderId: string, pinned: boolean): Promise<void> =>
    invoke<void>('set_folder_pin', { folder_id: folderId, pinned }),
  reorderPinned: (orderedIds: string[]): Promise<void> =>
    invoke<void>('reorder_pinned_folders', { ordered_ids: orderedIds }),
};

/** 密钥文档（附件）类。D-60：附件为父条目内嵌 `attachments`，按 (item_id, attachment_id) 定位。 */
export const keydocs = {
  // 向父条目添加附件，返回**父条目**（rev+1）。
  addAttachment: (itemId: string, sourcePath: string, title: string): Promise<ItemDto> =>
    invoke<ItemDto>('add_attachment', { item_id: itemId, source_path: sourcePath, title }),
  // 从父条目移除附件（blob 交孤儿 GC），返回父条目（rev+1）。
  removeAttachment: (itemId: string, attachmentId: string): Promise<ItemDto> =>
    invoke<ItemDto>('remove_attachment', { item_id: itemId, attachment_id: attachmentId }),
  exportAttachment: (itemId: string, attachmentId: string, destPath: string): Promise<void> =>
    invoke<void>('export_attachment', { item_id: itemId, attachment_id: attachmentId, dest_path: destPath }),
  // D-53/D-60：附件 blob 是否已同步到本机（「附件内容待同步」态判定，按 attachment_id）。
  attachmentBlobReady: (attachmentId: string): Promise<boolean> =>
    invoke<boolean>('attachment_blob_ready', { attachment_id: attachmentId }),
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
  /**
   * D-72：把内置的 CSV 导入模板写到用户选定的位置。
   *
   * 为什么前端不自己写文件：Android 的 `save()` 返回的是 **`content://…` URI**
   * （`DialogPlugin.kt` 的 `saveFileDialog` 用 `ACTION_CREATE_DOCUMENT`，回调里
   * `put("file", uri.toString())`），WebView 侧没有能直接写它的 API，必须交给后端
   * 用 Android 的文件描述符通路写（同一个原因也导致 D-71 的导出报「文件读写出错」）。
   * 前端只负责把用户选到的位置**原样**传下去，不做任何路径解析/拼接。
   *
   * 参数名用 snake_case `dest_path`：本仓 `tauri::command` 全部带
   * `rename_all = "snake_case"`（`src-tauri/src/commands/import_cmd.rs` 同文件的
   * `import_preview` / `import_apply` 也是），所以 wire 上的键必须与 Rust 形参同名。
   * 若某天该命令去掉 `rename_all`，这里要同步改成 `destPath`，否则会 invalid args。
   */
  saveTemplate: (destPath: string): Promise<void> =>
    invoke<void>('save_import_template', { dest_path: destPath }),
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
  // D-66：重命名已配对设备（本地昵称，不参与同步）。
  renameDevice: (deviceId: string, name: string): Promise<void> =>
    invoke<void>('rename_paired_device', { device_id: deviceId, name }),
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
