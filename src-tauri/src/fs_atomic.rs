// 원자적 파일 쓰기 + 롤링 1세대 백업 유틸. `config/user_config.rs`와
// `otel_settings.rs`에 동일 코드로 2벌 존재하던 것을, 앱링크(`app_links`)가
// 3번째 소비자가 되면서 이 공용 모듈로 추출했다(설계 `docs/design-app-links.md`
// §2-4 — 소비자가 2곳뿐일 때는 공용 모듈로 뽑지 않는다는 기존 원칙이 3번째
// 소비자 등장으로 해제됨).
//
// 동작은 추출 전과 완전히 동일하다. 유일한 차이는 `path.file_name()`이 `None`일
// 때만 쓰이는 하드코딩 폴백 파일명을 각 모듈 전용 값("malgn-agent.json" /
// "settings.json")에서 범용 값("config.json")으로 통일한 것 뿐이다 — 이 폴백은
// 두 모듈이 실제로 다루는 경로(항상 파일명이 있는 절대경로)에서는 결코
// 발생하지 않으므로 동작 변화가 없다.

use std::path::{Path, PathBuf};

/// 백업 파일 경로 — `<path>.malgn-bak`.
pub(crate) fn backup_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("config.json");
    path.with_file_name(format!("{file_name}.malgn-bak"))
}

/// 원자적 쓰기 — 같은 디렉터리에 임시 파일을 쓰고 `fs::rename`한다.
pub(crate) fn write_atomically(path: &Path, content: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| "설정 파일의 상위 디렉터리를 확인할 수 없습니다.".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| format!("설정 디렉터리를 만들지 못했습니다: {e}"))?;
    let tmp_name = format!(
        ".{}.malgn-tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.json"),
        std::process::id()
    );
    let tmp_path = dir.join(tmp_name);
    std::fs::write(&tmp_path, content).map_err(|e| format!("임시 파일을 쓰지 못했습니다: {e}"))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("설정 파일 교체에 실패했습니다: {e}"))?;
    Ok(())
}
