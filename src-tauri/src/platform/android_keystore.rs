//! Android Keystore 适配器（`docs/08-biometric-unlock.md` §23）。
//!
//! 本文件是**薄壳**：真正的 Keystore 与 BiometricPrompt 调用在 Kotlin 插件
//! `BiometricPlugin.kt` 里（`src-tauri/gen/android/app/src/main/java/com/passwordms/app/`）。
//! Rust 侧只做三件事：算密钥别名、base64 过线、把 Kotlin 的错误码翻译成 [`SealError`]。
//!
//! ## 为什么别名从 `bind` 派生，而不是传 `vault_id`
//!
//! [`VaultSealer::seal`] 的入参只有 `vk` 和 `bind`，拿不到 `vault_id`。给 trait 加参数会影响
//! core 的公共 API 与全部测试；而 `bind` 本身就是
//! `SHA-256("passwordms/v1/biometric" ‖ 0x00 ‖ vault_id ‖ verify_mac ‖ nonce ‖ ct)`——
//! **换库、换主密码、换 VK 都会得到不同的 bind**，用它派生别名天然等价于「每库一把密钥」，
//! 且不需要额外传参。撤销时用同一个 `bind` 重算别名即可删掉。
//!
//! ## 错误码翻译表（Kotlin → core）
//!
//! | Kotlin `code` | [`SealError`] | 为什么 |
//! |---|---|---|
//! | `canceled` | `Canceled` | 用户按了取消 / 返回键，**不是**失效 |
//! | `invalidated` | `Invalidated` | 新增指纹 / PIN 重置导致密钥永久失效（§21.3 的那个信号） |
//! | `tampered` | `Platform` | GCM tag 不符 ⇒ core 归为篡改（`bind` 变了或数据被改） |
//! | 其它 | `Platform` | 平台故障，保留原文便于排查 |

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use passwordms_core::biometric::{Availability, SealError, Subtype, VaultSealer};
use serde::{Deserialize, Serialize};
use tauri::plugin::mobile::PluginInvokeError;
use tauri::plugin::PluginHandle;
use tauri::Runtime;
use zeroize::Zeroizing;

// 纯逻辑（别名派生 / 可用性映射）放在平台无关模块里，桌面 `cargo test` 才跑得到。
use super::keystore_rules::{availability_from, key_id};

#[derive(Serialize)]
struct KeyArgs<'a> {
    key_id: &'a str,
}

#[derive(Serialize)]
struct SealArgs<'a> {
    key_id: &'a str,
    /// VK 的 base64。明文不可避免要过线（Kotlin 侧得拿到明文才能加密）——
    /// 用 `Zeroizing` 包住中间 String，drop 时清零，别在堆上留一地副本。
    data: &'a str,
    bind: &'a str,
}

#[derive(Serialize)]
struct UnsealArgs<'a> {
    key_id: &'a str,
    sealed: &'a str,
    bind: &'a str,
}

#[derive(Deserialize)]
struct SealResp {
    sealed: String,
}

#[derive(Deserialize)]
struct UnsealResp {
    data: String,
}

#[derive(Deserialize)]
struct AvailabilityResp {
    status: String,
}

/// Android 适配器。持有 Kotlin 插件的 [`PluginHandle`]（由 `lib.rs` 注册时拿到）。
pub struct KeystoreSealer<R: Runtime> {
    handle: PluginHandle<R>,
}

impl<R: Runtime> KeystoreSealer<R> {
    pub fn new(handle: PluginHandle<R>) -> Self {
        Self { handle }
    }

    /// 删掉本库对应的 Keystore 密钥（撤销的实质动作）。
    ///
    /// 幂等：别名不存在也算成功。**失败不上报**——文件都删了，留一句报错只会让用户困惑；
    /// 孤儿密钥拿不到密文，没有可利用面。
    pub fn purge_key(&self, bind: &[u8]) {
        let kid = key_id(bind);
        let _ = self
            .handle
            .run_mobile_plugin::<serde_json::Value>("remove", KeyArgs { key_id: &kid });
    }
}

impl<R: Runtime> VaultSealer for KeystoreSealer<R> {
    fn availability(&self) -> Availability {
        // 传 `{}` 而不是 `()`：Kotlin 侧 `invoke.getArgs()` 会 `JSObject(argsJson)`，
        // 拿到 `null` 会直接抛 JSONException。
        match self
            .handle
            .run_mobile_plugin::<AvailabilityResp>("availability", serde_json::json!({}))
        {
            Ok(r) => availability_from(&r.status),
            // 插件调不通 ⇒ 本平台此刻做不了（`Unsupported`），**不能**报「未配置」：
            // 那会把用户赶去系统设置里找一个本来就不存在开关。
            Err(_) => Availability::Unsupported,
        }
    }

    fn subtype(&self) -> Subtype {
        Subtype::AndroidKeystore
    }

    fn seal(&self, vk: &[u8], bind: &[u8]) -> Result<Vec<u8>, SealError> {
        // 与 Windows 侧同样的前置检查：不可用时不产出密文（否则等于开了条免密通道）。
        let availability = self.availability();
        if availability != Availability::Available {
            return Err(SealError::Unavailable(availability));
        }

        let kid = key_id(bind);
        let data = Zeroizing::new(STANDARD.encode(vk));
        let bind_b64 = STANDARD.encode(bind);
        let resp: SealResp = self
            .handle
            .run_mobile_plugin(
                "seal",
                SealArgs {
                    key_id: &kid,
                    data: &data,
                    bind: &bind_b64,
                },
            )
            .map_err(map_err)?;

        STANDARD
            .decode(resp.sealed.as_bytes())
            .map_err(|e| SealError::Platform(format!("sealed base64: {e}")))
    }

    fn unseal(&self, sealed: &[u8], bind: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        let availability = self.availability();
        if availability != Availability::Available {
            return Err(SealError::Unavailable(availability));
        }

        let kid = key_id(bind);
        let sealed_b64 = Zeroizing::new(STANDARD.encode(sealed));
        let bind_b64 = STANDARD.encode(bind);
        let resp: UnsealResp = self
            .handle
            .run_mobile_plugin(
                "unseal",
                UnsealArgs {
                    key_id: &kid,
                    sealed: &sealed_b64,
                    bind: &bind_b64,
                },
            )
            .map_err(map_err)?;

        let raw = STANDARD
            .decode(resp.data.as_bytes())
            .map_err(|e| SealError::Platform(format!("vk base64: {e}")))?;
        Ok(Zeroizing::new(raw))
    }
}

fn map_err(e: PluginInvokeError) -> SealError {
    let code = match &e {
        PluginInvokeError::InvokeRejected(r) => r.code.clone().unwrap_or_default(),
        _ => String::new(),
    };
    match code.as_str() {
        "canceled" => SealError::Canceled,
        "invalidated" => SealError::Invalidated,
        // tag 不符：core 会把 `Platform` 归为篡改（§10），保留原文便于排查。
        "tampered" => SealError::Platform(format!("aead tag mismatch: {e}")),
        _ => SealError::Platform(e.to_string()),
    }
}
