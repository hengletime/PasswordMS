//! 本地滚动备份（v1.5.29 / B-03）。
//!
//! ## 要解决的问题
//!
//! 容器是**追加帧**格式，`compact()` 会丢弃旧帧。于是这条路径是真实存在的：
//! 误删条目 → 清空回收站 → 压缩 → **数据永久消失**，只剩用户上次手动导出的那份
//! （可能几个月前）。备份不是"锦上添花"，是这条路径唯一的保险。
//!
//! ## 三种备份文件，职责单一（不要合并成一套轮转规则）
//!
//! | 文件 | 何时生成 | 作用 | 保留 |
//! |---|---|---|---|
//! | `bak.<yyyyMMdd>` | 当天**首次**写入前 | 「回到今天开始时」 | 最近 5 天 |
//! | `last.bak` | 每次写入前（大库节流） | 「刚才那次改错了，退回去」 | 1 份 |
//! | `<event>.<epoch-ms>` | 恢复 / 换主密码 / 压缩 / 导入 apply 前 | 破坏性操作前的回头路 | 共 2 份 |
//!
//! 为什么不能合并：这几种场景的时间粒度差两个数量级。用一套「按次数轮转」的规则，
//! 一天保存 20 次的活跃用户会让「几天后才发现误删」完全没有依靠；反过来按天分桶又
//! 让「刚才改错了」无从回退。拆开之后每种场景各自精确命中。
//!
//! 事件快照**独立槽位**（共 2 份，不占日备份的 5 个）——连续恢复三次不会把日备份挤空，
//! 而日备份恰是「几天后才发现」的唯一依靠。
//!
//! ## 三条硬约束（来自安全审计，改动前先想清楚）
//!
//! 1. **失败不阻断保存，但绝不静默**：备份是旁路，写不进去也要让用户改得动数据；
//!    但失败必须变成可见警告（[`BackupOutcome::error`]），否则用户以为有备份。
//! 2. **不留半成品**：先写 `.tmp`、校验长度与源一致，再 `rename` 到最终名。
//! 3. **恢复期间暂停轮转**（[`RestoreGuard`]）：靠调用方「记得别删」守不住，
//!    所以把状态放在本模块内部，由 RAII 守卫维护。
//!
//! ## 日期为什么是 UTC
//!
//! 文件名里的 `yyyyMMdd` 用 **UTC 日界**（core 不引入时区库）。代价是东八区早上 8 点前的
//! 备份会归到前一天；影响仅限于"最近 5 天"实际覆盖 4~5 天，无功能损失。
//! **界面一律按文件 mtime 的本地时间显示**（前端有 `Date`），所以用户看到的日期是对的，
//! 只有磁盘上的文件名按 UTC 分组。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::error::CoreResult;

/// 日备份保留份数。
pub const DAILY_KEEP: usize = 5;
/// `pre-restore` 保留份数。
///
/// **至少 2 份**：恢复错了往往要再恢复一次，而「第二次恢复之前」的状态正是第一次的
/// `pre-restore`。只留 1 份会被第二次覆盖——用户想退回「恢复之前」时发现退路没了。
/// 这是恢复功能最常见的误用路径，也是这个常量不跟其它事件一样的唯一原因。
pub const PRE_RESTORE_KEEP: usize = 2;
/// 其它事件快照（换主密码 / 压缩 / 导入）**每种**保留 1 份。
///
/// 这几种事件不会「连着做两次还想要中间态」：换密码只有最后一次有意义，
/// 压缩/导入同理。
pub const EVENT_KEEP_PER_TAG: usize = 1;
/// 备份根目录名（与保险库同目录下的子目录）。
pub const BACKUP_DIR_NAME: &str = "backups";
/// 换主密码后旧备份的隔离目录名（`backups/old-key/<epoch-ms>/`）。
pub const OLD_KEY_DIR_NAME: &str = "old-key";
/// 「大库」阈值：超过它，`last.bak` 改走时间节流而不是每次写都复制。
///
/// 4 MiB 是产品侧定的分界：典型库（几百 KB~几 MB）每次写复制无感；24 MB 以上的库
/// 每写一次复制 24 MB 会变成可感知的卡顿，此时退化为「最多每 60 秒一份」。
pub const LARGE_VAULT_BYTES: u64 = 4 * 1024 * 1024;
/// 大库 `last.bak` 的最小刷新间隔。
pub const LARGE_VAULT_MIN_INTERVAL_MS: i64 = 60_000;
/// 日备份的最小间隔（跨零点保护：23:59 与 00:01 相差两分钟，不该产生两份"日备份"）。
pub const DAILY_MIN_INTERVAL_MS: i64 = 60 * 60 * 1000;
/// 备份总占用相对库体积的倍数（配额）。
///
/// 12 = 日备份 5 + `last.bak` 1 + `pre-restore` 2 + 其余事件 3 + 1 份余量给"正在写的新快照"。
/// 按槽位算出来的理论上限本就是 11 份，12 倍是给"库刚变小、旧备份还很大"留的一口气。
pub const QUOTA_MULTIPLIER: u64 = 12;
/// 备份总占用的绝对上限（1 GB）。
///
/// 为什么要封顶：容量大的库（带附件，200 MB 级）按 12 倍算就是 2.4 GB，备份目录会变成
/// 磁盘上最大的东西，而它保护的只是同一份数据的历史副本。1 GB 足够覆盖绝大多数人的
/// 「回到几天前」，也保证 12 倍的倍数不会失控。
pub const QUOTA_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// 事件快照的四种触发点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventTag {
    /// 恢复备份之前（整库替换，最危险的动作）。
    PreRestore,
    /// 换主密码成功之后（这份用**新**主密码可开，是用户忘了新密码时的退路）。
    PostRekey,
    /// 压缩之前（会丢弃旧帧）。
    PreCompact,
    /// 导入 apply 之前（整批写入）。
    PreImport,
}

impl EventTag {
    /// 文件名前缀。
    pub fn prefix(self) -> &'static str {
        match self {
            EventTag::PreRestore => "pre-restore",
            EventTag::PostRekey => "post-rekey",
            EventTag::PreCompact => "pre-compact",
            EventTag::PreImport => "pre-import",
        }
    }
}

/// 备份被跳过的原因（**只含正常跳过**，全部不上面向用户的界面）。
///
/// ## 为什么配额不在这里
///
/// 这个枚举的语义是"这次没做，但一切正常"（今天已经有了 / 刚备份过 / 正在恢复）。
/// 配额装不下则是**备份停了**——它跟 `DailyExists` 完全不是一类事，混进来早晚有人
/// 把 `skipped == QuotaExceeded` 渲染成灰字。所以配额走
/// [`BackupErrorKind::QuotaExceeded`] 的失败通道，见 [`BackupErrorKind`]。
///
/// （v1.5.29 早期版本曾把 `QuotaExceeded` 放在这里，已推翻。）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// 今天已经有日备份了。
    DailyExists,
    /// 距上一份日备份不足 1 小时（跨零点保护）。
    DailyTooSoon,
    /// 大库且距上次 `last.bak` 不足 60 秒。
    Throttled,
    /// 正在恢复，整体暂停。
    Restoring,
    /// 源文件不存在（首次创建库时）。
    ///
    /// **必须保留**：它决定恢复入口要不要放行，见 [`restore_safety`] 与
    /// [`RestoreSafety::NothingToLose`]。`last.bak` 暂时丢了可以用那份独占的备份恢复；
    /// 库文件整个不见了就**没有"当前状态"可失去**——那正是最需要恢复的时刻。
    NoSource,
}

/// 备份失败的原因（**可本地化枚举**）。
///
/// ## 为什么不能直接把 `io::Error` 的文本给界面
///
/// `io::Error` 的 Display 跟**系统语言**走：泰语系统上给泰语、中文系统上给中文。我们有四套
/// 界面语言，用户可能在英文系统上使用中文界面，于是同一条失败会显示成"没人看得懂的语言"。
/// 而且 `fs::copy` 的错误文本在部分平台带**绝对路径**，与「路径不回传前端」的约定冲突。
///
/// 所以：枚举进 IPC 由前端 i18n 映射文案，原始 `io::Error` 只进日志。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupErrorKind {
    /// 磁盘空间不足。
    NoSpace,
    /// 目标只读（文件系统只读 / 介质写保护）。
    ReadOnly,
    /// 权限不足（目录属主变了、被 ACL 挡住）。
    Permission,
    /// 设备没了（U 盘/移动硬盘被拔、外置盘休眠掉线）。
    DeviceGone,
    /// 配额装不下：已裁到保底，再加这一份就超上限。
    ///
    /// **不是"正常跳过"**：它意味着备份从这个时刻起停了，必须进常驻警告。
    QuotaExceeded,
    /// 其它 IO 失败。
    Io,
    /// 认不出来（也是 `.state` 里遇到未知值时的回落）。
    Other,
}

impl BackupErrorKind {
    /// `.state` 里存的单 token（**进了磁盘就是格式，加值可以、改既有语义不行**）。
    pub fn as_str(self) -> &'static str {
        match self {
            BackupErrorKind::NoSpace => "no_space",
            BackupErrorKind::ReadOnly => "readonly",
            BackupErrorKind::Permission => "permission",
            BackupErrorKind::DeviceGone => "device_gone",
            BackupErrorKind::QuotaExceeded => "quota_exceeded",
            BackupErrorKind::Io => "io",
            BackupErrorKind::Other => "other",
        }
    }

    /// 解析 `.state` 里的 token；认不出回落 [`BackupErrorKind::Other`]。
    fn from_token(token: &str) -> Self {
        match token {
            "no_space" => BackupErrorKind::NoSpace,
            "readonly" => BackupErrorKind::ReadOnly,
            "permission" => BackupErrorKind::Permission,
            "device_gone" => BackupErrorKind::DeviceGone,
            "quota_exceeded" => BackupErrorKind::QuotaExceeded,
            "io" => BackupErrorKind::Io,
            _ => BackupErrorKind::Other,
        }
    }
}

