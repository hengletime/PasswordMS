//! D-87 导入源编码容错：`decode_source` + `preview_from_file` 端到端。
//!
//! 真实产源（主人实测踩坑）：中文 Windows 上 Excel / WPS 编辑 CSV 保存为 ANSI（GBK），
//! 旧实现严格 UTF-8 → `ERR_IO`「文件读写出错」（错误归类也错）。本套测试钉死：
//! GBK / UTF-16 BOM 自动转换、UTF-8 BOM 剥离、二进制垃圾显式报 `ImportParse`、
//! 纯 UTF-8 行为不变（快路径，不许回归）。

use passwordms_core::error::CoreError;
use passwordms_core::import::{decode_source, preview_from_file, preview_from_str, ImportFormat};

const CSV_OK: &str = "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields\n示例分组,0,login,示例条目,备注,user@example.com,Pw123456!,,\n";

fn gbk(s: &str) -> Vec<u8> {
    let (cow, _, had_errors) = encoding_rs::GBK.encode(s);
    assert!(!had_errors, "测试数据必须是合法 GBK");
    cow.into_owned()
}

#[test]
fn gbk_csv_解码后可完整解析() {
    let bytes = gbk(CSV_OK);
    let text = decode_source(&bytes).expect("GBK 必须自动转换");
    assert_eq!(text, CSV_OK);
    let preview = preview_from_str(&text, ImportFormat::Csv).expect("转换后必须能解析");
    assert_eq!(preview.items.len(), 1);
    assert_eq!(preview.skipped, 0);
}

#[test]
fn gbk_csv_从文件端到端导入预览() {
    let mut path = std::env::temp_dir();
    path.push(format!("pms-d87-gbk-{}.csv", std::process::id()));
    std::fs::write(&path, gbk(CSV_OK)).expect("写临时文件");
    let preview = preview_from_file(&path, ImportFormat::Csv).expect("GBK 文件必须可预览");
    std::fs::remove_file(&path).ok();
    assert_eq!(preview.items.len(), 1);
    assert_eq!(preview.folders.len(), 1);
}

#[test]
fn utf16le_bom_剥离且可解析() {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in CSV_OK.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let text = decode_source(&bytes).expect("UTF-16LE BOM 必须自动转换");
    assert_eq!(text, CSV_OK, "BOM 必须剥离（污染首列名 = folder 映射失败）");
    let preview = preview_from_str(&text, ImportFormat::Csv).expect("转换后必须能解析");
    assert_eq!(preview.items.len(), 1);
}

#[test]
fn utf16be_bom_剥离且可解析() {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in CSV_OK.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    let text = decode_source(&bytes).expect("UTF-16BE BOM 必须自动转换");
    assert_eq!(text, CSV_OK);
}

#[test]
fn utf8_bom_剥离() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(CSV_OK.as_bytes());
    let text = decode_source(&bytes).expect("UTF-8 BOM 必须剥离");
    assert_eq!(text, CSV_OK, "BOM 留在串首会污染首列名 folder");
}

#[test]
fn 纯_utf8_行为不变() {
    let text = decode_source(CSV_OK.as_bytes()).expect("纯 UTF-8 走快路径");
    assert_eq!(text, CSV_OK);
}

#[test]
fn 二进制垃圾_显式报解析错误() {
    // 0xFF：UTF-8 非法首字节，GBK 非法字节（lead 上限 0xFE）——两端都必错，判定确定。
    let err = decode_source(&[0xFF]).expect_err("无法映射的字节必须报错");
    match err {
        CoreError::ImportParse(stage) => assert_eq!(stage, "encoding"),
        other => panic!("应报 ImportParse，实际 {other:?}"),
    }
}

#[test]
fn 错误码走_import_parse_而非_io() {
    // 回归钉：编码失败不得再落进 ERR_IO（旧版「文件读写出错」误导排查方向）。
    let garbage: Vec<u8> = vec![0xFF, 0xFE, 0x81, 0x40, 0xFF, 0xFF, 0x00];
    let err = decode_source(&garbage).expect_err("坏 UTF-16 必须报错");
    assert!(matches!(err, CoreError::ImportParse(ref s) if s == "encoding"));
    assert_eq!(err.code(), "ERR_IMPORT_PARSE");
    assert_eq!(err.message_key(), "err.import_parse");
}
