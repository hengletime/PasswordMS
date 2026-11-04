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

use crate::dto::AutotypeCandidateDto;
use crate::error::{lock_err, shell_codes, IpcError, IpcResult};
use crate::state::{AppState, AutotypeTarget};

/// 每个字符注入后的间隔（毫秒）。
///
/// 1.5.41：6 → 12。6ms 在记事本、普通输入框里没问题，但 **Chromium 的地址栏
/// （omnibox）跟不上**——它在收到输入后会做 inline autocomplete 并把补全的那截选中，
/// 太快的后续字符会替换掉那截选中内容，表现为"网址打了一半就断了"。慢一倍换稳定，
/// 30 个字符也就多 180ms。
#[cfg(windows)]
const PER_CHAR_DELAY_MS: u64 = 12;
/// 每个 Token（文本块 / Tab / Enter / 停顿）之间的间隔（毫秒）。
#[cfg(windows)]
const PER_TOKEN_DELAY_MS: u64 = 50;
/// 切回目标窗口后的等待（毫秒）：`SetForegroundWindow` 是异步的，焦点转移
/// 需要一点时间；等完再校验，不等就是"对着旧窗口注入"。
///
/// 1.5.41：200 → 300。Chromium 系窗口拿回前台后还要让 renderer 侧的编辑框恢复
/// 输入状态，200ms 常常不够（表现为第一个字符丢在空白里）。
#[cfg(windows)]
const FOCUS_SETTLE_MS: u64 = 300;
/// 开始打一段文本前的静默期（毫秒）。
///
/// 1.5.41：给"编辑框被自动补全选中一截"留出消化时间（见 [`PER_CHAR_DELAY_MS`]）。
#[cfg(windows)]
const TEXT_SETTLE_MS: u64 = 150;

