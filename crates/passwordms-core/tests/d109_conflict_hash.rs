//! D-109 冲突仲裁死循环守门（根因：`item_hash` 整结构 JSON 哈希包含同步元数据）。
//!
//! 死循环机制（主人实测：手机同步→电脑 3 冲突；仲裁后电脑同步→手机又 3 冲突，永不收敛）：
//! 1. 双端各自仲裁「采用远端」→ `resolve_conflict` 重写产物元数据
//!    （`rev = max+1`、`updated_at = now`、`modified_by = 本机`）；
//! 2. 旧口径下两端产物**业务内容一致也必 hash 异**（元数据不同）；
//! 3. `detect_conflict` 判「双改且 hash 异」→ 冲突重生，rev 交替抬升永不收敛。
//!
//! 修复：`item_hash` / `folder_hash` 只哈希业务内容（剥离 `rev` / `updated_at` /
//! `modified_by` / `use_count` / `last_used_at`）。本套件锁定：
//! ① hash 元数据无关性（单测）；② 双端 TakeRemote 三轮收敛（e2e，旧实现第 3 轮仍出冲突）；
//! ③ KeepLocal 双端第二轮零重发零冲突；④ folder_hash 同口径。

use std::path::PathBuf;

use passwordms_core::model::conflict::ConflictChoice;
use passwordms_core::model::item::{Item, ItemKind, LoginData};
use passwordms_core::model::sync_state::{BaseRev, DeviceSyncState};
use passwordms_core::session::Session;
use passwordms_core::store::repository::{folder_hash, item_hash, ItemDraft};
use passwordms_core::sync::merge;

const PWD_A: &str = "D109#Vault-A1";
const PWD_B: &str = "D109#Vault-B2";

fn temp_vault(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pms-d109-{tag}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("passwordms.vault")
}

/// 手工构造登录条目（全字段可控，用于 hash 口径断言）。
fn login_item(
    id: &str,
    title: &str,
    rev: u64,
    updated_at: i64,
    modified_by: &str,
    use_count: u64,
    last_used_at: Option<i64>,
) -> Item {
    Item {
        item_id: id.to_string(),
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: None,
            username: Some("user".to_string()),
            password: Some("pw".to_string()),
            notes: None,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
        created_at: 1000,
        updated_at,
        rev,
        modified_by: modified_by.to_string(),
        attachments: Vec::new(),
        pinned: false,
        pin_order: 0,
        use_count,
        last_used_at,
    }
}

// ── ① hash 元数据无关性 ──

#[test]
fn item_hash_ignores_sync_metadata_and_local_usage() {
    let base = login_item("id-1", "Site", 2, 5000, "device-A", 0, None);
    // 仅同步元数据与本地统计不同（rev / updated_at / modified_by / use_count / last_used_at）。
    let meta_shifted = login_item("id-1", "Site", 9, 999_999, "device-B", 42, Some(888_888));
    assert_eq!(
        item_hash(&base).expect("hash base"),
        item_hash(&meta_shifted).expect("hash shifted"),
        "D-109：同步元数据 / 本地统计差异不得改变内容摘要"
    );
    // 业务内容不同（密码变了）→ hash 必须不同。
    let mut content_changed = base.clone();
    if let ItemKind::Login(login) = &mut content_changed.kind {
        login.password = Some("pw-CHANGED".to_string());
    }
    assert_ne!(
        item_hash(&base).expect("hash base"),
        item_hash(&content_changed).expect("hash changed"),
        "业务内容差异必须反映在内容摘要中"
    );
}

