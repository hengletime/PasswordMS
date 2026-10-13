// 桌面可执行入口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    passwordms_lib::run();
}