/// 自动输入：把 `item_id` 对应条目的**指定内容**打进热键触发那一刻的前台窗口。
///
/// `action` ∈ `default | username | password | url | totp`（与前端选择窗的键逐字一致，
/// 模板由 core 的 [`autotype_seq::template_for`] 给出）。
#[tauri::command(rename_all = "snake_case")]
pub async fn auto_type_fill(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    state: tauri::State<'_, AppState>,
    item_id: String,
    action: String,
) -> IpcResult<()> {
    // Android 上没有浮层可藏、也没有窗口身份要查；下划线前缀防 unused 警告。
    #[cfg(not(desktop))]
    let _ = (&app, &window);
    // 【S3】只有浮层窗口能调这条命令。Tauri 的自定义命令不受 capability 约束
    // （docs/02 §9 有记）：任何窗口的 JS 都能 invoke 任意自定义命令，而这是唯一
    // 会把密钥"打"进任意前台窗口的命令——多一道窗口身份锁，正常路径零成本。
    #[cfg(desktop)]
    if window.label() != crate::autotype::LABEL {
        return Err(IpcError::from(CoreError::Validation(
            "auto_type_window".to_string(),
        )));
    }
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
    let fields = {
        let session = state.lock_session().map_err(lock_err)?;
        let mut item = session.repo()?.get_item(&item_id)?;
        // 【M4】先算字段、**再无条件擦整条条目**：此前的 `let else { return }` 早退
        // 会跳过擦除，把解密后的条目明文（含附件文件名）整份留在堆上。这里把
        // 非登录条目折成 `None`，擦除语句在任何分支都得执行到。
        let fields = match &item.kind {
            ItemKind::Login(data) => Some(Fields {
                // 一到手立刻 Zeroizing 包住（照 copy_secret 先例）。URL 与 TOTP 密钥
                // 同属用户私密（「访问了哪个站点、用了哪个账号」），一并按敏感托管。
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
            }),
            _ => None,
        };
        // 整条擦除：kind 内的字段 + 内嵌附件的原始文件名（`Item::wipe_plaintext`）。
        item.wipe_plaintext();
        fields
    };
    let Some(mut fields) = fields else {
        return Err(IpcError::from(CoreError::Validation(
            "auto_type_not_login".to_string(),
        )));
    };
    // 【S7】非 totp 动作不需要 TOTP 密钥：当场丢掉，不让它进注入层（少一份
    // 在阻塞线程上存活的秘密副本）。
    if action_key != "totp" {
        fields.totp = None;
    }

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
        // 上面的动作级校验已经拦过 `None`，但这里**不许 unwrap**：这句话跑在
        // 持锁之后的注入路径上，一旦校验逻辑将来被改动，panic 会跨过 unwind 边界
        // 把整个 IPC 线程带走（前端只看到 invoke 永远不 resolve）。取不到就照样
        // 报同一个错误码——用户看到的话与校验路径完全一致。
        let Some(secret) = fields.totp.as_ref() else {
            return Err(IpcError::shell(
                shell_codes::ERR_AUTOTYPE_NO_TOTP,
                "err.autotype_no_totp",
            ));
        };
        let passwordms_core::totp::TotpCode { code, .. } =
            passwordms_core::totp::now(secret).map_err(IpcError::from)?;
        fields.totp = Some(Zeroizing::new(code));
    }

    // 热键触发时记录的目标窗口（句柄 + pid；None = 未记录/是自身窗口，注入前必拒）。
    let target = *state.autotype_target.lock().map_err(lock_err)?;

    // 【1.5.41】预检必须在 `hide` **之前**——这是失败原因能出现在"还看得见的浮层"
    // 里的唯一时机。管理员权限的目标程序在这里被拦下：跨完整性注入时 `SendInput`
    // 返回"成功"但字一个都没进去（主人 2026-11-04 实测，日志里连失败都没有），
    // 所以注入层根本发现不了，只能提前判。
    #[cfg(windows)]
    precheck_target(target)?;

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
    let log_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        inject_with_focus(&log_app, fields, target, &tokens)
    })
    .await
    .map_err(|e| IpcError::internal(&format!("spawn_blocking auto_type_fill: {e}")))?;

    if result.is_ok() {
        // 注入成功补一次触点：整段注入期间的用户活动以完成时刻为准。
        state.touch_activity();
    } else {
        // 失败必须让用户看见（1.5.40）。注入前浮层已被 [`crate::autotype::hide`]，
        // 前端的失败文案会写进一个**已隐藏的窗口**——1.5.39 真机验收的症状就是
        // 「按了没反应，也没提示」：`ERR_AUTOTYPE_BLOCKED` 其实产生了，但没人看得见。
        // 这里把浮层只"重新显示"（不重发事件，搜索词与选中项留着，方便换动作重试）。
        #[cfg(desktop)]
        crate::autotype::reveal(&app);
        // 失败留痕：只记错误码，不记任何字段值（安全基线：参数值一律不进日志）。
        if let Err(e) = &result {
            crate::state::append_log(&app, &format!("[autotype] 注入失败：{}", e.code));
        }
    }
    result
}

