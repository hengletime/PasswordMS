//! 单文件容器二进制布局读写（架构文档 §2.6）。
//!
//! 顶层布局：
//! ```text
//! [0..8)     magic "PMSVAULT"
//! [8..10)    format_version u16 LE
//! [10..12)   header_len u16 LE
//! [12..16)   reserved（0）
//! [16..16+header_len)  header_json（UTF-8，尾部空格填充至固定容量）
//! [16+header_len..)    record_area（记录帧序列）
//! ```
//!
//! **头部容量固定**（[`crate::constants::HEADER_RESERVED_BYTES`]）并尾部填充空格，
//! 使 `item_count`/`folder_count` 等计数更新可在原位移位写入，满足 A-02。

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::constants::FRAME_HEADER_LEN;
use crate::constants::{
    CONTAINER_PREFIX_LEN, ENCRYPTION_SCHEME, FORMAT_VERSION, HEADER_RESERVED_BYTES, KDF_ALGORITHM,
    MAX_HEADER_BYTES, VAULT_MAGIC,
};
use crate::error::{CoreError, CoreResult};
use crate::model::vault::Vault;
use crate::store::frame::Frame;

/// 容器前缀的结构缺陷分类。
///
/// 抽成枚举而不是「就地返回错误」：同一条判定要服务**两种错误语汇**——`open` 走
/// `Internal(...)`（库坏了，是运行期内部错误），`restore_backup_from` 走
/// `Validation("…bakcup")`（这是用户交上来的文件不合格）。两边各自负责翻译，判定
/// 只写一遍，才不会有「打开时查、恢复时不查」这种只差一行的漏洞（F-02）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderDefect {
    /// magic 不是 `PMSVAULT`。
    BadMagic,
    /// `format_version` 与当前 `FORMAT_VERSION` 不一致（通常是更新的版本导出的备份）。
    UnsupportedVersion,
    /// `header_len == 0`：容器不可能合法（打开时会被拒，这里必须同样拒）。
    ZeroHeader,
}

/// 校验容器前缀（前 [`CONTAINER_PREFIX_LEN`] 字节），返回头部长度。
fn check_prefix(prefix: &[u8]) -> Result<usize, HeaderDefect> {
    debug_assert!(prefix.len() >= CONTAINER_PREFIX_LEN, "prefix 不足 16 字节");
    if &prefix[0..8] != VAULT_MAGIC {
        return Err(HeaderDefect::BadMagic);
    }
    let format_version = u16::from_le_bytes([prefix[8], prefix[9]]);
    if format_version != FORMAT_VERSION {
        return Err(HeaderDefect::UnsupportedVersion);
    }
    let header_len = u16::from_le_bytes([prefix[10], prefix[11]]) as usize;
    if header_len == 0 {
        return Err(HeaderDefect::ZeroHeader);
    }
    Ok(header_len)
}

/// 头部「能不能打开」的最低条件（F-02）。
///
/// 结构上解析成功不等于能解锁：KDF 算法名不认识、密文/校验位为空，恢复出来的库
/// 一定打不开。这类文件必须在**覆盖当前库之前**被拒——否则用户看到「恢复成功」，
/// 下次解锁才发现进不去（`.bak` 是事后补救，不是事务保证）。
fn verify_header_usable(header: &Vault) -> CoreResult<()> {
    let corrupt = || CoreError::Validation("corrupt_vault_backup".to_string());
    if header.kdf.algorithm != KDF_ALGORITHM || header.encryption_scheme != ENCRYPTION_SCHEME {
        // 算法不认识 = 本版本打不开（不是文件坏了，但同样不能用）。
        return Err(CoreError::Validation(
            "unsupported_vault_backup".to_string(),
        ));
    }
    if header.kdf_salt.is_empty()
        || header.verify_mac.is_empty()
        || header.wrapped_vault_key.ct.is_empty()
        || header.wrapped_vault_key.nonce.is_empty()
    {
        return Err(corrupt());
    }
    Ok(())
}

/// 容器句柄：封装路径、头部与预留容量。
pub struct Container {
    path: PathBuf,
    header: Vault,
    header_capacity: usize,
}

