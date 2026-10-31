// PasswordMS 前端类型定义（架构文档 §5.2 IPC 契约）。
//
// 说明：此处与 Rust 侧 DTO **逐字段对齐**；wire 格式统一 `snake_case`。
// 数值型时间戳一律为「UTC 毫秒」（i64 → number）。

/** 启动 / 解锁状态。 */
export interface StatusDto {
  has_vault: boolean;
  locked: boolean;
}

/**
 * 生物识别快速解锁状态（`docs/08-biometric-unlock.md` §10 / §23）。
 *
 * 只带状态、不带密钥材料：`availability` 决定按钮显不显示/置不置灰，
 * `state` 决定是「未开启」「已开启」还是「已失效」，`reason` 只在 `stale` 时有值，
 * `platform` 决定用哪套平台名词（Windows Hello / 指纹 · 人脸）。
 */
export interface BiometricStatusDto {
  /** `available` / `not_configured` / `not_present` / `disabled_by_policy` / `busy` / `unsupported` / `not_implemented`。 */
  availability: string;
  /** `absent` / `corrupt` / `ready` / `stale`。 */
  state: string;
  /** `master_password_changed` / `vault_key_changed` / `enrollment_changed` / `tampered`。 */
  reason?: string;
  /** 封印创建时间（毫秒），仅 `ready` 时有值。 */
  enabled_at?: number;
  /**
   * 当前平台：`windows` / `android` / `other`（§23）。
   *
   * 由后端给出而非前端嗅探 UA——UA 会随 WebView 版本与厂商 ROM 漂移。
   * 老版本后端不带这个字段时按 `other` 处理（见 `bioPlatformOf`）。
   */
  platform?: string;
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
  // #531：排序模型 = 钉住块（pin_order）→ 未钉按频度（use_count → last_used_at → updated_at）。
  // 后端在途（#531 并行注册中）：落地前 wire 上没有这些键 → 运行时为 undefined，
  // 组件层一律用 `pinned === true` 这类守卫读，不得直接当真值用。
  pinned: boolean;
  pin_order: number;
  use_count: number;
  last_used_at: number | null;
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
  // #531：与 ItemDto 同族（钉住块 + 频度；组频度 = 组内条目使用之和）。后端在途，见 ItemDto 注。
  pinned: boolean;
  pin_order: number;
  use_count: number;
  last_used_at: number | null;
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
  /** D-134：配对记录里保存的对端 IPv4（null = 没记下地址）。 */
  peer_ip?: string | null;
  /** D-134：配对记录里保存的对端重同步端口（null = 从未协商）。 */
  peer_port?: number | null;
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
  // D-133（v1.4.16）：`auto_sync` / `sync_on_start` 已随自动同步一起回退删除。
  // 旧库里残留的这两个键会被 Rust 侧 serde 忽略，下次保存即消失。
}

/** D-134：本机同步网络信息（只读诊断，`sync_net_info`）。 */
export interface SyncNetInfoDto {
  /** 本机全部局域网 IPv4。 */
  local_ips: string[];
  /** 当前重同步监听端口（null = 监听器未启动）。 */
  listen_port: number | null;
  /** UDP 发现应答器是否绑定成功。 */
  discovery_enabled: boolean;
  /** 配对时的固定发现端口（两端应一致）。 */
  discovery_port: number;
}

/**
 * D-149：Android 局域网访问权限状态。
 *
 * `supported = false` 表示**这个平台/系统版本没有这个权限**（桌面、Android < 17），
 * 前端据此整块不渲染——「未授予」在那些平台是个伪问题。
 */
export interface LanPermissionDto {
  supported: boolean;
  /** `supported === false` 时恒 true（语义：不构成阻塞）。 */
  granted: boolean;
}

/** 体检报告。 */
export interface HealthReport {
  total_score: number;
  login_count: number;
  weak: string[];
  reused: string[];
  /** 重复使用按密码分组（D-99）：每组 = 共用同一密码的条目 ID 列表。 */
  reused_groups: string[][];
  stale: string[];
  // D-112：suggestions 文案字段已移除——core 只出数据，前端按计数 + i18n 组装建议。
}

/** D-112：导入 CSV 模板的本地化示例文本（前端从 i18n 取值后传入）。 */
export interface TemplateTexts {
  folder: string;
  login_name: string;
  /** 必须含至少一个半角逗号（引号转义示范的组成部份）。 */
  login_notes: string;
  note_name: string;
  note_body: string;
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
  /** D-90：同名同型条目跳过数。 */
  duplicate_skipped: number;
  /** D-90：与现有库同名而合并（未新建）的分组数。 */
  merged_folders: number;
}

