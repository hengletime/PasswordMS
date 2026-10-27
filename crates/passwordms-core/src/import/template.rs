//! 可导入 CSV 模板（D-72；D-112 本地化改造）。
//!
//! 用途：用户「保存导入模板」拿到一份**能被本应用直接导入**的样例文件
//! （`save_import_template` 命令把它写到 SAF / 系统对话框选定的位置），照着填就能批量录入。
//!
//! ## D-112 架构裁定：core 只出结构，不出文案
//!
//! 表头与转义示范这类**结构**留在 core（与解析器的契约由 `tests/import_template.rs`
//! 守着）；示例行的**人类可读文本**（分组名、条目名、备注……）由前端按当前 UI 语言
//! 经 i18n 字典传入——泰文用户拿到的就是泰文模板。core 不再内置任何语言。
//!
//! ## 硬约束（`tests/import_template.rs` 逐条锁死）
//!
//! 1. **表头顺序必须与 [`KNOWN_COLUMNS`] 完全一致**——解析器按表头名映射列，
//!    但模板是给用户看的，顺序错位会让「示例值落到别的列」这种错很难自查；
//! 2. **模板自身导入必须 0 跳过**（前提：传入文本不含 `"` / 换行，构造函数校验）；
//! 3. **示例行不得溢出到 `custom_fields`**——列数与表头严格相等，多一列就会被
//!    当成自定义字段（那是给「用户自加列」的通道，不该被模板示范污染）。
//!
//! ## 内容说明
//!
//! - 第 1 条示范 `login` 类型 + **引号包裹且内含半角逗号的 `login_notes`**。
//!   CSV 的字段分隔符是半角 `,`，字段里要写半角逗号就必须整段用英文双引号包住；
//!   全角「，」不算分隔符、不需要包裹（旧版模板曾误用全角逗号，导致转义路径
//!   从没被验证过）。**传入的 `login_notes` 必须自带至少一个半角逗号**，
//!   否则引号包裹等于没示范——各语言文案由 zh-CN 字典（唯一事实来源）保证这点；
//! - 第 2 条示范 `secure note` 类型 + `favorite=1` + 空分组（`folder` 留空即不归类）；
//! - `fields` 列留空：自定义字段的写法是「表头里另加列名」，不是这一列的语法
//!   （`fields` 仅作为已知列占位，保证与 Bitwarden 导出的表头同名）；
//! - 换行同样支持（引号内的 `\n`），但模板**故意不演示**：演示后模板会变成多行，
//!   用户更难看出「一行一条记录」的结构。
//!
//! [`KNOWN_COLUMNS`]: crate::import::bitwarden_csv::KNOWN_COLUMNS

use crate::error::{CoreError, CoreResult};

/// 模板示例文本（D-112）：由前端按当前 UI 语言从 i18n 字典取值后传入。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct TemplateTexts {
    /// 第 1 行的分组名。
    pub folder: String,
    /// 第 1 行的登录条目标题。
    pub login_name: String,
    /// 第 1 行的备注——**必须含至少一个半角逗号**（引号转义示范的组成部份）。
    pub login_notes: String,
    /// 第 2 行的安全笔记标题。
    pub note_name: String,
    /// 第 2 行的笔记正文。
    pub note_body: String,
}

impl TemplateTexts {
    /// 构造并校验：任何字段含 `"`、`\n` 或 `\r` 都会破坏「1 行 1 条」的模板结构
    /// （引号会提前闭合 CSV 字段，换行会把一行拆成两行），直接拒绝。
    pub fn new(
        folder: String,
        login_name: String,
        login_notes: String,
        note_name: String,
        note_body: String,
    ) -> CoreResult<Self> {
        let t = Self {
            folder,
            login_name,
            login_notes,
            note_name,
            note_body,
        };
        for (field, value) in [
            ("folder", &t.folder),
            ("login_name", &t.login_name),
            ("login_notes", &t.login_notes),
            ("note_name", &t.note_name),
            ("note_body", &t.note_body),
        ] {
            if value.contains('"') || value.contains(['\n', '\r']) {
                return Err(CoreError::Validation(format!(
                    "template text {field} must not contain quotes or newlines"
                )));
            }
        }
        Ok(t)
    }
}

/// 用传入的本地化示例文本拼装模板 CSV（UTF-8，无 BOM，`\n` 换行）。
///
/// 表头 10 列与 [`crate::import::bitwarden_csv::KNOWN_COLUMNS`] 顺序一致；
/// `login_notes` 整段用英文双引号包住（真正的 CSV 转义示范）。
pub fn csv_template(t: &TemplateTexts) -> String {
    format!(
        concat!(
            "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields\n",
            "{folder},0,login,{login_name},\"{login_notes}\",",
            "https://example.com,user@example.com,ChangeMe123!,,\n",
            ",1,secure note,{note_name},{note_body},,,,\n",
        ),
        folder = t.folder,
        login_name = quote_if_needed(&t.login_name),
        login_notes = t.login_notes,
        // 标题/正文也可能含半角逗号（如泰文文案），同样引号包裹保安全。
        note_name = quote_if_needed(&t.note_name),
        note_body = quote_if_needed(&t.note_body),
    )
}

/// 含半角逗号的字段用英文双引号包裹（CSV 转义）；文本已由 [`TemplateTexts::new`]
/// 保证不含引号与换行，包裹后必然安全。
fn quote_if_needed(s: &str) -> String {
    if s.contains(',') {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_quotes_and_newlines() {
        assert!(TemplateTexts::new(
            "a\"b".into(),
            "n".into(),
            "x".into(),
            "n".into(),
            "b".into()
        )
        .is_err());
        assert!(TemplateTexts::new(
            "a\nb".into(),
            "n".into(),
            "x".into(),
            "n".into(),
            "b".into()
        )
        .is_err());
        assert!(
            TemplateTexts::new(
                "f".into(),
                "n".into(),
                "备注, 含逗号可以".into(),
                "n".into(),
                "b".into()
            )
            .is_ok(),
            "半角逗号是转义示范的组成部份，必须放行"
        );
    }

    #[test]
    fn thai_text_renders_utf8_rows() {
        let t = TemplateTexts::new(
            "โฟลเดอร์ตัวอย่าง".into(),
            "รายการเข้าสู่ระบบตัวอย่าง".into(),
            "หมายเหตุใส่เครื่องหมายจุลภาคได้, แต่ต้องครอบด้วยเครื่องหมายอัญประกาศ".into(),
            "โน้ตความปลอดภัยตัวอย่าง".into(),
            "เนื้อหาเขียนที่นี่".into(),
        )
        .unwrap();
        let csv = csv_template(&t);
        assert_eq!(csv.lines().count(), 3);
        assert!(csv.contains("โฟลเดอร์ตัวอย่าง"));
        // notes 引号包裹且内含半角逗号（转义示范不因语言而失效）。
        assert!(csv.contains("\"หมายเหตุใส่เครื่องหมายจุลภาคได้, แต่"));
    }
}
