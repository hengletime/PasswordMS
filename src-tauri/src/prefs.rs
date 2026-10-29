//! 应用级配置：与保险库无关、**未解锁也要能读写**的那部分状态（v1.4.10）。
//!
//! ## 为什么不塞进 `VaultSettings`
//!
//! `VaultSettings` 存在保险库内部，随库加密，**未解锁根本读不到**。而托盘菜单、
//! 关窗行为、开机自启、全局热键键位这几件事，恰恰发生在锁屏状态——甚至发生在一台
//! 从没解锁过的新机器上（开机自启要在启动瞬间就知道开没开）。
//!
//! 把热键键位放进库里，等于「保险库锁着的时候热键注册不出来」，而这正是最需要它的时刻。
//! 所以这份配置独立落在 `app_config_dir/prefs.json`，**不含任何秘密**，明文即可。
//!
//! ## 三条设计约束
//!
//! 1. **绝不让坏配置把功能永久打死**：文件损坏/字段非法一律回落默认值并记日志，
//!    绝不 panic、绝不返回错误让上层无法启动。热键配置尤其如此——写坏一个字符，
//!    恢复手段就只有手工删文件了。
//! 2. **原子写**：先写 `.tmp` 再 rename。断电/崩溃时要么是旧内容、要么是新内容，
//!    不会出现半截 JSON。
//! 3. **向前兼容**：`#[serde(default)]`，旧版本文件缺字段照样能读。

use std::path::Path;

use serde::{Deserialize, Serialize};
// 只有桌面端有注册器。移动端（Android）没有这个插件，也不需要热键——设置卡不渲染。
#[cfg(desktop)]
use tauri_plugin_global_shortcut::Shortcut;

/// 默认全局热键（规范化形态）。
///
/// 为什么是 `Ctrl+Alt+P` 而不是 `Ctrl+Shift+Space`：中文输入法普遍占着
/// `Ctrl+Space` / `Ctrl+Shift` 那一家子，`Ctrl+Alt+P` 几乎不会撞车。
/// `P` = Password。
pub const DEFAULT_HOTKEY: &str = "ctrl+alt+KeyP";

/// 应用级配置。
///
/// `hotkey` 存的是**规范化字符串**（小写修饰符 + W3C `KeyboardEvent.code`），
/// 见 [`canonical_hotkey`]。别手改这个字段的格式，[`Shortcut`] 的解析器认死理。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPrefs {
    /// 全局热键。空字符串表示**用户主动禁用热键**（托盘与关窗行为照常）。
    pub hotkey: String,
    /// 开机自启是否开启。**以系统实际状态为准**：这里只是「上次我们设置的值」，
    /// 用户在任务管理器里禁用了启动项时，界面要显示真实状态而不是这个字段。
    pub autostart: bool,
    /// 是否已经给过「关闭窗口 = 最小化到托盘」的教育提示（只提示一次）。
    pub tray_hint_shown: bool,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            autostart: false,
            tray_hint_shown: false,
        }
    }
}

/// 热键被拒的原因。前端据此选 i18n 文案，**不复用自由文本错误**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyReject {
    /// 空串或只有分隔符。
    Empty,
    /// 缺主修饰键（Ctrl/Alt/Super 至少一个）。只按 Shift 的组合会与正常输入打架。
    NoModifier,
    /// 同一个修饰键写了两遍（`ctrl+ctrl+KeyP`）。
    RepeatedModifier,
    /// 键名无法被注册器识别（`KeyP` 之外的东西，比如 `P`、`Foo`）。
    UnknownKey,
}