/// `ErrorKind` 取不到的细分原因，按平台用 `raw_os_error()` 兜底。
///
/// `ErrorKind::StorageFull` / `ReadOnlyFilesystem` 在项目锁定的 Rust 版本上还没稳定，
/// 所以只能靠 errno / Win32 错误码。**必须按平台分开写**：同一个数字在两个平台含义不同
/// （Windows 112 是磁盘满，Linux 112 是 EHOSTDOWN）。
#[cfg(windows)]
fn classify_raw_os(code: i32) -> Option<BackupErrorKind> {
    match code {
        39 | 112 => Some(BackupErrorKind::NoSpace), // ERROR_HANDLE_DISK_FULL / ERROR_DISK_FULL
        19 => Some(BackupErrorKind::ReadOnly),      // ERROR_WRITE_PROTECT
        5 | 65 => Some(BackupErrorKind::Permission), // ERROR_ACCESS_DENIED / NETWORK_ACCESS_DENIED
        21 | 53 | 55 | 1168 => Some(BackupErrorKind::DeviceGone), // NOT_READY / BAD_NETPATH / DEV_NOT_EXIST / NOT_FOUND
        _ => None,
    }
}

#[cfg(unix)]
fn classify_raw_os(code: i32) -> Option<BackupErrorKind> {
    match code {
        28 | 122 | 69 => Some(BackupErrorKind::NoSpace), // ENOSPC / EDQUOT(Linux) / EDQUOT(macOS)
        30 => Some(BackupErrorKind::ReadOnly),           // EROFS
        1 | 13 => Some(BackupErrorKind::Permission),     // EPERM / EACCES
        5 | 6 | 19 => Some(BackupErrorKind::DeviceGone), // EIO / ENXIO / ENODEV
        _ => None,
    }
}

#[cfg(not(any(windows, unix)))]
fn classify_raw_os(_code: i32) -> Option<BackupErrorKind> {
    None
}

/// 把一个 `io::Error` 归类成可本地化的原因。
fn classify_io(e: &std::io::Error) -> BackupErrorKind {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::PermissionDenied => return BackupErrorKind::Permission,
        ErrorKind::NotFound => return BackupErrorKind::DeviceGone,
        _ => {}
    }
    e.raw_os_error()
        .and_then(classify_raw_os)
        .unwrap_or(BackupErrorKind::Io)
}

/// 一次备份尝试的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupOutcome {
    /// 本次新建的备份文件。
    pub created: Vec<PathBuf>,
    /// 被跳过时的原因。
    ///
    /// **到不了前端**：`#[serde(skip)]` 在结构上封住，不只是靠纪律。语义分工是
    /// [`SkipReason`] 给代码（日志/测试用），[`BackupErrorKind`] 给用户。
    #[serde(skip)]
    pub skipped: Option<SkipReason>,
    /// 轮转删掉的文件。
    pub pruned: Vec<PathBuf>,
    /// 失败原因（**非空时必须让用户看见**，不得静默）。前端按枚举映射四语言文案。
    pub error_kind: Option<BackupErrorKind>,
    /// 失败环节（`dir` / `daily` / `last` / 事件前缀），只作诊断区分。
    pub error_stage: Option<String>,
    /// 原始 `io::Error` 文本。
    ///
    /// **只进日志，绝不进 IPC**：它跟系统语言、且可能带绝对路径。`skip` 从结构上封住
    /// "有人在界面里顺手把它渲染出去"这条路。
    #[serde(skip)]
    pub error: Option<String>,
}

impl BackupOutcome {
    /// 记录一次失败的原因与环节（第一条生效，后来的不覆盖）。
    ///
    /// 只留第一条：一次写入里多个环节可能连环失败，**根因通常是第一条**，后面的多是它的
    /// 后果。界面报"磁盘空间不足"比报"某个 rename 失败"有用得多。
    fn fail_kind(&mut self, kind: BackupErrorKind, stage: &str) {
        if self.error_kind.is_none() {
            self.error_kind = Some(kind);
            self.error_stage = Some(stage.to_string());
        }
    }

    /// 记录一次 IO 失败（原因归类 + 环节 + 原始文本留日志）。
    fn fail(&mut self, stage: &str, e: &std::io::Error) {
        self.fail_kind(classify_io(e), stage);
        self.error = Some(format!("{stage}: {e}"));
    }
}

/// 备份文件分类（界面按类分组展示，不合并计数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupClass {
    Daily,
    Last,
    Event,
    /// 换主密码之前留下的旧备份（`backups/old-key/<epoch-ms>/`）。
    ///
    /// 单独一类而不并进上面三种：它们**只能用旧主密码打开**，界面必须分开讲，
    /// 否则用户会拿新旧混在一列里，以为换过密码就都统一了。
    OldKey,
}

/// 一个已存在的备份文件。
///
/// ## 为什么分类与排序**只用文件名**、绝不用 mtime
///
/// `std::fs::copy` 会把源文件的修改时间一起复制（Windows `CopyFileW` 必带；Unix 侧
/// Rust 只保证权限位）。我们用 `fs::copy` 造备份，于是**每份备份的 mtime 都等于源库的
/// mtime**——拿它排序会让所有备份并列、拿它判"最近"会全体误判。这个坑在 Windows 上
/// 必现、在 Linux 上可能不复现，是最难查的一类。
///
/// 因此：`created_ms` 一律从**文件名**解析（`bak.<yyyyMMdd>` 解析成该日 0 点，
/// `<tag>.<epoch-ms>` 解析成 epoch），排序、轮转、"最新一份"全部只看它。
/// `modified_ms` 仅作展示兜底，不参与任何判断。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupFile {
    pub path: PathBuf,
    pub class: BackupClass,
    /// 事件快照的具体类型（`Daily` / `Last` 时为 `None`）。
    pub event: Option<EventTag>,
    pub size_bytes: u64,
    /// 由**文件名**解析出的创建时刻（毫秒）。`last.bak` 无法从名字得知，回退为文件 mtime
    /// ——它不参与排序（恒排最后），只用于界面显示"更新于多久前"。
    pub created_ms: i64,
    /// 文件系统 mtime。仅展示兜底，**不用于任何判断**。
    pub modified_ms: i64,
}

/// 从文件名解析创建时刻；认不出返回 `None`。
fn created_ms_from_name(name: &str) -> Option<i64> {
    // bak.YYYYMMDD → 该日 00:00:00 UTC
    if let Some(day) = name.strip_prefix("bak.") {
        if day.len() == 8 && day.chars().all(|c| c.is_ascii_digit()) {
            let y: i64 = day[0..4].parse().ok()?;
            let m: i64 = day[4..6].parse().ok()?;
            let d: i64 = day[6..8].parse().ok()?;
            let days = days_from_civil(y, m as u32, d as u32);
            return Some(days * 86_400_000);
        }
        return None;
    }
    // <tag>.<epoch-ms>
    let (_, tail) = name.rsplit_once('.')?;
    tail.parse::<i64>().ok()
}

/// `civil_from_days` 的逆运算（年月日 → 1970-01-01 起的天数）。
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

// ── 恢复期暂停（core 内部状态，不依赖调用方自觉）──────────────────────────

fn restoring_registry() -> &'static Mutex<Vec<PathBuf>> {
    static REG: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(Vec::new()))
}

fn is_restoring() -> bool {
    restoring_registry()
        .lock()
        .map(|g| !g.is_empty())
        .unwrap_or(false)
}

/// 恢复期守卫：构造时暂停全部轮转与快照，`drop` 时恢复。
///
/// 为什么需要它：恢复备份要先读某个备份文件、再整体替换库文件。如果这期间有别的
/// 线程触发轮转，可能把**正在被恢复的那一份**删掉——恢复到一半源文件没了。
/// 而"让调用方记得别删"这种约定守不住（本项目在此类约定上已经踩过坑）。
pub struct RestoreGuard {
    path: PathBuf,
}

impl RestoreGuard {
    /// 进入恢复期。同一路径可重入（嵌套调用只记一次）。
    pub fn new(vault_path: &Path) -> Self {
        let path = canonical_key(vault_path);
        if let Ok(mut g) = restoring_registry().lock() {
            if !g.contains(&path) {
                g.push(path.clone());
            }
        }
        Self { path }
    }
}

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = restoring_registry().lock() {
            g.retain(|p| p != &self.path);
        }
    }
}

// ── 路径与日期 ──────────────────────────────────────────────────────────

/// 备份目录（与保险库同目录的 `backups/`）。
pub fn backup_dir(vault_path: &Path) -> PathBuf {
    vault_path
        .parent()
        .map(|p| p.join(BACKUP_DIR_NAME))
        .unwrap_or_else(|| PathBuf::from(BACKUP_DIR_NAME))
}

/// 旧密钥备份的隔离根目录（`backups/old-key/`）。
pub fn old_key_dir(vault_path: &Path) -> PathBuf {
    backup_dir(vault_path).join(OLD_KEY_DIR_NAME)
}

fn canonical_key(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// 由毫秒时间戳算 `yyyyMMdd`（**UTC 日界**，见模块文档）。
pub fn utc_date_key(now_ms: i64) -> String {
    let days = now_ms.div_euclid(86_400_000);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}")
}

