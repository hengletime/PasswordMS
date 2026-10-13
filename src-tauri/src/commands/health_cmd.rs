//! 密码体检命令（架构文档 §5.2、F-16）。

use passwordms_core::health;
use passwordms_core::model::now_ms;
use passwordms_core::model::vault::HealthConfig;

use crate::dto::{HealthConfigDto, HealthReportDto};
use crate::error::{lock_err, IpcResult};
use crate::state::AppState;

/// F-16 执行体检（可选传入配置；缺省使用库内设置）。
#[tauri::command(rename_all = "snake_case")]
pub fn health_check(
    state: tauri::State<'_, AppState>,
    config: Option<HealthConfigDto>,
) -> IpcResult<HealthReportDto> {
    let session = state.session.lock().map_err(lock_err)?;
    let repo = session.repo()?;
    let cfg: HealthConfig = config.unwrap_or_else(|| repo.settings().health);
    let items = repo.list_items(None, false, None);
    Ok(health::check(&items, &cfg, now_ms()))
}