#[test]
fn folder_hash_ignores_sync_metadata() {
    use passwordms_core::model::folder::Folder;
    let make = |rev: u64, updated_at: i64| Folder {
        folder_id: "f-1".to_string(),
        name: "Work".to_string(),
        icon: Some("briefcase".to_string()),
        color: None,
        sort_order: 3,
        rev,
        updated_at,
        pinned: false,
        pin_order: 0,
    };
    assert_eq!(
        folder_hash(&make(1, 100)).expect("hash a"),
        folder_hash(&make(77, 999_999)).expect("hash b"),
        "D-109：分组同步元数据差异不得改变内容摘要"
    );
    let mut renamed = make(1, 100);
    renamed.name = "Personal".to_string();
    assert_ne!(
        folder_hash(&make(1, 100)).expect("hash a"),
        folder_hash(&renamed).expect("hash renamed"),
    );
}

// ── ② / ③ 双端手动交换 e2e ──

struct Side {
    session: Session,
    /// 本端「对另一端」的同步状态（base + 发送水位），等价真实链路的 per-peer state。
    state: DeviceSyncState,
}

impl Side {
    fn create(vault: &PathBuf, pwd: &str, device_id: &str) -> Side {
        let session = Session::create(vault, pwd, true).expect("create vault");
        Side {
            session,
            state: DeviceSyncState {
                device_id: device_id.to_string(),
                base: std::collections::HashMap::new(),
                last_recv_seq: 0,
                last_sent_seq: 0,
                sent_watermark: std::collections::HashMap::new(),
            },
        }
    }

    fn update_title(&mut self, item_id: &str, new_title: &str) {
        let repo = self.session.repo_mut().expect("repo");
        let item = repo.get_item(item_id).expect("item");
        let mut kind = item.kind.clone();
        kind.set_title(new_title.to_string());
        repo.update_item(
            item_id,
            ItemDraft {
                kind,
                folder_id: item.folder_id.clone(),
                favorite: item.favorite,
            },
        )
        .expect("update_item");
    }

    fn unresolved_conflicts(&self) -> Vec<String> {
        self.session
            .repo()
            .expect("repo")
            .list_conflicts()
            .into_iter()
            .map(|c| c.conflict_id)
            .collect()
    }

    fn item_title(&self, item_id: &str) -> String {
        self.session
            .repo()
            .expect("repo")
            .get_item(item_id)
            .expect("item")
            .title()
            .to_string()
    }

    fn item(&self, item_id: &str) -> Item {
        self.session
            .repo()
            .expect("repo")
            .get_item(item_id)
            .expect("item")
    }
}

/// 单向交换：sender 按（对 receiver 的）状态导出 → receiver 合并。
/// 返回 receiver 本轮产生的冲突 item_id 列表。发送成功即推进 sender 水位（宁多勿丢语义）。
fn pull(receiver: &mut Side, sender: &mut Side) -> Vec<String> {
    let changes = sender
        .session
        .repo_mut()
        .expect("repo")
        .export_changes(&sender.state);
    merge::note_sent_changes(&mut sender.state, &changes);
    merge::note_sent_tombstones(&mut sender.state, &changes);
    let rep = merge::apply_changes(
        receiver.session.repo_mut().expect("repo"),
        &mut receiver.state,
        &changes,
    )
    .expect("apply_changes");
    merge::note_received_changes(
        receiver.session.repo().expect("repo"),
        &mut receiver.state,
        &changes,
    );
    rep.conflicts
}

/// 双向交换（A 发 → B 收；B 发 → A 收）。返回（B 新冲突数、A 新冲突数）。
fn exchange(a: &mut Side, b: &mut Side) -> (usize, usize) {
    let to_b = pull(b, a).len();
    let to_a = pull(a, b).len();
    (to_b, to_a)
}

fn resolve_all_take_remote(side: &mut Side) {
    for cid in side.unresolved_conflicts() {
        let resets = side
            .session
            .repo_mut()
            .expect("repo")
            .resolve_conflict(&cid, ConflictChoice::TakeRemote)
            .expect("resolve_conflict");
        // D-109：水位回落清单应用到本测试持有的 per-peer 内存 state（等价命令层行为）。
        for r in resets {
            if let Some(wm) = side.state.sent_watermark.get_mut(&r.record_id) {
                if *wm > r.new_rev {
                    *wm = r.new_rev;
                }
            }
        }
    }
}