/// Howard Hinnant 的 `civil_from_days`（1970-01-01 起的天数 → 年月日）。
///
/// 自己算而不引时区库：只需要一个纯函数的日期换算，引入 `chrono` 为这点需求增加
/// 一棵依赖树不值。算法本身是公开的常量公式，下面有覆盖闰年/世纪年的测试。
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ── 文件操作 ────────────────────────────────────────────────────────────

/// 原子复制：`.tmp` → fsync → 校验长度 → `rename`。
///
/// `fs::copy` 只有在目标不存在时才安全（它内部会 truncate 已有文件，中途崩溃留下半成品）。
/// 所以先写临时名，长度对上了才改名——rename 在同一目录内是原子的。
///
/// **必须 fsync 再 rename**：`rename` 只保证「目录项换名」这一动作的原子性，不保证数据
/// 已经落盘。若掉电发生在 rename 之后、数据块回写之前，重启后会得到一份「名字对、内容
/// 是空的」的备份——比没有备份更糟（用户以为有）。备份是灾难恢复的唯一依靠，这个
/// 顺序不能省。
fn copy_atomic(src: &Path, dest: &Path) -> std::io::Result<()> {
    let tmp = dest.with_extension(format!(
        "{}.tmp",
        dest.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    let _ = fs::remove_file(&tmp);

    fs::copy(src, &tmp)?;

    // 长度校验放在 fsync 之前：先确认搬全了，再花代价刷盘。
    let src_len = fs::metadata(src)?.len();
    let tmp_len = fs::metadata(&tmp)?.len();
    if src_len != tmp_len {
        let _ = fs::remove_file(&tmp);
        return Err(std::io::Error::other(format!(
            "backup size mismatch: src={src_len} tmp={tmp_len}"
        )));
    }
    // 以可写句柄打开再 sync_all（只读句柄在部分平台 sync 会失败）。
    {
        let handle = fs::OpenOptions::new().write(true).open(&tmp)?;
        handle.sync_all()?;
    }

    tighten_permissions(&tmp);
    // rename 失败（目标被同名的目录/只读文件占住、跨卷等）时必须清掉 tmp，
    // 否则每失败一次就在备份目录里留一份整库大小的垃圾文件，而它永远不会被轮转看到。
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn mtime_ms(path: &Path) -> Option<i64> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let dur = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(dur.as_millis() as i64)
}

/// 列出备份目录内的全部备份文件（按 mtime 升序）。
pub fn inventory(vault_path: &Path) -> Vec<BackupFile> {
    let dir = backup_dir(vault_path);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `.tmp` 是正在写的半成品，不列给用户看。
        if std::path::Path::new(name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("tmp"))
        {
            continue;
        }
        let created_ms = created_ms_from_name(name).unwrap_or_else(|| mtime_ms(&path).unwrap_or(0));
        let modified_ms = mtime_ms(&path).unwrap_or(0);
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let class = if name == "last.bak" {
            BackupClass::Last
        } else if name.starts_with("bak.") {
            BackupClass::Daily
        } else {
            let tag = [
                EventTag::PreRestore,
                EventTag::PostRekey,
                EventTag::PreCompact,
                EventTag::PreImport,
            ]
            .into_iter()
            .find(|t| name.starts_with(t.prefix()));
            match tag {
                Some(t) => {
                    out.push(BackupFile {
                        path: path.clone(),
                        class: BackupClass::Event,
                        event: Some(t),
                        size_bytes,
                        created_ms,
                        modified_ms,
                    });
                    continue;
                }
                None => continue,
            }
        };
        out.push(BackupFile {
            path: path.clone(),
            class,
            event: None,
            size_bytes,
            created_ms,
            modified_ms,
        });
    }
    out.sort_by_key(|f| f.created_ms);
    out
}

/// 备份占用总字节与文件数（设置卡用来报「占了多少」）。
pub fn usage(vault_path: &Path) -> (u64, usize) {
    let files = inventory(vault_path);
    (files.iter().map(|f| f.size_bytes).sum(), files.len())
}

/// 旧密钥备份的占用与份数（**与 [`usage`] 分开报**，不合并）。
///
/// 合并会让「删除全部备份」之后显示归零、而磁盘上还躺着一整份能用旧密码打开的库。
/// 界面必须两栏分开，用户才知道"这一批不是当前主密码的备份"。
pub fn old_key_usage(vault_path: &Path) -> (u64, usize) {
    let files = old_key_inventory(vault_path);
    (files.iter().map(|f| f.size_bytes).sum(), files.len())
}

fn parse_event_tag(name: &str) -> Option<EventTag> {
    [
        EventTag::PreRestore,
        EventTag::PostRekey,
        EventTag::PreCompact,
        EventTag::PreImport,
    ]
    .into_iter()
    .find(|t| name.starts_with(t.prefix()))
}

/// 列出旧密钥隔离区里的备份（按创建时刻升序）。
///
/// 与 [`inventory`] 严格分开：`inventory` 只扫**当前代**顶层，绝不递归进 `old-key/`。
/// 分开的理由不是洁癖——`prune()` 与 `clear_all()` 都建立在 `inventory` 之上，
/// 一旦递归，配额裁剪和"清空备份"就会去动旧代，那是不可逆的。
pub fn old_key_inventory(vault_path: &Path) -> Vec<BackupFile> {
    let root = old_key_dir(vault_path);
    let Ok(batches) = fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for batch in batches.flatten() {
        let batch_path = batch.path();
        if !batch_path.is_dir() {
            continue;
        }
        let Ok(items) = fs::read_dir(&batch_path) else {
            continue;
        };
        for entry in items.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name == STATE_FILE {
                continue;
            }
            if std::path::Path::new(name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("tmp"))
            {
                continue;
            }
            let created_ms =
                created_ms_from_name(name).unwrap_or_else(|| mtime_ms(&path).unwrap_or(0));
            out.push(BackupFile {
                path: path.clone(),
                class: BackupClass::OldKey,
                event: parse_event_tag(name),
                size_bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
                created_ms,
                modified_ms: mtime_ms(&path).unwrap_or(0),
            });
        }
    }
    out.sort_by_key(|f| f.created_ms);
    out
}

/// 换主密码后隔离旧备份的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IsolateOutcome {
    /// 挪走的备份份数。
    pub moved: usize,
    /// 隔离目录（没挪任何东西时为 `None`，不留空目录）。
    pub dir: Option<PathBuf>,
    /// 失败原因。**非空不代表换密失败**——换密已经成功了，只是旧备份没挪走。
    pub error: Option<String>,
}

/// 换主密码成功后：把既有备份与 `.state` 整体挪进 `backups/old-key/<epoch-ms>/`。
///
/// ## 为什么必须隔离
///
/// 换主密码只重包裹 VK，**不重写**容器里的密文——所以旧备份并没有作废：用**旧主密码**
/// 仍然能打开它们，里面是完整的条目明文。危险在两处：
///
/// 1. **用户的心理假设是错的**："换了密码，旧的就失效了"。实际上攻击者拿走旧备份 +
///    旧密码即得到全部数据，而"旧密码已经没用了"正是用户换密码的主要动机。
/// 2. **被怀疑的那把钥匙一直留着**：换密码常因为怀疑泄露。旧备份躺在原处，且界面把它
///    算作"我的备份"，用户不会知道它属于哪一代。
///
/// 隔离不是删除（删了就没有回头路，而换错密码时旧备份是唯一救命的），是**分区**：
/// 挪进子目录、单独计量、界面单独归类、文案说明"这批要用旧主密码开"。
///
/// ## 为什么带时间戳子目录
///
/// 换两次密码就有两批旧备份，密钥各不相同。混在一个目录里无法判断哪份对应哪把钥匙；
/// 按时刻分目录后，每一批天然对应一次换密动作。
///
/// ## 调用时机
///
/// 必须在**换密成功之后、写 `post-rekey` 快照之前**：`post-rekey` 那份是用新材料写的，
/// 属于"新一代"，不能被挪进 `old-key/`。
pub fn isolate_old_key_backups(vault_path: &Path, now_ms: i64) -> IsolateOutcome {
    let mut out = IsolateOutcome::default();
    let dir = backup_dir(vault_path);
    let files = inventory(vault_path);
    let state_path = dir.join(STATE_FILE);
    if files.is_empty() && !state_path.is_file() {
        // 没有任何旧备份：不留空目录，也别让界面出现一个空分组。
        return out;
    }

    let dest_dir = old_key_dir(vault_path).join(now_ms.to_string());
    if let Err(e) = fs::create_dir_all(&dest_dir) {
        out.error = Some(format!("old-key dir: {e}"));
        return out;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dest_dir, fs::Permissions::from_mode(0o700));
    }

    let mut moved = 0usize;
    for f in files {
        let Some(name) = f.path.file_name() else {
            continue;
        };
        if fs::rename(&f.path, dest_dir.join(name)).is_ok() {
            moved += 1;
        }
    }
    out.moved = moved;
    // `.state` **删掉而不是跟着走**：它只是当前代的节流/健康缓存，旧代已经不会再被写入，
    // 留着没有任何用处；而留着会让批次目录永远非空，[`clear_old_key`] 清不干净。
    // 删掉同时达到目的：新库读不到旧库的"上次备份"和那条早已无关的失败警告。
    if state_path.is_file() {
        let _ = fs::remove_file(&state_path);
    }

    if out.moved > 0 {
        out.dir = Some(dest_dir);
    } else {
        let _ = fs::remove_dir(&dest_dir);
    }
    out
}

