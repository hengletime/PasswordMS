//! Windows 适配器：DPAPI 用户域密封 + Windows Hello 在场闸门
//! （`docs/08-biometric-unlock.md` §6.1）。
//!
//! ## 两段式，不是一段
//!
//! | 段 | 手段 | 挡住什么 |
//! |---|---|---|
//! | 加解密 | `CryptProtectData` / `CryptUnprotectData`（用户域） | 换机、换 Windows 用户、别的用户账户直接读 `biometric.json` |
//! | 在场闸门 | `UserConsentVerifier` | 本机解锁态被别人顺手点开 |
//!
//! **两段都是软的**（§6.3 已如实登记）：DPAPI 用户域主密钥在磁盘上，只是被账户口令派生的
//! 密钥加密 ⇒ 拿到本机 Windows 登录凭据者可以绕过 Hello 直接解密。UI 文案必须写「Windows
//! Hello 快速解锁 + 风险说明」，**不得**写成「指纹解锁」——`UserConsentVerifier` 不暴露认证
//! 方式，PIN 也算通过。
//!
//! ## 为什么 `bind` 必须进 `pOptionalEntropy`
//!
//! `bind` 是「本库当前密钥身份」的摘要（vault_id + verify_mac + wrapped_vault_key）。把它当
//! 附加熵之后：主密码改了、或配对换过 VK，`bind` 就变，**旧密文必然解不开**——这是 D-113
//! 堵死「换 VK 后旧封印交出旧 VK」的最后一环。若适配器图省事传 `None`，core 侧 27 例守门
//! 全绿而失效模型归零，故本文件自带 `mismatched_entropy_cannot_unseal` 守着（§18.5 约束 1）。

use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use passwordms_core::biometric::{Availability, SealError, Subtype, VaultSealer};
use zeroize::{Zeroize, Zeroizing};

use windows::core::{Error as WinError, HSTRING, PCWSTR};
use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

use super::{PresenceGate, PresenceReason};

/// Windows Hello 在场闸门。
///
/// 无状态：可用性每次都现查（系统设置可能随时被改），缓存会让「用户刚关掉 Hello，应用仍显示
/// 可用」这种状态持续到重启。
#[derive(Debug, Default, Clone, Copy)]
pub struct HelloGate;

impl PresenceGate for HelloGate {
    fn availability(&self) -> Availability {
        match hello::check_availability(Duration::from_secs(20)) {
            Ok(a) => map_availability(a),
            // WinRT 激活失败（被裁剪的系统 / 服务异常）：当「不支持」处理。
            // **绝不能**当成 Invalidated —— 那会引导用户删掉一条本来好用的封印（§18.5 约束 3）。
            Err(_) => Availability::Unsupported,
        }
    }

    fn request(&self, reason: PresenceReason) -> Result<(), SealError> {
        let message = match reason {
            PresenceReason::Enable => "开启 PasswordMS 快速解锁",
            PresenceReason::Unlock => "解锁 PasswordMS",
        };
        // 120 秒：够慢的用户点完，又不至于让解锁页永远转圈。
        match hello::request_verification(message, Duration::from_secs(120)) {
            Ok(result) => map_result(result),
            // 超时 = 暂时不可用，**不是**失效：不删封印，只是要求改用主密码。
            Err(hello::WaitError::Timeout) => Err(SealError::Unavailable(Availability::Busy)),
            Err(hello::WaitError::Platform(detail)) => Err(SealError::Platform(detail)),
        }
    }
}

/// `UserConsentVerifierAvailability` → core 的 [`Availability`]。
fn map_availability(a: UserConsentVerifierAvailability) -> Availability {
    match a {
        UserConsentVerifierAvailability::Available => Availability::Available,
        UserConsentVerifierAvailability::DeviceNotPresent => Availability::NotPresent,
        UserConsentVerifierAvailability::NotConfiguredForUser => Availability::NotConfigured,
        UserConsentVerifierAvailability::DisabledByPolicy => Availability::DisabledByPolicy,
        UserConsentVerifierAvailability::DeviceBusy => Availability::Busy,
        _ => Availability::Unsupported,
    }
}

