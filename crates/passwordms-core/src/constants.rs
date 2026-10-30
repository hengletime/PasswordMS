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
/// 头部区容量的硬上限（= `u16::MAX`，D-78）。
///
/// `header_len` 是 u16（容器前缀 `[10..12)`），**65536 写进去会被截断成 0**，而
/// [`crate::store::container::Container::open`] 把 `header_len == 0` 判为
/// `zero header` → **库永久打不开**（且坏 header 在报错前已落盘）。
///
/// 故 `update_header` 的容量翻倍必须夹在本值内；夹不住时**返回错误**，绝不用夹住后的
/// 容量继续写（那会把 header JSON 截掉一截，比报错糟得多）。
pub const MAX_HEADER_BYTES: usize = u16::MAX as usize;
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
/// KDF 内存上限 1 GiB【D-96】：头部 KDF 参数是**不可信输入**（库文件可被替换），
/// 恶意头部声明超大内存可让解锁路径一次分配炸掉进程（DoS）。夹在 1 GiB 与既有
/// 默认 64 MiB 之间——存量库零影响，正常调参仍有约 16 倍向上空间。
pub const KDF_MAX_MEMORY_KIB: u32 = 1_048_576;
/// KDF 迭代次数上限【D-96】（默认 3；恶意头部声明 t=1e9 会让解锁永久挂起）。
pub const KDF_MAX_ITERATIONS: u32 = 16;
/// KDF 并行度上限【D-96】（默认 1；Argon2 并行度直接乘内存与 CPU 占用）。
pub const KDF_MAX_PARALLELISM: u32 = 8;

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
/// Host 身份密钥文件（D-96 VK 包裹格式）的 AAD。
pub const AAD_IDENTITY: &[u8] = b"identity";
/// 身份密钥文件新格式魔数（10 字节：9 ASCII "PMSIDENT1" + 0x00 终止）【D-96】。
/// 旧格式 = Base64 裸私钥（无魔数，Base64 解码恒不以本串开头，判定无歧义）。
pub const IDENTITY_MAGIC: &[u8; 10] = b"PMSIDENT1\x00";

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
/// 单条目附件数上限（D-60 §3.14.1）：16 × ~200B 元数据 ≈ 3.2KB，远低于 1MB 同步帧上限。
pub const MAX_ATTACHMENTS_PER_ITEM: usize = 16;
/// 附件文件名最大字节数（D-60 §3.14.5，导入时超长拒绝）。
pub const MAX_ATTACHMENT_FILENAME_BYTES: usize = 255;
/// 安全笔记正文上限 256 KiB。
pub const MAX_NOTE_BYTES: usize = 256 * 1024;
/// 单条目**统一明文上限** 384 KiB【D-96】：`ItemKind` 序列化后的字节总数上限。
/// 覆盖登录 notes（此前无上限）等全部字段。换算关系：明文 384 KiB + AEAD tag 16 B
/// → 线上估算（paging.rs `estimate_record_size` = ciphertext×2 + 512）≈ 2×(384 KiB+16)
/// 再 + 512 ≈ 769 KiB < 单条超限阈值（`PAGE_BUDGET_BYTES` ≈ 992 KiB → ct ≈ 496 KiB）。
/// 故 384 KiB 的条目永远到不了「change_too_large」，同步链路不会因本上限而报错。
pub const MAX_ITEM_PLAINTEXT_BYTES: usize = 384 * 1024;
/// 附件同步分片大小 1 MB【已裁定 U4：≤ 1 MB 分片】。
pub const ATTACHMENT_CHUNK_BYTES: usize = 1024 * 1024;
/// 线上 blob 分片尺寸（D-53）：768 KiB 密文切片。
///
/// 推导（红线：`SYNC_MAX_FRAME_BYTES` 不放宽）：
/// `Base64(786,432) = 1,048,576 B`（恰 1 MiB，×4/3 精确整除）+ 信封开销
/// （session_id/seq/ts/nonce/字段名 + AEAD tag，实测 < 300 B）≈ 1,048,800 B
/// `< SYNC_MAX_FRAME_BYTES = 1,114,112 B`（余量 ≈ 64 KiB）。
/// 注意 blob 信封的线上形态**只有一次** Base64（切片以二进制进内层 pt，
/// 见 `sync/blob_transfer.rs`）；若走 JSON-in-JSON 双层 b64，768 KiB 实为
/// ~1.40 MB 必超限。与本地分片常量 [`ATTACHMENT_CHUNK_BYTES`]（1 MB）分离命名、
/// 分离取值，互不牵动。
pub const SYNC_BLOB_CHUNK_BYTES: usize = 768 * 1024;

