//! 生物识别快速解锁：旁路密封（`docs/08-biometric-unlock.md` §4）。
//!
//! 本模块**零平台依赖**：真正的加解密由 [`VaultSealer`] 的实现承担（Windows DPAPI / Android
//! Keystore，落在 `src-tauri` 与 Kotlin 插件侧）。本模块只负责三件平台无关的事：
//!
//! 1. **封印清单** `biometric.json` 的结构、白名单校验与崩溃安全原子写（§4.2 / §4.7）；
//! 2. **绑定熵** `bind = SHA-256(前缀 ‖ vault_id ‖ verify_mac)` 的算法（§4.3）——
//!    改主密码会让头部 `verify_mac` 变化，于是平台解封必然失败，**无需额外协调代码**；
//! 3. **状态分类** [`SealState`]（未开启 / 可用 / 已失效），把「改主密码即失效」变成可断言行为。
//!
//! 安全约束（不得越界）：
//! - 本模块**不生成、不缓存**主密码 / MK / KEK，只处理 VK 密文；
//! - 清单**不进同步、不进容器**（§12），因此不参与任何跨设备分发；
//! - 任何解析失败一律当「未开启」（[`SealState::Absent`]），不得 panic、不得误判为可用。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::constants::KEY_LEN;
use crate::crypto::keys::VaultKey;
use crate::error::{CoreError, CoreResult};
use crate::model::now_ms;

/// 绑定熵的域分隔前缀（与 KEK/VK 包裹用的 AAD 常量同风格，互不复用）。
pub const SEAL_BIND_PREFIX: &[u8] = b"passwordms/v1/biometric";
/// 封印清单文件名（与保险库容器同目录）。
pub const SEAL_FILE: &str = "biometric.json";
/// 原子写的临时文件名（同目录，保证同卷 rename）。
pub const SEAL_TMP_FILE: &str = "biometric.tmp.json";
/// 清单结构版本。
pub const SEAL_VERSION: u16 = 1;
/// 绑定校验值长度（字节）。只承载「绑定串是否变化」这一个事实，不含机密。
pub const SEAL_BIND_CHECK_LEN: usize = 8;

/// 平台大类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// Windows（DPAPI / TPM2）。
    Windows,
    /// Android（Keystore）。
    Android,
}

/// 保护手段细分。
///
/// **`WindowsTpm2Seal` 在 v1 就预留**：枚举值一旦发布就无法回头加——老版本读到未识别取值
/// 只能当「未开启」，所以宁可现在留空位（`docs/08` §4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Subtype {
    /// Windows DPAPI 用户域 + Windows Hello 在场校验（**软绑定**，§6.3）。
    #[serde(rename = "windows-dpapi-user")]
    WindowsDpapiUser,
    /// Windows TPM2 密封（1.3.x 实现，抗磁盘镜像离线解密）。
    #[serde(rename = "windows-tpm2-seal")]
    WindowsTpm2Seal,
    /// Android Keystore + BiometricPrompt（**硬绑定**，§6.3）。
    #[serde(rename = "android-keystore")]
    AndroidKeystore,
}

impl Subtype {
    /// 所属平台大类。
    pub fn platform(self) -> Platform {
        match self {
            Subtype::WindowsDpapiUser | Subtype::WindowsTpm2Seal => Platform::Windows,
            Subtype::AndroidKeystore => Platform::Android,
        }
    }
}

/// 平台可用性。映射自 `UserConsentVerifier` / `BiometricManager` 的可用性结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// 可用。
    Available,
    /// 本机未配置（未录入生物特征 / 未设置 Hello）。
    NotConfigured,
    /// 无相应硬件。
    NotPresent,
    /// 被策略禁用。
    DisabledByPolicy,
    /// 暂时占用（如 `DeviceBusy`、RDP 会话中断）。
    ///
    /// **不得**因该状态删除封印（§8：否则「什么都没做，快速解锁就没了」）。
    Busy,
    /// 当前平台不支持（例如非 Windows/Android 构建）。
    Unsupported,
}

