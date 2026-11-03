//! D-174 回归：分组被删之后，**不许有任何条目继续指向它**。
//!
//! 主人 2026-11-03 验收 1.5.29 时第一个问题：
//!
//! > 奇怪地发现有一个「未知分组」，不是「未分组」。
//!
//! 「未知分组」本身是 D-154 故意定的第三态（不许冒充「未分组」，因为侧栏「未分组 N 条」
//! 里并不包含它）。但**一个正常使用流程能造出来的第三态，说明数据脏了**，不是设计对。
//!
//! 脏的来源只有一条：删分组壳的时候没有通知挂在它下面的条目。
//!
//! - 本地删分组走 `delete_folder`，条目会被逐条迁出（ToUngrouped / ToTrash）—— 干净；
//! - **同步收到「对端删分组」走 `remove_folder_direct`，只 tombstone + 删壳** ——
//!   条目原样留着已不存在的 `folder_id`。
//!
//! 而这条路径恰恰最容易留下永久脏数据：对端「条目移出分组」的 live 变更 rev 不一定
//! 压过本端（本端可能刚编辑过那条），LWW 判负之后本端条目就永久悬空；对端已经认为它
//! 在未分组里，**不会再发第二次**，于是这份不一致会一直留着。
//!
//! 三条断言互相独立，对应三层修法：
//!
//! 1. 源头（`remove_folder_direct`）：删壳的同一次操作里把条目移出；
//! 2. 收口（`apply_changes` 尾部 + `open_with_container`）：整批应用完 / 开库时扫一遍；
//! 3. 恒等式（D-153）：修完之后 `list_ungrouped_items().len() == item_counts().ungrouped`
//!    —— 侧栏读数与点进去的条数必须相等，否则用户会把差额读成「数据丢了」。

use std::collections::HashMap;
use std::path::PathBuf;

use passwordms_core::constants::RECORD_TYPE_ITEM;
use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::sync_state::DeviceSyncState;
use passwordms_core::session::Session;
use passwordms_core::store::repository::{FolderDeleteStrategy, ItemDraft};
use passwordms_core::sync::merge;

const PWD: &str = "Dangling#Pw-174";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-dangling-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("vault.pms")
}

fn draft(title: &str, folder_id: Option<&str>) -> ItemDraft {
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
        favorite: false,
    }
}

fn title_of(item: &Item) -> String {
    match &item.kind {
        ItemKind::Login(l) => l.title.clone(),
        _ => String::new(),
    }
}

fn empty_sync_state() -> DeviceSyncState {
    DeviceSyncState {
        device_id: "peer-test".to_string(),
        base: HashMap::new(),
        last_recv_seq: 0,
        last_sent_seq: 0,
        sent_watermark: HashMap::new(),
    }
}

/// 造「分组 F 下 2 条 + 未分组 1 条」，返回 F 的 id。
fn seed(session: &mut Session) -> String {
    let repo = session.repo_mut().unwrap();
    let f = repo
        .create_folder("工作".into(), None, None)
        .unwrap()
        .folder_id;
    repo.create_item(draft("in-1", Some(&f))).unwrap();
    repo.create_item(draft("in-2", Some(&f))).unwrap();
    repo.create_item(draft("free-1", None)).unwrap();
    f
}

