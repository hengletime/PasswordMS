//! D-152 回归：侧栏条目数必须「自己就能对上」。
//!
//! 主人诉求（2026-10-31）：三台设备都同步完、待同步归零后，**用户无法判断同步有没有
//! 丢数据**——界面上没有任何条目数。要在分组名后加组内条目数。
//!
//! 这件事的成败不在「能不能显示一个数字」，而在**数字之间能不能互相对上**。所以本文件
//! 钉的不是某个字段的值，而是三条不变量：
//!
//! ① `Σ 各组 item_count + ungrouped == total`（用户把侧栏数字加起来必须等于总数）；
//! ② `item_count` 只数**活跃**条目——把条目丢进回收站后组内数字跟着减，
//!    否则用户点进分组数不上数，读数反而制造新的怀疑；
//! ③ 读数**跨解锁不变**（写进容器而非内存假象）——同步核对通常发生在重启之后。
//!
//! 另外顺带钉住 `favorites` 的口径：它是 `total` 的**子集**，不是第四个数。
//! 若把它当作「收藏 + 各组 + 未分组」的一部分参与加总，恒等式立刻失衡。

use std::path::PathBuf;

use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::repository::ItemDraft;

const PWD: &str = "Count#Pw-152a";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-counts-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("vault.pms")
}

fn draft(title: &str, folder_id: Option<&str>, favorite: bool) -> ItemDraft {
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
        folder_id: folder_id.map(|s| s.to_string()),
        favorite,
    }
}

#[test]
fn counts_add_up_and_exclude_trash() {
    let path = temp_vault("sum");
    let mut session = Session::create(&path, PWD, true).unwrap();

    // 三个分组：A 三条、B 一条、C 空；未分组两条；其中两条标了收藏。
    let (fa, fb, _fc) = {
        let repo = session.repo_mut().unwrap();
        let a = repo
            .create_folder("A".into(), None, None)
            .unwrap()
            .folder_id;
        let b = repo
            .create_folder("B".into(), None, None)
            .unwrap()
            .folder_id;
        let c = repo
            .create_folder("C".into(), None, None)
            .unwrap()
            .folder_id;
        for t in ["a1", "a2", "a3"] {
            repo.create_item(draft(t, Some(&a), false)).unwrap();
        }
        repo.create_item(draft("b1", Some(&b), true)).unwrap();
        // 未分组的永远不落在任何分组里 —— 恒等式的另一半。
        repo.create_item(draft("u1", None, true)).unwrap();
        repo.create_item(draft("u2", None, false)).unwrap();
        (a, b, c)
    };

    {
        let repo = session.repo().unwrap();
        let counts = repo.item_counts();
        let usage = repo.folder_usage();

        assert_eq!(
            counts.total, 6,
            "活跃条目共 6 条（A 三条 + B 一条 + 未分组两条）"
        );
        assert_eq!(counts.ungrouped, 2, "未分组 2 条");
        assert_eq!(counts.favorites, 2, "收藏 2 条");

        let a = usage.get(&fa).map(|u| u.item_count).unwrap_or(0);
        let b = usage.get(&fb).map(|u| u.item_count).unwrap_or(0);
        assert_eq!((a, b), (3, 1));

        // ① 恒等式：各组 + 未分组 = 总数。**这条是本次改动真正的验收标准**。
        let sum_of_folders: usize = usage.values().map(|u| u.item_count).sum();
        assert_eq!(
            sum_of_folders + counts.ungrouped,
            counts.total,
            "Σ各组({sum_of_folders}) + 未分组({}) != 总数({})",
            counts.ungrouped,
            counts.total
        );

        // 收藏是 total 的子集，不进恒等式。
        assert!(counts.favorites <= counts.total);
    }

    // ② 回收站：`total` 与组内 `item_count` 必须同步减 1。
    {
        let repo = session.repo_mut().unwrap();
        let a1 = repo
            .list_items(Some(&fa), false, None)
            .first()
            .unwrap()
            .item_id
            .clone();
        repo.delete_item(&a1, false).unwrap();
    }
    {
        let repo = session.repo().unwrap();
        let counts = repo.item_counts();
        let a = repo
            .folder_usage()
            .get(&fa)
            .map(|u| u.item_count)
            .unwrap_or(0);
        assert_eq!(counts.total, 5, "回收站的条目不算活跃条目");
        assert_eq!(a, 2, "进回收站后组内数量必须跟着减");
        assert_eq!(counts.ungrouped, 2, "回收站条目原本在分组里，未分组数不变");
    }
}

#[test]
fn counts_survive_reopen() {
    let path = temp_vault("reopen");
    let folder_id = {
        let mut session = Session::create(&path, PWD, true).unwrap();
        let repo = session.repo_mut().unwrap();
        let f = repo
            .create_folder("F".into(), None, None)
            .unwrap()
            .folder_id;
        repo.create_item(draft("in", Some(&f), true)).unwrap();
        repo.create_item(draft("out", None, false)).unwrap();
        f
    };

    // ③ 重新解锁（= 从容器重建索引）：读数必须一模一样。
    let session = Session::unlock(&path, PWD).unwrap();
    let repo = session.repo().unwrap();
    let counts = repo.item_counts();
    assert_eq!(
        (counts.total, counts.favorites, counts.ungrouped),
        (2, 1, 1)
    );
    assert_eq!(
        repo.folder_usage()
            .get(&folder_id)
            .map(|u| u.item_count)
            .unwrap_or(0),
        1
    );
}

#[test]
fn empty_vault_reports_zero_not_none() {
    // 空库必须是 `0` 而不是「末知」：侧栏要能显示 0，而不是留白让用户猜。
    let path = temp_vault("empty");
    let session = Session::create(&path, PWD, true).unwrap();
    let counts = session.repo().unwrap().item_counts();
    assert_eq!(
        (counts.total, counts.favorites, counts.ungrouped),
        (0, 0, 0)
    );
}