/** Host 配对信息。 */
export interface PairingDto {
  qr_svg: string;
  qr_text: string;
  ip: string;
  /** 全部候选局域网 IPv4（多网卡时主选在前，D-44）。 */
  ips: string[];
  port: number;
  /**
   * D-69：本次配对的**会话码**，`sha256(host_eph 公钥)` 前 8 字节的小写 hex（16 位）。
   *
   * 与 `PairResultDto.session_code`（加入侧由 `Accept.host_pub` 算出）**必然同值** ——
   * 两屏算的是同一把临时公钥，这才是用户能核对、且核对得出意义的量。
   *
   * **不是**「设备指纹」：会话码每次配对都换（绑本次临时密钥），设备指纹
   * （`DeviceDto.fingerprint`，长期身份密钥）稳定不变。二者混用会让两屏永远对不上，
   * 用户每次配对都看到「不一致」→ 要么误判被劫持，要么从此不再核对。
   */
  session_code: string;
  expires_at: number;
}

/** 配对结果。 */
export interface PairResultDto {
  device_id: string;
  device_name: string;
  /** D-69：Host 的**长期设备指纹**（取自二维码 `fp`，稳定不变），用于设备列表辨识。 */
  device_fingerprint: string;
  /** D-69：本次配对的**会话码**，与电脑屏显示的必须是同一个值（16 位小写 hex）。 */
  session_code: string;
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

/** 保险库文件信息（D-108：后端只回传文件名，不暴露绝对路径）。 */
export interface VaultFileInfoDto {
  file_name: string;
  size_bytes: number;
}

/**
 * 保险库体积构成（D-76，**只读**统计；镜像 Rust `dto.rs::CompactionStatsDto`）。
 *
 * 回答「设置里显示的 24 MB 里，有多少是已删附件留下的死数据」——不需要真的压缩就能给结论。
 * 字段名与 serde 输出**逐字一致**（Rust 结构体本身就是 snake_case，未额外加 `rename_all`）。
 *
 * ⚠️ **精度说明（不是疏忽）**：Rust 侧字节数字段是 `u64`，这里一律映射成 `number` ——
 * 这是本仓既有惯例（`VaultFileInfoDto.size_bytes` 同样写法）。理论上 > 2^53 会丢精度，
 * 但容器大小被 `MAX_VAULT_BYTES = 200 MB` 兜住（≈2×10⁸，远低于 9×10¹⁵），实际不可达。
 * 若哪天放开体积上限，**必须**回头改这里（改用 string 传字节数），否则大库的体积数字会静默失真。
 */
export interface CompactionStatsDto {
  /** 容器文件当前字节数。 */
  file_bytes: number;
  /** 头部区字节数（压缩后不变）。 */
  header_bytes: number;
  /** 实际有效数据字节数（压缩后保留的部分）。 */
  live_bytes: number;
  /** 墓碑帧总字节数（死数据总量，含必须保留的少量墓碑）。 */
  tombstone_bytes: number;
  /** 其中必须保留的墓碑字节数。 */
  kept_tombstone_bytes: number;
  /** 压缩可回收字节数。 */
  reclaimable_bytes: number;
  /** 压缩后预计文件字节数。 */
  after_bytes: number;
  /** 已删 / 无引用附件密文字节数（回答「删掉的附件还在不在」）。 */
  attachment_dead_bytes: number;
  /** 活跃附件密文字节数。 */
  attachment_live_bytes: number;
  /** 保留帧数。 */
  live_frames: number;
  /** 将被丢弃的帧数。 */
  dead_frames: number;
}

/**
 * 压缩结果（D-76；镜像 Rust `dto.rs::CompactionResultDto`）。
 *
 * `u64` → `number` 的精度取舍同 `CompactionStatsDto` 的注释（被 `MAX_VAULT_BYTES` 兜住）。
 */
export interface CompactionResultDto {
  /** 压缩前文件字节数。 */
  before_bytes: number;
  /** 压缩后文件字节数。 */
  after_bytes: number;
  /** 实际回收字节数（`before_bytes - after_bytes`，恒 ≥ 0）。 */
  reclaimed_bytes: number;
  /** 丢弃的帧数。 */
  dropped_frames: number;
}

/**
 * 应用级配置（v1.4.10；镜像 Rust `dto.rs::PrefsDto`）。
 *
 * 与 `VaultSettings` 的区别：这份**不随保险库加密**，锁屏态/冷启动即可读写。
 */
export interface PrefsDto {
  /**
   * 当前平台是否具备系统集成能力（桌面 true / 移动端 false）。
   *
   * `false` 时设置卡**整块不渲染**——与 D-116 同一口径：还没有的功能不做广告。
   * 平台身份由后端自报，前端不做 UA 嗅探。
   */
  supported: boolean;
  /** 规范化后的热键串（`ctrl+alt+KeyP`）。**空串 = 用户主动禁用热键**。 */
  hotkey: string;
  /** 该热键此刻是否真的注册在系统里。`false` 且 `hotkey` 非空 = 注册失败（被占用）。 */
  hotkey_ok: boolean;
  /** 开机自启**系统实际状态**（读注册表，不是读我们的配置）。 */
  autostart: boolean;
}

/** IPC 错误 JSON（Rust `IpcError`）。 */

export interface IpcErrorShape {
  code: string;
  message_key: string;
  record_count?: number;
}
