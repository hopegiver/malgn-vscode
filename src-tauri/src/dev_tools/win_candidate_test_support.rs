// R-8(review-devtools-windows-parity-2026-09-15-r3.md) 재발 방지: classify_tests.rs와
// plan_table_tests.rs가 "DEV_TOOLS × windows_path_candidates" 전수 순회와 합성
// Windows EnvRoots 리터럴을 각각 따로 복사해 갖고 있었다 — 한쪽만 고치면
// 조용히 갈린다(3차 T4가 두 파일 모두에서 동시에 발견된 이유가 이 중복이다).
// 순회 자체를 이 헬퍼 하나로 모은다. `#[cfg(test)]`로만 컴파일되므로(mod.rs
// 선언부) 릴리스 바이너리에는 포함되지 않는다.

use super::platform::{expand_path_tokens, EnvRoots, Platform};
use super::{DevTool, DEV_TOOLS};
use std::path::PathBuf;

/// 3차 리뷰 N1/N3 가드가 공유하는 합성 Windows 환경 — 실기 미검증(§9), 사용자명은
/// 임의 값이다(경로 조립 로직만 검증하면 되므로 실제 존재 여부는 무관하다).
pub(crate) fn synthetic_windows_env_roots() -> EnvRoots {
    EnvRoots {
        home: Some(PathBuf::from(r"C:\Users\hopegiver")),
        appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Roaming")),
        local_appdata: Some(PathBuf::from(r"C:\Users\hopegiver\AppData\Local")),
        program_files: Some(PathBuf::from(r"C:\Program Files")),
        program_files_x86: Some(PathBuf::from(r"C:\Program Files (x86)")),
        pnpm_home: None,
        system_root: Some(PathBuf::from(r"C:\Windows")),
    }
}

/// T4(3차) 재발 방지: 새 도구가 `windows_path_candidates: &[]`로 추가되면
/// `assert_eq!(checked, total)` 류 가드는 양변에 0을 더해 조용히 통과한다 —
/// 여기서 도구마다 후보가 비어 있지 않음을 먼저 단언하므로, 이 헬퍼를 쓰는
/// 가드는 전부 그 면제를 자동으로 잡는다(mod.rs의
/// `every_tool_has_at_least_one_windows_path_candidate`와 같은 사실을 이
/// 순회 자체 안에서도 한 번 더 지킨다 — 이 헬퍼만 봐도 안전해야 하므로
/// 이중이라도 유지한다). `f`는 (도구, 원본 템플릿, 확장된 절대경로 문자열)를
/// 받고, 반환값은 실제로 순회한 후보 개수다.
pub(crate) fn for_every_windows_candidate(
    mut f: impl FnMut(&'static DevTool, &'static str, String),
) -> usize {
    let roots = synthetic_windows_env_roots();
    let mut checked = 0usize;
    for def in DEV_TOOLS.iter() {
        assert!(
            !def.windows_path_candidates.is_empty(),
            "{}에 windows_path_candidates가 없습니다 — 이 도구는 Windows 가드를 \
             한 번도 거치지 않고 조용히 면제됩니다",
            def.key
        );
        for template in def.windows_path_candidates.iter().copied() {
            let expanded = expand_path_tokens(Platform::Win, template, &roots)
                .unwrap_or_else(|| panic!("{}의 후보 {template:?} 토큰 확장 실패", def.key));
            f(def, template, expanded);
            checked += 1;
        }
    }
    checked
}