/// 浮层条目清单（S2 / M3 的 Rust 半边）：只回**五个字段**，绝不带密码等明文。
///
/// 为什么不让浮层继续用 `list_items`：那个接口回的是完整条目 DTO，浮层只需要
/// 「选中哪条」，多给的每一个字段都是一份可以被打包带走的明文面。这里在 Rust 侧
/// 直接裁剪成 [`AutotypeCandidateDto`]——注入本身只认 `item_id`，密码由
/// `auto_type_fill` 内部现取，从不跨 IPC。
///
/// 五个字段对应前端 `rankCandidates` 的真实入参（`title`/`username`/`url` 打分、
/// `use_count` 排序），少一个都会静默退化，详见 DTO 上的注释。
///
/// **命令不变量：只回登录条目**。笔记/遗留附件没有可注入的字段，列出来只会让
/// 「选中之后什么都没发生」变成新的困惑。这条判断在 core 的
/// [`passwordms_core::autotype_candidates::login_candidates`] 里做（有单测覆盖），
/// 命令层只剩三件事：闸门 → 取条目 → 擦明文。
///
/// **这里不截断条数**：前端的 `quick-search.ts` `RESULT_LIMIT = 40` 是**显示上限**，
/// 不是传输上限——先截到 40 再排序等于让排序看不到后面的条目（用户搜一个第 41 条的
/// 站点就永远搜不到）。全量过 IPC 的代价是几条 JSON，而候选行只有五个字段、不含密码。
#[tauri::command(rename_all = "snake_case")]
pub fn autotype_candidates(
    state: tauri::State<'_, AppState>,
) -> IpcResult<Vec<AutotypeCandidateDto>> {
    // 与注入同一道闸门：锁着的时候没有条目可列。
    if !state.session_unlocked() {
        return Err(IpcError::from(CoreError::NotUnlocked));
    }
    state.touch_activity();
    let session = state.lock_session().map_err(lock_err)?;
    let repo = session.repo()?;
    let mut items = repo.list_items(None, false, None);
    // 业务判断下沉 core（只收登录条目 + 裁成五字段），命令层不重复实现。
    let rows = passwordms_core::autotype_candidates::login_candidates(&items);
    // 列表副本里带着整份明文（密码/备注/TOTP 密钥/附件文件名）：五字段取完立即
    // 整条擦除，不留一批明文条目在堆上等自然回收。
    for item in &mut items {
        item.wipe_plaintext();
    }
    Ok(rows.into_iter().map(AutotypeCandidateDto::from).collect())
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
fn inject_with_focus(
    app: &tauri::AppHandle,
    fields: Fields,
    target: Option<AutotypeTarget>,
    tokens: &[Token],
) -> IpcResult<()> {
    #[cfg(windows)]
    {
        windows_inject(app, fields, target, tokens)
    }
    #[cfg(not(windows))]
    {
        // 非 Windows（Android）：运行时可读事实。cfg 块里禁止写 return，用尾表达式。
        let _ = (app, fields, target, tokens);
        Err(IpcError::shell(
            shell_codes::ERR_NOT_SUPPORTED,
            "err.desktop_only",
        ))
    }
}

/// 焦点丢失：句柄失效 / 句柄换了主人 / 切回失败 / 注入中途前台变了。
#[cfg(windows)]
fn focus_lost() -> IpcError {
    IpcError::shell(
        shell_codes::ERR_AUTOTYPE_FOCUS_LOST,
        "err.autotype_focus_lost",
    )
}

/// 注入途中被系统拦下（`SendInput` 返回的事件数不足）。
#[cfg(windows)]
fn blocked() -> IpcError {
    IpcError::shell(shell_codes::ERR_AUTOTYPE_BLOCKED, "err.autotype_blocked")
}

/// 进程是否以提权身份运行（`TokenElevation.TokenIsElevated`）。
///
/// `None` = 查不到（进程已退出、受保护进程、没有查询权限）。**查不到不当成"是"**：
/// 拿不到信息就拦掉正常场景，比漏判更糟。
///
/// 为什么用 `TokenElevation` 而不解析完整性级别 SID：在"会不会被 UIPI 拦"这件事上
/// 两者等价（Elevated 即中/高完整性），而前者不需要把 SID 拆出来比，几行就够。
#[cfg(windows)]
fn process_is_elevated(pid: u32) -> Option<bool> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    // 注意：`OpenProcessToken` 在 windows 0.62 里挂在 Threading 下（不在 Security），
    // 与 Win32 头文件的直觉相反——照头文件去 Security 里找会 E0432。
    use windows::Win32::System::Threading::{
        OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: 只申请查询权限、不继承句柄；下面每条出路都会关掉已打开的句柄。
    let process: HANDLE =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut token = HANDLE::default();
    // SAFETY: process 是刚打开的有效句柄；token 是本栈上的合法 out 参数。
    let token_ok = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.is_ok();
    let mut elevation = TOKEN_ELEVATION::default();
    let mut ret_len = 0u32;
    let queried = token_ok
        && unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut TOKEN_ELEVATION as *mut core::ffi::c_void),
                u32::try_from(std::mem::size_of::<TOKEN_ELEVATION>()).unwrap_or(0),
                &mut ret_len,
            )
        }
        .is_ok();
    // SAFETY: 只关闭确实打开过的句柄；关闭失败无补救动作，忽略。
    unsafe {
        if token_ok {
            let _ = CloseHandle(token);
        }
        let _ = CloseHandle(process);
    }
    queried.then_some(elevation.TokenIsElevated != 0)
}

