//! D-172 导入原子性回归：**全成或全无**，失败后同一份预览可直接重试。
//!
//! 背景（Kimi 审计 F-07 / F-08）：旧实现边算边逐条 `put_folder_direct` /
//! `put_item_direct`，每条都 `sync_all`。第 700 条失败时前 699 条**已经落盘**，
//! 而调用方只拿到一个 Err——界面说「导入失败」，库里却多了半批数据；用户重导时
//! 又被同名查重挡掉一部分，结果无从解释。命令层则更早一步把预览缓存删掉，
//! 于是「失败」连带把「重试的凭据」一起消费掉了。
//!
//! 本套测试钉四件事：
//!
//! 1. **校验类失败零写入**：有一条不合格（明文超限），整批一条都不落——
//!    内存列表为空、文件字节数一字不差；
//! 2. **成功路径一次落盘**：条目与分组都在，重开库能读回，**头部计数与实扫一致**
//!    （`persist_header` 真的跑了，不是靠下次启动自愈）；
//! 3. **失败后可重试**：换成合格预览重试即成功（对应用户点「重试」）；
//! 4. **写盘失败不留痕**（Windows 只读库文件）：apply 报错、库内容与内存都不变，
//!    恢复可写后原样重试成功。

use std::path::{Path, PathBuf};

use passwordms_core::crypto::keys::MasterKey;
use passwordms_core::crypto::{kdf, keys, rng, verify};
use passwordms_core::import::{self, ImportFormat};
use passwordms_core::model::folder::Folder;
use passwordms_core::model::item::Item;
use passwordms_core::model::now_ms;
use passwordms_core::model::vault::Vault;
use passwordms_core::store::repository::Repository;
use zeroize::Zeroizing;

const CSV_HEADER: &str =
    "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields";

