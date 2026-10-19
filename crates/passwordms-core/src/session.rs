//! 会话生命周期：创建 / 解锁 / 锁定 / 改主密码（架构文档 §2.1、§2.5、§2.7、§6.①②）。
//!
//! - **主密码不落盘**：仅在内存中派生 MK → KEK，随后 MK 立即被 `Zeroizing` 清零。
//! - **锁定即清零（A-04）**：`lock()` 将 `Session` 置为 `Locked`，旧 `Unlocked` 立即 Drop，
//!   级联清零 KEK / VK 及所有 `Zeroizing` 缓冲。
//! - **统一报错（A-05）**：解锁失败一律 [`CoreError::UnlockFailed`]。

use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::constants::MIN_MASTER_PASSWORD_LEN;
use crate::crypto::keys::{self, Kek, VaultKey};
use crate::crypto::{kdf, verify};
use crate::crypto::rng;
use crate::error::{CoreError, CoreResult};
use crate::model::now_ms;
use crate::model::vault::{KdfParams, Vault};
use crate::store::container::Container;
use crate::store::repository::Repository;

/// 会话状态。
pub enum SessionState {
    /// 已锁定（无任何敏感数据在内存）。
    Locked,
    /// 已解锁。
    Unlocked(Unlocked),
}

/// 解锁后持有的全部敏感上下文；`Drop` 时级联清零。
pub struct Unlocked {
    pub repo: Repository,
    pub kek: Kek,
}

/// 会话。
pub struct Session {
    state: SessionState,
}

impl Default for Session {
    fn default() -> Self {
        Session::locked()
    }
}

impl Session {
    /// 构造一个锁定态会话。
    pub fn locked() -> Self {
        Self {
            state: SessionState::Locked,
        }
    }

    /// 是否已解锁。
    pub fn is_unlocked(&self) -> bool {
        matches!(self.state, SessionState::Unlocked(_))
    }

    /// 是否有保险库文件存在。
    pub fn has_vault(path: &Path) -> bool {
        path.exists()
    }

    /// 创建保险库并返回已解锁会话（F-01）。
    pub fn create(path: &Path, master_password: &str, strength_confirmed: bool) -> CoreResult<Self> {
        // U6 已裁定：< 8 位硬拦截；≥ 8 位仅强度提示（前端确认），核心不强制混合字符类型。
        let _ = strength_confirmed;
        if master_password.chars().count() < MIN_MASTER_PASSWORD_LEN {
            return Err(CoreError::WeakMasterPassword);
        }
        if path.exists() {
            return Err(CoreError::VaultExists);
        }

        let params = KdfParams::default();
        let salt = rng::random_salt16()?.to_vec();
        let vk_raw = rng::random_key()?;

        // MK 派生后立即用于派生 KEK / verify，随后随作用域清零。
        let mk = kdf::derive_master_key(master_password, &salt, &params)?;
        let kek = kdf::derive_kek(&mk[..])?;
        let verify_key = kdf::derive_verify_key(&mk[..])?;
        drop(mk);

        let verify_mac = verify::compute_verify_mac(&verify_key)?;
        let wrapped_vault_key = keys::wrap_vault_key(&kek, &vk_raw)?;

        let header = Vault::new(
            uuid::Uuid::new_v4().to_string(),
            salt,
            wrapped_vault_key,
            verify_mac,
            now_ms(),
        );

        let vault_key: VaultKey = Zeroizing::new(vk_raw);
        let device_id = load_or_create_device_id(path)?;
        let repo = Repository::create(path, header, vault_key, device_id)?;

        Ok(Self {
            state: SessionState::Unlocked(Unlocked { repo, kek }),
        })
    }

