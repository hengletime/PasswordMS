//! 容器读写往返 + 单条更新字节差异（架构文档 §8.4，验收点 A-01 / A-02）。
//!
//! 状态：已在本机实跑通过（`cargo test -p passwordms-core`，见 docs/03b-verification.md）。

use std::path::{Path, PathBuf};

use passwordms_core::constants::{
    CONTAINER_PREFIX_LEN, FLAG_TOMBSTONE, FRAME_HEADER_LEN, HEADER_RESERVED_BYTES,
};
use passwordms_core::crypto::keys::MasterKey;
use passwordms_core::crypto::{kdf, keys, rng, verify};
use passwordms_core::error::CoreError;
use passwordms_core::model::item::{CustomField, Item, ItemKind, LoginData};
use passwordms_core::model::now_ms;
use passwordms_core::model::vault::Vault;
use passwordms_core::store::container::Container;
use passwordms_core::store::frame::Frame;
use passwordms_core::store::repository::{ItemDraft, Repository};
use zeroize::Zeroizing;

fn tmp_vault_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "passwordms_test_{name}_{}.vault",
        uuid::Uuid::new_v4()
    ));
    p
}

fn build_header(kek: &keys::Kek, vk: &[u8; 32]) -> Vault {
    let salt = rng::random_salt16().expect("salt").to_vec();
    let wrapped = keys::wrap_vault_key(kek, vk).expect("wrap");
    let verify_key = Zeroizing::new([7u8; 32]);
    let mac = verify::compute_verify_mac(&verify_key).expect("mac");
    Vault::new(
        "00000000-0000-4000-8000-000000000000".to_string(),
        salt,
        wrapped,
        mac,
        now_ms(),
    )
}

fn login_draft(title: &str) -> ItemDraft {
    ItemDraft {
        kind: ItemKind::Login(LoginData {
            title: title.to_string(),
            url: Some("https://example.invalid".to_string()),
            username: Some("qa_test_user".to_string()),
            password: Some("qa_test_password".to_string()),
            notes: None,
            totp_secret: None,
            custom_fields: vec![CustomField {
                key: "qa_test_field".to_string(),
                value: "v".to_string(),
                hidden: false,
            }],
        }),
        folder_id: None,
        favorite: false,
    }
}

/// 容器中解析出的一帧（用于 A-02 的字节级比对）。
struct ParsedFrame {
    offset: usize,
    record_id: String,
    flags: u8,
    bytes: Vec<u8>,
}

/// 从容器文件字节中解析记录区全部帧（`(offset, record_id, flags, 帧字节)`）。
///
/// 头部长度取自文件第 10..12 字节（`header_len`），记录区起点 = `CONTAINER_PREFIX_LEN + header_len`。
fn parse_frames(data: &[u8]) -> Vec<ParsedFrame> {
    assert!(data.len() >= CONTAINER_PREFIX_LEN, "文件短于容器前缀");
    let header_len = u16::from_le_bytes([data[10], data[11]]) as usize;
    let start = CONTAINER_PREFIX_LEN + header_len;
    let mut out = Vec::new();
    let mut cursor = start;
    while cursor + FRAME_HEADER_LEN <= data.len() {
        match Frame::decode(&data[cursor..]) {
            Ok((frame, consumed)) if consumed > 0 => {
                out.push(ParsedFrame {
                    offset: cursor,
                    record_id: frame.record_id_string(),
                    flags: frame.flags,
                    bytes: data[cursor..cursor + consumed].to_vec(),
                });
                cursor += consumed;
            }
            // 记录区之后无有效帧（尾部无填充，此处仅作安全退出）。
            _ => break,
        }
    }
    out
}

/// 取指定条目对应的帧。
fn frame_for<'a>(frames: &'a [ParsedFrame], item_id: &str) -> &'a ParsedFrame {
    frames
        .iter()
        .find(|f| f.record_id == item_id)
        .expect("应存在该条目的帧")
}

