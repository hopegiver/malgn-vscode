mod app_links;
mod autonomy;
mod claude_auth;
mod cli_launcher;
mod cloudflare_integration;
mod config;
mod dev_auto_login;
mod dev_tools;
mod fs_atomic;
mod github_integration;
mod global_cwd;
mod google_oauth;
mod mcp_manager;
mod otel_settings;
mod plugins;
mod process_util;
mod session_chat;
mod session_list;
mod session_watch;
mod usage_stats;
mod window_focus;
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
        // GitHub Releases(`latest.json`) 기반 자동업데이트. 서명 공개키는
        // tauri.conf.json의 `plugins.updater.pubkey`에 실제 값으로 설정돼 있다
        // (정본은 그 파일 하나 — 여기엔 값을 옮기지 않는다). 이 pubkey는
        // `check()`에서는 전혀 참조되지 않고, 오직 `download()` 내부의
        // 서명 검증(`verify_signature()`)에서만 쓰인다. 따라서 pubkey가
        // 비어 있으면 `check()`는 통과하지만 `download()`가 Err를 반환해
        // (fail-closed, no-op 아님) 업데이트가 영구히 적용되지 않는 상태가
        // 되므로 반드시 유효한 값이 채워져 있어야 한다. 서명용 개인키(비밀번호
        // 포함)는 저장소에 두지 않고 GitHub Actions repo secret
        // `TAURI_SIGNING_PRIVATE_KEY`(+선택 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`)로만
        // 주입한다 — tauri-cli가 `tauri build` 시 이 두 env를 자동으로 읽어
        // 업데이트 아티팩트에 서명하고 `.sig`를 만든다(코드 변경 불필요).
        .plugin(tauri_plugin_updater::Builder::new().build())
        // 업데이트 설치 후 재시작(`relaunch()`)에 필요. capabilities/default.json엔
        // 필요한 최소 권한만(`process:allow-restart`) 부여했다 — `allow-exit`은
        // 이 기능에 불필요해 제외.
        .plugin(tauri_plugin_process::init())
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
            app_links::app_links_get,
            app_links::app_links_save,
            app_links::app_links_open,
            session_list::list_claude_sessions,
            workspace::list_workspace_projects,
            dev_tools::check_dev_tools,
            dev_tools::preview_dev_tool_update,
            dev_tools::update_dev_tool,
            dev_tools::install_dev_tool,
            dev_tools::open_manual_instruction,
            plugins::list_installed_plugins,
            plugins::list_known_marketplaces,
            plugins::list_global_catalog,
            otel_settings::otel_settings_get,
            otel_settings::otel_settings_save,
            plugins::update_plugin,
            plugins::install_plugin,
            plugins::refresh_marketplaces,
            plugins::add_marketplace,
            plugins::remove_marketplace,
            workspace::list_project_tree,
            workspace::read_project_file,
            usage_stats::get_daily_usage,
            usage_stats::get_daily_detail,
            google_oauth::google_oauth_login,
            dev_auto_login::dev_auto_login,
            github_integration::github_status,
            github_integration::github_connect,
            github_integration::github_disconnect,
            cloudflare_integration::cloudflare_status,
            cloudflare_integration::cloudflare_connect,
            cloudflare_integration::cloudflare_disconnect,
            autonomy::autonomy_list,
            autonomy::autonomy_save_task,
            autonomy::autonomy_delete_task,
            autonomy::autonomy_set_enabled,
            autonomy::autonomy_runtime_status,
            autonomy::autonomy_scheduler_health,
            autonomy::autonomy_run_now,
            autonomy::autonomy_task_history,
            config::malgn_agent_config_get,
            config::malgn_agent_config_save,
            mcp_manager::mcp_list,
            mcp_manager::mcp_get,
            mcp_manager::mcp_add,
            mcp_manager::mcp_remove,
            mcp_manager::mcp_catalog_list,
            mcp_manager::mcp_install,
            mcp_manager::mcp_login,
            mcp_manager::mcp_logout,
            session_chat::read_session_transcript,
            session_chat::send_session_message,
            session_chat::start_new_session_message,
            session_chat::cancel_session_turn,
            session_chat::open_claude_login_terminal,
            claude_auth::check_claude_auth_status,
            claude_auth::start_claude_auth_login,
            claude_auth::cancel_claude_auth_login,
            claude_auth::submit_claude_auth_login_code,
            claude_auth::logout_claude_auth
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // 앱 종료 시 자식 프로세스 정리(설계 §1-4) — 진행 중인 자율업무
            // `claude -p`가 고아 프로세스로 남지 않도록, 종료 요청 시점에
            // 최대 2초만 대기하며 정리한다(그 이상 앱 종료를 붙잡지 않는다).
            if let tauri::RunEvent::ExitRequested { .. } = event {
                session_chat::request_shutdown();
                claude_auth::request_shutdown();
                autonomy::request_shutdown_and_wait(std::time::Duration::from_secs(2));
            }
        });
}
