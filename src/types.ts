// PasswordMS 前端类型定义（架构文档 §5.2 IPC 契约）。
//
// 说明：此处与 Rust 侧 DTO **逐字段对齐**；wire 格式统一 `snake_case`。
// 数值型时间戳一律为「UTC 毫秒」（i64 → number）。

/** 启动 / 解锁状态。 */
export interface StatusDto {
  has_vault: boolean;
  locked: boolean;
}

/** 自定义键值对。 */
export interface CustomField {
  key: string;
  value: string;
  hidden: boolean;
}

/** 登录型条目数据。 */
export interface LoginData {
  title: string;
  url?: string | null;
  username?: string | null;
  password?: string | null;
  notes?: string | null;
  totp_secret?: string | null;
  custom_fields: CustomField[];
}

/** 安全笔记数据。 */
export interface SecureNoteData {
  title: string;
  body: string;
}

/** 加密附件数据（D-60：仅存量库迁移读取用，运行期不再新建独立附件条目）。 */
export interface AttachmentData {
  title: string;
  original_filename: string;
  mime_type: string;
  size_bytes: number;
  blob_ref: string;
  blob_size_bytes: number;
}

/** 附件元数据（D-60 §3.14.1：内嵌于父条目 `attachments` 数组；字段名与 Rust serde 一致）。 */
export interface AttachmentMetaDto {
  /** 附件自身 UUID，同时是 blob 帧 record_id（≠ 父 item_id）。 */
  attachment_id: string;
  /** 原始文件名（随父帧密文过线，wipe_plaintext 覆盖）。 */
  filename: string;
  mime_type: string;
  size_bytes: number;
  /** 密文占用（容量统计）。 */
  blob_size_bytes: number;
}

/** 条目类型（内部标签 `type`）。 */
export type ItemKind =
  | ({ type: 'login' } & LoginData)
  | ({ type: 'secure_note' } & SecureNoteData)
  | ({ type: 'attachment' } & AttachmentData);

/** 条目。 */
export interface ItemDto {
  item_id: string;
  kind: ItemKind;
  /** 父条目内嵌附件（D-60 §3.14.1；serde default 向后兼容旧帧 → 旧帧反序列化为 []）。 */
  attachments: AttachmentMetaDto[];
  folder_id?: string | null;
  favorite: boolean;
  created_at: number;
  updated_at: number;
  rev: number;
  modified_by: string;
}

/** 条目草稿。 */
export interface ItemDraftDto {
  kind: ItemKind;
  folder_id?: string | null;
  favorite: boolean;
}

/** 分组。 */
export interface FolderDto {
  folder_id: string;
  name: string;
  icon?: string | null;
  color?: string | null;
  sort_order: number;
  rev: number;
  updated_at: number;
}

/** 回收站条目。 */
export interface TrashDto {
  trash_id: string;
  item_id: string;
  title: string;
  original_folder_id?: string | null;
  deleted_at: number;
  expire_at: number;
}

/** 冲突条目。 */
export interface ConflictDto {
  conflict_id: string;
  item_id: string;
  local_title: string;
  remote_title: string;
  local_updated_at: number;
  remote_updated_at: number;
  detected_at: number;
}

/** 冲突三选一。 */
export type ConflictChoice = 'keep_local' | 'take_remote' | 'keep_both';

/** 已配对设备。 */
export interface DeviceDto {
  device_id: string;
  device_name: string;
  fingerprint: string;
  paired_at: number;
  last_sync_at?: number | null;
}

/** 生成器选项。 */
export interface GeneratorOptions {
  length: number;
  uppercase: boolean;
  lowercase: boolean;
  digits: boolean;
  symbols: boolean;
  exclude_ambiguous: boolean;
  /** 自选符号集（D-50）：null/undefined = 默认安全集（+-_.,:）。 */
  symbol_set?: string | null;
}

/** 体检配置。 */
export interface HealthConfig {
  weak_max_len: number;
  stale_days: number;
}

/** 保险库设置。 */
export interface VaultSettings {
  generator: GeneratorOptions;
  health: HealthConfig;
  clipboard_clear_secs: number;
  auto_sync: boolean;
  sync_on_start: boolean;
}

/** 体检报告。 */
export interface HealthReport {
  total_score: number;
  login_count: number;
  weak: string[];
  reused: string[];
  stale: string[];
  suggestions: string[];
}

/** TOTP 验证码。 */
export interface TotpDto {
  code: string;
  period: number;
  remaining: number;
}

/** 生成结果。 */
export interface GeneratedPasswordDto {
  password: string;
}

/** 导入预览。 */
export interface ImportPreviewDto {
  preview_token: string;
  item_count: number;
  folder_count: number;
  skipped: number;
  skipped_by_type: Record<string, number>;
  total: number;
}

/** 导入结果。 */
export interface ImportResultDto {
  imported_items: number;
  imported_folders: number;
  skipped: number;
}

/** Host 配对信息。 */
export interface PairingDto {
  qr_svg: string;
  qr_text: string;
  ip: string;
  /** 全部候选局域网 IPv4（多网卡时主选在前，D-44）。 */
  ips: string[];
  port: number;
  expires_at: number;
}

/** 配对结果。 */
export interface PairResultDto {
  device_id: string;
  device_name: string;
  fingerprint: string;
  applied: number;
  conflicts: string[];
  /** D-53：blob 阶段实际入库的附件数。 */
  blobs?: number;
}

/** 同步结果。 */
export interface SyncResultDto {
  applied: number;
  conflicts: string[];
  /** D-53：blob 阶段实际入库的附件数。 */
  blobs?: number;
}

/** 同步状态。 */
export interface SyncStatusDto {
  last_sync_at?: number | null;
  pending_changes: number;
  device_count: number;
}

/** 保险库文件信息。 */
export interface VaultFileInfoDto {
  path: string;
  size_bytes: number;
}

/** IPC 错误 JSON（Rust `IpcError`）。 */
export interface IpcErrorShape {
  code: string;
  message_key: string;
  record_count?: number;
}
