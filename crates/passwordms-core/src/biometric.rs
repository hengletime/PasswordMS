//! 生物识别快速解锁：旁路密封（`docs/08-biometric-unlock.md` §4）。
//!
//! 本模块**零平台依赖**：真正的加解密由 [`VaultSealer`] 的实现承担（Windows DPAPI / Android
//! Keystore，落在 `src-tauri` 与 Kotlin 插件侧）。本模块只负责三件平台无关的事：
//!
//! 1. **封印清单** `biometric.json` 的结构、白名单校验与崩溃安全原子写（§4.2 / §4.7）；
//! 2. **绑定熵** `bind = SHA-256(前缀 ‖ vault_id ‖ verify_mac ‖ wrapped_vault_key)` 的算法（§4.3）——
//!    `verify_mac` 随主密码变、`wrapped_vault_key` 随 VK **或** KEK 变，于是**改主密码**与
//!    **换 VK（配对加入）**都会让平台解封失败，无需额外协调代码；
//! 3. **状态分类** [`SealState`]（未开启 / 可用 / 已失效 / 已损坏），把失效原因变成可断言行为。
//!
//! 安全约束（不得越界）：
//! - 本模块**不生成、不缓存**主密码 / MK / KEK，只处理 VK 密文；
//! - 清单**不进同步、不进容器**（§12），因此不参与任何跨设备分发；
//! - 任何解析失败一律当「不可用」（[`SealState::Absent`] 或 [`SealState::Corrupt`]），
//!   不得 panic、不得误判为可用。
//!
//! ## 为什么 `bind` 里必须含 `wrapped_vault_key`（D-113 / 审计 B-1）
//!
//! 早期版本只用 `vault_id + verify_mac` 作绑定串。但 `Session::adopt_vault_key()`（配对加入）
//! 会**换掉 VK 而保留原 `verify_mac`**，于是 bind 不变、封印仍被判「可用」，而里面封的是
//! **换 VK 之前那个 VK**：空库没有任何活帧可解密，错 VK 也能「成功」打开，用户在其上新建的
//! 条目全部用错密钥加密 ⇒ **整库永久打不开**（已实测复现）。
//!
//! 把 `header.wrapped_vault_key`（公开字段：nonce + 密文）纳入 bind 即可根治——它是「当前 VK
//! 在 KEK 下的包裹」，VK 变或 KEK 变它必然变，且**无需任何 secret**（unlock 时也能从头部算出）。

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::crypto::keys::VaultKey;
use crate::error::{CoreError, CoreResult};
use crate::model::{now_ms, Vault, WrappedKey};

/// 绑定熵的域分隔前缀（与 KEK/VK 包裹用的 AAD 常量同风格，互不复用）。
pub const SEAL_BIND_PREFIX: &[u8] = b"passwordms/v1/biometric";
/// `mac_check` 的域分隔前缀（只对 `verify_mac`，用于区分「改主密码」）。
const MAC_CHECK_PREFIX: &[u8] = b"passwordms/v1/biometric-mac";
/// `wkv_check` 的域分隔前缀（对 `wrapped_vault_key`，用于区分「VK 已被替换」）。
const WKV_CHECK_PREFIX: &[u8] = b"passwordms/v1/biometric-wkv";
/// 封印清单文件名（与保险库容器同目录）。
pub const SEAL_FILE: &str = "biometric.json";
/// 原子写的临时文件名（同目录，保证同卷 rename）。
pub const SEAL_TMP_FILE: &str = "biometric.tmp.json";
/// 清单结构版本。
///
/// **D-113 起为 2**：`bind_check`（单个）改为 `mac_check` + `wkv_check`（两个）是**破坏性**形状变更，
/// 留 1 就是自欺——旧文件必须能被识别为「版本不认识」而不是「碰巧能解析个半拉」。
pub const SEAL_VERSION: u16 = 2;
/// 校验值长度（字节）。只承载「输入是否变化」这一个事实，不含机密。
pub const SEAL_CHECK_LEN: usize = 8;
/// 清单文件大小上限（字节）。防「能写数据目录者塞巨型文件」造成每次进解锁页全量读入（审计 M-4）。
pub const SEAL_MAX_BYTES: u64 = 4096;
/// `sealed_vk` 密文长度下限（字节）。
///
/// **必须 > 明文 VK 长度（32）**：若适配器有 bug 把明文 VK 原样返回，32 就正好落在
/// 白名单里 ⇒ 明文 VK 被当密文写进磁盘（D-113 复审 L-5）。48 = 32 明文 + 16 字节 GCM tag，
/// 是「任何真实平台输出都不可能小于」的下界（DPAPI 输出还带 ~200 字节头部）。
pub const SEAL_VK_MIN_LEN: usize = 48;
/// `sealed_vk` 上限。
pub const SEAL_VK_MAX_LEN: usize = 1024;

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
    /// 保险库密钥已更换（配对加入 `adopt_vault_key` 后 `wrapped_vault_key` 变 ⇒ 绑定熵不符）。
    ///
    /// **D-113 新增**：这是审计 B-1 的根因场景——旧封印封的是换 VK 之前那个 VK，
    /// 若沿用旧绑定串就会被判「可用」，进而把库锁死。
    VaultKeyChanged,
    /// 生物特征注册集变更（Android 新增/删除指纹 ⇒ 平台密钥永久失效）。
    BiometricEnrollmentChanged,
    /// 密文被篡改 / 损坏。
    Tampered,
}

