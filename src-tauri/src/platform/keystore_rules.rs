//! Android Keystore 适配器的**纯逻辑**（无平台依赖，任何机器可编译、可跑测试）。
//!
//! 为什么单独拆出来：`android_keystore.rs` 整个文件被 `cfg(target_os = "android")` 门控，
//! 里面的 `#[cfg(test)]` 在桌面永远跑不到。而「别名怎么派生」「可用性怎么映射」恰恰是
//! 最容易写错、又最该被测住的两处——把它们放到这里，`cargo test` 在 Windows 上就能验。

use passwordms_core::biometric::Availability;

/// 密钥别名：`bind` 全字节转小写 hex。
///
/// **不截断**。截断（比如只取前 8 字节）会让不同的库以极小概率压到同一别名上，
/// 而一旦撞上就是「两个库共用一把密钥」——这是不可接受的失效模式，不值得为省几十个字节冒险。
pub fn key_id(bind: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bind.len() * 2);
    for b in bind {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Kotlin 插件上报的可用性字符串 → [`Availability`]。
///
/// **未知取值一律 `Unsupported`**，绝不猜成 `NotConfigured`：后者会把用户赶去系统设置里
/// 找一个本来就不存在的开关（D-114 的教训在 Android 上的等价物）。
pub fn availability_from(status: &str) -> Availability {
    match status {
        "available" => Availability::Available,
        "not_configured" => Availability::NotConfigured,
        // D-125：只录了便捷级（Class 2，多半是 2D 人脸）。**不是** `NotConfigured`：
        // 那会让「人脸解锁明明能用」的用户读到「本机尚未设置指纹 / 人脸」。
        "weak_only" => Availability::WeakOnly,
        "not_present" => Availability::NotPresent,
        "disabled_by_policy" => Availability::DisabledByPolicy,
        "busy" => Availability::Busy,
        _ => Availability::Unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_id_is_full_length_and_stable() {
        let bind = [0xABu8; 32];
        let a = key_id(&bind);
        assert_eq!(a.len(), 64, "32 字节必须转成 64 个 hex 字符");
        assert_eq!(a, key_id(&bind), "同一 bind 必须得到同一别名");
        assert!(a.starts_with("abab"));
    }

    #[test]
    fn different_bind_never_collides() {
        let a = [0u8; 32];
        let mut b = [0u8; 32];
        b[31] = 1;
        assert_ne!(key_id(&a), key_id(&b), "只差最后一个字节也必须不同");
    }

    #[test]
    fn empty_bind_yields_empty_id() {
        // 空 bind 不该出现（core 的 bind_of 恒 32 字节），但也不许 panic。
        assert_eq!(key_id(&[]), "");
    }

    #[test]
    fn availability_mapping_never_guesses_not_configured() {
        assert_eq!(availability_from("available"), Availability::Available);
        assert_eq!(
            availability_from("not_configured"),
            Availability::NotConfigured
        );
        assert_eq!(availability_from("not_present"), Availability::NotPresent);
        assert_eq!(availability_from("busy"), Availability::Busy);
        assert_eq!(
            availability_from("disabled_by_policy"),
            Availability::DisabledByPolicy
        );
        // 未知取值必须落到 Unsupported。
        assert_eq!(
            availability_from("some_future_value"),
            Availability::Unsupported
        );
        assert_eq!(availability_from(""), Availability::Unsupported);
    }

    #[test]
    fn weak_only_is_never_folded_into_not_configured() {
        // D-125：只录了便捷级人脸时，Kotlin 上报 `weak_only`。若映射回 NotConfigured，
        // 界面就会对一个人脸解锁用得好好的用户说「本机尚未设置指纹 / 人脸」。
        assert_eq!(availability_from("weak_only"), Availability::WeakOnly);
        assert_ne!(availability_from("weak_only"), Availability::NotConfigured);
    }

    #[test]
    fn not_implemented_is_not_produced_by_android() {
        // `NotImplemented` 的语义是「本平台没有适配器」，Android 有适配器，
        // 所以任何输入都不该映射到它——否则前端会把整张卡片藏起来。
        for s in [
            "available",
            "not_configured",
            "weak_only",
            "not_present",
            "busy",
            "unsupported",
            "",
        ] {
            assert_ne!(availability_from(s), Availability::NotImplemented);
        }
    }
}
