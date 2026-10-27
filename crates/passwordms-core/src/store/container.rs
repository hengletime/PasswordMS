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
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::constants::FRAME_HEADER_LEN;
use crate::constants::{
    CONTAINER_PREFIX_LEN, FORMAT_VERSION, HEADER_RESERVED_BYTES, MAX_HEADER_BYTES, VAULT_MAGIC,
};
use crate::error::{CoreError, CoreResult};
use crate::model::vault::Vault;
use crate::store::frame::Frame;

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

        if &prefix[0..8] != VAULT_MAGIC {
            return Err(CoreError::Internal("bad magic".to_string()));
        }
        let format_version = u16::from_le_bytes([prefix[8], prefix[9]]);
        if format_version != FORMAT_VERSION {
            return Err(CoreError::Internal("unsupported format".to_string()));
        }
        let header_len = u16::from_le_bytes([prefix[10], prefix[11]]) as usize;
        if header_len == 0 {
            return Err(CoreError::Internal("zero header".to_string()));
        }
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
            file.flush()?;
        }
        fs::rename(&tmp, &self.path)?;
        self.header = header;
        self.header_capacity = new_capacity;
        Ok(true)
    }

    /// 读取全部帧（含墓碑），返回 `(offset, frame)`。
    pub fn read_frames(&self) -> CoreResult<Vec<(u64, Frame)>> {
        let data = fs::read(&self.path)?;
        let start = self.record_start() as usize;
        let mut out = Vec::new();
        if data.len() <= start {
            return Ok(out);
        }
        let mut cursor = start;
        while cursor + FRAME_HEADER_LEN <= data.len() {
            let (frame, consumed) = Frame::decode(&data[cursor..])?;
            out.push((cursor as u64, frame));
            cursor += consumed;
            if consumed == 0 {
                break;
            }
        }
        Ok(out)
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

/// D-55 从备份恢复：校验源文件确为 PasswordMS 保险库备份，然后**字节级**复制到目标路径。
///
/// - 校验：源文件存在、不短于容器前缀（[`CONTAINER_PREFIX_LEN`]）、头部 magic 为
///   [`VAULT_MAGIC`]（"PMSVAULT"）；不符返回 `CoreError::Validation("not_a_vault_backup")`，
///   此时**不得触碰目标文件**。
/// - **整库帧流完整性校验**（[`verify_frame_stream`]）：从记录区起点逐帧解码至恰好
///   EOF，挡住「尾部截断的备份」——否则 restore 成功但记录静默丢失（QA 实证 caseA/
///   caseB）；不符返回 `CoreError::Validation("corrupt_vault_backup")`，不复制。
/// - 复制用 [`fs::copy`]：Windows/Unix 上都会**保留源文件的修改时间**——「备份文件的
///   mtime = 库数据最后写入时刻」是导出（`export_vault`）既有的语义，客户明确要求
///   恢复路径保留一致。
/// - 目标已存在时会被覆盖；调用方（src-tauri `restore_backup` 命令）负责先留
///   `.bak-<unix_ms>` 覆盖保护。
pub fn restore_backup_from(src: &Path, dest: &Path) -> CoreResult<()> {
    let meta = fs::metadata(src)?;
    if meta.len() < CONTAINER_PREFIX_LEN as u64 {
        return Err(CoreError::Validation("not_a_vault_backup".to_string()));
    }
    let mut file = File::open(src)?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if &magic != VAULT_MAGIC {
        return Err(CoreError::Validation("not_a_vault_backup".to_string()));
    }
    // QA 实证加固：仅查 magic 会放行尾部截断的备份（restore Ok → unlock 成功但
    // record_count=0，静默丢数据）。复制前先做整库帧流校验。
    let bytes = fs::read(src)?;
    verify_frame_stream(&bytes)?;
    fs::copy(src, dest)?;
    Ok(())
}