/// 封印清单（`biometric.json` 的内存形态）。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// `wrapped_vault_key` 校验值 = `SHA-256(域前缀 ‖ vault_id ‖ wkv)[..8]`。
    ///
    /// 它**不是机密**（输入全是公开头部字段），存在的唯一目的是把「VK 已被替换（配对加入）」
    /// 与「主密码已变更」区分开（D-113）。
    #[serde(with = "crate::model::b64serde::vec")]
    pub wkv_check: Vec<u8>,
    /// `verify_mac` 校验值 = `SHA-256(域前缀 ‖ vault_id ‖ verify_mac)[..8]`。
    ///
    /// 同样不是机密；用于把「主密码已变更」单列出来（它是**必然**导致失效的更强原因）。
    #[serde(with = "crate::model::b64serde::vec")]
    pub mac_check: Vec<u8>,
    /// 创建时间（毫秒）。
    pub created_at: i64,
}

/// 手写 `Debug`：**不打 `sealed_vk` 密文**（L-4）——密文进日志/崩溃报告没有收益。
impl std::fmt::Debug for SealRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealRecord")
            .field("version", &self.version)
            .field("vault_id", &self.vault_id)
            .field("platform_subtype", &self.platform_subtype)
            .field("sealed_vk_len", &self.sealed_vk.len())
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// 平台适配层：把 VK 用平台密钥设施加密 / 解密（需要用户在场时由实现自行弹窗）。
///
/// 实现方**不得**持久化明文 VK；`sealed` 的字节布局由实现自定（含 IV/nonce）。
/// 实现方**必须**真的把 `bind` 用作 DPAPI `pOptionalEntropy` / AEAD `AAD`——
/// 忽略 `bind` 会让整个失效模型归零，而 core 的单测（用 Mock）仍会全绿（§17.5 约束 2）。
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

/// 封印状态（[`SealStore::inspect`] 的结论）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealState {
    /// 未开启 / 文件不存在 / 是别的库的封印 / 版本未知（一律按「未开启」处理）。
    Absent,
    /// 存在但**结构已损坏**（JSON 不可解析、内部字段自相矛盾、长度越界）。
    ///
    /// 与 [`SealState::Absent`] 分开是为了让前端能说「快速解锁配置损坏，请重新开启」，
    /// 而不是显示成「从未开启」让用户以为设置丢了（审计 M-3）。
    Corrupt,
    /// 可用。
    Ready(SealRecord),
    /// 存在但已失效，附原因。
    Stale(InvalidatedReason),
}

/// 保险库头部的「当前封印身份」——`bind` 的三项输入。
///
/// 用结构体而非三个散参数，是为了让 [`SealStore::inspect`] / [`unlock`] 的调用方
/// **不可能**把 A 库的头部和 B 库的容器拼在一起（审计 L-5）。
#[derive(Debug, Clone, Copy)]
pub struct SealTarget<'a> {
    /// 头部 `vault_id`。
    pub vault_id: &'a str,
    /// 头部 `verify_mac`。
    pub verify_mac: &'a [u8],
    /// 头部 `wrapped_vault_key`。
    pub wrapped_vault_key: &'a WrappedKey,
}

impl<'a> SealTarget<'a> {
    /// 从容器头部取封印身份。
    pub fn from_header(header: &'a Vault) -> Self {
        Self {
            vault_id: &header.vault_id,
            verify_mac: &header.verify_mac,
            wrapped_vault_key: &header.wrapped_vault_key,
        }
    }
}

