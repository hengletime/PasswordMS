//! 全局常量（架构文档 §9.3）。Rust 与 TS 同名同值，改动须同步 `src/lib/constants.ts`。

// ===== 容器 / 协议版本 =====
/// 保险库文件魔数（8 字节 ASCII）。
pub const VAULT_MAGIC: &[u8; 8] = b"PMSVAULT";
/// 容器格式版本。
pub const FORMAT_VERSION: u16 = 1;
/// 顶层固定前缀长度：magic(8) + format_version(2) + header_len(2) + reserved(4)。
pub const CONTAINER_PREFIX_LEN: usize = 16;
/// 头部 JSON 预留容量（字节，含尾部空格填充）。固定容量使计数更新无需移动记录区。
pub const HEADER_RESERVED_BYTES: usize = 8192;
/// 记录帧固定头部长度（字节）。布局见 [`crate::store::frame`]。
pub const FRAME_HEADER_LEN: usize = 124;
/// 二维码配对协议版本。
pub const QR_PROTOCOL_VERSION: u32 = 1;
/// 二维码文本前缀。
pub const QR_PREFIX: &str = "PMS1:";

// ===== KDF（Argon2id，架构文档 §2.2）=====
pub const KDF_ALGORITHM: &str = "argon2id";
/// 内存开销 64 MiB。
pub const KDF_MEMORY_KIB: u32 = 65536;
/// 迭代次数。
pub const KDF_ITERATIONS: u32 = 3;
/// 并行度。
pub const KDF_PARALLELISM: u32 = 1;
/// 盐长度（字节）。
pub const KDF_SALT_LEN: usize = 16;
/// 派生输出长度（字节）= 主密钥长度。
pub const KDF_OUTPUT_LEN: usize = 32;

// ===== AEAD（XChaCha20-Poly1305，§2.3）=====
pub const ENCRYPTION_SCHEME: &str = "xchacha20poly1305";
/// 对称密钥长度。
pub const KEY_LEN: usize = 32;
/// XChaCha20-Poly1305 nonce 长度。
pub const AEAD_NONCE_LEN: usize = 24;
/// Poly1305 tag 长度。
pub const AEAD_TAG_LEN: usize = 16;
/// 包裹后的密钥密文长度 = 32B 密钥 + 16B tag。
pub const WRAPPED_KEY_CT_LEN: usize = KEY_LEN + AEAD_TAG_LEN;

// ===== HKDF info 标签 =====
pub const HKDF_INFO_KEK: &str = "passwordms/v1/kek";
pub const HKDF_INFO_VERIFY: &str = "passwordms/v1/verify";
pub const HKDF_INFO_SYNC_KEY: &str = "passwordms/sync/v1/key";
pub const HKDF_INFO_SYNC_CONFIRM: &str = "passwordms/sync/v1/confirm";
pub const HKDF_INFO_SYNC_SESSION: &str = "passwordms/sync/v1/session";

// ===== AAD 常量 =====
/// 根层 VK 包裹 AAD。
pub const AAD_VAULT_KEY: &[u8] = b"passwordms/v1/vault-key";
/// verify_mac 覆盖的固定上下文（与 HKDF_INFO_VERIFY 同值，语义不同故分别命名）。
pub const AAD_VERIFY: &[u8] = b"passwordms/v1/verify";

// ===== 记录类型 =====
pub const RECORD_TYPE_FOLDER: u8 = 0x01;
pub const RECORD_TYPE_ITEM: u8 = 0x02;
pub const RECORD_TYPE_TRASH: u8 = 0x03;
pub const RECORD_TYPE_CONFLICT: u8 = 0x04;
pub const RECORD_TYPE_SETTINGS: u8 = 0x05;
/// 已配对设备记录【偏差 D-3：架构 §2.6 记录类型表未列出，为持久化 §5.1 `PairedDevice` 新增】。
pub const RECORD_TYPE_DEVICE: u8 = 0x06;
/// 每设备同步状态记录【偏差 D-3：持久化 §5.1 `DeviceSyncState` 新增】。
pub const RECORD_TYPE_SYNC_STATE: u8 = 0x07;

