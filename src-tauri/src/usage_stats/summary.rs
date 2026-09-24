// ---------------- 사용량 통계: 30일 요약(모델·프로젝트·툴·비용) ----------------
// "사용량 통계" 화면 확장을 위한 새 집계다. `daily::aggregate_daily_usage()`는
// 모델·프로젝트·비용을 읽고도 버린다 — 이 파일은 그 버려지는 필드를 마저
// 읽어 30일 전체를 "한 번의 JSONL 스캔"으로 모델별/프로젝트별/툴별로
// 나눠 담는다(프론트가 `get_daily_detail`을 30번 부르는 방식은 쓰지 않는다).
//
// `daily.rs`와 합계가 어긋나지 않도록, 파일 선정(`find_recent_jsonl_files`)과
// 줄 단위 필터·중복 제거(`scan_file_daily_usage`가 쓰는 것과 동일한 규칙:
// "type":"assistant", timestamp 30일 컷오프, message.id 파일 내 중복 제거)를
// 여기서도 똑같이 지킨다 — 모델/프로젝트/툴 추출만 더할 뿐 그 판정 로직은
// 새로 만들지 않고 `daily::find_recent_jsonl_files`를 그대로 재사용한다.

use super::daily::{self, parse_iso_timestamp, USAGE_LOOKBACK_DAYS};
use super::pricing::cost_for_usage;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// 프로젝트별/툴별 랭킹에서 상위 몇 개까지 개별 노출할지.
const TOP_PROJECTS: usize = 5;
const TOP_TOOLS: usize = 5;

#[derive(Serialize, Debug, Clone, Default)]
pub(crate) struct ModelUsageSummary {
    /// "opus"/"sonnet"/"haiku" 중 하나, 또는 세 키워드 어디에도 매칭되지
    /// 않은 모델명은 "unknown"으로 따로 묶는다(=아래 `pricing_matched: false`).
    family: String,
    tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
    /// false면 이 버킷의 비용은 `pricing.rs` 단가표의 모델명 매칭에 실패해
    /// sonnet 단가로 대체 계산한 추정치다 — 프론트는 이 값이 false인 행에
    /// "단가 미확인(추정)" 같은 표시를 붙일 수 있다. 조용히 다른 family의
    /// 실측치에 섞이지 않도록 family를 "unknown"으로 분리해 둔다.
    #[serde(rename = "pricingMatched")]
    pricing_matched: bool,
}

#[derive(Serialize, Debug, Clone, Default)]
pub(crate) struct ProjectUsageSummary {
    #[serde(rename = "projectKey")]
    project_key: String,
    /// 사람이 읽을 이름. `~/workspace/<이름>` 관례를 따르는 프로젝트는
    /// `<이름>`만 남기고, 그 관례를 벗어나면(다른 루트, 관례 불일치 등)
    /// 추측하지 않고 `projectKey`에서 선행 `-`만 뗀 원문을 그대로 쓴다.
    #[serde(rename = "displayName")]
    display_name: String,
    tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
}

#[derive(Serialize, Debug, Clone, Default)]
pub(crate) struct ToolUsageSummary {
    #[serde(rename = "toolName")]
    tool_name: String,
    count: u32,
}

#[derive(Serialize, Debug, Clone, Default)]
pub(crate) struct UsageSummaryReport {
    /// 집계 룩백 일수(현재 30) — 프론트가 "최근 N일" 라벨을 하드코딩하지
    /// 않도록 그대로 실어 보낸다.
    days: i64,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    #[serde(rename = "totalCostUsd")]
    total_cost_usd: f64,
    #[serde(rename = "inputTokens")]
    input_tokens: u64,
    #[serde(rename = "outputTokens")]
    output_tokens: u64,
    #[serde(rename = "cacheCreationTokens")]
    cache_creation_tokens: u64,
    #[serde(rename = "cacheReadTokens")]
    cache_read_tokens: u64,
    /// family(opus/sonnet/haiku/unknown)별 — tokens desc 정렬.
    models: Vec<ModelUsageSummary>,
    /// 토큰 기준 상위 `TOP_PROJECTS`개 — tokens desc 정렬.
    projects: Vec<ProjectUsageSummary>,
    /// 상위 노출에서 빠진 나머지 프로젝트들의 합(투명성 — top5 밖으로 사라진
    /// 토큰/비용이 없다는 것을 프론트가 확인할 수 있게).
    #[serde(rename = "otherProjectsTokens")]
    other_projects_tokens: u64,
    #[serde(rename = "otherProjectsCostUsd")]
    other_projects_cost_usd: f64,
    /// 횟수 기준 상위 `TOP_TOOLS`개 — count desc 정렬.
    tools: Vec<ToolUsageSummary>,
}

