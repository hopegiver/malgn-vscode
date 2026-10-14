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
    write_atomically_inner(path, content, false)
}

/// 원자적 쓰기 + 소유자 전용 권한 — `write_atomically`와 같되, unix에서는 임시
/// 파일을 처음부터 `0600`으로 만들어 그룹/기타 읽기를 막는다(자격증명이 든
/// 파일용). 임시 파일이 umask 기본 권한(보통 0644)으로 잠깐이라도 존재하는
/// 구간이 없도록 *생성 시점*에 모드를 지정하고, 같은 이름의 잔여 파일이 이미
/// 있었던 경우를 대비해 내용을 쓰기 전에 `set_permissions`로 한 번 더 조인다.
/// Windows 등 비-unix에서는 `write_atomically`와 동작이 같다(ACL 변경 없음).
pub(crate) fn write_atomically_private(path: &Path, content: &str) -> Result<(), String> {
    write_atomically_inner(path, content, true)
}

fn write_atomically_inner(path: &Path, content: &str, private: bool) -> Result<(), String> {
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
    if private {
        write_private_file(&tmp_path, content)
            .map_err(|e| format!("임시 파일을 쓰지 못했습니다: {e}"))?;
    } else {
        std::fs::write(&tmp_path, content)
            .map_err(|e| format!("임시 파일을 쓰지 못했습니다: {e}"))?;
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        if private {
            // 자격증명이 든 임시 파일이 남지 않게 치운다(실패해도 원 오류가 우선).
            let _ = std::fs::remove_file(&tmp_path);
        }
        return Err(format!("설정 파일 교체에 실패했습니다: {e}"));
    }
    Ok(())
}

#[cfg(unix)]
fn write_private_file(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // 같은 이름의 잔여 파일이 더 넓은 권한으로 이미 있었다면 `mode()`는 무시된다.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(content.as_bytes())
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, content: &str) -> std::io::Result<()> {
    std::fs::write(path, content)
}