#[test]
fn 删壳本身不许动条目_收口交给批末扫描() {
    // 这条用例钉的是一个**反直觉的**设计决定：删壳时**不要**顺手把条目迁出。
    //
    // 听起来"顺手迁出"更彻底，实测会制造假冲突：条目 `rev` 一推进，对端同一批发来的
    // 「删除该条目」墓碑（`tombstone()` 只翻标志位、沿用删除时的 rev）就会被读成
    // 「本端改过、对端删了」→ 进冲突箱、条目留在活跃表。
    // `sync_deletion.rs::case4` 就是这么红的（分组 ToTrash 删除时，级联的条目该进
    // 回收站，结果停在活跃表）。所以这里断言的是**不动**：
    let path = temp_vault("direct");
    let mut session = Session::create(&path, PWD, true).unwrap();
    let folder_id = seed(&mut session);

    let repo = session.repo_mut().unwrap();
    let before: Vec<_> = repo
        .list_items(None, false, None)
        .into_iter()
        .filter(|i| i.folder_id.as_deref() == Some(folder_id.as_str()))
        .map(|i| (i.item_id, i.rev))
        .collect();
    assert_eq!(before.len(), 2, "夹具应有两条目在这组里");

    repo.remove_folder_direct(&folder_id).unwrap();

    for (item_id, rev_before) in &before {
        let item = repo.get_item(item_id).unwrap();
        assert_eq!(
            item.rev, *rev_before,
            "删壳不许推进条目 rev —— 一批变更共享同一批 rev 基准，             中途抬任何一条都会让同批里针对它的删除判定失真"
        );
        assert!(
            item.folder_id.is_some(),
            "此刻条目仍挂着已删分组的引用（这是设计的中间态，由批末收口处理）"
        );
    }

    // 批末收口：一条命令把整库的悬空引用清干净。
    let fixed = repo.repair_dangling_folder_refs().unwrap();
    assert_eq!(fixed, 2, "两条悬空引用都该被修");
    for (item_id, rev_before) in &before {
        let item = repo.get_item(item_id).unwrap();
        assert!(item.folder_id.is_none(), "收口之后必须落到未分组");
        assert_eq!(
            item.rev,
            rev_before + 1,
            "收口要推进 rev（否则修正发不出去）"
        );
    }
    // 删分组不该连带删条目。
    assert_eq!(repo.list_items(None, false, None).len(), 3);
    // D-153 恒等式：侧栏读数 == 未分组视图里的条数。
    let counts = repo.item_counts();
    assert_eq!(
        repo.list_ungrouped_items(false, None).len(),
        counts.ungrouped,
        "修完之后「未分组 N 条」必须与点进去看到的条数相等"
    );
    assert_eq!(counts.ungrouped, 3, "原有 1 条 + 移出 2 条");
}

#[test]
fn 整批同步应用完之后不许留下悬空引用() {
    let path = temp_vault("merge");
    // 先造好一台「本机」，落盘关闭。
    let folder_id = {
        let mut session = Session::create(&path, PWD, true).unwrap();
        seed(&mut session)
    };
    // 对端 = 同一份库的副本：同一把 VK（否则收不到 / 解不开），只是少一条条目。
    let peer_path = path.with_file_name("peer.pms");
    std::fs::copy(&path, &peer_path).unwrap();

    // 本机加一条「指向根本不存在的分组」的条目 —— 对端那边这条的来源不重要，
    // 重要的是它落地时分组不在，这就是悬空引用的典型形态。
    //
    // **必须同一个会话里「造脏 + 导出」**：中间只要重开一次库，开库自愈就会先把它
    // 修干净，这条用例就变成在测另一层（2026-11-03 变异检验时就是这样被糊过去的）。
    let (ghost, batch) = {
        let mut session = Session::unlock(&path, PWD).unwrap();
        let ghost = {
            let repo = session.repo_mut().unwrap();
            let mut item = repo.list_items(None, false, None).remove(0);
            item.item_id = uuid::Uuid::new_v4().to_string();
            item.folder_id = Some("never-existed".to_string());
            item.rev = 1;
            item.updated_at += 1; // 比副本里的旧版本新，LWW 才会采用
            repo.put_item_direct(item.clone()).unwrap();
            item
        };
        let batch: Vec<_> = session
            .repo()
            .unwrap()
            .export_changes(&empty_sync_state())
            .into_iter()
            .filter(|c| c.record_type == RECORD_TYPE_ITEM)
            .collect();
        (ghost, batch)
    };
    assert!(
        batch.iter().any(|c| c.record_id == ghost.item_id),
        "批次里必须包含那条悬空条目，否则本用例没测到东西"
    );

    let mut peer = Session::unlock(&peer_path, PWD).unwrap();
    assert!(
        peer.repo().unwrap().get_item_opt(&ghost.item_id).is_none(),
        "对端此刻不该有这条，否则走的是「已存在」分支，测不到落库路径"
    );
    let report = {
        let repo = peer.repo_mut().unwrap();
        let mut state = empty_sync_state();
        merge::apply_changes(repo, &mut state, &batch).unwrap()
    };
    assert!(report.applied > 0, "批次应被应用，而不是全部跳过");

    let repo = peer.repo().unwrap();
    let landed = repo.get_item(&ghost.item_id).unwrap();
    assert!(
        landed.folder_id.is_none(),
        "merge 收口之后仍留着悬空引用：{:?}",
        landed.folder_id
    );
    assert!(
        landed.rev > ghost.rev,
        "rev 必须推进（{} 未超过 {}）——否则这份修正永远发不出去，\
         而且对端那份「同 rev 异内容」每轮都会再报一次冲突",
        landed.rev,
        ghost.rev
    );
    assert_eq!(
        repo.list_ungrouped_items(false, None).len(),
        repo.item_counts().ungrouped
    );
    // 正常归属的条目一条都不许被顺手改掉。
    let still_grouped = repo
        .list_items(None, false, None)
        .into_iter()
        .filter(|i| i.folder_id.as_deref() == Some(folder_id.as_str()))
        .count();
    assert_eq!(still_grouped, 2);
}