/// 把用户/前端给的键位串规范化成注册器能吃的形态。
///
/// 接受 `Ctrl+Alt+KeyP`、`ctrl + alt + keyp`(键名大小写敏感，见下) 这类写法，
/// 输出固定为 `ctrl+alt+KeyP`：
///
/// * 修饰符统一小写、按 `ctrl → alt → shift → super` 排序（同一组合永远只有一种写法，
///   否则「用户改了顺序」会被当成改键，白白触发一次注销+注册）；
/// * 键名**原样保留**并要求 W3C `KeyboardEvent.code` 写法（`KeyP` / `Digit1` / `F5`）。
///   前端录制时直接取 `event.code` 即可，两边不需要维护映射表；
/// * 校验器直接用 [`Shortcut`] 的 `FromStr`——**和注册器是同一个解析器**，
///   杜绝「校验通过、注册时才报错」的两套标准。
pub fn canonical_hotkey(raw: &str) -> Result<String, HotkeyReject> {
    let parts: Vec<&str> = raw
        .split('+')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let (key, mods_raw) = parts.split_last().ok_or(HotkeyReject::Empty)?;

    let mut mods: Vec<&'static str> = Vec::with_capacity(mods_raw.len());
    for tok in mods_raw {
        let m = normalize_modifier(tok).ok_or(HotkeyReject::UnknownKey)?;
        if mods.contains(&m) {
            return Err(HotkeyReject::RepeatedModifier);
        }
        mods.push(m);
    }

    // 键名位置出现修饰符名（`Ctrl+Alt`、`Ctrl+Shift`）不是合法键位——这是「录到一半松手」的典型形态。
    if normalize_modifier(key).is_some() || key.is_empty() {
        return Err(HotkeyReject::NoModifier);
    }

    // 只按 Shift 的组合会和正常输入打架（Shift+A 就是大写 A），必须带 Ctrl/Alt/Super 之一。
    if !mods.iter().any(|m| matches!(*m, "ctrl" | "alt" | "super")) {
        return Err(HotkeyReject::NoModifier);
    }

    const ORDER: [&str; 4] = ["ctrl", "alt", "shift", "super"];
    mods.sort_by_key(|m| ORDER.iter().position(|x| x == m).unwrap_or(ORDER.len()));

    let canonical = format!("{}+{}", mods.join("+"), normalize_key(key));
    // 与注册器同一个解析器：这里过了，注册就不会因为「键名不认识」而失败。
    //
    // 移动端跳过这一层：那里没有 `Shortcut`（插件不存在），也没有热键功能
    // （设置卡按 DTO 的 `supported` 不渲染）。语法层的校验仍然照做。
    #[cfg(desktop)]
    canonical
        .parse::<Shortcut>()
        .map_err(|_| HotkeyReject::UnknownKey)?;
    Ok(canonical)
}

/// 修饰符同义词归一。大小写不敏感（用户手输时不会在意）。
fn normalize_modifier(tok: &str) -> Option<&'static str> {
    match tok.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some("ctrl"),
        "alt" | "option" => Some("alt"),
        "shift" => Some("shift"),
        "super" | "meta" | "win" | "cmd" | "command" => Some("super"),
        _ => None,
    }
}

/// 键名归一成 W3C `KeyboardEvent.code` 写法。
///
/// **实测事实（2026-10-29，`Shortcut` 0.8 / keyboard-types 0.7）**：解析器对字母和数字
/// **两种写法都认**——`P` 与 `KeyP` 都能注册成功。但存储必须唯一，否则同一个物理键会有
/// 两种规范化结果：前端录制给的是 `KeyP`（`event.code`），而手工填 `P` 会存成另一份，
/// 于是「改了写法」在配置里看起来像换了键，去重/回滚判断都会漂。
///
/// 所以这里只做一件事：单字母 → `KeyA`…、单数字 → `Digit0`…，其余（`F5` / `Space` /
/// `ArrowLeft` / `Numpad5`）原样保留。
fn normalize_key(tok: &str) -> String {
    let t = tok.trim();
    let mut chars = t.chars();
    if let (Some(only), None) = (chars.next(), chars.next()) {
        if only.is_ascii_alphabetic() {
            return format!("Key{}", only.to_ascii_uppercase());
        }
        if only.is_ascii_digit() {
            return format!("Digit{only}");
        }
    }
    t.to_string()
}

