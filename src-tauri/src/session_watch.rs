// ---------------- 세션목록 실시간 감시 (파일시스템 이벤트, 폴링 아님) ----------------
// ~/.claude/sessions/ 를 앱이 켜져 있는 동안 계속 감시한다(세션목록 화면을 보고
// 있을 때만이 아니라 setup()에서 한 번 등록). 변경이 감지되면 프론트엔드에
// "claude-sessions-changed" 이벤트만 쏘고, 실제로 무엇이 바뀌었는지는 프론트가
// 이미 있는 list_claude_sessions()를 다시 호출해서 알아낸다 — 이 함수는 "다시
// 불러올 시점"만 알려준다. notify-debouncer-mini가 짧은 시간에 몰리는 이벤트를
// 500ms로 묶어준다(디바운스) — setInterval 폴링이 아니라 OS 파일시스템 이벤트
// 기반이다.
pub(crate) fn watch_claude_sessions_dir(app_handle: tauri::AppHandle) {
    use notify_debouncer_mini::new_debouncer;
    use notify_debouncer_mini::notify::RecursiveMode;
    use std::sync::mpsc;
    use std::time::Duration;

    let Some(home) = dirs::home_dir() else {
        return;
    };
    let sessions_dir = home.join(".claude").join("sessions");

    // 세션 디렉터리가 아직 없으면(첫 세션 전) 조용히 포기한다 — 앱이 죽으면 안 된다.
    if !sessions_dir.is_dir() {
        return;
    }

    let (tx, rx) = mpsc::channel();
    let Ok(mut debouncer) = new_debouncer(Duration::from_millis(500), tx) else {
        return;
    };
    if debouncer
        .watcher()
        .watch(&sessions_dir, RecursiveMode::NonRecursive)
        .is_err()
    {
        return;
    }

    // debouncer를 이 블록 안에 살려둔 채 rx를 계속 받는다 — 이 for 루프가 이
    // 백그라운드 스레드의 나머지 수명이다(앱이 종료되면 스레드도 함께 끝난다).
    for result in rx {
        if let Ok(events) = result {
            if !events.is_empty() {
                use tauri::Emitter;
                let _ = app_handle.emit("claude-sessions-changed", ());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // 세션목록 실시간 감시의 핵심 메커니즘(디바운서로 감싼 notify 워처가 디렉터리
    // 변경을 실제로 잡아내는지)만 격리해서 검증한다 — app_handle.emit()까지 엮인
    // 전체 흐름(Tauri 앱 컨텍스트 필요)은 유닛 테스트로 무리하게 재현하지 않고
    // `cargo check`/`tsc`로 컴파일 정확성만 확인했다(프론트 연동은 수동 확인 필요).
    #[test]
    fn detects_file_change_in_watched_directory() {
        use notify_debouncer_mini::new_debouncer;
        use notify_debouncer_mini::notify::RecursiveMode;
        use std::sync::mpsc;
        use std::time::Duration;

        let tmp_dir =
            std::env::temp_dir().join(format!("malgn-vscode-watch-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).expect("임시 디렉터리를 만들지 못했습니다");

        let (tx, rx) = mpsc::channel();
        let mut debouncer =
            new_debouncer(Duration::from_millis(200), tx).expect("디바운서 생성 실패");
        debouncer
            .watcher()
            .watch(&tmp_dir, RecursiveMode::NonRecursive)
            .expect("워처 등록 실패");

        std::fs::write(tmp_dir.join("test.json"), "{}").expect("테스트 파일 쓰기 실패");

        let event = rx.recv_timeout(Duration::from_secs(5));
        assert!(
            event.is_ok(),
            "디렉터리 변경 이벤트를 감지하지 못했습니다(타임아웃)"
        );
        let events = event.unwrap();
        assert!(
            events.is_ok(),
            "워처가 에러를 반환했습니다: {:?}",
            events.err()
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
