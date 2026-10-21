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

use crate::constants::FRAME_HEADER_LEN;
use crate::constants::{CONTAINER_PREFIX_LEN, FORMAT_VERSION, HEADER_RESERVED_BYTES, VAULT_MAGIC};
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
            self.header = header;
            return Ok(false);
        }

        // 容量不足：整文件重写，头部扩为两倍容量。
        let frames = self.read_frames()?;
        let new_capacity = (self.header_capacity * 2).max(HEADER_RESERVED_BYTES);
        let header_bytes = serialize_header_padded(&header, new_capacity)?;

        let tmp = self.path.with_extension("vault.tmp");
        {
            let mut file = File::create(&tmp)?;
            file.write_all(VAULT_MAGIC)?;
            file.write_all(&FORMAT_VERSION.to_le_bytes())?;
            file.write_all(&(new_capacity as u16).to_le_bytes())?;
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
    pub fn append_frame(&self, frame: &Frame) -> CoreResult<u64> {
        let bytes = frame.encode()?;
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        let offset = file.seek(SeekFrom::End(0))?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(offset)
    }

    /// 在指定偏移处覆写一帧（要求字节长度不变）。
    pub fn pwrite_frame(&self, offset: u64, frame: &Frame) -> CoreResult<()> {
        let bytes = frame.encode()?;
        let mut file = OpenOptions::new().write(true).open(&self.path)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }
}

/// 将头部序列化并尾部填充空格至 `capacity`，超限则报错。
fn serialize_header_padded(header: &Vault, capacity: usize) -> CoreResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec(header)?;
    if bytes.len() > capacity {
        return Err(CoreError::Internal(format!(
            "header exceeds reserved capacity ({})",
            capacity
        )));
    }
    while bytes.len() < capacity {
        bytes.push(b' ');
    }
    Ok(bytes)
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
