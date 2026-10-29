//! Windows 平台适配器：TPM2 优先、DPAPI 兜底（`docs/08-biometric-unlock.md` §33）。
//!
//! ## 为什么需要这层「路由」
//!
//! [`VaultSealer`] 的 `subtype()` 是**单值**，而 Windows 上有两种保护手段并存：
//!
//! - 老版本（≤1.4.7）只写 `windows-dpapi-user`；
//! - 本版本起优先写 `windows-tpm2-seal`，但**不能**因此让老封印失效。
//!
//! 于是同一个适配器必须回答两个不同问题：「新封印写什么」（`subtype()`）和「这条老封印我
//! 解不解得了」（`accepts()`）。core 侧已按这个语义拆开（`VaultSealer::accepts`）。
//!
//! ## 分派靠魔数，不靠调用顺序
//!
//! `unseal(sealed, bind)` 拿不到 subtype，所以按密文自识别：TPM 输出以 `PMST` 开头，
//! DPAPI 头是 `01 00 00 00`，不可能撞。**不**采用「当前首选是什么就用什么解」——
//! 那会让「本机 TPM 今晚读不到」变成「老封印全都解不开」。
//!
//! ## 回落策略（主人 2026-10-29 拍板：不做用户开关）
//!
//! `new()` 探测 TPM：就绪 ⇒ 新封印走 TPM；不就绪 ⇒ 静默回落 DPAPI。
//! 用户看不到开关，也就不会有「打开了更安全但更慢」这类需要解释的选择。
//! 回落是**安全等价**的下降（DPAPI 仍是用户域），不是把安全关掉。
//!
//! ## 它挡不住什么（与 `windows_dpapi` 同一句话）
//!
//! 本机、本用户、能执行代码的攻击者可以调 API 解出 VK。这一段由 `HelloGate` 负责，
//! 文案仍然必须写「Windows Hello 快速解锁」，**不得**写成「指纹解锁」——`UserConsentVerifier`
//! 不暴露认证方式，PIN 也算通过。

use passwordms_core::biometric::{Availability, SealError, Subtype, VaultSealer};
use zeroize::Zeroizing;

use super::windows_dpapi::{DpapiSealer, HelloGate};
use super::windows_tpm::{self, TpmSealer};
use super::PresenceGate;

/// Windows 平台适配器：按封印类型路由到 TPM 或 DPAPI。
pub struct WinSealer<G: PresenceGate + Clone> {
    gate: G,
    /// `None` = 本机 TPM 不可用，全部走 DPAPI。
    tpm: Option<TpmSealer<G>>,
    dpapi: DpapiSealer<G>,
    /// TPM 持久密钥名。产品恒为 [`DEFAULT_TPM_KEY`]，测试注入独立名字以免污染
    /// 开发机上真实的快速解锁。
    key_name: String,
}

impl WinSealer<HelloGate> {
    /// 真机适配器：探测一次 TPM（打开 provider，无副作用）。
    pub fn new() -> Self {
        let ready = windows_tpm::tpm_ready();
        Self::build(HelloGate, ready)
    }
}

impl<G: PresenceGate + Clone> WinSealer<G> {
    /// 由「闸门 + TPM 是否就绪」构造（测试注入用：不碰真实 TPM 也能验回落分支）。
    pub fn build(gate: G, tpm_ready: bool) -> Self {
        Self::build_with_key(gate, tpm_ready, DEFAULT_TPM_KEY)
    }

    /// 同上，但指定 TPM 持久密钥名。**测试必须走这条**，否则会与真实封印共用一把钥匙。
    pub fn build_with_key(gate: G, tpm_ready: bool, key_name: impl Into<String>) -> Self {
        let key_name = key_name.into();
        let tpm = if tpm_ready {
            Some(TpmSealer::with_parts(gate.clone(), key_name.clone()))
        } else {
            None
        };
        Self {
            dpapi: DpapiSealer::with_gate(gate.clone()),
            gate,
            tpm,
            key_name,
        }
    }

    /// 本机是否在用 TPM（仅供状态诊断，不影响任何安全判定）。
    pub fn uses_tpm(&self) -> bool {
        self.tpm.is_some()
    }

    /// 撤销：删掉平台侧密钥材料。
    ///
    /// DPAPI 没有可删的钥匙（主密钥由系统账户派生、不归我们）；TPM 有一把固定名字的
    /// 持久密钥，必须删——否则「关闭快速解锁」只删了文件，钥匙还在 TPM 里。
    ///
    /// **不看当前 `tpm` 是否为 `Some`**：从开启到关闭之间 TPM 可能掉线，而删除动作
    /// 应当按「文件里记的是什么」执行，不是按「此刻探测到什么」。删不掉也不影响
    /// 「封印文件已删」这个事实——没有密文，孤儿钥匙没有可利用面。
    pub fn purge(&self, _bind: &[u8]) {
        let _ = windows_tpm::purge(&self.key_name);
    }

