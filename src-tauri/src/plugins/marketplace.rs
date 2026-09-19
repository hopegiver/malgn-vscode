// ---------------- 마켓플레이스 (실제 로컬 데이터 + 추가/제거) ----------------
// 조회(read_known_marketplaces)는 `lib.rs`에 있던 코드를 그대로 옮긴 것이다 —
// 로직은 한 글자도 바꾸지 않았다. add_marketplace/remove_marketplace는 이번에
// 추가한 실행 커맨드로, `super`(plugins::mod)의 `run_claude_command`를 그대로
// 재사용한다 — `install_plugin`/`update_plugin`/`refresh_marketplaces`와 완전히
// 동일한 계열: argv 배열로만 `claude` CLI를 실행하고(셸 경유 없음), `claude`
// 실행 파일은 `cli_launcher::resolve_binary_expand_home`로 절대경로를 해석하며
// (bare-name spawn 금지, PATH 하이재킹 방어), PATH는 `build_child_path_env`로
// 주입되고(`run_claude_command` 내부), `.silent()`로 자식 프로세스 창이 뜨지
// 않는다 — 새 패턴을 발명하지 않았다.
//
// 사용자가 직접 입력하는 자유 텍스트(source/name)가 처음 생긴 지점이라 argv에
// 들어가기 전 빈 값·공백만·개행 포함을 거부한다(validate_marketplace_field) —
// 프런트(views/settings.ts)도 동일 검증을 하지만, 프런트만 막으면 devtools
// 콘솔에서 invoke()를 직접 호출해 우회할 수 있으므로 백엔드에서도 반드시
// 재검증한다.

use serde::Serialize;
use serde_json::Value;

use super::{run_claude_command, CommandResult};

#[derive(Serialize, Debug)]
pub(crate) struct MarketplaceInfo {
    id: String,
    repo: Option<String>,
    #[serde(rename = "lastUpdated")]
    last_updated: Option<String>,
}

pub(crate) fn read_known_marketplaces() -> Vec<MarketplaceInfo> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let path = home
        .join(".claude")
        .join("plugins")
        .join("known_marketplaces.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<Value>(&content) else {
        return Vec::new();
    };
    let Some(obj) = root.as_object() else {
        return Vec::new();
    };

    let mut results = Vec::new();
    for (id, value) in obj {
        let repo = value
            .get("source")
            .and_then(|s| s.get("repo"))
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());
        let last_updated = value
            .get("lastUpdated")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        results.push(MarketplaceInfo {
            id: id.clone(),
            repo,
            last_updated,
        });
    }
    results.sort_by(|a, b| a.id.cmp(&b.id));
    results
}

// malgn-agent 플러그인이 배포되는 마켓플레이스 — 프런트 `src/views/catalog.ts`의
// `DEFAULT_PLUGIN_ID`("malgn-agent@malgnsoft-plugins")와 동일한 값이다.
// 예전에는 이 이름의 마켓플레이스 제거를 여기서 거부했으나, 45명 사내
// 사용자가 잘못 추가한 마켓플레이스를 되돌릴 수 있어야 한다는 결정에 따라
// 더는 특별 취급하지 않는다(build_marketplace_remove_args 참고) — 다른
// 마켓플레이스와 동일하게 제거할 수 있다. 테스트에서 리터럴 대신 이 상수를
// 참조하기 위해 남겨둔다.
#[cfg(test)]
const PINNED_MARKETPLACE_ID: &str = "malgnsoft-plugins";

/// 사용자 자유 입력(source/name)에 대한 공용 검증 — 빈 값·공백만·개행 포함을
/// 거부하고, 통과하면 앞뒤 공백만 제거한 값을 돌려준다. argv에 그대로 들어갈
/// 값이라 셸 인젝션 경로는 없지만(Command::args), 개행이 섞이면 `claude` CLI가
/// 인자를 오해석하거나 로그가 깨질 수 있어 애초에 거부한다.
fn validate_marketplace_field(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label}을(를) 입력하세요."));
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(format!("{label}에 개행 문자를 포함할 수 없습니다."));
    }
    Ok(trimmed.to_string())
}

/// `claude plugin marketplace add <source>` argv를 조립한다. 검증 실패 시
/// CLI를 전혀 실행하지 않고 그대로 에러를 반환한다(add_marketplace가 이어받음).
fn build_marketplace_add_args(source: &str) -> Result<Vec<String>, String> {
    let trimmed = validate_marketplace_field(source, "마켓플레이스 소스")?;
    Ok(vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "add".to_string(),
        trimmed,
    ])
}

/// `claude plugin marketplace remove <name>` argv를 조립한다. 검증 실패 시에만
/// CLI를 실행하지 않는다 — malgn-agent의 출처 마켓플레이스(과거
/// PINNED_MARKETPLACE_ID)도 더는 특별 취급하지 않고 다른 이름과 동일하게
/// 제거를 허용한다(사용자 결정: 잘못 추가한 마켓플레이스를 되돌릴 수 있어야
/// 한다). 프런트(views/settings.ts)도 이 이름에 대한 "필수" 배지·제거 버튼
/// 숨김을 두지 않는다.
fn build_marketplace_remove_args(name: &str) -> Result<Vec<String>, String> {
    let trimmed = validate_marketplace_field(name, "마켓플레이스 이름")?;
    Ok(vec![
        "plugin".to_string(),
        "marketplace".to_string(),
        "remove".to_string(),
        trimmed,
    ])
}