/// 注入**前**预检：目标窗口仍有效、仍属于记录的进程、且没有提权。
///
/// 必须在 [`crate::autotype::hide`] **之前**调用——这是让失败原因出现在"还看得见
/// 的浮层"里的唯一时机。理由（1.5.40 的教训）：
///
/// - hide 之后的失败只能靠 [`crate::autotype::reveal`] 把窗口塞回前台，而**目标窗口
///   权限更高时普通进程抢不回前台**（前台锁），那条路对管理员程序必然失效；
/// - 更根本的是：跨完整性注入时 `SendInput` **返回成功**，注入层压根不会报错，
///   所以"事后检测"这条路本身就不存在。
///
/// 注入层仍会重复做句柄/pid 复核：预检到真正注入之间有窗口期，用户可能已经切走。
/// 两份检查目的不同——这份是为了"报得出错"，那份是为了"绝不误打"。
#[cfg(windows)]
fn precheck_target(target: Option<AutotypeTarget>) -> IpcResult<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    let Some(target) = target else {
        // 热键那一刻前台是本进程自己的窗口，或被清掉了：没有可注入的目标。
        return Err(focus_lost());
    };
    if crate::platform::hwnd_if_valid(target.hwnd).is_none() {
        return Err(focus_lost());
    }
    let mut pid = 0u32;
    // SAFETY: 句柄刚过 IsWindow 校验；pid 是本栈上的合法 out 参数。
    unsafe {
        GetWindowThreadProcessId(HWND(target.hwnd as *mut core::ffi::c_void), Some(&mut pid))
    };
    if pid == 0 || pid != target.pid {
        return Err(focus_lost());
    }
    // 提权预检：目标 elevated、自身不是 → UIPI 必然丢弃我们的输入。
    if matches!(
        (
            process_is_elevated(target.pid),
            process_is_elevated(std::process::id())
        ),
        (Some(true), Some(false))
    ) {
        return Err(IpcError::shell(
            shell_codes::ERR_AUTOTYPE_TARGET_ELEVATED,
            "err.autotype_target_elevated",
        ));
    }
    Ok(())
}

