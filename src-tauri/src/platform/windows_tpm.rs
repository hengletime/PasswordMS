//! Windows TPM2 密封器（`windows-tpm2-seal`，`docs/08-biometric-unlock.md` §33）。
//!
//! ## 它比 DPAPI 强在哪（只强一件事，别夸大）
//!
//! DPAPI 用户域主密钥在磁盘上，被**Windows 账户口令派生的密钥**保护 ⇒ 拿到账户口令
//! （或 SAM + 口令破解）的人可以**离线**解开封印文件。TPM 的私钥**不可导出**
//! （本机实测 `NCryptExportKey` 返回 `0x8009000A`），解密只能在装有这颗 TPM 的机器上、
//! 由 TPM 内部完成 ⇒ **磁盘镜像 / 备份 / 换机**这三条离线路径被堵死（威胁 T1b）。
//!
//! **没有变强的地方**：本机、本用户、能执行代码的攻击者依然能调 `NCryptDecrypt` 解出 VK，
//! 与 DPAPI 完全同级。这一层由 Windows Hello 在场闸门负责，不由密封器负责。
//!
//! ## 三个被实测钉住的约束（`target/probe-tpm`，2026-10-29）
//!
//! 1. **必须显式放开密钥用途**：默认密钥用途只允许签名，`NCryptEncrypt` 直接报
//!    `0x80090027`（`NTE_INVALID_PARAMETER`）——看起来像「TPM 不支持加密」，其实是用途位。
//! 2. **只有 PKCS#1 v1.5 能用**：OAEP 带/不带 `BCRYPT_PKCS1_PADDING_INFO` 四种组合全失败。
//!    这是**平台唯一支持**的选项，不是我们的取舍（下方 §选择 PKCS#1 的代价）。
//! 3. **私钥不可导出**：这条是 T1b 的全部依据，`tpm_private_key_is_not_exportable` 守着它。
//!
//! ## 选择 PKCS#1 v1.5 的代价（如实登记）
//!
//! PKCS#1 v1.5 存在 Bleichenbacher 类「填充 oracle」风险，前提是攻击者能**反复提交密文
//! 并观察到「填充是否正确」这一比特**。本场景不满足：
//! - 密文只存在于本机 `biometric.json`，没有网络端点、没有跨进程解密服务；
//! - `unseal` 把「填充错」「长度错」「bind 尾巴不符」**一律**映射成同一个
//!   [`SealError::Platform`]，不向调用方提供任何可区分的比特；
//! - 能直接调 `NCryptDecrypt` 的攻击者本来就能拿到明文，不需要 oracle。
//!
//! ## 为什么用**一把固定名字**的持久密钥，而不是按 `bind` 派生
//!
//! Android 侧按 `bind` 派生 Keystore 别名（`key_id(bind)`），遗留的孤儿条目只是占磁盘。
//! TPM 不同：持久密钥占的是 **TPM NVRAM slot，数量有限**（典型只有几十个），而 `bind`
//! 每次改主密码都会变 ⇒ 按 `bind` 派生意味着改 20 次密码就把 slot 用光，之后**永久无法开启**。
//!
//! 固定一把钥匙 + 明文尾部附 `SHA-256(bind)[..16]` 达到同样的效果：`bind` 不符必然解不开
//! （尾巴校验），而 slot 零增长。安全性上两者等价——尾巴只是「误用防护」，真正的屏障是
//! 「私钥在 TPM 里出不来」。

use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use passwordms_core::biometric::{Availability, SealError, Subtype, VaultSealer};

use windows::core::HSTRING;
use windows::Win32::Security::Cryptography::{
    NCryptCreatePersistedKey, NCryptDecrypt, NCryptDeleteKey, NCryptEncrypt, NCryptExportKey,
    NCryptFinalizeKey, NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider,
    NCryptSetProperty, CERT_KEY_SPEC, NCRYPT_ALLOW_ALL_USAGES, NCRYPT_FLAGS, NCRYPT_HANDLE,
    NCRYPT_KEY_HANDLE, NCRYPT_KEY_USAGE_PROPERTY, NCRYPT_PAD_PKCS1_FLAG, NCRYPT_PROV_HANDLE,
    NCRYPT_SILENT_FLAG,
};

use super::{PresenceGate, PresenceReason};

/// TPM 绑定的 CNG 密钥存储 provider（Windows 自带，非自研密码学）。
pub const PROVIDER: &str = "Microsoft Platform Crypto Provider";

