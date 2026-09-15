// ==================== jsonl 기반 목록 행 (architect C안) ====================
//
// 목록 행의 소스를 registry(`~/.claude/sessions/`) 단독에서 jsonl
// (`~/.claude/projects/**/<sessionId>.jsonl`)로 옮긴다. registry는 "지금 실행
// 중"이라는 사실 하나만 얹는 live 오버레이로 강등한다(`session_list::registry::overlay_running_state`).
//
// 안전장치 ①: `/private/tmp` 유래 프로젝트 디렉터리(`-private-tmp-...`,
// `-tmp-...`, 대소문자 무관) 제외.
// 안전장치 ②: `crate::scan_workspace_projects()` 결과에 있는 프로젝트로
// 한정. 2단계로 나뉜다 — (a) `dir_name` 문자열 접두사로 거르는 싼 프리필터
// (`is_allowed_project_dir`, 아래), (b) 프리필터를 통과한 파일에 한해 실제
// `cwd`를 경로 컴포넌트 단위로 대조하는 권위 검사(`is_cwd_within_allowed_workspace`).
// (a)만으로는 `sanitize_cwd_for_project_dir`가 `/`를 `-`로 뭉개 정보를 버리는
// 탓에 `foo`가 허용이면 `foo-bar`(별개 디렉터리)까지 문자열 접두사로 통과시키는
// 결함이 있다 — 그래서 (b)가 최종 권위를 가진다.
// registry 오버레이 경로(`read_claude_sessions`의 live_registry)도 이제 (b)를
// 통과해야만 목록에 노출된다(m3: 이전에는 registry 유래 폴백 행이 ①②를 전혀
// 거치지 않아 워크스페이스 밖 cwd가 새어나갈 수 있었다 — 게이트를 추가해
// 닫았다).
// 셋 다 사람 승인 조건이라 이 함수들에서 빠지면 안 된다.

use super::sanitize_cwd_for_project_dir;
use std::path::{Path, PathBuf};

/// 안전장치 ①의 판정 근거: 이 프로젝트의 디렉터리명 규칙(`/`→`-`)에서
/// `/private/tmp/...` 경로는 반드시 이 접두사로 시작한다(실측 확인 — S6 근처
/// 실측과 동일한 방식). 대소문자 구분 없이(`eq_ignore_ascii_case`) 비교한다
/// (m4: macOS 파일시스템은 기본적으로 대소문자를 구분하지 않으므로 `/Private/Tmp`
/// 등도 걸러야 한다).
const PRIVATE_TMP_PROJECT_DIR_PREFIX: &str = "-private-tmp-";

/// m4: `/tmp/...`(중간에 `private`을 거치지 않는 경로)도 `-tmp-...`로
/// sanitize되므로 별도 접두사로 제외한다.
const BARE_TMP_PROJECT_DIR_PREFIX: &str = "-tmp-";

