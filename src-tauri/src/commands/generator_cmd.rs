//! 生成器 / TOTP 命令（架构文档 §5.2、F-07、F-11）。
//!
//! 这两个能力不依赖解锁状态（生成器为纯随机、TOTP 只读密钥），故不需要会话锁；
//! 但它们是**用户活动**——D-108（3.4-6）auto-lock 兜底以 IPC 活动为闲置基准，
//! 故入口处显式 `touch_activity`。

use passwordms_core::generator;
use passwordms_core::totp;

use crate::dto::{GeneratedPasswordDto, GeneratorOptionsDto, TotpDto};
use crate::error::IpcResult;

/// F-07 生成密码。
#[tauri::command(rename_all = "snake_case")]
pub fn generate_password(
    app: tauri::AppHandle,
    opts: GeneratorOptionsDto,
) -> IpcResult<GeneratedPasswordDto> {
    touch_activity(&app);
    Ok(GeneratedPasswordDto {
        password: generator::generate(&opts)?,
    })
}

/// F-11 当前 TOTP 验证码。
#[tauri::command(rename_all = "snake_case")]
pub fn totp_now(app: tauri::AppHandle, secret: String) -> IpcResult<TotpDto> {
    touch_activity(&app);
    Ok(totp::now(&secret)?)
}

/// 用户活动触点（轮询详情页的 TOTP 每秒刷新也会走到这里——这正是期望行为：
/// 用户盯着验证码页面时不算闲置）。
fn touch_activity(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<crate::state::AppState>() {
        state.touch_activity();
    }
}
