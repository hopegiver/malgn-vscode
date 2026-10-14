// ---------------- 사용량 통계: 일별 사용량 (실제 로컬 데이터) ----------------
// ~/.claude/projects/**/*.jsonl 전체(모든 프로젝트)를 대상으로 "type":"assistant"
// 줄의 message.usage(입력/출력/캐시 토큰)와 최상위 timestamp만 읽는다. 대화 내용
// (content)은 절대 읽지 않는다 — 순수 숫자 집계다. 대화 로그는 append-only라
// 파일 수정시각이 그 파일의 가장 최신 줄 시각과 같다는 성질을 이용해, 30일보다
// 오래 전에 마지막으로 수정된 파일은 통째로 건너뛴다(열지도 않는다) — 그 다음
// 줄 단위 필터(타임스탬프 30일 이전이면 스킵)로 범위를 다시 좁힌다. `BufReader`로
// 줄 단위 스트리밍한다 — 파일 전체를 메모리에 올리지 않는다.
//
// 어제까지는 다시 바뀔 수 없는 확정값이라 HISTORICAL_USAGE_CACHE에 담아 날짜가
// 바뀌기 전까지 재사용하고(앱 시작 시 백그라운드로 미리 채워둠), 오늘만 호출마다
// 새로 스캔한다(대상 파일이 훨씬 적어 가볍다). 두 경우 다 파일 단위로 rayon
// 병렬 스캔한다 — 파일마다 독립이라 합치기 쉽다.
//
// 같은 스캔에서 날짜별 비용(USD)·미산정 토큰·도구 호출 수·고유 세션 수도 함께
// 집계한다(파일을 두 번 읽지 않는다). 규칙은 `summary.rs`의 요약 집계와 한 치도
// 어긋나지 않아야 한다 — 같은 기간이면 일별 합 = 요약 합:
//   - 토큰·비용: message.id당 첫 줄의 usage만, 단가는 `pricing::cost_for_usage`
//     (캐시 쓰기 5m/1h 분리). 단가표에 없는 모델은 비용 0 + `unpricedTokens`로 노출.
//   - 도구 호출: tool_use.id당 1회(같은 message.id가 여러 줄로 쪼개져도 각 줄의
//     tool_use를 센다). id가 없는 블록은 블록마다 센다.
//   - 세션 수: 그날 usage 또는 tool_use가 집계된 줄의 sessionId 집합 크기. 파일 간
//     병합은 카운트 덧셈이 아니라 집합 합집합이다(본 세션 + subagents/*.jsonl이
//     같은 sessionId를 쓰므로 1로 센다).

use super::pricing;
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use serde::{Serialize, Serializer};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// 집계 창의 길이 = "오늘을 포함한 로컬 달력일 N개". `pub(crate)`: `summary.rs`가
/// 요약 집계에서도 같은 창을 쓴다(정본 하나를 공유해야 30일 합계가 어긋나지 않는다).
/// 창의 시작 시각은 반드시 `usage_window_for`로 구한다 — 롤링 `now - 720h`는
/// 로컬 날짜 31개에 걸칠 수 있어 쓰지 않는다.
pub(crate) const USAGE_LOOKBACK_DAYS: i64 = 30;

/// 집계 창: 오늘 포함 최근 `USAGE_LOOKBACK_DAYS`개 로컬 달력일.
pub(crate) struct UsageWindow {
    /// 창 시작(= 오늘 - (N-1)일의 로컬 00:00). 이 시각 이상의 줄만 센다.
    pub(crate) start_dt: DateTime<Utc>,
    /// 파일 선정용 mtime 컷오프(`start_dt`와 같은 시각). append-only 로그라 이보다
    /// 오래전에 마지막으로 수정된 파일은 창 안의 줄을 담을 수 없다.
    pub(crate) start_mtime: std::time::SystemTime,
}

/// `today`(로컬 날짜)를 기준으로 한 집계 창.
pub(crate) fn usage_window_for(today: NaiveDate) -> UsageWindow {
    let start_date = today - chrono::Duration::days(USAGE_LOOKBACK_DAYS - 1);
    let naive_midnight = start_date.and_hms_opt(0, 0, 0).unwrap_or_default();
    let local_start = match Local.from_local_datetime(&naive_midnight) {
        chrono::LocalResult::Single(dt) => dt,
        chrono::LocalResult::Ambiguous(dt, _) => dt,
        // 그 자정이 DST 건너뜀으로 존재하지 않는 시간대 — 존재하는 가장 이른 시각으로 보정.
        chrono::LocalResult::None => Local
            .from_local_datetime(&(naive_midnight + chrono::Duration::hours(1)))
            .earliest()
            .unwrap_or_else(|| Local::now() - chrono::Duration::days(USAGE_LOOKBACK_DAYS)),
    };
    let start_dt = local_start.with_timezone(&Utc);
    UsageWindow {
        start_dt,
        start_mtime: start_dt.into(),
    }
}