/// 删除**旧密钥代**的备份（独立动作，不复用「删除全部备份」）。
///
/// 为什么不合并：旧代可能是用户唯一还记得的那把密码的退路。把它并进「清空备份」，
/// 用户点一次就永久失去那一代——单向门，不能藏在同一个按钮里。
pub fn clear_old_key(vault_path: &Path) -> CoreResult<usize> {
    let mut removed = 0;
    for f in old_key_inventory(vault_path) {
        if fs::remove_file(&f.path).is_ok() {
            removed += 1;
        }
    }
    // 顺手清掉空批次目录，别在设置页留一堆空文件夹（非空会失败，忽略即可）。
    if let Ok(batches) = fs::read_dir(old_key_dir(vault_path)) {
        for batch in batches.flatten() {
            let _ = fs::remove_dir(batch.path());
        }
    }
    let _ = fs::remove_dir(old_key_dir(vault_path));
    Ok(removed)
}

/// 轮转：日备份留 [`DAILY_KEEP`] 份；`pre-restore` 留 [`PRE_RESTORE_KEEP`] 份、
/// 其余事件**每种**留 [`EVENT_KEEP_PER_TAG`] 份。
///
/// 只删本模块认识的命名（`bak.*` / 事件前缀），**绝不删目录里的其它文件**——
/// 用户若把别的东西放进 `backups/`，那不是我们的。
fn prune(vault_path: &Path) -> Vec<PathBuf> {
    let files = inventory(vault_path);
    let mut removed = Vec::new();

    // incoming 已按 mtime 升序（inventory 排过）→ 删头部（最旧）的那几份。
    let mut daily: Vec<&BackupFile> = files
        .iter()
        .filter(|f| f.class == BackupClass::Daily)
        .collect();
    if daily.len() > DAILY_KEEP {
        let cut = daily.len() - DAILY_KEEP;
        for f in daily.drain(..cut) {
            if fs::remove_file(&f.path).is_ok() {
                removed.push(f.path.clone());
            }
        }
    }

    // 事件快照**按类型**分别计数：`pre-restore` 2 份，其余各 1 份。
    // 混在一起算总数会让「连续恢复两次」把 `pre-compact` 挤掉——那是另一件事的退路。
    for tag in [
        EventTag::PreRestore,
        EventTag::PostRekey,
        EventTag::PreCompact,
        EventTag::PreImport,
    ] {
        let keep = if tag == EventTag::PreRestore {
            PRE_RESTORE_KEEP
        } else {
            EVENT_KEEP_PER_TAG
        };
        let mut list: Vec<&BackupFile> = files.iter().filter(|f| f.event == Some(tag)).collect();
        if list.len() > keep {
            let cut = list.len() - keep;
            for f in list.drain(..cut) {
                if fs::remove_file(&f.path).is_ok() {
                    removed.push(f.path.clone());
                }
            }
        }
    }

    removed
}

fn prepare_dir(vault_path: &Path) -> std::io::Result<PathBuf> {
    let dir = backup_dir(vault_path);
    fs::create_dir_all(&dir)?;
    // 备份里是**整个保险库的密文副本**。在多账户 Unix 机器上，`create_dir_all` 的默认
    // 权限（0777 & ~umask，常见 0755）会让同机其它账户能列目录、读文件——虽然内容是
    // 加密的，但仍泄露「库多大」「多久改一次」，且给离线爆破递了一份现成副本。
    // 本应用自己的数据目录已经是私有的，备份目录按同样口径收紧（0700，只属主）。
    // Windows 侧靠 ACL 继承（数据目录本就在 `%APPDATA%` 私有树内），无需额外处理。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    Ok(dir)
}

/// 把一份备份的权限收紧到「只属主可读」（跨平台尽力而为）。
///
/// Windows 上 `copy_atomic` 复制出的文件会继承源文件的 ACL，而源库本身已在私有目录，
/// 所以不额外处理；Unix 上 `fs::copy` 只保证权限位复制，源若曾被人改成 0644，备份也会
/// 是 0644 —— 这里统一收紧一次。
fn tighten_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

// ── 配额（C-45）──────────────────────────────────────────────────────────

/// 由库体积算备份总配额（字节）：`min(体积 × 12, 1 GB)`。
pub fn quota_bytes(vault_size: u64) -> u64 {
    vault_size
        .saturating_mul(QUOTA_MULTIPLIER)
        .min(QUOTA_MAX_BYTES)
}

/// 当前库对应的备份配额（库不存在时为 0）。
pub fn quota_for(vault_path: &Path) -> u64 {
    quota_bytes(fs::metadata(vault_path).map(|m| m.len()).unwrap_or(0))
}

/// 在**创建新快照之前**腾地方。
///
/// ## 为什么不"先建再删"
///
/// 配额的意义是「备份目录不至于吃满磁盘」。若先写新快照再裁剪，峰值占用已经是
/// `配额 + 一份库体积`；而配额恰恰会在磁盘快满时才有存在感——那时多写一份大备份
/// 可能直接失败，甚至把磁盘写满拖垮正在保存的库。所以顺序必须是先算账、够地方才动手。
///
/// ## 裁不动就一份都不删
///
/// 两趟走：先在内存里试算「删到哪一份才够」，够就照单执行；不够就**原样返回 Err**，
/// 磁盘上什么都不动。否则会出现「删了 4 份旧备份、最后还是不够、于是新备份也没建」——
/// 白丢退路。
///
/// ## 裁剪顺序与保底
///
/// 由旧到新：日备份 → 一般事件快照（`post-rekey` / `pre-compact` / `pre-import`）
/// → `pre-restore`。保底三样永不因配额被删：
///
/// * `last.bak`（"刚才改错了"的唯一退路，且只有 1 份）；
/// * 最新 1 份日备份（"回到今天开始时"）；
/// * 最新 1 份 `pre-restore`（恢复错了要能再恢复一次）。
///
/// `incoming_bytes` 传**净增量**（源体积减去将被覆盖的同名文件体积），稳态口径，
/// 不计写 `.tmp` 期间的瞬时峰值。
fn make_room(vault_path: &Path, incoming_bytes: u64) -> Result<Vec<PathBuf>, BackupErrorKind> {
    let quota = quota_for(vault_path);
    let files = inventory(vault_path);
    let used: u64 = files.iter().map(|f| f.size_bytes).sum();
    if used.saturating_add(incoming_bytes) <= quota {
        return Ok(Vec::new());
    }

    // inventory 已按 created_ms 升序 → 从头取就是由旧到新。
    let mut candidates: Vec<&BackupFile> = Vec::new();
    let daily: Vec<&BackupFile> = files
        .iter()
        .filter(|f| f.class == BackupClass::Daily)
        .collect();
    if daily.len() > 1 {
        candidates.extend(daily[..daily.len() - 1].iter().copied());
    }
    candidates.extend(
        files
            .iter()
            .filter(|f| f.class == BackupClass::Event && f.event != Some(EventTag::PreRestore)),
    );
    let pre_restore: Vec<&BackupFile> = files
        .iter()
        .filter(|f| f.event == Some(EventTag::PreRestore))
        .collect();
    if pre_restore.len() > 1 {
        candidates.extend(pre_restore[..pre_restore.len() - 1].iter().copied());
    }

    // 第一趟：纯试算，不碰磁盘。
    let mut plan: Vec<&BackupFile> = Vec::new();
    let mut projected = used;
    for f in candidates {
        if projected.saturating_add(incoming_bytes) <= quota {
            break;
        }
        projected = projected.saturating_sub(f.size_bytes);
        plan.push(f);
    }
    if projected.saturating_add(incoming_bytes) > quota {
        return Err(BackupErrorKind::QuotaExceeded);
    }

    // 第二趟：确认装得下才真删。
    let mut removed = Vec::new();
    for f in plan {
        if fs::remove_file(&f.path).is_ok() {
            removed.push(f.path.clone());
        }
    }
    Ok(removed)
}

// ── 对外入口 ────────────────────────────────────────────────────────────

/// 备份状态文件（`backups/.state`），四字段：
///
/// ```text
/// <daily_ms> <last_ms> <last_error_ms> <last_error_kind|->
/// ```
///
/// ## 为什么必须落盘，不能留在内存
///
/// 界面不呈现 `skipped`（那是内部决策，不是用户能据以行动的信息），于是「备份是否在正常
/// 工作」只能靠两个**历史事实**判断：
///
/// * [`BackupState::last_ms`] —— 「上次备份：今天 21:14」的唯一权威来源。注意**失败与节流
///   跳过都不更新它**，否则失败会被写成"刚刚备份成功"。
/// * [`BackupState::last_error_ms`] + `last_error_kind` —— **粘性**错误。第一帧失败、后面
///   又成功了，最后一帧的 `error` 自然是空的；只看返回值，界面上的「最近一次自动备份失败」
///   会在下一次成功时凭空消失。失败记录只能由用户显式清除。
///
/// 也不用进程内静态量：测试并行跑会互相污染，重启后「跨零点保护」也会失效一次。
/// 放在备份目录里，跨进程保持、且随测试目录天然隔离。
const STATE_FILE: &str = ".state";

/// 备份节流与健康状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupState {
    /// 最近一份日备份的生成时刻（跨零点保护用）。
    pub daily_ms: i64,
    /// 最近一次 `last.bak` **成功刷新**的时刻。`0` 表示从未备份成功过。
    pub last_ms: i64,
    /// 最近一次失败的原始时刻（粘性，成功不清除）。`0` 表示没有未清除的失败。
    pub last_error_ms: i64,
    /// 失败原因（见 [`BackupErrorKind`]）。有时刻就必然有原因，两者同生同灭。
    pub last_error_kind: Option<BackupErrorKind>,
}