impl Container {
    /// 头部 JSON 起始偏移。
    const HEADER_OFFSET: u64 = CONTAINER_PREFIX_LEN as u64;

    /// 新建容器（若目标已存在返回 [`CoreError::VaultExists`]）。
    pub fn create(path: &Path, header: Vault) -> CoreResult<Self> {
        if path.exists() {
            return Err(CoreError::VaultExists);
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let capacity = HEADER_RESERVED_BYTES;
        let header_bytes = serialize_header_padded(&header, capacity)?;

        let mut file = File::create(path)?;
        file.write_all(VAULT_MAGIC)?;
        file.write_all(&FORMAT_VERSION.to_le_bytes())?;
        file.write_all(&(capacity as u16).to_le_bytes())?;
        file.write_all(&0u32.to_le_bytes())?;
        file.write_all(&header_bytes)?;
        file.flush()?;

        Ok(Self {
            path: path.to_path_buf(),
            header,
            header_capacity: capacity,
        })
    }

    /// 打开已有容器（仅读取头部）。
    pub fn open(path: &Path) -> CoreResult<Self> {
        if !path.exists() {
            return Err(CoreError::NoVault);
        }
        let mut file = File::open(path)?;
        let mut prefix = [0u8; CONTAINER_PREFIX_LEN];
        file.read_exact(&mut prefix)?;

        // F-02：前缀判定与「备份预检」共用 [`check_prefix`]，避免两处条件漂移——
        // 恢复路径曾经只查 magic，于是「能过恢复、打不开库」的文件被放行。
        let header_len = check_prefix(&prefix).map_err(|defect| match defect {
            HeaderDefect::BadMagic => CoreError::Internal("bad magic".to_string()),
            HeaderDefect::UnsupportedVersion => {
                CoreError::Internal("unsupported format".to_string())
            }
            HeaderDefect::ZeroHeader => CoreError::Internal("zero header".to_string()),
        })?;
        let mut header_bytes = vec![0u8; header_len];
        file.read_exact(&mut header_bytes)?;
        let header: Vault = serde_json::from_slice(&header_bytes)?;

        Ok(Self {
            path: path.to_path_buf(),
            header,
            header_capacity: header_len,
        })
    }

    /// 头部引用。
    pub fn header(&self) -> &Vault {
        &self.header
    }

    /// 容器路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 记录区起始偏移。
    pub fn record_start(&self) -> u64 {
        Self::HEADER_OFFSET + self.header_capacity as u64
    }

    /// 当前文件大小（字节）。
    pub fn file_size(&self) -> CoreResult<u64> {
        Ok(fs::metadata(&self.path)?.len())
    }

    /// 原地更新头部（容量不足时回退整文件重写）。
    ///
    /// 返回 `true` 表示发生了整文件重写、记录区偏移已变化，调用方需重新扫描帧偏移。
    pub fn update_header(&mut self, header: Vault) -> CoreResult<bool> {
        // 先尝试原位写入（固定容量 + 空格填充）。
        if let Ok(bytes) = serialize_header_padded(&header, self.header_capacity) {
            let mut file = OpenOptions::new().write(true).open(&self.path)?;
            file.seek(SeekFrom::Start(Self::HEADER_OFFSET))?;
            file.write_all(&bytes)?;
            file.flush()?;
            // 【D-96】原地分支也要真刷盘：对 `std::fs::File` 的 `flush` 是 no-op，
            // 头部数据仍留在 OS page cache。本分支承载着 `change_master_password`
            // 改 `wrapped_vault_key` 的写入——掉电时「文件已存在、VK 却没落盘」
            // 等于把库锁死（主密码换了但包裹密钥还是旧的）。压缩路径的
            // `sync_all` 语义论证见 `rewrite_with_frames` 文档第 2 条。
            file.sync_all()?;
            self.header = header;
            return Ok(false);
        }

        // 容量不足：整文件重写，头部扩为两倍容量（下界为格式默认值，上界见 `MAX_HEADER_BYTES`）。
        //
        // D-78：`header_len` 是 u16，`65536 as u16 == 0`；`open` 把 `header_len == 0`
        // 判为 `zero header` → 库**永久打不开**。坏头在 rename 之前就写进了 tmp，故这条
        // 路径必须「宁可不长，也不写坏」。
        //
        // 下界不是补偿而是不变量：`open` 接受任意 `header_len ≥ 1`，没有下界的畸形库
        // （如 `header_len=100`）会 100→200→400… 一级级爬，**每级都要整文件重写**。
        // 夹取用 `clamp`（等价于先 `.max(下界)` 再 `.min(上界)`）：顺序保证 `cap*2` 冲出
        // u16 时先被压回 65535；两个常量本身满足 下界 ≤ 上界，不会 panic。
        let new_capacity =
            (self.header_capacity * 2).clamp(HEADER_RESERVED_BYTES, MAX_HEADER_BYTES);
        if new_capacity <= self.header_capacity {
            // 已在硬上限：再翻只会被截断成 0。此刻磁盘一字未动，直接拒绝。
            return Err(CoreError::VaultCapacityExceeded);
        }
        // 先序列化并判装得下，再读整库：两条失败路径都不必付 `read_frames()` 的整库读取代价。
        let header_bytes = try_serialize_header_padded(&header, new_capacity)?
            .ok_or(CoreError::VaultCapacityExceeded)?;
        let frames = self.read_frames()?;

        let tmp = self.path.with_extension("vault.tmp");
        {
            let mut file = File::create(&tmp)?;
            file.write_all(VAULT_MAGIC)?;
            file.write_all(&FORMAT_VERSION.to_le_bytes())?;
            file.write_all(&(new_capacity as u16).to_le_bytes())?; // new_capacity ≤ MAX_HEADER_BYTES，此 cast 不会截断
            file.write_all(&0u32.to_le_bytes())?;
            file.write_all(&header_bytes)?;
            for (_, frame) in &frames {
                file.write_all(&frame.encode()?)?;
            }
            // D-164（Kimi 审计 F-04）：与上方就地写头分支、以及 `rewrite_with_frames`
            // 保持同一持久化顺序。只 `flush` 不 `sync_all`，掉电后可能出现
            // 「rename 已可见、内容仍在 page cache」—— 而这条路径**同样承载改主密码**
            // （头部扩容时新 VK 更大），失败代价是库永久锁死，不只是元数据回退。
            file.flush()?;
            file.sync_all()?;
        }
        fs::rename(&tmp, &self.path)?;
        self.header = header;
        self.header_capacity = new_capacity;
        Ok(true)
    }

    /// 读取全部帧（含墓碑），返回 `(offset, frame)`。
    pub fn read_frames(&self) -> CoreResult<Vec<(u64, Frame)>> {
        Ok(self.read_frames_with_tail()?.0)
    }

    /// 读取全部帧（含墓碑），并报告**无法成帧的尾部残料字节数**。
    ///
    /// D-108（豆包审查 3.1-9）：原 `read_frames` 对尾部不足一帧头的残料
    /// （`< FRAME_HEADER_LEN`，典型成因：写入中途掉电 / 备份截断）静默容忍——
    /// 打开路径上与「文件本来就这么长」无从区分。备份恢复路径的
    /// `verify_frame_stream` 早已严格校验，本方法把同样的检查带给正常打开：
    /// 残料字节数 > 0 时由调用方（`rebuild_index` → open_warnings）留痕，不阻断解锁。
    pub fn read_frames_with_tail(&self) -> CoreResult<(Vec<(u64, Frame)>, usize)> {
        let data = fs::read(&self.path)?;
        let start = self.record_start() as usize;
        let mut out = Vec::new();
        let mut trailing = 0usize;
        if data.len() > start {
            let mut cursor = start;
            while cursor + FRAME_HEADER_LEN <= data.len() {
                let (frame, consumed) = Frame::decode(&data[cursor..])?;
                out.push((cursor as u64, frame));
                cursor += consumed;
                if consumed == 0 {
                    // 帧头可读但解码器拒绝消费：剩余全部按残料计。
                    trailing = data.len() - cursor;
                    break;
                }
            }
            if cursor + FRAME_HEADER_LEN > data.len() && cursor < data.len() {
                trailing = data.len() - cursor;
            }
        }
        Ok((out, trailing))
    }

    /// 尾部追加一帧，返回其偏移。
    ///
    /// **D-106（豆包审查 3.1-1）**：`flush()` 在 `std::fs::File` 上是 no-op（数据仍在
    /// OS page cache），掉电会丢最后几秒写入——这里补 `sync_all()` 真正落盘。
    /// 业务写路径（create/update/delete/attach）每帧一次 fsync：本库写频低
    /// （用户交互级），可靠性优先于吞吐。
    pub fn append_frame(&self, frame: &Frame) -> CoreResult<u64> {
        let bytes = frame.encode()?;
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        let offset = file.seek(SeekFrom::End(0))?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        Ok(offset)
    }

    /// 批量追加帧（D-172 导入用）：**全成或全无**。
    ///
    /// 与逐条 [`Self::append_frame`] 的差别不只是性能（万余条导入 = 万余次
    /// `sync_all`），而是失败语义：任一处写入出错就把文件截回写前长度，磁盘上
    /// 一条都不留。逐条追加做不到——中途失败会留下半批已落盘的记录，而调用方
    /// 只看到「导入失败」（Kimi 审计 F-07：界面说失败、库里却已有部分数据）。
    ///
    /// 帧在**打开文件之前**全部编码完成（[`Frame::encode`] 有形状校验，可能失败），
    /// 所以编码类错误发生时磁盘一字未动。截断本身失败时只把两个错误一起报出来，
    /// 不再重试第二遍——把失败原因说清楚，比刷出一个「看起来成功」的状态有用。
    pub fn append_frames(&self, frames: &[Frame]) -> CoreResult<()> {
        let mut encoded: Vec<Vec<u8>> = Vec::with_capacity(frames.len());
        for frame in frames {
            encoded.push(frame.encode()?);
        }
        if encoded.is_empty() {
            return Ok(());
        }

        // 先拿到写前长度：截断必须以它为界，否则回滚会把上一批数据一起切掉。
        let start = self.file_size()?;
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        for bytes in &encoded {
            if let Err(write_err) = file.write_all(bytes) {
                drop(file);
                return Err(match self.truncate_to(start) {
                    Ok(()) => write_err.into(),
                    Err(rollback_err) => CoreError::Internal(format!(
                        "batch append failed ({write_err}); rollback to {start} failed ({rollback_err})"
                    )),
                });
            }
        }
        file.sync_all()?;
        Ok(())
    }

    /// 把文件截到指定长度（D-172 批量写入的回滚原语）。
    ///
    /// 只允许往小里截：`len` 大于当前长度时什么都不做（`set_len` 在 Windows 上会
    /// 用 0 填充延长文件——那会把记录区变成一堆零字节，反而毁库）。
    pub fn truncate_to(&self, len: u64) -> CoreResult<()> {
        if self.file_size()? <= len {
            return Ok(());
        }
        // 重开一个写句柄：append 模式打开的句柄不保证支持改文件长度。
        let file = OpenOptions::new().write(true).open(&self.path)?;
        file.set_len(len)?;
        file.sync_all()?;
        Ok(())
    }

    /// 在指定偏移处覆写一帧（要求字节长度不变）。
    ///
    /// **D-106（豆包审查 3.1-1）**：同 [`Self::append_frame`] 补 `sync_all()`。
    pub fn pwrite_frame(&self, offset: u64, frame: &Frame) -> CoreResult<()> {
        let bytes = frame.encode()?;
        let mut file = OpenOptions::new().write(true).open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }

    /// 以**给定的帧序列**整体重写容器文件（D-76 压缩专用）。
    ///
    /// 与 [`Container::create`] 的差别只有两处：目标文件**已存在**（覆盖写），以及
    /// 头部沿用调用方给的 `header`（压缩要求逐字段保留 `kdf_salt` /
    /// `wrapped_vault_key` / `verify_mac`，否则用户主密码会失效）。
    ///
    /// # 崩溃安全序列（顺序不可调换）
    ///
    /// 1. 写同目录临时文件 `*.vault.compact.tmp`（同卷才能原子 rename）；
    /// 2. **`sync_all()`**（不是 `flush()` —— 对 `std::fs::File` 的 `flush` 是 no-op，
    ///    数据仍在 OS page cache 里。本仓其它写路径历史上只 `flush`，但压缩"替换整库"
    ///    的语义下，rename 落盘而内容没落盘会得到一个残缺的新库，所以这里必须真刷盘）；
    /// 3. **rename 之前**完成全部校验：重新 [`Container::open`] 临时文件（magic /
    ///    version / header_len / 头部 JSON）+ 逐帧解码 + 尺寸与帧数比对；
    /// 4. `fs::rename` 原子替换（Windows 上映射 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`，
    ///    可直接覆盖已存在目标；杀软/索引器瞬时占用会失败，故带退避重试）。
    ///
    /// # 失败时的磁盘状态
    ///
    /// 1～3 步失败：**旧文件一字未动**，临时文件**刻意保留**（它就是完整的候选新库，
    /// 可用作事后诊断或手工恢复；清理掉等于把唯一产物也毁了）。
    /// 第 4 步失败：同上，旧文件仍在。
    ///
    /// 因此本方法要么留下"新库 + 已消费的 tmp"，要么留下"旧库 + 待诊断的 tmp"，
    /// **不存在"两个都残缺"的中间态**。
    pub fn rewrite_with_frames(&mut self, header: Vault, frames: &[Frame]) -> CoreResult<()> {
        if self.header_capacity > MAX_HEADER_BYTES {
            // header_len 是 u16；超过就写不进去（防御：正常路径下容量最多到 65535，
            // 见 `update_header` 的 D-78 夹取）。
            return Err(CoreError::Internal("header_capacity overflow".to_string()));
        }
        let tmp = self.path.with_extension("vault.compact.tmp");
        // 头部必须先按**既有容量**填充满空格，否则 record_start 会与写入的字节数错位。
        let header_bytes = serialize_header_padded(&header, self.header_capacity)?;

        let mut written = CONTAINER_PREFIX_LEN as u64 + self.header_capacity as u64;
        {
            let mut file = File::create(&tmp)?;
            file.write_all(VAULT_MAGIC)?;
            file.write_all(&FORMAT_VERSION.to_le_bytes())?;
            file.write_all(&(self.header_capacity as u16).to_le_bytes())?;
            file.write_all(&0u32.to_le_bytes())?;
            file.write_all(&header_bytes)?;
            for frame in frames {
                let encoded = frame.encode()?;
                file.write_all(&encoded)?;
                written += encoded.len() as u64;
            }
            // 关键：必须是 sync_all，见方法文档第 2 条。
            file.sync_all()?;
        }

        // ── rename 之前把能校验的全校验掉 ──
        let check = Self::open(&tmp)?;
        let actual = check.file_size()?;
        if actual != written {
            return Err(CoreError::Internal(format!(
                "compacted size mismatch: wrote {written}, on disk {actual}"
            )));
        }
        let scanned = check.read_frames()?;
        if scanned.len() != frames.len() {
            return Err(CoreError::Internal(format!(
                "compacted frame count mismatch: expected {}, decoded {}",
                frames.len(),
                scanned.len()
            )));
        }
        drop(check);

        rename_replace(&tmp, &self.path)?;
        self.header = header;
        Ok(())
    }
}

/// `fs::rename` + 退避重试（D-76）。
///
/// Windows 上 `fs::rename` 映射 `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`，**可以**覆盖
/// 已存在目标——所以绝不要先 `remove_file` 再改名（那会打开一个"目标不存在"的崩溃窗口）。
/// 唯一常见的失败原因是目标被外部进程持有不共享的句柄（杀毒实时扫描、Windows Search
/// 索引器、用户正在复制该文件）；本仓自身不长期持有 `File`（每次操作 open→写→drop），
/// 故 3 次退避足够覆盖瞬时占用。仍失败时**保留 tmp 不删**，由调用方记录路径。
fn rename_replace(from: &Path, to: &Path) -> CoreResult<()> {
    const BACKOFF_MS: [u64; 4] = [0, 50, 100, 200];
    let mut last: Option<std::io::Error> = None;
    for &delay in &BACKOFF_MS {
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(err) => last = Some(err),
        }
    }
    Err(match last {
        Some(err) => CoreError::from(err),
        None => CoreError::Internal("rename attempts exhausted".to_string()),
    })
}