/// 지금(로컬 달력) 기준 집계 창.
pub(crate) fn current_usage_window() -> UsageWindow {
    usage_window_for(Local::now().date_naive())
}

fn serialize_set_len<S: Serializer>(set: &HashSet<String>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(set.len() as u64)
}

/// 날짜별 사용량. IPC 계약(camelCase): `costUsd`(USD, 단가표 매칭 모델분만),
/// `unpricedTokens`(단가표에 없는 모델의 토큰 합 — 0보다 크면 `costUsd`는 과소 표기),
/// `toolCalls`(tool_use.id당 1회), `sessionCount`(고유 세션 수).
#[derive(Serialize, Debug, Default, Clone)]
pub(crate) struct DailyUsage {
    pub(crate) date: String,
    #[serde(rename = "inputTokens")]
    pub(crate) input_tokens: u64,
    #[serde(rename = "outputTokens")]
    pub(crate) output_tokens: u64,
    #[serde(rename = "cacheCreationTokens")]
    pub(crate) cache_creation_tokens: u64,
    #[serde(rename = "cacheReadTokens")]
    pub(crate) cache_read_tokens: u64,
    #[serde(rename = "costUsd")]
    pub(crate) cost_usd: f64,
    #[serde(rename = "unpricedTokens")]
    pub(crate) unpriced_tokens: u64,
    #[serde(rename = "toolCalls")]
    pub(crate) tool_calls: u64,
    /// 직렬화 때 집합 크기(`sessionCount`)로만 나간다 — 세션 ID 자체는 IPC로
    /// 나가지 않는다. 병합은 합집합이라 파일 간 중복 세션이 두 번 세어지지 않는다.
    #[serde(rename = "sessionCount", serialize_with = "serialize_set_len")]
    pub(crate) session_ids: HashSet<String>,
}

pub(crate) fn parse_iso_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

pub(crate) fn local_date_key(dt: &DateTime<Utc>) -> String {
    let local: DateTime<Local> = dt.with_timezone(&Local);
    local.format("%Y-%m-%d").to_string()
}

/// `pub(crate)`: `summary.rs`가 요약 집계(`get_usage_summary`)와 프로젝트별
/// 30일 추이(`get_project_daily_trend`)에서 그대로 재사용한다 — 파일 선정
/// 기준(30일 mtime 컷오프)이 같아야 `get_daily_usage`와 합계가 어긋나지 않는다.
pub(crate) fn find_recent_jsonl_files(dir: &Path, cutoff_mtime: std::time::SystemTime, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_recent_jsonl_files(&path, cutoff_mtime, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        // 파일 전체가 30일보다 오래 전에 마지막으로 수정됐으면(=append-only 로그의
        // 마지막 줄조차 30일 이전) 열어보지도 않고 건너뛴다.
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff_mtime {
                    continue;
                }
            }
        }
        out.push(path);
    }
}

