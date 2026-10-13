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
    pub fn unlock(path: &Path, master_password: &str) -> CoreResult<Self> {
        if !path.exists() {
            return Err(CoreError::NoVault);
        }
        // 仅读取头部（KDF 参数 / 盐），随后丢弃容器句柄。
        let container = Container::open(path)?;
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
        // 启动清理：到期回收站条目（F-15）。
        let _ = repo.purge_expired_trash();

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
