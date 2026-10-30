//! D-136 守门：同步端口**选取区间**判定（只用于诊断标注，不得用于连接决策）。
//!
//! 背景是真机日志：配对记录写着 `192.168.1.102:46879`，而本程序选端口用的是
//! `[49152, 65535]`（项目第一个提交起就是这两个值），46879 落在区间外 —— 这条记录
//! 几乎一定是历史残留，值得在日志里点名。
//!
//! **但这个判定绝不能用来「跳过连接」**：它判的是「本程序会把监听端口选在哪」，
//! 不是「合法端口必须落在哪」。实测本机动态端口范围被 WSL2/Hyper-V 改成了
//! 1024–65535，`bind(0)` 分到 29581 —— 区间外、但完全可连。曾把该判定接进连接流程，
//! 既有 `attachment_sync` 三例立刻全红（`connect|io:entity not found`），正是假阴性。
//!
//! 所以本文件守的是「诊断语义 + 边界」，并**显式钉住那条禁令的反例**。

use passwordms_core::constants::{is_sync_port, DISCOVERY_PORT, SYNC_PORT_MAX, SYNC_PORT_MIN};

#[test]
fn recorded_port_outside_range_is_flagged() {
    // 真机实测的历史残留值：写死它，防止有人「顺手放宽」区间时这条回归被悄悄绕过。
    assert!(!is_sync_port(46879), "46879 是实测到的区间外记录值");
    // 下界紧邻的两个点，成对断言等价于钉住 `>=` 而不是 `>`。
    assert!(!is_sync_port(SYNC_PORT_MIN - 1), "49151 在区间外");
    assert!(is_sync_port(SYNC_PORT_MIN), "49152 是区间下界");
    // 发现口刻意避开同步区间，它绝不该被当成同步端口。
    assert!(
        !is_sync_port(DISCOVERY_PORT),
        "47823 是 UDP 发现口，不是同步端口"
    );
    // 0 与常见服务端口。
    assert!(!is_sync_port(0));
    assert!(!is_sync_port(443));
}

#[test]
fn range_bounds_are_inclusive() {
    assert!(is_sync_port(SYNC_PORT_MAX), "65535 是区间上界");
    assert!(is_sync_port(SYNC_PORT_MIN));
    // 区间中点抽一个，防「只对上界/下界特判」式实现蒙过测试。
    assert!(is_sync_port(57344));
}

#[test]
fn outside_range_does_not_mean_unreachable() {
    // 这条不是测函数，是**钉住一条设计禁令**：区间外 ≠ 连不上。
    // 本机 `bind(0)` 实测分到 29581（WSL2/Hyper-V 把动态端口范围改成 1024-65535），
    // 它是真实可连的监听端口，但 `is_sync_port` 会判否。
    // 若将来有人又把本函数接进连接决策，这条用例就是反驳证据。
    assert!(
        !is_sync_port(29581),
        "29581 在区间外——正因如此该判定不能用于连接决策"
    );
}