    /// 解锁已有保险库（F-01）。失败统一返回 [`CoreError::UnlockFailed`]。
    ///
    /// **A-05 / D-10 统一化**：除 [`CoreError::NoVault`] 外，**任何**失败——密码错误、头部
    /// 结构性损坏（magic / version / header_len / JSON 反序列化 / 读截断）——都返回同一个
    /// [`CoreError::UnlockFailed`]；且结构性损坏路径在返回前会**刻意消耗一次等价 KDF**，
    /// 使耗时与密码错误路径相当，消除时序侧信道。
    ///
    /// [`CoreError::NoVault`] **不折叠**（主理人裁定）：它是本地 UI 状态而非机密
    /// （能访问该路径者本就知道文件是否存在），且前端要靠它区分「首次使用 → 创建向导」
    /// 与「已有库 → 解锁页」，折叠会打乱首次启动逻辑；A-05 针对的是「密码错 vs 数据损坏」，
    /// `NoVault` 不属于这两者。
    pub fn unlock(path: &Path, master_password: &str) -> CoreResult<Self> {
        if !path.exists() {
            return Err(CoreError::NoVault);
        }
        // 仅读取头部（KDF 参数 / 盐）。此处把 `Container::open` 的失败折叠为统一错误码。
        let container = match Container::open(path) {
            Ok(c) => c,
            // NoVault 原样返回（裁定见上）。
            Err(CoreError::NoVault) => return Err(CoreError::NoVault),
            // 结构性损坏：先花掉一次等价 KDF，再统一报 UnlockFailed，
            // 避免「错误码一致但耗时差上千倍」的时序侧信道（A-05 / D-10）。
            Err(_) => {
                burn_unlock_kdf(master_password);
                return Err(CoreError::UnlockFailed);
            }
        };
        let (salt, params, verify_mac, wrapped) = {
            let header = container.header();
            (
                header.kdf_salt.clone(),
                header.kdf.clone(),
                header.verify_mac.clone(),
                header.wrapped_vault_key.clone(),
            )
        };
        drop(container);

        let mk = kdf::derive_master_key(master_password, &salt, &params)?;
        let kek = kdf::derive_kek(&mk[..])?;
        let verify_key = kdf::derive_verify_key(&mk[..])?;
        drop(mk);

        // 统一校验：任何失败（密码错 / 数据损坏）都映射为同一错误码。
        let vault_key = verify::unlock_check(&kek, &verify_key, &verify_mac, &wrapped)?;

        let device_id = load_or_create_device_id(path)?;
        let mut repo = Repository::open(path, vault_key, device_id)?;
        // D-60 §3.14.3：存量独立附件条目 → SecureNote 挂附件（幂等；必须在同步 export
        // 之前、wipe_plaintext 之前执行，避免迁移产物与远端旧语义帧竞态）。
        let _ = repo.migrate_legacy_attachments();
        // 启动清理：到期回收站条目（F-15）。
        let _ = repo.purge_expired_trash();
        // D-60 §3.14.4：孤儿 blob 可达性 GC（幂等扫描）。
        let _ = repo.gc_orphan_blobs();

        Ok(Self {
            state: SessionState::Unlocked(Unlocked { repo, kek }),
        })
    }

    /// 锁定并清零敏感内存（A-04）。
    ///
    /// 先对仓储内的**条目明文**显式覆写擦除（`Zeroizing` 只能覆盖 KEK/VK 等密钥载体，
    /// 覆盖不到 `Repository` 里的普通 `String` 明文），再切换回锁定态触发级联 Drop。
    pub fn lock(&mut self) {
        if let SessionState::Unlocked(unlocked) = &mut self.state {
            unlocked.repo.wipe_plaintext();
        }
        self.state = SessionState::Locked;
    }

    /// 已解锁仓储引用。
    pub fn repo(&self) -> CoreResult<&Repository> {
        match &self.state {
            SessionState::Unlocked(u) => Ok(&u.repo),
            SessionState::Locked => Err(CoreError::NotUnlocked),
        }
    }

    /// 已解锁仓储可变引用。
    pub fn repo_mut(&mut self) -> CoreResult<&mut Repository> {
        match &mut self.state {
            SessionState::Unlocked(u) => Ok(&mut u.repo),
            SessionState::Locked => Err(CoreError::NotUnlocked),
        }
    }

