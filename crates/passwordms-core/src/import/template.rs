//! 内置可导入 CSV 模板（D-72）。
//!
//! 用途：用户在手机上「保存导入模板」拿到一份**能被本应用直接导入**的样例文件
//! （`save_import_template` 命令把它写到 SAF 选定的位置），照着填就能批量录入。
//!
//! ## 硬约束（`tests/import_template.rs` 逐条锁死）
//!
//! 1. **表头顺序必须与 [`KNOWN_COLUMNS`] 完全一致**——解析器按表头名映射列，
//!    但模板是给用户看的，顺序错位会让「示例值落到别的列」这种错很难自查；
//! 2. **模板自身导入必须 0 跳过**（`total == 2` / `items.len() == 2` / `skipped == 0`）；
//! 3. **示例行不得溢出到 `custom_fields`**——列数与表头严格相等，多一列就会被
//!    当成自定义字段（那是给「用户自加列」的通道，不该被模板示范污染）。
//!
//! 内容说明：
//! - 第 1 条示范 `login` 类型 + **引号包裹且内含半角逗号的 `notes`**
//!   （CSV 的字段分隔符是半角 `,`，所以字段里要写半角逗号就必须整段用英文双引号包住；
//!   全角「，」不算分隔符、不需要包裹 —— 旧版模板误用了全角逗号，导致这一行其实
//!   从没验证过转义路径）；换行同样支持（引号内的 `\n`），但模板**故意不演示**：
//!   演示后模板会变成多行，用户更难看出「一行一条记录」的结构；
//! - 第 2 条示范 `secure note` 类型 + `favorite=1` + 空分组（`folder` 留空即不归类）；
//! - `fields` 列留空：自定义字段的写法是「表头里另加列名」，不是这一列的语法
//!   （`fields` 仅作为已知列占位，保证与 Bitwarden 导出的表头同名）。
//!
//! [`KNOWN_COLUMNS`]: crate::import::bitwarden_csv::KNOWN_COLUMNS

/// 可直接导入的样例 CSV（UTF-8，无 BOM，`\n` 换行）。
///
/// 表头 10 列与 [`crate::import::bitwarden_csv::KNOWN_COLUMNS`] 顺序一致；
/// 两个示例行分别示范 `login` 与 `secure note`。
///
/// 第 1 行的 `notes` 用英文双引号包住、内部含**半角逗号**——这是真正的 CSV 转义示范
/// （若内部只有全角「，」，引号形同虚设：删掉引号解析结果完全一样，等于没演示）。
/// 源码里的 `\"` 就是 CSV 里的那一对英文双引号。
pub const CSV_TEMPLATE: &str = concat!(
    "folder,favorite,type,name,notes,login_uri,login_username,login_password,login_totp,fields\n",
    "示例分组,0,login,示例登录条目,\"备注里可以有半角逗号, 但整段必须用英文双引号包住\",",
    "https://example.com,user@example.com,ChangeMe123!,,\n",
    ",1,secure note,示例安全笔记,笔记正文写在这里,,,,\n",
);