/// 平台适配层返回的错误（由适配器映射平台异常，core 再翻译成 [`CoreError`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealError {
    /// 用户取消（Windows Hello 弹框被关掉 / Android 返回负键）。
    Canceled,
    /// 平台不可用。
    Unavailable(Availability),
    /// 平台密钥已永久失效（Android `KeyPermanentlyInvalidatedException` 等）。
    Invalidated,
    /// 其它平台错误（含「密文/tag 校验失败」——core 会归类为篡改）。
    Platform(String),
}

impl From<SealError> for CoreError {
    fn from(e: SealError) -> Self {
        match e {
            SealError::Canceled => CoreError::BiometricCanceled,
            SealError::Unavailable(_) => CoreError::BiometricUnavailable,
            SealError::Invalidated => {
                CoreError::BiometricInvalidated(InvalidatedReason::BiometricEnrollmentChanged)
            }
            SealError::Platform(msg) => CoreError::Crypto(format!("biometric: {msg}")),
        }
    }
}

/// 失效原因（§10：让前端能给出可排查的文案，而不是笼统一句「已关闭」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidatedReason {
    /// 主密码已变更（`verify_mac` 变了 ⇒ 绑定熵不符）。
    MasterPasswordChanged,
    /// 生物特征注册集变更（Android 新增/删除指纹 ⇒ 平台密钥永久失效）。
    BiometricEnrollmentChanged,
    /// 密文被篡改 / 损坏。
    Tampered,
}

/// 封印清单（`biometric.json` 的内存形态）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealRecord {
    /// 结构版本（≠ [`SEAL_VERSION`] 一律当未开启）。
    pub version: u16,
    /// 绑定的保险库 ID（与容器头部 `vault_id` 同源）。
    pub vault_id: String,
    /// 平台大类。
    pub platform: Platform,
    /// 保护手段细分。
    pub platform_subtype: Subtype,
    /// 平台加密后的 VK 密文（含平台侧 IV/nonce）。
    #[serde(with = "crate::model::b64serde::vec")]
    pub sealed_vk: Vec<u8>,
    /// 绑定校验值 = `SHA-256(bind)` 前 [`SEAL_BIND_CHECK_LEN`] 字节。
    ///
    /// 它**不是机密**（输入全是公开数据：前缀 + `vault_id` + 头部 `verify_mac`），存在的唯一
    /// 目的是把「绑定串变了（改主密码）」与「密文被篡改」区分开，从而给出不同文案（§10）。
    #[serde(with = "crate::model::b64serde::vec")]
    pub bind_check: Vec<u8>,
    /// 创建时间（毫秒）。
    pub created_at: i64,
}

/// 平台适配层：把 VK 用平台密钥设施加密 / 解密（需要用户在场时由实现自行弹窗）。
///
/// 实现方**不得**持久化明文 VK；`sealed` 的字节布局由实现自定（含 IV/nonce）。
pub trait VaultSealer {
    /// 当前可用性（每次解锁/开启前都查，别缓存）。
    fn availability(&self) -> Availability;

    /// 本实现对应的细分类型。
    fn subtype(&self) -> Subtype;

    /// 用 `bind` 作附加认证数据加密 VK。
    fn seal(&self, vk: &[u8], bind: &[u8]) -> Result<Vec<u8>, SealError>;

    /// 用 `bind` 作附加认证数据解密 VK。
    fn unseal(&self, sealed: &[u8], bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError>;
}

/// 封印状态（[`SealStore::inspect`] 的三态结论）。
#[derive(Debug)]
pub enum SealState {
    /// 未开启 / 文件不存在 / 结构不可识别（一律按「未开启」处理）。
    Absent,
    /// 可用。
    Ready(SealRecord),
    /// 存在但已失效，附原因。
    Stale(InvalidatedReason),
}

/// 绑定熵（§4.3）：`SHA-256(前缀 ‖ 0x00 ‖ vault_id ‖ 0x00 ‖ verify_mac)`。
pub fn bind_of(vault_id: &str, verify_mac: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SEAL_BIND_PREFIX);
    h.update([0u8]);
    h.update(vault_id.as_bytes());
    h.update([0u8]);
    h.update(verify_mac);
    h.finalize().into()
}