/// 绑定熵（§4.3）：`SHA-256(前缀 ‖ 0 ‖ vault_id ‖ 0 ‖ verify_mac ‖ 0 ‖ len ‖ nonce ‖ len ‖ ct)`。
///
/// `nonce` / `ct` 是二进制，可能含 `0x00`，故用长度前缀消歧（不可依赖分隔符）。
pub fn bind_of(t: &SealTarget<'_>) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(SEAL_BIND_PREFIX);
    h.update([0u8]);
    // 四个输入**全部**长度前缀：没有任何一段靠分隔符界定（复审 L-3）。
    // 只用 `0x00` 分隔时，`vault_id="a\0b"` 与 `vault_id="a" + verify_mac="b\0"+M` 可构造同一 preimage。
    update_len_prefixed(&mut h, t.vault_id.as_bytes());
    update_len_prefixed(&mut h, t.verify_mac);
    update_len_prefixed(&mut h, &t.wrapped_vault_key.nonce);
    update_len_prefixed(&mut h, &t.wrapped_vault_key.ct);
    h.finalize().into()
}

/// `mac_check`：只覆盖 `verify_mac`，用于把「改主密码」这一**必然**失效原因单列。
pub fn mac_check_of(t: &SealTarget<'_>) -> [u8; SEAL_CHECK_LEN] {
    let mut h = Sha256::new();
    h.update(MAC_CHECK_PREFIX);
    h.update([0u8]);
    update_len_prefixed(&mut h, t.vault_id.as_bytes());
    update_len_prefixed(&mut h, t.verify_mac);
    truncate(&h.finalize())
}

/// `wkv_check`：只覆盖 `wrapped_vault_key`，用于识别「VK 已被替换（配对加入）」。
pub fn wkv_check_of(t: &SealTarget<'_>) -> [u8; SEAL_CHECK_LEN] {
    let mut h = Sha256::new();
    h.update(WKV_CHECK_PREFIX);
    h.update([0u8]);
    update_len_prefixed(&mut h, t.vault_id.as_bytes());
    update_len_prefixed(&mut h, &t.wrapped_vault_key.nonce);
    update_len_prefixed(&mut h, &t.wrapped_vault_key.ct);
    truncate(&h.finalize())
}

fn update_len_prefixed(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u32).to_be_bytes());
    h.update(bytes);
}

fn truncate(digest: &[u8]) -> [u8; SEAL_CHECK_LEN] {
    let mut out = [0u8; SEAL_CHECK_LEN];
    out.copy_from_slice(&digest[..SEAL_CHECK_LEN]);
    out
}

/// 封印清单的存取（同目录原子写，见 `docs/08` §4.7）。
#[derive(Debug, Clone)]
pub struct SealStore {
    path: PathBuf,
}