/// 读配置。**永不失败**：任何异常都回落默认值，并把原因交给调用方记日志。
///
/// 返回第二项是「发生过降级」的说明（`None` = 一切正常），调用方负责落到
/// `passwordms.log`——GUI 程序没有 stderr，静默回落等于把线索丢掉。
pub fn load_or_default(path: &Path) -> (AppPrefs, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        // 首次启动、或用户手工删了配置：这是正常路径，不算降级。
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (AppPrefs::default(), None),
        Err(e) => {
            return (
                AppPrefs::default(),
                Some(format!("[prefs] 读取失败（{e}），已回落默认配置")),
            )
        }
    };

    let mut prefs = match serde_json::from_str::<AppPrefs>(&text) {
        Ok(p) => p,
        Err(e) => {
            return (
                AppPrefs::default(),
                Some(format!("[prefs] 解析失败（{e}），已回落默认配置")),
            )
        }
    };

    // 字段级的「合法但不可用」：热键写坏了，只该回退热键那一项，不该把自启设置一起清掉。
    if !prefs.hotkey.is_empty() && canonical_hotkey(&prefs.hotkey).is_err() {
        let bad = std::mem::replace(&mut prefs.hotkey, DEFAULT_HOTKEY.to_string());
        return (
            prefs,
            Some(format!(
                "[prefs] 热键 {bad:?} 非法，已回落到 {DEFAULT_HOTKEY}"
            )),
        );
    }

    (prefs, None)
}