// 파일 하나를 스캔해 그 파일 안에서 나온 날짜별 부분합을 반환한다(다른 파일과
// 독립이라 rayon으로 파일 단위 병렬화하기 좋다). `skip_date`가 있으면 그 날짜
// 줄은 제외한다 — "오늘"은 매번 별도로 실시간 스캔하므로 과거분 캐시 계산에서는
// 오늘 줄을 빼서 이중 집계를 막는다.
/// `pub(crate)`: `summary.rs`의 `get_project_daily_trend`가 특정 프로젝트
/// 디렉터리에 한정해 같은 줄 단위 필터·중복 제거 규칙으로 일별 추이를
/// 뽑을 때 그대로 재사용한다(복붙하면 두 곳의 필터 규칙이 갈라질 위험이 있다).
pub(crate) fn scan_file_daily_usage(
    path: &Path,
    cutoff_dt: DateTime<Utc>,
    skip_date: Option<&str>,
) -> std::collections::BTreeMap<String, DailyUsage> {
    let mut buckets: std::collections::BTreeMap<String, DailyUsage> =
        std::collections::BTreeMap::new();
    let Ok(f) = std::fs::File::open(path) else {
        return buckets;
    };
    // 스트리밍 응답이 여러 JSONL 줄로 쪼개져 기록될 때 같은 message.id가 반복
    // 등장하며 매번 그 턴의 usage를 그대로 다시 실어 나른다(실측: 한 세션에서
    // 고유 메시지 124개인데 usage가 실린 줄은 238개 — 거의 2배 중복). id별로
    // 파일 안에서 한 번만 센다 — 안 세면 토큰이 최대 ~2배 부풀려진다.
    // 단 tool_use는 같은 message.id의 줄마다 다른 블록이 실리므로 줄 단위 dedup 대상이
    // 아니다 — tool_use.id 단위로 따로 센다(`summary.rs`와 같은 규칙).
    let mut seen_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_tool_use_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    // sessionId 필드가 없는 줄을 위한 경로 기반 대체값(지연 계산).
    let mut fallback_session: Option<String> = None;
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(ts) = value
            .get("timestamp")
            .and_then(|t| t.as_str())
            .and_then(parse_iso_timestamp)
        else {
            continue;
        };
        if ts < cutoff_dt {
            continue;
        }
        let Some(msg) = value.get("message") else {
            continue;
        };
        let first_line_of_message = match msg.get("id").and_then(|v| v.as_str()) {
            Some(id) => seen_message_ids.insert(id.to_string()),
            None => true,
        };
        let usage = if first_line_of_message {
            msg.get("usage")
        } else {
            None
        };

        // 이 줄에서 새로 세어질 tool_use 개수. seen 집합 갱신은 skip_date 판정보다
        // 앞서야 한다(오늘 줄을 건너뛰어도 id는 소비돼 전체 스캔과 같은 결과가 된다).
        let mut new_tool_calls: u64 = 0;
        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            for item in content {
                if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                    continue;
                }
                if let Some(tid) = item.get("id").and_then(|v| v.as_str()) {
                    if !seen_tool_use_ids.insert(tid.to_string()) {
                        continue;
                    }
                }
                // summary.rs와 같이 name이 있는 블록만 센다.
                if item.get("name").and_then(|n| n.as_str()).is_some() {
                    new_tool_calls += 1;
                }
            }
        }
        if usage.is_none() && new_tool_calls == 0 {
            continue;
        }

        let key = local_date_key(&ts);
        if skip_date.is_some_and(|d| d == key) {
            continue;
        }
        let entry = buckets.entry(key.clone()).or_insert_with(|| DailyUsage {
            date: key,
            ..Default::default()
        });
        entry.tool_calls += new_tool_calls;

        if let Some(usage) = usage {
            let model = msg
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            let input = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_creation = usage
                .get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_read = usage
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            entry.input_tokens += input;
            entry.output_tokens += output;
            entry.cache_creation_tokens += cache_creation;
            entry.cache_read_tokens += cache_read;

            // 비용은 summary.rs와 같은 경로: 토큰 합계(cache_creation_input_tokens)와
            // 별개로 비용에는 5m/1h 분해값을 쓴다.
            let (cache_write_5m, cache_write_1h) = pricing::split_cache_write_tokens(usage);
            let cost = pricing::cost_for_usage(
                model,
                input,
                output,
                cache_write_5m,
                cache_write_1h,
                cache_read,
            );
            entry.cost_usd += cost.cost_usd;
            if !cost.pricing_matched {
                entry.unpriced_tokens += input + output + cache_creation + cache_read;
            }
        }

        let session = match value.get("sessionId").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s,
            _ => fallback_session
                .get_or_insert_with(|| session_id_from_path(path))
                .as_str(),
        };
        if !entry.session_ids.contains(session) {
            entry.session_ids.insert(session.to_string());
        }
    }
    buckets
}