fn resolve_all_keep_local(side: &mut Side) {
    for cid in side.unresolved_conflicts() {
        let resets = side
            .session
            .repo_mut()
            .expect("repo")
            .resolve_conflict(&cid, ConflictChoice::KeepLocal)
            .expect("resolve_conflict");
        // D-111：base 推进清单应用到本测试持有的 per-peer 内存 state（与命令层
        // sync_cmd 同构——rev 抬升天然高于发送水位，无水位回落项）。
        for r in resets {
            side.state.base.insert(
                r.record_id.clone(),
                BaseRev {
                    rev: r.new_rev,
                    hash: r.new_hash,
                },
            );
        }
    }
}

/// 同 rev 双改的导出不得被回声消除吞掉（D-109 第二缺陷的直接守门）。
///
/// B 收到 A 的 rev2 后，B 自己独立改出的 rev2 必须**照常导出**给 A——
/// 旧 `note_received_changes` 无条件推水位，把 B 的 rev2 当成「A 已持有」，
/// A 永远收不到 B 的修改（冲突只在一侧出现、另一侧静默丢改）。
#[test]
fn same_rev_divergence_still_exports_to_peer() {
    let vault_a = temp_vault("ea");
    let vault_b = temp_vault("eb");
    let mut a = Side::create(&vault_a, PWD_A, "device-A");
    let mut b = Side::create(&vault_b, PWD_B, "device-B");
    let host_vk = a.session.repo().expect("repo").vault_key().clone();
    b.session
        .adopt_vault_key(&host_vk)
        .expect("adopt_vault_key");

    let item_id = {
        let repo = a.session.repo_mut().expect("repo");
        repo.create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "site".to_string(),
                url: None,
                username: Some("user".to_string()),
                password: Some("pw".to_string()),
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        })
        .expect("create_item")
        .item_id
    };
    let first = pull(&mut b, &mut a);
    assert!(first.is_empty(), "首次推送不得产生冲突：{first:?}");
    // 回程空交换：让 A 的 base 建立（真实链路双向同步天然如此；否则 A 无基准，
    // detect_conflict 无基准不冲突 → A 静默 skip，测不出双改）。
    let back = pull(&mut a, &mut b);
    assert!(back.is_empty(), "回程空交换不得产生冲突：{back:?}");

    // 双端各自修改 → 恰好同 rev（同 rev 双改）。
    a.update_title(&item_id, "edited-on-A");
    b.update_title(&item_id, "edited-on-B");

    // 单向：A → B。B 收到 A 的 rev2 并合并。
    let to_b = pull(&mut b, &mut a);
    assert_eq!(to_b.len(), 1, "B 侧应产生冲突");

    // 关键断言：B 的 rev2 仍须导出（旧实现在此被回声消除吞掉 → A 收不到）。
    let changes = b.session.repo().expect("repo").export_changes(&b.state);
    assert!(
        changes
            .iter()
            .any(|c| c.record_id == item_id && c.rev >= 2 && !c.deleted),
        "B 的独立修改必须照常导出（回声消除不得吞掉同 rev 双改）"
    );

    // 反向：B → A。A 也应收到 B 的 rev2 并产生冲突（两侧对称感知）。
    if std::env::var("PMS_D109_DEBUG").is_ok() {
        let changes = b.session.repo().expect("repo").export_changes(&b.state);
        for c in &changes {
            eprintln!(
                "DEBUG b export: id={} rev={} deleted={}",
                c.record_id, c.rev, c.deleted
            );
        }
        eprintln!("DEBUG a base: {:?}", a.state.base.get(&item_id));
        eprintln!("DEBUG b base: {:?}", b.state.base.get(&item_id));
        eprintln!(
            "DEBUG a watermark: {:?}",
            a.state.sent_watermark.get(&item_id)
        );
        eprintln!(
            "DEBUG b watermark: {:?}",
            b.state.sent_watermark.get(&item_id)
        );
        let la = a
            .session
            .repo()
            .expect("repo")
            .get_item(&item_id)
            .expect("a item");
        eprintln!("DEBUG a item rev={} title={}", la.rev, la.title());
    }
    let to_a = pull(&mut a, &mut b);
    assert_eq!(to_a.len(), 1, "A 侧也应产生冲突（同 rev 双改对称感知）");
}

