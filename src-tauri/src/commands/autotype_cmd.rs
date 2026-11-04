//! 自动输入命令（1.5.39）：把选中条目的凭据用 `SendInput` 打回原焦点窗口。
//!
//! ## 整条链路
//!
//! 1. 热键（解锁态）→ `hotkey.rs` 用 `GetForegroundWindow()` 记下目标窗口
//!    （`AppState.autotype_target`）→ 唤出 `autotype` 浮层；
//! 2. 用户在浮层选中条目与**动作** → 前端 `invoke("auto_type_fill", { item_id, action })`；
//! 3. 本命令：隐藏浮层 → 校验并切回目标窗口 → 逐 Token `SendInput` 注入。
//!
//! 动作（1.5.39 增强，与前端选择窗逐字一致）：`default`（用户名+Tab+密码+回车）/
//! `username` / `password` / `url` / `totp`（后四者单字段、不带回车）。模板由 core
//! 的 [`autotype_seq::template_for`] 给出；缺字段按动作报专用错误码（前端显式映射）。
//!
//! ## 安全纪律（与 `copy_secret` / `ClipboardMemory` 同基线）
//!
//! - **只收 `item_id`**：明文一律在命令内走 `get_item` 同一条解密路径现取，
//!   绝不信任前端传来的任何明文；
//! - 用户名 / 密码一到手立刻 `Zeroizing` 包住；UTF-16 注入缓冲用完即
//!   `securemem::wipe`；条目其余明文字段在取完两个字段后 `wipe_plaintext`；
//! - 明文不进任何日志（错误只带错误码）；
//! - 注入全程在每个 Token 边界复查前台窗口，不是目标立即中止——密码绝不
//!   打进用户没看见的窗口。
//!
//! ## 平台对称（D-144）
//!
//! Windows 实现拆独立函数；非 Windows（Android）同名对称函数返回
//! `ERR_NOT_SUPPORTED`——**整个命令在 Android 上必须能编译**（cfg 块里禁止
//! 写 `return`，用尾表达式）。运行时报错而非编译期缺失，前端才拿得到可读事实。

use passwordms_core::autotype_seq::{self, Token};
use passwordms_core::model::ItemKind;
use passwordms_core::securemem::Zeroizing;
use passwordms_core::CoreError;

use crate::error::{lock_err, shell_codes, IpcError, IpcResult};
use crate::state::AppState;

/// 每个字符注入后的间隔（毫秒）。SendInput 是异步进系统队列的，太快会被
/// 目标应用的输入处理吞掉；6ms 是保守起点，真机再调。
#[cfg(windows)]
const PER_CHAR_DELAY_MS: u64 = 6;
/// 每个 Token（文本块 / Tab / Enter / 停顿）之间的间隔（毫秒）。
#[cfg(windows)]
const PER_TOKEN_DELAY_MS: u64 = 50;
/// 切回目标窗口后的等待（毫秒）：`SetForegroundWindow` 是异步的，焦点转移
/// 需要一点时间；等完再校验，不等就是"对着旧窗口注入"。
#[cfg(windows)]
const FOCUS_SETTLE_MS: u64 = 200;