/// 파일 하나를 스캔한 부분합(모델별/프로젝트별/툴별) — rayon reduce로 합친다.
#[derive(Default)]
struct PartialSummary {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    cost_usd: f64,
    /// key: (family, pricing_matched)
    model_tally: HashMap<(String, bool), (u64, f64)>,
    /// key: projectKey
    project_tally: HashMap<String, (u64, f64)>,
    tool_counts: HashMap<String, u32>,
}

fn merge_partial(mut a: PartialSummary, b: PartialSummary) -> PartialSummary {
    a.input_tokens += b.input_tokens;
    a.output_tokens += b.output_tokens;
    a.cache_creation_tokens += b.cache_creation_tokens;
    a.cache_read_tokens += b.cache_read_tokens;
    a.cost_usd += b.cost_usd;
    for (key, (tokens, cost)) in b.model_tally {
        let entry = a.model_tally.entry(key).or_insert((0, 0.0));
        entry.0 += tokens;
        entry.1 += cost;
    }
    for (key, (tokens, cost)) in b.project_tally {
        let entry = a.project_tally.entry(key).or_insert((0, 0.0));
        entry.0 += tokens;
        entry.1 += cost;
    }
    for (name, count) in b.tool_counts {
        *a.tool_counts.entry(name).or_insert(0) += count;
    }
    a
}

/// `model`(원문, 예: "claude-opus-4-8")을 pricing.rs 단가표가 실제로
/// 구분하는 family로 분류한다. `pricing::model_family`(private)와 키워드는
/// 같지만, 저 함수는 미매칭 시 조용히 "sonnet"으로 폴백해 비용 계산에만
/// 쓰기 적합하다 — 여기서는 "어떤 모델이 진짜 sonnet인지 vs 미매칭이라
/// sonnet 단가로 대체됐는지"를 구분해서 보여줘야 하므로 미매칭은 "unknown"
/// 이라는 별도 family로 분리하고 `pricing_matched=false`를 반환한다.
fn model_bucket_key(model: &str) -> (&'static str, bool) {
    let m = model.to_lowercase();
    if m.contains("opus") {
        ("opus", true)
    } else if m.contains("haiku") {
        ("haiku", true)
    } else if m.contains("sonnet") {
        ("sonnet", true)
    } else {
        ("unknown", false)
    }
}