/// JSON 序列化并按 `capacity` 尾部空格填充；**装不下返回 `None`**。
///
/// 「装不装得下」的判定**只有这一处**（D-78 裁定）：错误该是什么变体由调用方决定 ——
/// `create` / `rewrite_with_frames` / 原地探测是 `Internal`，而 `update_header` 的
/// 增长分支是 `VaultCapacityExceeded`（前端有 `err.vault_capacity` 人话文案）。
///
/// 刻意不在这里返回错误：若把两个变体的判定分叉到「预检 + 再序列化」两处，将来改填充
/// 规则（不填充、按块对齐等）就会出现「预检说能装、实际装不下」的分叉，而这种分叉只在
/// 冷路径暴露，最难查。
fn try_serialize_header_padded(header: &Vault, capacity: usize) -> CoreResult<Option<Vec<u8>>> {
    let mut bytes = serde_json::to_vec(header)?;
    if bytes.len() > capacity {
        return Ok(None);
    }
    bytes.resize(capacity, b' ');
    Ok(Some(bytes))
}

/// 将头部序列化并尾部填充空格至 `capacity`，超限则报错。
fn serialize_header_padded(header: &Vault, capacity: usize) -> CoreResult<Vec<u8>> {
    try_serialize_header_padded(header, capacity)?.ok_or_else(|| {
        CoreError::Internal(format!("header exceeds reserved capacity ({capacity})"))
    })
}