fn read_state(dir: &Path) -> BackupState {
    let Ok(text) = fs::read_to_string(dir.join(STATE_FILE)) else {
        return BackupState::default();
    };
    let mut it = text.split_whitespace();
    let daily_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let last_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let last_error_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let token = it.next().unwrap_or("");
    // 一致性兜底：没有时刻就没有"未清除的失败"，半条记录一律当没有。
    // 反之有时刻但 token 认不出，必须落 `Other` 而不是 `None`——失败确实发生过，
    // 只是原因来自更新的版本，不能因为认不出来就把警告抹掉。
    let last_error_kind = if last_error_ms == 0 {
        None
    } else {
        Some(BackupErrorKind::from_token(token))
    };
    BackupState {
        daily_ms,
        last_ms,
        last_error_ms,
        last_error_kind,
    }
}

fn write_state(dir: &Path, st: &BackupState) {
    // 写不进去不影响数据安全（只是下次可能多备份一次 / 少一条历史），故忽略错误。
    let kind = st.last_error_kind.map(|k| k.as_str()).unwrap_or("-");
    let _ = fs::write(
        dir.join(STATE_FILE),
        format!(
            "{} {} {} {}",
            st.daily_ms, st.last_ms, st.last_error_ms, kind
        ),
    );
}

/// 当前备份状态（界面读「上次备份」与粘性错误用）。
pub fn state(vault_path: &Path) -> BackupState {
    read_state(&backup_dir(vault_path))
}

/// 备份陈旧判定超过这个距离就告警（一天）。
pub const STALE_AFTER_MS: i64 = 86_400_000;

/// 时间差超过这个值一律判「无法判定」（十年）。
///
/// 出现"备份已 19000 天未更新"的唯一现实原因是设备时钟被改过或库从别的机器拷来，
/// 那种情况下告警没用，只会让用户直接不再相信这个界面。
pub const STALE_TRUST_MAX_MS: i64 = 3650 * 86_400_000;

/// 备份新鲜度判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Staleness {
    /// 从未成功备份过（新库）。文案「当前没有备份」，**不能**拿去算差值，
    /// 否则会显示"已 19000 天未更新"。
    Never,
    /// 无从判断（拿不到库 mtime，或时间差为负 / 超出可信区间——改过系统时间 / 跨时区 /
    /// 库从别的机器拷来）。这种情况**不告警、不显示"N 天前"**。
    Unknown,
    Fresh,
    /// 库内容已经比最近一次备份新了一天以上 —— 备份在静默失败。
    Stale,
}

/// 判断备份是否陈旧。
///
/// 用**库文件 mtime** 而不是 `now_ms` 做参照：用户三天没改库，备份也停在三天前，那是正常
/// 的，不该告警。只有「库一直在变、备份没跟上」才是真问题——而 `RestoreGuard` 泄漏导致
/// `is_restoring()` 永久为真、所有备份静默停摆，这是唯一能看见它的通道。
pub fn staleness(vault_path: &Path, last_ms: i64) -> Staleness {
    if last_ms == 0 {
        return Staleness::Never;
    }
    let Some(vault_mtime) = mtime_ms(vault_path) else {
        return Staleness::Unknown;
    };
    let diff = vault_mtime - last_ms;
    if !(0..=STALE_TRUST_MAX_MS).contains(&diff) {
        return Staleness::Unknown;
    }
    if diff > STALE_AFTER_MS {
        Staleness::Stale
    } else {
        Staleness::Fresh
    }
}

/// 恢复入口的安全判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreSafety {
    /// 当前库的状态已经保住（或本来就没东西可保），可以直接往下走。
    Clear,
    /// 当前库**可备份但备份失败**：往下走会丢掉当前状态，必须由用户明确承担。
    Blocked(BackupErrorKind),
    /// 当前库本来就不存在，没有"当前状态"可失去——备份失败在这里不成立。
    ///
    /// 这不是"被跳过"，是**目的自动消失**。库文件不见了恰恰是只能从备份恢复的场景，
    /// 也就是最需要放行的时候。
    NothingToLose,
}

/// 由 `pre-restore` 快照的返回值判断恢复能不能自动往下走。
///
/// ## 为什么判据只看 `error_kind`，不按 `SkipReason` 逐一豁免
///
/// 按枚举写白名单需要在 `SkipReason` 每次加值时重新审一遍，漏一个就把用户挡在唯一出路
/// 门外。而这里要回答的问题其实只有一个：**当前有没有可保住的东西**。
///
/// 没有（库不存在 / 本来就读不到源），这个目的就自动消失，不算失败。
/// 所以判据是 [`BackupOutcome::error_kind`] —— 它非空才代表"该保的没保住"。
pub fn restore_safety(outcome: &BackupOutcome) -> RestoreSafety {
    if outcome.skipped == Some(SkipReason::NoSource) {
        return RestoreSafety::NothingToLose;
    }
    match outcome.error_kind {
        Some(kind) => RestoreSafety::Blocked(kind),
        None => RestoreSafety::Clear,
    }
}

/// 写入前的常规备份（日备份 + `last.bak`）。
///
/// 必须在**外层操作边界**调用一次（一次用户操作 = 一次），不要挂在单帧写入上：
/// 一次导入 5000 条若逐帧触发，就是 5000 次整库复制。
///
/// 失败**不返回 Err**——备份写不进去也要让用户改得动数据，但失败会落在
/// [`BackupOutcome::error`] 里，由调用方转成可见警告。
pub fn snapshot_before_write(vault_path: &Path, now_ms: i64) -> BackupOutcome {
    let mut out = BackupOutcome::default();
    if is_restoring() {
        out.skipped = Some(SkipReason::Restoring);
        return out;
    }
    if !vault_path.is_file() {
        out.skipped = Some(SkipReason::NoSource);
        return out;
    }
    let dir = match prepare_dir(vault_path) {
        Ok(d) => d,
        Err(e) => {
            out.fail("dir", &e);
            return out;
        }
    };
    let mut st = read_state(&dir);
    let src_len = fs::metadata(vault_path).map(|m| m.len()).unwrap_or(0);

    // 1) 日备份：当天首次写入前生成；已存在或距上一份不足 1 小时则跳过。
    let daily_path = dir.join(format!("bak.{}", utc_date_key(now_ms)));
    if daily_path.exists() {
        out.skipped = Some(SkipReason::DailyExists);
    } else if st.daily_ms != 0 && now_ms - st.daily_ms < DAILY_MIN_INTERVAL_MS {
        // 跨零点保护：23:59 与 00:01 只差两分钟，不该产生两份「日备份」。
        out.skipped = Some(SkipReason::DailyTooSoon);
    } else {
        match make_room(vault_path, src_len) {
            Ok(removed) => {
                out.pruned.extend(removed);
                match copy_atomic(vault_path, &daily_path) {
                    Ok(()) => {
                        out.created.push(daily_path);
                        st.daily_ms = now_ms;
                    }
                    Err(e) => out.fail("daily", &e),
                }
            }
            // 配额装不下 → 走**失败**通道，不是"正常跳过"。备份从这一刻起停了。
            // `error` 保持 `None`：这不是 IO 错误，是策略性拒绝，不该有系统报错文本。
            Err(kind) => out.fail_kind(kind, "daily"),
        }
    }

    // 2) last.bak：大库按时间节流（小库每次都刷新，保证「刚才改错了」能退回去）。
    let last_path = dir.join("last.bak");
    let size = fs::metadata(vault_path).map(|m| m.len()).unwrap_or(0);
    let throttled = size > LARGE_VAULT_BYTES
        && st.last_ms != 0
        && now_ms - st.last_ms < LARGE_VAULT_MIN_INTERVAL_MS;
    if throttled {
        if out.skipped.is_none() {
            out.skipped = Some(SkipReason::Throttled);
        }
    } else {
        // 覆盖同名文件是净额为零的操作，只有"新库比旧库大"才真占地方。
        let last_existing = fs::metadata(&last_path).map(|m| m.len()).unwrap_or(0);
        let delta = src_len.saturating_sub(last_existing);
        match make_room(vault_path, delta) {
            Ok(removed) => {
                out.pruned.extend(removed);
                match copy_atomic(vault_path, &last_path) {
                    Ok(()) => {
                        out.created.push(last_path);
                        // 只有**成功刷新**才推进 `last_ms`。失败与节流跳过都不动它，
                        // 否则界面会显示"上次备份：刚刚"，而实际一次也没写成。
                        st.last_ms = now_ms;
                    }
                    Err(e) => out.fail("last", &e),
                }
            }
            Err(kind) => out.fail_kind(kind, "last"),
        }
    }

    // 粘性错误：只在本次真的失败时才落盘；成功**不清除**上一次未处理的失败
    // （第一帧失败、后面又成功时，只看最后一帧的 `error_kind` 是空的）。
    if let Some(kind) = out.error_kind {
        st.last_error_ms = now_ms;
        st.last_error_kind = Some(kind);
    }
    write_state(&dir, &st);
    out.pruned.extend(prune(vault_path));
    // 不再有"只有全都没写成才报"的守卫：那等于"部分成功就不警告"，而配额挡掉日备份
    // 意味着"几天前"那条退路在悄悄变薄，哪怕 `last.bak` 写成了也必须让用户看见。
    out
}