/// 原子写配置（`.tmp` + rename）。目录不存在时自动创建。
pub fn save_to(path: &Path, prefs: &AppPrefs) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("建目录失败：{e}"))?;
    }
    let body = serde_json::to_vec_pretty(prefs).map_err(|e| format!("序列化失败：{e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| format!("写临时文件失败：{e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("替换配置失败：{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单测专用临时目录（不引 tempfile 依赖，避免为几条断言扩依赖树）。
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("pms-prefs-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    #[test]
    fn default_hotkey_is_canonical() {
        // 默认值自己必须过规范化——否则全新机器上热键注册不上，而用户什么都没改。
        assert_eq!(
            canonical_hotkey(DEFAULT_HOTKEY),
            Ok(DEFAULT_HOTKEY.to_string())
        );
    }

    #[test]
    fn canonical_sorts_modifiers() {
        assert_eq!(
            canonical_hotkey("Shift+Ctrl+KeyA").as_deref(),
            Ok("ctrl+shift+KeyA")
        );
        assert_eq!(
            canonical_hotkey("Super+Alt+Ctrl+KeyZ").as_deref(),
            Ok("ctrl+alt+super+KeyZ")
        );
    }

    #[test]
    fn canonical_trims_and_normalizes_case() {
        assert_eq!(
            canonical_hotkey("  CTRL +  Alt + KeyP  ").as_deref(),
            Ok("ctrl+alt+KeyP")
        );
        assert_eq!(
            canonical_hotkey("Control+Option+KeyP").as_deref(),
            Ok("ctrl+alt+KeyP")
        );
    }

    #[test]
    fn rejects_without_primary_modifier() {
        assert_eq!(canonical_hotkey("KeyP"), Err(HotkeyReject::NoModifier));
        // 只有 Shift：Shift+A 就是大写 A，注册它等于废掉键盘。
        assert_eq!(
            canonical_hotkey("Shift+KeyP"),
            Err(HotkeyReject::NoModifier)
        );
        // 录到一半松手：修饰符占住了键名位。
        assert_eq!(canonical_hotkey("Ctrl+Alt"), Err(HotkeyReject::NoModifier));
    }

    #[test]
    fn rejects_repeated_modifier_and_empty() {
        assert_eq!(
            canonical_hotkey("ctrl+control+KeyP"),
            Err(HotkeyReject::RepeatedModifier)
        );
        assert_eq!(canonical_hotkey(""), Err(HotkeyReject::Empty));
        assert_eq!(canonical_hotkey(" + + "), Err(HotkeyReject::Empty));
    }

    #[test]
    fn rejects_unknown_key_names() {
        assert_eq!(
            canonical_hotkey("ctrl+alt+NotAKey"),
            Err(HotkeyReject::UnknownKey)
        );
        // 修饰符位置上出现非修饰符。
        assert_eq!(canonical_hotkey("Foo+KeyP"), Err(HotkeyReject::UnknownKey));
    }

    #[test]
    fn bare_letter_and_digit_are_normalized_to_w3c_codes() {
        // 实测：`Shortcut` 的解析器对 `P` 与 `KeyP` **都认**，所以这里不能靠"解析失败"来挡，
        // 必须主动归一——否则同一个物理键在配置里有两种写法，改键判定会漂。
        assert_eq!(
            canonical_hotkey("ctrl+alt+P").as_deref(),
            Ok("ctrl+alt+KeyP")
        );
        assert_eq!(
            canonical_hotkey("ctrl+alt+p").as_deref(),
            Ok("ctrl+alt+KeyP")
        );
        assert_eq!(canonical_hotkey("alt+1").as_deref(), Ok("alt+Digit1"));
        // 已经是规范写法的原样保持（前端 `event.code` 走的就是这条）。
        assert_eq!(
            canonical_hotkey("ctrl+alt+KeyP").as_deref(),
            Ok("ctrl+alt+KeyP")
        );
        // 多字符键名不参与单字符归一。
        assert_eq!(canonical_hotkey("ctrl+F5").as_deref(), Ok("ctrl+F5"));
    }

    #[test]
    fn accepts_function_and_digit_keys() {
        assert_eq!(canonical_hotkey("ctrl+F5").as_deref(), Ok("ctrl+F5"));
        assert_eq!(canonical_hotkey("alt+Digit1").as_deref(), Ok("alt+Digit1"));
    }

    #[test]
    fn missing_fields_fall_back_to_default() {
        // 向前兼容：将来加字段，旧文件不能读崩。
        let p: AppPrefs = serde_json::from_str(r#"{"autostart":true}"#).expect("缺字段可读");
        assert!(p.autostart);
        assert_eq!(p.hotkey, DEFAULT_HOTKEY);
        assert!(!p.tray_hint_shown);
    }

    #[test]
    fn load_missing_file_is_silent_default() {
        let dir = temp_dir("missing");
        let (p, warn) = load_or_default(&dir.join("nope.json"));
        assert_eq!(p, AppPrefs::default());
        assert!(warn.is_none(), "文件不存在是正常路径，不该记降级日志");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_corrupt_json_recovers_with_warning() {
        let dir = temp_dir("corrupt");
        let f = dir.join("prefs.json");
        std::fs::write(&f, b"{ not json").unwrap();
        let (p, warn) = load_or_default(&f);
        assert_eq!(p, AppPrefs::default());
        assert!(warn.is_some(), "损坏必须留痕");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_hotkey_only_resets_hotkey() {
        // 关键行为：热键写坏不该把用户的自启选择一起清掉。
        let dir = temp_dir("badhotkey");
        let f = dir.join("prefs.json");
        std::fs::write(&f, br#"{"hotkey":"ctrl+alt+NotAKey","autostart":true}"#).unwrap();
        let (p, warn) = load_or_default(&f);
        assert_eq!(p.hotkey, DEFAULT_HOTKEY);
        assert!(p.autostart, "其他字段必须保住");
        assert!(warn.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = temp_dir("roundtrip");
        let f = dir.join("prefs.json");
        let p = AppPrefs {
            hotkey: "ctrl+shift+KeyL".into(),
            autostart: true,
            tray_hint_shown: true,
        };
        save_to(&f, &p).expect("写入");
        let (back, warn) = load_or_default(&f);
        assert_eq!(back, p);
        assert!(warn.is_none());
        // 原子写不留残渣。
        assert!(
            !f.with_extension("json.tmp").exists(),
            "临时文件应已 rename 掉"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_hotkey_means_disabled_and_is_not_reset() {
        // 空串是「用户主动关掉热键」的合法状态，不能被当成坏数据补回默认值。
        let dir = temp_dir("disabled");
        let f = dir.join("prefs.json");
        std::fs::write(&f, br#"{"hotkey":""}"#).unwrap();
        let (p, warn) = load_or_default(&f);
        assert_eq!(p.hotkey, "");
        assert!(warn.is_none(), "主动禁用不是降级");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
