//! D-189：导出保险库必须是**原子**的（`.tmp` → 长度校验 → `fsync` → `rename`）。
//!
//! 为什么这条值得单独一个测试文件：导出对话框的默认文件名恒为
//! `passwordms-export.vault`，绝大多数人就是点"保存"覆盖上一次那份。旧实现是裸
//! `std::fs::copy`——它**先把目标截断再灌数据**，中途拔盘 / 空间不足 / 进程被杀，
//! 结果是"旧副本没了，新副本也不完整"。而导出恰恰是换机、卸载、重装前的那一步，
//! 毁在这里等于把退路一起毁掉。
//!
//! 说明：真正的"写到一半断电"无法在单测里注入，这里守住的是两条可观测的合同——
//! ① 覆盖成功后目标与源逐字节一致、且不留临时文件（防"写了 tmp 忘了 rename"，
//! 那会同时产生"目标还是旧的"和"永远清不掉的垃圾"两个后果）；
//! ② 失败时不留临时文件、不破坏原目标（防"失败路径没清 tmp"，
//! 每失败一次就在用户选的目录里留一份整库大小的垃圾）。

use std::path::{Path, PathBuf};

use passwordms_core::session::Session;

/// 测试主密码（≥ 8 位）。
const MASTER: &str = "qa-export-master-pw";

fn tmp_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("passwordms_export_{tag}_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// 目录里像临时文件的名字（`copy_atomic` 产出 `<名字>.<ext>.tmp`）。
fn tmp_leftovers(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".tmp"))
        .collect()
}

fn open_session(vault: &Path) -> Session {
    Session::create(vault, MASTER, true).expect("create")
}

#[test]
fn 覆盖已有外部副本后内容与源一致且不留临时文件() {
    let dir = tmp_dir("overwrite");
    let vault = dir.join("passwordms.vault");
    let session = open_session(&vault);

    // 上一次导出的"旧的那一份"：如果实现先截断再灌，这里就是它被毁的现场。
    let dest = dir.join("passwordms-export.vault");
    std::fs::write(&dest, b"STALE OLD EXPORT").expect("stale");

    session
        .repo()
        .expect("repo")
        .export_vault(&dest, MASTER)
        .expect("export");
    drop(session);

    let src = std::fs::read(&vault).expect("read src");
    let got = std::fs::read(&dest).expect("read dest");
    assert_eq!(got, src, "导出必须是源的逐字节副本，不许残留旧内容尾巴");

    let left = tmp_leftovers(&dir);
    assert!(left.is_empty(), "导出成功后不该留临时文件：{left:?}");
}

/// **这条是分清"原子写"与"直接覆盖"的那一条。**
///
/// 把 `copy_atomic` 的临时文件路径（`<目标>.tmp`）预先占成一个目录：
/// 写临时文件必然失败。于是——
/// * 原子写：失败，**目标文件一个字节都没被碰过**（还是 `STALE`）；
/// * 裸 `fs::copy`：根本不经过临时文件，直接成功并把目标覆盖成库字节。
///
/// 也就是说，这条用例的绿/红就是"写入有没有先落到别处"的直接判据，
/// 不是"跑通了就算"。
#[test]
fn 临时路径被占时导出必须失败且旧副本原封不动() {
    let dir = tmp_dir("tmp-occupied");
    let vault = dir.join("passwordms.vault");
    let session = open_session(&vault);

    let dest = dir.join("passwordms-export.vault");
    std::fs::write(&dest, b"STALE OLD EXPORT").expect("stale");
    // `copy_atomic` 的临时名 = 目标名 + ".tmp"（扩展名换成 `<ext>.tmp`）。
    std::fs::create_dir_all(dir.join("passwordms-export.vault.tmp")).expect("occupy tmp");

    let err = session
        .repo()
        .expect("repo")
        .export_vault(&dest, MASTER)
        .expect_err("临时路径被占住时导出必须失败");
    let _ = err;
    drop(session);

    let got = std::fs::read(&dest).expect("旧副本必须还在");
    assert_eq!(
        got, b"STALE OLD EXPORT",
        "导出失败了，旧的外部副本却被改了——说明写入直接落在了目标文件上"
    );
}

#[test]
fn 导出失败时不留临时文件也不搬走原目标() {
    let dir = tmp_dir("failed");
    let vault = dir.join("passwordms.vault");
    let session = open_session(&vault);

    // 目标位置上放一个**目录**：`rename` 无论如何盖不掉它（Windows 与 Unix 都是 Err），
    // 但前面的 `fs::copy` 到一个新名字是会成功的——所以这条能测到失败路径有没有清 tmp。
    let dest = dir.join("occupied");
    std::fs::create_dir_all(&dest).expect("mkdir dest");

    let err = session
        .repo()
        .expect("repo")
        .export_vault(&dest, MASTER)
        .expect_err("目标被目录占住时必须报错，不能静默成功");
    // 错误种类不强求：这里守的是"失败"，不是"哪种失败"。
    let _ = err;
    drop(session);

    assert!(dest.is_dir(), "失败不许动原目标");
    let left = tmp_leftovers(&dir);
    assert!(
        left.is_empty(),
        "失败路径必须清掉临时文件，否则每失败一次留一份整库大小的垃圾：{left:?}"
    );
}