/// 自动输入：把 `item_id` 对应条目的**指定内容**打进热键触发那一刻的前台窗口。
///
/// `action` ∈ `default | username | password | url | totp`（与前端选择窗的键逐字一致，
/// 模板由 core 的 [`autotype_seq::template_for`] 给出）。
#[tauri::command(rename_all = "snake_case")]
pub async fn auto_type_fill(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    item_id: String,
    action: String,
) -> IpcResult<()> {
    // Android 上没有浮层可藏，`app` 无用武之地；下划线前缀防 unused 警告。
    #[cfg(not(desktop))]
    let _ = &app;
    // 入口闸门：未解锁直接拒（错误码与其它会话命令同一张表）。
    if !state.session_unlocked() {
        return Err(IpcError::from(CoreError::NotUnlocked));
    }
    // 注入要花时间，入口先触点，防 auto-lock 在注入中途把库锁掉。
    state.touch_activity();

    let action_key = action.trim().to_ascii_lowercase();
    // 非法 action 是前端契约违例（设置窗只发五个固定值）→ 参数校验失败。
    let template = autotype_seq::template_for(&action_key)
        .map_err(|_| IpcError::from(CoreError::Validation("auto_type_action".to_string())))?;

    // ── 取明文（与 get_item 同一条解密路径；会话锁只跨解密，不跨注入）──
    let mut fields = {
        let session = state.lock_session().map_err(lock_err)?;
        let mut item = session.repo()?.get_item(&item_id)?;
        let ItemKind::Login(data) = &item.kind else {
            return Err(IpcError::from(CoreError::Validation(
                "auto_type_not_login".to_string(),
            )));
        };
        // 一到手立刻 Zeroizing 包住（照 copy_secret 先例）。URL 与 TOTP 密钥同属
        // 用户私密（「访问了哪个站点、用了哪个账号」），一并按敏感字符串托管。
        let fields = Fields {
            username: data
                .username
                .clone()
                .filter(|s| !s.is_empty())
                .map(Zeroizing::new),
            password: data
                .password
                .clone()
                .filter(|s| !s.is_empty())
                .map(Zeroizing::new),
            url: data
                .url
                .clone()
                .filter(|s| !s.is_empty())
                .map(Zeroizing::new),
            totp: data
                .totp_secret
                .clone()
                .filter(|s| !s.is_empty())
                .map(Zeroizing::new),
        };
        // 条目其余明文字段（标题/备注/原始字段…）用完即擦，
        // 不让整份明文条目副本活到注入结束。
        item.kind.wipe_plaintext();
        fields
    };

    // ── 动作级必填校验 ──
    match action_key.as_str() {
        "username" if fields.username.is_none() => {
            return Err(IpcError::shell(
                shell_codes::ERR_AUTOTYPE_NO_USERNAME,
                "err.autotype_no_username",
            ))
        }
        "url" if fields.url.is_none() => {
            return Err(IpcError::shell(
                shell_codes::ERR_AUTOTYPE_NO_URL,
                "err.autotype_no_url",
            ))
        }
        "totp" if fields.totp.is_none() => {
            return Err(IpcError::shell(
                shell_codes::ERR_AUTOTYPE_NO_TOTP,
                "err.autotype_no_totp",
            ))
        }
        // 其余动作都要密码；没有密码时自动输入无物可打（既有行为，错误码沿用）。
        "username" | "url" | "totp" => {}
        _ if fields.password.is_none() => {
            return Err(IpcError::from(CoreError::Validation(
                "auto_type_no_password".to_string(),
            )))
        }
        _ => {}
    }

    // totp 动作：现算动态码（core 唯一 TOTP 入口；密钥可能是 otpauth URI，由
    // `totp::code_at` 内部抽 secret，见其文档）。算出的 6 位码同样按敏感托管。
    if action_key == "totp" {
        let secret = fields.totp.as_ref().expect("totp 动作已校验密钥非空");
        let passwordms_core::totp::TotpCode { code, .. } =
            passwordms_core::totp::now(secret).map_err(IpcError::from)?;
        fields.totp = Some(Zeroizing::new(code));
    }

    // 热键触发时记录的目标窗口原始句柄（0 = 未记录，注入前校验必拒）。
    let target_raw = state.autotype_target.lock().map_err(lock_err)?.unwrap_or(0);

    // 注入前先把浮层藏起来：它此刻是前台窗口，不藏就等于"往自己身上打"。
    // 失焦即收起（lib.rs on_window_event）通常已把它藏了，这里显式再藏一次防竞态。
    // （浮层是桌面专属模块，Android 上没有第二个窗口可藏。）
    #[cfg(desktop)]
    crate::autotype::hide(&app);

    // ── 序列组装：default 无用户名时退化为仅密码（既有行为），其余按 action 模板 ──
    let template = match action_key.as_str() {
        "default" if fields.username.is_none() => "{PASSWORD}{ENTER}",
        _ => template,
    };
    // 模板是机器拼装的常量，解析失败属于内部不变量破坏 → internal，不该静默吞掉。
    let tokens = autotype_seq::parse(template)
        .map_err(|_| IpcError::internal("auto_type_template_parse"))?;

    // ── 注入（平台对称；会话锁已释放，阻塞注入不占锁，也不阻塞 IPC 线程池）──
    let result = tauri::async_runtime::spawn_blocking(move || {
        inject_with_focus(fields, target_raw, &tokens)
    })
    .await
    .map_err(|e| IpcError::internal(&format!("spawn_blocking auto_type_fill: {e}")))?;

    if result.is_ok() {
        // 注入成功补一次触点：整段注入期间的用户活动以完成时刻为准。
        state.touch_activity();
    }
    result
}