/// 产品使用的持久密钥名。
///
/// **改它等于让所有已开启的 TPM 封印失效**（旧钥匙还在 TPM 里、但没人找得到了）。
/// 需要换名时先想清楚迁移，别顺手改。`windows_sealer` 里另有一份同名常量，
/// 一致性由 `tpm_key_name_matches_tpm_module` 守着。
pub(crate) const DEFAULT_KEY_NAME: &str = "PasswordMS-VK-Seal";

/// 密文魔数：标识「这是 TPM 密封器的输出」，用于在 [`super::windows_sealer`] 里
/// 与 DPAPI 密文分派（DPAPI 头是 `01 00 00 00`，不会撞）。
pub const MAGIC: [u8; 4] = *b"PMST";

/// 明文尾部的 bind 摘要长度。
const BIND_TAG_LEN: usize = 16;
/// VK 长度（`passwordms_core::constants::KEY_LEN`，此处硬编码只为让布局一眼可读）。
const VK_LEN: usize = 32;
/// 明文总长 = VK + bind 摘要。
const PLAIN_LEN: usize = VK_LEN + BIND_TAG_LEN;
/// RSA 模数位数（`NCryptCreatePersistedKey` 的默认值，探针实测确认为 2048）。
const RSA_KEY_BITS: u32 = 2048;
/// 对应的模数字节数——TPM 密文的长度必须正好是这个值。
const RSA_BYTES: usize = (RSA_KEY_BITS / 8) as usize;

/// bind 摘要的域分隔前缀（与 `biometric.rs` 里几个前缀同风格、互不复用）。
const BIND_TAG_PREFIX: &[u8] = b"passwordms/v1/biometric-tpm";

/// `PG` + `bind` 的摘要前 16 字节。
fn bind_tag(bind: &[u8]) -> [u8; BIND_TAG_LEN] {
    let mut h = Sha256::new();
    h.update(BIND_TAG_PREFIX);
    h.update([0u8]);
    h.update((bind.len() as u32).to_be_bytes());
    h.update(bind);
    let digest = h.finalize();
    let mut out = [0u8; BIND_TAG_LEN];
    out.copy_from_slice(&digest[..BIND_TAG_LEN]);
    out
}

/// 判断一段密文是不是本密封器的产出。
pub fn is_tpm_blob(sealed: &[u8]) -> bool {
    sealed.len() > MAGIC.len() && sealed[..MAGIC.len()] == MAGIC
}

// ── CNG 句柄的 RAII 包装 ──────────────────────────────────────────────
//
// 手写 `Drop` 而不是靠「记得每个错误分支都 FreeObject」：本模块有十余个提前返回点，
// 漏一个就是进程生命周期内的句柄泄漏，而 TPM 句柄是稀缺资源。

struct Prov(NCRYPT_PROV_HANDLE);

impl Drop for Prov {
    fn drop(&mut self) {
        unsafe {
            let _ = NCryptFreeObject(NCRYPT_HANDLE(self.0 .0));
        }
    }
}

struct Key(NCRYPT_KEY_HANDLE);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = NCryptFreeObject(NCRYPT_HANDLE(self.0 .0));
        }
    }
}

impl Key {
    /// 删除持久密钥。`NCryptDeleteKey` **会自己释放句柄**，所以先 `forget` 再调，
    /// 否则 `Drop` 会二次释放同一个句柄。
    fn delete(self) -> Result<(), String> {
        let handle = self.0;
        std::mem::forget(self);
        unsafe { NCryptDeleteKey(handle, 0) }.map_err(|e| format!("delete key {}", hr(&e)))
    }
}

fn hr(e: &windows::core::Error) -> String {
    format!("0x{:08X}", e.code().0)
}

/// 进程内 TPM 操作互斥。
///
/// TPM 是**全局单命令队列**：同一时刻只能有一个命令在跑。并发的 CNG 调用会随机拿到
/// `NTE_DEVICE_NOT_READY`（`0x80090030`）或 `0x80280909`——本机并行跑测试时实测复现，
/// 单线程跑必绿。命令层虽有 `try_begin_biometric()`，但它只挡同一进程内的生物识别命令，
/// 挡不住「另一个 PasswordMS 实例」和「系统里别的 TPM 使用者（Hello / BitLocker）」。
///
/// 锁只包住 CNG 调用本身，**不包**闸门弹框——用户盯着对话框的 120 秒不能占着锁。
static TPM_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock_tpm() -> std::sync::MutexGuard<'static, ()> {
    // 中毒（持锁线程 panic）不应让 TPM 永久不可用：锁保护的只是「别并发」，没有
    // 跨调用的不变量要维持，恢复内部值继续用。
    TPM_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// `NTE_DEVICE_NOT_READY`：TPM 忙。