// ===== 会话 / 剪贴板 =====
/// 剪贴板自动清空秒数。
pub const CLIPBOARD_TTL_SECS: u16 = 30;
/// 自动锁默认分钟数（合法档位 1/5/15）。
pub const AUTO_LOCK_MINUTES: u16 = 5;
/// 自动锁合法档位。
pub const AUTO_LOCK_OPTIONS: [u16; 3] = [1, 5, 15];

// ===== 导入（F-14）【D-96】=====
/// 导入源文件大小上限 64 MB：`std::fs::read` 前先 `metadata` 预检，超大文件不进内存。
pub const MAX_IMPORT_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// 单次导入条目数上限（解析后可落库条目数）：防止巨型 CSV 撑爆内存与库容量。
pub const MAX_IMPORT_ITEMS: usize = 10_000;

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
/// TCP 连接超时（秒）。配对（D-44）与重同步（D-57）共用：对 SYN 被静默丢弃的
/// 不可路由地址，系统级 connect 可挂约 21s（Android 更久），必须用短超时。
pub const SYNC_CONNECT_TIMEOUT_SECS: u64 = 4;
/// 长度前缀帧的单帧上限（附件分片 + 余量）。
pub const SYNC_MAX_FRAME_BYTES: u32 = (ATTACHMENT_CHUNK_BYTES as u32) + 64 * 1024;
/// 接收侧一次同步交换累计字节数上限【D-96】：等于库容量上限（[`MAX_VAULT_BYTES`]，
/// 200 MB）。正常分页总量 = 库内活跃数据 ≪ 本值；超限说明对端在发垃圾帧（恶意或
/// 协议失控），接收循环立即终止，防「无限 cursor 分页拖死接收端」。
pub const SYNC_MAX_RECV_TOTAL_BYTES: usize = MAX_VAULT_BYTES as usize;
/// 接收侧一次同步交换的最大页数【D-96】：按最小记录 ~512 B 计，4096 页可承载
/// ~4 GB 估算量（远超 200 MB 库上限），正常同步最多几十页。
pub const SYNC_MAX_RECV_PAGES: usize = 4096;
/// 同步默认监听端口范围下界。
pub const SYNC_PORT_MIN: u16 = 49152;
/// 同步默认监听端口范围上界。
pub const SYNC_PORT_MAX: u16 = 65535;

// ===== 使用频度（#531）=====
/// 同一条目两次计频的最小间隔（毫秒）。#531：「使用」= 仅复制动作，60 秒节流
/// 防止连续复制把 use_count 刷爆。`record_item_usage_at` 的节流判定用本值。
pub const USAGE_THROTTLE_MS: i64 = 60_000;

// ===== 局域网自动发现（D-74）=====
/// 发现协议使用的固定 UDP 端口。**双端必须一致**（编译期常量，不漂移）。
///
/// 刻意避开 [`SYNC_PORT_MIN`]..=[`SYNC_PORT_MAX`]：那是 **TCP** 重同步口（可因占用而
/// 漂移），这是 **UDP** 发现口（永不变）。两者语义不同，混用会让"端口漂移即发现失效"。
pub const DISCOVERY_PORT: u16 = 47823;

/// 广播后等待应答的时长（毫秒）。
///
/// v1.4.15（D-132）从 1.5s 提到 2.5s。原口径只考虑「200ms 轮询节拍下最多错过 7 个
/// 周期」，但真机实测暴露出两件没算进去的事：
/// 1. 应答方只有**解锁态**才轮询发现端口（锁屏时 `stop_resync_listener` 已停），
///    手机刚从锁屏抬起时第一个 200ms 周期不一定赶得上；
/// 2. 单轮广播在 WiFi 省电/AP 缓冲场景下会整体丢失，需要窗口内重播（见
///    `discovery::discover_peer_with`），窗口太短则重播打不满。
///
/// 代价是「发现失败 → 报错」总时长升到 4s + 2.5s ≈ 6.5s，仍在用户可接受量级，
/// 且只在静态地址已失败（换网、对端换 IP）这条兜底路径上发生。
pub const DISCOVERY_WAIT_MS: u64 = 2500;

/// 单个请求最多携带的 tag 数（超过截断，不报错）。
///
/// 请求方把**所有已配对设备**的 tag 都带上，一次广播即可找到任意一台对端；
/// 上限防止报文被恶意撑大（响应方同样按此截断）。
pub const DISCOVERY_MAX_TAGS: usize = 8;

/// 发现报文版本（`v` 字段）。不兼容变更时递增。
pub const DISCOVERY_VERSION: u16 = 1;

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
