// ---------------- 사용량 "빈 상태" 원인 판정용 신호 (R14 P2-6) ----------------
// 사용량 집계(`get_daily_usage` 등)는 데이터가 없으면 그냥 빈 배열을 돌려줘서,
// "Claude Code가 없어서 비었다"와 "있는데 아직 안 썼다"를 화면이 구분할 수 없었다.
// 이 모듈은 그 구분에 쓸 **사실 신호 3개만** 모아 돌려준다 — 원인을 판정하지는
// 않는다(판정과 문구는 프론트 `usageSource.ts`의 몫이고, 신호가 서로 모순되면
// 단정하지 않고 "알 수 없음"으로 둔다).
//
// 신호와 한계:
//   - claude_binary_found: `dev_tools`의 Claude 탐지(후보 경로 + 플랫폼 PATH 폴백)와
//     같은 함수를 쓴다. 버전은 실행하지 않는다(macOS PATH 폴백의 `claude --version`
//     존재 확인 1회만 — 기존 개발 도구 화면 탐지와 동일). 후보 밖(nvm 등 버전
//     매니저·사용자 정의 prefix)에 설치돼 있고 GUI 앱 PATH에 없으면 "없음"으로 오탐할 수
//     있다 — 그래서 이 신호 하나로 "미설치"를 단정하지 않는다.
//   - claude_dir_exists: `~/.claude` 존재. 설치 자체가 만드는 폴더는 아니고(첫 실행·
//     로그인 때 생김) 삭제 후에도 남을 수 있다 — "한 번이라도 쓴 흔적"의 약한 신호다.
//   - projects_dir_has_entries: `~/.claude/projects` 아래에 프로젝트 폴더가 하나라도
//     있는가 — 사용량 집계가 읽는 바로 그 디렉터리다. 기간(30일)과 무관하게 "과거
//     기록이 있었는가"를 본다. `CLAUDE_CONFIG_DIR`로 위치를 옮긴 경우는 기존 집계
//     코드와 마찬가지로 보지 않는다.

use serde::Serialize;
use std::path::Path;

/// IPC 계약(camelCase): `claudeBinaryFound`, `claudeDirExists`, `projectsDirHasEntries`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSourceStatus {
    pub(crate) claude_binary_found: bool,
    pub(crate) claude_dir_exists: bool,
    pub(crate) projects_dir_has_entries: bool,
}

/// `~/.claude`와 `~/.claude/projects`의 상태만 본다(파일 내용은 읽지 않는다).
/// `home`을 인자로 받아 임시 디렉터리로 테스트할 수 있다.
pub(crate) fn probe_claude_dirs(home: &Path) -> (bool, bool) {
    let claude_dir = home.join(".claude");
    let claude_dir_exists = claude_dir.is_dir();
    let projects_dir_has_entries = std::fs::read_dir(claude_dir.join("projects"))
        .map(|entries| {
            entries
                .flatten()
                .any(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        })
        .unwrap_or(false);
    (claude_dir_exists, projects_dir_has_entries)
}

/// 홈 디렉터리를 못 찾으면 `None` — 프론트가 "판정 불가"로 다룬다(없다고 단정하지 않는다).
pub(crate) fn collect_usage_source_status() -> Option<UsageSourceStatus> {
    let home = dirs::home_dir()?;
    let (claude_dir_exists, projects_dir_has_entries) = probe_claude_dirs(&home);
    let claude = crate::dev_tools::tool_definition(crate::dev_tools::ToolId::Claude);
    Some(UsageSourceStatus {
        claude_binary_found: crate::dev_tools::resolve_tool_path(claude).is_some(),
        claude_dir_exists,
        projects_dir_has_entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-usage-source-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_claude_dir_reports_nothing() {
        let home = temp_home("none");
        assert_eq!(probe_claude_dirs(&home), (false, false));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn claude_dir_without_projects_is_not_history() {
        let home = temp_home("cfg-only");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        assert_eq!(probe_claude_dirs(&home), (true, false));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn empty_projects_dir_or_stray_file_is_not_history() {
        let home = temp_home("empty-projects");
        let projects = home.join(".claude").join("projects");
        std::fs::create_dir_all(&projects).unwrap();
        assert_eq!(probe_claude_dirs(&home), (true, false));
        std::fs::write(projects.join(".DS_Store"), b"x").unwrap();
        assert_eq!(probe_claude_dirs(&home), (true, false));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn project_subdir_counts_as_history() {
        let home = temp_home("history");
        std::fs::create_dir_all(home.join(".claude").join("projects").join("-Users-x-proj")).unwrap();
        assert_eq!(probe_claude_dirs(&home), (true, true));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn serializes_camel_case_contract() {
        let v = serde_json::to_value(UsageSourceStatus {
            claude_binary_found: true,
            claude_dir_exists: false,
            projects_dir_has_entries: true,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({"claudeBinaryFound": true, "claudeDirExists": false, "projectsDirHasEntries": true})
        );
    }

    #[test]
    fn collect_never_panics_on_this_machine() {
        // 실제 머신 상태는 가정하지 않는다 — 패닉 없이 값이 나오는지만 본다.
        let _ = collect_usage_source_status();
    }
}
