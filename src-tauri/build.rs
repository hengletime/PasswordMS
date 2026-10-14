// Tauri 构建脚本：生成能力/权限 schema 与资源嵌入。
//
// 附加职责一（Android 出包配套）：
// 为移动端恢复 `cdylib` 后，Windows(gnullvm) 侧链接 `passwordms_lib.dll` 时，
// GNU 版 `ld` 对 DLL **默认开启 `--export-all-symbols`**，会把整棵依赖树的全局
// 符号全部导出去，超过 PE 导出序号上限（65535）而链接失败：
//   rust-lld: error: too many exported symbols (got 120869, max 65535)
// 桌面端并不需要 cdylib 的导出表（exe 经 rlib 链接），故**仅对本目标**关闭
// 「自动导出全部」，保留 rustc 显式列出的导出即可。
//
// 放在 build.rs（随仓库提交、按 `TARGET` 条件判断）而不是 .cargo/config.toml
// （机器本地、已被 .gitignore 排除），以保证新克隆的机器无需额外配置即可编译。
//
// 附加职责二：Windows 资源编译器 `windres` 的 PATH 兜底（事故 D-33）
// 详见 `ensure_windres_on_path()` 的注释。

use std::env;
use std::path::PathBuf;

fn main() {
    ensure_windres_on_path();

    // 仅对 x86_64-pc-windows-gnullvm 生效：Android 目标用的是 NDK clang 链接器，
    // 该参数不会也不应该加进去（Android JNI 导出依赖默认的显式导出）。
    if env::var("TARGET").as_deref() == Ok("x86_64-pc-windows-gnullvm") {
        println!("cargo::rustc-link-arg-cdylib=--exclude-all-symbols");
    }
    tauri_build::build()
}

/// 把 `windres` 所在的目录前置进本进程的 PATH，让下游能找到它。
///
/// # 为什么需要这层兜底（事故 D-33，2026-10-14）
///
/// `tauri_build::build()` → `tauri-winres` → `embed-resource 3.x` 这条链路里，
/// Windows + GNU 宿主走的 `windows_not_msvc.rs` 把编译器探测**写死**成
/// `Command::new("windres")`，即**只按 PATH 查找**。它既**不认** `RC` /
/// `RC_<target>` 环境变量（那两个变量只在 msvc 分支被消费），
/// `WindowsResource::set_windres_path()` 又早在 0.1.1 就成了 deprecated 的 no-op
/// （"handled by the embed-resource crate"），官方没有留任何注入口子。
/// 本机既无 MSVC 也无系统级 mingw-w64，`windres` 只存在于用户自备目录里。
///
/// 于是那段时间的构建全靠手工 `export PATH=...`，而这条 PATH **从未固化** ——
/// 换一个 shell、换一个构建入口（npm/npx 起的进程、GUI 里点构建）就丢，
/// 构建立刻炸在 `tauri-winres-0.3.6/src/lib.rs:543`：
///     panicked at ... called `Result::unwrap()` on an `Err` value: NotAttempted("windres")
///
/// # 为什么不用 `.cargo/config.toml` 的 `[env] PATH = { prepend = true }`
///
/// 实测（同日，在同一份 config 里做了对照探针）**在 Windows 上无效**：
/// 自定义变量能进 build script（`PROBE=yes` 读到了），但 PATH 前置没生效。
/// 原因是 Windows 的环境变量名大小写不敏感、而环境块里实际存的是系统注入的
/// `Path`；cargo 再把 `PATH` 作为**新条目**塞进去，两个条目并存，
/// build script 读到的仍是旧的 `Path`。所以只能由构建脚本自己动手。
///
/// # 为什么由 build.rs 而不是别处
///
/// 与文件头「附加职责一」同一个理由：build.rs 随仓库提交、按需自愈，
/// 新克隆的机器只要按需设一个环境变量即可编译，**不依赖任何人工前置操作**。
///
/// # 查找顺序
///
/// 1. 环境变量 `PMS_WINRES_DIR`（跨机器的通用入口，指向含 `windres.exe` 的目录）
/// 2. `%USERPROFILE%\.workbuddy\tools\mingw-bin\`（本机 Commit 的约定位置）
/// 3. `C:\msys64\mingw64\bin\`、`C:\mingw64\bin\`（常见 mingw-w64 安装位置）
///
/// 若 PATH 上已有 `windres` 则什么都不做；全找不到时只发 warning，
/// 把失败留给下游报出原始错误（避免在这里吞掉真实症状）。
fn ensure_windres_on_path() {
    // 已经把环境变量设好的机器（或 PATH 里本来就有）直接放行。
    if find_in_path("windres.exe").is_some() || find_in_path("windres").is_some() {
        return;
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = env::var_os("PMS_WINRES_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    if let Some(home) = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) {
        candidates.push(PathBuf::from(home).join(".workbuddy").join("tools").join("mingw-bin"));
    }
    candidates.push(PathBuf::from(r"C:\msys64\mingw64\bin"));
    candidates.push(PathBuf::from(r"C:\mingw64\bin"));

    for dir in candidates {
        if !dir.join("windres.exe").is_file() {
            continue;
        }
        if let Some(path) = env::var_os("PATH") {
            let mut parts: Vec<PathBuf> = vec![dir.clone()];
            parts.extend(env::split_paths(&path));
            if let Ok(joined) = env::join_paths(parts) {
                // 关键：必须改**本进程的环境块**（env::set_var），
                // 而不是给某个 Command 单独 env()。资源编译是
                // tauri_build::build() 内部另起的子进程，只有前者会被它继承。
                //
                // 注：`set_var` 在 Rust 2024 edition 起是 unsafe。
                //     本 crate 仍是 edition 2021（见 workspace Cargo.toml），
                //     将来升级时这里需要包一层 unsafe 并附 SAFETY 说明：
                //     build script 是单线程的，此刻不存在并发读写环境的风险。
                env::set_var("PATH", joined);
            }
        }
        println!(
            "cargo:warning=windres 不在 PATH 上，已自动前置 {}（D-33 兜底）",
            dir.display()
        );
        return;
    }

    println!(
        "cargo:warning=PATH 上找不到 windres，且候选目录均不存在；\
         Windows 资源嵌入（图标/manifest）会失败。\
         设 PMS_WINRES_DIR 指向含 windres.exe 的目录，\
         或见 docs/03b-verification.md §8.2"
    );
}

/// 在 PATH 里找可执行文件（不引第三方依赖，手写即可）。
fn find_in_path(exe: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|candidate| candidate.is_file())
}