/// D-55 后续加固：对备份字节流做**整库帧流完整性校验**（只读，不建任何状态）。
///
/// 从记录区起点（`CONTAINER_PREFIX_LEN + header_len`，`header_len` 取自文件
/// `[10..12)` 的 u16 LE）起逐帧 [`Frame::decode`]，要求**恰好消费到文件尾**：
/// - 头部声明的 `header_len` 使记录区起点越过文件尾（头区截断）→ 拒绝；
/// - 任何帧头不足 [`FRAME_HEADER_LEN`]、帧体截断（decode 报 `truncated frame` 等）、
///   或记录区遗留无法成帧的尾料 → 拒绝。
///
/// 上述任一情况返回 `CoreError::Validation("corrupt_vault_backup")`。容器格式**没有**
/// 合法的尾部填充/预留区（空格填充在头部容量内完成，记录区直到 EOF 都是帧），故无放行分支。
/// 背景（QA 实证）：仅查 magic 会放行尾部截断的备份 → `restore` Ok 后 unlock 竟然
/// 成功但 `record_count = 0`（read_frames 对 <124B 尾料静默退出）——静默丢数据。
pub fn verify_frame_stream(bytes: &[u8]) -> CoreResult<()> {
    if bytes.len() < CONTAINER_PREFIX_LEN {
        return Err(CoreError::Validation("corrupt_vault_backup".to_string()));
    }
    let header_len = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
    let record_start = CONTAINER_PREFIX_LEN + header_len;
    if record_start > bytes.len() {
        return Err(CoreError::Validation("corrupt_vault_backup".to_string()));
    }
    let mut cursor = record_start;
    while cursor < bytes.len() {
        // decode 的失败变体是 Internal（truncated frame 等）——在恢复入口一律折叠为
        // Validation("corrupt_vault_backup")，报错语义收敛到「备份文件不完整/损坏」。
        let (frame, consumed) = Frame::decode(&bytes[cursor..])
            .map_err(|_| CoreError::Validation("corrupt_vault_backup".to_string()))?;
        if consumed == 0 {
            return Err(CoreError::Validation("corrupt_vault_backup".to_string()));
        }
        cursor += frame.encoded_len();
    }
    Ok(())
}