/// 绑定校验值（`SHA-256(bind)` 前 8 字节）。
pub fn bind_check_of(bind: &[u8; 32]) -> [u8; SEAL_BIND_CHECK_LEN] {
    let digest = Sha256::digest(bind);
    let mut out = [0u8; SEAL_BIND_CHECK_LEN];
    out.copy_from_slice(&digest[..SEAL_BIND_CHECK_LEN]);
    out
}

/// 封印清单的存取（同目录原子写，见 `docs/08` §4.7）。
#[derive(Debug, Clone)]
pub struct SealStore {
    path: PathBuf,
}

impl SealStore {
    /// 在保险库所在目录构造清单存取器。
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join(SEAL_FILE),
        }
    }

    /// 清单路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 临时文件路径（成功写入后会消失；失败时**刻意保留**供诊断）。
    pub fn tmp_path(&self) -> PathBuf {
        self.path.with_file_name(SEAL_TMP_FILE)
    }

    /// 判定当前状态：未开启 / 可用 / 已失效（含原因）。
    ///
    /// 分类规则（顺序不可调换）：
    /// 1. 文件不存在 → `Absent`；
    /// 2. 解析失败 / 版本未知 / 平台与子类型不一致 → `Absent`（白名单，不 panic）；
    /// 3. `vault_id` 与本库不同 → `Absent`（不是本库的封印，不报错）；
    /// 4. `bind_check` 与当前头部算出的不符 → `Stale(MasterPasswordChanged)`；
    /// 5. 其余 → `Ready`。
    pub fn inspect(&self, vault_id: &str, verify_mac: &[u8]) -> CoreResult<SealState> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SealState::Absent),
            Err(e) => return Err(CoreError::Io(format!("{:?}", e.kind()))),
        };
        let rec = match serde_json::from_slice::<SealRecord>(&bytes) {
            Ok(r) => r,
            // 解析失败 = 结构不可识别（含未知子类型），当未开启。
            Err(_) => return Ok(SealState::Absent),
        };
        if rec.version != SEAL_VERSION {
            return Ok(SealState::Absent);
        }
        if rec.platform != rec.platform_subtype.platform() {
            return Ok(SealState::Absent);
        }
        if rec.bind_check.len() != SEAL_BIND_CHECK_LEN {
            return Ok(SealState::Absent);
        }
        if rec.vault_id != vault_id {
            return Ok(SealState::Absent);
        }
        let expect = bind_check_of(&bind_of(vault_id, verify_mac));
        if rec.bind_check != expect {
            return Ok(SealState::Stale(InvalidatedReason::MasterPasswordChanged));
        }
        Ok(SealState::Ready(rec))
    }

    /// 原子写入清单：tmp → `sync_all` → rename 前校验 → rename（带退避重试）。
    ///
    /// 失败语义与 `Container::rewrite_with_frames` 对齐：**旧文件一字未动，tmp 保留**——
    /// 存在的永远是「完整旧文件 + 待诊断 tmp」，不存在「两个都残缺」的中间态。
    pub fn save(&self, rec: &SealRecord) -> CoreResult<()> {
        let json =
            serde_json::to_vec(rec).map_err(|_| CoreError::Internal("seal encode".into()))?;
        // rename 之前先把能校验的校验掉：写下去的内容必须能原样读回。
        match serde_json::from_slice::<SealRecord>(&json) {
            Ok(back) if &back == rec => {}
            _ => return Err(CoreError::Internal("seal verify before rename".into())),
        }

        let tmp = self.tmp_path();
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&json)?;
            // 必须是 sync_all：只 rename 不刷盘 = 换上一个内容还在页缓存里的空壳。
            f.sync_all()?;
        }

        let mut last: Option<std::io::Error> = None;
        for attempt in 0..3u32 {
            match fs::rename(&tmp, &self.path) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // 杀软 / 索引器瞬时占用 → 退避重试；tmp 保留，不删。
                    last = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(20 * (attempt as u64 + 1)));
                }
            }
        }
        Err(match last {
            Some(e) => CoreError::Io(format!("{:?}", e.kind())),
            None => CoreError::Internal("seal rename".into()),
        })
    }

    /// 删除清单（幂等）。同时清掉残留 tmp，避免「关闭后仍有旧密钥密文」。
    pub fn remove(&self) -> CoreResult<()> {
        for p in [self.path.clone(), self.tmp_path()] {
            match fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(CoreError::Io(format!("{:?}", e.kind()))),
            }
        }
        Ok(())
    }
}

