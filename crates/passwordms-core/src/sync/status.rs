//! 同步状态读数（D-151）。
//!
//! 背景：`sync_status` 的「待同步变更」按 D-56 的口径取**最落后对端**的数量
//! （多设备时，只要有一台还没拿到，这个数就该是它）。口径本身没错，但落到真实数据上
//! 会变成一个**没有主语**的数字：主人电脑上有 4 条配对记录（2 台手机 + 重装/重配留下的
//! 残影），残影的 `sync_state` 是空的 —— `export_changes(空状态)` 恒等于全库条数，
//! 于是「待同步」永远停在 58，同步多少次都不动，而用户完全看不出这 58 是谁欠的。
//!
//! 本模块只做一件事：把「最落后」**归因到具体设备**，并顺带说明它是否从未同步过。
//! 数字不变、口径不变（不隐藏任何待发数据 —— 宁可显示多，不可显示少），
//! 只是让用户能判断「这是台还活着但落后的设备」还是「这是条历史残影，删掉就好」。

use crate::store::repository::Repository;

/// 待同步摘要。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingSummary {
    /// 待发变更数 = 最落后对端的数量（无配对设备时为 0）。
    pub count: usize,
    /// 承担这个数字的设备（`None` = 没有配对设备）。并列时按 `device_id` 取字典序最小者，
    /// 保证同一份数据每次算出来的归因**稳定**（`list_paired_devices` 走 HashMap，顺序不定）。
    pub worst_device_id: Option<String>,
    /// 该设备的展示名（用户用它去设备页找到那一行）。
    pub worst_device_name: Option<String>,
    /// 该设备是否**从未同步过**。`count > 0 && 从未同步` 时，这个数字不代表「同步失败」，
    /// 而代表「这台设备一条都没拿到」——残影设备的典型特征。
    pub worst_never_synced: bool,
}

/// 计算待同步摘要。
///
/// 纯读取，不改任何状态；`export_changes` 本身也是只读（O1 水位/base 过滤）。
pub fn pending_summary(repo: &Repository) -> PendingSummary {
    let mut out = PendingSummary::default();
    let mut best: Option<(usize, String, String, bool)> = None;
    for device in repo.list_paired_devices() {
        let count = repo
            .export_changes(&repo.sync_state(&device.device_id))
            .len();
        let never = device.last_sync_at.is_none();
        let take = match &best {
            None => true,
            Some((best_count, best_id, _, _)) => {
                // 并列时取 device_id 字典序最小 → 归因稳定（HashMap 遍历顺序不定）。
                count > *best_count || (count == *best_count && device.device_id < *best_id)
            }
        };
        if take {
            best = Some((
                count,
                device.device_id.clone(),
                device.device_name.clone(),
                never,
            ));
        }
    }
    if let Some((count, id, name, never)) = best {
        out.count = count;
        out.worst_device_id = Some(id);
        out.worst_device_name = Some(name);
        out.worst_never_synced = never;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::item::{ItemKind, LoginData};
    use crate::model::sync_state::PairedDevice;
    use crate::model::vault::WrappedKey;
    use crate::session::Session;
    use crate::store::repository::ItemDraft;

    const PWD: &str = "Pending#Pw1";

    fn draft(title: &str) -> ItemDraft {
        ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: title.to_string(),
                url: None,
                username: None,
                password: None,
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        }
    }

    fn device(id: &str, name: &str, last_sync_at: Option<i64>) -> PairedDevice {
        PairedDevice {
            device_id: id.to_string(),
            device_name: name.to_string(),
            fingerprint: String::new(),
            sync_key_wrapped: WrappedKey {
                nonce: vec![0u8; 12],
                ct: vec![0u8; 32],
            },
            paired_at: 1,
            last_sync_at,
            peer_ip: None,
            peer_port: None,
        }
    }

    #[test]
    fn empty_when_no_paired_device() {
        let dir = std::env::temp_dir().join(format!("pms-pending-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session::create(&dir.join("v.vault"), PWD, true).unwrap();
        s.repo_mut().unwrap().create_item(draft("a")).unwrap();
        let repo = s.repo().unwrap();
        let sum = pending_summary(repo);
        assert_eq!(sum.count, 0, "无配对设备时无可同步对象");
        assert!(sum.worst_device_id.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 回归主人现场：一台已同步（水位覆盖全库）+ 一台从未同步的残影。
    /// 修复前：界面只有一个「58」，看不出是谁欠的；修复后：数字仍是 58（不隐藏），
    /// 但归因到那台**从未同步**的设备。
    #[test]
    fn attributes_stuck_pending_to_never_synced_ghost() {
        let dir = std::env::temp_dir().join(format!("pms-pending-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session::create(&dir.join("v.vault"), PWD, true).unwrap();
        let live_id = uuid::Uuid::new_v4().to_string();
        let ghost_id = uuid::Uuid::new_v4().to_string();
        {
            let repo = s.repo_mut().unwrap();
            for i in 0..3 {
                repo.create_item(draft(&format!("i{i}"))).unwrap();
            }
            repo.upsert_paired_device(device(&live_id, "手机A", Some(1000)))
                .unwrap();
            repo.upsert_paired_device(device(&ghost_id, "手机A-旧", None))
                .unwrap();
            // 活设备已同步完毕：水位覆盖全部条目。
            let mut st = repo.sync_state(&live_id);
            for d in repo.list_items(None, false, None) {
                st.sent_watermark.insert(d.item_id.clone(), u64::MAX);
            }
            repo.save_sync_state(st).unwrap();
        }
        let repo = s.repo().unwrap();
        let sum = pending_summary(repo);
        assert_eq!(
            sum.count, 3,
            "残影设备的待发 = 全库条数（口径不变，不隐藏）"
        );
        assert_eq!(sum.worst_device_id.as_deref(), Some(ghost_id.as_str()));
        assert_eq!(sum.worst_device_name.as_deref(), Some("手机A-旧"));
        assert!(sum.worst_never_synced, "必须标出「从未同步过」");
        let live = repo.export_changes(&repo.sync_state(&live_id)).len();
        assert_eq!(live, 0, "已同步设备的待发应为 0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tie_break_is_stable() {
        let dir = std::env::temp_dir().join(format!("pms-pending-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session::create(&dir.join("v.vault"), PWD, true).unwrap();
        let a = "00000000-0000-4000-8000-000000000001".to_string();
        let b = "00000000-0000-4000-8000-000000000002".to_string();
        {
            let repo = s.repo_mut().unwrap();
            repo.create_item(draft("x")).unwrap();
            repo.upsert_paired_device(device(&b, "B", None)).unwrap();
            repo.upsert_paired_device(device(&a, "A", None)).unwrap();
        }
        let repo = s.repo().unwrap();
        let sum = pending_summary(repo);
        assert_eq!(
            sum.worst_device_id.as_deref(),
            Some(a.as_str()),
            "并列时必须取字典序最小者，否则同一份数据每次归因都不同"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