/// 注入时按占位符标记取值的一组字段（全部按敏感字符串托管）。
struct Fields {
    username: Option<Zeroizing<String>>,
    password: Option<Zeroizing<String>>,
    url: Option<Zeroizing<String>>,
    /// totp 动作下这里放的是**现算出的 6 位码**（不是密钥）。
    totp: Option<Zeroizing<String>>,
}

/// 解析 Token → 注入。**平台对称函数**：Windows 真实现 / 其它平台报不支持。
fn inject_with_focus(fields: Fields, target_raw: isize, tokens: &[Token]) -> IpcResult<()> {
    #[cfg(windows)]
    {
        windows_inject(fields, target_raw, tokens)
    }
    #[cfg(not(windows))]
    {
        // 非 Windows（Android）：运行时可读事实。cfg 块里禁止写 return，用尾表达式。
        let _ = (fields, target_raw, tokens);
        Err(IpcError::shell(
            shell_codes::ERR_NOT_SUPPORTED,
            "err.desktop_only",
        ))
    }
}

/// Windows 注入本体。
///
/// 前置校验链：保存的句柄 `IsWindow` 有效 → `SetForegroundWindow` → 等
/// [`FOCUS_SETTLE_MS`] → 前台确实是它。任何一步不过都按「焦点丢失」拒绝——
/// 宁可让用户手动粘贴，也绝不把密码打进一个没被看见的窗口。
#[cfg(windows)]
fn windows_inject(fields: Fields, target_raw: isize, tokens: &[Token]) -> IpcResult<()> {
    use passwordms_core::securemem;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, VK_RETURN, VK_TAB,
    };
    // VIRTUAL_KEY 与事件结构体同在 KeyboardAndMouse；前台/消息类 API 在 WindowsAndMessaging。
    use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetMessageExtraInfo, SetForegroundWindow,
    };

    /// KEYBDINPUT.dwExtraInfo 取本线程消息队列的附加值（与系统注入协同的惯例填充）。
    fn extra_info() -> usize {
        // SAFETY: 无参数，只读本线程消息队列的附加字段。LPARAM 是 isize 新类型。
        let lparam = unsafe { GetMessageExtraInfo() };
        lparam.0 as usize
    }

    /// 发一对「按下 + 抬起」的键盘事件。
    fn send_pair(down: INPUT, up: INPUT) {
        // SAFETY: 传入的 INPUT 是本函数栈上的完整合法结构；cbsize 用真实大小。
        unsafe {
            SendInput(&[down], std::mem::size_of::<INPUT>() as i32);
            SendInput(&[up], std::mem::size_of::<INPUT>() as i32);
        }
    }

    /// 造一个键盘事件的半件（按下或抬起，flags 决定）。
    fn key_event(wvk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: wvk,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: extra_info(),
                },
            },
        }
    }

    /// 用 KEYEVENTF_UNICODE 注入一个 UTF-16 码元（wVk 必须为 0）。
    fn send_unicode(unit: u16) {
        let down = key_event(Default::default(), unit, KEYEVENTF_UNICODE);
        let up = key_event(
            Default::default(),
            unit,
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        );
        send_pair(down, up);
    }

    /// 虚拟键注入（Tab / Enter 等控制键：走 VK，不走 UNICODE）。
    fn send_vk(vk: VIRTUAL_KEY) {
        let down = key_event(vk, 0, Default::default());
        let up = key_event(vk, 0, KEYEVENTF_KEYUP);
        send_pair(down, up);
    }

    /// 前台窗口是否仍是 `raw`。
    fn foreground_is(raw: isize) -> bool {
        // SAFETY: 只读前台窗口句柄。
        (unsafe { GetForegroundWindow() }.0 as isize) == raw
    }

    /// 取字段值；缺省（该动作用不到 / 条目没这个字段）按空串处理。
    fn borrow_opt(value: &Option<Zeroizing<String>>) -> std::borrow::Cow<'_, str> {
        match value {
            Some(v) => std::borrow::Cow::Borrowed(v.as_str()),
            None => std::borrow::Cow::Borrowed(""),
        }
    }

    // ── 目标句柄：IsWindow 现查（0 / 已销毁 / 假句柄一律拒，与 platform 同判据）──
    let saved = crate::platform::hwnd_if_valid(target_raw).ok_or_else(|| {
        IpcError::shell(
            shell_codes::ERR_AUTOTYPE_FOCUS_LOST,
            "err.autotype_focus_lost",
        )
    })?;

    // SAFETY: hwnd 来自 IsWindow 校验通过的句柄值，仅请求前台切换。
    let set_ok = unsafe { SetForegroundWindow(HWND(saved as *mut core::ffi::c_void)) };
    if !set_ok.as_bool() {
        return Err(IpcError::shell(
            shell_codes::ERR_AUTOTYPE_FOCUS_LOST,
            "err.autotype_focus_lost",
        ));
    }
    std::thread::sleep(std::time::Duration::from_millis(FOCUS_SETTLE_MS));
    if !foreground_is(saved) {
        // 焦点被别的东西抢走了（用户切走 / 系统弹窗）：中止。
        return Err(IpcError::shell(
            shell_codes::ERR_AUTOTYPE_FOCUS_LOST,
            "err.autotype_focus_lost",
        ));
    }

    for token in tokens {
        // 每个 Token 边界复查前台：变了立即中止，密码绝不外流。
        if !foreground_is(saved) {
            return Err(IpcError::shell(
                shell_codes::ERR_AUTOTYPE_FOCUS_LOST,
                "err.autotype_focus_lost",
            ));
        }
        match token {
            Token::Tab => send_vk(VK_TAB),
            Token::Enter => send_vk(VK_RETURN),
            Token::DelayMs(ms) => {
                std::thread::sleep(std::time::Duration::from_millis(u64::from(*ms)));
            }
            Token::Text(payload) => {
                // 字段占位符 → 真实内容（明文/动态码不经过解析器，见 autotype_seq 文档）；
                // 其余 → 字面文本。内容以 Zeroizing 借用形态进入本段，不另落副本。
                let text: std::borrow::Cow<'_, str> = match payload.as_str() {
                    "{USERNAME}" => borrow_opt(&fields.username),
                    "{PASSWORD}" => borrow_opt(&fields.password),
                    "{URL}" => borrow_opt(&fields.url),
                    "{TOTP}" => borrow_opt(&fields.totp),
                    other => std::borrow::Cow::Borrowed(other),
                };
                // UTF-16 码元的小端字节缓冲（KEYEVENTF_UNICODE 按 wScan 逐码元发）。
                // 用字节缓冲是为了用完能直接过 `securemem::wipe`（吃 &mut [u8]），
                // 不需要手工转切片的 unsafe。
                let mut buf: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
                // `as_chunks::<2>()`：固定 2 字节一组，比 `chunks_exact(2)` 少一次
                // 运行期对齐检查（与 `capture_screen` 的 `as_chunks_mut::<4>` 同款写法）。
                for pair in buf.as_chunks::<2>().0 {
                    send_unicode(u16::from_le_bytes(*pair));
                    std::thread::sleep(std::time::Duration::from_millis(PER_CHAR_DELAY_MS));
                }
                // 明文缓冲用完即擦（安全基线：不依赖作用域回收）。
                securemem::wipe(&mut buf);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(PER_TOKEN_DELAY_MS));
    }
    Ok(())
}