/// 双端 TakeRemote 收敛（D-109 主守门）：仲裁 = **互采对方版本**，此后零振荡。
///
/// 旧实现（rev = max+1 制造新版本）双端仲裁产物永远互为「对方上一版」，每轮同步
/// 重新判冲突、rev 交替抬升，永不收敛——即主人实测的死循环。原样落地后：两端各持
/// 对方版本（互采语义，各自获得想要的版本），内容不再变化，同步流量归零。
#[test]
fn take_remote_both_sides_converge_within_six_rounds() {
    let vault_a = temp_vault("ta");
    let vault_b = temp_vault("tb");
    let mut a = Side::create(&vault_a, PWD_A, "device-A");
    let mut b = Side::create(&vault_b, PWD_B, "device-B");
    // B 空库采纳 A 的 vault key（等价真实配对的 VK 共享，否则条目密钥互不相通）。
    let host_vk = a.session.repo().expect("repo").vault_key().clone();
    b.session
        .adopt_vault_key(&host_vk)
        .expect("adopt_vault_key");

    // A 建条目并全量首推给 B（B 从未见过 → 无冲突，base 建立）。
    let item_id = {
        let repo = a.session.repo_mut().expect("repo");
        repo.create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "site".to_string(),
                url: None,
                username: Some("user".to_string()),
                password: Some("pw".to_string()),
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        })
        .expect("create_item")
        .item_id
    };
    let first = pull(&mut b, &mut a);
    assert!(first.is_empty(), "首次推送不得产生冲突：{first:?}");
    // 回程空交换：让 A 的 base 建立（真实链路双向同步天然如此；否则 A 无基准，
    // detect_conflict 无基准不冲突 → A 静默 skip，测不出双改）。
    let back = pull(&mut a, &mut b);
    assert!(back.is_empty(), "回程空交换不得产生冲突：{back:?}");

    // 双端各自修改同一条目（真实双改场景）。
    a.update_title(&item_id, "edited-on-A");
    b.update_title(&item_id, "edited-on-B");

    // 第 1 轮：双端各进 1 个冲突。
    let (b_c1, a_c1) = exchange(&mut a, &mut b);
    assert_eq!((a_c1, b_c1), (1, 1), "双改首轮两端各应产生 1 个冲突");

    // 双端都仲裁「采用远端」（旧实现下这是死循环引擎）。
    resolve_all_take_remote(&mut a);
    resolve_all_take_remote(&mut b);

    // 后续 6 轮：必须零新冲突（互采后内容不再变化，流量归零）。
    for round in 2..=7 {
        let (to_b, to_a) = exchange(&mut a, &mut b);
        assert_eq!(
            (to_a, to_b),
            (0, 0),
            "D-109：第 {round} 轮不得再产生冲突（死循环守门）"
        );
    }

    // 互采语义：两端各持对方版本，各自不再变化。
    assert_eq!(
        a.item_title(&item_id),
        "edited-on-B",
        "A 采用远端 = 持有 B 版"
    );
    assert_eq!(
        b.item_title(&item_id),
        "edited-on-A",
        "B 采用远端 = 持有 A 版"
    );
}