impl SealStore {
    /// 由**保险库文件路径**构造清单存取器（清单落在同目录）。
    ///
    /// 用文件路径而非目录，是为了与 [`crate::store::Repository::path`] 同口径——
    /// 避免将来支持多库/可配置路径时「A 库的封印被 B 库目录读到」（审计 L-1）。
    pub fn new(vault_path: &Path) -> Self {
        Self {
            path: vault_path.with_file_name(SEAL_FILE),
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

    /// 判定当前状态：未开启 / 已损坏 / 可用 / 已失效（含原因）。
    ///
    /// 分类规则（顺序不可调换）：
    /// 1. 文件不存在 → `Absent`；
    /// 2. 文件超过 [`SEAL_MAX_BYTES`] / JSON 不可解析 / 内部字段自相矛盾 → `Corrupt`；
    /// 3. `version` 未知 → `Absent`（更新的格式，本版本不认识，当未开启）；
    /// 4. `vault_id` 与本库不同 → `Absent`（不是本库的封印，不报错）；
    /// 5. `mac_check` 不符 → `Stale(MasterPasswordChanged)`（改密必然连带 `wkv` 变，优先报它）；
    /// 6. `wkv_check` 不符 → `Stale(VaultKeyChanged)`（配对加入换过 VK）；
    /// 7. 其余 → `Ready`。
    pub fn inspect(&self, target: &SealTarget<'_>) -> CoreResult<SealState> {
        // 用 `take` 直接限量读：先 `metadata` 再 `fs::read` 中间文件长大了就绕过上限
        // （复审 L-7），而全量 `fs::read` 会把任意大文件吃进内存。
        let mut bytes = Vec::new();
        {
            let f = match fs::File::open(&self.path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(SealState::Absent),
                Err(e) => return Err(CoreError::Io(format!("{:?}", e.kind()))),
            };
            if f.take(SEAL_MAX_BYTES + 1).read_to_end(&mut bytes)? > SEAL_MAX_BYTES as usize {
                return Ok(SealState::Corrupt);
            }
        }
        // 先探一层 `Value`：为了把三件不同的事分开——「本版本不认识的**新**格式」（→ 未开启）、
        // 「字段缺失/自相矛盾」（→ 损坏）、「能解析」（→ 继续分类）。
        let value: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => return Ok(SealState::Corrupt),
        };
        // 规则 3 必须在反序列化**之前**：否则新版本的记录一旦字段形状变了，
        // 会先解析失败被归成「损坏」，前向兼容契约永远走不到（复审 M-1）。
        match value.get("version") {
            Some(v) => match v.as_u64() {
                // 未知版本 = 更新版本写的，当未开启，不惊动用户。
                Some(n) if n != u64::from(SEAL_VERSION) => return Ok(SealState::Absent),
                Some(_) => {}
                // 字段在但不是整数 = 被改坏。
                None => return Ok(SealState::Corrupt),
            },
            None => return Ok(SealState::Corrupt),
        }
        // 子类型用 serde 解析而不是硬编码字符串白名单——白名单是 `Subtype` 的第二处真相，
        // 将来加变体必漏，且漏了会**静默降级成 Absent**（复审 L-4）。
        let subtype = match value.get("platform_subtype") {
            Some(v) if v.is_string() => match serde_json::from_value::<Subtype>(v.clone()) {
                Ok(s) => s,
                // 是字符串但不认识 ⇒ 更新版本的新保护手段，当未开启。
                Err(_) => return Ok(SealState::Absent),
            },
            // 缺失或非字符串 ⇒ 结构坏了，不能伪装成「从未开启」。
            _ => return Ok(SealState::Corrupt),
        };
        let rec = match serde_json::from_value::<SealRecord>(value) {
            Ok(r) => r,
            Err(_) => return Ok(SealState::Corrupt),
        };
        if rec.platform != subtype.platform() {
            // 平台与子类型自相矛盾 = 文件被改坏，不是「未开启」。
            return Ok(SealState::Corrupt);
        }
        if rec.mac_check.len() != SEAL_CHECK_LEN
            || rec.wkv_check.len() != SEAL_CHECK_LEN
            || !(SEAL_VK_MIN_LEN..=SEAL_VK_MAX_LEN).contains(&rec.sealed_vk.len())
        {
            return Ok(SealState::Corrupt);
        }
        if rec.vault_id != target.vault_id {
            return Ok(SealState::Absent);
        }
        if rec.mac_check != mac_check_of(target) {
            return Ok(SealState::Stale(InvalidatedReason::MasterPasswordChanged));
        }
        if rec.wkv_check != wkv_check_of(target) {
            return Ok(SealState::Stale(InvalidatedReason::VaultKeyChanged));
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
        if json.len() as u64 > SEAL_MAX_BYTES {
            return Err(CoreError::Internal("seal too large".into()));
        }
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
    ///
    /// **这是撤销快速解锁的唯一动作**：改主密码只会让封印「应用层不可用」，
    /// 旧密文里的 VK 在法律意义上仍然有效（§4.3 的已知限制，审计 H-2）。
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
/// 前置（由命令层保证，§4.5）：已用**主密码**验证过身份；`vk` 必须取自当前会话
/// [`crate::session::Session::repo`] 的 `vault_key()`（类型即 `&VaultKey`，杜绝传错 32 字节）。
pub fn enable(
    store: &SealStore,
    sealer: &dyn VaultSealer,
    target: &SealTarget<'_>,
    vk: &VaultKey,
) -> CoreResult<SealRecord> {
    if sealer.availability() != Availability::Available {
        return Err(CoreError::BiometricUnavailable);
    }
    let bind = bind_of(target);
    let sealed = sealer.seal(&vk[..], &bind)?;
    if !(SEAL_VK_MIN_LEN..=SEAL_VK_MAX_LEN).contains(&sealed.len()) {
        // 适配器返回了不可能的密文长度：不落盘（否则下次必判 Corrupt）。
        return Err(CoreError::Crypto("biometric sealed length".into()));
    }
    let rec = SealRecord {
        version: SEAL_VERSION,
        vault_id: target.vault_id.to_string(),
        platform: sealer.subtype().platform(),
        platform_subtype: sealer.subtype(),
        sealed_vk: sealed,
        wkv_check: wkv_check_of(target).to_vec(),
        mac_check: mac_check_of(target).to_vec(),
        created_at: now_ms(),
    };
    store.save(&rec)?;
    Ok(rec)
}

/// 用封印解锁：返回 VK。
///
/// 失败分类（**不得**折叠成 `UnlockFailed`，§10）：
/// - 未开启 / 平台子类型不符 → [`CoreError::BiometricUnavailable`]
/// - 改过主密码 → `BiometricInvalidated(MasterPasswordChanged)`
/// - 配对换过 VK → `BiometricInvalidated(VaultKeyChanged)`
/// - 用户取消 → [`CoreError::BiometricCanceled`]
/// - 平台密钥失效 → `(BiometricEnrollmentChanged)`
/// - 结构损坏 / 其它解封失败（tag 不符等）→ `(Tampered)`
pub fn unlock(
    store: &SealStore,
    sealer: &dyn VaultSealer,
    target: &SealTarget<'_>,
) -> CoreResult<VaultKey> {
    // 可用性优先于文件解析：平台不可用是更基础的原因，别让「文件损坏」抢在前面（§17.5 约束 9）。
    if sealer.availability() != Availability::Available {
        return Err(CoreError::BiometricUnavailable);
    }
    let rec = match store.inspect(target)? {
        SealState::Absent => return Err(CoreError::BiometricUnavailable),
        SealState::Corrupt => {
            return Err(CoreError::BiometricInvalidated(InvalidatedReason::Tampered))
        }
        SealState::Stale(reason) => return Err(CoreError::BiometricInvalidated(reason)),
        SealState::Ready(rec) => rec,
    };
    // 跨平台拷来的封印（如 Android 目录拷到 Windows）：是「本机用不了」而不是「已损坏」，
    // 因此**不得**让上层据此删除封印（审计 M-1）。
    if rec.platform_subtype != sealer.subtype() {
        return Err(CoreError::BiometricUnavailable);
    }
    let bind = bind_of(target);
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
    if raw.len() != crate::constants::KEY_LEN {
        return Err(CoreError::BiometricInvalidated(InvalidatedReason::Tampered));
    }
    let mut vk: VaultKey = Zeroizing::new([0u8; crate::constants::KEY_LEN]);
    vk.copy_from_slice(&raw);
    Ok(vk)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target<'a>(vault_id: &'a str, mac: &'a [u8], wkv: &'a WrappedKey) -> SealTarget<'a> {
        SealTarget {
            vault_id,
            verify_mac: mac,
            wrapped_vault_key: wkv,
        }
    }

    #[test]
    fn bind_changes_with_every_input() {
        let mac1 = [1u8; 32];
        let mac2 = [2u8; 32];
        let wkv_a = WrappedKey {
            nonce: vec![9u8; 12],
            ct: vec![7u8; 48],
        };
        // 只改 ct：模拟「VK 被替换 / KEK 重包裹」。
        let wkv_b = WrappedKey {
            nonce: vec![9u8; 12],
            ct: vec![8u8; 48],
        };

        let base = bind_of(&target("vault-a", &mac1, &wkv_a));
        assert_ne!(
            base,
            bind_of(&target("vault-a", &mac2, &wkv_a)),
            "改 verify_mac 必须改变 bind"
        );
        assert_ne!(
            base,
            bind_of(&target("vault-a", &mac1, &wkv_b)),
            "换 VK（wrapped_vault_key 变）必须改变 bind —— D-113 的核心"
        );
        assert_ne!(
            base,
            bind_of(&target("vault-b", &mac1, &wkv_a)),
            "换 vault_id 必须改变 bind"
        );
        assert_eq!(base, bind_of(&target("vault-a", &mac1, &wkv_a)));
    }

    #[test]
    fn checks_are_short_and_domain_separated() {
        let mac = [7u8; 32];
        let wkv = WrappedKey {
            nonce: vec![1u8; 12],
            ct: vec![2u8; 48],
        };
        let t = target("vault-a", &mac, &wkv);
        assert_eq!(mac_check_of(&t).len(), SEAL_CHECK_LEN);
        assert_eq!(wkv_check_of(&t).len(), SEAL_CHECK_LEN);
        // 两个校验必须域分离，否则无法区分「改密」与「换 VK」。
        assert_ne!(mac_check_of(&t), wkv_check_of(&t));
        // 各自只覆盖自己的输入：改 ct 只应影响 wkv_check。
        let wkv2 = WrappedKey {
            nonce: vec![1u8; 12],
            ct: vec![3u8; 48],
        };
        let t2 = target("vault-a", &mac, &wkv2);
        assert_eq!(
            mac_check_of(&t),
            mac_check_of(&t2),
            "ct 变不应影响 mac_check"
        );
        assert_ne!(
            wkv_check_of(&t),
            wkv_check_of(&t2),
            "ct 变必须影响 wkv_check"
        );
    }
}
