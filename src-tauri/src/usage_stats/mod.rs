// ---------------- 사용량 통계 ----------------
// `lib.rs`에 있던 "일별 사용량"·"일별 상세" 두 절을 그대로 옮긴 것이다 — 로직은
// 한 글자도 바꾸지 않았다. `autonomy`/`dev_tools`와 동일한 패턴: `#[tauri::command]`
// 진입점만 이 파일에 두고, 무거운 로직은 서브모듈(`pricing`/`daily`/`detail`)로
// 옮겼다.

mod daily;
mod detail;
mod pricing;

/// `lib.rs`의 `run()` setup이 앱 시작과 동시에 백그라운드에서 어제까지의
/// 사용량 캐시를 미리 채워둘 때 쓴다.
pub(crate) use daily::get_or_refresh_historical_daily_usage;

/// `check_dev_tools`(dev_tools.rs)와 동일한 이유·관용구 — `~/.claude/projects/**/*.jsonl`
/// 재귀 스캔+파싱이 sync 커맨드로 메인 스레드를 막는 P0 버그 계열이라
/// async + `spawn_blocking`으로 옮긴다.
#[tauri::command]
pub async fn get_daily_usage() -> Vec<daily::DailyUsage> {
    tauri::async_runtime::spawn_blocking(daily::aggregate_daily_usage)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_daily_detail(date: String) -> detail::DailyDetailReport {
    detail::aggregate_daily_detail(&date)
}