#[test]
fn container_roundtrip_and_minimal_rewrite() {
    let path = tmp_vault_path("roundtrip");
    let mk: MasterKey = Zeroizing::new(rng::random_key().expect("mk"));
    let kek = kdf::derive_kek(&mk[..]).expect("kek");
    let vk = rng::random_key().expect("vk");
    let header = build_header(&kek, &vk);

    let mut repo = Repository::create(
        &path,
        header,
        Zeroizing::new(vk),
        "qa_test_device".to_string(),
    )
    .expect("create");

    let item_a = repo.create_item(login_draft("qa_test_alpha")).expect("a");
    let item_b = repo.create_item(login_draft("qa_test_beta")).expect("b");
    assert_ne!(item_a.item_id, item_b.item_id);

    let frames_before = parse_frames(&std::fs::read(&path).expect("read"));
    let a_before_offset = frame_for(&frames_before, &item_a.item_id).offset;
    let size_before = repo.file_size().expect("size");

    // 该更新改变了 `favorite`（false→true）→ 序列化 JSON 长度变化 → 密文长度变化
    // → 走 `place_frame` 的「旧帧置墓碑 + 尾部追加」分支（**非**原位写入）。
    let mut draft = login_draft("qa_test_alpha");
    draft.favorite = true;
    let updated = repo.update_item(&item_a.item_id, draft).expect("update");
    assert_eq!(updated.item_id, item_a.item_id);
    assert!(updated.favorite);
    assert!(updated.rev > item_a.rev);

    let size_after = repo.file_size().expect("size");
    // A-02：改 1 条不应产生接近「整库」的字节增长。
    assert!(
        size_after <= size_before + 4096,
        "单条更新增长 {size_after} - {size_before} 字节，疑似重写全库"
    );

    // 追加分支的可观测后果：帧总数 +1，且旧帧在原偏移被置为墓碑（flags bit0）。
    let frames_after = parse_frames(&std::fs::read(&path).expect("read"));
    assert_eq!(
        frames_after.len(),
        frames_before.len() + 1,
        "长度变化的更新应产生「墓碑 + 追加」两处写入"
    );
    let tomb = frames_after
        .iter()
        .find(|f| f.offset == a_before_offset)
        .expect("旧帧槽位应仍在");
    assert!(
        tomb.flags & FLAG_TOMBSTONE != 0,
        "旧帧应被置为墓碑（flags bit0）"
    );

    // 密文中不应出现明文密码（A-01 的粗粒度检查）。
    let bytes = std::fs::read(&path).expect("read");
    let needle = b"qa_test_password";
    assert!(
        !bytes.windows(needle.len()).any(|w| w == needle),
        "库文件中出现明文密码"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn single_item_update_hits_in_place_branch_and_leaves_others_intact() {
    // A-02 强断言：**密文长度不变**时，目标帧必须在**原偏移被原地覆写**，
    // 且未改动记录的帧字节**完全一致**、文件总长度不变。
    let path = tmp_vault_path("inplace");
    let mk: MasterKey = Zeroizing::new(rng::random_key().expect("mk"));
    let kek = kdf::derive_kek(&mk[..]).expect("kek");
    let vk = rng::random_key().expect("vk");
    let header = build_header(&kek, &vk);

    let mut repo = Repository::create(
        &path,
        header,
        Zeroizing::new(vk),
        "qa_test_device".to_string(),
    )
    .expect("create");
    let item_a = repo.create_item(login_draft("qa_test_alpha")).expect("a");
    let item_b = repo.create_item(login_draft("qa_test_beta")).expect("b");

    let bytes_before = std::fs::read(&path).expect("read");
    let frames_before = parse_frames(&bytes_before);
    let a_before = frame_for(&frames_before, &item_a.item_id).clone_bytes();
    let b_before = frame_for(&frames_before, &item_b.item_id).clone_bytes();

    // 构造「密文长度不变」的更新：仅把 password 换成**等长**另一串，其余字段逐一保持原值。
    // 序列化 JSON 字节数不变 → 帧 `encoded_len()` 不变 → 命中 `place_frame` 的**原位写入**分支。
    let mut draft = login_draft("qa_test_alpha");
    match &mut draft.kind {
        ItemKind::Login(login) => login.password = Some("qa_test_PASSWORD".to_string()),
        _ => unreachable!("login_draft 必为 login"),
    }
    let updated = repo.update_item(&item_a.item_id, draft).expect("update");
    assert_eq!(updated.rev, 2, "rev 应仅 +1（一位数 → 长度不变）");

    let bytes_after = std::fs::read(&path).expect("read");
    let frames_after = parse_frames(&bytes_after);
    let a_after = frame_for(&frames_after, &item_a.item_id).clone_bytes();
    let b_after = frame_for(&frames_after, &item_b.item_id).clone_bytes();

    // 文件总长度不变（未追加帧、未整库重写）。
    assert_eq!(
        bytes_after.len(),
        bytes_before.len(),
        "等长更新不应改变文件总大小"
    );
    // 帧数不变（无墓碑、无追加）。
    assert_eq!(
        frames_after.len(),
        frames_before.len(),
        "等长更新不应新增帧"
    );
    // 目标帧：同一偏移被覆写，字节已变化。
    assert_eq!(a_after.0, a_before.0, "目标帧应在原偏移被原地覆写");
    assert_ne!(a_after.1, a_before.1, "目标帧字节应已更新");
    // 未改动记录：偏移与字节完全一致。
    assert_eq!(b_after.0, b_before.0, "无关帧偏移不应变化");
    assert_eq!(b_after.1, b_before.1, "无关帧字节应完全不变");

    let _ = std::fs::remove_file(&path);
}

impl ParsedFrame {
    /// 取 `(offset, 帧字节)`。
    fn clone_bytes(&self) -> (usize, Vec<u8>) {
        (self.offset, self.bytes.clone())
    }
}

#[test]
fn item_json_roundtrip() {
    let item = Item {
        item_id: "11111111-1111-4111-8111-111111111111".to_string(),
        kind: ItemKind::Login(LoginData {
            title: "qa_test_title".to_string(),
            url: None,
            username: None,
            password: None,
            notes: None,
            totp_secret: None,
            custom_fields: Vec::new(),
        }),
        folder_id: None,
        favorite: false,
        created_at: 0,
        updated_at: 0,
        rev: 1,
        modified_by: "qa_test".to_string(),
        attachments: Vec::new(),
        pinned: false,
        pin_order: 0,
        use_count: 0,
        last_used_at: None,
    };
    let json = serde_json::to_string(&item).expect("ser");
    let back: Item = serde_json::from_str(&json).expect("de");
    assert_eq!(item, back);
    // 内部标签为 `type`，取值 snake_case。
    assert!(json.contains("\"type\":\"login\""));
}

// ===== D-78：头部容量增长的上界与失败语义 =====
//
// 缺陷：`update_header` 的翻倍没有上界，`65536 as u16 == 0` 写进 `[10..12)` 后
// `open` 判定 `zero header` → 库永久打不开；而本次运行一切正常（内存里容量是对的
// 65536），用户要重启才发现。以下 4 例钉住"能长多少"、"长不动时怎么失败"、
// "失败时磁盘必须一字未动"。

/// 建库并写入两个条目（记录区非空，供"帧数不变"断言），随后释放 `Repository`。
fn seed_vault(path: &Path) {
    let mk: MasterKey = Zeroizing::new(rng::random_key().expect("mk"));
    let kek = kdf::derive_kek(&mk[..]).expect("kek");
    let vk = rng::random_key().expect("vk");
    let header = build_header(&kek, &vk);
    {
        let mut repo = Repository::create(
            path,
            header,
            Zeroizing::new(vk),
            "qa_test_device".to_string(),
        )
        .expect("create");
        repo.create_item(login_draft("hdr_alpha")).expect("alpha");
        repo.create_item(login_draft("hdr_beta")).expect("beta");
    }
}

/// 构造一个**序列化后 JSON 恰好 `n` 字节**的头部。
///
/// 撑长手段：往 `vault_id` 尾部追加 `'a'`。选它是因为 `'a'` 不需要 JSON 转义，
/// 序列化后 1 字节对 1 字节，长度可精确控制；容器层不校验 `vault_id` 是否为 UUID
/// （本 helper 只用于把头部撑到指定长度，不参与任何业务断言）。
///
/// 末尾自校一次，避免 helper 自己算错导致用例在测别的容量段。
fn header_with_json_len(base: &Vault, n: usize) -> Vault {
    let mut v = base.clone();
    let skeleton = serde_json::to_vec(&v).expect("serialize").len();
    assert!(
        n >= skeleton,
        "目标 JSON 长度 {n} 小于骨架长度 {skeleton}，无法靠追加达到"
    );
    v.vault_id.push_str(&"a".repeat(n - skeleton));
    assert_eq!(
        serde_json::to_vec(&v).expect("serialize").len(),
        n,
        "helper 自校失败：JSON 长度不等于目标 {n}"
    );
    v
}

/// 文件 `[10..12)` 的 `header_len`（== 头部容量）。
fn capacity_on_disk(path: &Path) -> u16 {
    let data = std::fs::read(path).expect("read");
    u16::from_le_bytes([data[10], data[11]])
}

/// 按文件里的 `header_len` 解析出的记录区帧数。
fn frame_count(path: &Path) -> usize {
    parse_frames(&std::fs::read(path).expect("read")).len()
}

/// **T1（防过修）**：常规翻倍没被上界改坏。
#[test]
fn header_grows_to_next_capacity_and_reopens() {
    let path = tmp_vault_path("hdr_grow");
    seed_vault(&path);

    let mut container = Container::open(&path).expect("open");
    assert_eq!(
        container.record_start(),
        CONTAINER_PREFIX_LEN as u64 + HEADER_RESERVED_BYTES as u64,
        "新建库容量应为 HEADER_RESERVED_BYTES"
    );
    let frames_before = frame_count(&path);
    assert!(frames_before >= 2, "构造前提：记录区至少两个条目帧");
    let base = container.header().clone();

    // 8192 装不下 9000 → 翻倍到 16384。
    assert!(
        container
            .update_header(header_with_json_len(&base, 9000))
            .expect("容量不足应成功翻倍"),
        "发生整文件重写时必须返回 true"
    );

    assert_eq!(capacity_on_disk(&path), 16384, "`[10..12)` 应写入 16384");
    let reopened = Container::open(&path).expect("翻倍后必须能重新打开");
    assert_eq!(
        reopened.record_start(),
        CONTAINER_PREFIX_LEN as u64 + 16384,
        "记录区偏移应随容量移动"
    );
    assert_eq!(frame_count(&path), frames_before, "翻倍不得丢帧");

    let _ = std::fs::remove_file(&path);
}

/// **T2**：涨到超过 `u16::MAX` 时**必须失败且磁盘一字未动**。
///
/// 这条覆盖的是"夹到 65535 仍装不下"的路径（JSON 70000）。
#[test]
fn header_growth_past_u16_ceiling_fails_without_corruption() {
    let path = tmp_vault_path("hdr_overflow");
    seed_vault(&path);

    let mut container = Container::open(&path).expect("open");
    let base = container.header().clone();

    // 一段一段爬到 32768（一次最多翻一倍，且必须踩在 (cap, 2*cap] 区间内）。
    assert!(container
        .update_header(header_with_json_len(&base, 9000))
        .expect("8192→16384"));
    assert_eq!(capacity_on_disk(&path), 16384);
    assert!(container
        .update_header(header_with_json_len(&base, 20000))
        .expect("16384→32768"));
    assert_eq!(capacity_on_disk(&path), 32768);
    let frames_before = frame_count(&path);

    // 70000 > u16::MAX：夹到 65535 也装不下 → 拒绝，而不是写一个截断的容量。
    let err = container
        .update_header(header_with_json_len(&base, 70000))
        .expect_err("头部超过 u16 上限时必须拒绝");
    assert!(
        matches!(err, CoreError::VaultCapacityExceeded),
        "错误变体必须是 VaultCapacityExceeded，实际 code = {}",
        err.code()
    );

    // 核心断言：**失败后库仍可打开，且容量/偏移/帧数全部未变**。
    // 只断言 `is_err()` 对"先写坏再报错"完全无感。
    assert_eq!(capacity_on_disk(&path), 32768, "失败时容量字段不得变化");
    let reopened = Container::open(&path).expect("失败后库必须仍可打开");
    assert_eq!(
        reopened.record_start(),
        CONTAINER_PREFIX_LEN as u64 + 32768,
        "失败时记录区偏移不得变化"
    );
    assert_eq!(frame_count(&path), frames_before, "失败时帧数不得变化");

    let _ = std::fs::remove_file(&path);
}

/// **T3**：恰好 65535（`u16::MAX`）必须能写进去并往返成功 —— 上界是闭区间。
#[test]
fn header_at_exact_u16_max_roundtrips() {
    let path = tmp_vault_path("hdr_max_ok");
    seed_vault(&path);

    let mut container = Container::open(&path).expect("open");
    let base = container.header().clone();
    assert!(container
        .update_header(header_with_json_len(&base, 9000))
        .expect("8192→16384"));
    assert!(container
        .update_header(header_with_json_len(&base, 20000))
        .expect("16384→32768"));
    assert_eq!(capacity_on_disk(&path), 32768);
    let frames_before = frame_count(&path);

    // 32768 → min(65536, 65535) = 65535，JSON 恰好 65535（`>` 严格判超限 → 不算超）。
    assert!(
        container
            .update_header(header_with_json_len(&base, u16::MAX as usize))
            .expect("恰好 u16::MAX 应成功"),
        "发生整文件重写时必须返回 true"
    );

    assert_eq!(capacity_on_disk(&path), 65535, "`[10..12)` 应写入 65535");
    let reopened = Container::open(&path).expect("65535 容量必须能重新打开");
    assert_eq!(
        reopened.record_start(),
        CONTAINER_PREFIX_LEN as u64 + 65535,
        "记录区偏移应为 16 + 65535"
    );
    assert_eq!(frame_count(&path), frames_before, "容量变化不得丢帧");

    let _ = std::fs::remove_file(&path);
}

/// **T4**：已在硬上限时再要求增长 → 立即失败（`new_capacity` 没有进展那条分支）。
///
/// 与 T2 是**两条不同的失败路径**：T2 是"夹住后仍装不下"，T4 是"夹住后等于原容量"。
#[test]
fn header_growth_at_max_capacity_fails_fast() {
    let path = tmp_vault_path("hdr_max_full");
    seed_vault(&path);

    let mut container = Container::open(&path).expect("open");
    let base = container.header().clone();
    assert!(container
        .update_header(header_with_json_len(&base, 9000))
        .expect("8192→16384"));
    assert!(container
        .update_header(header_with_json_len(&base, 20000))
        .expect("16384→32768"));
    // 32768 → 65535
    assert!(container
        .update_header(header_with_json_len(&base, 40000))
        .expect("32768→65535"));
    assert_eq!(capacity_on_disk(&path), 65535);
    let frames_before = frame_count(&path);

    let err = container
        .update_header(header_with_json_len(&base, 66000))
        .expect_err("已在硬上限时必须拒绝增长");
    assert!(
        matches!(err, CoreError::VaultCapacityExceeded),
        "错误变体必须是 VaultCapacityExceeded，实际 code = {}",
        err.code()
    );

    assert_eq!(capacity_on_disk(&path), 65535, "失败时容量字段不得变化");
    let reopened = Container::open(&path).expect("失败后库必须仍可打开");
    assert_eq!(
        reopened.record_start(),
        CONTAINER_PREFIX_LEN as u64 + 65535,
        "失败时记录区偏移不得变化"
    );
    assert_eq!(frame_count(&path), frames_before, "失败时帧数不得变化");

    let _ = std::fs::remove_file(&path);
}