/// 破坏性操作前的事件快照（恢复 / 换主密码 / 压缩 / 导入 apply）。
///
/// 与 [`snapshot_before_write`] 的区别：**不受日备份去重与节流影响**——这几种操作
/// 一旦出错就是整库级损失，哪怕同一秒内已经备份过也要再留一份。
pub fn snapshot_event(vault_path: &Path, tag: EventTag, now_ms: i64) -> BackupOutcome {
    let mut out = BackupOutcome::default();
    // 恢复期一律不碰备份目录：此时可能有另一线程正把某份备份读进内存，
    // 而本函数的尾部会 `prune()` 删旧文件——删到正在被读的那一份就等于恢复失败。
    // 调用约定：`pre-restore` 快照必须在 [`RestoreGuard::new`] **之前**写。
    if is_restoring() {
        out.skipped = Some(SkipReason::Restoring);
        return out;
    }
    if !vault_path.is_file() {
        out.skipped = Some(SkipReason::NoSource);
        return out;
    }
    let dir = match prepare_dir(vault_path) {
        Ok(d) => d,
        Err(e) => {
            out.fail("dir", &e);
            return out;
        }
    };
    let mut st = read_state(&dir);
    let dest = dir.join(format!("{}.{}", tag.prefix(), now_ms));
    // 事件快照的名字带 epoch，几乎不会与既有文件同名；仍按净增量算，防同一毫秒重复调用。
    let src_len = fs::metadata(vault_path).map(|m| m.len()).unwrap_or(0);
    let delta = src_len.saturating_sub(fs::metadata(&dest).map(|m| m.len()).unwrap_or(0));
    match make_room(vault_path, delta) {
        Ok(removed) => out.pruned.extend(removed),
        Err(kind) => {
            // 装不下就一份都不新建：破坏性操作前的退路宁可没有，也不能是"删了旧的凑出来"。
            out.fail_kind(kind, tag.prefix());
            // 早退也必须留痕，否则"配额挡住了所有事件快照"这条事实只活在这一帧的返回值里，
            // 下一帧成功就消失——而事件快照被挡意味着用户在马上面临破坏性操作时没有退路。
            if let Some(k) = out.error_kind {
                st.last_error_ms = now_ms;
                st.last_error_kind = Some(k);
            }
            write_state(&dir, &st);
            return out;
        }
    }
    match copy_atomic(vault_path, &dest) {
        Ok(()) => {
            out.created.push(dest);
            // 事件快照也是备份：界面「上次备份」必须把它算进去。否则换完密码明明有一份
            // 可用的退路，界面却说「当前没有备份」——因为 `isolate` 刚把旧 `.state` 挪走了。
            st.last_ms = now_ms;
            write_state(&dir, &st);
        }
        Err(e) => {
            out.fail(tag.prefix(), &e);
            if let Some(kind) = out.error_kind {
                st.last_error_ms = now_ms;
                st.last_error_kind = Some(kind);
            }
            write_state(&dir, &st);
            return out;
        }
    }
    // 事件快照生成后才轮转（先保证新备份可用，再删旧的）。
    out.pruned.extend(prune(vault_path));
    out
}

/// 删除全部备份，返回删除的文件数。
///
/// 为什么需要：Android 备份目录在应用私有存储里，用户**进不去**（没有文件管理器入口，
/// 也没有 SAF 授权），只能靠应用内提供出口。删完立即刷新占用。
///
/// `.state` 一并删掉：它是**这次**备份历史的一部分。留着会让界面在"已删除全部备份"之后
/// 仍然显示"上次备份：昨天 21:14"，用户以为还有东西可恢复。删掉后 [`staleness`] 回到
/// `Never`，文案自然变成「当前没有备份」。
pub fn clear_all(vault_path: &Path) -> CoreResult<usize> {
    let files = inventory(vault_path);
    let mut removed = 0;
    for f in files {
        if fs::remove_file(&f.path).is_ok() {
            removed += 1;
        }
    }
    let _ = fs::remove_file(backup_dir(vault_path).join(STATE_FILE));
    Ok(removed)
}

/// 清除粘性错误记录（用户显式确认"我知道了"）。
///
/// 失败记录**只能**由这个入口清掉：下一次成功备份不会自动清，否则"第一帧失败、后面又成功"
/// 会让警告凭空消失，而那次失败里丢掉的退路不会回来。
pub fn clear_error(vault_path: &Path) -> BackupState {
    let dir = backup_dir(vault_path);
    let mut st = read_state(&dir);
    st.last_error_ms = 0;
    st.last_error_kind = None;
    write_state(&dir, &st);
    st
}

