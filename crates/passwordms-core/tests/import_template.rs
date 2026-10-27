//! 导入模板回归（D-72；D-112 文本参数化改造）。
//!
//! `save_import_template` 把 [`csv_template`] 的产物写到用户选定位置，用户照着填再导回本应用。
//! 所以模板与解析器之间存在三条**必须锁死**的契约：
//!
//! 1. **能导入**：模板自身解析后 `total == 2` / `items.len() == 2` / `skipped == 0`
//!    —— 否则用户拿到的模板本身就是「坏样例」，第一个动作就失败；
//! 2. **表头顺序 == [`KNOWN_COLUMNS`]**：列名映射靠表头，但顺序是给人看的；
//!    模板改了而解析器没跟上时，示例值会静默落到别的列（最难自查的一类错）；
//! 3. **示例行不溢出到 `custom_fields`**：列数与表头严格相等。多一列就会被当成
//!    「用户自加的自定义字段」，把模板示范污染成脏数据。
//!
//! D-112：示例文本改由前端 i18n 传入——用**中文与泰文两组文本**各跑一遍全量契约，
//! 证明「结构不随语言漂移」；引号/换行注入由 `template.rs` 单元测试守门。

use passwordms_core::import::bitwarden_csv::{self, KNOWN_COLUMNS};
use passwordms_core::import::template::{csv_template, TemplateTexts};
use passwordms_core::model::item::ItemKind;

/// 中文示例文本（与旧版内置模板逐字一致，回归基线）。
fn zh_texts() -> TemplateTexts {
    TemplateTexts::new(
        "示例分组".into(),
        "示例登录条目".into(),
        "备注里可以有半角逗号, 但整段必须用英文双引号包住".into(),
        "示例安全笔记".into(),
        "笔记正文写在这里".into(),
    )
    .unwrap()
}

/// 泰文示例文本（D-112 本地化主场景）。
fn th_texts() -> TemplateTexts {
    TemplateTexts::new(
        "โฟลเดอร์ตัวอย่าง".into(),
        "รายการเข้าสู่ระบบตัวอย่าง".into(),
        "หมายเหตุใส่เครื่องหมายจุลภาค (comma) ได้, แต่ต้องครอบด้วยเครื่องหมายอัญประกาศ".into(),
        "โน้ตความปลอดภัยตัวอย่าง".into(),
        "เนื้อหาเขียนที่นี่".into(),
    )
    .unwrap()
}

#[test]
fn template_parses_with_zero_skipped() {
    let csv = csv_template(&zh_texts());
    let preview = bitwarden_csv::parse(&csv).expect("内置模板必须能被自己的解析器导入");

    assert_eq!(preview.total, 2, "模板应含 2 个数据行");
    assert_eq!(preview.items.len(), 2, "2 行都应被接受（无一被跳）");
    assert_eq!(preview.skipped, 0, "模板不得出现被跳过的行");
    assert!(
        preview.skipped_by_type.is_empty(),
        "模板导入不得产生任何 skip 分类，实际: {:?}",
        preview.skipped_by_type
    );
    assert_eq!(
        preview.folders.len(),
        1,
        "只有第 1 行带分组，应创建 1 个分组"
    );
    assert_eq!(preview.folders[0].name, "示例分组");

    // 两条示例行分别是 login / secure note，且引号包裹的 notes 被正确剥引号保留内容。
    match &preview.items[0].kind {
        ItemKind::Login(login) => {
            assert_eq!(login.title, "示例登录条目");
            assert_eq!(
                login.notes.as_deref(),
                Some("备注里可以有半角逗号, 但整段必须用英文双引号包住"),
                "引号包裹的字段必须剥掉引号并完整保留内容（含其中的半角逗号）"
            );
            // 模板必须真的含**半角**逗号，否则「引号包裹」这一行等于没示范：
            // 旧版模板用的是全角「，」（不是 CSV 分隔符），删掉引号解析结果完全一样，
            // 谁也不会发现转义路径其实从没被验证过。
            assert!(
                login.notes.as_deref().is_some_and(|n| n.contains(',')),
                "模板 notes 必须含半角逗号，否则引号包裹没有演示任何东西"
            );
            assert_eq!(login.url.as_deref(), Some("https://example.com"));
            assert_eq!(login.username.as_deref(), Some("user@example.com"));
            assert_eq!(login.password.as_deref(), Some("ChangeMe123!"));
        }
        other => panic!("第 1 行应为 Login，实际: {other:?}"),
    }
    assert!(
        preview.items[0].folder_id.is_some(),
        "第 1 行应归入示例分组"
    );
    assert!(!preview.items[0].favorite);

    match &preview.items[1].kind {
        ItemKind::SecureNote(note) => {
            assert_eq!(note.title, "示例安全笔记");
            assert_eq!(note.body, "笔记正文写在这里");
        }
        other => panic!("第 2 行应为 SecureNote，实际: {other:?}"),
    }
    assert!(
        preview.items[1].folder_id.is_none(),
        "第 2 行 folder 留空 → 不归类"
    );
    assert!(preview.items[1].favorite, "第 2 行 favorite=1 → 收藏");
}