/// `UserConsentVerificationResult` → core 的 [`SealError`]。
///
/// **`DeviceBusy` 与 `RetriesExhausted` 的落点**（§18.5 约束 3）：
/// - `DeviceBusy` → `Unavailable(Busy)`：设备被占用，稍后重试即可；
/// - `RetriesExhausted`（连试多次都不对）→ `Canceled`：是「这次没成」，不是「密钥失效」。
///   把任何一条映射成 `Invalidated` 都会弹「生物特征有变更」并引导删封印——那是数据事故。
fn map_result(r: UserConsentVerificationResult) -> Result<(), SealError> {
    match r {
        UserConsentVerificationResult::Verified => Ok(()),
        UserConsentVerificationResult::Canceled
        | UserConsentVerificationResult::RetriesExhausted => Err(SealError::Canceled),
        UserConsentVerificationResult::DeviceNotPresent => {
            Err(SealError::Unavailable(Availability::NotPresent))
        }
        UserConsentVerificationResult::NotConfiguredForUser => {
            Err(SealError::Unavailable(Availability::NotConfigured))
        }
        UserConsentVerificationResult::DisabledByPolicy => {
            Err(SealError::Unavailable(Availability::DisabledByPolicy))
        }
        UserConsentVerificationResult::DeviceBusy => {
            Err(SealError::Unavailable(Availability::Busy))
        }
        _ => Err(SealError::Platform("hello unknown result".into())),
    }
}

/// DPAPI 用户域密封器（Windows 桌面）。
pub struct DpapiSealer<G: PresenceGate> {
    gate: G,
}

impl DpapiSealer<HelloGate> {
    /// 真机适配器。
    pub fn new() -> Self {
        Self { gate: HelloGate }
    }
}

impl<G: PresenceGate> DpapiSealer<G> {
    /// 注入闸门的适配器（测试用：真 DPAPI + 假闸门，才能在不弹框的前提下验 `bind`）。
    pub fn with_gate(gate: G) -> Self {
        Self { gate }
    }
}

impl Default for DpapiSealer<HelloGate> {
    fn default() -> Self {
        Self::new()
    }
}

impl<G: PresenceGate> VaultSealer for DpapiSealer<G> {
    fn availability(&self) -> Availability {
        self.gate.availability()
    }

    fn subtype(&self) -> Subtype {
        Subtype::WindowsDpapiUser
    }

    fn seal(&self, vk: &[u8], bind: &[u8]) -> Result<Vec<u8>, SealError> {
        // 闸门先于加解密：没有在场就不该产出密文（否则等于给了免密通道）。
        let availability = self.gate.availability();
        if availability != Availability::Available {
            return Err(SealError::Unavailable(availability));
        }
        self.gate.request(PresenceReason::Enable)?;
        dpapi_protect(vk, bind)
    }

    fn unseal(&self, sealed: &[u8], bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        let availability = self.gate.availability();
        if availability != Availability::Available {
            return Err(SealError::Unavailable(availability));
        }
        self.gate.request(PresenceReason::Unlock)?;
        dpapi_unprotect(sealed, bind)
    }
}

/// `bind` 作附加熵加密（用户域；`CRYPTPROTECT_UI_FORBIDDEN` 禁止 DPAPI 自己弹框）。
fn dpapi_protect(plain: &[u8], bind: &[u8]) -> Result<Vec<u8>, SealError> {
    if plain.is_empty() {
        return Err(SealError::Platform("dpapi empty plaintext".into()));
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: bind.len() as u32,
        pbData: bind.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // **绝不加** `CRYPTPROTECT_LOCAL_MACHINE`：那会让同机任意账户都能解密，直接把
    // 「用户域」这个安全前提丢掉（§18.5 约束 4）。
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|e| SealError::Platform(format!("CryptProtectData {}", hr_hex(&e))))?;
    }
    Ok(unsafe { take_blob(&mut output) })
}

