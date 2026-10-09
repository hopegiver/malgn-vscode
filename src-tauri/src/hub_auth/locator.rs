// CLI 실행파일 locator (설계 §4 D7).
//
// 플러그인의 bin/hub-auth-header.mjs 래퍼는 PATH에 의존하지 않고 이 파일에서 앱 실행파일
// 경로를 찾는다. 앱(GUI)이 시작할 때마다 `current_exe()`를 원자적으로 기록한다.
//   macOS:   ~/Library/Application Support/com.malgnsoft.agent/cli-path.json
//   Windows: %APPDATA%\com.malgnsoft.agent\cli-path.json
// (Tauri `app_config_dir()` 규칙과 같다.) 내용: {"exe":"…","version":"0.x.y"}
//
// 일시 경로는 기록하지 않는다: 디스크 이미지에서 바로 실행한 경우(`/Volumes/`), macOS
// Gatekeeper 임시 격리 실행(`/AppTranslocation/`), OS 임시 디렉터리 아래 — 영구 locator가
// 사라질 경로를 가리키면 안 된다(m4).
//
// debug 빌드는 기록하지 않는다. `pnpm tauri dev`가 설치된 앱의 locator를
// `target/debug/…`로 덮어쓰면 개발 중 모든 Claude 세션의 헬퍼가 개발 빌드를 가리키게
// 된다. e2e는 `MALGN_AGENT_EXE` 오버라이드로 개발 빌드를 지정한다(설계 §4 D7 1번).

use std::path::{Path, PathBuf};

pub(crate) fn locator_path(config_dir: &Path) -> PathBuf {
    config_dir.join("cli-path.json")
}

fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

/// 영구 설치 경로가 아닌 일시 경로인가.
pub(crate) fn is_transient_path(exe: &Path, temp_dir: &Path) -> bool {
    let e = norm(exe);
    if e.contains("/apptranslocation/") || e.starts_with("/volumes/") {
        return true;
    }
    let mut temps = vec![norm(temp_dir)];
    if let Ok(c) = std::fs::canonicalize(temp_dir) {
        temps.push(norm(&c));
    }
    temps.into_iter().any(|t| {
        !t.is_empty()
            && t != "/"
            && (e == t || e.starts_with(&format!("{}/", t.trim_end_matches('/'))))
    })
}

/// 기록했으면 `Ok(true)`, 일시 경로라 건너뛰었으면 `Ok(false)`.
pub(crate) fn write(
    config_dir: &Path,
    exe: &Path,
    version: &str,
    temp_dir: &Path,
) -> Result<bool, String> {
    if !exe.is_absolute() {
        return Err("실행파일 경로가 절대경로가 아닙니다.".to_string());
    }
    let exe_str = exe
        .to_str()
        .ok_or_else(|| "실행파일 경로가 UTF-8이 아닙니다.".to_string())?;
    if is_transient_path(exe, temp_dir) {
        return Ok(false);
    }
    let body = serde_json::json!({ "exe": exe_str, "version": version }).to_string();
    crate::fs_atomic::write_atomically(&locator_path(config_dir), &body)?;
    Ok(true)
}

/// GUI 시작 시 호출. 실패해도 앱 동작에는 영향이 없다(래퍼가 기본 설치 경로로 폴백한다).
pub(crate) fn write_current(config_dir: &Path) {
    if cfg!(debug_assertions) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = write(
            config_dir,
            &exe,
            env!("CARGO_PKG_VERSION"),
            &std::env::temp_dir(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub_auth::renew::testing::tmp_dir;

    #[test]
    fn transient_paths_are_detected() {
        let tmp = Path::new("/var/folders/ab/T");
        for p in [
            "/private/var/folders/xx/AppTranslocation/ABC/d/malgn-agent.app/Contents/MacOS/malgn-agent",
            "/Volumes/malgn-agent/malgn-agent.app/Contents/MacOS/malgn-agent",
            "/var/folders/ab/T/x/malgn-agent",
            "/var/folders/ab/T",
        ] {
            assert!(is_transient_path(Path::new(p), tmp), "{p}");
        }
        for p in [
            "/Applications/malgn-agent.app/Contents/MacOS/malgn-agent",
            "/Users/me/Applications/malgn-agent.app/Contents/MacOS/malgn-agent",
            "/var/folders/ab/Tx/malgn-agent", // 접두사만 같은 다른 디렉터리
            "C:\\Users\\me\\AppData\\Local\\malgn-agent\\malgn-agent.exe",
        ] {
            assert!(!is_transient_path(Path::new(p), tmp), "{p}");
        }
        // Windows 임시 경로(구분자·대소문자 무시)
        assert!(is_transient_path(
            Path::new("C:\\Users\\Me\\AppData\\Local\\Temp\\x\\malgn-agent.exe"),
            Path::new("C:\\Users\\me\\AppData\\Local\\Temp")
        ));
    }

    #[test]
    fn writes_json_atomically_and_skips_transient() {
        let dir = tmp_dir("locator");
        let exe = Path::new("/Applications/malgn-agent.app/Contents/MacOS/malgn-agent");
        assert_eq!(
            write(&dir, exe, "0.9.9", Path::new("/nonexistent-tmp")).unwrap(),
            true
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(locator_path(&dir)).unwrap()).unwrap();
        assert_eq!(v["exe"], exe.to_str().unwrap());
        assert_eq!(v["version"], "0.9.9");

        let dir2 = tmp_dir("locator2");
        let dmg = Path::new("/Volumes/x/malgn-agent.app/Contents/MacOS/malgn-agent");
        assert_eq!(
            write(&dir2, dmg, "0.9.9", Path::new("/nonexistent-tmp")).unwrap(),
            false
        );
        assert!(!locator_path(&dir2).exists());
    }

    #[test]
    fn rejects_relative_exe() {
        let dir = tmp_dir("locator3");
        assert!(write(&dir, Path::new("malgn-agent"), "1", Path::new("/t")).is_err());
    }
}