// ===== 记录帧 flags =====
/// bit0：墓碑（已删除占位）。
pub const FLAG_TOMBSTONE: u8 = 0b0000_0001;
/// bit1：附件密文 blob 帧。
pub const FLAG_BLOB: u8 = 0b0000_0010;

// ===== 体积 / 容量上限 =====
/// 单附件上限 20 MB。
pub const MAX_ATTACHMENT_BYTES: u64 = 20 * 1024 * 1024;
/// 库总容量上限 200 MB。
pub const MAX_VAULT_BYTES: u64 = 200 * 1024 * 1024;
/// 安全笔记正文上限 256 KiB。
pub const MAX_NOTE_BYTES: usize = 256 * 1024;
/// 附件同步分片大小 1 MB【已裁定 U4：≤ 1 MB 分片】。
pub const ATTACHMENT_CHUNK_BYTES: usize = 1024 * 1024;

// ===== 会话 / 剪贴板 =====
/// 剪贴板自动清空秒数。
pub const CLIPBOARD_TTL_SECS: u16 = 30;
/// 自动锁默认分钟数（合法档位 1/5/15）。
pub const AUTO_LOCK_MINUTES: u16 = 5;
/// 自动锁合法档位。
pub const AUTO_LOCK_OPTIONS: [u16; 3] = [1, 5, 15];

// ===== 主密码（§U6 已裁定：< 8 位拦截）=====
pub const MIN_MASTER_PASSWORD_LEN: usize = 8;

// ===== 同步协议 =====
/// 配对令牌有效期（秒）。
pub const PAIR_TOKEN_TTL_SECS: u64 = 120;
/// 同步时间窗（秒，正负）。
pub const SYNC_TIME_WINDOW_SECS: i64 = 120;
/// 同步重试次数。
pub const SYNC_RETRY_ATTEMPTS: u32 = 3;
/// 同步重试退避（毫秒）。
pub const SYNC_RETRY_BACKOFF_MS: [u64; 3] = [1000, 2000, 4000];
/// 长度前缀帧的单帧上限（附件分片 + 余量）。
pub const SYNC_MAX_FRAME_BYTES: u32 = (ATTACHMENT_CHUNK_BYTES as u32) + 64 * 1024;
/// 同步默认监听端口范围下界。
pub const SYNC_PORT_MIN: u16 = 49152;
/// 同步默认监听端口范围上界。
pub const SYNC_PORT_MAX: u16 = 65535;

// ===== 生成器（§3.3 F-07）=====
pub const GENERATOR_MIN_LEN: u8 = 8;
pub const GENERATOR_MAX_LEN: u8 = 64;
pub const GENERATOR_DEFAULT_LEN: u8 = 16;
/// 易混字符集合（排除选项命中时移除）。
pub const AMBIGUOUS_CHARS: &str = "0O1lI";
pub const CHARSET_LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
pub const CHARSET_UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const CHARSET_DIGITS: &str = "0123456789";
/// 符号字符集（**D-49 精编**）：只保留在常见语境中**无语法含义**的标点，
/// 避免生成的密码被粘贴进命令行 / 代码 / URL 时被特殊解释。
///
/// 被排除的高危符号及其原因：
/// - `=@`：赋值 / 环境变量 / URL 查询串 / 装饰器（主人点名）
/// - `` " ' ` \ ``：字符串定界与转义
/// - `$ & | ; < > ( )`：shell 变量、管道、语句分隔、重定向、子命令
/// - `* ? [ ] { }`：glob 通配、大括号展开、格式化占位
/// - `# % ! / ^`：注释、批处理变量、历史展开、路径分隔、行锚 / 转义
///
/// 熵代价：池从 24 → 6（每符号位约少 2 bit），可由长度补偿；换取「任何场景可直贴」。
pub const CHARSET_SYMBOLS: &str = "+-_.,:";

// ===== 体检（§3.3 F-16）=====
pub const HEALTH_WEAK_MAX_LEN: u8 = 8;
pub const HEALTH_STALE_DAYS: u32 = 180;

// ===== 回收站 =====
/// 回收站保留天数。
pub const TRASH_RETENTION_DAYS: i64 = 30;
pub const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

// ===== TOTP（F-11）=====
pub const TOTP_PERIOD_SECS: u64 = 30;
pub const TOTP_DIGITS: u32 = 6;