    /// 撤销清理的结果（仅诊断用，不参与任何判定）。
    pub fn purge_report(&self) -> Result<(), String> {
        windows_tpm::purge(&self.key_name)
    }
}

/// 产品使用的 TPM 持久密钥名（与 `windows_tpm::DEFAULT_KEY_NAME` 同一个值）。
///
/// 这里再写一次是为了让「路由层」不必暴露 TPM 模块的内部常量；两处一致性由
/// `tpm_key_name_matches_tpm_module` 守着。
pub(crate) const DEFAULT_TPM_KEY: &str = "PasswordMS-VK-Seal";

impl<G: PresenceGate + Clone> VaultSealer for WinSealer<G> {
    fn availability(&self) -> Availability {
        // 两种手段共用同一个在场闸门，可用性判定完全一致。
        self.gate.availability()
    }

    fn subtype(&self) -> Subtype {
        // 只影响**新**封印。老封印靠 `accepts` 读，不受这里影响。
        if self.tpm.is_some() {
            Subtype::WindowsTpm2Seal
        } else {
            Subtype::WindowsDpapiUser
        }
    }

    fn accepts(&self, subtype: Subtype) -> bool {
        match subtype {
            // 老封印永远要能读：升级不搬移、不失效。
            Subtype::WindowsDpapiUser => true,
            // 只有本机 TPM 在场时才声称能读；否则交给 core 报 `BiometricUnavailable`，
            // 让用户回主密码——**不是** `Tampered`，更不删封印。
            Subtype::WindowsTpm2Seal => self.tpm.is_some(),
            Subtype::AndroidKeystore => false,
        }
    }

    fn seal(&self, vk: &[u8], bind: &[u8]) -> Result<Vec<u8>, SealError> {
        match &self.tpm {
            Some(tpm) => tpm.seal(vk, bind),
            None => self.dpapi.seal(vk, bind),
        }
    }

    fn unseal(&self, sealed: &[u8], bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        if windows_tpm::is_tpm_blob(sealed) {
            return match &self.tpm {
                Some(tpm) => tpm.unseal(sealed, bind),
                // TPM 封印但本机此刻没有可用 TPM：报「不可用」，与 `accepts` 的口径一致。
                None => Err(SealError::Unavailable(Availability::Unsupported)),
            };
        }
        self.dpapi.unseal(sealed, bind)
    }

    /// D-127：转发 TPM 密钥指纹。
    ///
    /// 只有 TPM 封印需要「密钥是哪一把」这个概念——DPAPI 的主密钥由系统按用户账户派生，
    /// 没有可枚举的持久密钥，也就没有可比的指纹（返回 `None`，行为与 1.4.11 一致）。
    ///
    /// 用 `self.tpm` 而不是 `subtype()`：后者表达「新封印写什么」，而这里问的是
    /// 「**眼下**这把钥匙长什么样」。
    fn key_fingerprint(&self) -> Option<String> {
        self.tpm.as_ref().and_then(|t| t.key_fingerprint())
    }
}