/// Windows 注入本体。
///
/// 前置校验链：记录过目标 → 句柄 `IsWindow` 有效 → **句柄仍属于记录的那个进程**
/// （S6）→ `SetForegroundWindow` → 等 [`FOCUS_SETTLE_MS`] → 前台确实是它。任何一步
/// 不过都按「焦点丢失」拒绝——宁可让用户手动粘贴，也绝不把密码打进一个没被看见的窗口。
///
/// 注入途中 `SendInput` 返回不足（被 UIPI 拦下）时报 [`ERR_AUTOTYPE_BLOCKED`]：
/// 「系统不让打」与「焦点跑了」是两件事，用户要做的动作也不同（见错误码注释）。
#[cfg(windows)]
fn windows_inject(
    app: &tauri::AppHandle,
    fields: Fields,
    target: Option<AutotypeTarget>,
    tokens: &[Token],
) -> IpcResult<()> {
    use passwordms_core::securemem;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_RETURN, VK_TAB,
    };
    // 前台/消息/窗口归属类 API 在 WindowsAndMessaging。
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
    };

    /// `KEYBDINPUT.dwExtraInfo`。
    ///
    /// **显式写 0，不是省掉**：该字段的语义是"本线程消息队列的附加值"，惯例上与
    /// `GetMessageExtraInfo()` 保持一致，好让目标程序分得清输入来源。但注入跑在
    /// `spawn_blocking` 的工作线程上，**那个线程没有消息队列**，`GetMessageExtraInfo()`
    /// 在那里只会返回 0；为读一个恒为 0 的常量去调消息类 API，反而暗示"这里有什么
    /// 需要读"。写 0 就是事实，也少一次跨模块 unsafe。
    const EXTRA_INFO: usize = 0;

    /// 发一对「按下 + 抬起」的键盘事件。
    ///
    /// **两次的返回值都要检查**（M2）：`SendInput` 返回实际插入系统队列的事件数，
    /// 被 UIPI 拦下（目标是管理员权限进程）或输入桌面被切换时返回 0，且**不报错**。
    /// 不检查就等于"用户以为打上了，实际一个字都没进去"——尤其是密码字段，用户
    /// 大概率会直接按回车提交一个空表单。
    fn send_pair(down: INPUT, up: INPUT) -> IpcResult<()> {
        let size = std::mem::size_of::<INPUT>() as i32;
        // SAFETY: 传入的 INPUT 是本函数栈上的完整合法结构；cbsize 用真实大小。
        if unsafe { SendInput(&[down], size) } != 1 {
            return Err(blocked());
        }
        // SAFETY: 同上，抬起事件同样是本栈上的完整结构。
        if unsafe { SendInput(&[up], size) } != 1 {
            return Err(blocked());
        }
        Ok(())
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
                    dwExtraInfo: EXTRA_INFO,
                },
            },
        }
    }

    /// 用 KEYEVENTF_UNICODE 注入一个 UTF-16 码元（wVk 必须为 0）。
    fn send_unicode(unit: u16) -> IpcResult<()> {
        let down = key_event(Default::default(), unit, KEYEVENTF_UNICODE);
        let up = key_event(
            Default::default(),
            unit,
            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
        );
        send_pair(down, up)
    }

    /// 虚拟键注入（Tab / Enter 等控制键：走 VK，不走 UNICODE）。
    fn send_vk(vk: VIRTUAL_KEY) -> IpcResult<()> {
        let down = key_event(vk, 0, Default::default());
        let up = key_event(vk, 0, KEYEVENTF_KEYUP);
        send_pair(down, up)
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

    // ── 目标：先要有记录（None = 热键那一刻前台是本进程自己的窗口，或被清掉了）──
    let Some(target) = target else {
        return Err(focus_lost());
    };
    // 句柄 IsWindow 现查（0 / 已销毁 / 假句柄一律拒，与 platform 同判据）。
    let saved = crate::platform::hwnd_if_valid(target.hwnd).ok_or_else(focus_lost)?;

    // 【S6】`IsWindow` 只证明"这个句柄有人在用"，不证明"还是那个窗口"：窗口句柄
    // 会被系统回收再分配给别的进程，而记录到点击之间可能隔了几十秒。注入前用
    // `GetWindowThreadProcessId` 复核归属进程，不一致按焦点丢失拒绝。
    let mut pid = 0u32;
    // SAFETY: saved 已通过 IsWindow 校验；pid 是本栈上的合法 out 参数，不保留指针。
    unsafe { GetWindowThreadProcessId(HWND(saved as *mut core::ffi::c_void), Some(&mut pid)) };
    if pid == 0 || pid != target.pid {
        return Err(focus_lost());
    }

    // SAFETY: hwnd 来自 IsWindow 校验通过的句柄值，仅请求前台切换。
    let set_ok = unsafe { SetForegroundWindow(HWND(saved as *mut core::ffi::c_void)) };
    if !set_ok.as_bool() {
        return Err(focus_lost());
    }
    std::thread::sleep(std::time::Duration::from_millis(FOCUS_SETTLE_MS));
    if !foreground_is(saved) {
        // 焦点被别的东西抢走了（用户切走 / 系统弹窗）：中止。
        return Err(focus_lost());
    }

    for token in tokens {
        // 每个 Token 边界复查前台：变了立即中止，密码绝不外流。
        if !foreground_is(saved) {
            return Err(focus_lost());
        }
        match token {
            Token::Tab => send_vk(VK_TAB)?,
            Token::Enter => send_vk(VK_RETURN)?,
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
                // 开口前先静默一下：Chromium 系编辑框（尤其地址栏）在上一段输入后
                // 会跑一次自动补全并选中补全内容，立刻接着打会把那截选中替换掉。
                std::thread::sleep(std::time::Duration::from_millis(TEXT_SETTLE_MS));
                for pair in buf.as_chunks::<2>().0 {
                    send_unicode(u16::from_le_bytes(*pair))?;
                    std::thread::sleep(std::time::Duration::from_millis(PER_CHAR_DELAY_MS));
                }
                // 明文缓冲用完即擦（安全基线：不依赖作用域回收）。
                securemem::wipe(&mut buf);
                // 留证据（**不含内容、不含长度**）：SendInput 说成功而用户没看到字，
                // 就是目标应用吞了。没有这行，"没注入"和"注入了没人收"在日志里长得
                // 一模一样——1.5.40 排查管理员程序那次就卡在这里。
                crate::state::append_log(app, "[autotype] 文本段注入完成");
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(PER_TOKEN_DELAY_MS));
    }
    Ok(())
}
