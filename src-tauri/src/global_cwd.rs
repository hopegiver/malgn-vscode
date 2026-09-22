// ---------------- 전역(프로젝트 무관) claude CLI 호출의 작업 디렉터리 기본값 ----------------
// 마켓플레이스 갱신/설치, 인증 상태 확인/로그인처럼 특정 프로젝트에 묶이지 않는
// claude CLI 호출이 어느 디렉터리에서 실행돼야 하는지 정한다. 프로젝트 컨텍스트가
// 있는 호출(세션 채팅·자율업무)은 이 모듈을 쓰지 않고 project_path를 그대로
// current_dir로 고정한다(session_chat::turn::run_turn, autonomy::runner — 이미
// 그렇게 돼 있었다, 이 작업으로 바뀌지 않는다).
//
// 배경: Windows 실기에서 `plugins::run_claude_command`(마켓플레이스 갱신 등)가
// `.current_dir()`을 전혀 호출하지 않아 앱 프로세스의 CWD를 그대로 물려받았고,
// 그 값이 `C:\Windows\system32`로 관측됐다(hub decision 01m33w4pp3gyrjapczh1q7j4wm).
// `dev_tools::run_process_with_timeout`(도구 버전조회 등)은 이미 Windows에서
// `child_current_dir`로 CWD를 고정하는데 `plugins::run_claude_command`만 그
// 보호를 못 받는 구조적 비대칭이었다 — 이 모듈은 그 비대칭을 없앤다.
//
// ⚠ 인과 미확정: 이 CWD 값 하나만으로 실기에서 난 `claude` 실행 실패의 원인이
// 확정되는 것은 아니다. 진단 블록의 나머지 두 값(주입 PATH 전문, `ProgramFiles`)은
// 아직 회신받지 못했고, `claude`가 쓰는 `where.exe` 후보 스킵 로직은 CWD **하위**
// 경로만 건너뛰는데 git은 `C:\Program Files\Git\cmd`에 있어 system32 하위가
// 아니다. 그래서 이 모듈은 "요청된 개선"이지 "원인 확정 수정"이 아니다.
//
// 폴백 순서:
// 1) workspace 루트 중 실제로 존재하는 첫 번째 디렉터리 — "전역" 호출이라도
//    사용자가 실제로 설정해 둔 작업 맥락에 가장 가깝다.
// 2) 사용자 홈 디렉터리 — workspace가 하나도 설정/존재하지 않는 상태(신규 설치
//    직후 등)에서도 사실상 항상 존재한다.
// 3) 둘 다 존재하지 않으면 `None`을 반환한다 — 호출자는 이 경우 `.current_dir()`
//    자체를 호출하지 않고 기존 동작(앱 프로세스의 CWD 상속)으로 남겨둔다.
//    `Command::current_dir()`에 존재하지 않는 경로를 주면 spawn 자체가 즉시
//    실패한다(OS가 그 디렉터리로 chdir하지 못함) — "틀린 값이라도 일단 고정"보다
//    "확인된 값이 없으면 고정하지 않는다"가 안전하다. 이 폴백을 타면 최악의
//    경우도 지금까지의 동작(앱 CWD 상속)보다 나빠지지 않는다.

use std::path::PathBuf;

/// 순수함수 — 후보 목록에서 실제로 존재하는 **디렉터리**(파일이면 스킵)인 첫
/// 항목을 고른다. 후보 순서 자체가 폴백 우선순위다. `dev_tools::platform`의
/// 값 주입 패턴과 동일하게, 실제 파일시스템 상태를 매번 새로 만들지 않고도
/// 단위 테스트가 폴백 동작을 직접 검증할 수 있게 분리했다.
pub(crate) fn pick_existing_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.is_dir()).cloned()
}

/// 실제 환경(workspace 설정 + 홈 디렉터리)을 읽어 후보 목록을 조립한다 — 이
/// 함수만 실제 환경에 닿고, 선택 로직(`pick_existing_dir`)은 순수하게 남긴다.
fn global_cwd_candidates() -> Vec<PathBuf> {
    let mut candidates = crate::workspace::workspace_roots();
    if let Some(home) = dirs::home_dir() {
        candidates.push(home);
    }
    candidates
}

/// 마켓플레이스 갱신/설치, 인증 상태 확인/로그인 등 전역 claude CLI 호출이 쓸
/// CWD. `None`이면 호출자는 `.current_dir()`을 아예 호출하지 않는다(위 모듈
/// 주석 ③).
pub(crate) fn default_global_cwd() -> Option<PathBuf> {
    pick_existing_dir(&global_cwd_candidates())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_subdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-global-cwd-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
        dir
    }

    // ① CWD 기본값이 기대 경로로 조립된다 — 존재하는 첫 후보를 고른다(순서 보존).
    #[test]
    fn pick_existing_dir_returns_first_existing_candidate() {
        let missing = PathBuf::from("/definitely/does/not/exist/malgn-vscode-test-a");
        let existing = temp_subdir("first-existing");
        let candidates = vec![missing, existing.clone()];
        assert_eq!(pick_existing_dir(&candidates), Some(existing.clone()));
        let _ = std::fs::remove_dir_all(&existing);
    }

    // ② 지정 경로가 없을 때 폴백이 동작한다 — 모든 후보가 존재하지 않으면
    // None(호출자가 `.current_dir()`을 아예 호출하지 않도록 신호한다).
    #[test]
    fn pick_existing_dir_returns_none_when_all_candidates_missing() {
        let candidates = vec![
            PathBuf::from("/definitely/does/not/exist/malgn-vscode-test-b"),
            PathBuf::from("/definitely/does/not/exist/malgn-vscode-test-c"),
        ];
        assert_eq!(pick_existing_dir(&candidates), None);
    }

    // 폴백은 "존재하지 않음"뿐 아니라 "디렉터리가 아님"도 건너뛰어야 한다 —
    // `Command::current_dir()`은 파일 경로를 받아도 spawn이 실패한다.
    #[test]
    fn pick_existing_dir_skips_a_file_and_falls_back_to_next_dir() {
        let parent = temp_subdir("file-candidate-parent");
        let file_candidate = parent.join("not-a-dir.txt");
        std::fs::write(&file_candidate, "hi").unwrap();
        let dir_candidate = temp_subdir("file-candidate-fallback");
        let candidates = vec![file_candidate, dir_candidate.clone()];
        assert_eq!(pick_existing_dir(&candidates), Some(dir_candidate.clone()));
        let _ = std::fs::remove_dir_all(&parent);
        let _ = std::fs::remove_dir_all(&dir_candidate);
    }

    // 빈 후보 목록(둘 다 조회 불가한 극단적 케이스)도 패닉 없이 None.
    #[test]
    fn pick_existing_dir_returns_none_for_empty_candidates() {
        assert_eq!(pick_existing_dir(&[]), None);
    }

    // 이 머신 실측 — 홈 디렉터리는 항상 존재하므로 `default_global_cwd()`가
    // 최소한 하나는 반환해야 한다(workspace 설정 상태와 무관하게, 홈이
    // 후보 마지막 항목으로 항상 들어간다).
    #[test]
    fn default_global_cwd_returns_some_on_a_normal_machine_with_a_home_dir() {
        assert!(
            default_global_cwd().is_some(),
            "홈 디렉터리가 있는 정상 머신에서도 None이 나오면 후보 조립 로직을 의심해야 합니다"
        );
    }
}
