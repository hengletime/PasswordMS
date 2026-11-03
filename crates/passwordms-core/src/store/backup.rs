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
//! 3. **占用期间暂停轮转**（[`RestoreGuard`] / [`RekeyWindow`]）：靠调用方「记得别删」
//!    守不住，所以把状态放在本模块内部，由 RAII 守卫维护。
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

use crate::error::{CoreError, CoreResult};
use crate::model::now_ms;

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
    /// 换主密码**之前**。
    ///
    /// 用**旧**主密码可开——**这是"忘了新主密码"唯一的真退路**。换密是产品里唯一
    /// 真正不可逆的操作，而 `pre-restore` / `pre-compact` / `pre-import` 这些可挽回的
    /// 操作都拍了前置快照，它没有，这个不一致的方向是错的。
    PreRekey,
    /// 换主密码成功之后。**用新密码可开。**
    ///
    /// 它的价值是"换密过程本身出问题"（重包裹写坏、头部不一致）时能退回。
    /// **它救不了忘记新密码的人**——那要靠 [`EventTag::PreRekey`]。用新密码加密的备份，
    /// 拿新密码去开才有意义，这个逻辑不能写反。
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
            EventTag::PreRekey => "pre-rekey",
            EventTag::PostRekey => "post-rekey",
            EventTag::PreCompact => "pre-compact",
            EventTag::PreImport => "pre-import",
        }
    }
}

/// 全部事件类型（新增变体时**必须**在这里加，`inventory` / `prune` / `parse_event_tag`
/// 都从这里取）。
///
/// 为什么抽成一处：这三处原本各自写一遍 4 项数组，加变体时漏一处就有具体后果——
/// 漏 `inventory` 则界面说不出这一份是什么，漏 `prune` 则它永不被轮转。
/// **"加变体要改四处"是记不住的，"加变体只改这里"才记得住。**
const ALL_EVENT_TAGS: [EventTag; 5] = [
    EventTag::PreRestore,
    EventTag::PreRekey,
    EventTag::PostRekey,
    EventTag::PreCompact,
    EventTag::PreImport,
];

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
    /// 正在换主密码，整体暂停（见 [`RekeyWindow`]）。
    Rekeying,
    /// 源文件不存在（首次创建库时）。
    ///
    /// **必须保留**：它**不**决定恢复入口是否放行——那是 [`restore_safety`] 的事，
    /// 而那条判据**根本不读 `skipped`**，只读"库文件在不在"与"有没有真的产出一份"。
    ///
    /// 它服务的是另一件事：调用方据此知道"这次没备份，是因为源文件不存在"（首次建库），
    /// 而不是"写失败了"。两者要上报的东西完全不同——前者什么都不用做，
    /// 后者要进常驻警告。混成一类会让新建的库一出生就带一条假故障。
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

/// 会真正相撞的两个失败环节名。
///
/// 抽成常量而不是散落字面量：`fail_kind` 的判定依赖这两个名字**完全相等**。写成字面量时，
/// 改一处拼写（`"daily"` → `"day"`）会让规则静默失效——`fail_kind` 从"last 覆盖 daily"
/// 退回"first-wins"，而且不报错、不报警、测试也未必覆盖。**按字面量判的规则，迟早被拼写
/// 改坏。**
const STAGE_DAILY: &str = "daily";
const STAGE_LAST: &str = "last";

impl BackupOutcome {
    /// 记录一次失败的原因与环节，**两者同源、整对替换**。
    ///
    /// ## 唯一会真正相撞的一组：`daily` 与 `last`
    ///
    /// 只有 [`snapshot_before_write`] 会先后记录两次失败，且两者共用同一组 `used`/`quota`。
    /// `dir` 是早退路径（`return out`），`daily`/`last` 在那种调用里根本没跑；事件前缀在
    /// [`snapshot_event`] 里只记一次。所以规则只需要一条：**`last` 覆盖 `daily`，其余
    /// first-wins**。
    ///
    /// 为什么不是更细的优先级表：一张 4 级表会**暗示存在一个不存在的碰撞**——后来的人会
    /// 以为 `dir` 与 `daily` 会同时出现、需要比优先级，然后为这个假想场景加测试或改逻辑。
    /// 注释比代码多说一句，代价就是这样。
    ///
    /// 为什么 `last` 优先：日备份丢的是「回到今天开始时」，`last.bak` 丢的是「退回刚才那次
    /// 改动」——**按丢的最小单位报，不按最先发生的错报**。
    ///
    /// 两条同败的组合都不会指错动作：都因配额时 kind 相同、只差 stage；都因磁盘满时 kind
    /// 是 `NoSpace`，动作仍是"腾磁盘"。**"daily 因空间不足、last 因配额"没有生成路径**
    /// ——daily 的 `make_room` 能过就说明配额够 `src_len`，而 `last` 的净增（`src_len -
    /// last_existing`）只会更小，必然也能过。
    ///
    /// 返回是否采纳，供 [`Self::fail`] 决定日志文本要不要一起写。
    fn fail_kind(&mut self, kind: BackupErrorKind, stage: &str) -> bool {
        let take = match self.error_stage.as_deref() {
            None => true,
            Some(STAGE_DAILY) => stage == STAGE_LAST,
            Some(_) => false,
        };
        if take {
            self.error_kind = Some(kind);
            self.error_stage = Some(stage.to_string());
        }
        take
    }

    /// 记录一次 IO 失败。
    ///
    /// `error`（原始文本）与 `error_kind`/`error_stage` **同进同退**，不是"始终记最后一次"。
    /// 理由不是"它不进界面所以无所谓"，而是**日志行必须自洽**：一条日志写着
    /// `stage=last, kind=quota`，文本却是 `"daily: permission denied"`，它在同时说两件事，
    /// 排障的人会被带偏。**自相矛盾的日志比少一条日志更坏，因为它看起来完整。**
    fn fail(&mut self, stage: &str, e: &std::io::Error) {
        if self.fail_kind(classify_io(e), stage) {
            self.error = Some(format!("{stage}: {e}"));
        }
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
    /// 这一份属于**哪一次换密**归档的批次（`old-key/<epoch-ms>/` 的目录名）。
    ///
    /// 当前代恒为 `None`。这个字段存在的唯一理由是：换两次密码就有两批旧代，
    /// **两批要用两把不同的密码开**。只有扁平列表时界面只能说"用旧主密码打开"，
    /// 而第二次换密之后这句话指哪一把就不确定了——用户按它去试，试错的那一次
    /// 往往就是最后一次。
    ///
    /// **不推断"哪批配哪把密码"**：应用只在换密那一刻见过旧密码，不存它。
    /// 所以界面只能写事实——「{日期} 改密前的回滚点」，日期来自批次目录名。
    ///
    /// **只用于显示与排序，绝不用于定位**：它是对目录名做 `parse::<i64>()` 的产物，
    /// 解析失败就是 `None`。拿它当批次身份会把所有解析不出的批次压成同一批。
    pub batch_ms: Option<i64>,
    /// 批次目录名（`old-key/<dir>/` 的 `dir`）——**批次唯一的身份**。
    ///
    /// 删除、分组一律按它，因为它是目录系统里真实存在的字符串。用解析后的毫秒数
    /// 当身份会出两种错：解析不出的批次被压成 `0`（与真的 0 混作一批），
    /// 以及"删不掉却报成功"——按 `0` 拼出的路径根本不存在，`clear` 返回 0，
    /// 界面于是说"已删除 0 份"而文件一份没动。
    pub batch_dir: Option<String>,
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

/// 备份目录里事件快照的最大时刻（没有则为 0）。
///
/// 只服务一件事：给换密时新写的快照挑一个**不与既有文件重名**的时间戳。
/// 它**不参与**任何"这份属于哪一代"的判断——那个问题由调用方传进来的"换密前文件清单"
/// 回答（见 [`isolate_old_key_backups`]）。
pub fn latest_ms_timestamp(vault_path: &Path) -> i64 {
    inventory(vault_path)
        .iter()
        .filter(|f| f.event.is_some())
        .map(|f| f.created_ms)
        .max()
        .unwrap_or(0)
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

// ── 占用期暂停（core 内部状态，不依赖调用方自觉）──────────────────────────

/// 备份目录被谁占着。
///
/// 两种占用的**后果**不同（恢复怕"正在被读的那份被 prune 删掉"，换密怕"`last.bak`
/// 被原地覆盖后错判世代"），但**要求完全一致：这段时间一个字节都不许写进备份目录**。
/// 所以只做一个注册表——分成两个静态量的话，加第三种占用时必然漏掉其中一个，
/// 而漏掉的那一个正是"守卫形同虚设"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BusyKind {
    Restore,
    Rekey,
}

impl BusyKind {
    fn skip_reason(self) -> SkipReason {
        match self {
            BusyKind::Restore => SkipReason::Restoring,
            BusyKind::Rekey => SkipReason::Rekeying,
        }
    }
}

fn busy_registry() -> &'static Mutex<Vec<(PathBuf, BusyKind)>> {
    static REG: OnceLock<Mutex<Vec<(PathBuf, BusyKind)>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(Vec::new()))
}