    /// 修改主密码（O-08）：仅重包裹 VK，不重写任何条目密文。
    pub fn change_master_password(&mut self, old_password: &str, new_password: &str) -> CoreResult<()> {
        if new_password.chars().count() < MIN_MASTER_PASSWORD_LEN {
            return Err(CoreError::WeakMasterPassword);
        }
        let unlocked = match &mut self.state {
            SessionState::Unlocked(u) => u,
            SessionState::Locked => return Err(CoreError::NotUnlocked),
        };

        let header = unlocked.repo.header().clone();

        // 校验旧密码：派生后尝试解包，任何失败统一为 UnlockFailed。
        let mk_old = kdf::derive_master_key(old_password, &header.kdf_salt, &header.kdf)?;
        let kek_old = kdf::derive_kek(&mk_old[..])?;
        let verify_old = kdf::derive_verify_key(&mk_old[..])?;
        drop(mk_old);
        let vk_check = verify::unlock_check(&kek_old, &verify_old, &header.verify_mac, &header.wrapped_vault_key)?;

        // 与当前会话 VK 常量时间比对，确保旧密码确为当前主密码。
        if !verify::constant_time_eq(&vk_check[..], &unlocked.repo.vault_key()[..]) {
            return Err(CoreError::UnlockFailed);
        }

        // 用新密码重新派生 KEK，仅重包裹同一个 VK。
        let mk_new = kdf::derive_master_key(new_password, &header.kdf_salt, &header.kdf)?;
        let kek_new = kdf::derive_kek(&mk_new[..])?;
        let verify_new = kdf::derive_verify_key(&mk_new[..])?;
        drop(mk_new);

        let verify_mac = verify::compute_verify_mac(&verify_new)?;
        let wrapped = keys::wrap_vault_key(&kek_new, &unlocked.repo.vault_key()[..])?;
        unlocked.repo.update_key_material(wrapped, verify_mac)?;
        unlocked.kek = kek_new;
        Ok(())
    }

    /// 采用 Host 下发的 VK（U1/U2 裁定）。
    ///
    /// - Joiner 本地库**为空** → 直接用本地 KEK 重新包裹并替换；
    /// - Joiner 本地库**非空** → 返回 [`CoreError::VkAdoptRequiresEmpty`]，由前端弹确认，
    ///   MVP **不做两库合并**。
    pub fn adopt_vault_key(&mut self, host_vk: &[u8; 32]) -> CoreResult<()> {
        let unlocked = match &mut self.state {
            SessionState::Unlocked(u) => u,
            SessionState::Locked => return Err(CoreError::NotUnlocked),
        };
        let count = unlocked.repo.record_count();
        if count != 0 {
            return Err(CoreError::VkAdoptRequiresEmpty { record_count: count });
        }
        let header = unlocked.repo.header().clone();
        let wrapped = keys::wrap_vault_key(&unlocked.kek, host_vk)?;
        unlocked
            .repo
            .update_key_material(wrapped, header.verify_mac.clone())?;
        unlocked.repo.replace_vault_key(Zeroizing::new(*host_vk));
        Ok(())
    }

    /// 当前保险库文件路径（已解锁时）。
    pub fn vault_path(&self) -> CoreResult<PathBuf> {
        Ok(self.repo()?.path())
    }
}

/// 在「头部结构性损坏」的早退路径上**刻意**跑一次与正常解锁等价的 Argon2id。
///
/// 目的（A-05 / D-10）：结构性损坏（magic / version / header_len / JSON 反序列化 / 读截断）
/// 原本会在 KDF **之前**毫秒级返回，而正常失败要跑满 Argon2id(m=64MiB,t=3)（本机数秒级）。
/// 即使错误码已统一，耗时仍差上千倍 → 依旧可区分。故这里先用 [`KdfParams::default()`]
/// （与 [`Session::create`] 写入的默认参数一致）派生一次 MK 并**丢弃结果**。
///
/// 返回值是 [`zeroize::Zeroizing`]，`drop` 时自动清零；再用 [`std::hint::black_box`]
/// 确保该计算不会被编译器优化掉。**这是刻意的等价消耗，不是冗余代码。**
fn burn_unlock_kdf(master_password: &str) {
    // 固定 16 字节盐：仅为消耗等价算力，不参与任何真实密钥派生。
    const BURN_SALT: [u8; 16] = [0u8; 16];
    let params = KdfParams::default();
    if let Ok(mk) = kdf::derive_master_key(master_password, &BURN_SALT, &params) {
        std::hint::black_box(&mk[..]);
        drop(mk); // Zeroizing：即刻清零
    }
}

/// 设备稳定 ID：存放于与保险库同目录的 `<name>.device` 文件。
fn load_or_create_device_id(vault_path: &Path) -> CoreResult<String> {
    let device_path = vault_path.with_extension("device");
    if device_path.exists() {
        let raw = std::fs::read_to_string(&device_path)?;
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    std::fs::write(&device_path, id.as_bytes())?;
    Ok(id)
}