/// 줄에 `sessionId`가 없을 때의 대체 식별자. 실제 로그 구조: 본 세션은
/// `<projectKey>/<sessionId>.jsonl`, 서브에이전트는
/// `<projectKey>/<sessionId>/subagents/agent-*.jsonl` — 둘 다 같은 sessionId를 낸다.
fn session_id_from_path(path: &Path) -> String {
    let parent = path.parent();
    if parent.and_then(|p| p.file_name()).and_then(|n| n.to_str()) == Some("subagents") {
        if let Some(name) = parent
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
        {
            return name.to_string();
        }
    }
    path.file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// `pub(crate)`: `summary.rs`의 `get_project_daily_trend`가 파일 단위 병렬
/// 스캔 결과를 합칠 때 재사용한다.
pub(crate) fn merge_daily_usage_maps(
    mut a: std::collections::BTreeMap<String, DailyUsage>,
    b: std::collections::BTreeMap<String, DailyUsage>,
) -> std::collections::BTreeMap<String, DailyUsage> {
    for (key, v) in b {
        let entry = a.entry(key.clone()).or_insert_with(|| DailyUsage {
            date: key,
            ..Default::default()
        });
        entry.input_tokens += v.input_tokens;
        entry.output_tokens += v.output_tokens;
        entry.cache_creation_tokens += v.cache_creation_tokens;
        entry.cache_read_tokens += v.cache_read_tokens;
        entry.cost_usd += v.cost_usd;
        entry.unpriced_tokens += v.unpriced_tokens;
        entry.tool_calls += v.tool_calls;
        // 세션은 카운트가 아니라 집합으로 합친다 — 같은 세션이 여러 파일에 걸쳐도 1.
        entry.session_ids.extend(v.session_ids);
    }
    a
}

fn today_local_date_key() -> String {
    local_date_key(&Utc::now())
}

/// 요청받은 로컬 날짜("YYYY-MM-DD")의 로컬 자정을 `SystemTime`으로 바꾼다 —
/// 파일 mtime이 이보다 이전이면 그 날짜의 활동을 담고 있을 수 없으므로 열어보지
/// 않고 건너뛰는 컷오프로 쓴다. 날짜 파싱에 실패하면 `None`. `detail.rs`의
/// `aggregate_daily_detail`도 같은 목적으로 이 함수를 재사용한다(원래도 하나의
/// 함수를 두 절이 공유했다 — `pub(crate)`로 옮긴 것 외에 로직은 그대로다).
pub(crate) fn local_midnight_as_system_time(date: &str) -> Option<std::time::SystemTime> {
    let naive_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let naive_midnight = naive_date.and_hms_opt(0, 0, 0)?;
    let local_midnight = match Local.from_local_datetime(&naive_midnight) {
        chrono::LocalResult::Single(dt) => dt,
        chrono::LocalResult::Ambiguous(dt, _) => dt,
        chrono::LocalResult::None => return None,
    };
    Some(local_midnight.with_timezone(&Utc).into())
}

// 어제까지의 집계는 성격상 다시는 안 바뀐다(과거 날짜로 새 줄이 추가될 리 없다)
// — 그래서 무효화 로직 없이 "오늘 날짜가 바뀌기 전까지" 그냥 재사용해도 된다.
// 앱을 오래 켜둔 채 자정을 넘기면 다음 호출에서 cached_as_of가 어긋난 걸 감지해
// 하루 한 번만 다시 계산한다.
struct HistoricalUsageCache {
    cached_as_of: String,
    days: Vec<DailyUsage>,
}

static HISTORICAL_USAGE_CACHE: std::sync::Mutex<Option<HistoricalUsageCache>> =
    std::sync::Mutex::new(None);

// 30일 룩백 범위에서 "오늘"을 뺀 나머지(어제까지)를 파일 단위로 병렬 스캔한다.
// 파일이 서로 독립이라 rayon의 파일별 par_iter + reduce로 코어 수만큼 나눠
// 읽는다 — 이 부분이 전체 스캔 시간의 대부분을 차지했다.
fn compute_historical_daily_usage(today: &str) -> Vec<DailyUsage> {
    use rayon::prelude::*;

    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return Vec::new();
    }

    // 캐시 키(`today`)와 같은 날짜로 창을 만든다 — 자정 경계에서 캐시 날짜와 창이 어긋나지 않게.
    let window = usage_window_for(
        NaiveDate::parse_from_str(today, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive()),
    );
    let cutoff_dt = window.start_dt;

    let mut files = Vec::new();
    find_recent_jsonl_files(&projects_dir, window.start_mtime, &mut files);

    let buckets = files
        .par_iter()
        .map(|file| scan_file_daily_usage(file, cutoff_dt, Some(today)))
        .reduce(std::collections::BTreeMap::new, merge_daily_usage_maps);

    buckets.into_values().collect()
}

/// `lib.rs`의 `run()` setup이 앱 시작 시 백그라운드 스레드에서 미리 채워둔다
/// (사용자가 "사용량 통계"를 처음 열었을 때부터 이미 준비돼 있게) —
/// `usage_stats::mod`가 `pub(crate) use daily::get_or_refresh_historical_daily_usage;`로
/// 재노출한다.
pub(crate) fn get_or_refresh_historical_daily_usage() -> Vec<DailyUsage> {
    let today = today_local_date_key();
    {
        let guard = HISTORICAL_USAGE_CACHE.lock().unwrap();
        if let Some(cache) = guard.as_ref() {
            if cache.cached_as_of == today {
                return cache.days.clone();
            }
        }
    }
    let days = compute_historical_daily_usage(&today);
    *HISTORICAL_USAGE_CACHE.lock().unwrap() = Some(HistoricalUsageCache {
        cached_as_of: today,
        days: days.clone(),
    });
    days
}