// `#[tauri::command]`는 붙이지 않는다 — `tauri::generate_handler!`는 매크로가
// 함수 옆에 만든 숨은 헬퍼 항목을 "그 커맨드를 호출한 것과 동일한 경로"로
// 찾는데, `lib.rs`는 이 저장소의 기존 패턴대로 `plugins::add_marketplace`
// 경로로 등록한다(마켓플레이스/설치 계열 커맨드는 전부 `plugins/mod.rs`가
// 진입점 — `list_known_marketplaces`/`install_plugin`과 동일). 그래서 여기서는
// 순수 함수로 실행 로직만 두고, `#[tauri::command]` 진입점은 mod.rs의
// 얇은 래퍼(add_marketplace/remove_marketplace)가 맡는다.

/// 사용자가 마켓플레이스 설정 탭에서 직접 입력한 소스(URL/경로/GitHub repo)를
/// `claude plugin marketplace add`로 등록한다. 성공하면 프런트가 목록을
/// 다시 읽어(list_known_marketplaces) 반영한다.
pub(crate) fn run_add_marketplace(source: String) -> CommandResult {
    match build_marketplace_add_args(&source) {
        Ok(args) => {
            let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_claude_command(&arg_refs)
        }
        Err(message) => CommandResult {
            success: false,
            message,
        },
    }
}

/// 사용자가 마켓플레이스 설정 탭에서 등록된 항목을 제거한다. 파괴적 동작이라
/// 프런트가 삭제 전 confirmDialog로 확인을 받는다 — malgn-agent의 출처
/// 마켓플레이스도 다른 이름과 동일하게 제거 대상이다(더 이상 고정 표시하지
/// 않는다).
pub(crate) fn run_remove_marketplace(name: String) -> CommandResult {
    match build_marketplace_remove_args(&name) {
        Ok(args) => {
            let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_claude_command(&arg_refs)
        }
        Err(message) => CommandResult {
            success: false,
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 머신 의존(이 머신에 malgnsoft-plugins 마켓플레이스가 등록돼 있어야 함) — CI
    // 러너에는 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored plugins::marketplace::tests::finds_known_marketplaces
    #[test]
    #[ignore]
    fn finds_known_marketplaces() {
        let marketplaces = read_known_marketplaces();
        assert!(
            !marketplaces.is_empty(),
            "마켓플레이스를 하나도 찾지 못했습니다"
        );
        assert!(marketplaces.iter().any(|m| m.id == "malgnsoft-plugins"));
    }

    // ⚠️ add_marketplace()/remove_marketplace() 자체(claude CLI를 실제로 실행하는
    // 경로)를 호출하는 테스트는 두지 않는다 — update_plugin/refresh_marketplaces/
    // install_plugin과 동일한 이유(plugins/mod.rs 하단 주석 참고): 이 머신에 실제
    // 등록된 마켓플레이스를 `cargo test`를 돌릴 때마다 건드리는 부작용은 누구도
    // 원하지 않는다. 아래는 전부 CLI를 전혀 실행하지 않는(검증만 하는) 순수 단위다.

    #[test]
    fn validate_marketplace_field_rejects_empty() {
        let err = validate_marketplace_field("", "이름").unwrap_err();
        assert!(err.contains("입력하세요"));
    }

    #[test]
    fn validate_marketplace_field_rejects_whitespace_only() {
        let err = validate_marketplace_field("   \t  ", "이름").unwrap_err();
        assert!(err.contains("입력하세요"));
    }

    #[test]
    fn validate_marketplace_field_rejects_embedded_newline() {
        let err = validate_marketplace_field("foo\nbar", "이름").unwrap_err();
        assert!(err.contains("개행"));
    }

    #[test]
    fn validate_marketplace_field_rejects_embedded_carriage_return() {
        let err = validate_marketplace_field("foo\r\nbar", "이름").unwrap_err();
        assert!(err.contains("개행"));
    }

    #[test]
    fn validate_marketplace_field_trims_outer_whitespace() {
        let ok = validate_marketplace_field("  https://github.com/acme/plugins  ", "소스").unwrap();
        assert_eq!(ok, "https://github.com/acme/plugins");
    }

    #[test]
    fn build_marketplace_add_args_builds_expected_argv_for_valid_source() {
        let args = build_marketplace_add_args("https://github.com/acme/plugins").unwrap();
        assert_eq!(
            args,
            vec!["plugin", "marketplace", "add", "https://github.com/acme/plugins"]
        );
    }

    #[test]
    fn build_marketplace_add_args_rejects_blank_source_without_building_argv() {
        assert!(build_marketplace_add_args("   ").is_err());
    }

    #[test]
    fn build_marketplace_remove_args_builds_expected_argv_for_valid_name() {
        let args = build_marketplace_remove_args("acme-plugins").unwrap();
        assert_eq!(args, vec!["plugin", "marketplace", "remove", "acme-plugins"]);
    }

    #[test]
    fn build_marketplace_remove_args_rejects_blank_name_without_building_argv() {
        assert!(build_marketplace_remove_args("").is_err());
    }

    // 새 계약: malgn-agent의 출처 마켓플레이스(예전 PINNED_MARKETPLACE_ID)도
    // 더 이상 특별 취급하지 않고 다른 이름과 동일하게 제거 argv를 만든다
    // (사용자 결정: 잘못 추가한 마켓플레이스를 되돌릴 수 있어야 한다).
    #[test]
    fn build_marketplace_remove_args_allows_formerly_pinned_marketplace() {
        let args = build_marketplace_remove_args(PINNED_MARKETPLACE_ID).unwrap();
        assert_eq!(args, vec!["plugin", "marketplace", "remove", PINNED_MARKETPLACE_ID]);
    }

    #[test]
    fn build_marketplace_remove_args_allows_formerly_pinned_marketplace_with_surrounding_whitespace() {
        let args = build_marketplace_remove_args("  malgnsoft-plugins  ").unwrap();
        assert_eq!(args, vec!["plugin", "marketplace", "remove", PINNED_MARKETPLACE_ID]);
    }
}
