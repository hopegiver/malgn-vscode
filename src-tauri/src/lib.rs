mod autonomy;
mod cli_launcher;
mod cloudflare_integration;
mod config;
mod dev_tools;
mod github_integration;
mod google_oauth;
mod jira_integration;
mod mcp_manager;
mod otel_settings;
mod plugins;
mod process_util;
mod session_chat;
mod session_list;
mod session_watch;
mod usage_stats;
mod workspace;

// `autonomy`/`session_chat`이 이 크레이트 루트 경로(`crate::resolve_validated_project_root`,
// `crate::scan_workspace_projects`)로 재사용한다 — 함수 정의는 `workspace/` 서브모듈로
// 옮겼지만 이 재노출로 두 모듈의 호출부는 손대지 않아도 된다.
pub(crate) use workspace::{resolve_validated_project_root, scan_workspace_projects};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let sessions_handle = app.handle().clone();
            std::thread::spawn(move || {
                session_watch::watch_claude_sessions_dir(sessions_handle);
            });
            // 어제까지의 사용량 캐시를 앱 시작과 동시에 백그라운드에서 미리 채워둔다
            // — 사용자가 "사용량 통계"를 처음 열었을 때부터 이미 준비돼 있게.
            std::thread::spawn(|| {
                usage_stats::get_or_refresh_historical_daily_usage();
            });
            // 자율업무 스케줄러 — 10초 tick으로 워크스페이스 전체를 훑어 due한
            // task를 `claude -p`로 무인 실행한다. 앱이 켜져 있는 동안만 돈다.
            // 리로드가 곧 tick 본체다(설계 §1) — 별도 워처 없이 설정 변경에
            // 최대 10초 안에 반응한다.
            autonomy::spawn_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            session_list::list_claude_sessions,
            workspace::list_workspace_projects,
            dev_tools::check_dev_tools,
            dev_tools::preview_dev_tool_update,
            dev_tools::update_dev_tool,
            dev_tools::install_dev_tool,
            dev_tools::open_manual_instruction,
            plugins::list_installed_plugins,
            plugins::list_known_marketplaces,
            otel_settings::otel_settings_get,
            otel_settings::otel_settings_save,
            plugins::update_plugin,
            plugins::refresh_marketplaces,
            workspace::list_project_tree,
            workspace::read_project_file,
            usage_stats::get_daily_usage,
            usage_stats::get_daily_detail,
            google_oauth::google_oauth_login,
            github_integration::github_status,
            github_integration::github_connect,
            github_integration::github_disconnect,
            cloudflare_integration::cloudflare_status,
            cloudflare_integration::cloudflare_connect,
            cloudflare_integration::cloudflare_disconnect,
            jira_integration::jira_status,
            jira_integration::jira_connect,
            jira_integration::jira_disconnect,
            autonomy::autonomy_list,
            autonomy::autonomy_save_task,
            autonomy::autonomy_delete_task,
            autonomy::autonomy_set_enabled,
            autonomy::autonomy_runtime_status,
            config::malgn_agent_config_get,
            mcp_manager::mcp_list,
            mcp_manager::mcp_get,
            mcp_manager::mcp_add,
            mcp_manager::mcp_remove,
            mcp_manager::mcp_catalog_list,
            mcp_manager::mcp_install,
            mcp_manager::mcp_login,
            session_chat::read_session_transcript,
            session_chat::send_session_message,
            session_chat::start_new_session_message,
            session_chat::cancel_session_turn
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // 앱 종료 시 자식 프로세스 정리(설계 §1-4) — 진행 중인 자율업무
            // `claude -p`가 고아 프로세스로 남지 않도록, 종료 요청 시점에
            // 최대 2초만 대기하며 정리한다(그 이상 앱 종료를 붙잡지 않는다).
            if let tauri::RunEvent::ExitRequested { .. } = event {
                session_chat::request_shutdown();
                autonomy::request_shutdown_and_wait(std::time::Duration::from_secs(2));
            }
        });
}