/// `daily::scan_file_daily_usage`와 같은 줄 필터·중복 제거 규칙(assistant
/// 타입, timestamp 컷오프, message.id 파일 내 dedup)을 쓰되, 모델/프로젝트/
/// 툴 사용까지 함께 집계한다. `project_key`는 호출자가 파일 경로에서 미리
/// 뽑아 넘긴다(이 파일 자체에는 프로젝트를 식별할 필드가 없다 — 디렉터리
/// 구조가 유일한 출처).
fn scan_file_for_summary(path: &Path, project_key: &str, cutoff_dt: DateTime<Utc>) -> PartialSummary {
    let mut summary = PartialSummary::default();
    let Ok(f) = std::fs::File::open(path) else {
        return summary;
    };
    let mut seen_message_ids: HashSet<String> = HashSet::new();
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

        if let Some(usage) = msg.get("usage") {
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

            let tokens = input + output + cache_creation + cache_read;
            let cost = cost_for_usage(model, input, output, cache_creation, cache_read);

            summary.input_tokens += input;
            summary.output_tokens += output;
            summary.cache_creation_tokens += cache_creation;
            summary.cache_read_tokens += cache_read;
            summary.cost_usd += cost;

            let (family, matched) = model_bucket_key(model);
            let model_entry = summary
                .model_tally
                .entry((family.to_string(), matched))
                .or_insert((0, 0.0));
            model_entry.0 += tokens;
            model_entry.1 += cost;

            let project_entry = summary
                .project_tally
                .entry(project_key.to_string())
                .or_insert((0, 0.0));
            project_entry.0 += tokens;
            project_entry.1 += cost;
        }

        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            for item in content {
                if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                    continue;
                }
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    *summary.tool_counts.entry(name.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    summary
}

/// `~/workspace`를 이 프로젝트의 `session_list::sanitize_cwd_for_project_dir`와
/// 같은 규칙('/'→'-')으로 치환한 접두사 + 구분자. 이 접두사로 시작하는
/// projectKey만 "관례를 따르는 프로젝트"로 보고 나머지를 사람이 읽을
/// 프로젝트명으로 취급한다(추측이 아니라 이 회사 프로젝트 표준 —
/// CLAUDE.md/`project-standards`가 명시하는 `~/workspace/<이름>/` 배치를
/// 그대로 반영한다).
fn home_workspace_prefix(home: &Path) -> String {
    let workspace = home.join("workspace");
    format!("{}-", workspace.display().to_string().replace('/', "-"))
}

/// `projectKey`를 사람이 읽을 이름으로 바꾼다. `~/workspace/<이름>` 관례를
/// 따르면 `<이름>`(원래 디렉터리명 그대로, 하이픈 포함 — 뒷부분은 경계를
/// 안다고 확신할 수 있는 구간이라 안전하게 복원된다)을 반환하고, 그 관례를
/// 벗어나면 추측하지 않고 선행 `-`만 뗀 원문을 반환한다(`sanitize_cwd_for_project_dir`가
/// `/`를 전부 `-`로 뭉개 버려 일반적으로는 원본 경로를 안전하게 복원할 수
/// 없다 — 이 관례 이탈 케이스에서 억지로 `/`로 되돌리면 틀린 이름을 보여줄
/// 위험이 있으므로 하지 않는다).
fn project_display_name(project_key: &str, workspace_prefix: &str) -> String {
    if let Some(rest) = project_key.strip_prefix(workspace_prefix) {
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
    project_key.trim_start_matches('-').to_string()
}

pub(crate) fn aggregate_usage_summary() -> UsageSummaryReport {
    let mut report = UsageSummaryReport {
        days: USAGE_LOOKBACK_DAYS,
        ..Default::default()
    };

    let Some(home) = dirs::home_dir() else {
        return report;
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return report;
    }

    let lookback = std::time::Duration::from_secs(USAGE_LOOKBACK_DAYS as u64 * 24 * 60 * 60);
    let cutoff_mtime = std::time::SystemTime::now()
        .checked_sub(lookback)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let cutoff_dt = Utc::now() - chrono::Duration::days(USAGE_LOOKBACK_DAYS);

    let mut files = Vec::new();
    daily::find_recent_jsonl_files(&projects_dir, cutoff_mtime, &mut files);

    use rayon::prelude::*;
    let combined = files
        .par_iter()
        .map(|file| {
            // projects_dir 바로 아래 첫 컴포넌트가 projectKey다(메인 세션
            // 파일과 그 서브에이전트 파일 둘 다 이 규칙을 따른다 —
            // `<projectKey>/<sessionId>.jsonl`, `<projectKey>/<sessionId>/subagents/*.jsonl`).
            let project_key = file
                .strip_prefix(&projects_dir)
                .ok()
                .and_then(|rel| rel.components().next())
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            scan_file_for_summary(file, &project_key, cutoff_dt)
        })
        .reduce(PartialSummary::default, merge_partial);

    report.input_tokens = combined.input_tokens;
    report.output_tokens = combined.output_tokens;
    report.cache_creation_tokens = combined.cache_creation_tokens;
    report.cache_read_tokens = combined.cache_read_tokens;
    report.total_tokens = combined.input_tokens
        + combined.output_tokens
        + combined.cache_creation_tokens
        + combined.cache_read_tokens;
    report.total_cost_usd = combined.cost_usd;

    let mut models: Vec<ModelUsageSummary> = combined
        .model_tally
        .into_iter()
        .map(|((family, matched), (tokens, cost))| ModelUsageSummary {
            family,
            tokens,
            cost_usd: cost,
            pricing_matched: matched,
        })
        .collect();
    models.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    report.models = models;

    let workspace_prefix = home_workspace_prefix(&home);
    let mut all_projects: Vec<ProjectUsageSummary> = combined
        .project_tally
        .into_iter()
        .map(|(project_key, (tokens, cost))| {
            let display_name = project_display_name(&project_key, &workspace_prefix);
            ProjectUsageSummary {
                project_key,
                display_name,
                tokens,
                cost_usd: cost,
            }
        })
        .collect();
    all_projects.sort_by(|a, b| b.tokens.cmp(&a.tokens));

    let split_at = all_projects.len().min(TOP_PROJECTS);
    let (top, rest) = all_projects.split_at(split_at);
    report.other_projects_tokens = rest.iter().map(|p| p.tokens).sum();
    report.other_projects_cost_usd = rest.iter().map(|p| p.cost_usd).sum();
    report.projects = top.to_vec();

    let mut tools: Vec<ToolUsageSummary> = combined
        .tool_counts
        .into_iter()
        .map(|(tool_name, count)| ToolUsageSummary { tool_name, count })
        .collect();
    tools.sort_by(|a, b| b.count.cmp(&a.count));
    tools.truncate(TOP_TOOLS);
    report.tools = tools;

    report
}

/// `project_key`가 `~/.claude/projects/` 바로 아래에 실제로 존재하는
/// 디렉터리 항목과 완전히 같을 때만 그 경로를 돌려준다. 임의 문자열을
/// `projects_dir.join(project_key)`로 직접 이어붙이지 않고 `read_dir()`가
/// 실제로 나열한 항목명과 등호 비교만 하므로, `project_key`에 `..`나 `/`가
/// 섞여 있어도 애초에 어떤 실제 항목명과도 같을 수 없어 후보에 오르지
/// 않는다(경로 트래버설이 구조적으로 불가능 — 프론트가 호출하는 커맨드라
/// 입력을 신뢰하지 않는다).
fn resolve_project_dir(projects_dir: &Path, project_key: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(projects_dir).ok()?;
    entries.flatten().find_map(|entry| {
        let name = entry.file_name().to_str()?.to_string();
        if name == project_key {
            Some(entry.path())
        } else {
            None
        }
    })
}

/// 선택한 프로젝트 하나의 30일 일별 추이. `daily::aggregate_daily_usage`가
/// 쓰는 것과 동일한 파일 선정·줄 필터·dedup 함수(`find_recent_jsonl_files`/
/// `scan_file_daily_usage`/`merge_daily_usage_maps`)를 그대로 재사용해
/// 프로젝트 범위로만 좁힌다 — 필터 규칙이 두 곳에서 따로 갈라질 일이 없다.
pub(crate) fn aggregate_project_daily_trend(project_key: &str) -> Vec<daily::DailyUsage> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let projects_dir = home.join(".claude").join("projects");
    let Some(project_dir) = resolve_project_dir(&projects_dir, project_key) else {
        return Vec::new();
    };

    let lookback = std::time::Duration::from_secs(USAGE_LOOKBACK_DAYS as u64 * 24 * 60 * 60);
    let cutoff_mtime = std::time::SystemTime::now()
        .checked_sub(lookback)
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let cutoff_dt = Utc::now() - chrono::Duration::days(USAGE_LOOKBACK_DAYS);

    let mut files = Vec::new();
    daily::find_recent_jsonl_files(&project_dir, cutoff_mtime, &mut files);

    use rayon::prelude::*;
    let buckets = files
        .par_iter()
        .map(|file| daily::scan_file_daily_usage(file, cutoff_dt, None))
        .reduce(BTreeMap::new, daily::merge_daily_usage_maps);

    buckets.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

    fn write_fixture(content: &str) -> PathBuf {
        let seq = TEST_FILE_SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "malgn-vscode-usage-summary-test-{}-{}.jsonl",
            std::process::id(),
            seq
        ));
        std::fs::write(&path, content).expect("fixture 파일 쓰기 실패");
        path
    }

    // 비용 기대값을 손으로 계산해 주석에 남긴다(pricing.rs 단가표 기준,
    // 1M 토큰당 USD):
    //   opus:   input 15.0 / output 75.0 / cache_write 18.75 / cache_read 1.5
    //   sonnet: input 3.0  / output 15.0 / cache_write 3.75  / cache_read 0.3
    //
    // opus 메시지 — input=100_000, output=50_000, cache_creation=20_000, cache_read=30_000
    //   = (100_000/1e6)*15.0 + (50_000/1e6)*75.0 + (20_000/1e6)*18.75 + (30_000/1e6)*1.5
    //   = 0.1*15.0=1.5 + 0.05*75.0=3.75 + 0.02*18.75=0.375 + 0.03*1.5=0.045
    //   = 5.67
    //
    // sonnet 메시지 — input=200_000, output=80_000, cache_creation=10_000, cache_read=500_000
    //   = (200_000/1e6)*3.0 + (80_000/1e6)*15.0 + (10_000/1e6)*3.75 + (500_000/1e6)*0.3
    //   = 0.2*3.0=0.6 + 0.08*15.0=1.2 + 0.01*3.75=0.0375 + 0.5*0.3=0.15
    //   = 1.9875
    //
    // 합계 = 5.67 + 1.9875 = 7.6575
    #[test]
    fn scans_file_and_computes_expected_cost_for_two_models() {
        let now = Utc::now();
        let ts1 = (now - chrono::Duration::days(1)).to_rfc3339();
        let ts2 = (now - chrono::Duration::days(2)).to_rfc3339();
        let content = format!(
            concat!(
                r#"{{"type":"assistant","timestamp":"{ts1}","message":{{"id":"msg_opus_1","model":"claude-opus-4-8","usage":{{"input_tokens":100000,"output_tokens":50000,"cache_creation_input_tokens":20000,"cache_read_input_tokens":30000}},"content":[{{"type":"tool_use","name":"Read"}}]}}}}"#,
                "\n",
                r#"{{"type":"assistant","timestamp":"{ts2}","message":{{"id":"msg_sonnet_1","model":"claude-sonnet-4-6","usage":{{"input_tokens":200000,"output_tokens":80000,"cache_creation_input_tokens":10000,"cache_read_input_tokens":500000}},"content":[{{"type":"tool_use","name":"Edit"}},{{"type":"tool_use","name":"Edit"}}]}}}}"#,
                "\n",
            ),
            ts1 = ts1,
            ts2 = ts2,
        );
        let path = write_fixture(&content);
        let cutoff_dt = now - chrono::Duration::days(30);
        let summary = scan_file_for_summary(&path, "test-project", cutoff_dt);
        let _ = std::fs::remove_file(&path);

        assert_eq!(summary.input_tokens, 100_000 + 200_000);
        assert_eq!(summary.output_tokens, 50_000 + 80_000);
        assert_eq!(summary.cache_creation_tokens, 20_000 + 10_000);
        assert_eq!(summary.cache_read_tokens, 30_000 + 500_000);
        assert!(
            (summary.cost_usd - 7.6575).abs() < 1e-9,
            "합산 비용이 손계산과 다릅니다: {}",
            summary.cost_usd
        );

        let opus_entry = summary
            .model_tally
            .get(&("opus".to_string(), true))
            .expect("opus 버킷이 없습니다");
        assert!((opus_entry.1 - 5.67).abs() < 1e-9, "opus 비용: {}", opus_entry.1);
        assert_eq!(opus_entry.0, 100_000 + 50_000 + 20_000 + 30_000);

        let sonnet_entry = summary
            .model_tally
            .get(&("sonnet".to_string(), true))
            .expect("sonnet 버킷이 없습니다");
        assert!(
            (sonnet_entry.1 - 1.9875).abs() < 1e-9,
            "sonnet 비용: {}",
            sonnet_entry.1
        );

        let project_entry = summary
            .project_tally
            .get("test-project")
            .expect("프로젝트 버킷이 없습니다");
        assert_eq!(project_entry.0, summary.input_tokens + summary.output_tokens + summary.cache_creation_tokens + summary.cache_read_tokens);
        assert!((project_entry.1 - summary.cost_usd).abs() < 1e-9);

        assert_eq!(summary.tool_counts.get("Read"), Some(&1));
        assert_eq!(summary.tool_counts.get("Edit"), Some(&2));
    }

    // 단가표에 없는(family 키워드 어디에도 안 걸리는) 모델명은 "unknown"으로
    // 따로 묶이고 pricing_matched=false여야 한다 — sonnet 실측치와 조용히
    // 섞이면 안 된다는 정직성 요구사항의 회귀 방지 테스트.
    #[test]
    fn unmatched_model_name_is_bucketed_separately_as_unknown() {
        let now = Utc::now();
        let ts = (now - chrono::Duration::days(1)).to_rfc3339();
        let content = format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"id":"msg_unknown_1","model":"some-unreleased-model","usage":{{"input_tokens":1000000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#,
        );
        let path = write_fixture(&content);
        let cutoff_dt = now - chrono::Duration::days(30);
        let summary = scan_file_for_summary(&path, "test-project", cutoff_dt);
        let _ = std::fs::remove_file(&path);

        assert!(
            summary.model_tally.contains_key(&("unknown".to_string(), false)),
            "미매칭 모델이 unknown/false 버킷에 없습니다: {:?}",
            summary.model_tally.keys().collect::<Vec<_>>()
        );
        assert!(
            !summary.model_tally.contains_key(&("sonnet".to_string(), true)),
            "미매칭 모델이 진짜 sonnet 버킷에 섞였습니다"
        );
        // 비용 자체은 pricing.rs 폴백대로 sonnet 단가로 계산된다(3.0/1M input).
        let (_, cost) = summary.model_tally[&("unknown".to_string(), false)];
        assert!((cost - 3.0).abs() < 1e-9, "미매칭 모델 비용: {cost}");
    }

    // 30일 컷오프보다 오래된 줄은 세지 않는다(daily.rs와 동일한 계약).
    #[test]
    fn drops_lines_older_than_cutoff() {
        let now = Utc::now();
        let old_ts = (now - chrono::Duration::days(40)).to_rfc3339();
        let content = format!(
            r#"{{"type":"assistant","timestamp":"{old_ts}","message":{{"id":"msg_old","model":"claude-sonnet-4-6","usage":{{"input_tokens":999,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#,
        );
        let path = write_fixture(&content);
        let cutoff_dt = now - chrono::Duration::days(30);
        let summary = scan_file_for_summary(&path, "test-project", cutoff_dt);
        let _ = std::fs::remove_file(&path);

        assert_eq!(summary.input_tokens, 0, "30일보다 오래된 줄이 집계됐습니다");
        assert!(summary.model_tally.is_empty());
    }

    // 같은 message.id가 반복돼도(스트리밍 청크) 한 번만 센다 — daily.rs/detail.rs와
    // 같은 계약.
    #[test]
    fn deduplicates_repeated_message_id() {
        let now = Utc::now();
        let ts = (now - chrono::Duration::days(1)).to_rfc3339();
        let content = format!(
            concat!(
                r#"{{"type":"assistant","timestamp":"{ts}","message":{{"id":"msg_dup","model":"claude-sonnet-4-6","usage":{{"input_tokens":100,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#,
                "\n",
                r#"{{"type":"assistant","timestamp":"{ts}","message":{{"id":"msg_dup","model":"claude-sonnet-4-6","usage":{{"input_tokens":100,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#,
                "\n",
            ),
            ts = ts,
        );
        let path = write_fixture(&content);
        let cutoff_dt = now - chrono::Duration::days(30);
        let summary = scan_file_for_summary(&path, "test-project", cutoff_dt);
        let _ = std::fs::remove_file(&path);

        assert_eq!(summary.input_tokens, 100, "중복 message.id가 두 번 세어졌습니다");
    }

    // 모델별 합산 + 프로젝트별 합산이 전체 합산과 일치해야 한다(요구사항의
    // "모델별·프로젝트별 합산과 전체 합산이 일치하는지" 테스트) — 두 파일을
    // 서로 다른 프로젝트키로 스캔해 병합한 뒤 검증한다.
    #[test]
    fn model_and_project_subtotals_match_grand_total() {
        let now = Utc::now();
        let ts_a = (now - chrono::Duration::days(1)).to_rfc3339();
        let ts_b = (now - chrono::Duration::days(2)).to_rfc3339();
        let content_a = format!(
            r#"{{"type":"assistant","timestamp":"{ts_a}","message":{{"id":"msg_a","model":"claude-opus-4-8","usage":{{"input_tokens":1000,"output_tokens":2000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#,
        );
        let content_b = format!(
            r#"{{"type":"assistant","timestamp":"{ts_b}","message":{{"id":"msg_b","model":"claude-haiku-4-5","usage":{{"input_tokens":3000,"output_tokens":4000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#,
        );
        let path_a = write_fixture(&content_a);
        let path_b = write_fixture(&content_b);
        let cutoff_dt = now - chrono::Duration::days(30);

        let summary_a = scan_file_for_summary(&path_a, "project-a", cutoff_dt);
        let summary_b = scan_file_for_summary(&path_b, "project-b", cutoff_dt);
        let _ = std::fs::remove_file(&path_a);
        let _ = std::fs::remove_file(&path_b);

        let combined = merge_partial(summary_a, summary_b);

        let grand_total_tokens = combined.input_tokens
            + combined.output_tokens
            + combined.cache_creation_tokens
            + combined.cache_read_tokens;
        let model_subtotal_tokens: u64 = combined.model_tally.values().map(|(t, _)| t).sum();
        let project_subtotal_tokens: u64 = combined.project_tally.values().map(|(t, _)| t).sum();
        assert_eq!(grand_total_tokens, model_subtotal_tokens);
        assert_eq!(grand_total_tokens, project_subtotal_tokens);

        let model_subtotal_cost: f64 = combined.model_tally.values().map(|(_, c)| c).sum();
        let project_subtotal_cost: f64 = combined.project_tally.values().map(|(_, c)| c).sum();
        assert!((combined.cost_usd - model_subtotal_cost).abs() < 1e-9);
        assert!((combined.cost_usd - project_subtotal_cost).abs() < 1e-9);
    }

    #[test]
    fn project_display_name_decodes_workspace_convention() {
        let prefix = "-Users-hopegiver-workspace-";
        assert_eq!(
            project_display_name("-Users-hopegiver-workspace-malgn-vscode", prefix),
            "malgn-vscode"
        );
        // 관례를 벗어나면 추측하지 않고 선행 '-'만 뗀 원문을 돌려준다.
        assert_eq!(
            project_display_name("-private-tmp-some-scratch-dir", prefix),
            "private-tmp-some-scratch-dir"
        );
    }

    #[test]
    fn resolve_project_dir_rejects_path_traversal_attempts() {
        let base = std::env::temp_dir().join(format!(
            "malgn-vscode-usage-summary-traversal-test-{}",
            std::process::id()
        ));
        let projects_dir = base.join("projects");
        std::fs::create_dir_all(projects_dir.join("real-project")).expect("fixture 디렉터리 생성 실패");

        assert!(resolve_project_dir(&projects_dir, "real-project").is_some());
        assert!(resolve_project_dir(&projects_dir, "../etc").is_none());
        assert!(resolve_project_dir(&projects_dir, "..").is_none());
        assert!(resolve_project_dir(&projects_dir, "nonexistent-project").is_none());

        let _ = std::fs::remove_dir_all(&base);
    }

    // 실제 로컬 데이터로 30일 전체 요약 합계가 `daily::aggregate_daily_usage()`의
    // 30일 합계와 정확히 일치하는지 확인한다(요구사항: "새 요약의 30일 토큰 합
    // = 기존 get_daily_usage 30일 합"). 머신 의존(실제 대화 로그 필요) — CI
    // 러너에는 없어 #[ignore].
    // 로컬 실행: cargo test -- --ignored usage_stats::summary::tests::summary_total_matches_daily_usage_total
    #[test]
    #[ignore]
    fn summary_total_matches_daily_usage_total() {
        let daily_total: u64 = daily::aggregate_daily_usage()
            .iter()
            .map(|d| {
                // DailyUsage 필드는 daily 모듈 밖에서 비공개라 직렬화를 거쳐
                // 합산한다 — 필드 접근자를 새로 추가하지 않고도 같은 값을
                // 비교할 수 있다.
                let v = serde_json::to_value(d).unwrap();
                v.get("inputTokens").and_then(|x| x.as_u64()).unwrap_or(0)
                    + v.get("outputTokens").and_then(|x| x.as_u64()).unwrap_or(0)
                    + v.get("cacheCreationTokens").and_then(|x| x.as_u64()).unwrap_or(0)
                    + v.get("cacheReadTokens").and_then(|x| x.as_u64()).unwrap_or(0)
            })
            .sum();

        let summary = aggregate_usage_summary();
        assert_eq!(
            summary.total_tokens, daily_total,
            "요약 30일 합계({})가 get_daily_usage 30일 합계({})와 다릅니다",
            summary.total_tokens, daily_total
        );
    }
}