/// 云同步目录探测：命中返回产品名（用于设置页常驻告警）。
///
/// 为什么需要：备份目录里是**加密容器的副本**，语义上等于把库文件复制进了
/// OneDrive/Dropbox。这与产品「数据不经云」的叙事相反，而且会让 `last.bak`
/// 每 60 秒变一次 → 同步客户端天天传几十 MB。
/// 不阻断（那是用户的选择），但必须让用户知道自己在做什么。
pub fn detect_cloud_sync_dir(vault_path: &Path) -> Option<&'static str> {
    let lowered = vault_path.to_string_lossy().to_lowercase();
    const MARKERS: &[(&str, &str)] = &[
        ("onedrive", "OneDrive"),
        ("dropbox", "Dropbox"),
        ("google drive", "Google Drive"),
        ("googledrive", "Google Drive"),
        ("icloud drive", "iCloud Drive"),
        ("com~apple~clouddocs", "iCloud Drive"),
        ("nutstore", "坚果云"),
        ("坚果云", "坚果云"),
        ("baidunetdisk", "百度网盘"),
        ("baidu netdisk", "百度网盘"),
        ("stefan sync", "Stefan Sync"),
        ("syncthing", "Syncthing"),
        ("weiyun", "微云"),
        ("box sync", "Box"),
    ];
    MARKERS
        .iter()
        .find(|(needle, _)| lowered.contains(needle))
        .map(|(_, name)| *name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("pms-backup-test-{name}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let vault = root.join("passwordms.vault");
        fs::write(&vault, vec![7u8; 1024]).unwrap();
        (root, vault)
    }

    #[test]
    fn 日期换算覆盖闰年与世纪年() {
        // 2026-11-02 00:00:00 UTC
        assert_eq!(utc_date_key(1_793_577_600_000), "20261102");
        // 2024-02-29（闰年）
        assert_eq!(utc_date_key(1_709_164_800_000), "20240229");
        // 2000-03-01（世纪闰年：2000 能被 400 整除，是闰年）
        assert_eq!(utc_date_key(951_868_800_000), "20000301");
        // 1970-01-01
        assert_eq!(utc_date_key(0), "19700101");
    }

    #[test]
    fn 日期与天数互为逆运算() {
        // 两个方向都要对：解析 `bak.YYYYMMDD` 依赖 days_from_civil，
        // 生成文件名依赖 civil_from_days，任一方向错都会让轮转判错新旧。
        for days in [-20_000i64, -1, 0, 1, 19_000, 20_800] {
            let ms = days * 86_400_000;
            let key = utc_date_key(ms);
            assert_eq!(
                created_ms_from_name(&format!("bak.{key}")),
                Some(ms),
                "days={days} key={key}"
            );
        }
    }

    #[test]
    fn 首次写入生成日备份与上次改动前() {
        let (_root, vault) = temp_vault("first");
        let now = 1_793_577_600_000;
        let out = snapshot_before_write(&vault, now);
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.created.len(), 2, "应同时生成日备份与 last.bak");
        let files = inventory(&vault);
        assert_eq!(
            files
                .iter()
                .filter(|f| f.class == BackupClass::Daily)
                .count(),
            1
        );
        assert_eq!(
            files
                .iter()
                .filter(|f| f.class == BackupClass::Last)
                .count(),
            1
        );
    }

    #[test]
    fn 同一天第二次写入只刷新上次改动前() {
        let (_root, vault) = temp_vault("second");
        let now = 1_793_577_600_000;
        snapshot_before_write(&vault, now);
        // 内容变化后再写一次（同一天）
        fs::write(&vault, vec![9u8; 2048]).unwrap();
        let out = snapshot_before_write(&vault, now + 5_000);
        assert_eq!(out.skipped, Some(SkipReason::DailyExists));
        assert_eq!(out.created.len(), 1, "只应刷新 last.bak");
        assert!(out.created[0].ends_with("last.bak"), "{:?}", out.created[0]);
    }

    #[test]
    fn 跨零点不足一小时不新增日备份() {
        let (_root, vault) = temp_vault("midnight");
        // 2026-11-01 23:50 UTC
        let late = 1_793_577_000_000;
        snapshot_before_write(&vault, late);
        // 次日 00:10 UTC（文件名会变，但只差 20 分钟）
        let out = snapshot_before_write(&vault, late + 1_200_000);
        assert_eq!(out.skipped, Some(SkipReason::DailyTooSoon));
        assert!(
            inventory(&vault)
                .iter()
                .filter(|f| f.class == BackupClass::Daily)
                .count()
                == 1,
            "不应产生第二份日备份"
        );
    }

    #[test]
    fn 日备份超过五份时淘汰最旧() {
        let (_root, vault) = temp_vault("rotate");
        let base = 1_793_491_200_000;
        // 连续 8 天，每天间隔 25 小时（避开 1 小时跨零点保护）
        for day in 0..8 {
            snapshot_before_write(&vault, base + day * 90_000_000);
        }
        let daily = inventory(&vault)
            .into_iter()
            .filter(|f| f.class == BackupClass::Daily)
            .count();
        assert_eq!(daily, DAILY_KEEP, "日备份应只保留 {DAILY_KEEP} 份");
    }

    #[test]
    fn 事件快照按类型各自保留且不挤占日备份() {
        let (_root, vault) = temp_vault("event");
        let now = 1_793_491_200_000;
        snapshot_before_write(&vault, now);
        // 连续三次「恢复前」：必须留 2 份——第二次恢复之前的状态正是第一次的退路。
        for i in 0..3 {
            snapshot_event(&vault, EventTag::PreRestore, now + i);
        }
        // 连续三次「压缩前」：只留 1 份（压缩不会连着做两次还想要中间态）。
        for i in 0..3 {
            snapshot_event(&vault, EventTag::PreCompact, now + 1_000 + i);
        }
        let files = inventory(&vault);
        let pre_restore = files
            .iter()
            .filter(|f| f.event == Some(EventTag::PreRestore))
            .count();
        let pre_compact = files
            .iter()
            .filter(|f| f.event == Some(EventTag::PreCompact))
            .count();
        assert_eq!(pre_restore, PRE_RESTORE_KEEP, "恢复前快照应留 2 份");
        assert_eq!(pre_compact, EVENT_KEEP_PER_TAG, "压缩前快照应留 1 份");
        assert_eq!(
            files
                .iter()
                .filter(|f| f.class == BackupClass::Daily)
                .count(),
            1,
            "事件快照不得挤占日备份槽位"
        );
    }

    #[test]
    fn 恢复期间暂停全部快照() {
        let (_root, vault) = temp_vault("restoring");
        let now = 1_793_491_200_000;
        {
            let _guard = RestoreGuard::new(&vault);
            let out = snapshot_before_write(&vault, now);
            assert_eq!(out.skipped, Some(SkipReason::Restoring));
            assert!(out.created.is_empty());
            let ev = snapshot_event(&vault, EventTag::PreCompact, now);
            // 事件快照在恢复期**同样**跳过：它的尾部会 prune()，可能删到正被读取的备份。
            assert_eq!(ev.skipped, Some(SkipReason::Restoring));
            assert!(ev.created.is_empty());
        }
        // 守卫释放后恢复正常
        let out = snapshot_before_write(&vault, now);
        assert!(out.error.is_none());
        assert!(!out.created.is_empty());
    }

    #[test]
    fn 源文件不存在时不报错但记录跳过() {
        let root = std::env::temp_dir().join("pms-backup-test-nosrc");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let vault = root.join("missing.vault");
        let out = snapshot_before_write(&vault, 1_793_491_200_000);
        assert_eq!(out.skipped, Some(SkipReason::NoSource));
        assert!(out.error.is_none(), "首次创建库不该报备份错误");
    }

    #[test]
    fn 大库节流生效() {
        let (_root, vault) = temp_vault("throttle");
        // 造一个 > 4 MiB 的库
        fs::write(&vault, vec![1u8; (LARGE_VAULT_BYTES + 1024) as usize]).unwrap();
        let now = 1_793_491_200_000;
        snapshot_before_write(&vault, now);
        let out = snapshot_before_write(&vault, now + 1_000);
        // 日备份已存在 → 跳过；last.bak 在 60 秒内 → 也被节流
        assert_eq!(out.created.len(), 0, "{:?}", out.created);
        assert_eq!(out.skipped, Some(SkipReason::DailyExists));
    }

    #[test]
    fn 备份不留半成品临时文件() {
        let (_root, vault) = temp_vault("tmp");
        snapshot_before_write(&vault, 1_793_491_200_000);
        let dir = backup_dir(&vault);
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.ends_with(".tmp"), "残留临时文件：{name}");
        }
    }

    #[test]
    fn 备份内容与源文件逐字节一致() {
        let (_root, vault) = temp_vault("content");
        let original = fs::read(&vault).unwrap();
        snapshot_before_write(&vault, 1_793_491_200_000);
        let last = backup_dir(&vault).join("last.bak");
        assert_eq!(fs::read(&last).unwrap(), original, "备份必须是源文件的副本");
    }

    #[test]
    fn 删除全部备份会清空目录() {
        let (_root, vault) = temp_vault("clear");
        let now = 1_793_491_200_000;
        snapshot_before_write(&vault, now);
        snapshot_event(&vault, EventTag::PostRekey, now);
        assert!(usage(&vault).1 >= 2);
        let removed = clear_all(&vault).expect("删除应成功");
        assert_eq!(removed, 3, "日备份 + last.bak + 事件快照");
        assert_eq!(usage(&vault).1, 0);
        // 源库不受影响
        assert!(vault.is_file());
    }

    #[test]
    fn 轮转不删用户自己放进备份目录的文件() {
        let (_root, vault) = temp_vault("foreign");
        let now = 1_793_491_200_000;
        let dir = backup_dir(&vault);
        fs::create_dir_all(&dir).unwrap();
        let foreign = dir.join("我的笔记.txt");
        fs::write(&foreign, b"do not delete").unwrap();
        for day in 0..8 {
            snapshot_before_write(&vault, now + day * 90_000_000);
        }
        assert!(foreign.is_file(), "轮转只处理自己命名的文件");
    }

    #[test]
    fn 云同步目录能被识别() {
        assert_eq!(
            detect_cloud_sync_dir(Path::new(r"C:\Users\me\OneDrive\PasswordMS\vault.db")),
            Some("OneDrive")
        );
        assert_eq!(
            detect_cloud_sync_dir(Path::new("/home/me/Syncthing/pms/vault.db")),
            Some("Syncthing")
        );
        assert_eq!(
            detect_cloud_sync_dir(Path::new(
                r"C:\Users\me\AppData\Roaming\PasswordMS\vault.db"
            )),
            None
        );
    }

    // ── 配额（C-45）──────────────────────────────────────────────────────

    #[test]
    fn 配额按库体积十二倍且封顶一吉字节() {
        assert_eq!(quota_bytes(0), 0, "空库没有配额，也就没有备份");
        assert_eq!(quota_bytes(100 * 1024), 100 * 1024 * QUOTA_MULTIPLIER);
        // 200 MiB 的库按 12 倍算要 2.4 GB，必须被 1 GB 封住。
        assert_eq!(quota_bytes(200 * 1024 * 1024), QUOTA_MAX_BYTES);
        assert_eq!(quota_bytes(10 * 1024 * 1024 * 1024), QUOTA_MAX_BYTES);
    }

    /// 造出「5 份日备份 + last.bak + 2 份 pre-restore」这 8 份满槽备份。
    fn fill_slots(vault: &Path, size: usize, base: i64) {
        fs::write(vault, vec![3u8; size]).unwrap();
        for day in 0..5 {
            fs::write(vault, vec![3u8 + day as u8; size]).unwrap();
            snapshot_before_write(vault, base + day * 90_000_000);
        }
        for i in 0..2 {
            snapshot_event(vault, EventTag::PreRestore, base + 5 * 90_000_000 + i);
        }
    }

    #[test]
    fn 配额不足时由旧到新裁剪且保住上次改动前与恢复前() {
        let (_root, vault) = temp_vault("quota-trim");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        assert_eq!(usage(&vault).1, 8, "5 日备份 + last.bak + 2 份恢复前");

        // 库缩小到一半 → 配额跟着降到 2048×12 = 24576，旧备份已经装不下。
        fs::write(&vault, vec![5u8; 2048]).unwrap();
        assert_eq!(quota_for(&vault), 2048 * QUOTA_MULTIPLIER);

        let out = snapshot_event(&vault, EventTag::PreCompact, base + 5 * 90_000_000 + 100);
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.created.len(), 1, "装得下就该建");
        assert!(!out.pruned.is_empty(), "应裁掉最旧的几份");

        let files = inventory(&vault);
        assert!(
            files.iter().any(|f| f.class == BackupClass::Last),
            "last.bak 永不因配额被删"
        );
        assert_eq!(
            files
                .iter()
                .filter(|f| f.event == Some(EventTag::PreRestore))
                .count(),
            PRE_RESTORE_KEEP,
            "恢复前快照是保底项，配额不该动它"
        );
        assert!(
            files
                .iter()
                .filter(|f| f.class == BackupClass::Daily)
                .count()
                >= 1,
            "至少留 1 份日备份"
        );
        // 裁剪后总占用必须真的回到配额内，否则配额形同虚设。
        assert!(
            usage(&vault).0 <= quota_for(&vault),
            "裁剪后仍超配额：{} > {}",
            usage(&vault).0,
            quota_for(&vault)
        );
    }

    #[test]
    fn 裁到保底仍装不下时一份都不删() {
        let (_root, vault) = temp_vault("quota-block");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        let before = usage(&vault);
        assert_eq!(before.1, 8);

        // 库缩到 1 KiB → 配额 12288。保底三样（last.bak + 1 日备份 + 1 恢复前）
        // 已经是 3×4096 = 12288，再加新快照必然装不下。
        fs::write(&vault, vec![6u8; 1024]).unwrap();
        let out = snapshot_event(&vault, EventTag::PreCompact, base + 5 * 90_000_000 + 200);

        // 推翻 v1.5.29 早期决定：配额**不是**"正常跳过"，是"备份停了"，走失败通道。
        assert_eq!(out.error_kind, Some(BackupErrorKind::QuotaExceeded));
        assert!(out.created.is_empty(), "装不下就不该建");
        assert!(out.error.is_none(), "配额是策略性拒绝，不该带系统报错文本");

        // 关键：试算失败时磁盘必须原样不动——不能"删了 5 份还是不够，于是也没建"。
        let after = usage(&vault);
        assert_eq!(after.1, before.1, "裁不动时一份都不许删");
        assert_eq!(after.0, before.0, "裁不动时占用不许变");
    }

    #[test]
    fn 配额充裕时不做任何裁剪() {
        let (_root, vault) = temp_vault("quota-roomy");
        let base = 1_793_491_200_000;
        fs::write(&vault, vec![1u8; 2048]).unwrap();
        let out = snapshot_before_write(&vault, base);
        assert!(out.pruned.is_empty(), "远未到配额，不该动既有备份");
        assert_eq!(out.created.len(), 2);
    }

    #[test]
    fn 配额被挡时记失败但不推进上次备份时间() {
        let (_root, vault) = temp_vault("quota-state");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        let before = state(&vault).last_ms;
        assert_ne!(before, 0, "铺场后应有上次备份时间");

        fs::write(&vault, vec![6u8; 1024]).unwrap();
        let out = snapshot_event(&vault, EventTag::PreCompact, base + 5 * 90_000_000 + 300);
        assert_eq!(out.error_kind, Some(BackupErrorKind::QuotaExceeded));
        assert_eq!(
            state(&vault).last_ms,
            before,
            "一份都没写成就不许显示「上次备份：刚刚」"
        );
        assert_eq!(
            state(&vault).last_error_kind,
            Some(BackupErrorKind::QuotaExceeded),
            "配额装不下 = 备份停了，必须留痕让用户看得见"
        );
    }

    // ── 旧密钥隔离（R-5）────────────────────────────────────────────────

    #[test]
    fn 换主密码后旧备份被隔离且当前代归零() {
        let (_root, vault) = temp_vault("isolate");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        assert_eq!(usage(&vault).1, 8);

        let out = isolate_old_key_backups(&vault, base + 10_000_000);
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.moved, 8, "旧备份应全部挪走");
        assert!(out.dir.is_some());

        assert_eq!(usage(&vault).1, 0, "当前代必须归零");
        assert_eq!(old_key_usage(&vault).1, 8);
        // `.state` 跟着旧代走：新库不能继承旧库的"上次备份"，否则界面在一份都没有时
        // 显示"上次备份：昨天"，用户以为还有东西可恢复。
        assert_eq!(state(&vault).last_ms, 0);
        assert_eq!(staleness(&vault, 0), Staleness::Never);

        // 换密后的 post-rekey 属于新一代，不得再进 old-key。
        let ev = snapshot_event(&vault, EventTag::PostRekey, base + 10_000_001);
        assert_eq!(ev.created.len(), 1);
        assert_eq!(usage(&vault).1, 1);
        assert_eq!(old_key_usage(&vault).1, 8, "旧代份数不该变");
        assert_eq!(
            state(&vault).last_ms,
            base + 10_000_001,
            "事件快照也是备份，界面「上次备份」要认它"
        );
    }

    #[test]
    fn 删除全部备份只清当前代旧密钥代要单独清() {
        let (_root, vault) = temp_vault("clear-scope");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        isolate_old_key_backups(&vault, base + 10_000_000);
        snapshot_before_write(&vault, base + 10_000_001);
        assert_eq!(usage(&vault).1, 2, "新一代应有日备份 + last.bak");

        let removed = clear_all(&vault).expect("删除应成功");
        assert_eq!(removed, 2, "只清当前代");
        assert_eq!(usage(&vault).1, 0);
        assert_eq!(
            old_key_usage(&vault).1,
            8,
            "旧代是另一把密码的退路，不能被同一个按钮删掉"
        );

        let removed_old = clear_old_key(&vault).expect("删除旧代应成功");
        assert_eq!(removed_old, 8);
        assert_eq!(old_key_usage(&vault).1, 0);
        assert!(!old_key_dir(&vault).exists(), "空批次目录应被清掉");
    }

    #[test]
    fn 没有旧备份时不留下空隔离目录() {
        let (_root, vault) = temp_vault("isolate-empty");
        let out = isolate_old_key_backups(&vault, 1_793_491_200_000);
        assert_eq!(out.moved, 0);
        assert!(out.dir.is_none());
        assert!(!old_key_dir(&vault).exists());
    }

    // ── 状态与陈旧判定（R-4 / R-12）──────────────────────────────────────

    #[test]
    fn 失败会被记住且成功不会自动清除() {
        let (_root, vault) = temp_vault("sticky");
        let base = 1_793_491_200_000;
        let dir = backup_dir(&vault);
        fs::create_dir_all(&dir).unwrap();
        // 用同名目录占住 last.bak，让 rename 必然失败（跨平台都成立，不依赖权限位）。
        fs::create_dir_all(dir.join("last.bak")).unwrap();

        let out = snapshot_before_write(&vault, base);
        assert!(out.error_kind.is_some(), "应归类出失败原因");
        assert!(out.error.is_some(), "原始文本留给日志");
        let st = state(&vault);
        assert_eq!(st.last_error_ms, base);
        assert!(st.last_error_kind.is_some());

        fs::remove_dir(dir.join("last.bak")).unwrap();
        let out2 = snapshot_before_write(&vault, base + 1_000);
        assert!(out2.error_kind.is_none(), "{:?}", out2.error_kind);
        let st2 = state(&vault);
        assert!(
            st2.last_error_kind.is_some(),
            "成功不得清除未处理的失败——否则「第一帧失败、后面又成功」会把警告抹掉"
        );
        assert_eq!(st2.last_error_ms, base, "失败时刻也不得被成功覆盖");
        assert!(st2.last_ms > 0, "成功应推进上次备份时间");

        let st3 = clear_error(&vault);
        assert!(st3.last_error_kind.is_none(), "显式清除后才消失");
        assert_eq!(st3.last_error_ms, 0);
    }

    #[test]
    fn 保留零字节临时文件的清理不残留() {
        let (_root, vault) = temp_vault("tmp-clean");
        let base = 1_793_491_200_000;
        let dir = backup_dir(&vault);
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(dir.join("last.bak")).unwrap();
        snapshot_before_write(&vault, base);
        fs::remove_dir(dir.join("last.bak")).unwrap();
        // rename 失败后 tmp 必须被清掉，否则每失败一次留一份整库大小的垃圾。
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.ends_with(".tmp"), "残留临时文件：{name}");
        }
    }

    #[test]
    fn 陈旧判定把从未备份与时间倒流都当成不告警() {
        let (_root, vault) = temp_vault("stale");
        let now = mtime_ms(&vault).expect("临时库应有 mtime");

        assert_eq!(staleness(&vault, 0), Staleness::Never);
        // 备份时刻在未来：改过系统时间 / 跨时区 / 库从别的机器拷来。
        assert_eq!(staleness(&vault, now + 10 * 86_400_000), Staleness::Unknown);
        // 超出十年可信区间：一律未知，不许显示"已 19000 天未更新"。
        assert_eq!(
            staleness(&vault, now - 4000 * 86_400_000),
            Staleness::Unknown
        );
        assert_eq!(staleness(&vault, now), Staleness::Fresh);
        assert_eq!(staleness(&vault, now - 2 * 86_400_000), Staleness::Stale);
        // 边界：恰好一天算新鲜，超过一天才算陈旧。
        assert_eq!(staleness(&vault, now - STALE_AFTER_MS), Staleness::Fresh);
        assert_eq!(
            staleness(&vault, now - STALE_AFTER_MS - 1),
            Staleness::Stale
        );
    }

    #[test]
    fn 恢复安全判定只看有没有可保住的东西() {
        let (_root, vault) = temp_vault("restore-safety");
        let base = 1_793_491_200_000;

        // 1) 库文件整个不存在：没有"当前状态"可失去 → 放行。
        //    这正是最需要恢复的时刻，把它判成"备份失败"会锁死唯一出路。
        let missing = vault.with_file_name("missing.vault");
        let out = snapshot_event(&missing, EventTag::PreRestore, base);
        assert_eq!(out.skipped, Some(SkipReason::NoSource));
        assert_eq!(restore_safety(&out), RestoreSafety::NothingToLose);

        // 2) 正常快照写成 → 放行。
        let ok = snapshot_event(&vault, EventTag::PreRestore, base);
        assert!(ok.error_kind.is_none(), "{:?}", ok.error_kind);
        assert_eq!(restore_safety(&ok), RestoreSafety::Clear);

        // 3) 配额挡住快照 → 拦住，且要能说出原因（前端据此给不同动作）。
        let root2 = std::env::temp_dir().join("pms-backup-test-restore-safety-block");
        let _ = fs::remove_dir_all(&root2);
        fs::create_dir_all(&root2).unwrap();
        let v2 = root2.join("passwordms.vault");
        fill_slots(&v2, 4096, base);
        fs::write(&v2, vec![6u8; 1024]).unwrap();
        let blocked = snapshot_event(&v2, EventTag::PreRestore, base + 5 * 90_000_000 + 500);
        assert_eq!(
            blocked.error_kind,
            Some(BackupErrorKind::QuotaExceeded),
            "铺场后缩库应触发配额"
        );
        assert_eq!(
            restore_safety(&blocked),
            RestoreSafety::Blocked(BackupErrorKind::QuotaExceeded)
        );
    }

    #[test]
    fn 新旧两代状态互不干扰() {
        let (_root, vault) = temp_vault("two-gen");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        isolate_old_key_backups(&vault, base + 10_000_000);
        snapshot_before_write(&vault, base + 10_000_001);

        // 新一代的 `.state` 独立存在，且不指向旧代的任何文件。
        let st = state(&vault);
        assert_eq!(st.last_ms, base + 10_000_001);
        // 旧代的 `.state` 留在批次目录里，不参与读取。
        let old_states: Vec<_> = old_key_inventory(&vault)
            .into_iter()
            .filter(|f| f.path.ends_with(STATE_FILE))
            .collect();
        assert!(old_states.is_empty(), "`.state` 不该出现在备份列表里");
        assert!(usage(&vault).1 >= 1);
    }
}
