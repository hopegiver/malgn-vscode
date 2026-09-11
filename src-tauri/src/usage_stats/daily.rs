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
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.

use chrono::{DateTime, Local, TimeZone, Utc};
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const USAGE_LOOKBACK_DAYS: i64 = 30;

#[derive(Serialize, Debug, Default, Clone)]
pub(crate) struct DailyUsage {
    pub(crate) date: String,
    #[serde(rename = "inputTokens")]
    input_tokens: u64,
    #[serde(rename = "outputTokens")]
    output_tokens: u64,
    #[serde(rename = "cacheCreationTokens")]
    cache_creation_tokens: u64,
    #[serde(rename = "cacheReadTokens")]
    cache_read_tokens: u64,
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

fn find_recent_jsonl_files(dir: &Path, cutoff_mtime: std::time::SystemTime, out: &mut Vec<PathBuf>) {
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
fn scan_file_daily_usage(
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
    let mut seen_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
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
        if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
            if !seen_message_ids.insert(id.to_string()) {
                continue;
            }
        }
        let Some(usage) = msg.get("usage") else {
            continue;
        };

        let key = local_date_key(&ts);
        if skip_date.is_some_and(|d| d == key) {
            continue;
        }
        let entry = buckets.entry(key.clone()).or_insert_with(|| DailyUsage {
            date: key,
            ..Default::default()
        });
        entry.input_tokens += usage
            .get("input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        entry.output_tokens += usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        entry.cache_creation_tokens += usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        entry.cache_read_tokens += usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
    }
    buckets
}

fn merge_daily_usage_maps(
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

    let lookback = std::time::Duration::from_secs(USAGE_LOOKBACK_DAYS as u64 * 24 * 60 * 60);
    let cutoff_mtime = std::time::SystemTime::now()
        .checked_sub(lookback)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let cutoff_dt = Utc::now() - chrono::Duration::days(USAGE_LOOKBACK_DAYS);

    let mut files = Vec::new();
    find_recent_jsonl_files(&projects_dir, cutoff_mtime, &mut files);

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
    let cutoff_dt = Utc::now() - chrono::Duration::days(USAGE_LOOKBACK_DAYS);

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
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_timestamp() {
        assert!(parse_iso_timestamp("2026-09-02T09:01:16.983Z").is_some());
        assert!(parse_iso_timestamp("완전히 이상한 문자열").is_none());
    }

    // 이 세션 자체가 지금 malgn-vscode 프로젝트에서 대량의 assistant 메시지를
    // 만들어내고 있으니, 오늘 날짜의 집계가 0보다 커야 한다 — 실제 로컬 대화
    // 로그(usage 필드)를 정말로 읽어서 합산한다는 실증 근거로 쓴다.
    #[test]
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
