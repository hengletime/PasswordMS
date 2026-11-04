//! 自动输入候选行的纯逻辑（从命令层下沉，见 docs/06 §6.71）。
//!
//! 为什么在这里而不是在 `src-tauri/src/commands/autotype_cmd.rs`：命令层是薄壳
//! （docs/02 §2：参数解包 → 调 core → 错误映射），而"哪些条目能出现在浮层里、
//! 带哪五个字段"是**业务判断**，必须能被单测覆盖。命令层 18 个文件都没有测试台
//! （也不该为一条断言引入 tauri 装置），所以把判断下沉到这里。
//!
//! 两条不变量由本模块负责：
//! 1. **只收登录条目**——笔记/附件没有可注入字段，列出来只会让"选中之后什么都
//!    没发生"变成新的困惑；它同时是"浮层不会把 `secure_note` 的 id 交给
//!    `auto_type_fill`"的唯一保证（那条命令的早退路径因此保持不可达）。
//! 2. **只给五个字段**——`title` / `username` / `url` 参与打分；多一个字段就多
//!    一份可被带走的明文面。
//!
//! 第三条是**保序**，容易被忽略：`filter_map` 必须保持输入顺序。浮层的钉住块、
//! 钉住块内的手动顺序、未钉住的频度序**全部**由 `repo.list_items()` 的排序携带，
//! 本模块一旦重排（哪怕只是"顺手 .sort_by(use_count)"），前端再怎么保序也救不回来
//! ——D-196 就是这么发生的：前端重排 + 此处保序，两边只要有一边表错意，钉住就没了。

use serde::{Deserialize, Serialize};

use crate::model::{Item, ItemKind};

/// 浮层里的一行候选（wire 形状，与前端 `RankableCandidate` 逐字段对齐）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRow {
    pub item_id: String,
    pub title: String,
    /// 无用户名时为 `None`（不写空串：前端统一按 `?? ''` 归一）。
    pub username: Option<String>,
    /// 无网址时为 `None`，与 `username` 同一口径。
    pub url: Option<String>,
    /// 使用次数：**wire 上的元数据，前端不拿它排序**（D-196）。
    ///
    /// 曾经的用法是"空查询按它降序 = 最常用"，但那样会**覆盖掉 `list_items` 的
    /// 钉住块顺序**（钉住条目恰好用得少 → 一按频度重排就沉下去，主人实测
    /// 「钉住和手动排序在浮层里不管用」）。现在前端只按匹配得分做稳定排序，
    /// 其余维度一律继承 `list_items` 的顺序，此字段因此只剩展示/将来兜底的价值。
    ///
    /// 保留而不删：删除要动 wire 契约与四语言之外的一串用例，收益仅"少一个字段"；
    /// 它本身是**元数据不是凭据**（浮层 JS 堆里能看出"这人最常用哪些"，有弱侧信道
    /// 性质，泄露面比明文低两个数量级，判定可接受）。
    pub use_count: u64,
}

/// 从条目集合里挑出登录条目并裁成五字段。
///
/// 空字符串按"没有填写"处理（与编辑器保存时的归一一致）：前端会把 `None`
/// 显示为空白、并在对应动作上报"这一条没有填…"，而不是打出一段空白。
pub fn login_candidates(items: &[Item]) -> Vec<CandidateRow> {
    items
        .iter()
        .filter_map(|item| {
            let ItemKind::Login(data) = &item.kind else {
                return None;
            };
            Some(CandidateRow {
                item_id: item.item_id.clone(),
                title: data.title.clone(),
                username: data.username.clone().filter(|u| !u.is_empty()),
                url: data.url.clone().filter(|u| !u.is_empty()),
                use_count: item.use_count,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AttachmentData, LoginData, SecureNoteData};

    fn item(id: &str, kind: ItemKind, use_count: u64) -> Item {
        Item {
            item_id: id.to_string(),
            kind,
            folder_id: None,
            favorite: false,
            created_at: 0,
            updated_at: 0,
            rev: 1,
            modified_by: String::new(),
            attachments: Vec::new(),
            pinned: false,
            pin_order: 0,
            use_count,
            last_used_at: None,
        }
    }

    fn login(title: &str, username: Option<&str>, url: Option<&str>) -> ItemKind {
        ItemKind::Login(LoginData {
            title: title.to_string(),
            url: url.map(str::to_string),
            username: username.map(str::to_string),
            password: Some("hunter2".to_string()),
            notes: Some("note".to_string()),
            totp_secret: None,
            custom_fields: Vec::new(),
        })
    }

    #[test]
    // `ItemKind::Attachment` 是 D-60 废弃变体（只剩迁移读取能力），这里刻意造一个
    // 来证明它也不会被列进候选——正是因为它"理论上不该存在"，才更要在浮层里隐形。
    #[allow(deprecated)]
    fn 只收登录条目_笔记与附件一律不列() {
        let items = vec![
            item("a", login("邮箱", Some("me@example.com"), None), 3),
            item(
                "b",
                ItemKind::SecureNote(SecureNoteData {
                    title: "笔记".to_string(),
                    body: "正文".to_string(),
                }),
                9,
            ),
            item(
                "c",
                ItemKind::Attachment(AttachmentData {
                    title: "附件".to_string(),
                    original_filename: "a.bin".to_string(),
                    mime_type: "application/octet-stream".to_string(),
                    size_bytes: 1,
                    blob_ref: "blob".to_string(),
                    blob_size_bytes: 1,
                }),
                9,
            ),
        ];
        let rows = login_candidates(&items);
        assert_eq!(rows.len(), 1, "非登录条目必须被滤掉");
        assert_eq!(rows[0].item_id, "a");
        assert!(
            rows.iter().all(|r| r.item_id != "b" && r.item_id != "c"),
            "非登录条目的 id 不得出现在候选里"
        );
    }

    #[test]
    fn 五个字段原样透传_不含密码与备注() {
        let items = vec![item(
            "a",
            login(
                "工作邮箱",
                Some("me@corp.example"),
                Some("https://mail.corp.example"),
            ),
            42,
        )];
        let rows = login_candidates(&items);
        let row = &rows[0];
        assert_eq!(row.title, "工作邮箱");
        assert_eq!(row.username.as_deref(), Some("me@corp.example"));
        assert_eq!(row.url.as_deref(), Some("https://mail.corp.example"));
        // use_count 是空查询的排序主轴：丢了它「最常用」会静默退化成字母序。
        assert_eq!(row.use_count, 42, "use_count 必须透传，不能恒 0");
        // 结构上就不该有这几个字段——序列化后逐字段核对，防将来有人"顺手加成六个字段"。
        let wire = serde_json::to_value(row).expect("serialize");
        let obj = wire.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["item_id", "title", "url", "use_count", "username"]);
    }

    #[test]
    fn 空字符串按未填写处理() {
        let items = vec![item("a", login("空值", Some(""), Some("")), 0)];
        let rows = login_candidates(&items);
        assert_eq!(rows[0].username, None, "空用户名词按未填写处理");
        assert_eq!(rows[0].url, None, "空网址按未填写处理");
    }
}
