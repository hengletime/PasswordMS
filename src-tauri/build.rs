// Tauri 构建脚本：生成能力/权限 schema 与资源嵌入。
//
// 附加职责（Android 出包配套）：
// 为移动端恢复 `cdylib` 后，Windows(gnullvm) 侧链接 `passwordms_lib.dll` 时，
// GNU 版 `ld` 对 DLL **默认开启 `--export-all-symbols`**，会把整棵依赖树的全局
// 符号全部导出去，超过 PE 导出序号上限（65535）而链接失败：
//   rust-lld: error: too many exported symbols (got 120869, max 65535)
// 桌面端并不需要 cdylib 的导出表（exe 经 rlib 链接），故**仅对本目标**关闭
// 「自动导出全部」，保留 rustc 显式列出的导出即可。
//
// 放在 build.rs（随仓库提交、按 `TARGET` 条件判断）而不是 .cargo/config.toml
// （机器本地、已被 .gitignore 排除），以保证新克隆的机器无需额外配置即可编译。
fn main() {
    // 仅对 x86_64-pc-windows-gnullvm 生效：Android 目标用的是 NDK clang 链接器，
    // 该参数不会也不应该加进去（Android JNI 导出依赖默认的显式导出）。
    if std::env::var("TARGET").as_deref() == Ok("x86_64-pc-windows-gnullvm") {
        println!("cargo::rustc-link-arg-cdylib=--exclude-all-symbols");
    }
    tauri_build::build()
}