/// TakeRemote 水位回落守门：仲裁落地 rev 回落后，本端**后续修改必须可达对端**。
///
/// 场景：A 曾向 B 发出过 rev3（水位=3），后仲裁 TakeRemote 落地 rev2 的对端快照——
/// 若不回落水位，A 再编辑出的 rev3 会被「rev ≤ 水位免重发」永久吞掉，对端永远收不到。
#[test]
fn take_remote_resets_watermark_so_future_edits_reach_peer() {
    let vault_a = temp_vault("wa");
    let vault_b = temp_vault("wb");
    let mut a = Side::create(&vault_a, PWD_A, "device-A");
    let mut b = Side::create(&vault_b, PWD_B, "device-B");
    let host_vk = a.session.repo().expect("repo").vault_key().clone();
    b.session
        .adopt_vault_key(&host_vk)
        .expect("adopt_vault_key");

    let item_id = {
        let repo = a.session.repo_mut().expect("repo");
        repo.create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "v1".to_string(),
                url: None,
                username: Some("user".to_string()),
                password: Some("pw".to_string()),
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        })
        .expect("create_item")
        .item_id
    };
    let first = pull(&mut b, &mut a);
    assert!(first.is_empty(), "{first:?}");
    let back = pull(&mut a, &mut b);
    assert!(back.is_empty(), "{back:?}");

    // 双端同时离线各自修改（同 rev 双改）→ 交换 → 双方冲突。
    a.update_title(&item_id, "edited-on-A");
    b.update_title(&item_id, "edited-on-B");
    let (to_b, to_a) = exchange(&mut a, &mut b);
    assert_eq!((to_a, to_b), (1, 1));

    // 模拟复杂时序（如三端同步）中「本端曾向该对端发出过更高 rev」：手工抬高
    // A 的发送水位到 5。仲裁落地 rev2 后若不回落，A 的后续修改将被永久吞掉。
    a.state.sent_watermark.insert(item_id.clone(), 5);

    // A 仲裁 TakeRemote：落地 B 快照（rev2）。水位 5 > 2，须回落。
    resolve_all_take_remote(&mut a);

    // 关键守门：A 再编辑（rev3）必须能导出并到达 B（无回落则被水位吞掉）。
    a.update_title(&item_id, "v3-after-resolve");
    let wm = a.state.sent_watermark.get(&item_id).copied();
    assert_eq!(wm, Some(2), "仲裁后水位必须回落到落地 rev");
    let to_b2 = pull(&mut b, &mut a);
    assert!(to_b2.is_empty(), "B 无独立修改不得冲突：{to_b2:?}");
    assert_eq!(
        b.item_title(&item_id),
        "v3-after-resolve",
        "A 仲裁后的新修改必须到达 B（水位回落守门）"
    );
}