/// 备份可用性全检（F-02）：结构 + 头部 + 帧流，一次性做完。
///
/// 与 [`Container::open`] 共用 [`check_prefix`]，并补上 `open` 会拒、旧版恢复却放行的
/// 三项：`header_len == 0`、头部 JSON 解析、头部「可打开性」最低条件
/// （[`verify_header_usable`]）。这里**只读**、不建任何状态——任何一项不过，调用方
/// 都不得触碰目标库。
fn verify_backup_container(bytes: &[u8]) -> CoreResult<()> {
    parse_backup_header(bytes)?;
    verify_frame_stream(bytes)
}

/// 只解析并校验备份的**头部**（magic / 版本 / 头部 JSON / 可打开的最低条件）。
///
/// 与 [`verify_backup_container`] 分开，是因为「这个密码能不能开这份备份」只需要
/// 头部里那几样东西（`kdf_salt` / `kdf` / `verify_mac` / `wrapped_vault_key`），
/// **不需要**把整库帧流解一遍。恢复前的密码预检每按一次按钮就要跑一次，大库上
/// 全量解码纯属浪费。
fn parse_backup_header(bytes: &[u8]) -> CoreResult<Vault> {
    let not_vault = || CoreError::Validation("not_a_vault_backup".to_string());
    let corrupt = || CoreError::Validation("corrupt_vault_backup".to_string());
    if bytes.len() < CONTAINER_PREFIX_LEN {
        return Err(not_vault());
    }
    let header_len = match check_prefix(&bytes[..CONTAINER_PREFIX_LEN]) {
        Ok(len) => len,
        Err(HeaderDefect::BadMagic) => return Err(not_vault()),
        Err(HeaderDefect::UnsupportedVersion) => {
            return Err(CoreError::Validation(
                "unsupported_vault_backup".to_string(),
            ))
        }
        Err(HeaderDefect::ZeroHeader) => return Err(corrupt()),
    };
    let record_start = CONTAINER_PREFIX_LEN + header_len;
    if record_start > bytes.len() {
        return Err(corrupt());
    }
    let header: Vault = serde_json::from_slice(&bytes[CONTAINER_PREFIX_LEN..record_start])
        .map_err(|_| corrupt())?;
    verify_header_usable(&header)?;
    Ok(header)
}

