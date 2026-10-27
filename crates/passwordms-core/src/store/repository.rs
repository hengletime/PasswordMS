//! 保险库仓储：条目 / 分组 / 回收站 / 冲突 / 设置 / 附件 的 CRUD（架构文档 §4.3、§6.③）。
//!
//! 逐条加密：每条记录使用独立 IK，IK 由 VK 包裹并随帧存储；修改单条仅重写该帧
//! （长度不变则原位移位写入），未变帧密文完全复用 → 满足 A-02。

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{
    ATTACHMENT_CHUNK_BYTES, FLAG_BLOB, MAX_ATTACHMENTS_PER_ITEM, MAX_ATTACHMENT_BYTES,
    MAX_ATTACHMENT_FILENAME_BYTES, MAX_ITEM_PLAINTEXT_BYTES, MAX_NOTE_BYTES, MAX_VAULT_BYTES,
    MS_PER_DAY, RECORD_TYPE_CONFLICT, RECORD_TYPE_DEVICE, RECORD_TYPE_FOLDER, RECORD_TYPE_ITEM,
    RECORD_TYPE_SYNC_STATE, RECORD_TYPE_TRASH, TRASH_RETENTION_DAYS, USAGE_THROTTLE_MS,
};
use crate::crypto::keys::{self, VaultKey};
use crate::crypto::{aead, rng};
use crate::error::{CoreError, CoreResult};
use crate::model::conflict::{ConflictChoice, ConflictEntry};
use crate::model::folder::Folder;
use crate::model::item::{AttachmentMeta, Item, ItemKind};
use crate::model::now_ms;
use crate::model::sync_state::{DeviceSyncState, PairedDevice};
use crate::model::trash::TrashEntry;
use crate::model::vault::{Vault, VaultSettings, WrappedKey};
use crate::store::container::Container;
use crate::store::frame::{bytes_to_record_id, record_id_to_bytes, Frame};
use crate::sync::protocol::ChangeRecord;
use zeroize::Zeroizing;

/// 删除分组时其下条目的处置策略（前端二次确认后传入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderDeleteStrategy {
    /// 一并进回收站。
    ToTrash,
    /// 移至「未分组」。
    ToUngrouped,
}

/// 新建/更新条目草稿。
#[derive(Debug, Clone)]
pub struct ItemDraft {
    pub kind: ItemKind,
    pub folder_id: Option<String>,
    pub favorite: bool,
}

/// blob 帧写入结果（D-53：幂等补齐，不做 LWW 覆盖——blob 内容写后不可变）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobWriteOutcome {
    /// 本地已有字节完全一致的 blob 帧：跳过（不重传不重写）。
    Skipped,
    /// 本地已有但字节不同：**保留本地**（异常路径，调用方计入 missing 并留审计日志）。
    KeptLocal,
    /// 本地没有：追加入库。
    Appended,
}

/// 容器体积构成（D-76 §统计，**只读**，不改动任何字节）。
///
/// 为什么单独有这个类型：主人问的是「保险库文件有那么大吗？是不是被删除的附件都还
/// 保存在库里」——这个问题**不需要**压缩就能回答，只要把「活跃字节 / 墓碑字节 / 其中
/// 已删附件」分开报出来。故它是零风险的第一期能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionStats {
    /// 容器文件当前字节数。
    pub file_bytes: u64,
    /// 头部区字节数（`16 + header_capacity`，压缩后不变）。
    pub header_bytes: u64,
    /// 会被保留的**非墓碑**帧字节数（含活跃 blob）。
    pub live_bytes: u64,
    /// 全部墓碑帧字节数（不论去留）——用于外部独立核对「死数据 = 多少」。
    pub tombstone_bytes: u64,
    /// 其中**必须保留**的墓碑字节数（每个已死 ITEM/FOLDER 只留 rev 最高的一条）。
    pub kept_tombstone_bytes: u64,
    /// 压缩能回收的字节数（= 文件现有字节 - 头部 - 保留帧）。
    pub reclaimable_bytes: u64,
    /// 压缩后预计的文件字节数。
    pub after_bytes: u64,
    /// 其中「已删/无引用附件密文」字节数——回答「删掉的附件还在不在」。
    pub attachment_dead_bytes: u64,
    /// 活跃附件密文字节数。
    pub attachment_live_bytes: u64,
    /// 保留帧数。
    pub live_frames: u32,
    /// 将被丢弃的帧数。
    pub dead_frames: u32,
}

/// 一次压缩的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionOutcome {
    /// 压缩前文件字节数。
    pub before_bytes: u64,
    /// 压缩后文件字节数。
    pub after_bytes: u64,
    /// 实际回收字节数。
    pub reclaimed_bytes: u64,
    /// 丢弃的帧数。
    pub dropped_frames: u32,
}

/// 组内条目的频度聚合值（#531：分组排序与 `FolderDto` 聚合字段共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FolderUsage {
    /// 组内条目 `use_count` 之和。
    pub use_count: i64,
    /// 组内条目 `last_used_at` 的最大值（组内无条目或全部为 `None` 时为 `None`）。
    pub last_used_at: Option<i64>,
}

/// 压缩规划：保留哪些 slot、能省多少（[`Repository::plan_compaction`] 的产物）。
struct CompactionPlan {
    /// 保留帧在 `slots` 中的下标（按原顺序，即写入顺序）。
    keep: Vec<usize>,
    live_bytes: u64,
    tombstone_bytes: u64,
    kept_tombstone_bytes: u64,
    attachment_live_bytes: u64,
    attachment_dead_bytes: u64,
    live_frames: u32,
    dead_frames: u32,
    reclaimable_bytes: u64,
    after_bytes: u64,
}

/// 压缩前后用于逐项比对的计数快照（D-76：防止「结构合法但缺项」的静默丢数据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RepositoryCounts {
    items: usize,
    folders: usize,
    trash: usize,
    conflicts: usize,
    devices: usize,
    sync_states: usize,
    blobs: usize,
}

/// 磁盘上的一个帧槽位。
#[derive(Debug, Clone)]
struct Slot {
    offset: u64,
    frame: Frame,
}

/// 保险库仓储。
pub struct Repository {
    container: Container,
    vault_key: VaultKey,
    device_id: String,
    slots: Vec<Slot>,
    /// 活跃（非墓碑、非 blob）记录：`(record_type, record_id) -> slot 下标`。
    live: HashMap<(u8, String), usize>,
    /// 附件密文 blob：`record_id -> slot 下标`。
    blobs: HashMap<String, usize>,
    folders: BTreeMap<String, Folder>,
    items: BTreeMap<String, Item>,
    trash: BTreeMap<String, TrashEntry>,
    conflicts: BTreeMap<String, ConflictEntry>,
    devices: BTreeMap<String, PairedDevice>,
    sync_states: BTreeMap<String, DeviceSyncState>,
    /// 【D-96】打开期一致性检查的警告（如头部计数与实际扫描不符）。
    /// core 无日志设施，由命令层（`vault_cmd::unlock`）取走写入文件日志，
    /// 与 `Session::unlock` 的 `maintenance_warnings` 同通道。
    open_warnings: Vec<String>,
}

impl Repository {
    /// 新建空库并返回仓储。
    pub fn create(
        path: &Path,
        header: Vault,
        vault_key: VaultKey,
        device_id: String,
    ) -> CoreResult<Self> {
        let container = Container::create(path, header)?;
        Ok(Self {
            container,
            vault_key,
            device_id,
            slots: Vec::new(),
            live: HashMap::new(),
            blobs: HashMap::new(),
            folders: BTreeMap::new(),
            items: BTreeMap::new(),
            trash: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            devices: BTreeMap::new(),
            sync_states: BTreeMap::new(),
            open_warnings: Vec::new(),
        })
    }

    /// 打开已有库并解密全部记录。
    pub fn open(path: &Path, vault_key: VaultKey, device_id: String) -> CoreResult<Self> {
        let container = Container::open(path)?;
        let mut repo = Self {
            container,
            vault_key,
            device_id,
            slots: Vec::new(),
            live: HashMap::new(),
            blobs: HashMap::new(),
            folders: BTreeMap::new(),
            items: BTreeMap::new(),
            trash: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            devices: BTreeMap::new(),
            sync_states: BTreeMap::new(),
            open_warnings: Vec::new(),
        };
        repo.rebuild_index()?;
        // 【D-96】头部计数交叉核对：`item_count`（persist_header 维护的活条目数）
        // 与 rebuild_index 实际解密扫描到的条目数比对。不一致**不阻断解锁**（业务
        // 数据以实际扫描为准——帧密文才是真相源），只产生警告走 maintenance_warnings
        // 通道留痕。不一致的典型成因：写入路径漏调 persist_header、头部区损坏后
        // 原地重写丢计数、历史版本缺陷的存量库。
        let header_count = repo.container.header().item_count;
        let actual_count = repo.items.len() as u32;
        if header_count != actual_count {
            repo.open_warnings.push(format!(
                "header_item_count_mismatch: header={header_count}, actual={actual_count}"
            ));
        }
        Ok(repo)
    }