/// `bind` 作附加熵解密（熵不符 ⇒ DPAPI 报错 ⇒ 上层归类为「配置损坏」而非「失效」）。
fn dpapi_unprotect(sealed: &[u8], bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
    if sealed.is_empty() {
        return Err(SealError::Platform("dpapi empty ciphertext".into()));
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: sealed.len() as u32,
        pbData: sealed.as_ptr() as *mut u8,
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: bind.len() as u32,
        pbData: bind.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let result = unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    // 失败路径也可能已经分配了缓冲（MSDN 明确要求无论如何都检查 `pDataOut`）——
    // 先统一「拷出 + 覆写 + 释放」，再判成败。
    let bytes = unsafe { take_blob(&mut output) };
    if let Err(e) = result {
        return Err(SealError::Platform(format!(
            "CryptUnprotectData {}",
            hr_hex(&e)
        )));
    }
    Ok(Zeroizing::new(bytes))
}

/// 把 DPAPI 分配的输出缓冲拷出、**覆写后**释放。
///
/// 释放必须走 `LocalFree`（DPAPI 用 `LocalAlloc` 分配），且覆写要在释放前完成——
/// `CryptUnprotectData` 交出来的就是**明文 VK**，直接 `LocalFree` 会把它原样留在堆上
/// 等着被下一块分配读到（§18.5 约束 5）。`Zeroize` 用 volatile 写 + 编译器屏障，
/// 不会被「反正马上要 free」优化掉。
unsafe fn take_blob(blob: &mut CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let len = blob.cbData as usize;
    let ptr = blob.pbData;
    blob.cbData = 0;
    blob.pbData = std::ptr::null_mut();
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    let mut out = vec![0u8; len];
    std::ptr::copy_nonoverlapping(ptr, out.as_mut_ptr(), len);
    std::slice::from_raw_parts_mut(ptr, len).zeroize();
    // 释放失败也无法补救（缓冲已不可达），这里只关心副作用，不判返回值。
    LocalFree(Some(HLOCAL(ptr as *mut core::ffi::c_void)));
    out
}

fn hr_hex(e: &WinError) -> String {
    format!("0x{:08X}", e.code().0)
}

/// Windows Hello 调用（WinRT）。单独成模块，便于把「线程/公寓/消息泵」这些平台细节关在一处。
mod hello {
    use super::*;

    /// 等待过程中的失败分类。
    ///
    /// 与 [`SealError`] 分开：这里只描述「等不到结果」，语义映射留给调用方
    /// （超时 ≠ 取消 ≠ 平台不支持，混在一起就会误删封印）。
    pub enum WaitError {
        /// 超时（用户一直不响应 / 对话框没出来）。
        Timeout,
        /// 平台调用失败（`0x…` HRESULT）。
        Platform(String),
    }

    /// 查询可用性（快，15 秒内必有结果）。
    pub fn check_availability(
        timeout: Duration,
    ) -> Result<UserConsentVerifierAvailability, WaitError> {
        run_on_sta(timeout, |tx| {
            let op = UserConsentVerifier::CheckAvailabilityAsync()?;
            op.when(move |r| {
                let _ = tx.send(r);
            })
        })
    }

    /// 请求一次在场校验（会弹系统对话框）。
    pub fn request_verification(
        message: &str,
        timeout: Duration,
    ) -> Result<UserConsentVerificationResult, WaitError> {
        let message = HSTRING::from(message);
        run_on_sta(timeout, move |tx| {
            let op = UserConsentVerifier::RequestVerificationAsync(&message)?;
            op.when(move |r| {
                let _ = tx.send(r);
            })
        })
    }

    /// 在专用 **STA** 线程上跑一次 WinRT 异步调用。
    ///
    /// 为什么不让命令线程直接调：
    /// 1. WinRT 要求线程先 `CoInitializeEx`，而 Tauri 的命令线程既没初始化、也不能保证是哪种公寓；
    /// 2. `UserConsentVerifier` 会弹系统对话框——若对话框的消息落在本线程，就必须有人**抽送消息**
    ///    才能画出来。这里用 `CoInitializeEx(APARTMENTTHREADED)` + `PeekMessage` 泵，两头都满足；
    /// 3. 顺带把「生物识别操作」串行化：同一时刻只有一个 hello 线程在跑（§18.5 约束 8 的一半，
    ///    另一半是命令层自己的互斥）。
    fn run_on_sta<T, F>(timeout: Duration, start: F) -> Result<T, WaitError>
    where
        T: Send + 'static,
        F: FnOnce(Sender<windows::core::Result<T>>) -> windows::core::Result<()> + Send + 'static,
    {
        let (result_tx, result_rx) = std::sync::mpsc::channel::<Result<T, WaitError>>();
        let spawned = std::thread::Builder::new()
            .name("passwordms-hello".into())
            .spawn(move || {
                let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
                let (tx, rx) = std::sync::mpsc::channel();
                let outcome = match start(tx) {
                    Err(e) => Err(WaitError::Platform(hr_hex(&e))),
                    Ok(()) => match pump(&rx, timeout) {
                        Some(Ok(v)) => Ok(v),
                        Some(Err(e)) => Err(WaitError::Platform(hr_hex(&e))),
                        None => Err(WaitError::Timeout),
                    },
                };
                // 只有真初始化成功才配对反初始化（`RPC_E_CHANGED_MODE` 时本线程不归我们管）。
                if hr.is_ok() {
                    unsafe { CoUninitialize() };
                }
                let _ = result_tx.send(outcome);
            });
        if spawned.is_err() {
            return Err(WaitError::Platform("hello thread spawn".into()));
        }
        // 多给 5 秒余量：`pump` 自己的截止时间才是权威，这里只防线程彻底卡死。
        result_rx
            .recv_timeout(timeout + Duration::from_secs(5))
            .unwrap_or(Err(WaitError::Timeout))
    }

    /// 等异步结果，同时抽送本线程消息（对话框需要）。
    fn pump<T>(
        rx: &Receiver<windows::core::Result<T>>,
        timeout: Duration,
    ) -> Option<windows::core::Result<T>> {
        let deadline = Instant::now() + timeout;
        loop {
            match rx.try_recv() {
                Ok(value) => return Some(value),
                Err(TryRecvError::Disconnected) => return None,
                Err(TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return None;
            }
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use passwordms_core::biometric::{SEAL_VK_MAX_LEN, SEAL_VK_MIN_LEN};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 假闸门：不弹框，记录被调用次数，用来把「DPAPI 本身」和「Hello 闸门」分开验。
    struct Gate {
        availability: Availability,
        allow: bool,
        prompts: AtomicUsize,
    }

    impl Gate {
        fn allowing() -> Self {
            Self {
                availability: Availability::Available,
                allow: true,
                prompts: AtomicUsize::new(0),
            }
        }
        fn denying() -> Self {
            Self {
                availability: Availability::Available,
                allow: false,
                prompts: AtomicUsize::new(0),
            }
        }
        fn unavailable(a: Availability) -> Self {
            Self {
                availability: a,
                allow: true,
                prompts: AtomicUsize::new(0),
            }
        }
    }

    impl PresenceGate for Gate {
        fn availability(&self) -> Availability {
            self.availability
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

    fn bind(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    /// 同熵往返：密文能解回原 VK。
    #[test]
    fn protect_roundtrip_with_same_entropy() {
        let vk = [0xA5u8; 32];
        let sealed = dpapi_protect(&vk, &bind(1)).expect("protect");
        let opened = dpapi_unprotect(&sealed, &bind(1)).expect("unprotect");
        assert_eq!(&opened[..], &vk[..]);
    }

    /// **§18.5 约束 1 的守门**：`bind` 真的进了 `pOptionalEntropy`——换熵必须解不开。
    ///
    /// 若哪天有人把 `Some(&entropy)` 改成 `None`（「反正 VK 是随机的」），core 侧 27 例
    /// 守门依然全绿，而「改主密码 / 配对换 VK 后旧封印自动失效」这条链会当场断掉。
    #[test]
    fn mismatched_entropy_cannot_unseal() {
        let vk = [0x5Au8; 32];
        let sealed = dpapi_protect(&vk, &bind(1)).expect("protect");
        assert!(
            dpapi_unprotect(&sealed, &bind(2)).is_err(),
            "换了 bind 还能解开 ⇒ bind 没进 pOptionalEntropy"
        );
    }

    /// DPAPI 输出长度必须落在 core 的白名单里（`48..=1024`）。
    ///
    /// 这条守的是 **D-113 复审 L-5** 的前提：core 用 `SEAL_VK_MIN_LEN = 48` 来防「适配器把
    /// 明文 VK 原样返回」。若真实平台输出恰好短于下界，`enable` 会直接拒写——那属于
    /// 适配器与 core 契约漂移，必须在适配器侧就发现。
    #[test]
    fn dpapi_output_satisfies_core_length_whitelist() {
        let vk = [0x11u8; 32];
        let sealed = dpapi_protect(&vk, &bind(7)).expect("protect");
        assert!(
            (SEAL_VK_MIN_LEN..=SEAL_VK_MAX_LEN).contains(&sealed.len()),
            "DPAPI 密文长度 {} 不在 core 白名单 {}..={}",
            sealed.len(),
            SEAL_VK_MIN_LEN,
            SEAL_VK_MAX_LEN
        );
        // 明文的 32 字节不该原样出现在密文里（DPAPI 有头部，位置不固定，做整体包含检查）。
        assert!(!sealed.windows(32).any(|w| w == vk), "密文里出现了明文 VK");
    }

    /// 闸门取消 ⇒ 不产出密文，且不再往下调 DPAPI。
    #[test]
    fn canceled_gate_blocks_sealing() {
        let sealer = DpapiSealer::with_gate(Gate::denying());
        match sealer.seal(&[0x22u8; 32], &bind(3)) {
            Err(SealError::Canceled) => {}
            other => panic!("取消应映射为 Canceled，实际 {other:?}"),
        }
    }

    /// 平台不可用 ⇒ `Unavailable`（**不是** `Invalidated`），且一次闸门都不该弹。
    #[test]
    fn unavailable_platform_does_not_prompt() {
        let gate = Gate::unavailable(Availability::NotConfigured);
        let sealer = DpapiSealer::with_gate(gate);
        match sealer.seal(&[0x22u8; 32], &bind(3)) {
            Err(SealError::Unavailable(Availability::NotConfigured)) => {}
            other => panic!("未配置应映射为 Unavailable(NotConfigured)，实际 {other:?}"),
        }
        assert_eq!(
            sealer.gate.prompts.load(Ordering::SeqCst),
            0,
            "平台不可用时不得弹框"
        );
    }

    /// `subtype` 必须是 DPAPI 用户域——写成 TPM2 会让老机器读到就判「未开启」。
    #[test]
    fn subtype_is_windows_dpapi_user() {
        let sealer = DpapiSealer::with_gate(Gate::allowing());
        assert_eq!(sealer.subtype(), Subtype::WindowsDpapiUser);
    }

    /// 结果映射：`RetriesExhausted` 必须归「取消」，**绝不能**归「失效」。
    #[test]
    fn retries_exhausted_is_canceled_not_invalidated() {
        match map_result(UserConsentVerificationResult::RetriesExhausted) {
            Err(SealError::Canceled) => {}
            other => panic!("重试耗尽应为 Canceled，实际 {other:?}"),
        }
        match map_result(UserConsentVerificationResult::DeviceBusy) {
            Err(SealError::Unavailable(Availability::Busy)) => {}
            other => panic!("设备占用应为 Unavailable(Busy)，实际 {other:?}"),
        }
        assert!(map_result(UserConsentVerificationResult::Verified).is_ok());
    }
}