/// 这条库当前是否被占用；有则返回对应 [`SkipReason`]。
///
/// **按路径判，不是全局**：全局版在单库应用里行为相同，但两个库同时开着时，
/// 给 A 换密会让 B 的备份静默停摆——而"停摆"没有任何界面症状，只有 [`staleness`]
/// 事后才看得见。既然两个入口（`snapshot_before_write` / `snapshot_event`）都拿得到
/// `vault_path`，就没有理由再退化成全局。
///
/// **只返回第一条匹配**（同一路径上恢复与换密可以同时挂着）。这是刻意的，有两个前提：
/// ① `SkipReason` 只服务诊断读数与日志，**不参与任何决策**——两条占用对行为的后果
/// 完全一致（一律不写）；② 它不是"原因的全部"，只是一句提示。若将来有人要用它做
/// 判断（比如"恢复中才允许 X"），**必须先把这个函数改成返回集合**，
/// 否则就是在拿一条随顺序变化的标签当事实。
fn busy_reason(vault_path: &Path) -> Option<SkipReason> {
    let key = canonical_key(vault_path);
    busy_registry().lock().ok().and_then(|g| {
        g.iter()
            .find(|(p, _)| p == &key)
            .map(|(_, k)| k.skip_reason())
    })
}

/// 批次目录名是否合法（删除入参的唯一防线之一）。
///
/// 只允许单层、常见字符、有限长度，且不许是 `.` / `..`。上限 64 是保守值：
/// 应用自己写下的目录名是毫秒时间戳（13 位），留足余量给将来可能的时间戳格式。
///
/// **它不是唯一防线**：名字合法但目录本身是软链接时，字符检查拦不住。
/// 真正的兜底在 [`clear_old_key_batch`] 里的 `canonicalize` 比对。
fn valid_batch_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

/// 删除类操作的统一闸门：备份目录被占用（恢复 / 换密）时一律拒绝。
///
/// 占用守卫原本只挡**写入**，因为写入会污染世代判定。但**删除比写入更狠**：
/// - 恢复正在读的那份 `.bak` 被用户删掉 → 恢复中途失败，或最坏情况下部分应用；
/// - 换密刚拍下的 `pre-rekey` 被删掉 → 「忘了新密码」这条唯一退路当场消失，
///   而用户此刻的心理活动大概率正是"这些旧东西占地方，清一清"。
///
/// 所以"这段时间一个字节都不许写进备份目录"这条不变量，对删除同样成立——
/// 删除是写入的极端形式。**不能只靠界面挡住**：界面同时开着恢复进度与设置卡片，
/// 谁先被禁用取决于实现顺序，而这里是一条结构性约束。
fn ensure_not_busy(vault_path: &Path) -> CoreResult<()> {
    match busy_reason(vault_path) {
        Some(_) => Err(CoreError::BackupBusy),
        None => Ok(()),
    }
}

/// 占用守卫。`drop` 时释放**一次**占用。
///
/// 允许重复入栈、逐个弹出，而不是"已存在就跳过"：后者在嵌套路径上，
/// 内层 `drop` 会把外层那份占用一起清掉——外层还开着，备份已经恢复轮转。
/// 而这只在嵌套时才出现，正常测试永远跑不到。
struct BusyGuard {
    path: PathBuf,
    kind: BusyKind,
}

impl BusyGuard {
    fn enter(vault_path: &Path, kind: BusyKind) -> Self {
        let path = canonical_key(vault_path);
        if let Ok(mut g) = busy_registry().lock() {
            g.push((path.clone(), kind));
        }
        Self { path, kind }
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = busy_registry().lock() {
            // 按 `(路径, 种类)` 精确出栈：同一路径上恢复与换密可能同时挂着，
            // 只按路径删会删掉别人的那一条，把"谁占着"报错成另一种原因。
            if let Some(i) = g
                .iter()
                .position(|(p, k)| p == &self.path && *k == self.kind)
            {
                g.remove(i);
            }
        }
    }
}

/// 恢复期守卫：构造时暂停全部轮转与快照，`drop` 时恢复。
///
/// 为什么需要它：恢复备份要先读某个备份文件、再整体替换库文件。如果这期间有别的
/// 线程触发轮转，可能把**正在被恢复的那一份**删掉——恢复到一半源文件没了。
/// 而"让调用方记得别删"这种约定守不住（本项目在此类约定上已经踩过坑）。
pub struct RestoreGuard {
    _busy: BusyGuard,
}

impl RestoreGuard {
    /// 进入恢复期。
    pub fn new(vault_path: &Path) -> Self {
        Self {
            _busy: BusyGuard::enter(vault_path, BusyKind::Restore),
        }
    }
}

/// 换密窗口：拍下"换密开始时目录里有什么"，并在整段时间内独占备份目录。
///
/// ## 为什么必须独占，而不是只靠清单
///
/// 隔离旧代的判据是「换密开始那一刻它在不在目录里」。这对**新增文件**是完备的：
/// 窗口内新写的路径不在清单里 → 判"新一代" → 留下。
///
/// 但 `last.bak` 是 [`copy_atomic`] **原地替换**的（`last.bak` → 临时文件 → `rename`
/// 回 `last.bak`，**路径不变**）。窗口内若真发生一次快照，日备份会因
/// `DailyExists` 不产新文件，而 `last.bak` 被覆盖后**路径仍在清单里** → 判"上一代"
/// → 搬进 `old-key/` 并贴上"要用旧主密码开"的标签。可它其实是用**新**密钥写出来的。
///
/// 也就是说：窗口内最可能发生的那一种写入，恰好是清单判据**看不见**的那一种；
/// 而 `inventory()` 还跳过 `.tmp`，连"正在替换"都看不见。
///
/// 所以不是去**检测**覆盖（检测不到），而是让覆盖**不可能发生**：窗口内所有快照入口
/// 一律返回 [`SkipReason::Rekeying`]。窗口只覆盖重包裹那一步（两次 Argon2 都在
/// [`RekeyWindow::open`] 之前做完），代价可以忽略。
///
/// ## 顺序（照抄 [`RestoreGuard`] 的既有约定：`pre-*` 快照必须在守卫之前写）
///
/// ```text
/// snapshot_event(PreRekey)   ← 守卫之外 → 进 `before` 清单 → 会被搬走
/// RekeyWindow::open          ← 拍清单 + 独占
/// update_key_material
/// window.isolate()           ← 搬完即释放占用
/// snapshot_event(PostRekey)  ← 守卫之外 → 不在清单里，且已在扫描之后，不进 `kept`
/// ```
pub struct RekeyWindow {
    vault_path: PathBuf,
    before: Vec<PathBuf>,
    _busy: BusyGuard,
}

impl RekeyWindow {
    /// 必须在 `pre-rekey` 快照**写完之后**、改密钥**之前**调用。
    ///
    /// - 早于 `pre-rekey`：那份快照会落进"窗口内新增"，`kept` 每次换密必响一次，
    ///   报警变成噪音，真出事时没人看；
    /// - 晚于改密钥：清单里已经混进新一代文件，判据从根上失效。
    pub fn open(vault_path: &Path) -> Self {
        let busy = BusyGuard::enter(vault_path, BusyKind::Rekey);
        let before = inventory(vault_path).into_iter().map(|f| f.path).collect();
        Self {
            vault_path: vault_path.to_path_buf(),
            before,
            _busy: busy,
        }
    }

    /// 把清单里的一切搬进 `old-key/<epoch-ms>/`，然后**释放占用**。
    ///
    /// 取 `self` 而不是 `&self`：占用必须在这里结束（`post-rekey` 还要写），
    /// 用 `&self` 就把释放时机交回给调用方了——而"让调用方记得"正是本模块开头
    /// 那条硬约束第 3 点已经否决过的东西。
    pub fn isolate(self) -> IsolateOutcome {
        isolate_old_key_backups(&self.vault_path, now_ms(), &self.before)
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
            let tag = ALL_EVENT_TAGS
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
                        batch_ms: None,
                        batch_dir: None,
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
            batch_ms: None,
            batch_dir: None,
        });
    }
    out.sort_by_key(|f| f.created_ms);
    out
}