/// `dir_name`이 `prefix`로 시작하는지 대소문자 구분 없이 판정한다. 문자열
/// 슬라이싱이 UTF-8 문자 경계를 벗어나 패닉하지 않도록 `get(..)`으로 안전하게
/// 접근한다.
fn starts_with_ignore_case(dir_name: &str, prefix: &str) -> bool {
    dir_name
        .get(..prefix.len())
        .map(|head| head.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

/// 안전장치 ②의 (a)단계(프리필터): `scan_workspace_projects()`가 찾은 각
/// 프로젝트 경로를 이 파일이 이미 쓰는 `sanitize_cwd_for_project_dir` 규칙으로
/// 치환해 접두사 목록을 만든다. `crate::scan_workspace_projects()`를 그대로
/// 재사용한다(복붙하지 않는다) — `WorkspaceProject::path`는 이미 `pub(crate)`로
/// 열려 있다. 최종 권위는 `is_cwd_within_allowed_workspace`(실제 `cwd` 컴포넌트
/// 대조)에 있다.
pub(super) fn allowed_project_dir_prefixes() -> Vec<String> {
    crate::scan_workspace_projects()
        .iter()
        .map(|p| sanitize_cwd_for_project_dir(&p.path))
        .collect()
}

/// `dir_name`(예: `-Users-hopegiver-workspace-malgn-vscode-src-tauri-src`)이
/// 안전장치 ①과 안전장치 ②의 (a)단계(싼 프리필터)를 통과하는지 판정하는 순수
/// 함수(fs 접근 없음 — 유닛 테스트 대상). `cwd`가 프로젝트 루트 자신이면
/// 접두사와 완전히 같고, 프로젝트 내부 하위 디렉터리면 접두사 뒤에 `-`가
/// 이어진다.
///
/// **이 함수만으로는 최종 판정이 아니다.** 문자열 접두사 검사라 `foo`가
/// 허용이면 `foo-bar`(별개 디렉터리)까지 통과시킨다 — 상한을 통과한 파일에
/// 한해서만 `is_cwd_within_allowed_workspace`(실제 `cwd`를 경로 컴포넌트
/// 단위로 대조하는 권위 검사)가 최종 판정을 내린다.
pub(super) fn is_allowed_project_dir(dir_name: &str, allowed_prefixes: &[String]) -> bool {
    if starts_with_ignore_case(dir_name, PRIVATE_TMP_PROJECT_DIR_PREFIX)
        || starts_with_ignore_case(dir_name, BARE_TMP_PROJECT_DIR_PREFIX)
    {
        return false;
    }
    allowed_prefixes
        .iter()
        .any(|prefix| dir_name == prefix || dir_name.starts_with(&format!("{prefix}-")))
}

/// 안전장치 ②의 권위 검사(M1): `cwd`가 실제로 `scan_workspace_projects()`가
/// 찾은 프로젝트 경로 중 하나의 자기 자신이거나 그 하위인지 **경로 컴포넌트
/// 단위**로 대조하는 순수 함수(fs 접근 없음 — 유닛 테스트 대상).
/// `is_allowed_project_dir`의 문자열 접두사 프리필터는 `foo`가 허용일 때
/// `foo-bar`(별개 디렉터리, 허용 안 됨)까지 통과시키는 결함이 있다 —
/// `sanitize_cwd_for_project_dir`가 `/`를 `-`로 뭉개 경로 구분자 정보를 버려서
/// 문자열만으로는 `foo-bar`와 `foo/bar`를 구분할 수 없기 때문이다.
/// `Path::starts_with`는 컴포넌트 경계를 지키므로(`foo-bar`는 `foo`의
/// 컴포넌트 하위가 아니다) 이 역전을 막는다.
pub(super) fn is_cwd_within_allowed_workspace(cwd: &str, allowed_roots: &[PathBuf]) -> bool {
    let cwd_path = Path::new(cwd);
    allowed_roots.iter().any(|root| cwd_path.starts_with(root))
}

/// `is_cwd_within_allowed_workspace`가 대조할 실제 워크스페이스 프로젝트 경로
/// 목록. `crate::scan_workspace_projects()`를 그대로 재사용한다(복붙하지
/// 않는다).
pub(super) fn allowed_workspace_roots() -> Vec<PathBuf> {
    crate::scan_workspace_projects()
        .iter()
        .map(|p| PathBuf::from(&p.path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 안전장치 ①: `/private/tmp` 유래 프로젝트 디렉터리명은 허용 접두사와 무관하게
    // 항상 제외된다.
    #[test]
    fn private_tmp_derived_project_dir_is_always_excluded() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(!is_allowed_project_dir(
            "-private-tmp-claude-501--Users-hopegiver-workspace-malgn-vscode-scratchpad",
            &allowed
        ));
    }

    // m4: 대소문자가 달라도(`-Private-Tmp-...`) 제외되어야 한다.
    #[test]
    fn private_tmp_derived_project_dir_is_excluded_case_insensitively() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(!is_allowed_project_dir(
            "-Private-Tmp-claude-501--Users-hopegiver-workspace-malgn-vscode-scratchpad",
            &allowed
        ));
        assert!(!is_allowed_project_dir(
            "-PRIVATE-TMP-claude-501--Users-hopegiver-workspace-malgn-vscode-scratchpad",
            &allowed
        ));
    }

    // m4: `/private`를 거치지 않는 순수 `/tmp/...` 경로(`-tmp-...`로 sanitize됨)도
    // 대소문자 무관하게 제외되어야 한다.
    #[test]
    fn bare_tmp_derived_project_dir_is_excluded_case_insensitively() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(!is_allowed_project_dir("-tmp-some-scratch-dir", &allowed));
        assert!(!is_allowed_project_dir("-Tmp-some-scratch-dir", &allowed));
    }

    // 안전장치 ②의 (a)단계(프리필터): 스캔된 워크스페이스 프로젝트 목록에 없는
    // 디렉터리명은 제외된다. 프로젝트 루트 자신과 그 하위 디렉터리(cwd가
    // 서브폴더인 세션)는 모두 허용되어야 한다. **이 단계만으로는 `foo-bar`
    // (별개 디렉터리)가 `foo` 허용에 의해 통과된다** — 그 결함은 아래
    // `is_cwd_within_allowed_workspace`(권위 검사) 테스트가 고정한다.
    #[test]
    fn only_dirs_matching_scanned_workspace_projects_are_allowed() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(is_allowed_project_dir(
            "-Users-hopegiver-workspace-malgn-vscode",
            &allowed
        ));
        assert!(is_allowed_project_dir(
            "-Users-hopegiver-workspace-malgn-vscode-src-tauri-src",
            &allowed
        ));
        assert!(!is_allowed_project_dir(
            "-Users-hopegiver-workspace-some-other-project",
            &allowed
        ));
    }

    // M1 완료 판정: `foo`가 허용일 때 `foo-bar`(별개 디렉터리) 유래 세션은
    // 권위 검사에서 거부되어야 한다 — 프리필터(`is_allowed_project_dir`)는
    // 문자열 접두사라 이 역전을 막지 못하지만, 실제 `cwd`를 경로 컴포넌트
    // 단위로 대조하는 `is_cwd_within_allowed_workspace`는 막는다.
    #[test]
    fn cwd_authority_check_rejects_sibling_dir_with_shared_prefix() {
        let allowed_roots = vec![PathBuf::from("/Users/hopegiver/workspace/foo")];

        assert!(
            is_cwd_within_allowed_workspace("/Users/hopegiver/workspace/foo", &allowed_roots),
            "허용 루트 자신은 통과해야 합니다"
        );
        assert!(
            is_cwd_within_allowed_workspace(
                "/Users/hopegiver/workspace/foo/sub",
                &allowed_roots
            ),
            "허용 루트의 하위 디렉터리는 통과해야 합니다"
        );
        assert!(
            !is_cwd_within_allowed_workspace(
                "/Users/hopegiver/workspace/foo-bar",
                &allowed_roots
            ),
            "foo-bar는 foo의 컴포넌트 하위가 아니므로 거부되어야 하는데 통과했습니다 — \
             문자열 접두사와 경로 컴포넌트를 혼동했을 가능성이 있습니다"
        );
    }
}