const NTE_DEVICE_NOT_READY: u32 = 0x8009_0030;
/// 本机实测的 finalize 并发冲突码（串行执行时不出现）。
const TPM_CONFLICT_OBSERVED: u32 = 0x8028_0909;

fn is_transient(e: &windows::core::Error) -> bool {
    matches!(
        e.code().0 as u32,
        NTE_DEVICE_NOT_READY | TPM_CONFLICT_OBSERVED
    )
}

/// 对「TPM 忙」做有限退避重试（3 次，40/80/160 ms）。
///
/// 系统里 Hello 的对话框刚关、BitLocker 正在密封、或另一个实例正在解锁，都会让一次
/// `NCryptCreatePersistedKey` 撞上忙。不重试的后果是「用户点开启，随机报错」。
/// **只重试明确的瞬时码**：其它错误立刻上抛，免得把真故障拖成 3 倍等待。
fn retry_transient<T>(
    mut f: impl FnMut() -> windows::core::Result<T>,
) -> Result<T, windows::core::Error> {
    let mut delay = 40u64;
    let mut attempt = 0;
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if is_transient(&e) && attempt < 3 => {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                delay *= 2;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

fn open_provider(name: &str) -> Result<Prov, String> {
    let mut prov = NCRYPT_PROV_HANDLE::default();
    let hname = HSTRING::from(name);
    retry_transient(|| unsafe { NCryptOpenStorageProvider(&mut prov, &hname, 0) })
        .map_err(|e| format!("open provider {}: {}", name, hr(&e)))?;
    Ok(Prov(prov))
}

/// 打开持久密钥（**不创建**）。解封路径专用：解封时若能顺手建一把新钥匙，
/// 就等于「用错钥匙解不开 → 留下一把垃圾钥匙」，最后把 TPM slot 耗光。
fn open_key(prov: &Prov, name: &str) -> Result<Key, String> {
    let mut key = NCRYPT_KEY_HANDLE::default();
    unsafe {
        NCryptOpenKey(
            prov.0,
            &mut key,
            &HSTRING::from(name),
            CERT_KEY_SPEC(0),
            NCRYPT_FLAGS(0),
        )
    }
    .map_err(|e| format!("open key {}: {}", name, hr(&e)))?;
    Ok(Key(key))
}

/// 导出持久密钥的**公开**部分并取其摘要，作为「这把钥匙是哪一把」的指纹（D-127）。
///
/// 只导出 `RSAPUBLICBLOB`（模数 + 指数），私钥不可导出（这是 T1b 的立身之本，已有
/// `tpm_private_key_is_not_exportable` 守着）。指纹不是机密：它是公开参数的摘要，
/// 泄露它也解不出任何 VK。
///
/// 取前 8 字节十六进制就够：这把钥匙要么完全一致、要么完全不同，碰撞概率与"猜中
/// 2048 位模数"同量级。短一些也便于写进日志与 `biometric.json` 给人看。
fn key_public_fingerprint(key: &Key) -> Result<String, String> {
    let blob_type = HSTRING::from("RSAPUBLICBLOB");
    let mut needed = 0u32;
    // 第一次只问长度（`pboutput = None`）。
    unsafe {
        NCryptExportKey(
            key.0,
            None,
            &blob_type,
            None,
            None,
            &mut needed,
            NCRYPT_FLAGS(0),
        )
    }
    .map_err(|e| format!("export size: {}", hr(&e)))?;
    // RSA-2048 的公开 blob 是 276 字节；给足余量但**拒绝**荒谬值，
    // 免得一个坏驱动让这里分配出几百 MB。
    if needed == 0 || needed > 8192 {
        return Err(format!("export size out of range: {needed}"));
    }
    let mut buf = vec![0u8; needed as usize];
    unsafe {
        NCryptExportKey(
            key.0,
            None,
            &blob_type,
            None,
            Some(&mut buf),
            &mut needed,
            NCRYPT_FLAGS(0),
        )
    }
    .map_err(|e| format!("export: {}", hr(&e)))?;
    buf.truncate(needed as usize);
    let digest = Sha256::digest(&buf);
    Ok(digest[..8].iter().map(|b| format!("{b:02x}")).collect())
}

/// 打开持久密钥；不存在则创建并 finalize。
fn open_or_create_key(prov: &Prov, name: &str) -> Result<Key, String> {
    if let Ok(key) = open_key(prov, name) {
        return Ok(key);
    }
    let mut raw = NCRYPT_KEY_HANDLE::default();
    let alg = HSTRING::from("RSA");
    let name_h = HSTRING::from(name);
    retry_transient(|| unsafe {
        NCryptCreatePersistedKey(
            prov.0,
            &mut raw,
            &alg,
            &name_h,
            CERT_KEY_SPEC(0),
            NCRYPT_FLAGS(0),
        )
    })
    .map_err(|e| format!("create key {}: {}", name, hr(&e)))?;
    // `raw` 现在归我们管：下面任何一步失败都要通过 `Key` 的 Drop 释放。
    let key = Key(raw);

    // 顺序照抄已实测跑通的探针（`target/probe-tpm`）：**不设 Length**。
    // 实测 `NCRYPT_LENGTH_PROPERTY` 在 create 之后设置会让 finalize 报 `0x80280909`——
    // 该 provider 把长度当成创建参数，拒绝在 finalize 前改写。默认已是 RSA-2048，
    // 与探针验证过的组合一致；需要换长度就改 `NCryptCreatePersistedKey` 的参数，别绕这条路。

    // 约束 1：不设用途，`NCryptEncrypt` 必报 0x80090027。
    unsafe {
        NCryptSetProperty(
            NCRYPT_HANDLE(key.0 .0),
            NCRYPT_KEY_USAGE_PROPERTY,
            &NCRYPT_ALLOW_ALL_USAGES.to_le_bytes(),
            NCRYPT_FLAGS(0),
        )
    }
    .map_err(|e| format!("set key usage {}", hr(&e)))?;

    retry_transient(|| unsafe { NCryptFinalizeKey(key.0, NCRYPT_FLAGS(0)) })
        .map_err(|e| format!("finalize key {}", hr(&e)))?;
    Ok(key)
}

/// 本机的 TPM 是否就绪（只验 provider 能否打开，**不**建密钥、无副作用）。
///
/// 更深一层的「能不能真建出密钥」在 `seal` 里才暴露；探测阶段建钥匙会在旧机器上
/// 平白留下一个 slot，代价比收益高。
pub fn tpm_ready() -> bool {
    open_provider(PROVIDER).is_ok()
}

/// PKCS#1 v1.5 加密（约束 2：OAEP 在本 provider 上四种组合全失败）。
fn encrypt(key: &Key, plain: &[u8]) -> Result<Vec<u8>, String> {
    let flags = NCRYPT_FLAGS(NCRYPT_PAD_PKCS1_FLAG.0 | NCRYPT_SILENT_FLAG.0);
    let mut need = 0u32;
    retry_transient(|| unsafe { NCryptEncrypt(key.0, Some(plain), None, None, &mut need, flags) })
        .map_err(|e| format!("encrypt size {}", hr(&e)))?;
    let need = need as usize;
    if need == 0 {
        return Err("encrypt size: 0".into());
    }
    let mut out = vec![0u8; need];
    let mut written = 0u32;
    retry_transient(|| unsafe {
        NCryptEncrypt(
            key.0,
            Some(plain),
            None,
            Some(&mut out),
            &mut written,
            flags,
        )
    })
    .map_err(|e| format!("encrypt {}", hr(&e)))?;
    out.truncate(written as usize);
    Ok(out)
}

fn decrypt(key: &Key, cipher: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    let flags = NCRYPT_FLAGS(NCRYPT_PAD_PKCS1_FLAG.0 | NCRYPT_SILENT_FLAG.0);
    let mut need = 0u32;
    retry_transient(|| unsafe { NCryptDecrypt(key.0, Some(cipher), None, None, &mut need, flags) })
        .map_err(|e| format!("decrypt size {}", hr(&e)))?;
    let need = need as usize;
    if need == 0 {
        return Err("decrypt size: 0".into());
    }
    let mut out = vec![0u8; need];
    let mut written = 0u32;
    retry_transient(|| unsafe {
        NCryptDecrypt(
            key.0,
            Some(cipher),
            None,
            Some(&mut out),
            &mut written,
            flags,
        )
    })
    .map_err(|e| format!("decrypt {}", hr(&e)))?;
    out.truncate(written as usize);
    Ok(Zeroizing::new(out))
}

/// 删除持久密钥（幂等：不存在也算成功）。
///
/// **固定名字 ⇒ 删除影响本机所有以本密封器保护的库**。当前版本单库（`vault_path` 固定），
/// 故无实际影响；将来若支持多库，这里必须改成按库派生密钥名 + 枚举清理（§33.5）。
pub fn purge(key_name: &str) -> Result<(), String> {
    let _guard = lock_tpm();
    let prov = open_provider(PROVIDER)?;
    match open_key(&prov, key_name) {
        Ok(key) => key.delete(),
        Err(_) => Ok(()),
    }
}

/// TPM 密封器（Windows）。
pub struct TpmSealer<G: PresenceGate> {
    gate: G,
    key_name: String,
}

impl TpmSealer<super::windows_dpapi::HelloGate> {
    /// 真机适配器。
    pub fn new() -> Self {
        Self {
            gate: super::windows_dpapi::HelloGate,
            key_name: DEFAULT_KEY_NAME.to_string(),
        }
    }
}

impl<G: PresenceGate> TpmSealer<G> {
    /// 注入闸门与密钥名（测试用：真 TPM + 假闸门 + 独立钥匙，互不污染）。
    pub fn with_parts(gate: G, key_name: impl Into<String>) -> Self {
        Self {
            gate,
            key_name: key_name.into(),
        }
    }
}

impl<G: PresenceGate> VaultSealer for TpmSealer<G> {
    fn availability(&self) -> Availability {
        let gate = self.gate.availability();
        // 闸门不可用时不必再探 TPM：先说用户能理解的那件事（去开 Hello）。
        if gate != Availability::Available {
            return gate;
        }
        if !tpm_ready() {
            // 有 Hello 但 TPM 不可用（虚拟机、被策略禁用、TPM 被清）：
            // 报「本平台做不了」而不是「未配置」——把用户赶去系统设置找一个
            // 不存在的开关，比直接说不可用更糟。
            return Availability::Unsupported;
        }
        Availability::Available
    }

    fn subtype(&self) -> Subtype {
        Subtype::WindowsTpm2Seal
    }

    fn seal(&self, vk: &[u8], bind: &[u8]) -> Result<Vec<u8>, SealError> {
        if vk.len() != VK_LEN {
            return Err(SealError::Platform("tpm vk len".into()));
        }
        // 闸门先于加解密：没有在场就不该产出密文（否则等于给了免密通道）。
        let availability = self.availability();
        if availability != Availability::Available {
            return Err(SealError::Unavailable(availability));
        }
        self.gate.request(PresenceReason::Enable)?;

        // 锁只覆盖 CNG 段：闸门已经关掉了，用户不会再等。
        let _guard = lock_tpm();
        let prov = open_provider(PROVIDER).map_err(SealError::Platform)?;
        let key = open_or_create_key(&prov, &self.key_name).map_err(SealError::Platform)?;

        // bind 不进 RSA（CNG 没有 AAD 概念），改为附在明文尾部一起被加密，
        // 解封时比对 ⇒ 换 bind 必然解不开（与 DPAPI 的 `pOptionalEntropy` 同效）。
        let mut plain = Zeroizing::new(Vec::with_capacity(PLAIN_LEN));
        plain.extend_from_slice(vk);
        plain.extend_from_slice(&bind_tag(bind));

        let cipher = encrypt(&key, &plain).map_err(SealError::Platform)?;
        // 长度即强度：provider 里若有别人早先建的同名短钥匙（1024 位 = 128 字节），
        // 我们会照用不误——那会悄悄把保护降到可被分解的级别。
        if cipher.len() != RSA_BYTES {
            return Err(SealError::Platform(format!(
                "tpm modulus {} bytes, want {RSA_BYTES}",
                cipher.len()
            )));
        }
        let mut out = Vec::with_capacity(MAGIC.len() + cipher.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&cipher);
        Ok(out)
    }

    fn unseal(&self, sealed: &[u8], bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        if !is_tpm_blob(sealed) {
            return Err(SealError::Platform("tpm magic".into()));
        }
        let availability = self.availability();
        if availability != Availability::Available {
            return Err(SealError::Unavailable(availability));
        }
        self.gate.request(PresenceReason::Unlock)?;

        let _guard = lock_tpm();
        let prov = open_provider(PROVIDER).map_err(SealError::Platform)?;
        // 只开不建：解封路径不该产生任何新钥匙。
        //
        // D-127：打不开**不是**密文的问题。持久密钥被清理 / 被重建后，`biometric.json`
        // 一个字节都没变，却再也解不开——报 `KeyGone` 让 core 说「平台密钥被重置，
        // 重新开启即可」，而不是「配置被篡改」。
        let key = match open_key(&prov, &self.key_name) {
            Ok(k) => k,
            Err(_) => return Err(SealError::KeyGone),
        };
        let plain = decrypt(&key, &sealed[MAGIC.len()..]).map_err(SealError::Platform)?;

        if plain.len() != PLAIN_LEN {
            return Err(SealError::Platform("tpm plaintext len".into()));
        }
        // 定长比较，不泄露「前多少字节对上了」。
        let expect = bind_tag(bind);
        let mut diff = 0u8;
        for (a, b) in plain[VK_LEN..].iter().zip(expect.iter()) {
            diff |= a ^ b;
        }
        if diff != 0 {
            return Err(SealError::Platform("tpm bind tag".into()));
        }

        let mut vk: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0u8; VK_LEN]);
        vk.copy_from_slice(&plain[..VK_LEN]);
        Ok(vk)
    }

    /// D-127：当前 TPM 持久密钥的公钥指纹。
    ///
    /// `None` = 打不开（密钥不在 / TPM 此刻不可用）。**不弹任何窗**——这个调用发生在
    /// 用户已经点过「用 Windows Hello 解锁」之后，再弹一次就是两次认证框。
    fn key_fingerprint(&self) -> Option<String> {
        let _guard = lock_tpm();
        let prov = open_provider(PROVIDER).ok()?;
        let key = open_key(&prov, &self.key_name).ok()?;
        key_public_fingerprint(&key).ok()
    }
}