    /// 【D-96】取走打开期一致性警告（供命令层写入文件日志，取后清空）。
    pub fn take_open_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.open_warnings)
    }

    /// 头部引用。
    pub fn header(&self) -> &Vault {
        self.container.header()
    }

    /// 读取自动锁定档位（分钟，D-61：`Vault.auto_lock_minutes` 明文头部字段）。
    pub fn auto_lock_minutes(&self) -> u16 {
        self.container.header().auto_lock_minutes
    }

    /// 设置自动锁定档位（D-61 A 案：闲置达档位即锁定）。
    ///
    /// - 仅接受 [`crate::constants::AUTO_LOCK_OPTIONS`] 内的档位，否则 `ERR_VALIDATION`；
    /// - 幂等：与当前值相同直接返回 `Ok`（不重写头部）；
    /// - 仅改写头部 `auto_lock_minutes` 并 `update_header`，**不走 [`Repository::persist_header`]**
    ///   （后者会一并刷新 `item_count` 等计数，语义不符）；记录区若被连带重写
    ///   （`update_header` 返回 rewrote）则重建索引（业务数据仍在内存，无需重解密）。
    pub fn set_auto_lock_minutes(&mut self, minutes: u16) -> CoreResult<()> {
        if !crate::constants::AUTO_LOCK_OPTIONS.contains(&minutes) {
            return Err(CoreError::Validation(format!(
                "invalid_auto_lock_minutes: {minutes}"
            )));
        }
        if self.container.header().auto_lock_minutes == minutes {
            return Ok(());
        }
        let mut header = self.container.header().clone();
        header.auto_lock_minutes = minutes;
        header.updated_at = now_ms();
        let rewrote = self.container.update_header(header)?;
        if rewrote {
            self.rebuild_index()?;
        }
        Ok(())
    }

    /// 容器路径。
    pub fn path(&self) -> std::path::PathBuf {
        self.container.path().to_path_buf()
    }

    /// 当前库文件大小。
    pub fn file_size(&self) -> CoreResult<u64> {
        self.container.file_size()
    }

    /// 本机设备 ID。
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// VK 引用（供同步使用）。
    pub fn vault_key(&self) -> &VaultKey {
        &self.vault_key
    }

    /// 替换 VK（仅限本地库为空时采用 Host 的 VK，见 U1/U2 裁定）。
    ///
    /// 调用方须先确认 [`Repository::record_count`] 为 0；否则解不出既有密文。
    pub fn replace_vault_key(&mut self, vault_key: VaultKey) {
        self.vault_key = vault_key;
    }

    /// 重新扫描帧，重建 slot 与索引（不重新解密业务数据）。
    fn rebuild_index(&mut self) -> CoreResult<()> {
        self.slots.clear();
        self.live.clear();
        self.blobs.clear();
        self.folders.clear();
        self.items.clear();
        self.trash.clear();
        self.conflicts.clear();
        self.devices.clear();
        self.sync_states.clear();

        let frames = self.container.read_frames()?;
        for (offset, frame) in frames {
            let idx = self.slots.len();
            if frame.is_blob() {
                if !frame.is_tombstone() {
                    self.blobs.insert(frame.record_id_string(), idx);
                }
                self.slots.push(Slot { offset, frame });
                continue;
            }
            if frame.is_tombstone() {
                self.slots.push(Slot { offset, frame });
                continue;
            }

            // 解密并按记录类型归位。
            let pt = self.open_frame(&frame)?;
            match frame.record_type {
                RECORD_TYPE_FOLDER => {
                    let folder: Folder = serde_json::from_slice(&pt)?;
                    self.folders.insert(folder.folder_id.clone(), folder);
                }
                RECORD_TYPE_ITEM => {
                    let item: Item = serde_json::from_slice(&pt)?;
                    self.items.insert(item.item_id.clone(), item);
                }
                RECORD_TYPE_TRASH => {
                    let entry: TrashEntry = serde_json::from_slice(&pt)?;
                    self.trash.insert(entry.trash_id.clone(), entry);
                }
                RECORD_TYPE_CONFLICT => {
                    let entry: ConflictEntry = serde_json::from_slice(&pt)?;
                    self.conflicts.insert(entry.conflict_id.clone(), entry);
                }
                RECORD_TYPE_DEVICE => {
                    let device: PairedDevice = serde_json::from_slice(&pt)?;
                    self.devices.insert(device.device_id.clone(), device);
                }
                RECORD_TYPE_SYNC_STATE => {
                    let state: DeviceSyncState = serde_json::from_slice(&pt)?;
                    self.sync_states.insert(state.device_id.clone(), state);
                }
                _ => {}
            }
            let id = frame.record_id_string();
            self.live.insert((frame.record_type, id), idx);
            self.slots.push(Slot { offset, frame });
        }
        Ok(())
    }

    // ===== 记录读写原语 =====

    /// 用独立 IK 加密 payload 并组装帧。
    fn build_frame(
        &self,
        record_type: u8,
        record_id: &[u8; 16],
        flags: u8,
        payload: &[u8],
    ) -> CoreResult<Frame> {
        let aad = keys::record_aad(record_type, record_id);
        let ik = rng::random_key()?;
        let wrapped_key = keys::wrap_item_key(&self.vault_key, &ik, &aad)?;
        let nonce = rng::random_nonce()?;
        let ct = aead::seal(&ik, &nonce, &aad, payload)?;
        Ok(Frame {
            record_type,
            flags,
            record_id: *record_id,
            wrapped_key,
            nonce,
            plaintext_len: payload.len() as u32,
            ct,
        })
    }

    /// 解密一帧的 payload。
    fn open_frame(&self, frame: &Frame) -> CoreResult<zeroize::Zeroizing<Vec<u8>>> {
        let aad = keys::record_aad(frame.record_type, &frame.record_id);
        let ik = keys::unwrap_item_key(&self.vault_key, &frame.wrapped_key, &aad)?;
        aead::open(&ik[..], &frame.nonce, &aad, &frame.ct)
    }

    /// 写入（新增或更新）一条记录。
    fn write_record(
        &mut self,
        record_type: u8,
        id: &str,
        flags: u8,
        payload: &[u8],
    ) -> CoreResult<()> {
        let rid = record_id_to_bytes(id)?;
        let frame = self.build_frame(record_type, &rid, flags, payload)?;

        if flags & FLAG_BLOB != 0 {
            match self.blobs.get(id).copied() {
                Some(idx) => {
                    let new_idx = self.place_frame(idx, frame)?;
                    self.blobs.insert(id.to_string(), new_idx);
                }
                None => {
                    let offset = self.container.append_frame(&frame)?;
                    self.slots.push(Slot { offset, frame });
                    self.blobs.insert(id.to_string(), self.slots.len() - 1);
                }
            }
            return Ok(());
        }

        let key = (record_type, id.to_string());
        match self.live.get(&key).copied() {
            Some(idx) => {
                let new_idx = self.place_frame(idx, frame)?;
                self.live.insert(key, new_idx);
            }
            None => {
                let offset = self.container.append_frame(&frame)?;
                self.slots.push(Slot { offset, frame });
                self.live.insert(key, self.slots.len() - 1);
            }
        }
        Ok(())
    }

    /// 长度不变则原位移位写入（A-02）；否则旧帧置墓碑并在尾部追加新帧。
    ///
    /// **D-106（豆包审查 3.1-2）**：顺序为**先追加新帧、成功后再墓碑化旧帧**。
    /// 原顺序（先墓碑后追加）在磁盘满 / IO 错误下旧内容已毁、新内容未落，重启后
    /// 条目从索引消失且不可恢复。改后：追加失败 → 旧帧未动可重试；追加成功但
    /// 墓碑失败 → 磁盘短暂双 live（`rebuild_index` HashMap 后写胜，尾部新帧生效，
    /// 用户更新不丢），仅内存侧报错——两者都不再丢数据。
    fn place_frame(&mut self, idx: usize, frame: Frame) -> CoreResult<usize> {
        if self.slots[idx].frame.encoded_len() == frame.encoded_len() {
            self.container
                .pwrite_frame(self.slots[idx].offset, &frame)?;
            self.slots[idx].frame = frame;
            return Ok(idx);
        }
        let offset = self.container.append_frame(&frame)?;
        let mut tomb = self.slots[idx].frame.clone();
        tomb.flags |= crate::constants::FLAG_TOMBSTONE;
        self.container.pwrite_frame(self.slots[idx].offset, &tomb)?;
        self.slots[idx].frame = tomb;
        self.slots.push(Slot { offset, frame });
        Ok(self.slots.len() - 1)
    }

    /// 将一条活跃记录置为墓碑。
    ///
    /// **D-106（豆包审查 3.1-7）**：先写盘、成功后再删内存索引——原顺序在写盘失败
    /// 时内存已删、磁盘仍 live，重启后条目「复活」，与用户删除预期冲突。失败时
    /// live 保留，下次操作自然重试。
    fn tombstone(&mut self, record_type: u8, id: &str) -> CoreResult<()> {
        if let Some(idx) = self.live.get(&(record_type, id.to_string())).copied() {
            let mut tomb = self.slots[idx].frame.clone();
            tomb.flags |= crate::constants::FLAG_TOMBSTONE;
            self.container.pwrite_frame(self.slots[idx].offset, &tomb)?;
            self.live.remove(&(record_type, id.to_string()));
            self.slots[idx].frame = tomb;
        }
        Ok(())
    }

    /// 追加一枚**新 payload** 的墓碑帧（不进 `live`/`blobs` 索引——墓碑不属于活跃
    /// 记录；`rebuild_index` 对墓碑帧同样只进 slots，重载语义一致）。
    ///
    /// 与 [`Self::tombstone`] 的区别：tombstone 只把既有活跃帧**原 payload** 置位
    /// flags；本函数写入新密文的墓碑（D-98 purge 意图过线用）。同 `(type, id)` 多枚
    /// 墓碑帧并存是有意设计：导出（`tombstone_changes`）与复活抑制（`tombstone_rev`）
    /// 均取 rev 最高者，压缩每死记录只留 rev 最高一枚。
    fn append_tombstone_frame(
        &mut self,
        record_type: u8,
        id: &str,
        payload: &[u8],
    ) -> CoreResult<()> {
        let rid = record_id_to_bytes(id)?;
        let frame =
            self.build_frame(record_type, &rid, crate::constants::FLAG_TOMBSTONE, payload)?;
        let offset = self.container.append_frame(&frame)?;
        self.slots.push(Slot { offset, frame });
        Ok(())
    }

    /// 将附件 blob 置为墓碑。
    fn tombstone_blob(&mut self, id: &str) -> CoreResult<()> {
        if let Some(idx) = self.blobs.remove(id) {
            let mut tomb = self.slots[idx].frame.clone();
            tomb.flags |= crate::constants::FLAG_TOMBSTONE;
            self.container.pwrite_frame(self.slots[idx].offset, &tomb)?;
            self.slots[idx].frame = tomb;
        }
        Ok(())
    }

    /// 持久化头部计数与时间戳。
    fn persist_header(&mut self) -> CoreResult<()> {
        let mut header = self.container.header().clone();
        header.item_count = self.items.len() as u32;
        header.folder_count = self.folders.len() as u32;
        header.updated_at = now_ms();
        let rewrote = self.container.update_header(header)?;
        if rewrote {
            // 记录区整体重写导致偏移变化，索引需重建（业务数据仍在内存，无需重解密）。
            self.rebuild_index()?;
        }
        Ok(())
    }

    /// 对外暴露的计数持久化（供导入等批量写入后调用）。
    pub fn persist_counts(&mut self) -> CoreResult<()> {
        self.persist_header()
    }

    /// 更新密钥材料（改主密码时仅重包裹 VK，不动任何条目密文）。
    pub fn update_key_material(
        &mut self,
        wrapped_vault_key: WrappedKey,
        verify_mac: Vec<u8>,
    ) -> CoreResult<()> {
        let mut header = self.container.header().clone();
        header.wrapped_vault_key = wrapped_vault_key;
        header.verify_mac = verify_mac;
        header.updated_at = now_ms();
        let rewrote = self.container.update_header(header)?;
        if rewrote {
            self.rebuild_index()?;
        }
        Ok(())
    }

    // ===== 分组 =====

    /// 全部分组的频度聚合（`folder_id` → [`FolderUsage`]）。
    ///
    /// `list_folders` 的排序与 `FolderDto` 的聚合字段共用本函数，保证「排序看到的
    /// 数字」与「UI 显示的数字」必然一致。
    pub fn folder_usage(&self) -> HashMap<String, FolderUsage> {
        let mut out: HashMap<String, FolderUsage> = HashMap::new();
        for item in self.items.values() {
            if let Some(fid) = &item.folder_id {
                let agg = out.entry(fid.clone()).or_default();
                agg.use_count += item.use_count as i64;
                agg.last_used_at = match (agg.last_used_at, item.last_used_at) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
            }
        }
        out
    }

    /// 列出全部分组（#531 新模型：钉住块 `pin_order` ASC → 未钉住按组内频度
    /// `use_count` DESC / `last_used_at` DESC / `updated_at` DESC / `folder_id` ASC）。
    ///
    /// **`sort_order` 不再参与排序**（字段与 `reorder_folders` 保留仅为兼容）；
    /// 「收藏不再置顶」对分组同样成立——`Folder` 没有 favorite 字段，此处在历史
    /// 上也从未按 favorite 排过，列出仅为语义完整。聚合在调用时现算（items 在内存）。
    pub fn list_folders(&self) -> Vec<Folder> {
        let usage = self.folder_usage();
        let mut out: Vec<Folder> = self.folders.values().cloned().collect();
        out.sort_by(|a, b| {
            let ua = usage.get(&a.folder_id).copied().unwrap_or_default();
            let ub = usage.get(&b.folder_id).copied().unwrap_or_default();
            match (a.pinned, b.pinned) {
                (true, true) => a
                    .pin_order
                    .cmp(&b.pin_order)
                    .then_with(|| a.folder_id.cmp(&b.folder_id)),
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (false, false) => ub
                    .use_count
                    .cmp(&ua.use_count)
                    .then_with(|| {
                        ub.last_used_at
                            .unwrap_or(0)
                            .cmp(&ua.last_used_at.unwrap_or(0))
                    })
                    .then_with(|| b.updated_at.cmp(&a.updated_at))
                    .then_with(|| a.folder_id.cmp(&b.folder_id)),
            }
        });
        out
    }

    /// 新建分组。
    pub fn create_folder(
        &mut self,
        name: String,
        icon: Option<String>,
        color: Option<String>,
    ) -> CoreResult<Folder> {
        let name = name.trim().to_string();
        self.validate_folder_name(&name, None)?;
        let sort_order = self
            .folders
            .values()
            .map(|f| f.sort_order)
            .max()
            .unwrap_or(-1)
            + 1;
        let folder = Folder {
            folder_id: uuid::Uuid::new_v4().to_string(),
            name,
            icon,
            color,
            sort_order,
            rev: 1,
            updated_at: now_ms(),
            pinned: false,
            pin_order: 0,
        };
        self.write_record(
            RECORD_TYPE_FOLDER,
            &folder.folder_id,
            0,
            &serde_json::to_vec(&folder)?,
        )?;
        self.folders
            .insert(folder.folder_id.clone(), folder.clone());
        self.persist_header()?;
        Ok(folder)
    }

    /// 更新分组。
    pub fn update_folder(
        &mut self,
        folder_id: &str,
        name: String,
        icon: Option<String>,
        color: Option<String>,
    ) -> CoreResult<Folder> {
        let name = name.trim().to_string();
        self.validate_folder_name(&name, Some(folder_id))?;
        let mut folder = self
            .folders
            .get(folder_id)
            .cloned()
            .ok_or(CoreError::FolderNotFound)?;
        folder.name = name;
        folder.icon = icon;
        folder.color = color;
        folder.rev += 1;
        folder.updated_at = now_ms();
        self.write_record(
            RECORD_TYPE_FOLDER,
            folder_id,
            0,
            &serde_json::to_vec(&folder)?,
        )?;
        self.folders.insert(folder_id.to_string(), folder.clone());
        Ok(folder)
    }

    /// 删除分组（其下条目按策略处置）。
    pub fn delete_folder(
        &mut self,
        folder_id: &str,
        strategy: FolderDeleteStrategy,
    ) -> CoreResult<()> {
        if !self.folders.contains_key(folder_id) {
            return Err(CoreError::FolderNotFound);
        }
        let affected: Vec<Item> = self
            .items
            .values()
            .filter(|i| i.folder_id.as_deref() == Some(folder_id))
            .cloned()
            .collect();

        match strategy {
            FolderDeleteStrategy::ToUngrouped => {
                for mut item in affected {
                    item.folder_id = None;
                    item.rev += 1;
                    item.updated_at = now_ms();
                    item.modified_by = self.device_id.clone();
                    self.write_record(
                        RECORD_TYPE_ITEM,
                        &item.item_id,
                        0,
                        &serde_json::to_vec(&item)?,
                    )?;
                    self.items.insert(item.item_id.clone(), item);
                }
            }
            FolderDeleteStrategy::ToTrash => {
                for item in affected {
                    self.move_item_to_trash(&item)?;
                }
            }
        }

        self.tombstone(RECORD_TYPE_FOLDER, folder_id)?;
        self.folders.remove(folder_id);
        self.reindex_folders()?;
        self.persist_header()?;
        Ok(())
    }

    /// 按给定 ID 顺序重排分组。
    pub fn reorder_folders(&mut self, ordered_ids: &[String]) -> CoreResult<()> {
        for (pos, id) in ordered_ids.iter().enumerate() {
            if let Some(folder) = self.folders.get_mut(id) {
                folder.sort_order = pos as i32;
                folder.rev += 1;
                folder.updated_at = now_ms();
                let snapshot = folder.clone();
                self.write_record(
                    RECORD_TYPE_FOLDER,
                    &snapshot.folder_id,
                    0,
                    &serde_json::to_vec(&snapshot)?,
                )?;
            } else {
                return Err(CoreError::FolderNotFound);
            }
        }
        Ok(())
    }

    fn reindex_folders(&mut self) -> CoreResult<()> {
        let mut ordered: Vec<Folder> = self.folders.values().cloned().collect();
        ordered.sort_by_key(|f| (f.sort_order, f.folder_id.clone()));
        for (pos, mut folder) in ordered.into_iter().enumerate() {
            folder.sort_order = pos as i32;
            self.write_record(
                RECORD_TYPE_FOLDER,
                &folder.folder_id,
                0,
                &serde_json::to_vec(&folder)?,
            )?;
            self.folders.insert(folder.folder_id.clone(), folder);
        }
        Ok(())
    }

    fn validate_folder_name(&self, name: &str, exclude_id: Option<&str>) -> CoreResult<()> {
        if name.is_empty() {
            return Err(CoreError::FolderNameInvalid);
        }
        for folder in self.folders.values() {
            if Some(folder.folder_id.as_str()) == exclude_id {
                continue;
            }
            if folder.name == name {
                return Err(CoreError::FolderNameInvalid);
            }
        }
        Ok(())
    }

    // ===== 条目 =====

    /// 列出条目（支持分组过滤 / 仅收藏 / 模糊搜索）。
    pub fn list_items(
        &self,
        folder_id: Option<&str>,
        favorite_only: bool,
        query: Option<&str>,
    ) -> Vec<Item> {
        let needle = query
            .map(|q| q.trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let mut out: Vec<Item> = self
            .items
            .values()
            .filter(|item| {
                if let Some(fid) = folder_id {
                    if item.folder_id.as_deref() != Some(fid) {
                        return false;
                    }
                }
                if favorite_only && !item.favorite {
                    return false;
                }
                if let Some(needle) = &needle {
                    if !item_matches(item, needle) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        // #531 新排序模型（**favorite 不再置顶**，favorite 保留为纯标记）：
        // 钉住块（pin_order ASC）→ 未钉住（use_count DESC → last_used_at DESC（None=0）
        // → updated_at DESC → item_id ASC 兜底）。
        out.sort_by(|a, b| match (a.pinned, b.pinned) {
            (true, true) => a
                .pin_order
                .cmp(&b.pin_order)
                .then_with(|| a.item_id.cmp(&b.item_id)),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => b
                .use_count
                .cmp(&a.use_count)
                .then_with(|| {
                    b.last_used_at
                        .unwrap_or(0)
                        .cmp(&a.last_used_at.unwrap_or(0))
                })
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.item_id.cmp(&b.item_id)),
        });
        out
    }

    /// 取单条。
    pub fn get_item(&self, item_id: &str) -> CoreResult<Item> {
        self.items
            .get(item_id)
            .cloned()
            .ok_or(CoreError::ItemNotFound)
    }

    /// 校验草稿的 `folder_id`（【D-96】对齐 [`Self::move_item_to_folder`] 的语义）：
    /// `Some` 时分组必须存在，否则 [`CoreError::Validation`]（信息带 folder_id）。
    /// 此前 create/update 不校验，引用不存在的分组后该条目在列表中「隐身」
    /// （列表按 folder_id 过滤，无分组可命中）。
    fn validate_draft_folder(&self, folder_id: &Option<String>) -> CoreResult<()> {
        if let Some(fid) = folder_id {
            if !self.folders.contains_key(fid) {
                return Err(CoreError::Validation(format!(
                    "folder {fid} does not exist"
                )));
            }
        }
        Ok(())
    }

    /// 新建条目。
    pub fn create_item(&mut self, draft: ItemDraft) -> CoreResult<Item> {
        self.validate_item_kind(&draft.kind)?;
        self.validate_draft_folder(&draft.folder_id)?;
        let now = now_ms();
        let item = Item {
            item_id: uuid::Uuid::new_v4().to_string(),
            kind: draft.kind,
            folder_id: draft.folder_id,
            favorite: draft.favorite,
            created_at: now,
            updated_at: now,
            rev: 1,
            modified_by: self.device_id.clone(),
            attachments: Vec::new(),
            pinned: false,
            pin_order: 0,
            use_count: 0,
            last_used_at: None,
        };
        self.write_record(
            RECORD_TYPE_ITEM,
            &item.item_id,
            0,
            &serde_json::to_vec(&item)?,
        )?;
        self.items.insert(item.item_id.clone(), item.clone());
        self.persist_header()?;
        Ok(item)
    }

    /// 更新条目（逐条加密：仅重写该条）。
    pub fn update_item(&mut self, item_id: &str, draft: ItemDraft) -> CoreResult<Item> {
        self.validate_item_kind(&draft.kind)?;
        self.validate_draft_folder(&draft.folder_id)?;
        let mut item = self.get_item(item_id)?;
        item.kind = draft.kind;
        item.folder_id = draft.folder_id;
        item.favorite = draft.favorite;
        item.rev += 1;
        item.updated_at = now_ms();
        item.modified_by = self.device_id.clone();
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item_id.to_string(), item.clone());
        Ok(item)
    }

    /// 收藏切换。
    pub fn toggle_favorite(&mut self, item_id: &str, favorite: bool) -> CoreResult<Item> {
        let mut item = self.get_item(item_id)?;
        item.favorite = favorite;
        item.rev += 1;
        item.updated_at = now_ms();
        item.modified_by = self.device_id.clone();
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item_id.to_string(), item.clone());
        Ok(item)
    }

    // ===== 钉住 / 频度 / 归组（#531：拖拽排序与频度置顶的后端半边）=====

    /// 记录一次「使用」（#531）。
    ///
    /// 「使用」= **仅复制动作**（前端在复制成功后调用本命令埋点；`copy_secret`
    /// 本身不知道 item_id，埋点由前端负责）。同条目
    /// [`USAGE_THROTTLE_MS`](crate::constants::USAGE_THROTTLE_MS)（60s）节流：
    /// 窗口内重复调用直接 `Ok(())` 返回，**零状态变更**（不写帧、不推 rev）。
    pub fn record_item_usage(&mut self, item_id: &str) -> CoreResult<()> {
        self.record_item_usage_at(item_id, now_ms())
    }

    /// [`Self::record_item_usage`] 的可测变体：时钟由调用方注入。
    ///
    /// 集成测试用固定时钟验证 60s 节流窗口（前一次在窗口内 → 不计；达到窗口 → 计），
    /// 不必真等 60 秒。语义：`last_used_at` 为 `None` 或 `now - last_used_at >= 60s`
    /// 才计频（`use_count += 1; last_used_at = Some(now); rev += 1`）。
    ///
    /// 计频是元数据变更：**`updated_at` 不变**——不把「复制过」伪装成「修改过」，
    /// 用户可见的「最近更新时间」不因使用而跳变；`rev` 照常推进使变更走正常 LWW
    /// 同步（节流已把流量限制在每条目每分钟 ≤1 次）。
    pub fn record_item_usage_at(&mut self, item_id: &str, now: i64) -> CoreResult<()> {
        let mut item = self.get_item(item_id)?;
        let throttled = item
            .last_used_at
            .is_some_and(|last| now - last < USAGE_THROTTLE_MS);
        if throttled {
            return Ok(());
        }
        item.use_count += 1;
        item.last_used_at = Some(now);
        item.rev += 1;
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item_id.to_string(), item);
        Ok(())
    }

    /// 钉住 / 取消钉住条目（#531：拖拽到顶部 = 钉住）。
    ///
    /// 钉住时 `pin_order = 当前最大 pin_order + 1`（追加到钉住块末尾）；取消时
    /// `pin_order` 归零（未钉住条目不携带顺序）。排序语义见 [`Self::list_items`]。
    pub fn set_item_pin(&mut self, item_id: &str, pinned: bool) -> CoreResult<Item> {
        let mut item = self.get_item(item_id)?;
        if pinned {
            let max_order = self
                .items
                .values()
                .filter(|i| i.pinned)
                .map(|i| i.pin_order)
                .max()
                .unwrap_or(0);
            item.pin_order = max_order + 1;
        } else {
            item.pin_order = 0;
        }
        item.pinned = pinned;
        item.rev += 1;
        item.updated_at = now_ms();
        item.modified_by = self.device_id.clone();
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item_id.to_string(), item.clone());
        Ok(item)
    }

    /// 按给定 ID 顺序重排**已钉住**条目的 `pin_order`（#531：钉住块内拖拽）。
    ///
    /// 语义仿 [`Self::reorder_folders`]：先**整体校验**（每个 id 必须存在——
    /// 否则 [`CoreError::ItemNotFound`]；且必须已钉住——否则
    /// [`CoreError::Validation`]），校验全过才落笔，不存在写一半的状态。
    /// `pin_order = index`，**变更者**才推 rev 写帧（幂等：同序重复调用零写入）。
    pub fn reorder_pinned_items(&mut self, ordered_ids: &[String]) -> CoreResult<()> {
        for id in ordered_ids {
            match self.items.get(id) {
                Some(item) if item.pinned => {}
                Some(_) => return Err(CoreError::Validation(format!("item {id} is not pinned"))),
                None => return Err(CoreError::ItemNotFound),
            }
        }
        for (pos, id) in ordered_ids.iter().enumerate() {
            let pos = pos as i64;
            let mut item = match self.items.get(id) {
                Some(i) => i.clone(),
                None => unreachable!("整体校验已保证存在"), // 校验后必然存在
            };
            if item.pin_order == pos {
                continue;
            }
            item.pin_order = pos;
            item.rev += 1;
            item.updated_at = now_ms();
            item.modified_by = self.device_id.clone();
            self.write_record(RECORD_TYPE_ITEM, id, 0, &serde_json::to_vec(&item)?)?;
            self.items.insert(id.clone(), item);
        }
        Ok(())
    }

    /// 钉住 / 取消钉住分组（#531）。语义与 [`Self::set_item_pin`] 一致（`Folder`
    /// 无 `modified_by` 字段，其余同）。
    pub fn set_folder_pin(&mut self, folder_id: &str, pinned: bool) -> CoreResult<Folder> {
        let mut folder = self
            .folders
            .get(folder_id)
            .cloned()
            .ok_or(CoreError::FolderNotFound)?;
        if pinned {
            let max_order = self
                .folders
                .values()
                .filter(|f| f.pinned)
                .map(|f| f.pin_order)
                .max()
                .unwrap_or(0);
            folder.pin_order = max_order + 1;
        } else {
            folder.pin_order = 0;
        }
        folder.pinned = pinned;
        folder.rev += 1;
        folder.updated_at = now_ms();
        self.write_record(
            RECORD_TYPE_FOLDER,
            folder_id,
            0,
            &serde_json::to_vec(&folder)?,
        )?;
        self.folders.insert(folder_id.to_string(), folder.clone());
        Ok(folder)
    }

    /// 按给定 ID 顺序重排**已钉住**分组的 `pin_order`（#531）。
    /// 校验/幂等语义与 [`Self::reorder_pinned_items`] 一致。
    pub fn reorder_pinned_folders(&mut self, ordered_ids: &[String]) -> CoreResult<()> {
        for id in ordered_ids {
            match self.folders.get(id) {
                Some(folder) if folder.pinned => {}
                Some(_) => return Err(CoreError::Validation(format!("folder {id} is not pinned"))),
                None => return Err(CoreError::FolderNotFound),
            }
        }
        for (pos, id) in ordered_ids.iter().enumerate() {
            let pos = pos as i64;
            let mut folder = match self.folders.get(id) {
                Some(f) => f.clone(),
                None => unreachable!("整体校验已保证存在"),
            };
            if folder.pin_order == pos {
                continue;
            }
            folder.pin_order = pos;
            folder.rev += 1;
            folder.updated_at = now_ms();
            self.write_record(RECORD_TYPE_FOLDER, id, 0, &serde_json::to_vec(&folder)?)?;
            self.folders.insert(id.clone(), folder);
        }
        Ok(())
    }

    /// 移动条目到分组（#531；`None` = 移出为未分组）。
    ///
    /// `folder_id` 为 `Some` 时分组必须存在（否则 [`CoreError::Validation`]）。
    /// `updated_at` 不变：移动分组不改内容（与 [`Self::record_item_usage`] 同一裁定，
    /// 不把「挪了个位置」伪装成「修改过」）；`rev` 照常推进走正常 LWW。
    pub fn move_item_to_folder(
        &mut self,
        item_id: &str,
        folder_id: Option<String>,
    ) -> CoreResult<Item> {
        if let Some(fid) = &folder_id {
            if !self.folders.contains_key(fid) {
                return Err(CoreError::Validation(format!(
                    "folder {fid} does not exist"
                )));
            }
        }
        let mut item = self.get_item(item_id)?;
        item.folder_id = folder_id;
        item.rev += 1;
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item_id.to_string(), item.clone());
        Ok(item)
    }

    /// 删除条目（默认进回收站；`permanent=true` 时彻底删除）。
    ///
    /// D-60 §3.14.4：彻底删除后的 blob 清理**不再 1:1 连带**（KeepBoth 共享引用会误杀），
    /// 统一交孤儿 GC——父帧（含附件元数据）消失后，blob 若无任何引用即 tombstone。
    pub fn delete_item(&mut self, item_id: &str, permanent: bool) -> CoreResult<()> {
        let item = self.get_item(item_id)?;
        if permanent {
            self.tombstone(RECORD_TYPE_ITEM, item_id)?;
            self.items.remove(item_id);
            self.gc_orphan_blobs()?;
        } else {
            self.move_item_to_trash(&item)?;
        }
        self.persist_header()?;
        Ok(())
    }

    fn move_item_to_trash(&mut self, item: &Item) -> CoreResult<()> {
        let now = now_ms();
        let entry = TrashEntry {
            trash_id: uuid::Uuid::new_v4().to_string(),
            item_snapshot: item.clone(),
            original_folder_id: item.folder_id.clone(),
            deleted_at: now,
            expire_at: now + TRASH_RETENTION_DAYS * MS_PER_DAY,
        };
        self.write_record(
            RECORD_TYPE_TRASH,
            &entry.trash_id,
            0,
            &serde_json::to_vec(&entry)?,
        )?;
        self.trash.insert(entry.trash_id.clone(), entry);
        self.tombstone(RECORD_TYPE_ITEM, &item.item_id)?;
        self.items.remove(&item.item_id);
        Ok(())
    }

    /// 条目类型合法性 + 统一明文上限校验（【D-96】）。
    ///
    /// - 标题非空、SecureNote 正文 ≤ [`MAX_NOTE_BYTES`]（既有语义不变）；
    /// - **新增统一上限**：`ItemKind` 序列化后的字节 ≤ [`MAX_ITEM_PLAINTEXT_BYTES`]
    ///   （384 KiB）。口径说明：帧明文就是 `Item` 的 JSON，其中变长部分几乎全部在
    ///   `kind` 内，以 `serde_json::to_vec(kind)` 计量与真实落库量同源；固定元数据
    ///   （uuid/时间戳/rev 等 ~300 B）量级可忽略。此前 Login.notes 无任何上限，
    ///   一条巨型笔记可直接把条目密文顶过同步分页的单条阈值（`change_too_large`
    ///   指名报错且**永不收敛**——每轮同步都从它开始重发），384 KiB < ~496 KiB
    ///   阈值保证见 [`crate::constants::MAX_ITEM_PLAINTEXT_BYTES`] 文档。
    /// - 超限错误信息带条目标题（截断）与上限值，便于用户定位是哪一条。
    fn validate_item_kind(&self, kind: &ItemKind) -> CoreResult<()> {
        if kind.title().trim().is_empty() {
            return Err(CoreError::Validation("title".to_string()));
        }
        if let ItemKind::SecureNote(note) = kind {
            if note.body.len() > MAX_NOTE_BYTES {
                return Err(CoreError::Validation("note_too_large".to_string()));
            }
        }
        let est = serde_json::to_vec(kind)
            .map_err(|_| CoreError::Internal("item kind serialize".to_string()))?
            .len();
        if est > MAX_ITEM_PLAINTEXT_BYTES {
            // 标题仅截断展示（不含其它敏感字段）；上限值随报错给出，前端无需再查常量。
            let title = &kind.title().chars().take(64).collect::<String>();
            return Err(CoreError::Validation(format!(
                "item_too_large:{title}: {est} > {MAX_ITEM_PLAINTEXT_BYTES} bytes"
            )));
        }
        Ok(())
    }

    // ===== 附件（D-60：附件依附父条目，§3.14）=====

    /// 向父条目添加附件（§3.14.5 `add_attachment`）。
    ///
    /// 校验链：父条目存在且非遗留 attachment 型 → 附件数 ≤ [`MAX_ATTACHMENTS_PER_ITEM`]
    /// → 文件名 ≤ [`MAX_ATTACHMENT_FILENAME_BYTES`] → 单文件 ≤ [`MAX_ATTACHMENT_BYTES`]
    /// → 库容量 ≤ [`MAX_VAULT_BYTES`]。全部通过后：写 blob 帧（record_id =
    /// `attachment_id`，**≠ 父 item_id**，§3.14.1 解耦）→ 父条目元数据追加 → 父 rev+1。
    ///
    /// `title` 仅用于校验兼容（D-59 遗留参数）：附件展示名就是 `filename`，
    /// 空串原样接受；未来如需自定义展示名再扩展 `AttachmentMeta`。
    ///
    /// D-66：本入口只服务**文件系统路径**。先做一次 `std::fs::metadata` 预检，
    /// 让超大文件在**读进内存之前**就被 `ERR_ATTACHMENT_TOO_LARGE` 拦下；
    /// 真正的校验与落库逻辑全在 [`Self::insert_attachment`]，与
    /// [`Self::add_attachment_bytes`] 共用，保证两条入口的校验顺序/错误码逐字一致。
    #[allow(deprecated)]
    pub fn add_attachment(
        &mut self,
        item_id: &str,
        source_path: &Path,
        title: &str,
    ) -> CoreResult<Item> {
        let _ = title; // 展示名 = filename；参数保留以维持 IPC 形状（§3.14.5）
                       // 预检：超大文件不必读进内存（正式判定仍由 insert_attachment 按 data.len() 负责）。
        if std::fs::metadata(source_path)?.len() > MAX_ATTACHMENT_BYTES {
            return Err(CoreError::AttachmentTooLarge);
        }
        let data = std::fs::read(source_path)?;
        let filename = source_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "attachment".to_string());
        self.insert_attachment(item_id, &filename, &data)
    }

    /// 按**字节**向父条目添加附件（D-66）。
    ///
    /// Android 文件选择器返回 `content://` URI，不能直接 `std::fs::read`；外壳层
    /// 先用 `tauri-plugin-fs` 的 `getFileDescriptor` 把字节读出来，再走本方法。
    /// `filename` 由调用方给定（见 [`crate::attachment_name::filename_from_source`]）。
    ///
    /// 校验链、错误码、落库语义与 [`Self::add_attachment`] **完全一致**（同一实现）。
    pub fn add_attachment_bytes(
        &mut self,
        item_id: &str,
        filename: &str,
        data: &[u8],
    ) -> CoreResult<Item> {
        self.insert_attachment(item_id, filename, data)
    }

    /// 附件落库的**唯一**校验 + 写入实现（D-66 抽出，供两条入口共用）。
    ///
    /// 顺序（改动会让错误码语义漂移，务必保持）：
    /// ① 父条目存在（`ItemNotFound`）→ ② 非遗留 attachment 型
    /// （`Validation("legacy_attachment_item")`）→ ③ 附件数上限（`AttachmentLimit`）
    /// → ④ 单文件大小（`AttachmentTooLarge`）→ ⑤ 库容量（`VaultCapacityExceeded`）
    /// → ⑥ 文件名长度（`Validation("filename_too_long")`）→ 写 blob 帧 → 父 rev+1。
    #[allow(deprecated)]
    fn insert_attachment(
        &mut self,
        item_id: &str,
        filename: &str,
        data: &[u8],
    ) -> CoreResult<Item> {
        let mut item = self.get_item(item_id)?;
        if matches!(item.kind, ItemKind::Attachment(_)) {
            // 遗留型条目在迁移后不应存在；防御性拒绝。
            return Err(CoreError::Validation("legacy_attachment_item".to_string()));
        }
        if item.attachments.len() >= MAX_ATTACHMENTS_PER_ITEM {
            return Err(CoreError::AttachmentLimit);
        }

        let size = data.len() as u64;
        if size > MAX_ATTACHMENT_BYTES {
            return Err(CoreError::AttachmentTooLarge);
        }
        let current = self.file_size()?;
        // 预留密文膨胀（tag 16B + 帧头 + IK 包裹）的余量。
        if current + size + 4096 > MAX_VAULT_BYTES {
            return Err(CoreError::VaultCapacityExceeded);
        }
        if filename.len() > MAX_ATTACHMENT_FILENAME_BYTES {
            return Err(CoreError::Validation("filename_too_long".to_string()));
        }
        let mime = guess_mime(filename);

        let attachment_id = uuid::Uuid::new_v4().to_string();
        let blob_ct_len = data.len() + crate::constants::AEAD_TAG_LEN;
        let attachment = AttachmentMeta {
            attachment_id: attachment_id.clone(),
            filename: filename.to_string(),
            mime_type: mime,
            size_bytes: size,
            blob_size_bytes: blob_ct_len as u64,
        };

        // blob 帧：record_id = attachment_id（§3.14.1 解耦；spool / 拉取清单同键）。
        self.write_record(RECORD_TYPE_ITEM, &attachment_id, FLAG_BLOB, data)?;

        item.attachments.push(attachment);
        item.rev += 1;
        item.updated_at = now_ms();
        item.modified_by = self.device_id.clone();
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item_id.to_string(), item.clone());
        self.persist_header()?;
        Ok(item)
    }

    /// 从父条目移除单个附件（§3.14.5 `remove_attachment`）。
    ///
    /// 父条目 rev+1 正常传播（元数据减法随父帧过线）；blob **不立即墓碑**——
    /// 交孤儿 GC（§3.14.4）：该 attachment_id 脱离引用集合后，下一次 GC 才 tombstone。
    pub fn remove_attachment(&mut self, item_id: &str, attachment_id: &str) -> CoreResult<Item> {
        let mut item = self.get_item(item_id)?.clone();
        let before = item.attachments.len();
        item.attachments
            .retain(|a| a.attachment_id != attachment_id);
        if item.attachments.len() == before {
            return Err(CoreError::ItemNotFound);
        }
        item.rev += 1;
        item.updated_at = now_ms();
        item.modified_by = self.device_id.clone();
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item_id.to_string(), item.clone());
        self.persist_header()?;
        Ok(item)
    }

    /// 读取附件明文（供导出；`U4` 本地存储不分片）。
    ///
    /// **D-106（豆包审查 3.1-4）**：返回 [`Zeroizing`] 载体——原实现 `.to_vec()` 把
    /// `open_frame` 的零化缓冲克隆进普通 `Vec<u8>`，Drop 不覆写，附件明文（≤20MB）
    /// 在导出/同步后残留堆中，与 A-04「锁定无明文残留」目标不符。
    pub fn read_blob(&self, blob_ref: &str) -> CoreResult<Zeroizing<Vec<u8>>> {
        let idx = *self.blobs.get(blob_ref).ok_or(CoreError::BlobMissing)?;
        let frame = &self.slots[idx].frame;
        // Zeroizing<Vec<u8>> → Zeroizing<Vec<u8>> 的 map 保持零化语义。
        Ok(Zeroizing::new(self.open_frame(frame)?.to_vec()))
    }

    /// 导出附件到目标路径（§3.14.5：按 `(item_id, attachment_id)` 定位）。
    ///
    /// D-53 语义保留：附件元数据存在但 blob 未同步到本机时，报 `ERR_BLOB_MISSING`。
    /// D-71：本方法只服务**文件系统路径**；`content://` 的落盘由外壳层用
    /// [`Self::export_attachment_bytes`] 取到字节后自行写入（Android SAF）。
    pub fn export_attachment(
        &self,
        item_id: &str,
        attachment_id: &str,
        dest_path: &Path,
    ) -> CoreResult<()> {
        let data = self.export_attachment_bytes(item_id, attachment_id)?;
        std::fs::write(dest_path, &data)?;
        Ok(())
    }

    /// 取附件明文**字节**（D-71：`content://` 导出用，照 `add_attachment_bytes` 的既有范式）。
    ///
    /// 校验顺序与错误码与 `export_attachment` 的路径版**逐条一致**（两条入口共用本实现）：
    /// 父条目存在（`ItemNotFound`）→ 附件在父条目内（`ItemNotFound`）→
    /// blob 已同步到本机（`BlobMissing`）→ 解密读取。
    pub fn export_attachment_bytes(
        &self,
        item_id: &str,
        attachment_id: &str,
    ) -> CoreResult<Zeroizing<Vec<u8>>> {
        let item = self.get_item(item_id)?;
        let meta = item
            .attachments
            .iter()
            .find(|a| a.attachment_id == attachment_id)
            .ok_or(CoreError::ItemNotFound)?;
        if !self.has_blob(&meta.attachment_id) {
            return Err(CoreError::BlobMissing);
        }
        self.read_blob(&meta.attachment_id)
    }

    /// 附件分片（同步用，`U4`：≤ 1 MB）。
    ///
    /// D-106：`read_blob` 已零化；分片逐个 `to_vec` 后立刻进入 AEAD 帧序列化，
    /// 网络缓冲层的零化超出本层边界（帧层 `Zeroizing` 为后续项）。
    pub fn blob_chunks(&self, blob_ref: &str) -> CoreResult<Vec<Vec<u8>>> {
        let data = self.read_blob(blob_ref)?;
        Ok(data
            .chunks(ATTACHMENT_CHUNK_BYTES)
            .map(|c| c.to_vec())
            .collect())
    }

    // ===== blob 同步原语（D-53，架构文档 §3.12.4；只加不改既有签名）=====

    /// `blobs` 索引查询：本地是否已有该 blob 的密文帧（补齐判定依据）。
    pub fn has_blob(&self, blob_ref: &str) -> bool {
        self.blobs.contains_key(blob_ref)
    }

    /// 收集全部附件 blob 引用（供拉取方比对缺失清单 / spool 有效性 / GC 引用集合）。
    ///
    /// # 不变量（比实现本身重要）
    ///
    /// **引用集合必须覆盖所有可能恢复该 blob 的快照来源**：活条目、回收站快照、冲突快照。
    /// 新增任何「暂存一份 `Item` 等用户决定」的结构时，**必须同步加进这里**——漏一类
    /// 的代价不是"多留垃圾"，而是**永久丢数据**：GC 会墓碑它、压缩会把它从文件里抹掉、
    /// 同步方也不会再去拉它，而用户一旦决定「采用那份快照」，得到的就是 `BlobMissing`。
    ///
    /// 已体检的调用点（四处共用本函数，改这里等于同时改它们）：
    /// [`Self::gc_orphan_blobs`]（墓碑孤儿）、[`Self::plan_compaction`]（压缩保留集合）、
    /// `sync::blob_transfer::cleanup_orphan_spools`（spool 清理）、
    /// `sync::blob_transfer::send_blob` 的 `want` 清单（决定是否向对端拉取）。
    ///
    /// 清单来源 = **活跃条目的 `attachments[].attachment_id`**
    /// ∪ **回收站快照的 `attachments[].attachment_id`**（还原必须可用）
    /// ∪ **冲突箱两端快照的 `attachments[].attachment_id`**（仲裁必须可用；D-76 修复）；
    /// 另含遗留 `ItemKind::Attachment` 条目的 `blob_ref`（迁移前的存量库兼容窗口，
    /// 保证迁移前同步/GC 行为与 D-53 时代一致）。
    ///
    /// **冲突侧只保活未仲裁（`resolved == false`）的快照**（D-80 定论；D-76 时代是"一律计入"）。
    /// 依据是三条已核实的事实，不是"看起来没人读"的推测：
    ///
    /// 1. **读不到**：[`Self::list_conflicts`] 过滤 `!resolved`，前端拿不到已仲裁冲突，
    ///    不存在任何读取/回滚/再仲裁路径；
    /// 2. **不需要**：[`Self::resolve_conflict`] 的写入顺序是「先写活条目/副本（或软删进回收站），
    ///    **最后**才写 `resolved = true`」。所以磁盘上 `resolved=true` 成立的那一刻，
    ///    被继承的附件引用已经落在活条目/回收站快照里了 —— 它们由上面两段本函数自己保活；
    ///    逐级 `?` 上抛保证「写 conflict 失败 ⇒ resolved 仍为 false ⇒ 保活照旧」；
    /// 3. **留着有害**：已仲裁快照 = 永久 dead weight（每条冲突最多 2×16 个引用、单个 blob 20MB），
    ///    与 D-76「用户删掉的东西真的没了」直接冲突。
    ///
    /// 注意 `KeepLocal` 与 `KeepBoth` 的分野：前者是用户**明确丢弃**远端快照（其引用应当回收），
    /// 后者把远端附件**继承进新 `item_id` 的副本条目**（引用由活条目继续保活）。
    /// 两个分支的差别正是"停的是 dead weight、不是活引用"的证明。
    #[allow(deprecated)]
    pub fn attachment_blob_refs(&self) -> Vec<String> {
        let mut refs = Vec::new();
        for item in self.items.values() {
            match &item.kind {
                // 遗留窗口：迁移前的独立附件条目（§3.14.3）。
                ItemKind::Attachment(a) => refs.push(a.blob_ref.clone()),
                _ => {
                    for a in &item.attachments {
                        refs.push(a.attachment_id.clone());
                    }
                }
            }
        }
        for entry in self.trash.values() {
            match &entry.item_snapshot.kind {
                ItemKind::Attachment(a) => refs.push(a.blob_ref.clone()),
                _ => {
                    for a in &entry.item_snapshot.attachments {
                        refs.push(a.attachment_id.clone());
                    }
                }
            }
        }
        // 冲突箱：`local_snapshot` 与 `remote_snapshot` **都要收**——仲裁「保留本机」要前者、
        // 「采用远端」/「保留两者」要后者，任一侧缺引用都会在仲裁后变成 BlobMissing。
        for entry in self.conflicts.values() {
            // D-80：已仲裁冲突前端不可见（`list_conflicts` 过滤 `!resolved`）、无回滚路径，
            // 其快照里的附件引用是永久 dead weight。仲裁写入顺序保证「resolved 落盘时活条目
            // 已继承引用」（resolve_conflict 先写活条目/副本、最后才写 resolved），故可停止保活。
            if entry.resolved {
                continue;
            }
            for snapshot in [&entry.local_snapshot, &entry.remote_snapshot] {
                match &snapshot.kind {
                    ItemKind::Attachment(a) => refs.push(a.blob_ref.clone()),
                    _ => {
                        for a in &snapshot.attachments {
                            refs.push(a.attachment_id.clone());
                        }
                    }
                }
            }
        }
        refs
    }

    /// 存量独立附件条目 → SecureNote 挂附件（D-60 §3.14.3，解锁时一次性自动迁移）。
    ///
    /// 规则：
    /// 1. 新条目 `kind = SecureNote{ title: 原附件 title, body: "" }`——选 SecureNote
    ///    而非 Login：假 Login（无密码）会污染 F-16 体检「弱口令」统计；
    /// 2. `attachments = [AttachmentMeta{ attachment_id: 原条目 item_id（= 原 blob_ref
    ///    → **blob 帧字节级零重写**）, filename: 原 original_filename, … }]`；
    /// 3. 新条目 rev = 原 rev + 1、folder_id/favorite/created_at 继承 → 随同步传播；
    /// 4. 旧 ITEM 帧置墓碑（D-56 链路自动传播删除）；
    /// 5. 幂等：无 `kind == Attachment` 即空转。须在 wipe_plaintext 之前、同步 export
    ///    之前执行（§3.14.8：避免迁移产物与远端旧语义帧竞态）。
    ///
    /// 返回迁移条目数。
    pub fn migrate_legacy_attachments(&mut self) -> CoreResult<usize> {
        #[allow(deprecated)]
        let legacy: Vec<Item> = self
            .items
            .values()
            .filter(|item| matches!(item.kind, ItemKind::Attachment(_)))
            .cloned()
            .collect();
        let n = legacy.len();
        if n == 0 {
            return Ok(0);
        }
        for old in legacy {
            #[allow(deprecated)]
            let (attachment_data, old_id) = match &old.kind {
                ItemKind::Attachment(a) => (a.clone(), old.item_id.clone()),
                _ => unreachable!("filtered above"),
            };
            let new_item = Item {
                item_id: uuid::Uuid::new_v4().to_string(),
                kind: ItemKind::SecureNote(crate::model::item::SecureNoteData {
                    title: attachment_data.title.clone(),
                    body: String::new(),
                }),
                folder_id: old.folder_id.clone(),
                favorite: old.favorite,
                created_at: old.created_at,
                updated_at: old.updated_at,
                rev: old.rev + 1,
                modified_by: self.device_id.clone(),
                attachments: vec![AttachmentMeta {
                    // 关键：attachment_id = 原条目 item_id = 原 blob_ref → blob 帧零重写。
                    attachment_id: old_id.clone(),
                    filename: attachment_data.original_filename.clone(),
                    mime_type: attachment_data.mime_type.clone(),
                    size_bytes: attachment_data.size_bytes,
                    blob_size_bytes: attachment_data.blob_size_bytes,
                }],
                // 迁移产物是全新 item_id 的新条目，排序/频度字段从零开始。
                pinned: false,
                pin_order: 0,
                use_count: 0,
                last_used_at: None,
            };
            // 旧 ITEM 帧置墓碑（blob 帧是独立槽位，不受影响）。
            self.tombstone(RECORD_TYPE_ITEM, &old_id)?;
            self.items.remove(&old_id);
            self.write_record(
                RECORD_TYPE_ITEM,
                &new_item.item_id,
                0,
                &serde_json::to_vec(&new_item)?,
            )?;
            self.items.insert(new_item.item_id.clone(), new_item);
        }
        self.persist_header()?;
        Ok(n)
    }

    /// 孤儿可达性 GC（D-60 §3.14.4）：不在引用集合中的 blob 帧 → tombstone。
    ///
    /// 触发时机：`purge_item` / `empty_trash` / `purge_expired_trash` / 解锁时（幂等）。
    /// 正确性关键：KeepBoth 共享引用（两父条目同 attachment_id）任一存在即保活；
    /// 父软删（trash 快照）不脱离引用集合，blob 保留供还原。
    /// 返回本次 tombstone 的 blob 数。
    pub fn gc_orphan_blobs(&mut self) -> CoreResult<usize> {
        let referenced: std::collections::HashSet<String> =
            self.attachment_blob_refs().into_iter().collect();
        let orphans: Vec<String> = self
            .blobs
            .keys()
            .filter(|id| !referenced.contains(*id))
            .cloned()
            .collect();
        for id in &orphans {
            self.tombstone_blob(id)?;
        }
        if !orphans.is_empty() {
            self.persist_header()?;
        }
        Ok(orphans.len())
    }

    /// 完整 blob 帧（含 `wrapped_key`/`nonce`/`plaintext_len`/`ct`）。
    ///
    /// 供发送端**零解密**切片：直接取帧内 `ct` 切块过线，全程不触碰明文（A-01）。
    pub fn blob_frame(&self, blob_ref: &str) -> Option<Frame> {
        self.blobs
            .get(blob_ref)
            .map(|&idx| self.slots[idx].frame.clone())
    }

    /// 接收端入库一个远端 blob 帧（幂等，不覆盖本地）。
    ///
    /// - 帧已存在且字节一致 → [`BlobWriteOutcome::Skipped`]；
    /// - 帧已存在但字节不同 → [`BlobWriteOutcome::KeptLocal`]（保留本地，不覆盖）；
    /// - 不存在 → 追加入库 → [`BlobWriteOutcome::Appended`]（`blobs` 索引此刻才可见，
    ///   `read_blob` 永不见半成品）。
    ///
    /// 调用方须先做 AEAD 验证（[`Repository::verify_blob_frame`]）；本方法不重复验证。
    pub fn write_blob_frame(&mut self, frame: Frame) -> CoreResult<BlobWriteOutcome> {
        if !frame.is_blob() || frame.record_type != RECORD_TYPE_ITEM {
            return Err(CoreError::Validation("not_a_blob_frame".to_string()));
        }
        let id = frame.record_id_string();
        if let Some(&idx) = self.blobs.get(&id) {
            // 字节级一致判定（字段直比，避免整帧 encode 的 20MB 级拷贝）。
            let local = &self.slots[idx].frame;
            let same = local.record_type == frame.record_type
                && local.flags == frame.flags
                && local.record_id == frame.record_id
                && local.wrapped_key == frame.wrapped_key
                && local.nonce == frame.nonce
                && local.plaintext_len == frame.plaintext_len
                && local.ct == frame.ct;
            return Ok(if same {
                BlobWriteOutcome::Skipped
            } else {
                BlobWriteOutcome::KeptLocal
            });
        }
        // D-106（豆包审查 3.1-5）：同步接收路径补齐与本地 insert_attachment 相同的
        // 大小守卫——本地库校验只防「自己传大文件」，防不住异常/被入侵的配对端
        // 直接推帧（信任边界内 DoS 纵深）。口径与本地路径一致：明文 ≤ 20MB、
        // 库容量预留 4KB 密文膨胀余量。
        let plaintext_len = frame.plaintext_len as u64;
        if plaintext_len > MAX_ATTACHMENT_BYTES {
            return Err(CoreError::AttachmentTooLarge);
        }
        let current = self.file_size()?;
        if current + plaintext_len + 4096 > MAX_VAULT_BYTES {
            return Err(CoreError::VaultCapacityExceeded);
        }
        let offset = self.container.append_frame(&frame)?;
        self.slots.push(Slot { offset, frame });
        self.blobs.insert(id, self.slots.len() - 1);
        Ok(BlobWriteOutcome::Appended)
    }

    /// 验证一帧可被本库 VK 解开（unwrap IK + AEAD tag 校验，明文即弃）。
    ///
    /// 接收端 finalize 用它充当「组帧完整性 + 真实性」校验（D-53：AEAD tag 免费
    /// 充当组帧校验，无需额外哈希）。验证失败 = 分片缺失/错序/tag 不符。
    pub fn verify_blob_frame(&self, frame: &Frame) -> CoreResult<()> {
        let _pt = self.open_frame(frame)?;
        Ok(())
    }

    // ===== 回收站 =====

    /// 列出回收站。
    pub fn list_trash(&self) -> Vec<TrashEntry> {
        let mut out: Vec<TrashEntry> = self.trash.values().cloned().collect();
        out.sort_by_key(|t| std::cmp::Reverse(t.deleted_at));
        out
    }

    /// 还原条目。
    pub fn restore_item(&mut self, trash_id: &str) -> CoreResult<Item> {
        let entry = self
            .trash
            .get(trash_id)
            .cloned()
            .ok_or(CoreError::ItemNotFound)?;
        // D-91（全仓审查 R2 🔴）：同 item_id 已有**活跃条目**时拒绝还原——旧快照
        // rev+1 + updated_at=now 会把（典型场景：对端删除后又编辑、同步复活）新版本
        // 顶掉，并经 LWW 把覆盖传播回对端（两端丢数据）。复活路径已改为清除旧快照
        // （`drop_trash_snapshots`），此守卫兜底一切残余的异常并存形态。
        if self.items.contains_key(&entry.item_snapshot.item_id) {
            return Err(CoreError::RestoreConflict);
        }
        let mut item = entry.item_snapshot;
        item.rev += 1;
        item.updated_at = now_ms();
        item.modified_by = self.device_id.clone();
        // D-106（豆包审查 3.1-3）：回收站期间原分组可能已被删除——照抄快照的
        // folder_id 会让条目落进悬空分组（分组视图不可见，用户误以为丢失）。
        // 分组不存在则置 None 移入「未分组」，与 create/update/move 的守卫口径一致。
        if let Some(fid) = item.folder_id.as_ref() {
            if !self.folders.contains_key(fid) {
                item.folder_id = None;
            }
        }
        self.write_record(
            RECORD_TYPE_ITEM,
            &item.item_id,
            0,
            &serde_json::to_vec(&item)?,
        )?;
        self.items.insert(item.item_id.clone(), item.clone());
        self.tombstone(RECORD_TYPE_TRASH, trash_id)?;
        self.trash.remove(trash_id);
        self.persist_header()?;
        Ok(item)
    }

    /// 彻底删除回收站条目（D-60 §3.14.4：blob 清理交孤儿 GC，不再 1:1 连带）。
    ///
    /// **D-98（C 案，2026-10-25）：删除意图过线**——以回收站死亡快照为载荷、`rev+1`
    /// 重封一枚 ITEM 墓碑追加落盘。对端收到更高 rev 墓碑且本端无活跃条目时，顺带
    /// 彻底删除其回收站快照（`sync::merge::apply_changes` → `purge_trash_snapshots`）。
    /// 旧墓碑帧保留：导出/复活抑制均取 rev 最高者，压缩每死记录也只留最高一枚。
    pub fn purge_item(&mut self, trash_id: &str) -> CoreResult<()> {
        let entry = self
            .trash
            .get(trash_id)
            .cloned()
            .ok_or(CoreError::ItemNotFound)?;
        self.tombstone(RECORD_TYPE_TRASH, trash_id)?;
        self.trash.remove(trash_id);
        let mut death = entry.item_snapshot;
        death.rev += 1;
        let payload = serde_json::to_vec(&death)?;
        self.append_tombstone_frame(RECORD_TYPE_ITEM, &death.item_id, &payload)?;
        self.gc_orphan_blobs()?;
        Ok(())
    }

    /// 清空回收站。
    pub fn empty_trash(&mut self) -> CoreResult<()> {
        let ids: Vec<String> = self.trash.keys().cloned().collect();
        for id in ids {
            self.purge_item(&id)?;
        }
        Ok(())
    }

    /// 【D-91】清除某条目的全部回收站快照（同步复活成功后调用）。
    ///
    /// 对端「删除后又编辑」会把 live 新版本同步回来；旧「已删」快照若不清除，
    /// 用户「还原」会用旧数据覆盖刚同步来的新版本（R2 根因）。幂等：无快照返回 0。
    /// blob 引用集合随之收窄，孤儿 blob 交既有 `gc_orphan_blobs` 兜底。
    pub fn drop_trash_snapshots(&mut self, item_id: &str) -> CoreResult<usize> {
        let stale: Vec<String> = self
            .trash
            .values()
            .filter(|t| t.item_snapshot.item_id == item_id)
            .map(|t| t.trash_id.clone())
            .collect();
        let n = stale.len();
        for trash_id in stale {
            self.tombstone(RECORD_TYPE_TRASH, &trash_id)?;
            self.trash.remove(&trash_id);
        }
        Ok(n)
    }

    /// 【D-98】对端「彻底删除」意图的落点：本端无活跃条目时，清掉 rev 不高于对端
    /// 墓碑的回收站快照（blob 引用随之收窄，孤儿交 `gc_orphan_blobs`）。
    ///
    /// rev 条件保护本端「删了更新版本」的快照：快照 rev 高于对端墓碑 → 保留，待
    /// 自身墓碑过线收敛。返回清理的快照数。调用约束：该 item_id 无活跃条目（merge
    /// 侧已保证——活跃分支走删除-修改拉锯/常规软删，不进本方法）。
    pub fn purge_trash_snapshots(&mut self, item_id: &str, tomb_rev: u64) -> CoreResult<usize> {
        let stale: Vec<String> = self
            .trash
            .values()
            .filter(|t| t.item_snapshot.item_id == item_id && t.item_snapshot.rev <= tomb_rev)
            .map(|t| t.trash_id.clone())
            .collect();
        let n = stale.len();
        for trash_id in stale {
            self.tombstone(RECORD_TYPE_TRASH, &trash_id)?;
            self.trash.remove(&trash_id);
        }
        if n > 0 {
            self.gc_orphan_blobs()?;
        }
        Ok(n)
    }

    /// 清理超过 30 天的回收站条目（解锁时触发）。返回清理数。
    pub fn purge_expired_trash(&mut self) -> CoreResult<usize> {
        let now = now_ms();
        let expired: Vec<String> = self
            .trash
            .values()
            .filter(|e| e.expire_at <= now)
            .map(|e| e.trash_id.clone())
            .collect();
        let n = expired.len();
        for id in expired {
            self.purge_item(&id)?;
        }
        Ok(n)
    }

    // ===== 冲突箱 =====

    /// 列出未仲裁冲突。
    pub fn list_conflicts(&self) -> Vec<ConflictEntry> {
        let mut out: Vec<ConflictEntry> = self
            .conflicts
            .values()
            .filter(|c| !c.resolved)
            .cloned()
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.detected_at));
        out
    }

    /// 三选一仲裁。
    pub fn resolve_conflict(
        &mut self,
        conflict_id: &str,
        choice: ConflictChoice,
    ) -> CoreResult<()> {
        let entry = self
            .conflicts
            .get(conflict_id)
            .cloned()
            .ok_or(CoreError::ItemNotFound)?;

        match choice {
            ConflictChoice::KeepLocal => {
                // 保留本机：丢弃远端快照。
            }
            ConflictChoice::TakeRemote => {
                if entry.remote_deleted {
                    // D-56：删除冲突「采用远端」= 接受对端的删除 → 本端**软删**
                    // （blob 保留、回收站可还原），而非复活死亡快照（§3.13.2 收敛裁定）。
                    // D-91（审查 C2）：live 已不在且回收站已有同 item_id 快照时，
                    // 再软删会造出**重复**回收站条目（还原其一即触发 R2 覆盖路径）。
                    // 此时删除意图已生效（条目不在 live），只标记 resolved 即可。
                    match self.get_item_opt(&entry.item_id) {
                        Some(local) => self.trash_item_direct(&local)?,
                        None => {
                            let already = self
                                .trash
                                .values()
                                .any(|t| t.item_snapshot.item_id == entry.item_id);
                            if !already {
                                self.trash_item_direct(&entry.local_snapshot.clone())?;
                            }
                        }
                    }
                } else {
                    let mut remote = entry.remote_snapshot.clone();
                    remote.rev = entry.local_snapshot.rev.max(remote.rev) + 1;
                    remote.updated_at = now_ms();
                    remote.modified_by = self.device_id.clone();
                    // D-106（豆包审查 3.1-6）：与 put_item_direct 同款上限校验。
                    let bytes = serde_json::to_vec(&remote)?;
                    if bytes.len() > MAX_ITEM_PLAINTEXT_BYTES {
                        return Err(CoreError::Validation(format!(
                            "sync item_too_large: {} > {MAX_ITEM_PLAINTEXT_BYTES} bytes",
                            bytes.len()
                        )));
                    }
                    self.write_record(RECORD_TYPE_ITEM, &remote.item_id, 0, &bytes)?;
                    self.items.insert(remote.item_id.clone(), remote);
                }
            }
            ConflictChoice::KeepBoth => {
                let mut dup = entry.remote_snapshot.clone();
                dup.item_id = uuid::Uuid::new_v4().to_string();
                let new_title = format!("{} (冲突副本)", dup.title());
                dup.kind.set_title(new_title);
                dup.rev = 1;
                dup.created_at = now_ms();
                dup.updated_at = now_ms();
                dup.modified_by = self.device_id.clone();
                // D-106（豆包审查 3.1-6）：与 put_item_direct 同款上限校验。
                let bytes = serde_json::to_vec(&dup)?;
                if bytes.len() > MAX_ITEM_PLAINTEXT_BYTES {
                    return Err(CoreError::Validation(format!(
                        "sync item_too_large: {} > {MAX_ITEM_PLAINTEXT_BYTES} bytes",
                        bytes.len()
                    )));
                }
                self.write_record(RECORD_TYPE_ITEM, &dup.item_id, 0, &bytes)?;
                self.items.insert(dup.item_id.clone(), dup);
            }
        }

        // 标记冲突已仲裁（写回 resolved=true）。
        let mut updated = entry;
        updated.resolved = true;
        self.write_record(
            RECORD_TYPE_CONFLICT,
            &updated.conflict_id,
            0,
            &serde_json::to_vec(&updated)?,
        )?;
        self.conflicts.insert(updated.conflict_id.clone(), updated);
        self.persist_header()?;
        Ok(())
    }

    /// 写入冲突箱（供同步合并使用）。
    pub fn add_conflict(&mut self, entry: ConflictEntry) -> CoreResult<()> {
        self.write_record(
            RECORD_TYPE_CONFLICT,
            &entry.conflict_id,
            0,
            &serde_json::to_vec(&entry)?,
        )?;
        self.conflicts.insert(entry.conflict_id.clone(), entry);
        Ok(())
    }

    /// 直接落库（同步应用远端变更；不改写 rev 语义由调用方决定）。
    ///
    /// **D-106（豆包审查 3.1-6）**：补 384KiB 统一明文上限校验——本地 create/update
    /// 有守卫，但同步路径直接 `write_record`，异常对端可绕过上限把库撑爆。
    pub fn put_item_direct(&mut self, item: Item) -> CoreResult<()> {
        let bytes = serde_json::to_vec(&item)?;
        if bytes.len() > MAX_ITEM_PLAINTEXT_BYTES {
            return Err(CoreError::Validation(format!(
                "sync item_too_large: {} > {MAX_ITEM_PLAINTEXT_BYTES} bytes",
                bytes.len()
            )));
        }
        self.write_record(RECORD_TYPE_ITEM, &item.item_id, 0, &bytes)?;
        self.items.insert(item.item_id.clone(), item);
        Ok(())
    }

    /// 直接落库分组（同步）。
    pub fn put_folder_direct(&mut self, folder: Folder) -> CoreResult<()> {
        self.write_record(
            RECORD_TYPE_FOLDER,
            &folder.folder_id,
            0,
            &serde_json::to_vec(&folder)?,
        )?;
        self.folders.insert(folder.folder_id.clone(), folder);
        Ok(())
    }

    /// 远端删除条目（直接墓碑，不进回收站）。
    ///
    /// **D-56 降级为内部保留方法**：merge 的远端删除分支改走
    /// [`Repository::trash_item_direct`]（软删，O-05 要求对端同样享有回收站误删保护），
    /// 本方法不再被同步路径调用。
    /// **D-60 §3.14.4**：D-53 时代的「连带 `tombstone_blob`」1:1 规则作废，
    /// blob 清理统一交孤儿 GC（KeepBoth 共享引用不得误杀）。
    /// 保留本方法供未来 compaction / 内部工具使用。
    pub fn remove_item_direct(&mut self, item_id: &str) -> CoreResult<()> {
        self.tombstone(RECORD_TYPE_ITEM, item_id)?;
        self.items.remove(item_id);
        self.gc_orphan_blobs()?;
        Ok(())
    }

    /// merge 专用：远端删除到达本端时执行**软删**（D-56 §3.13.2 裁定二）。
    ///
    /// 等价 [`Repository::move_item_to_trash`]：新 `trash_id` 由本端生成、`deleted_at`
    /// 为本端当前时刻（`deleted_at` 不入 wire）、**不清 blob**（回收站还原需要；
    /// blob 墓碑仅随本端 purge 发生）。
    pub fn trash_item_direct(&mut self, item: &Item) -> CoreResult<()> {
        self.move_item_to_trash(item)
    }

    /// 取条目（不存在返回 `None`，供合并探测使用）。
    pub fn get_item_opt(&self, item_id: &str) -> Option<Item> {
        self.items.get(item_id).cloned()
    }

    /// 取分组（不存在返回 `None`）。
    pub fn get_folder_opt(&self, folder_id: &str) -> Option<Folder> {
        self.folders.get(folder_id).cloned()
    }

    /// 远端删除分组（直接墓碑）。
    pub fn remove_folder_direct(&mut self, folder_id: &str) -> CoreResult<()> {
        self.tombstone(RECORD_TYPE_FOLDER, folder_id)?;
        self.folders.remove(folder_id);
        Ok(())
    }

    /// 业务记录总数（不含墓碑 / blob），用于 VK 采用前的「空库」判定。
    pub fn record_count(&self) -> usize {
        self.items.len() + self.folders.len() + self.trash.len() + self.conflicts.len()
    }

    // ===== 已配对设备与同步状态 =====

    /// 列出已配对设备。
    pub fn list_paired_devices(&self) -> Vec<PairedDevice> {
        self.devices.values().cloned().collect()
    }

    /// 取单个已配对设备。
    pub fn get_paired_device(&self, device_id: &str) -> Option<PairedDevice> {
        self.devices.get(device_id).cloned()
    }

    /// 写入 / 覆盖已配对设备（`sync_key_wrapped` 由 VK 包裹）。
    pub fn upsert_paired_device(&mut self, device: PairedDevice) -> CoreResult<()> {
        self.write_record(
            RECORD_TYPE_DEVICE,
            &device.device_id,
            0,
            &serde_json::to_vec(&device)?,
        )?;
        self.devices.insert(device.device_id.clone(), device);
        Ok(())
    }

    /// 解除配对（废弃共享密钥，旧设备被拒）。
    pub fn remove_paired_device(&mut self, device_id: &str) -> CoreResult<()> {
        self.tombstone(RECORD_TYPE_DEVICE, device_id)?;
        self.devices.remove(device_id);
        self.tombstone(RECORD_TYPE_SYNC_STATE, device_id)?;
        self.sync_states.remove(device_id);
        Ok(())
    }

    /// 取某设备的同步状态（缺省则新建）。
    pub fn sync_state(&self, device_id: &str) -> DeviceSyncState {
        self.sync_states
            .get(device_id)
            .cloned()
            .unwrap_or(DeviceSyncState {
                device_id: device_id.to_string(),
                base: std::collections::HashMap::new(),
                last_recv_seq: 0,
                last_sent_seq: 0,
                sent_watermark: std::collections::HashMap::new(),
            })
    }

    /// 持久化某设备的同步状态。
    pub fn save_sync_state(&mut self, state: DeviceSyncState) -> CoreResult<()> {
        self.write_record(
            RECORD_TYPE_SYNC_STATE,
            &state.device_id,
            0,
            &serde_json::to_vec(&state)?,
        )?;
        self.sync_states.insert(state.device_id.clone(), state);
        Ok(())
    }

    /// 更新设备最近同步时间。
    pub fn touch_device_sync(&mut self, device_id: &str, at: i64) -> CoreResult<()> {
        if let Some(mut device) = self.devices.get(device_id).cloned() {
            device.last_sync_at = Some(at);
            self.upsert_paired_device(device)?;
        }
        Ok(())
    }

    /// 刷新已配对设备的对端地址（v1.2.4 根因 B：地址自愈）。
    ///
    /// 背景：`peer_ip` / `peer_port` 此前只在**配对那一刻**写一次（`server.rs` 的
    /// `handle_stream`）。此后对端换 IP（DHCP 续租）或监听端口 rebind
    /// （`resync.port` 被占用后 fallback 到随机端口）时，本机记录永久失效 →
    /// `sync_now` 一直连不上，只能重新配对。
    ///
    /// 语义：
    /// - `peer_ip`：本次连接的**真实来源**，无条件采用；
    /// - `sync_port`：对端自报的监听端口。**为 0 时保留原值**（老客户端不发该字段，
    ///   不得把可用的端口清空）；
    /// - 只改这两个字段，`sync_key_wrapped` / `fingerprint` / `paired_at` / `last_sync_at`
    ///   一律不动（本方法不承担「记录同步时间」的职责，那是 [`Self::touch_device_sync`]）；
    /// - 值未变化时不重写记录（避免每次同步都白写一条 device 帧）；
    /// - 设备不存在 → [`CoreError::NotPaired`]（调用方本已校验过配对关系，此处防御）。
    pub fn update_paired_addr(
        &mut self,
        device_id: &str,
        peer_ip: &str,
        sync_port: u16,
    ) -> CoreResult<()> {
        let mut device = self
            .devices
            .get(device_id)
            .cloned()
            .ok_or(CoreError::NotPaired)?;
        let next_ip = Some(peer_ip.to_string());
        let next_port = if sync_port == 0 {
            device.peer_port
        } else {
            Some(sync_port)
        };
        if device.peer_ip == next_ip && device.peer_port == next_port {
            return Ok(());
        }
        device.peer_ip = next_ip;
        device.peer_port = next_port;
        self.upsert_paired_device(device)?;
        Ok(())
    }

    /// 重命名已配对设备（D-66）。
    ///
    /// 背景：默认展示名由 [`crate::sync::resync::device_display_name`] 生成，形如
    /// `Android-3dd4390a`——两台同型号手机**前缀与短 ID 都可能相同**，用户在
    /// 设备列表里无法分辨，手动同步时点错的代价是同步到错误的机器。
    ///
    /// 语义：
    /// - `name.trim()` 后为空 → [`CoreError::Validation`] `"device_name_empty"`；
    /// - trim 后**字节**长度 > 64 → [`CoreError::Validation`] `"device_name_too_long"`
    ///   （上限比 [`crate::constants::MAX_ATTACHMENT_FILENAME_BYTES`] 更紧：这是人看的
    ///   短标签，不是文件名）；
    /// - 设备不存在 → [`CoreError::NotPaired`]；
    /// - **只改 `device_name`**：`peer_ip` / `peer_port` / `sync_key_wrapped` /
    ///   `fingerprint` / `paired_at` / `last_sync_at` 一字不动（改个名字绝不能影响
    ///   连通性与密钥绑定）；
    /// - trim 后与现值相同 → 不重写记录（与 [`Self::update_paired_addr`] 的
    ///   「值未变化不重写」保持一致，避免白写 device 帧）；
    /// - 成功返回更新后的 `PairedDevice`。
    pub fn rename_paired_device(
        &mut self,
        device_id: &str,
        name: &str,
    ) -> CoreResult<PairedDevice> {
        const MAX_DEVICE_NAME_BYTES: usize = 64;
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(CoreError::Validation("device_name_empty".to_string()));
        }
        if trimmed.len() > MAX_DEVICE_NAME_BYTES {
            return Err(CoreError::Validation("device_name_too_long".to_string()));
        }
        let mut device = self
            .devices
            .get(device_id)
            .cloned()
            .ok_or(CoreError::NotPaired)?;
        if device.device_name == trimmed {
            return Ok(device);
        }
        device.device_name = trimmed.to_string();
        self.upsert_paired_device(device.clone())?;
        Ok(device)
    }

    // ===== 设置 =====

    /// 读取设置。
    pub fn settings(&self) -> VaultSettings {
        self.container.header().settings.clone()
    }

    /// 更新设置。
    pub fn update_settings(&mut self, settings: VaultSettings) -> CoreResult<VaultSettings> {
        let mut header = self.container.header().clone();
        header.settings = settings.clone();
        header.updated_at = now_ms();
        let rewrote = self.container.update_header(header)?;
        if rewrote {
            self.rebuild_index()?;
        }
        Ok(settings)
    }

    // ===== 容器压缩（D-76）=====

    /// 统计容器体积构成（**只读**，不解密 blob、不写任何字节）。
    ///
    /// 这是 D-76 的第一期能力：它直接回答「24 MB 的库里有 24 MB 是已删附件吗」。
    pub fn compaction_stats(&self) -> CoreResult<CompactionStats> {
        let plan = self.plan_compaction()?;
        let file_bytes = self.file_size()?;
        let header_bytes = self.container.record_start();
        Ok(CompactionStats {
            file_bytes,
            header_bytes,
            live_bytes: plan.live_bytes,
            tombstone_bytes: plan.tombstone_bytes,
            kept_tombstone_bytes: plan.kept_tombstone_bytes,
            reclaimable_bytes: plan.reclaimable_bytes,
            after_bytes: plan.after_bytes,
            attachment_dead_bytes: plan.attachment_dead_bytes,
            attachment_live_bytes: plan.attachment_live_bytes,
            live_frames: plan.live_frames,
            dead_frames: plan.dead_frames,
        })
    }

    /// 压缩容器：把「该丢的帧」真正从文件里去掉（重写新容器 + 原子替换）。
    ///
    /// # 保留集合（口径见 D-76 调研 §1.4，**按黑名单**——只定义"丢什么"，其余一律保留）
    ///
    /// 丢弃：
    /// 1. 全部 blob 墓碑帧（`rebuild_index` / `read_blob` / `tombstone_changes` /
    ///    `tombstone_rev` 四处都跳过它们，是纯本地 GC 记账，从不过线）；
    /// 2. 未被引用的活跃 blob（GC 还没跑到的兜底；引用集合复用
    ///    [`Self::attachment_blob_refs`]，**回收站快照里的附件算有引用**）；
    /// 3. 全部 TRASH / CONFLICT / DEVICE / SYNC_STATE 墓碑（无任何消费者）；
    /// 4. 仍是活跃记录的 ITEM / FOLDER 历史墓碑（`place_frame` 长度变化时留下的中转墓碑，
    ///    导出侧 `alive` 检查会跳过、`tombstone_rev` 在记录存在时不会被查询）；
    /// 5. 同一"已死 id"的其余低 rev 墓碑（只留 rev 最高的一条）。
    ///
    /// **ITEM / FOLDER 墓碑必须至少留一条**：`tombstone_changes` 是删除意图过线的唯一
    /// 来源，`merge::apply_changes` 的复活抑制也只认 `tombstone_rev`。删掉 → 已删条目
    /// 会被对端陈旧 live 变更复活（数据回魂）。
    ///
    /// # 崩溃安全
    ///
    /// 见 [`crate::store::container::Container::rewrite_with_frames`]：同目录 tmp →
    /// `sync_all` → rename 前完成全部校验 → 原子替换。压缩前另存
    /// `*.precompact-<unix_ms>` 备份，**成功即删**（不在磁盘上长期留旧库）。
    ///
    /// # 失败语义
    ///
    /// 任一环节失败都会把备份复制回原位再返回错误，`self` 保持"旧文件 + 旧内存"的
    /// 可用状态（`slots` 偏移与恢复出的字节一致，因为备份是压缩前文件的字节副本）。
    pub fn compact(&mut self) -> CoreResult<CompactionOutcome> {
        let before_bytes = self.file_size()?;

        // ① 先把"漏标的孤儿 blob"变成墓碑：复用已审计的引用判定（KeepBoth 共享引用、
        //    回收站快照引用都算活），幂等。这样后续规划里"活跃但无引用的 blob"理论上
        //    不再出现，计划与实际写入不会因为 GC 时序而漂移。
        self.gc_orphan_blobs()?;

        // ② 规划
        let plan = self.plan_compaction()?;
        if plan.reclaimable_bytes == 0 {
            return Ok(CompactionOutcome {
                before_bytes,
                after_bytes: before_bytes,
                reclaimed_bytes: 0,
                dropped_frames: 0,
            });
        }
        let counts_before = self.counts();

        // ③ 头部：逐字段沿用（主密码不变），只重算两个计数，**不动 updated_at**
        //    （压缩不改任何业务数据，改时间戳会让它说谎。唯一例外见下：第 ① 步的兜底 GC
        //    若真的墓碑化了孤儿 blob，那是一次真实的数据变更，`updated_at` 由
        //    `persist_header` 正常刷新——与解锁时的既有行为一致）。
        let mut header = self.container.header().clone();
        header.item_count = self.items.len() as u32;
        header.folder_count = self.folders.len() as u32;

        let frames: Vec<Frame> = plan
            .keep
            .iter()
            .map(|&idx| self.slots[idx].frame.clone())
            .collect();
        let kept_frames = frames.len();

        // ④ 备份（字节副本；压缩成功即删）
        let path = self.container.path().to_path_buf();
        let backup = self.precompact_backup_path();
        std::fs::copy(&path, &backup)?;

        // ⑤ 重写 + ⑥ rename 前的结构校验（都在 rewrite_with_frames 内部）
        if let Err(err) = self.container.rewrite_with_frames(header, &frames) {
            restore_precompact(&backup, &path);
            return Err(err);
        }

        // ⑦ 造新实例重建索引并逐项比对（不在 self 上直接 rebuild：它先 clear 后扫描，
        //    失败会留下半空状态；而此刻 self 的 slots 偏移已经与新文件不符）。
        match self.reopen_after_rewrite(&counts_before, kept_frames) {
            Ok(fresh) => *self = fresh,
            Err(err) => {
                restore_precompact(&backup, &path);
                return Err(err);
            }
        }

        // ⑧ 立即删除备份
        let _ = std::fs::remove_file(&backup);

        let after_bytes = self.file_size()?;
        Ok(CompactionOutcome {
            before_bytes,
            after_bytes,
            reclaimed_bytes: before_bytes.saturating_sub(after_bytes),
            dropped_frames: plan.dead_frames,
        })
    }

    /// 计算保留集合与体积账（只读）。
    fn plan_compaction(&self) -> CoreResult<CompactionPlan> {
        let referenced: std::collections::HashSet<String> =
            self.attachment_blob_refs().into_iter().collect();
        // 每个"已死" ITEM/FOLDER id 应保留的那一条墓碑（rev 最高）。
        let best_dead = self.best_dead_tombstones();

        let mut keep: Vec<usize> = Vec::with_capacity(self.slots.len());
        let mut live_bytes = 0u64;
        let mut tombstone_bytes = 0u64;
        let mut kept_tombstone_bytes = 0u64;
        let mut attachment_live_bytes = 0u64;
        let mut attachment_dead_bytes = 0u64;
        let mut live_frames = 0u32;
        let mut dead_frames = 0u32;

        for (idx, slot) in self.slots.iter().enumerate() {
            let frame = &slot.frame;
            let size = frame.encoded_len() as u64;
            if frame.is_tombstone() {
                tombstone_bytes += size;
            }

            let keep_it = if frame.is_blob() {
                // blob：墓碑一律丢；活跃帧只在仍被引用时保留（含回收站快照引用）。
                !frame.is_tombstone() && referenced.contains(&frame.record_id_string())
            } else if !frame.is_tombstone() {
                // 活跃业务记录：一律保留（黑名单口径 —— 未来新增的记录类型自动保住）。
                true
            } else {
                // 非 blob 墓碑：只有"已死 ITEM/FOLDER 的 rev 最高那条"才有消费者。
                matches!(frame.record_type, RECORD_TYPE_ITEM | RECORD_TYPE_FOLDER)
                    && best_dead.get(&(frame.record_type, frame.record_id_string())) == Some(&idx)
            };

            if keep_it {
                keep.push(idx);
                if frame.is_tombstone() {
                    kept_tombstone_bytes += size;
                } else {
                    live_bytes += size;
                    live_frames += 1;
                    if frame.is_blob() {
                        attachment_live_bytes += size;
                    }
                }
            } else {
                dead_frames += 1;
                if frame.is_blob() {
                    attachment_dead_bytes += size;
                }
            }
        }

        let file_bytes = self.file_size()?;
        let header_bytes = self.container.record_start();
        let kept = live_bytes + kept_tombstone_bytes;
        Ok(CompactionPlan {
            keep,
            live_bytes,
            tombstone_bytes,
            kept_tombstone_bytes,
            attachment_live_bytes,
            attachment_dead_bytes,
            live_frames,
            dead_frames,
            // 用「文件现有字节 - 头部 - 保留帧」而不是「Σ丢弃帧」：容器若存在尾部尾料
            // （异常历史），它同样会被重写顺手清掉，这里如实计入可回收量。
            reclaimable_bytes: file_bytes.saturating_sub(header_bytes + kept),
            after_bytes: header_bytes + kept,
        })
    }

    /// 每个"已死"（无活跃帧）的 ITEM/FOLDER id → rev 最高的那条墓碑的 slot 下标。
    ///
    /// 选择语义与 [`Self::tombstone_changes`] / [`Self::tombstone_rev`] **逐条对齐**：
    /// 解密失败（损坏墓碑）跳过——那两处同样跳过，所以它本来就不产生任何效果；
    /// 同 rev 取先出现的（`best_rev >= rev` 不替换）。
    fn best_dead_tombstones(&self) -> HashMap<(u8, String), usize> {
        let mut best: HashMap<(u8, String), (u64, usize)> = HashMap::new();
        for (idx, slot) in self.slots.iter().enumerate() {
            let frame = &slot.frame;
            if frame.is_blob() || !frame.is_tombstone() {
                continue;
            }
            if frame.record_type != RECORD_TYPE_ITEM && frame.record_type != RECORD_TYPE_FOLDER {
                continue;
            }
            let id = frame.record_id_string();
            // 该记录仍活跃 → 历史墓碑不表达删除意图（导出侧 alive 检查同样跳过）。
            let alive = match frame.record_type {
                RECORD_TYPE_ITEM => self.items.contains_key(&id),
                _ => self.folders.contains_key(&id),
            };
            if alive {
                continue;
            }
            let rev = match self.open_frame(frame) {
                Ok(pt) => {
                    if frame.record_type == RECORD_TYPE_ITEM {
                        serde_json::from_slice::<Item>(&pt).ok().map(|i| i.rev)
                    } else {
                        serde_json::from_slice::<Folder>(&pt).ok().map(|f| f.rev)
                    }
                }
                Err(_) => None,
            };
            let Some(rev) = rev else { continue };
            match best.get(&(frame.record_type, id.clone())) {
                Some(&(best_rev, _)) if best_rev >= rev => {}
                _ => {
                    best.insert((frame.record_type, id), (rev, idx));
                }
            }
        }
        best.into_iter().map(|(key, (_, idx))| (key, idx)).collect()
    }

    /// 各业务 map 的计数快照（压缩前后比对用）。
    fn counts(&self) -> RepositoryCounts {
        RepositoryCounts {
            items: self.items.len(),
            folders: self.folders.len(),
            trash: self.trash.len(),
            conflicts: self.conflicts.len(),
            devices: self.devices.len(),
            sync_states: self.sync_states.len(),
            blobs: self.blobs.len(),
        }
    }

    /// 压缩备份路径：`<vault>.precompact-<unix_ms>`。
    fn precompact_backup_path(&self) -> std::path::PathBuf {
        let path = self.container.path().to_path_buf();
        let mut name = path
            .file_name()
            .map(|s| s.to_os_string())
            .unwrap_or_default();
        name.push(format!(".precompact-{}", now_ms()));
        path.with_file_name(name)
    }

    /// 压缩后重开文件、重建索引、逐项比对（全部一致才返回新实例）。
    ///
    /// 只在**新实例**上 `rebuild_index`：它先 `clear()` 再扫描，失败会留下半空状态，
    /// 不能让 `self` 冒这个险（此刻 `self.slots` 的偏移已与新文件不符）。
    fn reopen_after_rewrite(
        &self,
        expect: &RepositoryCounts,
        expect_frames: usize,
    ) -> CoreResult<Repository> {
        let container = Container::open(self.container.path())?;
        let mut fresh = Repository {
            container,
            vault_key: self.vault_key.clone(),
            device_id: self.device_id.clone(),
            slots: Vec::new(),
            live: HashMap::new(),
            blobs: HashMap::new(),
            folders: BTreeMap::new(),
            items: BTreeMap::new(),
            trash: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            devices: BTreeMap::new(),
            sync_states: BTreeMap::new(),
            open_warnings: Vec::new(),
        };
        // 逐帧解密归位：任何一帧解不开都会在这里暴露（这本身就是一次全量完整性校验）。
        fresh.rebuild_index()?;
        if fresh.slots.len() != expect_frames {
            return Err(CoreError::Internal(format!(
                "compacted frame count mismatch: expected {expect_frames}, reopened {}",
                fresh.slots.len()
            )));
        }
        let got = fresh.counts();
        if got != *expect {
            return Err(CoreError::Internal(format!(
                "compacted record counts mismatch: expected {expect:?}, reopened {got:?}"
            )));
        }
        Ok(fresh)
    }

    // ===== 导出 =====

    /// 导出保险库副本（需重新验证主密码，见 O-03）。
    pub fn export_vault(&self, dest_path: &Path, master_password: &str) -> CoreResult<()> {
        let header = self.container.header();
        let mk =
            crate::crypto::kdf::derive_master_key(master_password, &header.kdf_salt, &header.kdf)?;
        let kek = crate::crypto::kdf::derive_kek(&mk[..])?;
        let verify_key = crate::crypto::kdf::derive_verify_key(&mk[..])?;
        // 复用统一解锁校验：任何失败都映射为同一错误。
        crate::crypto::verify::unlock_check(
            &kek,
            &verify_key,
            &header.verify_mac,
            &header.wrapped_vault_key,
        )?;
        std::fs::copy(self.container.path(), dest_path)?;
        Ok(())
    }

    // ===== 同步辅助 =====

    /// 导出待同步变更（D-56 签名变更：增加对端 base 做免重发过滤）。
    ///
    /// 载荷构成（§3.13.6）：
    /// - **live 白名单**：仅 `ITEM` / `FOLDER`（TRASH/CONFLICT/DEVICE/SYNC_STATE 不过线，
    ///   修复「内部记录全量出网」缺陷）；
    /// - **墓碑**：本端 `ITEM`/`FOLDER` 墓碑帧（blob 墓碑不独立导出——item 墓碑已完整
    ///   表达删除意图），经 peer base 过滤（`base.rev >= 墓碑 rev` 不导出）；
    /// - **O1 发送水位**：live 记录仅导出 `rev > sent_watermark[id]`（每对端一份，
    ///   发送成功后由 `merge::note_sent_changes` 推进）——修复「每轮全量重发、大库
    ///   单帧必超限且永不收敛」的结构性缺口。水位为空（首次配对/老状态帧）= 全量
    ///   首推，行为不变。
    pub fn export_changes(&self, peer_base: &DeviceSyncState) -> Vec<ChangeRecord> {
        let mut out = Vec::new();
        for (key, idx) in &self.live {
            // 类型白名单：只有业务记录过线（D-56 ⑥）。
            if key.0 != RECORD_TYPE_ITEM && key.0 != RECORD_TYPE_FOLDER {
                continue;
            }
            let frame = &self.slots[*idx].frame;
            let (rev, updated_at, modified_by) = match key.0 {
                RECORD_TYPE_FOLDER => self
                    .folders
                    .get(&key.1)
                    .map(|f| (f.rev, f.updated_at, String::new()))
                    .unwrap_or((0, 0, String::new())),
                RECORD_TYPE_ITEM => self
                    .items
                    .get(&key.1)
                    .map(|i| (i.rev, i.updated_at, i.modified_by.clone()))
                    .unwrap_or((0, 0, String::new())),
                _ => (0, 0, String::new()),
            };
            // O1 发送水位：该对端已持有同 rev 或更新版本 → 免重发。
            if let Some(&sent_rev) = peer_base.sent_watermark.get(&key.1) {
                if rev <= sent_rev {
                    continue;
                }
            }
            out.push(ChangeRecord {
                record_type: frame.record_type,
                record_id: key.1.clone(),
                rev,
                updated_at,
                modified_by,
                deleted: false,
                wrapped_key: frame.wrapped_key.ct.clone(),
                wrapped_key_nonce: frame.wrapped_key.nonce.clone(),
                ciphertext: frame.ct.clone(),
                nonce: frame.nonce.to_vec(),
                plaintext_len: frame.plaintext_len,
            });
        }
        if let Ok(tombstones) = self.tombstone_changes(peer_base) {
            out.extend(tombstones);
        }
        out
    }

    /// 枚举本端墓碑并构造 `ChangeRecord { deleted: true }`（D-56 §3.13.1 裁定一）。
    ///
    /// - **零格式变更**：`tombstone()` 仅置位 flags、原 payload（删除前密文）原样保留
    ///   （实现前提已核实：repository.rs `tombstone`/`place_frame` 均为 `frame.clone()`
    ///   后 `flags |= FLAG_TOMBSTONE` 原地 pwrite）→ 解密原 payload 即取到 rev/updated_at；
    /// - 同一 `(type, id)` 可能存在多个历史墓碑帧（`place_frame` 长度变化时的中转墓碑），
    ///   取 **rev 最高**者（真正的删除墓碑）；
    /// - 该 id 仍有活跃记录（删除后被还原）→ 历史墓碑不再表达删除意图，跳过；
    /// - 解密失败（老库损坏墓碑）→ 跳过该帧**不中断**（统一错误语义，A-05；不外泄、
    ///   不计数上报——损坏墓碑静默缺席，重同步时对端亦无该记录可补）；
    /// - `wrapped_key/nonce/ciphertext` 取自墓碑帧原字段（对端凭共享 VK 可解出死亡快照）。
    pub fn tombstone_changes(&self, peer_base: &DeviceSyncState) -> CoreResult<Vec<ChangeRecord>> {
        // (type, id) -> (rev, updated_at, modified_by, slot 下标)。
        let mut best: HashMap<(u8, String), (u64, i64, String, usize)> = HashMap::new();
        for (idx, slot) in self.slots.iter().enumerate() {
            let frame = &slot.frame;
            if frame.is_blob() || !frame.is_tombstone() {
                continue;
            }
            if frame.record_type != RECORD_TYPE_ITEM && frame.record_type != RECORD_TYPE_FOLDER {
                continue;
            }
            let id = frame.record_id_string();
            // 复活取代：该 id 仍活跃 → 其历史墓碑一律不导出。
            let alive = match frame.record_type {
                RECORD_TYPE_ITEM => self.items.contains_key(&id),
                _ => self.folders.contains_key(&id),
            };
            if alive {
                continue;
            }
            let (rev, updated_at, modified_by) = match self.open_frame(frame) {
                Ok(pt) => {
                    if frame.record_type == RECORD_TYPE_ITEM {
                        match serde_json::from_slice::<Item>(&pt) {
                            Ok(item) => (item.rev, item.updated_at, item.modified_by),
                            Err(_) => continue,
                        }
                    } else {
                        match serde_json::from_slice::<Folder>(&pt) {
                            Ok(folder) => (folder.rev, folder.updated_at, String::new()),
                            Err(_) => continue,
                        }
                    }
                }
                Err(_) => continue,
            };
            match best.get(&(frame.record_type, id.clone())) {
                Some(&(best_rev, ..)) if best_rev >= rev => {}
                _ => {
                    best.insert((frame.record_type, id), (rev, updated_at, modified_by, idx));
                }
            }
        }
        let mut out = Vec::with_capacity(best.len());
        for ((record_type, id), (rev, updated_at, modified_by, idx)) in best {
            // peer base 免重发过滤（§3.13.3 + 实现补注）：仅当 base 已**推进越过**该
            // 墓碑 rev，或 base 恰为该 rev 的**墓碑标记**（hash=[0;32]，对端已应用过）
            // 时才不导出。base 为同 rev 活跃条目（hash 非零）= 该墓碑首次到达，必须导出
            // ——否则常规流「同步 rev1 → 删 rev1」的删除永远不过线。
            if let Some(base) = peer_base.base.get(&id) {
                if base.rev > rev || (base.rev == rev && base.hash == [0u8; 32]) {
                    continue;
                }
            }
            let frame = &self.slots[idx].frame;
            out.push(ChangeRecord {
                record_type,
                record_id: id,
                rev,
                updated_at,
                modified_by,
                deleted: true,
                wrapped_key: frame.wrapped_key.ct.clone(),
                wrapped_key_nonce: frame.wrapped_key.nonce.clone(),
                ciphertext: frame.ct.clone(),
                nonce: frame.nonce.to_vec(),
                plaintext_len: frame.plaintext_len,
            });
        }
        out.sort_by(|a, b| a.record_id.cmp(&b.record_id));
        Ok(out)
    }

    /// 本端某记录当前墓碑的最大 payload rev（无墓碑 → `None`）。
    ///
    /// merge 的 live 分支用于「本端已删、对端变更不新于删除」的**复活抑制**（D-56）：
    /// 本端本地无该条目但持有同 rev 墓碑时，对端发来的陈旧 live 变更不得把条目复活。
    /// 解密失败的墓碑帧跳过（与 [`Repository::tombstone_changes`] 同一容错语义）。
    pub fn tombstone_rev(&self, record_type: u8, id: &str) -> Option<u64> {
        let mut best: Option<u64> = None;
        for slot in &self.slots {
            let frame = &slot.frame;
            if frame.is_blob()
                || !frame.is_tombstone()
                || frame.record_type != record_type
                || frame.record_id_string() != id
            {
                continue;
            }
            let parsed_rev = match self.open_frame(frame) {
                Ok(pt) => {
                    if record_type == RECORD_TYPE_ITEM {
                        serde_json::from_slice::<Item>(&pt).ok().map(|i| i.rev)
                    } else {
                        serde_json::from_slice::<Folder>(&pt).ok().map(|f| f.rev)
                    }
                }
                Err(_) => None,
            };
            if let Some(rev) = parsed_rev {
                if best.is_none_or(|b| rev > b) {
                    best = Some(rev);
                }
            }
        }
        best
    }

    /// 解密一条远端变更（对端与本端共享同一 VK）。
    pub fn decrypt_change(&self, change: &ChangeRecord) -> CoreResult<zeroize::Zeroizing<Vec<u8>>> {
        let rid = record_id_to_bytes(&change.record_id)?;
        let aad = keys::record_aad(change.record_type, &rid);
        let wrapped = WrappedKey {
            nonce: change.wrapped_key_nonce.clone(),
            ct: change.wrapped_key.clone(),
        };
        let ik = keys::unwrap_item_key(&self.vault_key, &wrapped, &aad)?;
        if change.nonce.len() != crate::constants::AEAD_NONCE_LEN {
            return Err(CoreError::Crypto("sync nonce".to_string()));
        }
        aead::open(&ik[..], &change.nonce, &aad, &change.ciphertext)
    }

    /// 从变更中解出条目。
    pub fn change_to_item(&self, change: &ChangeRecord) -> CoreResult<Item> {
        let pt = self.decrypt_change(change)?;
        Ok(serde_json::from_slice(&pt)?)
    }

    /// 从变更中解出分组。
    pub fn change_to_folder(&self, change: &ChangeRecord) -> CoreResult<Folder> {
        let pt = self.decrypt_change(change)?;
        Ok(serde_json::from_slice(&pt)?)
    }

    // ===== 明文擦除（A-04）=====

    /// 显式擦除内存中的条目明文（A-04 / 安全需求 §3.2）。
    ///
    /// `items` / `trash` / `conflicts` 持有的是**解密后的 `String` 明文**；仅靠 `Drop`
    /// 只会释放内存而**不会覆写**，可能被内存转储或换页残留。此处在切换到锁定态**之前**
    /// 对所有用户可辨识的明文字段调用 [`zeroize::Zeroize::zeroize`] 覆写。
    ///
    /// 覆盖范围：
    /// - **必擦**：`password` / `totp_secret` / `notes` / 笔记 `body` / 自定义字段 `key`+`value`
    ///   —— 凭据本体或可直接推导凭据的信息；
    /// - **同擦**：`title` / `url` / `username` / 附件 `original_filename`（条目），
    ///   以及分组 `name` / `icon` / `color`
    ///   —— 「用了哪些站点 / 账号 / 文件名 / 分组主题」本身即情报，与 A-04 一致一并擦除；
    /// - **不擦**：`mime_type`、`blob_ref`、时间戳 / 修订号 / 布尔标志等不可反推凭据的元数据。
    pub fn wipe_plaintext(&mut self) {
        for item in self.items.values_mut() {
            item.kind.wipe_plaintext();
            for a in item.attachments.iter_mut() {
                a.wipe_plaintext();
            }
        }
        for folder in self.folders.values_mut() {
            folder.wipe_plaintext();
        }
        for entry in self.trash.values_mut() {
            entry.item_snapshot.kind.wipe_plaintext();
            for a in entry.item_snapshot.attachments.iter_mut() {
                a.wipe_plaintext();
            }
        }
        for entry in self.conflicts.values_mut() {
            entry.local_snapshot.kind.wipe_plaintext();
            entry.remote_snapshot.kind.wipe_plaintext();
            for a in entry.local_snapshot.attachments.iter_mut() {
                a.wipe_plaintext();
            }
            for a in entry.remote_snapshot.attachments.iter_mut() {
                a.wipe_plaintext();
            }
        }
    }
}