/// 开启快速解锁：平台加密 VK 并落盘。
///
/// 前置（由命令层保证，§4.5）：已用**主密码**验证过身份；`vk` 来自已解锁会话。
pub fn enable(
    store: &SealStore,
    sealer: &dyn VaultSealer,
    vault_id: &str,
    verify_mac: &[u8],
    vk: &[u8],
) -> CoreResult<SealRecord> {
    if vk.len() != KEY_LEN {
        return Err(CoreError::Validation("biometric vk len".into()));
    }
    if sealer.availability() != Availability::Available {
        return Err(CoreError::BiometricUnavailable);
    }
    let bind = bind_of(vault_id, verify_mac);
    let sealed = sealer.seal(vk, &bind)?;
    let rec = SealRecord {
        version: SEAL_VERSION,
        vault_id: vault_id.to_string(),
        platform: sealer.subtype().platform(),
        platform_subtype: sealer.subtype(),
        sealed_vk: sealed,
        bind_check: bind_check_of(&bind).to_vec(),
        created_at: now_ms(),
    };
    store.save(&rec)?;
    Ok(rec)
}

/// 用封印解锁：返回 VK。
///
/// 失败分类（**不得**折叠成 `UnlockFailed`，§10）：
/// - 未开启 → [`CoreError::BiometricUnavailable`]
/// - 改过主密码 → [`CoreError::BiometricInvalidated`]`(MasterPasswordChanged)`
/// - 用户取消 → [`CoreError::BiometricCanceled`]
/// - 平台密钥失效 → `(BiometricEnrollmentChanged)`
/// - 其它解封失败（tag 不符等）→ `(Tampered)`
pub fn unlock(
    store: &SealStore,
    sealer: &dyn VaultSealer,
    vault_id: &str,
    verify_mac: &[u8],
) -> CoreResult<VaultKey> {
    let rec = match store.inspect(vault_id, verify_mac)? {
        SealState::Absent => return Err(CoreError::BiometricUnavailable),
        SealState::Stale(reason) => return Err(CoreError::BiometricInvalidated(reason)),
        SealState::Ready(rec) => rec,
    };
    if sealer.availability() != Availability::Available {
        return Err(CoreError::BiometricUnavailable);
    }
    let bind = bind_of(vault_id, verify_mac);
    let raw = match sealer.unseal(&rec.sealed_vk, &bind) {
        Ok(v) => v,
        Err(SealError::Canceled) => return Err(CoreError::BiometricCanceled),
        Err(SealError::Unavailable(_)) => return Err(CoreError::BiometricUnavailable),
        Err(e @ SealError::Invalidated) => return Err(e.into()),
        // 绑定串对得上却解不开 ⇒ 密文被篡改/损坏。
        Err(SealError::Platform(_)) => {
            return Err(CoreError::BiometricInvalidated(InvalidatedReason::Tampered))
        }
    };
    if raw.len() != KEY_LEN {
        return Err(CoreError::BiometricInvalidated(InvalidatedReason::Tampered));
    }
    let mut vk: VaultKey = Zeroizing::new([0u8; KEY_LEN]);
    vk.copy_from_slice(&raw);
    Ok(vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_changes_with_verify_mac_and_vault_id() {
        let a = bind_of("vault-a", &[1u8; 32]);
        let b = bind_of("vault-a", &[2u8; 32]);
        let c = bind_of("vault-b", &[1u8; 32]);
        assert_ne!(a, b, "改 verify_mac 必须改变绑定串");
        assert_ne!(a, c, "换 vault_id 必须改变绑定串");
        assert_eq!(a, bind_of("vault-a", &[1u8; 32]));
    }

    #[test]
    fn bind_check_is_short_and_stable() {
        let bind = bind_of("vault-a", &[7u8; 32]);
        let first = bind_check_of(&bind);
        assert_eq!(first.len(), SEAL_BIND_CHECK_LEN);
        assert_eq!(first, bind_check_of(&bind));
    }
}