/// 用密码校验一份备份：**只回答"这个密码能不能开这份备份"，绝不碰目标库**。
///
/// 为什么必须有这一步：恢复是**整容器覆盖**（[`restore_backup_from`] 把源文件
/// `rename` 到目标），覆盖发生之后才发现密码不对就已经太晚——原库被换掉，用户被
/// 挡在门外，只能靠同目录那份 `passwordms.vault.bak-<ms>` 手救，而界面上没有任何
/// 入口指向它。所以恢复流程必须**先**问这一句，答"不"时一个字节都不动。
///
/// 与 [`verify_backup_container`] 的分工：那条验结构（是不是本产品的备份、帧流完整
/// 不完整），这条验**密码**。两条都要过才能安全覆盖。
pub fn verify_backup_password(src: &Path, master_password: &str) -> CoreResult<()> {
    let bytes = fs::read(src)?;
    let header = parse_backup_header(&bytes)?;
    // 帧流也要验：一份密码能开、帧却坏掉的备份，恢复过去就是一份坏库。
    verify_frame_stream(&bytes)?;
    let mk = crate::crypto::kdf::derive_master_key(master_password, &header.kdf_salt, &header.kdf)?;
    let kek = crate::crypto::kdf::derive_kek(&mk[..])?;
    let verify_key = crate::crypto::kdf::derive_verify_key(&mk[..])?;
    // 失败一律 `WrongMasterPassword`：调用方是已解锁/已认证的本地界面（锁屏页），
    // 且用户手里本来就有这个文件——这里不存在 A-05 那种"未认证调用者可区分解锁失败"
    // 的侧信道问题，含糊其辞只会让人不知道是自己密码打错还是文件坏了。
    crate::crypto::verify::unlock_check(
        &kek,
        &verify_key,
        &header.verify_mac,
        &header.wrapped_vault_key,
    )
    .map_err(|_| CoreError::WrongMasterPassword)?;
    Ok(())
}

