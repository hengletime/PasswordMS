//! #531 拖拽排序与频度置顶：排序模型 / 60s 节流 / 钉住 / 归组 / 旧库兼容 回归。
//!
//! 锁死的语义（客户已拍板，见 docs/02-arch.md 数据模型节）：
//! - 列表排序 = **钉住块在顶**（`pin_order` ASC）→ 未钉住按使用频度
//!   （`use_count` DESC → `last_used_at` DESC → `updated_at` DESC → `item_id` ASC）；
//! - **favorite 不再置顶**（保留为纯标记）；
//! - 「使用」= 仅复制动作，同条目 60 秒节流（`USAGE_THROTTLE_MS`）；
//! - 分组频度 = 组内条目 `use_count` 之和（`last_used_at` 取组内最大）；
//! - 新字段全部 `#[serde(default)]`：旧库缺字段读回默认值，`FORMAT_VERSION` 不动。
//!
//! 节流验证用注入时钟（`record_item_usage_at`），排序验证用 `put_item_direct`
//! 灌入显式字段值——两者都让断言对真实时钟零依赖（偶发红比稳定红贵十倍）。

use std::path::PathBuf;

use passwordms_core::constants::USAGE_THROTTLE_MS;
use passwordms_core::error::CoreError;
use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::session::Session;
use passwordms_core::store::repository::{ItemDraft, Repository};

