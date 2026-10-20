//! 保险库仓储：条目 / 分组 / 回收站 / 冲突 / 设置 / 附件 的 CRUD（架构文档 §4.3、§6.③）。
//!
//! 逐条加密：每条记录使用独立 IK，IK 由 VK 包裹并随帧存储；修改单条仅重写该帧
//! （长度不变则原位移位写入），未变帧密文完全复用 → 满足 A-02。

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::constants::{
    ATTACHMENT_CHUNK_BYTES, FLAG_BLOB, MAX_ATTACHMENT_BYTES, MAX_ATTACHMENT_FILENAME_BYTES,
    MAX_ATTACHMENTS_PER_ITEM, MAX_NOTE_BYTES, MAX_VAULT_BYTES, MS_PER_DAY, RECORD_TYPE_CONFLICT,
    RECORD_TYPE_DEVICE, RECORD_TYPE_FOLDER, RECORD_TYPE_ITEM, RECORD_TYPE_SYNC_STATE,
    RECORD_TYPE_TRASH, TRASH_RETENTION_DAYS,
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
        };
        repo.rebuild_index()?;
        Ok(repo)
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
            return Err(CoreError::Validation(format!("invalid_auto_lock_minutes: {minutes}")));
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
    fn build_frame(&self, record_type: u8, record_id: &[u8; 16], flags: u8, payload: &[u8]) -> CoreResult<Frame> {
        let aad = keys::record_aad(record_type, record_id);
        let ik = rng::random_key()?;
        let wrapped_key = keys::wrap_item_key(&self.vault_key, &ik, &aad)?;        let nonce = rng::random_nonce()?;
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
    fn write_record(&mut self, record_type: u8, id: &str, flags: u8, payload: &[u8]) -> CoreResult<()> {
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
    fn place_frame(&mut self, idx: usize, frame: Frame) -> CoreResult<usize> {
        if self.slots[idx].frame.encoded_len() == frame.encoded_len() {
            self.container.pwrite_frame(self.slots[idx].offset, &frame)?;
            self.slots[idx].frame = frame;
            return Ok(idx);
        }
        let mut tomb = self.slots[idx].frame.clone();
        tomb.flags |= crate::constants::FLAG_TOMBSTONE;
        self.container.pwrite_frame(self.slots[idx].offset, &tomb)?;
        self.slots[idx].frame = tomb;

        let offset = self.container.append_frame(&frame)?;
        self.slots.push(Slot { offset, frame });
        Ok(self.slots.len() - 1)
    }

    /// 将一条活跃记录置为墓碑。
    fn tombstone(&mut self, record_type: u8, id: &str) -> CoreResult<()> {
        if let Some(idx) = self.live.remove(&(record_type, id.to_string())) {
            let mut tomb = self.slots[idx].frame.clone();
            tomb.flags |= crate::constants::FLAG_TOMBSTONE;
            self.container.pwrite_frame(self.slots[idx].offset, &tomb)?;
            self.slots[idx].frame = tomb;
        }
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
    pub fn update_key_material(&mut self, wrapped_vault_key: WrappedKey, verify_mac: Vec<u8>) -> CoreResult<()> {
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

    /// 列出全部分组（按 sort_order 升序）。
    pub fn list_folders(&self) -> Vec<Folder> {
        let mut out: Vec<Folder> = self.folders.values().cloned().collect();
        out.sort_by_key(|f| (f.sort_order, f.folder_id.clone()));
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
        let sort_order = self.folders.values().map(|f| f.sort_order).max().unwrap_or(-1) + 1;
        let folder = Folder {
            folder_id: uuid::Uuid::new_v4().to_string(),
            name,
            icon,
            color,
            sort_order,
            rev: 1,
            updated_at: now_ms(),
        };
        self.write_record(RECORD_TYPE_FOLDER, &folder.folder_id, 0, &serde_json::to_vec(&folder)?)?;
        self.folders.insert(folder.folder_id.clone(), folder.clone());
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
        self.write_record(RECORD_TYPE_FOLDER, folder_id, 0, &serde_json::to_vec(&folder)?)?;
        self.folders.insert(folder_id.to_string(), folder.clone());
        Ok(folder)
    }

    /// 删除分组（其下条目按策略处置）。
    pub fn delete_folder(&mut self, folder_id: &str, strategy: FolderDeleteStrategy) -> CoreResult<()> {
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
                    self.write_record(RECORD_TYPE_ITEM, &item.item_id, 0, &serde_json::to_vec(&item)?)?;
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
                self.write_record(RECORD_TYPE_FOLDER, &snapshot.folder_id, 0, &serde_json::to_vec(&snapshot)?)?;
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
            self.write_record(RECORD_TYPE_FOLDER, &folder.folder_id, 0, &serde_json::to_vec(&folder)?)?;
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
    pub fn list_items(&self, folder_id: Option<&str>, favorite_only: bool, query: Option<&str>) -> Vec<Item> {
        let needle = query.map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
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
        // 收藏置顶，其次按更新时间倒序。
        out.sort_by(|a, b| {
            b.favorite
                .cmp(&a.favorite)
                .then(b.updated_at.cmp(&a.updated_at))
                .then(a.item_id.cmp(&b.item_id))
        });
        out
    }

    /// 取单条。
    pub fn get_item(&self, item_id: &str) -> CoreResult<Item> {
        self.items.get(item_id).cloned().ok_or(CoreError::ItemNotFound)
    }

    /// 新建条目。
    pub fn create_item(&mut self, draft: ItemDraft) -> CoreResult<Item> {
        self.validate_item_kind(&draft.kind)?;
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
        };
        self.write_record(RECORD_TYPE_ITEM, &item.item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item.item_id.clone(), item.clone());
        self.persist_header()?;
        Ok(item)
    }

    /// 更新条目（逐条加密：仅重写该条）。
    pub fn update_item(&mut self, item_id: &str, draft: ItemDraft) -> CoreResult<Item> {
        self.validate_item_kind(&draft.kind)?;
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
        self.write_record(RECORD_TYPE_TRASH, &entry.trash_id, 0, &serde_json::to_vec(&entry)?)?;
        self.trash.insert(entry.trash_id.clone(), entry);
        self.tombstone(RECORD_TYPE_ITEM, &item.item_id)?;
        self.items.remove(&item.item_id);
        Ok(())
    }

    fn validate_item_kind(&self, kind: &ItemKind) -> CoreResult<()> {
        if kind.title().trim().is_empty() {
            return Err(CoreError::Validation("title".to_string()));
        }
        if let ItemKind::SecureNote(note) = kind {
            if note.body.len() > MAX_NOTE_BYTES {
                return Err(CoreError::Validation("note_too_large".to_string()));
            }
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
    #[allow(deprecated)]
    pub fn add_attachment(
        &mut self,
        item_id: &str,
        source_path: &Path,
        title: &str,
    ) -> CoreResult<Item> {
        let _ = title; // 展示名 = filename；参数保留以维持 IPC 形状（§3.14.5）
        let item = self.get_item(item_id)?.clone();
        if matches!(item.kind, ItemKind::Attachment(_)) {
            // 遗留型条目在迁移后不应存在；防御性拒绝。
            return Err(CoreError::Validation("legacy_attachment_item".to_string()));
        }
        if item.attachments.len() >= MAX_ATTACHMENTS_PER_ITEM {
            return Err(CoreError::AttachmentLimit);
        }

        let meta = std::fs::metadata(source_path)?;
        let size = meta.len();
        if size > MAX_ATTACHMENT_BYTES {
            return Err(CoreError::AttachmentTooLarge);
        }
        let current = self.file_size()?;
        // 预留密文膨胀（tag 16B + 帧头 + IK 包裹）的余量。
        if current + size + 4096 > MAX_VAULT_BYTES {
            return Err(CoreError::VaultCapacityExceeded);
        }

        let data = std::fs::read(source_path)?;
        let filename = source_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "attachment".to_string());
        if filename.len() > MAX_ATTACHMENT_FILENAME_BYTES {
            return Err(CoreError::Validation("filename_too_long".to_string()));
        }
        let mime = guess_mime(&filename);

        let attachment_id = uuid::Uuid::new_v4().to_string();
        let blob_ct_len = data.len() + crate::constants::AEAD_TAG_LEN;
        let attachment = AttachmentMeta {
            attachment_id: attachment_id.clone(),
            filename,
            mime_type: mime,
            size_bytes: size,
            blob_size_bytes: blob_ct_len as u64,
        };

        // blob 帧：record_id = attachment_id（§3.14.1 解耦；spool / 拉取清单同键）。
        self.write_record(RECORD_TYPE_ITEM, &attachment_id, FLAG_BLOB, &data)?;

        let mut updated = item;
        updated.attachments.push(attachment);
        updated.rev += 1;
        updated.updated_at = now_ms();
        updated.modified_by = self.device_id.clone();
        self.write_record(RECORD_TYPE_ITEM, item_id, 0, &serde_json::to_vec(&updated)?)?;
        self.items.insert(item_id.to_string(), updated.clone());
        self.persist_header()?;
        Ok(updated)
    }

    /// 从父条目移除单个附件（§3.14.5 `remove_attachment`）。
    ///
    /// 父条目 rev+1 正常传播（元数据减法随父帧过线）；blob **不立即墓碑**——
    /// 交孤儿 GC（§3.14.4）：该 attachment_id 脱离引用集合后，下一次 GC 才 tombstone。
    pub fn remove_attachment(&mut self, item_id: &str, attachment_id: &str) -> CoreResult<Item> {
        let mut item = self.get_item(item_id)?.clone();
        let before = item.attachments.len();
        item.attachments.retain(|a| a.attachment_id != attachment_id);
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
    pub fn read_blob(&self, blob_ref: &str) -> CoreResult<Vec<u8>> {
        let idx = *self.blobs.get(blob_ref).ok_or(CoreError::ItemNotFound)?;
        let frame = &self.slots[idx].frame;
        Ok(self.open_frame(frame)?.to_vec())
    }

    /// 导出附件到目标路径（§3.14.5：按 `(item_id, attachment_id)` 定位）。
    ///
    /// D-53 语义保留：附件元数据存在但 blob 未同步到本机时，报 `ERR_BLOB_MISSING`。
    pub fn export_attachment(
        &self,
        item_id: &str,
        attachment_id: &str,
        dest_path: &Path,
    ) -> CoreResult<()> {
        let item = self.get_item(item_id)?;
        let meta = item
            .attachments
            .iter()
            .find(|a| a.attachment_id == attachment_id)
            .ok_or(CoreError::ItemNotFound)?;
        if !self.has_blob(&meta.attachment_id) {
            return Err(CoreError::BlobMissing);
        }
        let data = self.read_blob(&meta.attachment_id)?;
        std::fs::write(dest_path, &data)?;
        Ok(())
    }

    /// 附件分片（同步用，`U4`：≤ 1 MB）。
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
    /// D-60 §3.14.2/§3.14.4 修订：清单来源 = **活跃条目的 `attachments[].attachment_id`**
    /// ∪ **回收站快照的 `attachments[].attachment_id`**（还原必须可用）；
    /// 另含遗留 `ItemKind::Attachment` 条目的 `blob_ref`（迁移前的存量库兼容窗口，
    /// 保证迁移前同步/GC 行为与 D-53 时代一致）。
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
        self.blobs.get(blob_ref).map(|&idx| self.slots[idx].frame.clone())
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
            return Ok(if same { BlobWriteOutcome::Skipped } else { BlobWriteOutcome::KeptLocal });
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
        out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
        out
    }

    /// 还原条目。
    pub fn restore_item(&mut self, trash_id: &str) -> CoreResult<Item> {
        let entry = self.trash.get(trash_id).cloned().ok_or(CoreError::ItemNotFound)?;
        let mut item = entry.item_snapshot;
        item.rev += 1;
        item.updated_at = now_ms();
        item.modified_by = self.device_id.clone();
        self.write_record(RECORD_TYPE_ITEM, &item.item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item.item_id.clone(), item.clone());
        self.tombstone(RECORD_TYPE_TRASH, trash_id)?;
        self.trash.remove(trash_id);
        self.persist_header()?;
        Ok(item)
    }

    /// 彻底删除回收站条目（D-60 §3.14.4：blob 清理交孤儿 GC，不再 1:1 连带）。
    pub fn purge_item(&mut self, trash_id: &str) -> CoreResult<()> {
        if !self.trash.contains_key(trash_id) {
            return Err(CoreError::ItemNotFound);
        }
        self.tombstone(RECORD_TYPE_TRASH, trash_id)?;
        self.trash.remove(trash_id);
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
        let mut out: Vec<ConflictEntry> = self.conflicts.values().filter(|c| !c.resolved).cloned().collect();
        out.sort_by(|a, b| b.detected_at.cmp(&a.detected_at));
        out
    }

    /// 三选一仲裁。
    pub fn resolve_conflict(&mut self, conflict_id: &str, choice: ConflictChoice) -> CoreResult<()> {
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
                    let local = self
                        .get_item_opt(&entry.item_id)
                        .unwrap_or_else(|| entry.local_snapshot.clone());
                    self.trash_item_direct(&local)?;
                } else {
                    let mut remote = entry.remote_snapshot.clone();
                    remote.rev = entry.local_snapshot.rev.max(remote.rev) + 1;
                    remote.updated_at = now_ms();
                    remote.modified_by = self.device_id.clone();
                    self.write_record(RECORD_TYPE_ITEM, &remote.item_id, 0, &serde_json::to_vec(&remote)?)?;
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
                self.write_record(RECORD_TYPE_ITEM, &dup.item_id, 0, &serde_json::to_vec(&dup)?)?;
                self.items.insert(dup.item_id.clone(), dup);
            }
        }

        // 标记冲突已仲裁（写回 resolved=true）。
        let mut updated = entry;
        updated.resolved = true;
        self.write_record(RECORD_TYPE_CONFLICT, &updated.conflict_id, 0, &serde_json::to_vec(&updated)?)?;
        self.conflicts.insert(updated.conflict_id.clone(), updated);
        self.persist_header()?;
        Ok(())
    }

    /// 写入冲突箱（供同步合并使用）。
    pub fn add_conflict(&mut self, entry: ConflictEntry) -> CoreResult<()> {
        self.write_record(RECORD_TYPE_CONFLICT, &entry.conflict_id, 0, &serde_json::to_vec(&entry)?)?;
        self.conflicts.insert(entry.conflict_id.clone(), entry);
        Ok(())
    }

    /// 直接落库（同步应用远端变更；不改写 rev 语义由调用方决定）。
    pub fn put_item_direct(&mut self, item: Item) -> CoreResult<()> {
        self.write_record(RECORD_TYPE_ITEM, &item.item_id, 0, &serde_json::to_vec(&item)?)?;
        self.items.insert(item.item_id.clone(), item);
        Ok(())
    }

    /// 直接落库分组（同步）。
    pub fn put_folder_direct(&mut self, folder: Folder) -> CoreResult<()> {
        self.write_record(RECORD_TYPE_FOLDER, &folder.folder_id, 0, &serde_json::to_vec(&folder)?)?;
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
        self.write_record(RECORD_TYPE_DEVICE, &device.device_id, 0, &serde_json::to_vec(&device)?)?;
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
        self.sync_states.get(device_id).cloned().unwrap_or(DeviceSyncState {
            device_id: device_id.to_string(),
            base: std::collections::HashMap::new(),
            last_recv_seq: 0,
            last_sent_seq: 0,
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

    // ===== 导出 =====

    /// 导出保险库副本（需重新验证主密码，见 O-03）。
    pub fn export_vault(&self, dest_path: &Path, master_password: &str) -> CoreResult<()> {
        let header = self.container.header();
        let mk = crate::crypto::kdf::derive_master_key(master_password, &header.kdf_salt, &header.kdf)?;
        let kek = crate::crypto::kdf::derive_kek(&mk[..])?;
        let verify_key = crate::crypto::kdf::derive_verify_key(&mk[..])?;
        // 复用统一解锁校验：任何失败都映射为同一错误。
        crate::crypto::verify::unlock_check(&kek, &verify_key, &header.verify_mac, &header.wrapped_vault_key)?;
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
    ///   表达删除意图），经 peer base 过滤（`base.rev >= 墓碑 rev` 不导出）。
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
                if best.map_or(true, |b| rev > b) {
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