/// 双端 KeepLocal（D-111 新语义）：「保留本机」= 本机版本胜出并传播。
///
/// 旧实现（条目原样不动）两端被水位/base 双重卡死：胜出版本发不出去、对端同 rev
/// 静默 skip → 两端永久不一致、也不再报冲突（主人实测）。新实现各自抬 rev 落地
/// 胜出版本，交换后 LWW 按 updated_at 确定性收敛（后仲裁者胜），零冲突。
#[test]
fn keep_local_both_sides_settle_in_round_two() {
    let vault_a = temp_vault("ka");
    let vault_b = temp_vault("kb");
    let mut a = Side::create(&vault_a, PWD_A, "device-A");
    let mut b = Side::create(&vault_b, PWD_B, "device-B");
    let host_vk = a.session.repo().expect("repo").vault_key().clone();
    b.session
        .adopt_vault_key(&host_vk)
        .expect("adopt_vault_key");

    let item_id = {
        let repo = a.session.repo_mut().expect("repo");
        repo.create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "site".to_string(),
                url: None,
                username: Some("user".to_string()),
                password: Some("pw".to_string()),
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        })
        .expect("create_item")
        .item_id
    };
    let first = pull(&mut b, &mut a);
    assert!(first.is_empty(), "首次推送不得产生冲突：{first:?}");
    let back = pull(&mut a, &mut b);
    assert!(back.is_empty(), "回程空交换不得产生冲突：{back:?}");

    a.update_title(&item_id, "edited-on-A");
    b.update_title(&item_id, "edited-on-B");

    let (b_c1, a_c1) = exchange(&mut a, &mut b);
    assert_eq!((a_c1, b_c1), (1, 1));

    resolve_all_keep_local(&mut a);
    resolve_all_keep_local(&mut b);

    // 第 2 轮：胜出版本互换，LWW 确定性收敛（b 后仲裁 updated_at 更新 → b 胜），
    // 不产生新冲突。
    let (b_c2, a_c2) = exchange(&mut a, &mut b);
    assert_eq!((a_c2, b_c2), (0, 0), "KeepLocal 仲裁后不得重生冲突");

    // 两端收敛一致（后仲裁者的版本胜出），且业务 hash 相同。
    assert_eq!(
        a.item_title(&item_id),
        "edited-on-B",
        "双 KeepLocal 下后仲裁者胜（LWW updated_at）"
    );
    assert_eq!(b.item_title(&item_id), "edited-on-B");
    assert_eq!(
        item_hash(&a.item(&item_id)).expect("hash a"),
        item_hash(&b.item(&item_id)).expect("hash b"),
        "双 KeepLocal 后两端必须收敛一致"
    );

    // 第 3 轮：零交换零冲突（收敛后流量归零）。
    let (b_c3, a_c3) = exchange(&mut a, &mut b);
    assert_eq!((a_c3, b_c3), (0, 0));
}

/// D-111 主守门（主人实测场景）：仅一端仲裁 KeepLocal，另一端冲突挂着不动。
///
/// A KeepLocal → 胜出版本（rev 抬升）传播到 B：B 的 LWW 应用它，且 B 挂着的
/// 旧冲突（快照已过时）**自动过期**——否则用户在 B 仲裁旧快照会把条目打回旧版。
#[test]
fn keep_local_propagates_winner_and_expires_peer_conflict() {
    let vault_a = temp_vault("la");
    let vault_b = temp_vault("lb");
    let mut a = Side::create(&vault_a, PWD_A, "device-A");
    let mut b = Side::create(&vault_b, PWD_B, "device-B");
    let host_vk = a.session.repo().expect("repo").vault_key().clone();
    b.session
        .adopt_vault_key(&host_vk)
        .expect("adopt_vault_key");

    let item_id = {
        let repo = a.session.repo_mut().expect("repo");
        repo.create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "site".to_string(),
                url: None,
                username: Some("user".to_string()),
                password: Some("pw".to_string()),
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        })
        .expect("create_item")
        .item_id
    };
    let first = pull(&mut b, &mut a);
    assert!(first.is_empty(), "{first:?}");
    let back = pull(&mut a, &mut b);
    assert!(back.is_empty(), "{back:?}");

    a.update_title(&item_id, "edited-on-A");
    b.update_title(&item_id, "edited-on-B");
    let (b_c1, a_c1) = exchange(&mut a, &mut b);
    assert_eq!((a_c1, b_c1), (1, 1));

    // 仅 A 仲裁「保留本机」（主人流程：电脑仲裁，手机端冲突挂着）。
    resolve_all_keep_local(&mut a);
    assert_eq!(b.unresolved_conflicts().len(), 1, "B 的冲突仍挂着");

    // 交换：A 的胜出版本（rev3）到达 B → B LWW 应用 + 挂起冲突自动过期。
    let (b_c2, a_c2) = exchange(&mut a, &mut b);
    assert_eq!((a_c2, b_c2), (0, 0), "胜出版本传播不得产生新冲突");
    assert_eq!(
        b.unresolved_conflicts().len(),
        0,
        "B 收到胜出版本后，挂着的过时冲突必须自动过期"
    );
    assert_eq!(b.item_title(&item_id), "edited-on-A", "B 收敛到 A 版本");

    // 回程：B 与 A 已一致（同 rev 同内容），零交换零冲突。
    let (b_c3, a_c3) = exchange(&mut a, &mut b);
    assert_eq!((a_c3, b_c3), (0, 0));
    assert_eq!(
        item_hash(&a.item(&item_id)).expect("hash a"),
        item_hash(&b.item(&item_id)).expect("hash b"),
        "单端 KeepLocal 后两端必须收敛一致"
    );
    assert_eq!(a.item(&item_id).rev, b.item(&item_id).rev, "两端 rev 一致");
}