/// 压缩失败时把备份复制回原位（D-76）。
///
/// 刻意**不返回结果**：调用点已经在错误路径上，回滚失败不该覆盖原始错误。但回滚失败时
/// **必须保留备份文件**——那是用户数据最后的副本，删掉等于把失败升级成数据丢失。
fn restore_precompact(backup: &Path, path: &Path) {
    match std::fs::copy(backup, path) {
        Ok(_) => {
            let _ = std::fs::remove_file(backup);
        }
        Err(_) => {
            // 备份保留在磁盘上（文件名含 unix_ms，可由外壳层日志指引用户取回）。
        }
    }
}

/// 计算内容的 32 字节摘要（用于冲突「双改同内容」判定）。
pub fn hash32(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// 条目内容摘要（用于同步基准比对）。
pub fn item_hash(item: &Item) -> CoreResult<[u8; 32]> {
    Ok(hash32(&serde_json::to_vec(item)?))
}

/// 分组内容摘要（用于同步基准比对）。
pub fn folder_hash(folder: &Folder) -> CoreResult<[u8; 32]> {
    Ok(hash32(&serde_json::to_vec(folder)?))
}

/// 模糊匹配：标题 / 用户名 / 网址（F-08）。
fn item_matches(item: &Item, needle: &str) -> bool {
    let title = item.title().to_lowercase();
    if title.contains(needle) {
        return true;
    }
    if let ItemKind::Login(login) = &item.kind {
        if let Some(u) = &login.username {
            if u.to_lowercase().contains(needle) {
                return true;
            }
        }
        if let Some(url) = &login.url {
            if url.to_lowercase().contains(needle) {
                return true;
            }
        }
    }
    false
}

/// 由文件名推断 MIME（不引入额外依赖，覆盖常见类型，默认 octet-stream）。
fn guess_mime(filename: &str) -> String {
    let ext = filename
        .rsplit('.')
        .next()
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "html" | "htm" => "text/html",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "pem" => "application/x-pem-file",
        "crt" | "cer" => "application/x-x509-ca-cert",
        "key" => "application/octet-stream",
        "p12" | "pfx" => "application/x-pkcs12",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

/// 便于调试：不输出任何密文内容。
impl std::fmt::Debug for Repository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repository")
            .field("items", &self.items.len())
            .field("folders", &self.folders.len())
            .field("trash", &self.trash.len())
            .field("conflicts", &self.conflicts.len())
            .finish()
    }
}

/// 兜底：`Repository` 无论因何原因被丢弃（含未走 `Session::lock` 的路径，如进程退出、
/// 改主密码失败回滚等），都先擦除内存明文（A-04）。
impl Drop for Repository {
    fn drop(&mut self) {
        self.wipe_plaintext();
    }
}

// 保留 `bytes_to_record_id` 供后续（同步）使用，避免未使用告警。
#[allow(dead_code)]
fn _keep_bytes_to_record_id(bytes: &[u8; 16]) -> String {
    bytes_to_record_id(bytes)
}