/// 候选文件的回读复核（F-03）：长度 + 逐块字节相等。
///
/// **两侧都从磁盘读**，而不是拿内存里的 `bytes` 比：要证明的是「写进去的东西和源
/// 一致」，拿内存副本比只能证明「源没变」。分块比较是为了不把库再整份读进内存
/// （`fs::read` 已经占了一份，大库上再翻倍会顶到移动端内存上限）。
fn files_equal(a: &Path, b: &Path) -> CoreResult<bool> {
    if fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let mut fa = BufReader::new(File::open(a)?);
    let mut fb = BufReader::new(File::open(b)?);
    let mut buf_a = vec![0u8; 64 * 1024];
    let mut buf_b = vec![0u8; 64 * 1024];
    loop {
        let read = fa.read(&mut buf_a)?;
        if read == 0 {
            // a 到尾：b 也必须同时到尾（长度已比过，这里兜住并发增长）。
            return Ok(fb.read(&mut buf_b)? == 0);
        }
        fb.read_exact(&mut buf_b[..read])?;
        if buf_a[..read] != buf_b[..read] {
            return Ok(false);
        }
    }
}

/// 恢复候选文件路径：必须与目标**同目录**，`rename` 才是同卷原子移位。
fn restore_temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "passwordms.vault".to_string());
    dest.with_file_name(format!("{name}.restore-tmp"))
}