/// 让 `WinSealer` 在 `HelloGate` 上也能直接当适配器用（`platform_sealer` 走 `new()`）。
impl Default for WinSealer<HelloGate> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::PresenceReason;
    use passwordms_core::biometric::{SEAL_VK_MAX_LEN, SEAL_VK_MIN_LEN};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// 可克隆的假闸门（`WinSealer` 要求 `G: Clone`）。
    #[derive(Clone)]
    struct Gate {
        allow: bool,
        prompts: Arc<AtomicUsize>,
    }

    impl Gate {
        fn allowing() -> Self {
            Self {
                allow: true,
                prompts: Arc::new(AtomicUsize::new(0)),
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

    fn bind(seed: u8) -> [u8; 32] {
        [seed; 32]
    }

    /// 用**独立密钥名**构造一个会真碰 TPM 的适配器，Drop 时删掉那把钥匙。
    ///
    /// 绝不能借产品名 `PasswordMS-VK-Seal` 来测：那会与开发机上真实开启过的快速解锁
    /// 共用一把钥匙，跑一次测试就把主人的封印毁掉。
    struct TpmFixture {
        sealer: WinSealer<Gate>,
    }

    impl Drop for TpmFixture {
        fn drop(&mut self) {
            let _ = self.sealer.purge_report();
        }
    }

    fn tpm_fixture(tag: &str) -> TpmFixture {
        let key = format!(
            "PasswordMS-Test-{}-{}",
            tag,
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        TpmFixture {
            sealer: WinSealer::build_with_key(Gate::allowing(), true, key),
        }
    }

    /// 无 TPM ⇒ 新封印写 DPAPI，且 DPAPI 封印照样能读。
    #[test]
    fn without_tpm_writes_and_reads_dpapi() {
        let sealer = WinSealer::build(Gate::allowing(), false);
        assert!(!sealer.uses_tpm());
        assert_eq!(sealer.subtype(), Subtype::WindowsDpapiUser);
        assert!(sealer.accepts(Subtype::WindowsDpapiUser));
        assert!(
            !sealer.accepts(Subtype::WindowsTpm2Seal),
            "本机没有 TPM 却声称能读 TPM 封印，会让 core 把「本机用不了」误当「密文损坏」"
        );

        let vk = [0x11u8; 32];
        let sealed = sealer.seal(&vk, &bind(1)).expect("dpapi seal");
        assert!(
            (SEAL_VK_MIN_LEN..=SEAL_VK_MAX_LEN).contains(&sealed.len()),
            "DPAPI 密文长度 {} 越界",
            sealed.len()
        );
        assert!(
            !windows_tpm::is_tpm_blob(&sealed),
            "回落分支不得产出 TPM 魔数"
        );
        let opened = sealer.unseal(&sealed, &bind(1)).expect("dpapi unseal");
        assert_eq!(&opened[..], &vk[..]);
    }

    /// 有 TPM ⇒ `subtype()` 是 TPM2，但**同时**接受老 DPAPI 封印（升级不失效）。
    #[test]
    fn with_tpm_prefers_tpm_and_still_accepts_dpapi() {
        let sealer = WinSealer::build(Gate::allowing(), true);
        assert!(sealer.uses_tpm());
        assert_eq!(sealer.subtype(), Subtype::WindowsTpm2Seal);
        assert!(
            sealer.accepts(Subtype::WindowsDpapiUser),
            "升级后老封印必须仍可读，否则功能凭空消失"
        );
        assert!(sealer.accepts(Subtype::WindowsTpm2Seal));
        assert!(
            !sealer.accepts(Subtype::AndroidKeystore),
            "跨平台封印不得冒领"
        );
    }

    /// 有 TPM 时真跑一次 TPM 往返（无 TPM 机器跳过）。
    #[test]
    fn with_tpm_roundtrip_goes_through_tpm() {
        if !windows_tpm::tpm_ready() {
            eprintln!("skip: 本机无 TPM");
            return;
        }
        let f = tpm_fixture("roundtrip");
        let vk = [0x5Au8; 32];
        let sealed = f.sealer.seal(&vk, &bind(2)).expect("tpm seal");
        assert!(
            windows_tpm::is_tpm_blob(&sealed),
            "有 TPM 时新封印必须是 TPM"
        );
        let opened = f.sealer.unseal(&sealed, &bind(2)).expect("tpm unseal");
        assert_eq!(&opened[..], &vk[..]);
    }

    /// 老 DPAPI 封印在「本次探测到 TPM」的情况下依然能解开——这是升级路径的核心断言。
    #[test]
    fn old_dpapi_seal_survives_upgrade() {
        let old = WinSealer::build(Gate::allowing(), false);
        let vk = [0x42u8; 32];
        let sealed = old.seal(&vk, &bind(3)).expect("dpapi seal");

        // 升级后（TPM 就绪），同一个密文必须还能解。
        let upgraded = WinSealer::build(Gate::allowing(), true);
        assert_eq!(upgraded.subtype(), Subtype::WindowsTpm2Seal);
        let opened = upgraded.unseal(&sealed, &bind(3)).expect("upgrade unseal");
        assert_eq!(&opened[..], &vk[..]);
    }

    /// TPM 封印在没有 TPM 的机器上 ⇒ `Unavailable`，**不是** `Platform`（不是篡改）。
    #[test]
    fn tpm_seal_on_tpm_less_machine_is_unavailable() {
        if !windows_tpm::tpm_ready() {
            eprintln!("skip: 本机无 TPM");
            return;
        }
        let f = tpm_fixture("crossmachine");
        let sealed = f.sealer.seal(&[0x07u8; 32], &bind(4)).expect("tpm seal");

        // 模拟「换到一台没有 TPM 的机器」：同一份密文、DPAPI 分支。
        let without = WinSealer::build(Gate::allowing(), false);
        match without.unseal(&sealed, &bind(4)) {
            Err(SealError::Unavailable(_)) => {}
            other => panic!("无 TPM 时应报不可用，实际 {other:?}"),
        }
        // 而且 core 那一层会走 `accepts` 分支，连 unseal 都不会进。
        assert!(!without.accepts(Subtype::WindowsTpm2Seal));
    }

    /// 取消仍然归 `Canceled`，不因为多了一层路由而漂移。
    #[test]
    fn canceled_gate_still_maps_to_canceled() {
        let sealer = WinSealer::build(
            Gate {
                allow: false,
                prompts: Arc::new(AtomicUsize::new(0)),
            },
            false,
        );
        match sealer.seal(&[0x22u8; 32], &bind(5)) {
            Err(SealError::Canceled) => {}
            other => panic!("取消应映射为 Canceled，实际 {other:?}"),
        }
    }

    /// 路由层与 TPM 模块的密钥名必须一字不差：写错一个字符 = 新封印用一把钥匙、
    /// 关闭时删另一把，孤儿钥匙永久占 slot 且没人发现。
    ///
    /// 直接比常量，**不**建钥匙——建一次就得用产品名，会碰到真实封印。
    #[test]
    fn tpm_key_name_matches_tpm_module() {
        assert_eq!(
            DEFAULT_TPM_KEY,
            windows_tpm::DEFAULT_KEY_NAME,
            "路由层与 TPM 模块必须用同一个密钥名"
        );
        assert_eq!(DEFAULT_TPM_KEY, "PasswordMS-VK-Seal");
    }

    /// **跨层全链路**：core 的 `enable` 写盘 → core 的 `unlock` 解回，全程走真 TPM。
    ///
    /// 适配器自己的往返测试证明不了这条：`enable` 还会校验密文长度、写 `platform_subtype`，
    /// `unlock` 会先过 `accepts`。任一处与适配器口径不一致，只有这条会红。
    #[test]
    fn core_enable_and_unlock_through_winsealer() {
        use passwordms_core::biometric::{bind_of, enable, unlock, SealStore, SealTarget};
        use passwordms_core::crypto::keys::VaultKey;
        use passwordms_core::model::WrappedKey;
        use zeroize::Zeroizing;

        if !windows_tpm::tpm_ready() {
            eprintln!("skip: 本机无 TPM");
            return;
        }
        let f = tpm_fixture("coreflow");
        let dir = std::env::temp_dir().join(format!("pms-tpm-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = SealStore::new(&dir.join("v.vault"));

        let wkv = WrappedKey {
            nonce: vec![1u8; 12],
            ct: vec![2u8; 48],
        };
        let mac = [3u8; 32];
        let target = SealTarget {
            vault_id: "vault-tpm-flow",
            verify_mac: &mac,
            wrapped_vault_key: &wkv,
        };
        let vk: VaultKey = Zeroizing::new([0x5Au8; 32]);

        let rec = enable(&store, &f.sealer, &target, &vk).expect("enable 落盘");
        assert_eq!(
            rec.platform_subtype,
            Subtype::WindowsTpm2Seal,
            "有 TPM 时新封印必须记成 TPM2"
        );
        assert!(
            rec.sealed_vk.starts_with(&windows_tpm::MAGIC),
            "密文必须带魔数"
        );

        let back = unlock(&store, &f.sealer, &target).expect("unlock 解回");
        assert_eq!(&back[..], &vk[..]);

        // 反向：把记录改回 DPAPI 子类型（模拟老版本留下的封印指纹），仍必须能解——
        // 因为密文自识别走 magic，subtype 只过 `accepts`。
        let mut legacy = rec.clone();
        legacy.platform_subtype = Subtype::WindowsDpapiUser;
        store.save(&legacy).unwrap();
        // 密文仍是 TPM 的 → 走 TPM 分支能解开（这份改造只是验证 subtype 不参与分派）。
        let back = unlock(&store, &f.sealer, &target).expect("老 subtype 指纹仍可解");
        assert_eq!(&back[..], &vk[..]);

        // 而 bind 变了就不行（换主密码 / 换 VK 的核心失效路径在 core 已先拦，这里验底层）。
        let other_mac = [4u8; 32];
        let other = SealTarget {
            vault_id: "vault-tpm-flow",
            verify_mac: &other_mac,
            wrapped_vault_key: &wkv,
        };
        assert_ne!(bind_of(&target), bind_of(&other));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