const PWD: &str = "Ordering#Master1";

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-order-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn login_draft(title: &str) -> ItemDraft {
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

/// 灌入一个字段值**全部显式**的条目（排序断言对时钟零依赖的关键），返回其 `item_id`。
/// `item_id` 用合法 UUID（`write_record` 的 record_id 校验要求）；断言一律按 title。
#[allow(clippy::too_many_arguments)]
fn put_login(
    repo: &mut Repository,
    title: &str,
    updated_at: i64,
    use_count: u64,
    last_used_at: Option<i64>,
    pinned: bool,
    pin_order: i64,
    favorite: bool,
) -> String {
    let item_id = uuid::Uuid::new_v4().to_string();
    let item = Item {
        item_id: item_id.clone(),
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
        favorite,
        created_at: 0,
        updated_at,
        rev: 1,
        modified_by: "qa_test".to_string(),
        attachments: Vec::new(),
        pinned,
        pin_order,
        use_count,
        last_used_at,
    };
    repo.put_item_direct(item).unwrap();
    item_id
}

fn titles(repo: &Repository) -> Vec<String> {
    repo.list_items(None, false, None)
        .into_iter()
        .map(|i| i.title().to_string())
        .collect()
}

/// **① list_items 新排序**：钉住块（pin_order ASC）→ 频度段四键逐一生效。
///
/// - 钉住块：`p2(pin_order=2)`、`p1(pin_order=1)` → 按 pin_order，与 use_count 无关；
/// - 未钉住：`hot(use 5)` > `warm(use 3)` > `recent(use 0, last_used 新)` >
///   `updated(use 0, updated_at 新)` > `stale(use 0, updated_at 旧)`（id 兜底不触发）。
#[test]
fn list_items_orders_pinned_block_then_frequency() {
    let dir = temp_dir("sort");
    let mut session = Session::create(&dir.join("v"), PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();

    put_login(repo, "p1", 100, 0, None, true, 2, false);
    put_login(repo, "p0", 100, 0, None, true, 1, false);
    put_login(repo, "hot", 100, 5, Some(900), false, 0, false);
    put_login(repo, "warm", 100, 3, Some(900), false, 0, false);
    put_login(repo, "recent", 100, 0, Some(800), false, 0, false);
    // use_count 与 last_used_at 全同 → 落到 updated_at 兜底：新的在前。
    put_login(repo, "stale", 100, 0, None, false, 0, false);
    put_login(repo, "updated", 200, 0, None, false, 0, false);

    assert_eq!(
        titles(repo),
        vec!["p0", "p1", "hot", "warm", "recent", "updated", "stale"],
        "钉住块按 pin_order ASC 在顶；未钉住按 use_count → last_used_at → updated_at"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **② record_item_usage 的 60s 节流**（注入时钟，零真实等待）。
///
/// ① 首次计频（`last_used_at = None`）；② 窗口内（`now - last < 60s`）不计；
/// ③ 恰好到达窗口（`now - last == 60s`）再计一次；④ 节流窗口内的调用是零状态
/// no-op（rev 也不动）；⑤ 计频不动 `updated_at`（元数据变更不伪装成修改）。
#[test]
fn record_item_usage_throttles_within_window() {
    let dir = temp_dir("throttle");
    let mut session = Session::create(&dir.join("v"), PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();
    let item = repo.create_item(login_draft("被复制的条目")).unwrap();
    let id = item.item_id.clone();
    let updated_at_before = item.updated_at;

    let t0 = 1_000_000;
    repo.record_item_usage_at(&id, t0).unwrap();
    let a = repo.get_item(&id).unwrap();
    assert_eq!(a.use_count, 1, "首次（last_used_at=None）必须计频");
    assert_eq!(a.last_used_at, Some(t0));

    // 零状态 no-op：rev 不动、帧不写。
    let rev_after_first = a.rev;
    repo.record_item_usage_at(&id, t0 + USAGE_THROTTLE_MS - 1)
        .unwrap();
    let b = repo.get_item(&id).unwrap();
    assert_eq!(b.use_count, 1, "窗口内（60s - 1ms）不得计频");
    assert_eq!(
        b.rev, rev_after_first,
        "节流窗口内必须零状态变更（rev 不动）"
    );
    assert_eq!(b.last_used_at, Some(t0));

    // 恰好到达窗口：>= 60s 即计（规格原文「now - last_used_at >= 60 秒才计」）。
    repo.record_item_usage_at(&id, t0 + USAGE_THROTTLE_MS)
        .unwrap();
    let c = repo.get_item(&id).unwrap();
    assert_eq!(c.use_count, 2, "now - last == 60s 应当计频");
    assert_eq!(c.last_used_at, Some(t0 + USAGE_THROTTLE_MS));

    // 计频是元数据变更：updated_at 不变（不把「复制过」伪装成「修改过」）。
    assert_eq!(
        c.updated_at, updated_at_before,
        "record_item_usage 不得碰 updated_at"
    );

    // 持久化：计频结果落盘，重开读回一致。
    drop(session);
    let session = Session::unlock(&dir.join("v"), PWD).unwrap();
    let again = session.repo().unwrap().get_item(&id).unwrap();
    assert_eq!(again.use_count, 2);
    assert_eq!(again.last_used_at, Some(t0 + USAGE_THROTTLE_MS));

    let _ = std::fs::remove_dir_all(&dir);
}

/// **③ 两次钉住 = pin_order 递增**（追加到钉住块末尾，先钉的在前）。
#[test]
fn set_item_pin_appends_increasing_pin_order() {
    let dir = temp_dir("pin");
    let mut session = Session::create(&dir.join("v"), PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();
    let a = repo.create_item(login_draft("第一个钉")).unwrap();
    let b = repo.create_item(login_draft("第二个钉")).unwrap();

    let a = repo.set_item_pin(&a.item_id, true).unwrap();
    let b = repo.set_item_pin(&b.item_id, true).unwrap();
    assert_eq!(a.pin_order, 1, "第一个钉住的 pin_order = 1");
    assert_eq!(b.pin_order, 2, "第二个钉住 = 当前最大 + 1 = 2");
    assert!(a.pinned && b.pinned);

    // 取消钉住：pin_order 归零；再钉时重新追加到末尾。
    let a = repo.set_item_pin(&a.item_id, false).unwrap();
    assert!(!a.pinned);
    assert_eq!(a.pin_order, 0, "取消钉住必须清零 pin_order");
    let a = repo.set_item_pin(&a.item_id, true).unwrap();
    assert_eq!(a.pin_order, 3, "重新钉住 = 当前最大(2) + 1");

    // 持久化：重开读回。
    drop(session);
    let session = Session::unlock(&dir.join("v"), PWD).unwrap();
    let repo = session.repo().unwrap();
    assert_eq!(repo.get_item(&b.item_id).unwrap().pin_order, 2);
    assert_eq!(repo.get_item(&a.item_id).unwrap().pin_order, 3);

    let _ = std::fs::remove_dir_all(&dir);
}

/// **④ reorder_pinned_items 对未钉住 id 报 ERR_VALIDATION**；对不存在的 id 报
/// ItemNotFound；钉住块内重排生效且幂等（同序重放零写入）。
#[test]
fn reorder_pinned_items_rejects_unpinned_ids() {
    let dir = temp_dir("reorder");
    let mut session = Session::create(&dir.join("v"), PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();
    let a = repo.create_item(login_draft("钉A")).unwrap();
    let b = repo.create_item(login_draft("钉B")).unwrap();
    let c = repo.create_item(login_draft("未钉C")).unwrap();
    repo.set_item_pin(&a.item_id, true).unwrap();
    repo.set_item_pin(&b.item_id, true).unwrap();

    // 未钉住的 id：ERR_VALIDATION（Debug 只含错误码，不含载荷）。
    let err = repo
        .reorder_pinned_items(&[a.item_id.clone(), c.item_id.clone()])
        .unwrap_err();
    assert!(
        matches!(err, CoreError::Validation(_)),
        "未钉住 id 必须报 Validation（实际 {err:?}）"
    );
    // 不存在的 id：ItemNotFound（与 reorder_folders 的 FolderNotFound 同例）。
    let err = repo
        .reorder_pinned_items(&["no-such-id".to_string()])
        .unwrap_err();
    assert!(matches!(err, CoreError::ItemNotFound));

    // 合法重排：B 在前（b 的 pin_order 2→0 变更者推 rev；a 的 1→1 不变不写帧）。
    let rev_a = repo.get_item(&a.item_id).unwrap().rev;
    let rev_b = repo.get_item(&b.item_id).unwrap().rev;
    repo.reorder_pinned_items(&[b.item_id.clone(), a.item_id.clone()])
        .unwrap();
    assert_eq!(repo.get_item(&b.item_id).unwrap().pin_order, 0);
    assert_eq!(repo.get_item(&a.item_id).unwrap().pin_order, 1);
    assert!(
        repo.get_item(&b.item_id).unwrap().rev > rev_b,
        "pin_order 变更者必须推 rev（走 LWW 同步）"
    );
    assert_eq!(
        repo.get_item(&a.item_id).unwrap().rev,
        rev_a,
        "pin_order 未变者不得推 rev（幂等重放零写入的一半）"
    );

    // 幂等：同序重放不写帧（rev 不再变）。
    let rev_b2 = repo.get_item(&b.item_id).unwrap().rev;
    repo.reorder_pinned_items(&[b.item_id.clone(), a.item_id.clone()])
        .unwrap();
    assert_eq!(
        repo.get_item(&b.item_id).unwrap().rev,
        rev_b2,
        "同序重放必须零写入"
    );

    // 排序跟随新 pin_order。
    assert_eq!(titles(repo)[..2], ["钉B", "钉A"]);

    let _ = std::fs::remove_dir_all(&dir);
}

/// **⑤ move_item_to_folder**：目标分组不存在报 Validation；`None`（移出为未分组）
/// 成功；`updated_at` 不变。
#[test]
fn move_item_to_folder_validates_and_supports_ungrouped() {
    let dir = temp_dir("move");
    let mut session = Session::create(&dir.join("v"), PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();
    let folder = repo
        .create_folder("目标组".to_string(), None, None)
        .unwrap();
    let item = repo.create_item(login_draft("要挪的条目")).unwrap();
    let updated_at_before = item.updated_at;

    repo.move_item_to_folder(&item.item_id, Some(folder.folder_id.clone()))
        .unwrap();
    assert_eq!(
        repo.get_item(&item.item_id).unwrap().folder_id,
        Some(folder.folder_id.clone())
    );

    let err = repo
        .move_item_to_folder(&item.item_id, Some("no-such-folder".to_string()))
        .unwrap_err();
    assert!(
        matches!(err, CoreError::Validation(_)),
        "目标分组不存在必须报 Validation（实际 {err:?}）"
    );

    repo.move_item_to_folder(&item.item_id, None).unwrap();
    let moved = repo.get_item(&item.item_id).unwrap();
    assert_eq!(moved.folder_id, None, "None = 移出为未分组");
    assert_eq!(
        moved.updated_at, updated_at_before,
        "移动分组不得碰 updated_at"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **⑥ 旧库兼容**：缺新字段的 Item JSON 反序列化得默认值；落库后 **unlock 重开**
/// 读回仍为默认值（`FORMAT_VERSION` 不动，旧客户端 serde 忽略未知字段不会崩）。
///
/// 说明：落库走 `put_item_direct`（typed 写路径），磁盘帧因此携带**显式默认值**；
/// 「帧里缺字段」这一形态在 serde 边界（`from_value`）证明——两者对读方等价。
#[test]
fn legacy_item_without_new_fields_reads_back_defaults() {
    let dir = temp_dir("legacy");
    let vault = dir.join("v");
    let id = "33333333-3333-4333-8333-333333333333";

    // serde 边界：旧客户端的帧（无 pinned/pin_order/use_count/last_used_at）。
    let legacy_json = serde_json::json!({
        "item_id": id,
        "kind": { "type": "login", "title": "旧库条目" },
        "favorite": false,
        "created_at": 0,
        "updated_at": 0,
        "rev": 1,
        "modified_by": "old-client"
    });
    let legacy: Item = serde_json::from_value(legacy_json).expect("旧 JSON 必须可反序列化");
    assert!(!legacy.pinned);
    assert_eq!(legacy.pin_order, 0);
    assert_eq!(legacy.use_count, 0);
    assert_eq!(legacy.last_used_at, None);

    // 落库 + 重开：读回默认值。
    {
        let mut session = Session::create(&vault, PWD, true).unwrap();
        session.repo_mut().unwrap().put_item_direct(legacy).unwrap();
    }
    {
        let session = Session::unlock(&vault, PWD).unwrap();
        let read = session.repo().unwrap().get_item(id).unwrap();
        assert!(!read.pinned, "重开后 pinned 应为默认 false");
        assert_eq!(read.pin_order, 0);
        assert_eq!(read.use_count, 0);
        assert_eq!(read.last_used_at, None);
        assert_eq!(read.title(), "旧库条目", "既有字段不受影响");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// **⑦ list_folders 聚合频度排序**：组频度 = Σ组内 use_count；钉住分组在顶；
/// 聚合值与排序同源（`folder_usage`）。
#[test]
fn list_folders_orders_by_aggregated_usage() {
    let dir = temp_dir("folders");
    let mut session = Session::create(&dir.join("v"), PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();

    let quiet = repo.create_folder("冷组".to_string(), None, None).unwrap();
    let busy = repo.create_folder("热组".to_string(), None, None).unwrap();
    let warm = repo.create_folder("温组".to_string(), None, None).unwrap();
    let pinned = repo.create_folder("钉组".to_string(), None, None).unwrap();

    // 热组两条 use 3+5=8；温组一条 use 4 + last_used 较新；冷组 0。
    let mk = |repo: &mut Repository,
              title: &str,
              fid: Option<String>,
              use_count: u64,
              last: Option<i64>| {
        let id = put_login(repo, title, 100, use_count, last, false, 0, false);
        repo.move_item_to_folder(&id, fid).unwrap();
    };
    mk(repo, "热-1", Some(busy.folder_id.clone()), 3, Some(500));
    mk(repo, "热-2", Some(busy.folder_id.clone()), 5, Some(400));
    mk(repo, "温-1", Some(warm.folder_id.clone()), 4, Some(900));
    mk(repo, "冷-1", Some(quiet.folder_id.clone()), 0, None);
    repo.set_folder_pin(&pinned.folder_id, true).unwrap();

    let ordered = repo.list_folders();
    let names: Vec<String> = ordered.iter().map(|f| f.name.clone()).collect();
    assert_eq!(
        names,
        vec!["钉组", "热组", "温组", "冷组"],
        "钉住分组在顶；未钉住按 Σuse_count DESC（8 > 4 > 0）"
    );

    // 聚合值与排序同源（folder_usage）；温组 last_used = 组内最大 900。
    let usage = repo.folder_usage();
    assert_eq!(usage[&busy.folder_id].use_count, 8);
    assert_eq!(usage[&busy.folder_id].last_used_at, Some(500));
    assert_eq!(usage[&warm.folder_id].use_count, 4);
    assert_eq!(usage[&warm.folder_id].last_used_at, Some(900));
    assert_eq!(usage.get(&quiet.folder_id).map(|u| u.use_count), Some(0));

    // 持久化：钉住状态重开读回。
    drop(session);
    let session = Session::unlock(&dir.join("v"), PWD).unwrap();
    let repo = session.repo().unwrap();
    assert!(repo.list_folders()[0].pinned, "钉组重开后仍在钉住块");

    let _ = std::fs::remove_dir_all(&dir);
}

/// **⑧ 取消收藏置顶回归**：favorite=true 的未钉住条目**不再**自动在顶——
/// 排序完全由钉住块 + 频度键决定，favorite 只是标记。
#[test]
fn favorite_no_longer_promotes_to_top() {
    let dir = temp_dir("fav");
    let mut session = Session::create(&dir.join("v"), PWD, true).unwrap();
    let repo = session.repo_mut().unwrap();

    // favorite 条目：use 0、updated 旧；普通条目：use 1。
    put_login(repo, "fav-star", 100, 0, None, false, 0, true);
    put_login(repo, "plain", 100, 1, Some(700), false, 0, false);

    assert_eq!(
        titles(repo),
        vec!["plain", "fav-star"],
        "favorite=true 不得置顶：频度段内 use 1 > use 0"
    );

    // 频度完全同值时也不得用 favorite 破平——落回 updated_at/id 兜底。
    put_login(repo, "fav-star2", 300, 0, None, false, 0, true);
    put_login(repo, "plain2", 200, 0, None, false, 0, false);
    assert_eq!(
        titles(repo),
        vec!["plain", "fav-star2", "plain2", "fav-star"],
        "同频度下按 updated_at DESC（300→200→100），favorite 不参与排序"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