/// D-55 从备份恢复：校验源文件确为**可解锁的** PasswordMS 保险库备份，然后原子替换目标。
///
/// - 校验（[`verify_backup_container`]）：源文件存在、不短于容器前缀、magic 为
///   [`VAULT_MAGIC`]、`format_version` 受支持、`header_len` 非零、头部 JSON 可解析为
///   [`Vault`] 且具备可打开的最低条件、整库帧流从记录区起点恰好解码到 EOF。
///   magic / 长度不符 → `not_a_vault_backup`；版本不受支持 → `unsupported_vault_backup`；
///   其余结构问题 → `corrupt_vault_backup`。**任何一项不过都不触碰目标文件**。
/// - **原子替换（F-03）**：先写同目录候选 `*.restore-tmp`，回读逐字节复核并 `sync_all`，
///   最后 `rename`。旧实现 `fs::copy(src, dest)` 直接覆盖——复制中途失败会让当前库
///   停在「半份旧 + 半份新」，而调用方的 `.bak` 只是事后补救，不是事务保证。
/// - `fs::copy` 负责候选文件：Windows 上它会连文件时间一起复制，「备份文件的 mtime =
///   库数据最后写入时刻」这条导出语义（客户明确要求）因此在恢复后依旧成立。
/// - 目标已存在时会被覆盖；调用方（src-tauri `restore_backup` 命令）仍负责先留
///   `.bak-<unix_ms>` 覆盖保护（用户反悔的后路，与原子性无关）。
pub fn restore_backup_from(src: &Path, dest: &Path) -> CoreResult<()> {
    let bytes = fs::read(src)?;
    verify_backup_container(&bytes)?;

    let tmp = restore_temp_path(dest);
    // 上次中断留下的候选必然是半成品：`rename` 是最后一步，成功就不会留下它。
    let _ = fs::remove_file(&tmp);
    let outcome = (|| -> CoreResult<()> {
        fs::copy(src, &tmp)?;
        if !files_equal(src, &tmp)? {
            return Err(CoreError::Validation("corrupt_vault_backup".to_string()));
        }
        // 先刷盘再改名：只 rename 不 sync，掉电会得到「名字已换、内容还在 page cache」
        // 的半份库（与 `update_header` 扩容分支、`rewrite_with_frames` 同一论证）。
        OpenOptions::new().write(true).open(&tmp)?.sync_all()?;
        fs::rename(&tmp, dest)?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    outcome
}
