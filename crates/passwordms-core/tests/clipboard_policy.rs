//! D-135 守门：剪贴板 TTL 的「0」必须读成**不做自动清空**，而不是「0 秒后清空」。
//!
//! 缺陷现场（v1.4.16 及更早）：详情页复制用户名走 `copy_secret(text, 0)`，后端
//! 无条件 `sleep(0秒)` 后清理 → 写入与清空几乎同时发生 → toast 报「已复制」，
//! 用户粘贴得到空串。密码复制传 30 所以正常，于是表现为「只有用户名复制不了」。

use passwordms_core::clipboard_policy::{auto_clear_delay_secs, NO_AUTO_CLEAR_SECS};

#[test]
fn zero_ttl_means_no_auto_clear() {
    // 约定值本身钉死，防止有人把它改成 30 之类（那会让「不清空」变成「30 秒清」）。
    assert_eq!(NO_AUTO_CLEAR_SECS, 0);
    assert_eq!(
        auto_clear_delay_secs(NO_AUTO_CLEAR_SECS),
        None,
        "TTL=0 必须表示不安排清空；返回 Some(0) 就是「写完立刻清空」"
    );
}

#[test]
fn positive_ttl_still_schedules_clear() {
    // 负向对照：若这条挂了，说明改动把敏感文本的自动清空也关掉了——安全缺陷，
    // 比原 bug 更严重。所以两条必须成对存在（缺一就会「两边都绿但语义错」）。
    for ttl in [1u16, 5, 30, 60, 120] {
        assert_eq!(
            auto_clear_delay_secs(ttl),
            Some(u64::from(ttl)),
            "TTL={ttl} 应当安排清理"
        );
    }
}
