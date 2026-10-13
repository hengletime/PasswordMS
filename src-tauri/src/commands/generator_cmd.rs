//! 生成器 / TOTP 命令（架构文档 §5.2、F-07、F-11）。
//!
//! 这两个能力不依赖解锁状态（生成器为纯随机、TOTP 只读密钥），故不需要 `State`。

use passwordms_core::generator;
use passwordms_core::totp;

use crate::dto::{GeneratedPasswordDto, GeneratorOptionsDto, TotpDto};
use crate::error::IpcResult;

/// F-07 生成密码。
#[tauri::command(rename_all = "snake_case")]
pub fn generate_password(opts: GeneratorOptionsDto) -> IpcResult<GeneratedPasswordDto> {
    Ok(GeneratedPasswordDto {
        password: generator::generate(&opts)?,
    })
}

/// F-11 当前 TOTP 验证码。
#[tauri::command(rename_all = "snake_case")]
pub fn totp_now(secret: String) -> IpcResult<TotpDto> {
    Ok(totp::now(&secret)?)
}