/// 当前代是否有任何备份。
///
/// **必须问磁盘，不能从 `.state` 推。** `.state` 是缓存：它可能被删掉（用户清了备份）、
/// 被写坏、或来自旧版本，读出来都会是"没有备份"。而对一个用户身家全在备份里的产品，
/// 「当前没有备份」是最不该误报的一句话——这是用户决定"要不要恢复"的时刻，
/// **假阴性会让他放弃**。宁可多读一次目录。
pub fn has_backups(vault_path: &Path) -> bool {
    !inventory(vault_path).is_empty()
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
    ALL_EVENT_TAGS
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
        // 批次身份只从目录名取：目录名就是 `isolate` 那一刻的 `now_ms`，
        // **是应用自己写下的**，不是从文件 mtime 或内容推断的。
        let batch_dir = batch_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        // 解析出的毫秒只用于显示；解析不出就是 `None`，**不回落成 0**。
        let batch_ms = batch_dir.as_deref().and_then(|s| s.parse::<i64>().ok());
        // 读不出来的批次目录只能 `continue`：本函数返回 `Vec<BackupFile>`，没有错误通道，
        // 硬塞一个假条目会让调用方以为里面有一份名为 `""` 的备份。
        // 代价是整批从界面上消失——所以 [`unreadable_batch_dirs`] 必须把这一批数进去，
        // 否则"界面显示没有旧代、盘上却躺着一批旧密码能开的文件"就又成立了。
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
                batch_ms,
                batch_dir: batch_dir.clone(),
            });
        }
    }
    // 认得出时刻的批次排前面（按时间），认不出的排最后（按目录名稳定排序）。
    out.sort_by_key(|f| {
        (
            f.batch_ms.unwrap_or(i64::MAX),
            f.batch_dir.clone().unwrap_or_default(),
            f.created_ms,
        )
    });
    out
}

/// 一批旧代备份（一次换密产生的一整批）。
///
/// 为什么要有它而不是让界面自己按 `batch_ms` 分组：批次数、每批的份数与体积，
/// 是「我要不要删这一批」的**判断依据**，不该由渲染层现算。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OldKeyBatch {
    /// 批次**身份** = `old-key/<dir>/` 的目录名，也是删除时的入参。
    pub dir_name: String,
    /// 目录名解析出的毫秒时刻，**仅供显示**；解析不出为 `None`。
    pub batch_ms: Option<i64>,
    /// 这一批的份数。
    pub files: usize,
    /// 这一批的字节数。
    pub bytes: u64,
}

/// `old-key/` 下**进不了 [`old_key_batches`]、也没法按名字删**的批次个数。
///
/// 两类原因合成一个数，是刻意的：
/// ① **认不出**——目录名不是合法 UTF-8，`file_name().to_str()` 为 `None`，没有可回传的
///    身份字符串，用户没法按名字删它；
/// ② **读不出**——[`old_key_inventory`] 里 `read_dir` 失败只能 `continue`，于是整批从
///    界面上消失，而里面可能是满的、旧密码能打开。
///
/// **两类必须合成一个数**：用户能做的完全一样（去文件夹里手动处理），拆成两格只是让
/// 下一个人以为其中一类不用管。分不清的两种原因合成一个可行动的数字，好过两个精确但
/// 都被忽略的数字。
///
/// **不许用 `to_string_lossy()` 硬凑一个名字**：非法字节会变成 `U+FFFD`，用户拿着这个
/// 假名字回来删时永远匹配不上，界面会说"批次不存在"——而它明明在盘上。
///
/// `old-key/` 本身读不出来时返回 1 而不是 0：`NotFound` 是"还没有旧代"，返回 0 是对的；
/// 其余错误下 0 会被界面读成"一切正常"。返回 1 是**下界**——目的是让提示出现，不是
/// 报准确个数。宁可说"这里有问题，你自己看一眼"，不要说"没有这个东西"。
pub fn unreadable_batch_dirs(vault_path: &Path) -> usize {
    let entries = match fs::read_dir(old_key_dir(vault_path)) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(_) => return 1,
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name_ok = entry.file_name().to_str().is_some();
        // "读得出目录、但读到某一项就失败"也算读不出：那一批列不全，界面同样会漏东西。
        let read_ok = match fs::read_dir(&path) {
            Ok(items) => items.into_iter().all(|item| item.is_ok()),
            Err(_) => false,
        };
        if !name_ok || !read_ok {
            count += 1;
        }
    }
    count
}

/// 按批次汇总旧代（批内时刻升序）。
pub fn old_key_batches(vault_path: &Path) -> Vec<OldKeyBatch> {
    let mut out: Vec<OldKeyBatch> = Vec::new();
    for f in old_key_inventory(vault_path) {
        // 按**目录名**分组，不按解析出的毫秒数：解析不出的批次也要能看见、能删。
        // 归到 `0` 的写法会把它和真的 0 混成一批，而删除用 `0` 拼出的路径不存在，
        // 结果是"界面显示有一批、点了删除却说删了 0 份、文件一份没动"。
        let Some(dir) = f.batch_dir.clone() else {
            continue;
        };
        match out.iter_mut().find(|b| b.dir_name == dir) {
            Some(b) => {
                b.files += 1;
                b.bytes += f.size_bytes;
            }
            None => out.push(OldKeyBatch {
                dir_name: dir,
                batch_ms: f.batch_ms,
                files: 1,
                bytes: f.size_bytes,
            }),
        }
    }
    out.sort_by(|a, b| {
        a.batch_ms
            .unwrap_or(i64::MAX)
            .cmp(&b.batch_ms.unwrap_or(i64::MAX))
            .then_with(|| a.dir_name.cmp(&b.dir_name))
    });
    out
}

/// 删除**某一批**旧代备份，返回删除的文件数。
///
/// ## 为什么按批，而不是只有"全删"
///
/// 旧代是**单向门**，而"我怕旧密码还能开"这个需求往往只针对**其中一批**——
/// 最原始那把密码可能早已忘了，那一批对用户是死的；而最近一次改密前的副本正是
/// 他最想要的退路。整体清空把一个小需求绑成一个大动作，代价不对称。
///
/// 批次目录不存在时返回 `Ok(0)` 而不是报错：可能刚被另一处删掉，重试不该失败。
pub fn clear_old_key_batch(vault_path: &Path, dir_name: &str) -> CoreResult<ClearOutcome> {
    ensure_not_busy(vault_path)?;
    // 入参是**目录名本身**，不是解析出来的毫秒：调用方从 `old_key_batches()` 原样拿回
    // 什么就传什么，中间不再有一次 parse。这样"界面看到的批次"与"能删的批次"是同一个
    // 字符串，不存在"显示得出、删不掉"。
    //
    // ⚠️ **代价**：`i64` 天生做不了路径穿越，`&str` 可以。这条参数从整数换成字符串
    // 等于把一条**类型级**保证换成了**验证级**保证，强度不同，必须在这里补回来：
    // 下面的循环会对目标目录里所有普通文件逐个 `remove_file`，一旦能跳出 `old-key/`，
    // 它就是在删别人的文件。
    if !valid_batch_name(dir_name) {
        // 名字不合法 = 一次错误的调用，**必须报错**，不能用 `Ok(0)` 伪装成"已经删干净"。
        return Err(CoreError::Validation("bad_old_key_batch".to_string()));
    }
    // **路径只从 `read_dir` 来，绝不从字符串拼**。
    //
    // `old_key_dir(vault).join(user_input)` 是把一条路径交给调用方去写：`"../.."` 能
    // 把 `join` 抬出 `old-key/`，而下面的循环会对目录里**每一个普通文件** `remove_file`。
    // 改成"枚举真实条目 → 只拿名字去比对"之后，穿越在结构上不可能——它压根没有构造
    // 路径的机会，而不是"我们记得检查了"。这是比白名单更强的保证：白名单将来会先失效
    // （目录名换成带 `:` 的 ISO 时间戳完全有可能），这条不会。
    let root = old_key_dir(vault_path);
    // 这里**不能**把任何 `read_dir` 失败都当"没有批次"：权限被拒、句柄耗尽、I/O 错误
    // 都会走到这里，而 `{0,0}` 会让界面说"已删除 0 份"——用户以为清空了。
    // 只有"目录不存在"才等于"没有可删的东西"，其余一律报上去。
    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ClearOutcome::default());
        }
        Err(e) => return Err(CoreError::Io(format!("old-key dir: {e}"))),
    };
    let mut batch_path = None;
    for entry in entries.flatten() {
        // 这里的 `==` **故意严于文件系统**：Windows 上 `Alpha` 与 `alpha` 是同一个目录，
        // 但字符串不等。放宽成 `eq_ignore_ascii_case` 就回到"两个名字映射同一身份"的
        // 老坑——那正是这套改造要消灭的东西。宁可因为大小写不匹配拒绝一次（用户重试
        // 会拿到新清单），也不要让两个批次共用一条删除路径。
        if entry.file_name().to_str() == Some(dir_name) {
            batch_path = Some(entry.path());
            break;
        }
    }
    let Some(batch_path) = batch_path else {
        // 没匹配到 = 这个批次已经不存在了。**重试幂等**正是原注释要的行为：
        // 双击、界面没刷、用户再点一次，第二次不该报错。
        return Ok(ClearOutcome::default());
    };
    if !batch_path.is_dir() {
        return Ok(ClearOutcome::default());
    }
    // 第三道闸：名字合法、条目真实，但它可能是个**软链接**——`read_dir` 会跟进去，
    // 循环就在链接目标里删文件。白名单拦不住这个，`canonicalize` 能。
    let Ok(canon_root) = fs::canonicalize(&root) else {
        return Ok(ClearOutcome::default());
    };
    let Ok(canon_batch) = fs::canonicalize(&batch_path) else {
        return Ok(ClearOutcome::default());
    };
    if canon_batch == canon_root || !canon_batch.starts_with(&canon_root) {
        return Err(CoreError::Validation("bad_old_key_batch".to_string()));
    }
    let mut deleted = 0;
    // 内层这一处曾经漏了：外层已按 `ErrorKind` 区分，内层还是 `if let Ok(..)`。
    // 批次目录读不出来（权限 / 句柄耗尽）→ 循环不执行 → `deleted=0`；而
    // `old_key_inventory` 同样读不到 → `remaining=0` → `{0,0}`，界面会说
    // 「已经没有可删的东西」。那个目录**可能是满的**。
    // 同一个反模式修了外层、内层留着，比两处都错更难发现——diff 看起来是"已修"。
    let items = match fs::read_dir(&batch_path) {
        Ok(items) => items,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // 目录在我们枚举之后消失了：属于重试幂等，继续走剩下的清理与计数。
            return Ok(ClearOutcome::default());
        }
        Err(e) => return Err(CoreError::Io(format!("old-key batch: {e}"))),
    };
    {
        for entry in items.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                // `.state` / `.tmp` 也计数：它们确实被删掉了，但**不该算进"备份份数"**。
                // 所以计数只认 `old_key_inventory` 认得的名字。
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != STATE_FILE
                    && !std::path::Path::new(name)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("tmp"))
                {
                    deleted += 1;
                }
            }
        }
    }
    // 目录空了才删得掉；删不掉就是还有残留，那**不是错误**——`remaining` 会说出来。
    // 旧实现这里 `?` 报错，等于把"删掉 8 份、剩 1 份"报成一次彻底的失败。
    let _ = fs::remove_dir(&batch_path);
    let remaining = old_key_inventory(vault_path)
        .iter()
        .filter(|f| f.batch_dir.as_deref() == Some(dir_name))
        .count();
    Ok(ClearOutcome { deleted, remaining })
}