impl Default for TpmSealer<super::windows_dpapi::HelloGate> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use passwordms_core::biometric::{SEAL_VK_MAX_LEN, SEAL_VK_MIN_LEN};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 假闸门：不弹框、可计数。
    struct Gate {
        allow: bool,
        prompts: AtomicUsize,
    }

    impl Gate {
        fn allowing() -> Self {
            Self {
                allow: true,
                prompts: AtomicUsize::new(0),
            }
        }
    }

    impl PresenceGate for Gate {
        fn availability(&self) -> Availability {
            Availability::Available
        }
        fn request(&self, _reason: PresenceReason) -> Result<(), SealError> {
            self.prompts.fetch_add(1, Ordering::SeqCst);
            if self.allow {
                Ok(())
            } else {
                Err(SealError::Canceled)
            }
        }
    }

    /// 每个测试用独立钥匙名，跑完删掉——**绝不能**碰产品那把
    /// `PasswordMS-VK-Seal`，否则会毁掉开发机上真实开启过的快速解锁。
    fn test_key_name(tag: &str) -> String {
        format!(
            "PasswordMS-Test-{}-{}",
            tag,
            &uuid::Uuid::new_v4().to_string()[..8]
        )
    }

    struct Fixture {
        sealer: TpmSealer<Gate>,
        name: String,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = purge(&self.name);
        }
    }

    fn fixture(tag: &str) -> Fixture {
        let name = test_key_name(tag);
        Fixture {
            sealer: TpmSealer::with_parts(Gate::allowing(), name.clone()),
            name,
        }
    }

    /// D-127：指纹必须**真的取得到**，而且要稳定。
    ///
    /// 这一段是整个「密钥被重置 vs 密文被篡改」分流的地基：指纹取不到，
    /// `classify_unseal_failure` 就永远走「证据不足 ⇒ 按篡改处理」那条路，分流形同虚设，
    /// 而所有单测（用 Mock）照样全绿。所以必须碰真 TPM 验一次。
    ///
    /// 用独立密钥名（`fixture` 的 `test_key_name`），Drop 时 purge，不碰主人的真实封印。
    #[test]
    fn key_fingerprint_is_retrievable_and_stable() {
        if !tpm_or_skip() {
            return;
        }
        let f = fixture("fp");
        // 密钥还没建（`key_fingerprint` 只开不建）⇒ 必须如实返回 None，不能编一个假的。
        assert!(
            f.sealer.key_fingerprint().is_none(),
            "密钥未创建时不得凭空给出指纹"
        );

        f.sealer
            .seal(&[0x11u8; 32], &bind(9))
            .expect("seal 会建出持久密钥");

        let a = f
            .sealer
            .key_fingerprint()
            .expect("有 TPM 且密钥已建，指纹必须取得到");
        let b = f.sealer.key_fingerprint().expect("第二次同样要取得到");
        assert_eq!(
            a, b,
            "指纹必须稳定：不稳定会让每次解锁都误判成「密钥被重置」"
        );
        assert_eq!(a.len(), 16, "8 字节 hex = 16 字符");
        assert!(
            a.chars().all(|c| c.is_ascii_hexdigit()),
            "指纹应为小写十六进制，实际 {a}"
        );
    }

    /// 两把**不同的**持久密钥必须给出不同指纹——否则分流会把「换过钥匙」判成「没换」。
    #[test]
    fn different_keys_have_different_fingerprints() {
        if !tpm_or_skip() {
            return;
        }
        let a = fixture("fp-a");
        let b = fixture("fp-b");
        a.sealer.seal(&[0x22u8; 32], &bind(10)).unwrap();
        b.sealer.seal(&[0x22u8; 32], &bind(10)).unwrap();
        assert_ne!(
            a.sealer.key_fingerprint(),
            b.sealer.key_fingerprint(),
            "两把不同的 RSA 密钥不能撞指纹"
        );
    }

    /// 本机没有 TPM 时，依赖真 TPM 的用例跳过（不是失败）：CI/虚拟机上它们无从验证。
    fn tpm_or_skip() -> bool {
        if tpm_ready() {
            true
        } else {
            eprintln!("skip: 本机无 {PROVIDER}");
            false
        }
    }

    fn bind(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    /// 端到端：真 TPM 加密 48 字节并解回原 VK。
    #[test]
    fn tpm_roundtrip() {
        if !tpm_or_skip() {
            return;
        }
        let f = fixture("roundtrip");
        let vk = [0xA5u8; VK_LEN];
        let sealed = f.sealer.seal(&vk, &bind(1)).expect("seal");
        assert_eq!(&sealed[..MAGIC.len()], &MAGIC[..], "必须带魔数");
        let opened = f.sealer.unseal(&sealed, &bind(1)).expect("unseal");
        assert_eq!(&opened[..], &vk[..]);
    }

    /// **失效模型的守门**：换 `bind` 必须解不开（尾巴校验）。
    ///
    /// 若谁把 `bind_tag` 从明文里删掉（「反正 RSA 已经够了」），这条会红——
    /// 而 core 侧 29 例仍全绿，因为它们用的是 Mock。
    #[test]
    fn mismatched_bind_cannot_unseal() {
        if !tpm_or_skip() {
            return;
        }
        let f = fixture("bind-mismatch");
        let sealed = f.sealer.seal(&[0x5Au8; VK_LEN], &bind(1)).expect("seal");
        assert!(
            f.sealer.unseal(&sealed, &bind(2)).is_err(),
            "换了 bind 还能解开 ⇒ bind 没进明文"
        );
    }

    /// 密文任一字节被改 ⇒ 解不开（RSA 填充校验 + 长度校验）。
    #[test]
    fn tampered_ciphertext_cannot_unseal() {
        if !tpm_or_skip() {
            return;
        }
        let f = fixture("tamper");
        let mut sealed = f.sealer.seal(&[0x11u8; VK_LEN], &bind(3)).expect("seal");
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert!(f.sealer.unseal(&sealed, &bind(3)).is_err());
    }

    /// 输出长度必须落在 core 白名单（`48..=1024`）内，且**不得**含明文 VK。
    #[test]
    fn sealed_length_satisfies_core_whitelist() {
        if !tpm_or_skip() {
            return;
        }
        let f = fixture("len");
        let vk = [0x33u8; VK_LEN];
        let sealed = f.sealer.seal(&vk, &bind(7)).expect("seal");
        assert!(
            (SEAL_VK_MIN_LEN..=SEAL_VK_MAX_LEN).contains(&sealed.len()),
            "TPM 密文长度 {} 不在 core 白名单 {}..={}",
            sealed.len(),
            SEAL_VK_MIN_LEN,
            SEAL_VK_MAX_LEN
        );
        assert!(
            !sealed.windows(VK_LEN).any(|w| w == vk),
            "密文里出现了明文 VK"
        );
    }

    /// 钥匙不存在时解封：`KeyGone`（core 归为「密钥被重置」），**不是** `Invalidated`。
    ///
    /// 判成 `Invalidated` 会让命令层删平台侧材料并留墓碑——那是在「可能只是 TPM 暂时
    /// 读不到」的情况下替攻击者完成破坏（§18.5 约束 3 的同一条原则）。
    ///
    /// D-127 把这里从 `Platform` 改成 `KeyGone`：`Platform` 会被 core 归成「密文被篡改」，
    /// 而钥匙缺失与密文完整性毫无关系。`KeyGone` → `PlatformKeyReset`，**不 purge、不写墓碑**，
    /// 所以上面那条原则仍然成立，只是原因说得更准。
    #[test]
    fn missing_key_is_key_gone_not_invalidated() {
        if !tpm_or_skip() {
            return;
        }
        let name = test_key_name("missing");
        let sealer = TpmSealer::with_parts(Gate::allowing(), name.clone());
        // 造一个格式正确但钥匙不存在的密文。
        let prov = open_provider(PROVIDER).unwrap();
        let key = open_or_create_key(&prov, &name).unwrap();
        let mut plain = vec![0u8; PLAIN_LEN];
        plain[..VK_LEN].copy_from_slice(&[0x77u8; VK_LEN]);
        plain[VK_LEN..].copy_from_slice(&bind_tag(&bind(1)));
        let cipher = encrypt(&key, &plain).unwrap();
        key.delete().unwrap();

        let mut sealed = MAGIC.to_vec();
        sealed.extend(cipher);
        match sealer.unseal(&sealed, &bind(1)) {
            Err(SealError::KeyGone) => {}
            Err(SealError::Invalidated) => {
                panic!("钥匙缺失不得归 Invalidated：那会让命令层删材料、留墓碑")
            }
            other => panic!("钥匙缺失应归 KeyGone，实际 {other:?}"),
        }
    }

    /// 非本密封器的密文（DPAPI blob）必须被魔数挡住。
    #[test]
    fn foreign_blob_is_rejected() {
        let f = fixture("foreign");
        match f
            .sealer
            .unseal(&[0x01, 0x00, 0x00, 0x00, 0xAA, 0xBB], &bind(1))
        {
            Err(SealError::Platform(msg)) => assert!(msg.contains("magic"), "实际 {msg}"),
            other => panic!("外来密文应被魔数拒绝，实际 {other:?}"),
        }
    }

    /// `tpm_ready` 不能把「provider 打不开」当成就绪。
    #[test]
    fn unknown_provider_is_not_ready() {
        assert!(open_provider("PasswordMS No Such Provider").is_err());
        assert!(
            tpm_ready(),
            "本机 TPM 应就绪（这是开发机，probe 已验证）——若不就绪，请确认 Intel PTT 是否被关"
        );
    }

    /// **T1b 的全部依据**：私钥不可导出。这条一旦变绿→红，TPM 相对 DPAPI 的收益就归零，
    /// 必须立刻停下重新评估，而不是继续发版。
    #[test]
    fn tpm_private_key_is_not_exportable() {
        use windows::Win32::Security::Cryptography::NCryptExportKey;
        if !tpm_or_skip() {
            return;
        }
        let f = fixture("export");
        // 先建出钥匙（seal 时会建）。
        f.sealer.seal(&[0x99u8; VK_LEN], &bind(9)).expect("seal");
        let prov = open_provider(PROVIDER).unwrap();
        let key = open_key(&prov, &f.name).unwrap();

        let mut dump = vec![0u8; 4096];
        let mut written = 0u32;
        let result = unsafe {
            NCryptExportKey(
                key.0,
                None,
                &HSTRING::from("RSAPRIVATEKEYBLOB"),
                None,
                Some(&mut dump),
                &mut written,
                NCRYPT_FLAGS(0),
            )
        };
        assert!(
            result.is_err(),
            "私钥竟然导出了 {} 字节 ⇒ 磁盘镜像离线解密重新可行的风险，必须停下评估",
            written
        );
    }

    /// 闸门不可用时不该弹框、也不该白跑一次 TPM 探测。
    #[test]
    fn unavailable_gate_does_not_prompt() {
        use std::sync::Arc;

        struct Denied(Arc<AtomicUsize>);
        impl PresenceGate for Denied {
            fn availability(&self) -> Availability {
                Availability::NotConfigured
            }
            fn request(&self, _reason: PresenceReason) -> Result<(), SealError> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
        let prompts = Arc::new(AtomicUsize::new(0));
        let sealer = TpmSealer::with_parts(Denied(prompts.clone()), test_key_name("denied"));
        match sealer.seal(&[0x22u8; VK_LEN], &bind(3)) {
            Err(SealError::Unavailable(Availability::NotConfigured)) => {}
            other => panic!("未配置应映射为 Unavailable(NotConfigured)，实际 {other:?}"),
        }
        assert_eq!(
            prompts.load(Ordering::SeqCst),
            0,
            "平台不可用时不得请求在场校验"
        );
        // 解封同样先看可用性：连魔数都不必看，因为「平台不可用」是更基础的原因。
        match sealer.unseal(&sealed_dummy(), &bind(3)) {
            Err(SealError::Unavailable(Availability::NotConfigured)) => {}
            other => panic!("解封也应立刻报未配置，实际 {other:?}"),
        }
        assert_eq!(prompts.load(Ordering::SeqCst), 0);
    }

    fn sealed_dummy() -> Vec<u8> {
        let mut v = MAGIC.to_vec();
        v.extend([0u8; 8]);
        v
    }
}