/// D-112：泰文文本跑同一套契约——本地化不得破坏结构与可导入性。
#[test]
fn template_localized_thai_parses_with_zero_skipped() {
    let csv = csv_template(&th_texts());
    let preview = bitwarden_csv::parse(&csv).expect("泰文模板必须能被自己的解析器导入");

    assert_eq!(preview.total, 2);
    assert_eq!(preview.items.len(), 2);
    assert_eq!(preview.skipped, 0);
    assert_eq!(preview.folders.len(), 1);
    assert_eq!(preview.folders[0].name, "โฟลเดอร์ตัวอย่าง");

    match &preview.items[0].kind {
        ItemKind::Login(login) => {
            assert_eq!(login.title, "รายการเข้าสู่ระบบตัวอย่าง");
            assert!(
                login.notes.as_deref().is_some_and(|n| n.contains(',')),
                "泰文 notes 同样必须演示半角逗号转义"
            );
        }
        other => panic!("第 1 行应为 Login，实际: {other:?}"),
    }
    match &preview.items[1].kind {
        ItemKind::SecureNote(note) => {
            assert_eq!(note.title, "โน้ตความปลอดภัยตัวอย่าง");
            assert_eq!(note.body, "เนื้อหาเขียนที่นี่");
        }
        other => panic!("第 2 行应为 SecureNote，实际: {other:?}"),
    }
}

#[test]
fn template_header_matches_known_columns() {
    let csv = csv_template(&zh_texts());
    let header = csv.lines().next().expect("模板至少有一行表头");
    let columns: Vec<&str> = header.split(',').map(str::trim).collect();

    assert_eq!(
        columns.len(),
        KNOWN_COLUMNS.len(),
        "表头列数必须与 KNOWN_COLUMNS 一致"
    );
    // 顺序逐字相等：模板改了而解析器没跟上时，这条会先红。
    for (i, (actual, expected)) in columns.iter().zip(KNOWN_COLUMNS.iter()).enumerate() {
        assert_eq!(
            actual, expected,
            "第 {i} 列表头应为 {expected}，实际 {actual}"
        );
    }
}

#[test]
fn template_rows_do_not_leak_into_custom_fields() {
    for texts in [zh_texts(), th_texts()] {
        let preview = bitwarden_csv::parse(&csv_template(&texts)).unwrap();
        for (i, item) in preview.items.iter().enumerate() {
            let custom = match &item.kind {
                ItemKind::Login(login) => &login.custom_fields,
                // SecureNote 不带 custom_fields，等价于「无溢出」。
                ItemKind::SecureNote(_) => {
                    continue;
                }
                other => panic!("模板不应产生 {other:?} 类型条目"),
            };
            assert!(
                custom.is_empty(),
                "第 {i} 行溢出到 custom_fields: {custom:?}（示例行列数与表头不一致？）"
            );
        }
    }
}

#[test]
fn template_has_no_bom_and_ends_with_newline() {
    // 文件级细节也会影响「用 Excel 打开再存回去」的往返：
    // BOM 会让首列名变成 `\u{feff}folder`（表头匹配失败），缺尾换行会让最后一行被吞。
    for texts in [zh_texts(), th_texts()] {
        let csv = csv_template(&texts);
        assert!(
            !csv.starts_with('\u{feff}'),
            "模板不得带 UTF-8 BOM（会污染首列表头名）"
        );
        assert!(
            csv.ends_with('\n'),
            "模板应以换行结尾（避免最后一行被某些编辑器吞掉）"
        );
        assert_eq!(csv.lines().count(), 3, "模板应为 1 行表头 + 2 行示例");
    }
}
