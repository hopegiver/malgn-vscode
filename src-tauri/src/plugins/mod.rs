// ---------------- 플러그인/마켓플레이스 ----------------
// `lib.rs`에 있던 "설치된 플러그인 카탈로그" + "마켓플레이스" + "플러그인 실제
// 업데이트(사용자 명시 승인)" 세 절을 그대로 옮긴 것이다 — 로직은 한 글자도
// 바꾸지 않았다. `autonomy`/`dev_tools`와 동일한 패턴: `#[tauri::command]`
// 진입점만 이 파일에 두고, 조회 로직은 서브모듈(`installed`/`marketplace`)로
// 옮겼다.
//
// `claude` CLI를 셸을 거치지 않고 프로그램명+인자 배열로 직접 실행한다(인젝션
// 경로 없음). 인자는 항상 이 파일이 이미 읽어둔 신뢰할 수 있는 값(설치된 플러그인
// id·마켓플레이스 id)만 들어온다 — 프론트엔드에 자유 텍스트 입력 필드가 없어
// 임의 문자열이 인자로 들어갈 경로 자체가 없다.

mod installed;
mod marketplace;

use serde::Serialize;

/// `check_dev_tools`(dev_tools.rs)와 동일한 이유·관용구 — sync 커맨드가
/// 메인 스레드를 막는 P0 버그 계열이라 async + `spawn_blocking`으로 옮긴다.
#[tauri::command]
pub async fn list_installed_plugins() -> Vec<installed::InstalledPlugin> {
    tauri::async_runtime::spawn_blocking(installed::read_installed_plugins)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_known_marketplaces() -> Vec<marketplace::MarketplaceInfo> {
    marketplace::read_known_marketplaces()
}

#[derive(Serialize, Debug)]
pub(crate) struct CommandResult {
    success: bool,
    message: String,
}

fn run_claude_command(args: &[&str]) -> CommandResult {
    match std::process::Command::new("claude").args(args).output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if output.status.success() {
                CommandResult {
                    success: true,
                    message: if stdout.is_empty() {
                        "완료되었습니다.".to_string()
                    } else {
                        stdout
                    },
                }
            } else {
                let msg = if !stderr.is_empty() {
                    stderr
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    "알 수 없는 오류로 실패했습니다.".to_string()
                };
                CommandResult {
                    success: false,
                    message: msg,
                }
            }
        }
        Err(e) => CommandResult {
            success: false,
            message: format!("claude 명령을 실행할 수 없습니다: {e}"),
        },
    }
}

/// `plugin_id`는 `installed_plugins.json`의 키(예: "malgn-agent@malgnsoft-plugins")
/// 형식 그대로 `claude plugin update`에 넘긴다. 성공해도 재시작해야 적용된다 —
/// 프론트엔드가 안내 문구를 붙인다.
#[tauri::command]
pub fn update_plugin(plugin_id: String) -> CommandResult {
    run_claude_command(&["plugin", "update", &plugin_id])
}

#[tauri::command]
pub fn refresh_marketplaces() -> CommandResult {
    run_claude_command(&["plugin", "marketplace", "update"])
}

// ⚠️ update_plugin()/refresh_marketplaces()를 실제로 호출하는 테스트는 의도적으로
// 두지 않는다 — 이 머신에 실제 설치된 malgn-agent 플러그인(이 에이전트 자신이
// 로드되어 있는 바로 그 플러그인)을 `cargo test`를 돌릴 때마다 매번 실제로
// 업데이트해버리는 부작용은 누구도 원하지 않는다. 컴파일 통과(`cargo check`)로만
// 검증했고, 실제 클릭 검증은 사용자가 직접 GUI에서 해야 한다.
