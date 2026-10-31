//! D-153 回归：未分组必须「点得进去」，且点进去的条数与侧栏数字**相等**。
//!
//! 主人诉求（2026-10-31）：侧栏显示「另有 4 条未分组」，但库里 100 多条，
//! 用户无法从 100 多条里找出这 4 条 —— 数字成了只给线索不给入口。
//!
//! 本轮真正的验收标准只有一条**恒等式**：
//!
//! ```text
//! list_ungrouped_items().len() == item_counts().ungrouped
//! ```
//!
//! 侧栏写「4 条未分组」、点进去列出 5 条（或 3 条），比没有入口更糟：
//! 用户会先把差额读成「数据丢了」，再发现是筛选口径不一致。这与 D-150/D-151
//! 同型 —— **不报错，但读数与事实不一致**。
//!
//! 其余几条钉的是「未分组不是一个特例分支」：收藏叠加、搜索叠加、回收站排除、
//! 删除分组后条目真的落进来、排序与「全部」视图逐条一致。

use std::path::PathBuf;

use passwordms_core::model::item::{ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::repository::{FolderDeleteStrategy, ItemDraft};

const PWD: &str = "Ungrouped#Pw-153a";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-ungrouped-{tag}-{}", uuid::Uuid::new_v4()));
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

/// 造「2 个分组 + 3 条未分组（其中 1 条收藏）」的固定形态，返回 (fa, fb)。
fn seed(session: &mut Session) -> (String, String) {
    let repo = session.repo_mut().unwrap();
    let a = repo
        .create_folder("A".into(), None, None)
        .unwrap()
        .folder_id;
    let b = repo
        .create_folder("B".into(), None, None)
        .unwrap()
        .folder_id;
    repo.create_item(draft("a1", Some(&a), false)).unwrap();
    repo.create_item(draft("a2", Some(&a), false)).unwrap();
    repo.create_item(draft("b1", Some(&b), false)).unwrap();
    repo.create_item(draft("u1", None, true)).unwrap();
    repo.create_item(draft("u2", None, false)).unwrap();
    repo.create_item(draft("u3", None, false)).unwrap();
    (a, b)
}

fn titles(items: &[passwordms_core::model::item::Item]) -> Vec<String> {
    items
        .iter()
        .map(|i| match &i.kind {
            ItemKind::Login(l) => l.title.clone(),
            ItemKind::SecureNote(n) => n.title.clone(),
            // D-60：`Attachment` 变体已废弃（附件内嵌在 `Item.attachments`）。
            // 夹具不会产生它；用兜底分支而不是引用废弃变体，免得给测试引入 deprecated 警告。
            _ => String::new(),
        })
        .collect()
}

#[test]
fn ungrouped_view_matches_sidebar_count() {
    let path = temp_vault("count");
    let mut session = Session::create(&path, PWD, true).unwrap();
    seed(&mut session);

    let repo = session.repo().unwrap();
    let listed = repo.list_ungrouped_items(false, None);
    let counts = repo.item_counts();

    // ★ 本轮核心断言：侧栏读数 == 点进去的条数。
    assert_eq!(
        listed.len(),
        counts.ungrouped,
        "侧栏显示 {} 条未分组，点进去却列出 {} 条",
        counts.ungrouped,
        listed.len()
    );
    assert_eq!(listed.len(), 3, "夹具是 3 条未分组");
    assert!(
        listed.iter().all(|i| i.folder_id.is_none()),
        "未分组视图里不许出现任何属于分组的条目"
    );

    // 未分组视图 + 各组 = 全部：不能有既不在分组、也不在未分组的条目凭空消失。
    let grouped: usize = repo.folder_usage().values().map(|u| u.item_count).sum();
    assert_eq!(
        grouped + listed.len(),
        repo.list_items(None, false, None).len(),
        "各组 + 未分组必须等于全部"
    );

    let mut t = titles(&listed);
    t.sort();
    assert_eq!(t, vec!["u1", "u2", "u3"]);
}

#[test]
fn ungrouped_view_stacks_with_favorite_and_query() {
    let path = temp_vault("stack");
    let mut session = Session::create(&path, PWD, true).unwrap();
    seed(&mut session);

    let repo = session.repo().unwrap();
    // 收藏叠加：未分组里只有 u1 标了收藏。
    let fav = repo.list_ungrouped_items(true, None);
    assert_eq!(titles(&fav), vec!["u1"]);

    // 搜索叠加：u2 命中，且**只**在未分组范围内命中（a 组的条目名不含 "u2"）。
    let hit = repo.list_ungrouped_items(false, Some("u2"));
    assert_eq!(titles(&hit), vec!["u2"]);

    // 搜索落空时是空列表，而不是退回全库（退回全库 = 用户以为未分组里有这 100 条）。
    assert!(repo.list_ungrouped_items(false, Some("a1")).is_empty());
}

#[test]
fn trash_is_excluded_from_ungrouped_view() {
    let path = temp_vault("trash");
    let mut session = Session::create(&path, PWD, true).unwrap();
    seed(&mut session);

    let target = {
        let repo = session.repo_mut().unwrap();
        let u2 = repo
            .list_ungrouped_items(false, None)
            .into_iter()
            .find(|i| matches!(&i.kind, ItemKind::Login(l) if l.title == "u2"))
            .unwrap();
        repo.delete_item(&u2.item_id, false).unwrap();
        u2.item_id
    };
    assert!(!target.is_empty());

    let repo = session.repo().unwrap();
    let listed = repo.list_ungrouped_items(false, None);
    let counts = repo.item_counts();
    assert_eq!(listed.len(), 2, "进回收站后不再出现在未分组视图");
    assert_eq!(
        listed.len(),
        counts.ungrouped,
        "回收站条目必须同时从视图与读数里消失，否则两者又对不上"
    );
}

#[test]
fn deleting_folder_to_ungrouped_makes_items_listable() {
    let path = temp_vault("delfolder");
    let mut session = Session::create(&path, PWD, true).unwrap();
    let (fa, _fb) = seed(&mut session);

    {
        let repo = session.repo_mut().unwrap();
        // 真实路径：删除分组时选择「移出分组」——条目必须立刻出现在未分组视图里。
        repo.delete_folder(&fa, FolderDeleteStrategy::ToUngrouped)
            .unwrap();
    }

    let repo = session.repo().unwrap();
    let listed = repo.list_ungrouped_items(false, None);
    let counts = repo.item_counts();
    assert_eq!(listed.len(), 5, "原 3 条未分组 + A 组移出的 2 条");
    assert_eq!(listed.len(), counts.ungrouped);
    assert!(titles(&listed).contains(&"a1".to_string()));
}

#[test]
fn ungrouped_order_is_identical_to_all_view() {
    let path = temp_vault("order");
    let mut session = Session::create(&path, PWD, true).unwrap();
    // 只关心未分组那 3 条的顺序，分组 id 用不上。
    seed(&mut session);

    // 给未分组条目造出「钉住」的差异，否则顺序一致是平凡成立的。
    let u3 = {
        let repo = session.repo_mut().unwrap();
        let all = repo.list_ungrouped_items(false, None);
        let u3 = all[2].item_id.clone();
        repo.set_item_pin(&u3, true).unwrap();
        u3
    };

    let repo = session.repo().unwrap();
    // 「全部」视图里把未分组的挑出来，顺序必须与未分组视图**逐条相同**。
    let from_all: Vec<String> = repo
        .list_items(None, false, None)
        .into_iter()
        .filter(|i| i.folder_id.is_none())
        .map(|i| i.item_id)
        .collect();
    let direct: Vec<String> = repo
        .list_ungrouped_items(false, None)
        .into_iter()
        .map(|i| i.item_id)
        .collect();
    assert_eq!(
        from_all, direct,
        "未分组视图与全部视图的排序必须共用同一份实现（钉住在最前，其余按频度）"
    );
    // 钉住的 u3 必须排第一 —— 证明排序真的生效，不是两个空列表相等。
    assert_eq!(direct.first(), Some(&u3));
}