/// 删除类操作的回执：删掉几份、还剩几份。
///
/// 为什么不能只返回"删了几份"：删除是逐份尝试的，任何一份失败（被占用、杀软锁、
/// 只读）都会留下，而**界面已经把列表按返回值划掉了**。`remaining > 0` 是唯一能让
/// 用户知道"没删干净"的通道——它不说，用户就以为清空了，而那几份旧代仍能被旧密码开。
///
/// 也不返回 `Err`：删了一半是"成功但有残留"，报错会让界面说"删除失败"，而实际删掉了
/// 大部分。两件事必须分开：`Err` = 连枚举都做不到，`remaining > 0` = 做了但没做全。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ClearOutcome {
    /// 成功删除的份数。
    pub deleted: usize,
    /// 操作结束后**仍然存在**的份数（重新枚举得出，不是推算）。
    ///
    /// 三格语义，界面必须分开说：
    /// - `Err(unknown batch)`：名字不合法（真故障，不该被 `0/0` 吞掉）；
    /// - `{0, 0}`：那里已经没有可删的东西（重试幂等，**合法**）；
    /// - `{0, N}`：一份都没删掉，N 份还在（这才是要紧的那格）。
    ///
    /// 不另加 `target_existed` 之类的标注字段：**消除歧义优于标注歧义**——
    /// `{0,0}` 的歧义来自"身份靠字符串拼路径"，那条修掉之后，它只剩一个含义。
    pub remaining: usize,
}

