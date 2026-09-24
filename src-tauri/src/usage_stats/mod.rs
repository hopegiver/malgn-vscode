// ---------------- 사용량 통계 ----------------
// `lib.rs`에 있던 "일별 사용량"·"일별 상세" 두 절을 그대로 옮긴 것이다 — 로직은
// 한 글자도 바꾸지 않았다. `autonomy`/`dev_tools`와 동일한 패턴: `#[tauri::command]`
// 진입점만 이 파일에 두고, 무거운 로직은 서브모듈(`pricing`/`daily`/`detail`)로
// 옮겼다.

mod daily;
mod detail;
mod pricing;
mod summary;

/// `lib.rs`의 `run()` setup이 앱 시작과 동시에 백그라운드에서 어제까지의
/// 사용량 캐시를 미리 채워둘 때 쓴다.
pub(crate) use daily::get_or_refresh_historical_daily_usage;

/// `session_list`가 jsonl 행의 `startedAt`(head의 첫 `timestamp`)을 epoch ms로
/// 바꿀 때 재사용한다 — RFC3339 파싱 로직을 새로 만들지 않는다.
pub(crate) use daily::parse_iso_timestamp;

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

/// "사용량 통계" 확장 화면의 30일 요약(최고 사용일·캐시 절대량·입출력비율은
/// 기존 `get_daily_usage`의 `DailyUsage` 30일치만으로 프론트에서 계산 가능해
/// 여기서 다시 만들지 않는다) — 모델별/프로젝트별/툴별 비중과 30일 예상
/// 비용처럼 `DailyUsage`엔 없는 필드만 딱 한 번의 JSONL 스캔으로 추가한다.
/// `get_daily_usage`와 동일한 이유로 async + `spawn_blocking`.
#[tauri::command]
pub async fn get_usage_summary() -> summary::UsageSummaryReport {
    tauri::async_runtime::spawn_blocking(summary::aggregate_usage_summary)
        .await
        .unwrap_or_default()
}

/// 프로젝트별 Top5 카드에서 프로젝트 하나를 선택했을 때만 호출하는 파고들기
/// 뷰 — 그 프로젝트 디렉터리 하나로 스캔 범위를 좁혀 30일 일별 추이를 낸다.
/// 반환 타입은 `get_daily_usage`와 같은 `DailyUsage`라 프론트가 이미 아는
/// 차트 데이터 모양을 그대로 재사용할 수 있다.
#[tauri::command]
pub async fn get_project_daily_trend(project_key: String) -> Vec<daily::DailyUsage> {
    tauri::async_runtime::spawn_blocking(move || {
        summary::aggregate_project_daily_trend(&project_key)
    })
    .await
    .unwrap_or_default()
}
