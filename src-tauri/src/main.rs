// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // CLI 모드(`--hub-auth-header`)는 Tauri를 초기화하기 전에 분기한다 — 창·Dock 아이콘·
    // WebView·updater가 전혀 생기지 않는다(설계 docs/design/device-token-auth.md §7.3).
    if let Some(code) = tauri_app_lib::cli_mode::dispatch() {
        std::process::exit(code);
    }
    tauri_app_lib::run()
}