/// 换主密码后隔离旧备份的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IsolateOutcome {
    /// 挪进旧代的份数。
    pub moved: usize,
    /// 因**不在**换密前清单里而被故意留下的份数。
    ///
    /// 正常应为 0。非 0 意味着"在本该没有任何写入的窗口里发生了写入"——写入顺序被破坏，
    /// 或有人绕过了 [`RekeyWindow`] 的独占。留下是对的（那是新一代文件），但必须让调用方看见。
    pub kept: usize,
    /// **没能搬走**的份数（rename 失败：被占用、杀软锁、只读）。
    ///
    /// 这不是统计。搬不走的那份会留在当前代，被 `inventory()` 当成普通日备份/事件快照，
    /// 界面上贴着"当前主密码可开"——而它其实只能用旧密码开。用户忘记新密码时按这个标签
    /// 去开必然失败，而那是他最后一次尝试。`failed > 0` 时界面**必须**说出来。
    pub failed: usize,
    /// 隔离目录（没挪任何东西时为 `None`，不留空目录）。
    pub dir: Option<PathBuf>,
    /// 失败原因。**非空不代表换密失败**——换密已经成功了，只是旧备份没挪走。
    ///
    /// `serde(skip)`：它是原始 `io::Error` 文本（跟系统语言、可能带绝对路径），
    /// 只进日志。**将来接界面时不要顺手加回来**，对外只暴露 [`Self::moved`] 与
    /// 「有没有失败」这个布尔。
    #[serde(skip)]
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
/// ## 判据：`before` 清单，不是时间戳
///
/// `before` 是**换密开始那一刻目录里已有的文件清单**。在清单里 = 属于上一代，搬走；
/// 不在 = 换密窗口内新写的，留下并计入 [`IsolateOutcome::kept`]。
///
/// 为什么不用 `cutoff_ms` 比时间戳（试过，改掉了）：
/// 1. 时间戳是**代理**，而换密前它在不在就是**定义本身**；
/// 2. 时钟回拨期间，`max(now, 目录里最晚时刻 + 1)` 恒定高于任何新写文件的时刻，
///    `kept` 永远是 0——**探测能力恰好在时钟有偏差的那段时间被关掉**，而那正是最该报警
///    的时候；
/// 3. 它逼着给每个文件定时刻由谁算，而 `bak.<yyyyMMdd>` 是日粒度、`last.bak` 只能靠
///    mtime（源库带来的、跨机不准）。定义了判据就得定义一半取整。
///
/// 清单没有这些边界：只问成员关系。目录扫描的内容恰好就是换密前那一刻的备份集合。
///
/// ## 清单看不见的东西（所以还需要 [`RekeyWindow`]）
///
/// 清单对**新增路径**完备，对**原地覆盖**完全无效：`last.bak` 由 [`copy_atomic`]
/// 临时文件 + `rename` 回**同一个路径**替换，窗口内被覆盖一次，路径照样在清单里。
/// 这一条不能靠"再加一个字段"补——覆盖前后路径相同、`inventory()` 又跳过 `.tmp`，
/// 磁盘上没有任何可观察的差异可作判据。所以改成结构排除：窗口内禁止一切写入。
///
/// ## 调用时机
///
/// 生产代码请走 [`RekeyWindow::isolate`]，它保证 `before` 拍在正确的位置、
/// 且在隔离期间独占备份目录。直接调本函数只用于测试。
pub fn isolate_old_key_backups(
    vault_path: &Path,
    now_ms: i64,
    before: &[PathBuf],
) -> IsolateOutcome {
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
    let mut kept = 0usize;
    let mut failed = 0usize;
    for f in files {
        let Some(name) = f.path.file_name().and_then(|n| n.to_str()) else {
            failed += 1;
            continue;
        };
        // 判据只有一条：换密开始那一刻它在不在目录里。
        // 不在 = 这个窗口内新写的（正常只有 `post-rekey`，或有人反序调用），留下。
        if !before.contains(&f.path) {
            kept += 1;
            continue;
        }
        if fs::rename(&f.path, dest_dir.join(name)).is_ok() {
            moved += 1;
        } else {
            // **必须计数**：这份会留在当前代，被 `inventory()` 归为普通日备份/事件快照，
            // 界面上贴着"当前主密码可开"——而它其实只能旧密码开。用户忘记新密码时按这个
            // 标签去开，必然失败，而那是他最后一次尝试。静默吞掉等于给它贴了错标签。
            failed += 1;
        }
    }
    out.moved = moved;
    out.kept = kept;
    out.failed = failed;
    // `kept > 0` 是**异常信号**，不是一个正常的计数：正确顺序下 `isolate` 那一刻磁盘上
    // 不可能有"换密窗口内写入"的文件，[`RekeyWindow`] 还从结构上把写入挡掉了。
    // 真出现了 = 顺序被破坏 / 有人绕过了守卫 / 守卫被提前释放。
    //
    // **不写 `error`**：`error` 只有一个含义——整个隔离动作没做成（连批次目录都没建起来），
    // 命令层据此置 `isolate_error`，回执会说"旧备份没能归档"。而这里其实**全都归档了**，
    // 只是多留了一份没搬。把这句塞进 `error` 等于用一句错话代替一句真话。
    //
    // 也不进界面：用户对"多留了 1 份"既看不出差别、也没有可执行的动作（C-49），
    // 摆出来只会稀释真正的失败提示。所以只留在返回值里给调用方/测试判，加一行日志。
    if out.kept > 0 {
        eprintln!(
            "[backup] isolate: {} 份快照出现在换密窗口内（本不该有任何写入），已保留在当前代",
            out.kept
        );
    }
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

/// 换主密码后的备份处置结果。
///
/// 为什么把两件事分开而不是塞进一个 `error`：它们的回执完全不同，而且**互相不能推断**。
///
/// ## 四种组合，四种说法（少一种就会说错话）
///
/// | `isolate.moved` | `pre_rekey_saved` | 用户手上还剩什么 |
/// |---|---|---|
/// | `> 0` | `true` | 旧代已归档 + 一份换密前快照，都能用旧主密码打开 |
/// | `> 0` | `false` | 旧代已归档（旧密码可开），但没有「换密前最后一刻」那一份 |
/// | `0`   | `true` | 换密前没有旧备份，只有这一份换密前快照 |
/// | `0`   | `false` | **什么都没有**——这才是"当前没有旧的退路" |
///
/// 所以 `pre_rekey_saved == false` **不等于**"没有退路"：已经归档的旧代同样是退路，
/// 只是老一点。把它写成"当前没有旧的退路"会让用户在明明还有救的时候放弃，
/// 而这正是「假保险」的反面——假警报同样会让人下次不信真话。
///
/// 同理 `moved == 0` 也不等于没关系：它可能本来就是零份，也可能是整批没搬。
/// 判「有没有旧代」要问 [`old_key_usage`]，不要从这两个字段推。
///
/// **失败信息必须告诉用户手上还剩什么。**
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RekeyOutcome {
    /// 旧代隔离结果。
    pub isolate: IsolateOutcome,
    /// 换密前那一份库是否成功存下。
    ///
    /// `false` 时只能说"没能额外存下一份换密前的副本"，**不得**宣称"忘了新密码就救不回来"
    /// ——除非 `isolate.moved == 0`（那才是真的什么都没有）。
    pub pre_rekey_saved: bool,
}

/// 删除**旧密钥代**的备份（独立动作，不复用「删除全部备份」）。
///
/// 为什么不合并：旧代可能是用户唯一还记得的那把密码的退路。把它并进「清空备份」，
/// 用户点一次就永久失去那一代——单向门，不能藏在同一个按钮里。
pub fn clear_old_key(vault_path: &Path) -> CoreResult<ClearOutcome> {
    ensure_not_busy(vault_path)?;
    let mut deleted = 0;
    for f in old_key_inventory(vault_path) {
        if fs::remove_file(&f.path).is_ok() {
            deleted += 1;
        }
    }
    // 顺手清掉空批次目录，别在设置页留一堆空文件夹（非空会失败，忽略即可）。
    if let Ok(batches) = fs::read_dir(old_key_dir(vault_path)) {
        for batch in batches.flatten() {
            let batch_path = batch.path();
            // `old_key_inventory` 刻意跳过 `.state`（它不是备份）。不在这里补一刀的话，
            // 批次目录里若残留一份 `.state` 就永远非空、`remove_dir` 必然失败，
            // 「清空旧主密码备份」会变成清不干净。
            let _ = fs::remove_file(batch_path.join(STATE_FILE));
            let _ = fs::remove_file(batch_path.join(format!("{STATE_FILE}.tmp")));
            let _ = fs::remove_dir(&batch_path);
        }
    }
    let _ = fs::remove_dir(old_key_dir(vault_path));
    Ok(ClearOutcome {
        deleted,
        remaining: old_key_inventory(vault_path).len(),
    })
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
    for tag in ALL_EVENT_TAGS {
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
    // 三种"没有失败"要分开处理，不能合并：
    // 1. `last_error_ms == 0` —— 从来没有失败；
    // 2. token 是 `-` —— 写侧明确标注了"无失败"；
    // 3. token 为空 —— **既可能是老格式的正常状态，也可能是被截断的写坏文件**。
    //
    // 第 3 种按 `Other` 处理：把写坏的文件当成"没有失败"，会让一个真实的失败记录
    // 静默消失；而当它是老格式时多报一条 `Other`，代价只是诊断日志里多一行。
    // **两个方向的代价不对称，所以选能看见的那一边。**
    let last_error_kind = if last_error_ms == 0 || token == "-" {
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

/// 落盘状态（**原子写**）。
///
/// 为什么不能直接 `fs::write`：它是先 truncate 再写，进程在两者之间被杀会留下 **0 字节
/// 或半行**的 `.state`。而读侧对坏数据的兜底是"当作全新库"——`daily_ms` 归零、粘性错误
/// 消失，于是备份系统的整段历史被静默抹掉，且**两个方向都可能**：既可能白丢一条失败警告，
/// 也可能立刻多生成一份日备份。这个文件很小、写得很少，值得一次 tmp + rename。
///
/// 顺序：写 tmp → fsync → rename。rename 只保证换名原子，不保证数据落盘。
fn write_state(dir: &Path, st: &BackupState) {
    use std::io::Write;

    let kind = st.last_error_kind.map(|k| k.as_str()).unwrap_or("-");
    let body = format!(
        "{} {} {} {}",
        st.daily_ms, st.last_ms, st.last_error_ms, kind
    );
    let final_path = dir.join(STATE_FILE);
    let tmp = dir.join(format!("{STATE_FILE}.tmp"));

    let Ok(mut file) = fs::File::create(&tmp) else {
        return; // 写不进去不影响数据安全（下次可能多备份一次），故忽略。
    };
    if file.write_all(body.as_bytes()).is_err() || file.sync_all().is_err() {
        let _ = fs::remove_file(&tmp);
        return;
    }
    drop(file);
    tighten_permissions(&tmp);
    if fs::rename(&tmp, &final_path).is_err() {
        let _ = fs::remove_file(&tmp);
    }
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
/// 的，不该告警。只有「库一直在变、备份没跟上」才是真问题——而占用守卫（[`RestoreGuard`] /
/// [`RekeyWindow`]）泄漏会让 [`busy_reason`] 永久有值、所有备份静默停摆，这是唯一能看见它的通道。
pub fn staleness(vault_path: &Path, last_ms: i64) -> Staleness {
    // "有没有备份"**只问磁盘**，不许从 `.state` 推。
    //
    // `.state` 是缓存：它可能被删掉（用户清了备份）、被写坏、或来自没有该字段的旧版本。
    // 从它推会得到"当前没有备份"——这是用户决定"要不要恢复"的时刻，假阴性会让他直接
    // 放弃。而反过来（有文件、时间未知）报 `Unknown` 只是不告警，界面照常列得出文件。
    if !has_backups(vault_path) {
        return Staleness::Never;
    }
    if last_ms == 0 {
        return Staleness::Unknown;
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
/// ## 判据只看 `error_kind`，不按 `SkipReason` 逐一豁免
///
/// 按枚举写白名单需要在 `SkipReason` 每次加值时重新审一遍，漏一个就把用户挡在唯一出路
/// 门外。而这里要回答的问题其实只有一个：**当前有没有可保住的东西**。
///
/// ## "库在不在"问事实，不读 `skipped`
///
/// 早先这里写的是 `skipped == Some(SkipReason::NoSource)`。那是 `snapshot_event` 的一次
/// **观测**，而"库文件在不在"是**可以直接问的事实**——用观测代理事实，等于给下一个人留下
/// 一个可以照着加第二个枚举分支的模板。它当时不锁门（`NoSource` 时 `error_kind` 也是
/// `None`，同样放行），但形态是错的。
pub fn restore_safety(vault_path: &Path, outcome: &BackupOutcome) -> RestoreSafety {
    // 用 `is_file()` 而不是 `exists()`：`exists()` 对**目录**也为真。库路径被同名目录
    // 占住时，`exists()` 会返回 true 并落到 `Clear`——那就"有东西可保住"了吗？没有，
    // 那份备份必然失败，而失败又一次被当作成功放行。`is_file()` 与
    // `snapshot_before_write` / `snapshot_event` 的早退条件（`!vault_path.is_file()`）
    // 是同一个问题、同一个答案，必须一致。
    if !vault_path.is_file() {
        return RestoreSafety::NothingToLose;
    }
    // **判据是"有没有真的产出一份"，不是"有没有报错"**。
    //
    // 只看 `error_kind` 会漏掉一整格：恢复期 `snapshot_event` 会因为
    // `busy_reason` 早退并置 `skipped`，配额早退也是 `skipped`——两种情况下
    // `error_kind` 都是 `None`，却**一份退路都没留下**。按老写法这会被判成
    // `Clear` 直接放行，正好是"没有退路但界面说有"的情形。
    if !outcome.created.is_empty() {
        return RestoreSafety::Clear;
    }
    RestoreSafety::Blocked(outcome.error_kind.unwrap_or(BackupErrorKind::Other))
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
    // 占用期一律不碰备份目录。除了"恢复中不能删正在被读的那份"，这里还挡着换密窗口内
    // 对 `last.bak` 的**原地覆盖**——覆盖后路径不变，隔离判据（`before` 清单）看不见它，
    // 于是新一代的 `last.bak` 会被错搬进旧代并贴上"要用旧主密码开"。见 [`RekeyWindow`]。
    if let Some(reason) = busy_reason(vault_path) {
        out.skipped = Some(reason);
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
                    Err(e) => out.fail(STAGE_DAILY, &e),
                }
            }
            // 配额装不下 → 走**失败**通道，不是"正常跳过"。备份从这一刻起停了。
            // `error` 保持 `None`：这不是 IO 错误，是策略性拒绝，不该有系统报错文本。
            Err(kind) => {
                out.fail_kind(kind, STAGE_DAILY);
            }
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
                    Err(e) => out.fail(STAGE_LAST, &e),
                }
            }
            Err(kind) => {
                out.fail_kind(kind, STAGE_LAST);
            }
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
    //
    // 换密窗口同样一律不写，且这里**必须挡**：窗口内若允许写事件快照，
    // `post-rekey` 之外的任何一个 tag 都会落进"本该没有任何写入"的窗口里。
    // 只挡 [`snapshot_before_write`] 等于没挡——两条入口是并列的，不是包含关系。
    if let Some(reason) = busy_reason(vault_path) {
        out.skipped = Some(reason);
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

/// 删除**当前代**的全部备份，返回删除的文件数。
///
/// 为什么需要：Android 备份目录在应用私有存储里，用户**进不去**（没有文件管理器入口，
/// 也没有 SAF 授权），只能靠应用内提供出口。删完立即刷新占用。
///
/// ## 范围只有当前代（名字里的 all 指的是"当前代里的全部"）
///
/// `backups/old-key/` 一份都不动——那是**另一把密码**的退路，见 [`clear_old_key`]。
/// 这个区别必须在界面上说出来：回执若只写「已删除全部备份」，用户会以为磁盘上再也没有
/// 能用旧主密码打开的库，而实际上还躺着整整一代。**删旧代是另一个动作、另一个按钮、
/// 另一句话。**
///
/// `.state` 一并删掉：它是**这次**备份历史的一部分。留着会让界面在"已删除全部备份"之后
/// 仍然显示"上次备份：昨天 21:14"，用户以为还有东西可恢复。删掉后 [`staleness`] 回到
/// `Never`，文案自然变成「当前没有备份」。
pub fn clear_all(vault_path: &Path) -> CoreResult<ClearOutcome> {
    ensure_not_busy(vault_path)?;
    let files = inventory(vault_path);
    let mut deleted = 0;
    for f in files {
        if fs::remove_file(&f.path).is_ok() {
            deleted += 1;
        }
    }
    let dir = backup_dir(vault_path);
    let _ = fs::remove_file(dir.join(STATE_FILE));
    // 原子写的临时文件一起清：留一份 `.state.tmp` 会让目录"看起来还有东西"，
    // 也会被用户当成残留文件。
    let _ = fs::remove_file(dir.join(format!("{STATE_FILE}.tmp")));
    // 重新枚举，不推算：`deleted` 是"尝试成功"的次数，`remaining` 是"现在还看得见
    // 的份数"。只有后者能回答用户真正关心的问题——清干净了没有。
    Ok(ClearOutcome {
        deleted,
        remaining: inventory(vault_path).len(),
    })
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
        assert_eq!(removed.deleted, 3, "日备份 + last.bak + 事件快照");
        assert_eq!(removed.remaining, 0, "整体删除不该留残留");
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

    /// 当前代目录里已有的文件清单 —— 即 `before` 参数在真实调用里的取值。
    fn paths_now(vault: &Path) -> Vec<PathBuf> {
        inventory(vault).into_iter().map(|f| f.path).collect()
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
    fn 两张同时被配额挡住时报更近的那条退路() {
        let (_root, vault) = temp_vault("quota-both");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        // 缩到 512 → 配额 6144，而保底三份就有 12288，两张都装不下。
        // （缩到 1024 不够：那时 last.bak 净增为 0，裁到保底恰好装得下，它会成功。）
        fs::write(&vault, vec![6u8; 512]).unwrap();

        let out = snapshot_before_write(&vault, base + 5 * 90_000_000 + 700);
        assert_eq!(out.error_kind, Some(BackupErrorKind::QuotaExceeded));
        assert_eq!(
            out.error_stage.as_deref(),
            Some("last"),
            "报 daily 会让用户以为只有今日备份缺失；last.bak 才是他五秒后最想用的那条"
        );
        assert!(out.created.is_empty(), "两张都挡，一份都写不成");
    }

    #[test]
    fn 失败日志文本与环节同源() {
        let (_root, vault) = temp_vault("fail-log");
        let base = 1_793_491_200_000;
        let dir = backup_dir(&vault);
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(dir.join("last.bak")).unwrap();

        let out = snapshot_before_write(&vault, base);
        assert!(out.error.is_some());
        // 日志行必须与 stage/kind 说同一件事。`daily` 成功、`last` 失败时，
        // 文本若还写着 "daily: ..."，排障的人会被带偏。
        assert!(
            out.error.as_deref().unwrap_or("").starts_with("last:"),
            "日志文本与环节不同源：{:?}",
            out.error
        );
        assert_eq!(out.error_stage.as_deref(), Some("last"));
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

        let before = paths_now(&vault);
        let out = isolate_old_key_backups(&vault, base + 460_000_001, &before);
        assert!(out.error.is_none(), "{:?}", out.error);
        assert_eq!(out.moved, 8, "旧备份应全部挪走");
        assert_eq!(out.kept, 0, "清单里的一切都属上一代，一份都不该留");
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
        let before = paths_now(&vault);
        isolate_old_key_backups(&vault, base + 460_000_001, &before);
        snapshot_before_write(&vault, base + 10_000_001);
        assert_eq!(usage(&vault).1, 2, "新一代应有日备份 + last.bak");

        let removed = clear_all(&vault).expect("删除应成功");
        assert_eq!(removed.deleted, 2, "只清当前代");
        assert_eq!(removed.remaining, 0, "当前代必须清空");
        assert_eq!(usage(&vault).1, 0);
        assert_eq!(
            old_key_usage(&vault).1,
            8,
            "旧代是另一把密码的退路，不能被同一个按钮删掉"
        );

        let removed_old = clear_old_key(&vault).expect("删除旧代应成功");
        assert_eq!(removed_old.deleted, 8);
        assert_eq!(removed_old.remaining, 0);
        assert_eq!(old_key_usage(&vault).1, 0);
        assert!(!old_key_dir(&vault).exists(), "空批次目录应被清掉");
    }

    #[test]
    fn 状态文件的畸形输入不会假装成失败() {
        let (_root, vault) = temp_vault("state-parse");
        let dir = backup_dir(&vault);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(STATE_FILE);
        let base = 1_793_491_200_000;

        // 1) 正常写入（无失败）：`-` 是占位符，不是"认不出的原因"。
        //    早先这里会读成 `Some(Other)`，于是"写过任何备份"的库都被判成
        //    "上次备份失败：unknown"。
        write_state(
            &dir,
            &BackupState {
                daily_ms: base,
                last_ms: base,
                ..Default::default()
            },
        );
        let st = state(&vault);
        assert_eq!(st.last_ms, base);
        assert_eq!(st.last_error_kind, None, "`-` 不是失败原因");

        // 2) 有时刻 + 未知 token → 回落 `Other`（**不能**抹成"没有失败"：
        //    失败确实发生过，只是原因来自更新的版本）。
        fs::write(&path, format!("{base} {base} {base} garbage")).unwrap();
        assert_eq!(state(&vault).last_error_kind, Some(BackupErrorKind::Other));

        // 3) 老格式（只有两字段）：没有错误记录。
        fs::write(&path, format!("{base} {base}")).unwrap();
        let st = state(&vault);
        assert_eq!(st.last_error_ms, 0);
        assert_eq!(st.last_error_kind, None);

        // 4) 有时刻但 token 是 `-`：视为无失败。
        fs::write(&path, format!("{base} {base} {base} -")).unwrap();
        assert_eq!(state(&vault).last_error_kind, None);

        // 5) 被截断的半行：不能 panic，也不能编出一个错误。
        fs::write(&path, base.to_string()).unwrap();
        let st = state(&vault);
        assert_eq!(st.last_ms, 0);
        assert_eq!(st.last_error_kind, None);
    }

    #[test]
    fn 换密之后的快照不会被误搬进旧代() {
        let (_root, vault) = temp_vault("isolate-seq");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);

        let rekey_ms = base + 460_000_000;
        // 清单拍在写快照**之前**——这才是 `before` 的定义（换密开始那一刻目录里有什么）。
        let before = paths_now(&vault);
        // 反序调用：先写一份"换密之后"的快照，再执行隔离 —— 正是将来有人把
        // `post-rekey` 提前、或窗口内被别的写入插进来时会发生的事。
        let post = snapshot_event(&vault, EventTag::PostRekey, rekey_ms + 1);
        assert_eq!(post.created.len(), 1);

        let out = isolate_old_key_backups(&vault, rekey_ms + 2, &before);
        assert_eq!(out.kept, 1, "换密之后的快照必须留在当前代");
        assert_eq!(out.failed, 0);
        assert_eq!(out.moved, 8, "更早的 8 份应全部进旧代");
        // `kept > 0` 是异常信号，**不是**"整体没做成"。命令层据此置 `isolate_error`
        // （回执会声称"旧备份没能归档"），而这里其实全都归档了、只是多留了一份。
        // 两件事必须分开：`error` 只表示动作没做成。
        assert!(
            out.error.is_none(),
            "kept > 0 不许污染 error——那会让回执说一句假话"
        );

        let files = inventory(&vault);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].event, Some(EventTag::PostRekey));
        // 关键：它一旦被搬进旧代，界面会告诉用户"这批要用旧主密码开"，
        // 而它其实要用新密码开——用户按提示去开必然失败，那正是他最需要它的时候。
        assert!(
            !old_key_inventory(&vault)
                .iter()
                .any(|f| f.event == Some(EventTag::PostRekey)),
            "post-rekey 被误搬进旧代"
        );
    }

    #[test]
    fn 换密窗口内一条备份都不许写() {
        let (_root, vault) = temp_vault("rekey-window");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        // 换一份内容：否则窗口内即使 `last.bak` 被原地覆盖，字节也一模一样，
        // 下面那条"未被替换"的断言会假绿。长度不变，别扰动配额。
        fs::write(&vault, vec![9u8; 4096]).unwrap();
        let last_path = backup_dir(&vault).join("last.bak");
        let last_before = fs::read(&last_path).expect("fill_slots 应已产生 last.bak");

        // 真实顺序：`pre-rekey` 在守卫之外写（写在守卫里会被直接跳过）。
        let pre = snapshot_event(&vault, EventTag::PreRekey, base + 460_000_000);
        assert_eq!(pre.created.len(), 1, "pre-rekey 必须写得进去");
        assert_eq!(pre.skipped, None);

        let window = RekeyWindow::open(&vault);

        // ① 两条入口都必须被挡。只挡一条等于没挡——`last.bak` 的原地覆盖走的是
        //    `snapshot_before_write`，事件快照走的是另一条，两条是并列关系。
        let blocked = snapshot_before_write(&vault, base + 460_000_100);
        assert_eq!(blocked.skipped, Some(SkipReason::Rekeying));
        assert!(blocked.created.is_empty(), "窗口内不许新建任何备份");
        assert_eq!(
            fs::read(&last_path).expect("last.bak 不该消失"),
            last_before,
            "last.bak 被原地覆盖了：路径集合判据对它完全不可见，只能靠守卫挡住"
        );

        let blocked_ev = snapshot_event(&vault, EventTag::PreCompact, base + 460_000_200);
        assert_eq!(
            blocked_ev.skipped,
            Some(SkipReason::Rekeying),
            "事件快照入口也必须挡（这是第二条入口，不是同一个函数）"
        );
        assert!(blocked_ev.created.is_empty());

        // ② 窗口被守住时，清单里的一切都属上一代，一份都不该留。
        let out = window.isolate();
        assert_eq!(out.kept, 0, "窗口内没有写入 ⇒ kept 必须是 0");
        assert_eq!(out.failed, 0);
        assert_eq!(
            out.moved, 9,
            "5 日备份 + 2 pre-restore + last.bak + pre-rekey"
        );
        assert_eq!(usage(&vault).1, 0, "当前代必须归零");

        // ③ 守卫随 `isolate(self)` 一起释放，`post-rekey` 必须写得进去。
        let post = snapshot_event(&vault, EventTag::PostRekey, base + 460_000_300);
        assert_eq!(post.skipped, None, "守卫没在 isolate 结束时释放");
        assert_eq!(post.created.len(), 1);
        assert_eq!(usage(&vault).1, 1);
    }

    #[test]
    fn 占用期间三个删除入口都被拒且释放后恢复() {
        let (_root, vault) = temp_vault("busy-clear");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        let before = paths_now(&vault);
        isolate_old_key_backups(&vault, base + 460_000_001, &before);
        // 新一代长回一份，才有东西可被"当前代"删。
        snapshot_before_write(&vault, base + 10_000_001);

        let cur_files = inventory(&vault).len();
        let old_files = old_key_usage(&vault).1;
        assert!(cur_files > 0 && old_files > 0, "两侧都得有东西才测得出来");

        let window = RekeyWindow::open(&vault);

        // 三个删除入口必须**全部**被拒。只挡一个等于没挡——恢复读的是当前代的 `.bak`，
        // 换密的退路是当前代的 `pre-rekey`，而旧代的批次目录会在 `isolate` 里被
        // `create_dir_all` 之后当场写入。三条路对应三处，漏一条就是一个洞。
        assert!(
            matches!(clear_all(&vault), Err(CoreError::BackupBusy)),
            "换密窗口内删当前代必须被拒"
        );
        assert!(
            matches!(clear_old_key(&vault), Err(CoreError::BackupBusy)),
            "换密窗口内清空旧代必须被拒"
        );
        assert!(
            matches!(
                clear_old_key_batch(&vault, &(base + 460_000_001).to_string()),
                Err(CoreError::BackupBusy)
            ),
            "换密窗口内按批删除必须被拒"
        );

        // 拒绝必须是**真的没删**：报错但悄悄删掉几份，比不报错更坏。
        assert_eq!(inventory(&vault).len(), cur_files, "当前代被删动了");
        assert_eq!(old_key_usage(&vault).1, old_files, "旧代被删动了");

        // 释放后必须立刻恢复可用：守卫漏释放会把"删除回滚点"永久锁死，
        // 而用户在界面上只会看到一个说不清的错误。
        drop(window);
        let after = clear_all(&vault).expect("释放后应可删除");
        assert_eq!(after.deleted, cur_files, "释放后应删掉当前代全部");
        assert_eq!(after.remaining, 0);
        assert_eq!(inventory(&vault).len(), 0);
    }

    #[test]
    fn 批次名不合法被拒而不是静默成功() {
        let (_root, vault) = temp_vault("bad-batch");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        let before = paths_now(&vault);
        isolate_old_key_backups(&vault, base + 460_000_001, &before);

        let vault_before = fs::read(&vault).expect("库文件应可读");
        let long_name = "x".repeat(65);
        let mut bad = vec!["", ".", "..", "../..", "a/b", "a\\b", long_name.as_str()];
        // 多一层兜底：这个路径真的存在，但它不在 `old-key/` 里。
        bad.push("../passwordms.vault");

        for name in bad {
            match clear_old_key_batch(&vault, name) {
                Err(CoreError::Validation(code)) => {
                    assert_eq!(code, "bad_old_key_batch", "名字 {name:?} 应报 bad_old_key_batch");
                }
                other => panic!("名字 {name:?} 必须被拒，实得 {other:?}"),
            }
        }

        // 最关键的一条：拒绝之后库文件必须原样还在——穿越若生效，它正是会被删掉的那个。
        assert_eq!(
            fs::read(&vault).expect("库文件不该消失"),
            vault_before,
            "路径穿越删到了 old-key/ 之外"
        );
    }

    #[test]
    fn 同一条库上两种占用叠加时逐个释放仍然全程禁止写入() {
        let (_root, vault) = temp_vault("busy-nested");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);

        let is_busy = |r: Option<SkipReason>| {
            matches!(r, Some(SkipReason::Restoring) | Some(SkipReason::Rekeying))
        };

        // 恢复与换密同时挂在同一条库上（嵌套调用或并发命令）。
        let outer = RestoreGuard::new(&vault);
        let inner = RekeyWindow::open(&vault);

        // 只返回一条 reason，但"不写"这件事两条都在保证。
        assert!(
            is_busy(snapshot_before_write(&vault, base + 10_000_001).skipped),
            "两层占用时写入必须被拒"
        );

        // 这一格是"已存在就跳过"那种实现会漏掉的：内层释放把外层一起清掉，
        // 外层还开着、备份已经开始轮转。所以必须逐个弹出。
        drop(inner);
        assert!(
            is_busy(snapshot_before_write(&vault, base + 10_000_002).skipped),
            "内层释放后外层仍在，写入仍须被拒"
        );

        drop(outer);
        assert!(
            !is_busy(snapshot_before_write(&vault, base + 10_000_003).skipped),
            "两层都释放后不该再报占用"
        );
    }

    /// 一个**不是合法 UTF-8**、但文件系统肯接受的名字。
    ///
    /// 用来构造"名字认不出"的批次目录：`to_str()` 必为 `None`，而 `create_dir` 能建出来。
    /// Windows 用未配对的代理项 `0xD800`（NTFS 允许），其它平台用 `0xFF 0xFE` 两个字节。
    #[cfg(windows)]
    fn non_utf8_dir_name() -> std::ffi::OsString {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0xD800])
    }

    #[cfg(not(windows))]
    fn non_utf8_dir_name() -> std::ffi::OsString {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![0xFF, 0xFE])
    }

    #[test]
    fn 认不出或读不出的旧代目录要计数而不是从界面消失() {
        let (_root, vault) = temp_vault("unreadable-batch");
        // 1) 还没有 `old-key/`：0。这是"还没有旧代"，不是"读不出来"。
        assert_eq!(unreadable_batch_dirs(&vault), 0);

        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        let before = paths_now(&vault);
        isolate_old_key_backups(&vault, base + 460_000_001, &before);
        // 2) 正常的数字目录名不该被计数。
        assert_eq!(unreadable_batch_dirs(&vault), 0, "能识别的批次不算这一格");

        // 3) 手工塞一个名字不是合法 UTF-8 的批次目录，里面放一份备份。
        let odd = old_key_dir(&vault).join(non_utf8_dir_name());
        fs::create_dir_all(&odd).unwrap();
        fs::write(odd.join("daily.20260101"), vec![9u8; 64]).unwrap();

        assert_eq!(
            unreadable_batch_dirs(&vault),
            1,
            "认不出的批次必须被数进去——否则界面在它明明占着磁盘时说'没有旧代'"
        );
        // 它进不了可选列表（没有可回传的身份字符串），所以那个计数是界面唯一的信息来源。
        assert_eq!(old_key_batches(&vault).len(), 1, "只认得出那一批数字名的");
        assert_eq!(
            old_key_usage(&vault).1,
            9,
            "它仍然算进占用：8 份 + 这 1 份"
        );

        // 4) 按批删除只动得了有名字的那一批；认不出的那份仍在盘上，计数也必须还在。
        let removed =
            clear_old_key_batch(&vault, &(base + 460_000_001).to_string()).expect("按批删除应成功");
        assert_eq!(removed.deleted, 8);
        assert_eq!(unreadable_batch_dirs(&vault), 1, "删完能删的，这一格仍然要说");
        assert!(odd.exists(), "认不出的批次不该被误删");
    }

    #[test]
    fn 两批旧代各自带批次身份且按批删除只动那一批() {
        let (_root, vault) = temp_vault("old-batch");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);

        let before = paths_now(&vault);
        isolate_old_key_backups(&vault, base + 460_000_001, &before);
        // 新一代长回日备份 + last.bak，才有东西可给第二次换密搬。
        snapshot_before_write(&vault, base + 10_000_001);

        let before2 = paths_now(&vault);
        isolate_old_key_backups(&vault, base + 920_000_001, &before2);

        let batches = old_key_batches(&vault);
        assert_eq!(batches.len(), 2, "两次换密 = 两批，不能压成一批");
        assert_eq!(batches[0].batch_ms, Some(base + 460_000_001));
        assert_eq!(batches[1].batch_ms, Some(base + 920_000_001));
        assert_eq!(batches[0].files, 8);
        assert_eq!(batches[1].files, 2, "第二批只有新一代的日备份 + last.bak");

        // 批次身份必须落到每一份上，否则界面只能给出一句"用旧主密码打开"——
        // 而两次换密之后，那句话指哪一把密码就不确定了。
        assert!(
            old_key_inventory(&vault)
                .iter()
                .all(|f| f.batch_ms.is_some()),
            "旧代每一份都必须带批次身份"
        );
        assert!(
            inventory(&vault).iter().all(|f| f.batch_ms.is_none()),
            "当前代不属于任何批次"
        );

        let removed =
            clear_old_key_batch(&vault, &(base + 920_000_001).to_string()).expect("按批删除应成功");
        assert_eq!(removed.deleted, 2);
        assert_eq!(removed.remaining, 0, "按批删除要把这一批清干净");
        assert_eq!(old_key_batches(&vault).len(), 1, "第一批必须留着");
        assert_eq!(old_key_usage(&vault).1, 8, "只删了这一批");

        // 删第二批不影响第一批仍可被单独删。批次不存在时返回 0 而不是报错。
        let missing = clear_old_key_batch(&vault, "1").expect("不存在的批次不该报错");
        assert_eq!(missing.deleted, 0);
        assert_eq!(missing.remaining, 0);
        let first =
            clear_old_key_batch(&vault, &(base + 460_000_001).to_string()).expect("删第一批应成功");
        assert_eq!(first.deleted, 8);
        assert_eq!(first.remaining, 0);
        assert!(old_key_batches(&vault).is_empty());
    }

    #[test]
    fn 没有旧备份时不留下空隔离目录() {
        let (_root, vault) = temp_vault("isolate-empty");
        let out = isolate_old_key_backups(&vault, 1_793_491_200_001, &[]);
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
    fn 没有备份判从未备份时间异常判未知() {
        let (_root, vault) = temp_vault("stale");
        let now = mtime_ms(&vault).expect("临时库应有 mtime");

        // 一份备份都没有：判「从未备份」，而不是拿 `.state` 里的时刻去算差值。
        assert_eq!(staleness(&vault, 0), Staleness::Never);
        // 下面几条验的是**有备份时**怎么算差值，所以得先真的放一份文件——
        // 否则 `has_backups` 会把它们全部短路成 `Never`，测的就不再是差值逻辑。
        let dir = backup_dir(&vault);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("last.bak"), b"x").unwrap();

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
        assert_eq!(restore_safety(&missing, &out), RestoreSafety::NothingToLose);

        // 2) 正常快照写成 → 放行。
        let ok = snapshot_event(&vault, EventTag::PreRestore, base);
        assert!(ok.error_kind.is_none(), "{:?}", ok.error_kind);
        assert_eq!(restore_safety(&vault, &ok), RestoreSafety::Clear);

        // 2b) **被跳过 ≠ 安全**。恢复期里 `snapshot_event` 会因为占用守卫早退并置
        //     `skipped`，此时 `error_kind` 是 `None`——只按 `error_kind` 判就会把它
        //     算成 `Clear` 放行，而那份"退路"根本没写出来。判据必须是"真的产出了一份"。
        let skipped = BackupOutcome {
            skipped: Some(SkipReason::Restoring),
            ..Default::default()
        };
        assert!(
            matches!(restore_safety(&vault, &skipped), RestoreSafety::Blocked(_)),
            "被跳过的快照不许判成 Clear"
        );

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
            restore_safety(&v2, &blocked),
            RestoreSafety::Blocked(BackupErrorKind::QuotaExceeded)
        );
    }

    #[test]
    fn 新旧两代状态互不干扰() {
        let (_root, vault) = temp_vault("two-gen");
        let base = 1_793_491_200_000;
        fill_slots(&vault, 4096, base);
        let before = paths_now(&vault);
        isolate_old_key_backups(&vault, base + 460_000_001, &before);
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