// "오늘"만 매번 새로 스캔한다 — mtime 컷오프를 오늘 자정으로 좁혀서 대상 파일
// 자체가 훨씬 적다(과거분처럼 캐싱하면 방금 쓴 토큰이 안 보이니 여기만 캐시하지
// 않는다).
fn compute_today_daily_usage(today: &str) -> Option<DailyUsage> {
    use rayon::prelude::*;

    let Some(home) = dirs::home_dir() else {
        return None;
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return None;
    }
    let cutoff_mtime =
        local_midnight_as_system_time(today).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    // 줄 필터는 과거분 스캔과 같은 창 시작을 써야 한다 — message.id 첫 줄이 어제에 있으면
    // 오늘 줄은 중복으로 빠지는 규칙이 전체 스캔·요약과 같게 유지된다. 파일 선정만 오늘
    // 자정으로 좁힌다.
    let cutoff_dt = usage_window_for(
        NaiveDate::parse_from_str(today, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive()),
    )
    .start_dt;

    let mut files = Vec::new();
    find_recent_jsonl_files(&projects_dir, cutoff_mtime, &mut files);

    let buckets = files
        .par_iter()
        .map(|file| scan_file_daily_usage(file, cutoff_dt, None))
        .reduce(std::collections::BTreeMap::new, merge_daily_usage_maps);

    buckets
        .into_iter()
        .find(|(date, _)| date == today)
        .map(|(_, v)| v)
}