/// D-111：删除冲突「保留本机」（对端删除、本机保留修改）→ 对端复活为本机版本。
///
/// 旧语义同样卡死：本机条目 rev 不变已发过、对端墓碑判定压住复活 → 一端删一端留。
/// 新语义抬 rev 传播后，对端墓碑判定失效（D-56 复活路径），全网收敛到保留版本。
#[test]
fn keep_local_on_deletion_conflict_revives_on_peer() {
    let vault_a = temp_vault("da");
    let vault_b = temp_vault("db");
    let mut a = Side::create(&vault_a, PWD_A, "device-A");
    let mut b = Side::create(&vault_b, PWD_B, "device-B");
    let host_vk = a.session.repo().expect("repo").vault_key().clone();
    b.session
        .adopt_vault_key(&host_vk)
        .expect("adopt_vault_key");

    let item_id = {
        let repo = a.session.repo_mut().expect("repo");
        repo.create_item(ItemDraft {
            kind: ItemKind::Login(LoginData {
                title: "site".to_string(),
                url: None,
                username: Some("user".to_string()),
                password: Some("pw".to_string()),
                notes: None,
                totp_secret: None,
                custom_fields: Vec::new(),
            }),
            folder_id: None,
            favorite: false,
        })
        .expect("create_item")
        .item_id
    };
    let first = pull(&mut b, &mut a);
    assert!(first.is_empty(), "{first:?}");
    let back = pull(&mut a, &mut b);
    assert!(back.is_empty(), "{back:?}");

    // A 删除（墓碑 rev2）；B 编辑（live rev2）。
    {
        let repo = a.session.repo_mut().expect("repo");
        repo.delete_item(&item_id, false).expect("delete_item");
    }
    b.update_title(&item_id, "edited-on-B");

    // 交换：B 收到墓碑 → 删除冲突（remote_deleted）；A 收到 B 的 live rev2 →
    // 墓碑判定压制（rev 相同）→ skip。
    let to_b = pull(&mut b, &mut a);
    assert_eq!(to_b.len(), 1, "B 应产生删除冲突");
    let to_a = pull(&mut a, &mut b);
    assert!(to_a.is_empty(), "A 的墓碑应压制 B 的同 rev live：{to_a:?}");

    // B 仲裁「保留本机」→ 抬 rev3 落地并传播。
    resolve_all_keep_local(&mut b);

    // A 收到 live rev3 > 墓碑 rev2 → 复活为 B 版本。
    let to_a2 = pull(&mut a, &mut b);
    assert!(to_a2.is_empty(), "复活不得产生冲突：{to_a2:?}");
    assert_eq!(
        a.item_title(&item_id),
        "edited-on-B",
        "A 必须复活为 B 的保留版本"
    );
    assert_eq!(
        item_hash(&a.item(&item_id)).expect("hash a"),
        item_hash(&b.item(&item_id)).expect("hash b"),
        "删除冲突 KeepLocal 后两端必须收敛一致"
    );

    // 稳定轮：零交换零冲突。
    let (b_c, a_c) = exchange(&mut a, &mut b);
    assert_eq!((a_c, b_c), (0, 0));
}