#[test]
fn 打开库时自动修复存量悬空引用() {
    let path = temp_vault("open");
    let folder_id;
    let ghost_id;
    {
        let mut session = Session::create(&path, PWD, true).unwrap();
        folder_id = seed(&mut session);
        let repo = session.repo_mut().unwrap();
        let mut item = repo.list_items(None, false, None).remove(0);
        item.item_id = uuid::Uuid::new_v4().to_string();
        item.folder_id = Some("deleted-on-old-version".to_string());
        item.rev = 3;
        ghost_id = item.item_id.clone();
        repo.put_item_direct(item).unwrap();
    }
    // 重新打开：旧版本留下的脏库必须是「一打开就干净」，而不是等用户手改。
    let session = Session::unlock(&path, PWD).unwrap();
    let repo = session.repo().unwrap();
    let fixed = repo.get_item(&ghost_id).unwrap();
    assert!(fixed.folder_id.is_none(), "开库自愈没生效");
    assert_eq!(fixed.rev, 4, "自愈要推进 rev（3 → 4），否则修正发不出去");
    // 只有那条被自愈，别人家的分组引用一个不许动。
    let still_grouped = repo
        .list_items(None, false, None)
        .into_iter()
        .filter(|i| i.folder_id.as_deref() == Some(folder_id.as_str()))
        .count();
    assert_eq!(still_grouped, 2, "修复不能误伤正常归属");
    assert_eq!(
        repo.list_ungrouped_items(false, None).len(),
        repo.item_counts().ungrouped
    );
}

#[test]
fn 没有悬空引用时修复是零写的() {
    let path = temp_vault("noop");
    let mut session = Session::create(&path, PWD, true).unwrap();
    seed(&mut session);
    let repo = session.repo_mut().unwrap();
    let size_before = std::fs::metadata(&path).unwrap().len();
    let revs_before: Vec<u64> = repo
        .list_items(None, false, None)
        .iter()
        .map(|i| i.rev)
        .collect();

    // 每次开库都会调它，正常情况下必须是零成本：不写帧、不动 rev。
    assert_eq!(repo.repair_dangling_folder_refs().unwrap(), 0);
    assert_eq!(repo.repair_dangling_folder_refs().unwrap(), 0);

    let revs_after: Vec<u64> = repo
        .list_items(None, false, None)
        .iter()
        .map(|i| i.rev)
        .collect();
    assert_eq!(revs_before, revs_after, "干净的库不该被改动");
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        size_before,
        "干净的库不该多出任何一帧"
    );
}

#[test]
fn 本地删分组两种策略都不会留悬空引用() {
    for strategy in [
        FolderDeleteStrategy::ToUngrouped,
        FolderDeleteStrategy::ToTrash,
    ] {
        let path = temp_vault("local");
        let mut session = Session::create(&path, PWD, true).unwrap();
        let folder_id = seed(&mut session);

        let repo = session.repo_mut().unwrap();
        repo.delete_folder(&folder_id, strategy).unwrap();

        let counts = repo.item_counts();
        let ungrouped = repo.list_ungrouped_items(false, None);
        assert_eq!(ungrouped.len(), counts.ungrouped);
        // 两种策略都不许留下「指向已删分组」的活条目。
        for item in repo.list_items(None, false, None) {
            if let Some(fid) = item.folder_id.as_deref() {
                assert!(
                    repo.get_folder_opt(fid).is_some(),
                    "本地删分组（{:?}）之后条目 {} 仍指向不存在的分组",
                    strategy,
                    title_of(&item)
                );
            }
        }
    }
}