fn tmp_vault_path(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "passwordms_test_import_atomic_{name}_{}.vault",
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

fn fresh_repo(name: &str) -> (PathBuf, Repository) {
    let path = tmp_vault_path(name);
    let mk: MasterKey = Zeroizing::new(rng::random_key().expect("mk"));
    let kek = kdf::derive_kek(&mk[..]).expect("kek");
    let vk = rng::random_key().expect("vk");
    let repo = Repository::create(
        &path,
        build_header(&kek, &vk),
        Zeroizing::new(vk),
        "qa_test_device".to_string(),
    )
    .expect("create repo");
    (path, repo)
}

/// CSV 行（folder / favorite / type / name / notes / uri / username / password / totp / fields）。
fn csv_row(folder: &str, kind: &str, name: &str) -> String {
    format!("{folder},0,{kind},{name},,,,,,\n")
}

fn parse_csv(rows: &[String]) -> import::ImportPreview {
    let mut csv = String::from(CSV_HEADER);
    csv.push('\n');
    for row in rows {
        csv.push_str(row);
    }
    import::preview_from_str(&csv, ImportFormat::Csv).expect("测试 CSV 必须可解析")
}

/// 直接读文件头部的明文 JSON 取 `item_count`。
///
/// 不用「重开库后数条目」代替：那样只能证明记录写进去了，证明不了
/// `persist_header` 跑过——头部计数停在旧值时会留下一条
/// `header_item_count_mismatch` 警告，直到下次解锁被自愈。这里要钉的正是「写完就补上」。
fn header_item_count(path: &Path) -> u32 {
    let bytes = std::fs::read(path).expect("read vault");
    let header_len = u16::from_le_bytes([bytes[10], bytes[11]]) as usize;
    let header: Vault = serde_json::from_slice(&bytes[16..16 + header_len]).expect("header json");
    header.item_count
}

fn titles(items: &[Item]) -> Vec<String> {
    items.iter().map(|it| it.kind.title().to_string()).collect()
}

#[test]
fn 超限条目_整批一条都不落库() {
    let (path, mut repo) = fresh_repo("oversize");
    let before = std::fs::metadata(&path).expect("meta").len();

    // 中间那条超过 384 KiB 明文上限：旧实现会先把「正常条目甲」落盘再报错。
    let long_notes = "x".repeat(400_000);
    let preview = parse_csv(&[
        csv_row("工作", "login", "正常条目甲"),
        format!("工作,0,login,超长条目,{long_notes},,,,,\n"),
        csv_row("工作", "login", "正常条目乙"),
    ]);

    let err = import::apply(&preview, &mut repo).expect_err("超限条目必须让整批失败");
    // `CoreError` 的 Debug 只打错误码，成因在 `Validation(detail)` 里，必须解构取。
    match err {
        passwordms_core::error::CoreError::Validation(detail) => assert!(
            detail.contains("item_too_large"),
            "错误成因要说清楚是明文超限，实际 {detail}"
        ),
        other => panic!("期望 Validation(item_too_large)，实际 {other:?}"),
    }

    // 内存：一条都没有（不只是「没写盘」）。
    assert!(
        repo.list_items(None, false, None).is_empty(),
        "失败后内存里也不得残留半批条目"
    );
    assert!(repo.list_folders().is_empty(), "分组同样不得残留");
    // 文件：字节数一字不差。
    assert_eq!(
        std::fs::metadata(&path).expect("meta").len(),
        before,
        "校验失败必须在写盘之前挡住，文件不得增长"
    );

    // 重开库再确认一次（排除「只在内存里被藏起来」的可能）。
    let reopened = {
        let vk = repo.vault_key().clone();
        Repository::open(&path, vk, "qa_test_device".to_string()).expect("reopen")
    };
    assert!(reopened.list_items(None, false, None).is_empty());
    assert_eq!(header_item_count(&path), 0, "头部计数保持 0");
}

#[test]
fn 失败后换合格预览重试_一次成功() {
    let (_path, mut repo) = fresh_repo("retry");
    let long_notes = "x".repeat(400_000);
    let bad = parse_csv(&[format!("工作,0,login,超长条目,{long_notes},,,,,\n")]);
    import::apply(&bad, &mut repo).expect_err("超限必须失败");
    assert!(repo.list_items(None, false, None).is_empty());

    // 用户改好文件后再导（对应 F-08 的「重试」）：库是干净的，所以能成功。
    let good = parse_csv(&[
        csv_row("工作", "login", "条目甲"),
        csv_row("工作", "secure note", "笔记乙"),
    ]);
    let result = import::apply(&good, &mut repo).expect("重试必须成功");
    assert_eq!(result.imported_items, 2);
    assert_eq!(result.imported_folders, 1, "上次失败没留下同名的空分组");

    let folders: Vec<Folder> = repo.list_folders();
    assert_eq!(folders.len(), 1, "同名分组不得因上次失败而重复创建");
    assert_eq!(folders[0].name, "工作");
}

#[test]
fn 成功批次_条目分组与头部计数一起落盘() {
    let (path, mut repo) = fresh_repo("landed");
    let preview = parse_csv(&[
        csv_row("工作", "login", "条目甲"),
        csv_row("工作", "login", "条目乙"),
        csv_row("", "secure note", "笔记丙"),
    ]);

    let result = import::apply(&preview, &mut repo).expect("apply");
    assert_eq!(result.imported_items, 3);
    assert_eq!(result.imported_folders, 1);
    assert_eq!(result.merged_folders, 0);
    assert_eq!(result.duplicate_skipped, 0);

    assert_eq!(
        header_item_count(&path),
        3,
        "persist_header 必须随批次一起落盘（否则下次解锁报计数不一致）"
    );

    let vk = repo.vault_key().clone();
    let mut reopened = Repository::open(&path, vk, "qa_test_device".to_string()).expect("reopen");
    let got = titles(&reopened.list_items(None, false, None));
    assert_eq!(got.len(), 3);
    for want in ["条目甲", "条目乙", "笔记丙"] {
        assert!(got.iter().any(|t| t == want), "{want} 未落盘，实际 {got:?}");
    }
    assert_eq!(reopened.list_folders().len(), 1);
    let warnings = reopened.take_open_warnings();
    assert!(
        warnings.is_empty(),
        "整批写入后重开库不该有任何一致性警告，实际 {warnings:?}"
    );
}

#[test]
fn 同一预览重试_第二次全部跳过且不写盘() {
    let (path, mut repo) = fresh_repo("idempotent");
    let preview = parse_csv(&[
        csv_row("工作", "login", "条目甲"),
        csv_row("工作", "login", "条目乙"),
    ]);
    import::apply(&preview, &mut repo).expect("第一次");
    // 比字节而不是比长度：空批次若照样跑 `persist_header`，文件长度一字不变，
    // 只有头部的 `updated_at` 变了——只比长度会放过这种「写了个没意义的更新」。
    let after_first = std::fs::read(&path).expect("read vault");

    // 全部命中查重 → 没有可写的记录 → 空批次直接返回，不碰文件。
    let second = import::apply(&preview, &mut repo).expect("第二次");
    assert_eq!(second.imported_items, 0);
    assert_eq!(second.duplicate_skipped, 2);
    assert_eq!(second.merged_folders, 1);
    assert!(
        std::fs::read(&path).expect("read vault") == after_first,
        "零新增不该产生任何写入（连头部 updated_at 都不动）"
    );
    assert_eq!(repo.list_items(None, false, None).len(), 2);
}

#[cfg(windows)]
#[test]
fn 写盘失败_库与内存都保持原样_恢复可写后重试成功() {
    let (path, mut repo) = fresh_repo("readonly");
    let before = std::fs::metadata(&path).expect("meta").len();
    let preview = parse_csv(&[csv_row("工作", "login", "条目甲")]);

    // Windows 上只读属性会让 `OpenOptions::append` 直接失败——真实的写盘错误，
    // 不是模拟。create/update 走的也是同一条打开路径。
    let mut perm = std::fs::metadata(&path).expect("meta").permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&path, perm).expect("set readonly");
    let outcome = import::apply(&preview, &mut repo);

    // 先恢复可写，再断言（否则失败信息里会混进一堆权限噪音）。
    let mut perm = std::fs::metadata(&path).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&path, perm).expect("set writable");

    assert!(outcome.is_err(), "只读库文件必须让导入失败");
    assert!(
        repo.list_items(None, false, None).is_empty(),
        "写盘失败不得在内存里留下条目"
    );
    assert!(repo.list_folders().is_empty());
    assert_eq!(
        std::fs::metadata(&path).expect("meta").len(),
        before,
        "写盘失败不得改动库文件"
    );

    // 恢复可写后原样重试：这才是 F-08 要的「不必重新选文件」。
    let result = import::apply(&preview, &mut repo).expect("恢复可写后重试必须成功");
    assert_eq!(result.imported_items, 1);
    assert_eq!(header_item_count(&path), 1);
    assert_eq!(repo.list_items(None, false, None).len(), 1);
}