pub(crate) fn aggregate_daily_usage() -> Vec<DailyUsage> {
    let today = today_local_date_key();
    let mut buckets: std::collections::BTreeMap<String, DailyUsage> =
        get_or_refresh_historical_daily_usage()
            .into_iter()
            .map(|d| (d.date.clone(), d))
            .collect();
    if let Some(today_bucket) = compute_today_daily_usage(&today) {
        buckets.insert(today.clone(), today_bucket);
    }
    buckets.into_values().collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_timestamp() {
        assert!(parse_iso_timestamp("2026-09-02T09:01:16.983Z").is_some());
        assert!(parse_iso_timestamp("완전히 이상한 문자열").is_none());
    }

    // ---- 합성 fixture 헬퍼 (실제 로그 원문이 아니라 필드 모양만 흉내 낸 가짜 데이터) ----

    static TEST_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub(crate) fn test_dir() -> PathBuf {
        let seq = TEST_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-daily-test-{}-{}",
            std::process::id(),
            seq
        ));
        std::fs::create_dir_all(&dir).expect("fixture 디렉터리 생성 실패");
        dir
    }

    /// 로컬 `date`의 `h:m`을 RFC3339(UTC)로.
    pub(crate) fn ts_local(date: NaiveDate, h: u32, m: u32) -> String {
        let naive = date.and_hms_opt(h, m, 0).unwrap();
        Local
            .from_local_datetime(&naive)
            .earliest()
            .expect("로컬 시각 변환 실패")
            .with_timezone(&Utc)
            .to_rfc3339()
    }

    pub(crate) struct L<'a> {
        pub ts: String,
        pub session: Option<&'a str>,
        pub msg_id: &'a str,
        pub model: &'a str,
        /// None이면 usage 필드 자체가 없는 줄.
        pub usage: Option<(u64, u64)>,
        /// (tool_use.id, name)
        pub tools: Vec<(Option<&'a str>, &'a str)>,
    }

    impl<'a> L<'a> {
        pub(crate) fn json(&self) -> String {
            let content: Vec<Value> = self
                .tools
                .iter()
                .map(|(id, name)| match id {
                    Some(id) => serde_json::json!({"type":"tool_use","id":id,"name":name}),
                    None => serde_json::json!({"type":"tool_use","name":name}),
                })
                .collect();
            let mut message = serde_json::json!({
                "id": self.msg_id, "model": self.model, "content": content
            });
            if let Some((i, o)) = self.usage {
                message["usage"] = serde_json::json!({
                    "input_tokens": i, "output_tokens": o,
                    "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0
                });
            }
            let mut v = serde_json::json!({
                "type": "assistant", "timestamp": self.ts, "message": message
            });
            if let Some(sid) = self.session {
                v["sessionId"] = serde_json::json!(sid);
            }
            v.to_string()
        }
    }

    pub(crate) fn write_lines(path: &Path, lines: &[L]) {
        let body: Vec<String> = lines.iter().map(|l| l.json()).collect();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body.join("\n") + "\n").expect("fixture 쓰기 실패");
    }

    fn far_cutoff() -> DateTime<Utc> {
        Utc::now() - chrono::Duration::days(3650)
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    // 규칙 3: 같은 세션이 본 세션 파일 + subagents 파일에 걸쳐도 1로 센다(합집합).
    // 다른 세션은 따로 센다.
    #[test]
    fn session_count_is_union_across_files_not_sum() {
        let dir = test_dir();
        let day = d(2026, 10, 10);
        let mk = |sid: &'static str, id: &'static str| L {
            ts: ts_local(day, 10, 0),
            session: Some(sid),
            msg_id: id,
            model: "claude-sonnet-4-6",
            usage: Some((10, 5)),
            tools: vec![],
        };
        let main = dir.join("proj").join("sess-A.jsonl");
        let sub1 = dir.join("proj").join("sess-A").join("subagents").join("agent-1.jsonl");
        let sub2 = dir.join("proj").join("sess-A").join("subagents").join("agent-2.jsonl");
        let other = dir.join("proj").join("sess-B.jsonl");
        write_lines(&main, &[mk("sess-A", "m1"), mk("sess-A", "m2")]);
        write_lines(&sub1, &[mk("sess-A", "s1")]);
        write_lines(&sub2, &[mk("sess-A", "s2")]);
        write_lines(&other, &[mk("sess-B", "o1")]);

        let merged = [&main, &sub1, &sub2, &other]
            .iter()
            .map(|p| scan_file_daily_usage(p, far_cutoff(), None))
            .fold(std::collections::BTreeMap::new(), merge_daily_usage_maps);
        let bucket = &merged["2026-10-10"];
        assert_eq!(bucket.session_ids.len(), 2, "sess-A가 파일 수만큼 중복 집계됐습니다");
        assert_eq!(bucket.input_tokens, 50);

        // 병합 순서(rayon reduce 트리 모양)에 상관없이 같아야 한다.
        let rev = [&other, &sub2, &sub1, &main]
            .iter()
            .map(|p| scan_file_daily_usage(p, far_cutoff(), None))
            .fold(std::collections::BTreeMap::new(), merge_daily_usage_maps);
        assert_eq!(rev["2026-10-10"].session_ids.len(), 2);

        // 직렬화는 세션 ID가 아니라 개수만 내보낸다.
        let json = serde_json::to_value(bucket).unwrap();
        assert_eq!(json["sessionCount"], 2);
        assert!(!json.to_string().contains("sess-A"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // sessionId 필드가 없는 줄은 경로로 대체한다: 본 세션은 파일명, 서브에이전트는
    // subagents 상위 디렉터리명 — 실제 로그에서도 두 값이 줄의 sessionId와 일치한다.
    #[test]
    fn session_id_falls_back_to_path_when_field_missing() {
        let dir = test_dir();
        let day = d(2026, 10, 10);
        let line = |id| L {
            ts: ts_local(day, 10, 0),
            session: None,
            msg_id: id,
            model: "claude-sonnet-4-6",
            usage: Some((1, 1)),
            tools: vec![],
        };
        let main = dir.join("proj").join("sess-A.jsonl");
        let sub = dir.join("proj").join("sess-A").join("subagents").join("agent-9.jsonl");
        write_lines(&main, &[line("a")]);
        write_lines(&sub, &[line("b")]);
        let merged = [&main, &sub]
            .iter()
            .map(|p| scan_file_daily_usage(p, far_cutoff(), None))
            .fold(std::collections::BTreeMap::new(), merge_daily_usage_maps);
        assert_eq!(merged["2026-10-10"].session_ids.len(), 1);
        assert!(merged["2026-10-10"].session_ids.contains("sess-A"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 규칙 2: 같은 message.id가 여러 줄로 쪼개져도 각 줄의 tool_use를 센다.
    // usage 없는 줄의 tool_use도 센다. 같은 tool_use.id는 1회, id 없는 블록은 각각.
    // 토큰은 message.id당 한 번.
    #[test]
    fn counts_tool_use_across_split_lines_and_usage_less_lines() {
        let dir = test_dir();
        let day = d(2026, 10, 10);
        let path = dir.join("p").join("s.jsonl");
        let l = |usage, tools| L {
            ts: ts_local(day, 11, 0),
            session: Some("s"),
            msg_id: "msg_split",
            model: "claude-sonnet-4-6",
            usage,
            tools,
        };
        write_lines(
            &path,
            &[
                l(Some((100, 0)), vec![(Some("t_a"), "Read")]),
                l(Some((100, 0)), vec![(Some("t_b"), "Bash")]),
                l(Some((100, 0)), vec![(Some("t_b"), "Bash"), (Some("t_c"), "Bash")]), // t_b 중복
                l(None, vec![(Some("t_d"), "Grep")]),                                  // usage 없는 줄
                l(Some((100, 0)), vec![(None, "Edit"), (None, "Edit")]),               // id 없음
            ],
        );
        let buckets = scan_file_daily_usage(&path, far_cutoff(), None);
        let b = &buckets["2026-10-10"];
        assert_eq!(b.tool_calls, 6, "Read1 + Bash2 + Grep1 + Edit2");
        assert_eq!(b.input_tokens, 100, "토큰은 message.id당 첫 줄만");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 규칙 2 보조: 오늘 줄을 건너뛰는 과거분 스캔(skip_date)에서도 tool_use.id는 소비되어,
    // (과거분 + 오늘) 합이 전체 스캔과 같다. 또 오늘 줄은 과거분에 들어가지 않는다.
    #[test]
    fn skip_date_split_matches_full_scan() {
        let dir = test_dir();
        let (yday, today) = (d(2026, 10, 9), d(2026, 10, 10));
        let path = dir.join("p").join("s.jsonl");
        let mk = |ts: String, id, tools| L {
            ts,
            session: Some("s"),
            msg_id: id,
            model: "claude-sonnet-4-6",
            usage: Some((10, 0)),
            tools,
        };
        write_lines(
            &path,
            &[
                mk(ts_local(yday, 23, 59), "m1", vec![(Some("t1"), "Read")]),
                // 같은 message.id가 자정을 넘겨 오늘 줄로 이어진다(usage는 어제 첫 줄 몫).
                mk(ts_local(today, 0, 1), "m1", vec![(Some("t2"), "Bash"), (Some("t1"), "Read")]),
                mk(ts_local(today, 9, 0), "m2", vec![(Some("t3"), "Edit")]),
            ],
        );
        let full = scan_file_daily_usage(&path, far_cutoff(), None);
        let hist = scan_file_daily_usage(&path, far_cutoff(), Some("2026-10-10"));
        assert!(!hist.contains_key("2026-10-10"));
        let sum = |m: &std::collections::BTreeMap<String, DailyUsage>| {
            m.values().fold((0u64, 0u64), |a, b| (a.0 + b.tool_calls, a.1 + b.input_tokens))
        };
        assert_eq!(sum(&full), (3, 20));
        assert_eq!(full["2026-10-09"].tool_calls, 1);
        assert_eq!(full["2026-10-10"].tool_calls, 2);
        assert_eq!(hist["2026-10-09"].tool_calls, 1);
        assert_eq!(hist["2026-10-09"].input_tokens, 10);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 규칙 1: 비용은 summary와 같은 단가 경로(5m/1h 분리), 미매칭 모델은 비용 0 +
    // unpricedTokens에 토큰 전량. 손계산(sonnet-4-6: cache_write 5m 3.75, 1h 6.0 / 1M):
    //   40_000*3.75/1e6 + 60_000*6.0/1e6 = 0.15 + 0.36 = 0.51
    #[test]
    fn cost_uses_cache_ttl_split_and_flags_unpriced_models() {
        let dir = test_dir();
        let day = d(2026, 10, 10);
        let path = dir.join("p").join("s.jsonl");
        let priced = serde_json::json!({
            "type":"assistant","timestamp":ts_local(day, 9, 0),"sessionId":"s",
            "message":{"id":"m1","model":"claude-sonnet-4-6","usage":{
                "input_tokens":0,"output_tokens":0,
                "cache_creation_input_tokens":100000,"cache_read_input_tokens":0,
                "cache_creation":{"ephemeral_5m_input_tokens":40000,"ephemeral_1h_input_tokens":60000}}}
        });
        let unpriced = serde_json::json!({
            "type":"assistant","timestamp":ts_local(day, 10, 0),"sessionId":"s",
            "message":{"id":"m2","model":"some-unreleased-model","usage":{
                "input_tokens":1000,"output_tokens":200,
                "cache_creation_input_tokens":30,"cache_read_input_tokens":70}}
        });
        let other_day = serde_json::json!({
            "type":"assistant","timestamp":ts_local(d(2026, 10, 9), 10, 0),"sessionId":"s",
            "message":{"id":"m3","model":"claude-sonnet-4-6","usage":{
                "input_tokens":10,"output_tokens":0,
                "cache_creation_input_tokens":0,"cache_read_input_tokens":0}}
        });
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{priced}\n{unpriced}\n{other_day}\n")).unwrap();
        let buckets = scan_file_daily_usage(&path, far_cutoff(), None);
        let b = &buckets["2026-10-10"];
        assert!((b.cost_usd - 0.51).abs() < 1e-9, "비용: {}", b.cost_usd);
        assert_eq!(b.unpriced_tokens, 1000 + 200 + 30 + 70);
        // 토큰 합계는 미매칭 모델도 포함(기존 계약 유지).
        assert_eq!(b.input_tokens, 1000);
        assert_eq!(b.cache_creation_tokens, 100_030);
        // 미매칭이 없는 날은 unpriced 0.
        assert_eq!(buckets["2026-10-09"].unpriced_tokens, 0);
        let json = serde_json::to_value(b).unwrap();
        for key in ["costUsd", "unpricedTokens", "toolCalls", "sessionCount"] {
            assert!(json.get(key).is_some_and(|v| v.is_number()), "{key} 누락");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 규칙 4 (RV-007): 일별 결과의 날짜는 오늘 포함 30개. 41일치 하루 3줄(00:30/12:00/23:30)을
    // 넣고 창 경계(시작일 00:30 포함, 전날 23:30 제외)를 확인한다.
    #[test]
    fn window_yields_exactly_30_local_calendar_days() {
        let dir = test_dir();
        let today = d(2026, 10, 14);
        let path = dir.join("p").join("s.jsonl");
        let mut lines = Vec::new();
        let mut n = 0;
        let ids: Vec<String> = (0..41 * 3).map(|i| format!("m{i}")).collect();
        for back in 0..=40 {
            let date = today - chrono::Duration::days(back);
            for (h, m) in [(0, 30), (12, 0), (23, 30)] {
                lines.push(L {
                    ts: ts_local(date, h, m),
                    session: Some("s"),
                    msg_id: &ids[n],
                    model: "claude-sonnet-4-6",
                    usage: Some((1, 0)),
                    tools: vec![],
                });
                n += 1;
            }
        }
        write_lines(&path, &lines);
        let window = usage_window_for(today);
        let buckets = scan_file_daily_usage(&path, window.start_dt, None);
        assert_eq!(buckets.len() as i64, USAGE_LOOKBACK_DAYS, "날짜 수: {:?}", buckets.keys());
        assert_eq!(buckets.keys().next().unwrap(), "2026-09-15"); // 오늘-29
        assert_eq!(buckets.keys().last().unwrap(), "2026-10-14");
        // 창 첫날은 00:30부터 3줄 전부, 그 전날 23:30은 제외.
        assert_eq!(buckets["2026-09-15"].input_tokens, 3);
        assert!(!buckets.contains_key("2026-09-14"));
        assert_eq!(window.start_mtime, std::time::SystemTime::from(window.start_dt));

        // 롤링 720h 컷오프(이전 방식)는 같은 데이터에서 31개 날짜를 만든다 — 회귀 근거.
        let rolling = scan_file_daily_usage(
            &path,
            ts_local_dt(today, 12, 0) - chrono::Duration::days(USAGE_LOOKBACK_DAYS),
            None,
        );
        assert_eq!(rolling.len() as i64, USAGE_LOOKBACK_DAYS + 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn ts_local_dt(date: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
        parse_iso_timestamp(&ts_local(date, h, m)).unwrap()
    }

    // 이 세션 자체가 지금 malgn-vscode 프로젝트에서 대량의 assistant 메시지를
    // 만들어내고 있으니, 오늘 날짜의 집계가 0보다 커야 한다 — 실제 로컬 대화
    // 로그(usage 필드)를 정말로 읽어서 합산한다는 실증 근거로 쓴다.
    // 머신 의존(오늘 날짜 실사용 로그가 있어야 함) — CI 러너에는 없어 #[ignore].
    // 로컬 실행: cargo test -- --ignored usage_stats::daily::tests::aggregates_daily_usage_and_includes_today_with_nonzero_tokens
    #[test]
    #[ignore]
    fn aggregates_daily_usage_and_includes_today_with_nonzero_tokens() {
        let daily = aggregate_daily_usage();
        assert!(!daily.is_empty(), "최근 30일 사용량 집계가 비어 있습니다");

        let today = Local::now().format("%Y-%m-%d").to_string();
        let today_entry = daily.iter().find(|d| d.date == today);
        assert!(
            today_entry.is_some(),
            "오늘({today}) 날짜의 사용량 항목이 없습니다"
        );

        let today_entry = today_entry.unwrap();
        let total = today_entry.input_tokens
            + today_entry.output_tokens
            + today_entry.cache_creation_tokens
            + today_entry.cache_read_tokens;
        assert!(total > 0, "오늘 사용량 합계가 0입니다");
    }

    // 어제까지의 집계는 캐시에서 그대로 재사용되고, 다시 계산해도 같은 결과가
    // 나와야 한다(캐시가 틀린 값을 굳혀버리면 안 된다) — 캐시 히트/미스 두 경로
    // 모두 검증한다.
    #[test]
    fn historical_daily_usage_cache_is_consistent_across_calls() {
        let today = today_local_date_key();
        let first = get_or_refresh_historical_daily_usage();
        let cached = HISTORICAL_USAGE_CACHE
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.cached_as_of.clone());
        assert_eq!(
            cached,
            Some(today.clone()),
            "캐시가 오늘 날짜로 채워지지 않았습니다"
        );

        let second = get_or_refresh_historical_daily_usage();
        assert_eq!(
            first.len(),
            second.len(),
            "캐시 히트 결과의 항목 수가 달라졌습니다"
        );
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.date, b.date);
            assert_eq!(a.input_tokens, b.input_tokens);
            assert_eq!(a.output_tokens, b.output_tokens);
        }
        assert!(
            !first.iter().any(|d| d.date == today),
            "과거분 캐시에 오늘 날짜가 섞여 있습니다"
        );
    }
}
