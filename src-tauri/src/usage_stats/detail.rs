// ---------------- 사용량 통계: 일별 상세 (특정 날짜 하루치 세션/에이전트/툴 랭킹) ----------------
// 세션·서브에이전트 조인 개념은 ~/workspace/malgnai/bin/sync-claude.js
// readSessionUsage()(308~437줄)를 따르되, agentId→agent_type 매핑 방식만 이
// 머신의 실제 온디스크 포맷에 맞게 바꿨다: 원본은 "부모의 Agent/Task tool_use
// input.prompt"와 "서브에이전트 첫 메시지 텍스트"를 문자열로 매칭해서 타입을
// 추론하지만(그 환경은 서브에이전트가 부모와 같은 파일에 isSidechain:true로 인라인
// 기록됨), 이 Claude Code 버전은 서브에이전트를 별도 파일
// `<sessionId>/subagents/agent-<agentId>.jsonl` + 같은 이름의 `.meta.json`으로
// 저장하고 그 meta.json에 `agentType` 필드를 이미 직접 담고 있다 — 그래서 프롬프트
// 텍스트 매칭보다 훨씬 신뢰할 수 있는 직접 필드 읽기로 대체했다(개념은 "무엇이
// 서브에이전트였고 어떤 타입이었는지 조인한다"로 동일, 구현 방법만 실제 데이터
// 구조에 맞춰 적응).
//
// "일별 사용량"(최근 30일 요약, 가벼움)에서 특정 날짜를 클릭했을 때만 호출되는
// 상세 API다 — 60일 전체를 매번 스캔하던 이전 "토큰 도둑" 방식과 달리 딱 하루만
// 본다: 파일 mtime이 요청받은 날짜의 로컬 자정보다 이전이면 열어보지도 않고
// 건너뛰고, 연 파일 안에서도 각 줄의 timestamp를 로컬 날짜로 바꿔 요청 날짜와
// 일치하는 줄만 집계한다.
//
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.

use super::daily::{local_date_key, local_midnight_as_system_time, parse_iso_timestamp};
use super::pricing::{cost_for_usage, split_cache_write_tokens};
use crate::session_list::{extract_text_from_content, truncate_title};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Default)]
struct UsageTally {
    turns: u32,
    tokens: u64,
    cost: f64,
    /// `pricing.rs` 단가표에 없는 모델의 토큰 합 — `summary.rs`의
    /// `unpricedTokens`와 같은 정직성 요구사항을 detail 화면에도 적용한다.
    unpriced_tokens: u64,
}

// 이 시점부터는 프로덕션 경로에서 직접 호출되지 않는다(`get_daily_detail`은
// 날짜 필터·tool_use 집계까지 겸하는 `scan_usage_lines_for_date`를 쓴다) —
// 다만 message.id 중복 제거 계약 자체를 고정하는 회귀 테스트가 이 함수를
// 직접 검증하므로 테스트 빌드에서만 컴파일한다.
#[cfg(test)]
fn scan_usage_lines(path: &Path, tally: &mut UsageTally) {
    let Ok(f) = std::fs::File::open(path) else {
        return;
    };
    // aggregate_daily_usage()와 같은 이유로 message.id 기준 중복 제거가 필요하다 —
    // 스트리밍 응답이 여러 줄로 쪼개져 기록되며 같은 턴의 usage가 반복된다.
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
        let Some(msg) = value.get("message") else {
            continue;
        };
        if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
            if !seen_message_ids.insert(id.to_string()) {
                continue;
            }
        }
        let model = msg
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        let Some(usage) = msg.get("usage") else {
            continue;
        };
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
        let (cache_write_5m, cache_write_1h) = split_cache_write_tokens(usage);

        let tokens = input + output + cache_creation + cache_read;
        let cost_result = cost_for_usage(model, input, output, cache_write_5m, cache_write_1h, cache_read);
        tally.turns += 1;
        tally.tokens += tokens;
        tally.cost += cost_result.cost_usd;
        if !cost_result.pricing_matched {
            tally.unpriced_tokens += tokens;
        }
    }
}

fn first_user_title_from_file(path: &Path) -> String {
    let Ok(f) = std::fs::File::open(path) else {
        return String::new();
    };
    for line in BufReader::new(f).lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = value.get("message").and_then(|m| m.get("content")) {
            if let Some(text) = extract_text_from_content(content) {
                return truncate_title(&text, 80);
            }
        }
        return String::new();
    }
    String::new()
}

#[derive(Serialize, Debug, Clone)]
struct ToolUsage {
    #[serde(rename = "toolName")]
    tool_name: String,
    count: u32,
}

#[derive(Serialize, Debug, Clone)]
struct AgentUsage {
    // 세션 본인의 턴은 "main", 서브에이전트는 meta.json의 agentType.
    #[serde(rename = "agentType")]
    agent_type: String,
    turns: u32,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
}

// `get_session_detail`(세션 상세 화면 "에이전트 실행 이력")이 날짜별 상세와
// 같은 모양을 돌려줘야 해서 `pub(crate)`로 연다 — 필드는 이 모듈 안에서만
// 채우므로(생성자가 여기 없다) 그대로 private 필드로 둔다.
#[derive(Serialize, Debug, Clone)]
pub(crate) struct SessionDetail {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "projectKey")]
    project_key: String,
    title: String,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
    agents: Vec<AgentUsage>,
    tools: Vec<ToolUsage>,
}

#[derive(Serialize, Debug, Clone, Default)]
pub(crate) struct DailyDetailReport {
    date: String,
    sessions: Vec<SessionDetail>,
    #[serde(rename = "totalTokens")]
    total_tokens: u64,
    #[serde(rename = "costUsd")]
    cost_usd: f64,
    /// 단가표(`pricing.rs`)에 없는 모델의 토큰 합 — `summary.rs`의
    /// `unpricedTokens`와 같은 이유(정직성: costUsd가 실제보다 낮게 잡혔을 수
    /// 있음을 프론트가 알 수 있게).
    #[serde(rename = "unpricedTokens")]
    unpriced_tokens: u64,
}

/// 상위 개수 제한 없이 툴 사용 랭킹에 올릴 최대 항목 수.
const DAILY_DETAIL_TOP_TOOLS: usize = 10;

/// `scan_usage_lines`와 같은 message.id 중복 제거를 적용하되, `date_filter`가
/// `Some(date)`이면 그 로컬 날짜와 일치하는 줄만, `None`이면 파일 전체(전
/// 기간)를 집계하고 tool_use 블록 사용 횟수도 함께 센다(같은 파일 안에서
/// 세션의 메인 트랜스크립트와 서브에이전트 트랜스크립트를 모두 이 함수로
/// 훑어 `tool_counts`에 합산한다). `scan_usage_lines_for_date`(날짜별 상세)와
/// `aggregate_session_detail`(세션별 상세, 날짜 필터 없음) 양쪽이 이 함수
/// 하나를 공유한다.
fn scan_usage_lines_matching(
    path: &Path,
    date_filter: Option<&str>,
    tally: &mut UsageTally,
    tool_counts: &mut HashMap<String, u32>,
) {
    let Ok(f) = std::fs::File::open(path) else {
        return;
    };
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
        if let Some(date) = date_filter {
            let Some(ts) = value
                .get("timestamp")
                .and_then(|t| t.as_str())
                .and_then(parse_iso_timestamp)
            else {
                continue;
            };
            if local_date_key(&ts) != date {
                continue;
            }
        }
        let Some(msg) = value.get("message") else {
            continue;
        };
        // 스트리밍 응답이 여러 줄로 쪼개져 usage/tool_use가 반복 기록되는 것을
        // 막는다 — id당 한 번만 usage와 tool_use를 센다.
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
            let (cache_write_5m, cache_write_1h) = split_cache_write_tokens(usage);

            let tokens = input + output + cache_creation + cache_read;
            let cost_result = cost_for_usage(model, input, output, cache_write_5m, cache_write_1h, cache_read);
            tally.turns += 1;
            tally.tokens += tokens;
            tally.cost += cost_result.cost_usd;
            if !cost_result.pricing_matched {
                tally.unpriced_tokens += tokens;
            }
        }

        if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
            for item in content {
                if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                    continue;
                }
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    *tool_counts.entry(name.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
}

/// `scan_usage_lines_matching`을 날짜 필터만 걸어 호출하는 얇은 래퍼 —
/// 실제 프로덕션 경로(`aggregate_daily_detail`)는 이제
/// `aggregate_session_agents_and_tools`를 거쳐 `scan_usage_lines_matching`을
/// 직접 쓰지만, "날짜 필터링 + message.id 중복 제거" 계약 자체를 고정하는
/// 기존 회귀 테스트가 이 이름·시그니처를 그대로 쓰므로(동작 변경 없음)
/// `scan_usage_lines`와 같은 이유로 테스트 빌드에서만 컴파일한다.
#[cfg(test)]
fn scan_usage_lines_for_date(
    path: &Path,
    date: &str,
    tally: &mut UsageTally,
    tool_counts: &mut HashMap<String, u32>,
) {
    scan_usage_lines_matching(path, Some(date), tally, tool_counts);
}

/// 세션 하나(메인 트랜스크립트 + `<sessionId>/subagents/*.jsonl`)를
/// agentType별 턴/토큰/비용, toolName별 횟수로 집계한다 —
/// `aggregate_daily_detail`(날짜 하루만, `date_filter: Some`)과
/// `aggregate_session_detail`(세션 전체 기간, `date_filter: None`) 양쪽이
/// 이 함수 하나를 공유한다(세션당 처리 로직을 복붙하지 않는다).
///
/// `should_scan_main`/`should_scan_subagent_file`은 날짜별 집계에서만 의미가
/// 있는 mtime 기반 성능 가드다(그 날짜보다 먼저 mtime이 끊긴 파일은 열어볼
/// 필요가 없다) — 세션 전체를 보는 쪽은 항상 `true`/`|_| true`를 넘겨 모든
/// 파일을 연다.
fn aggregate_session_agents_and_tools(
    session_path: &Path,
    project_path: &Path,
    session_id: &str,
    date_filter: Option<&str>,
    should_scan_main: bool,
    should_scan_subagent_file: &dyn Fn(&Path) -> bool,
) -> (Vec<AgentUsage>, Vec<ToolUsage>, u64) {
    let mut tool_counts: HashMap<String, u32> = HashMap::new();
    let mut agent_accum: BTreeMap<String, (u32, u64, f64)> = BTreeMap::new();
    let mut unpriced_tokens: u64 = 0;

    if should_scan_main {
        let mut main_tally = UsageTally::default();
        scan_usage_lines_matching(session_path, date_filter, &mut main_tally, &mut tool_counts);
        unpriced_tokens += main_tally.unpriced_tokens;
        if main_tally.turns > 0 {
            agent_accum.insert(
                "main".to_string(),
                (main_tally.turns, main_tally.tokens, main_tally.cost),
            );
        }
    }

    let subagents_dir = project_path.join(session_id).join("subagents");
    if subagents_dir.is_dir() {
        if let Ok(agent_files) = std::fs::read_dir(&subagents_dir) {
            for agent_entry in agent_files.flatten() {
                let agent_path = agent_entry.path();
                if agent_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                if !should_scan_subagent_file(&agent_path) {
                    continue;
                }
                let Some(agent_stem) = agent_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                else {
                    continue;
                };

                let meta_path = subagents_dir.join(format!("{agent_stem}.meta.json"));
                let agent_type = std::fs::read_to_string(&meta_path)
                    .ok()
                    .and_then(|c| serde_json::from_str::<Value>(&c).ok())
                    .and_then(|v| {
                        v.get("agentType")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                let mut agent_tally = UsageTally::default();
                scan_usage_lines_matching(&agent_path, date_filter, &mut agent_tally, &mut tool_counts);
                unpriced_tokens += agent_tally.unpriced_tokens;
                if agent_tally.turns == 0 {
                    continue;
                }

                let entry = agent_accum.entry(agent_type).or_insert((0, 0, 0.0));
                entry.0 += agent_tally.turns;
                entry.1 += agent_tally.tokens;
                entry.2 += agent_tally.cost;
            }
        }
    }

    let mut agents: Vec<AgentUsage> = agent_accum
        .into_iter()
        .map(|(agent_type, (turns, total_tokens, cost_usd))| AgentUsage {
            agent_type,
            turns,
            total_tokens,
            cost_usd,
        })
        .collect();
    agents.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    let mut tools: Vec<ToolUsage> = tool_counts
        .into_iter()
        .map(|(tool_name, count)| ToolUsage { tool_name, count })
        .collect();
    tools.sort_by(|a, b| b.count.cmp(&a.count));
    tools.truncate(DAILY_DETAIL_TOP_TOOLS);

    (agents, tools, unpriced_tokens)
}

pub(crate) fn aggregate_daily_detail(date: &str) -> DailyDetailReport {
    let mut report = DailyDetailReport {
        date: date.to_string(),
        ..Default::default()
    };

    let Some(home) = dirs::home_dir() else {
        return report;
    };
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return report;
    }
    let Some(cutoff_mtime) = local_midnight_as_system_time(date) else {
        return report;
    };

    let is_recent_enough = |path: &Path| -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|modified| modified >= cutoff_mtime)
            .unwrap_or(true)
    };

    let mut sessions: Vec<SessionDetail> = Vec::new();
    let mut report_unpriced_tokens: u64 = 0;

    let Ok(project_entries) = std::fs::read_dir(&projects_dir) else {
        return report;
    };
    for project_entry in project_entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let Some(project_key) = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
        else {
            continue;
        };

        let Ok(session_entries) = std::fs::read_dir(&project_path) else {
            continue;
        };
        for session_entry in session_entries.flatten() {
            let session_path = session_entry.path();
            // 메인 세션 트랜스크립트만 여기서 다룬다("<sessionId>.jsonl" 직속 파일).
            // 서브에이전트 트랜스크립트는 "<sessionId>/subagents/*.jsonl"에 따로 있다.
            if session_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(session_id) = session_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
            else {
                continue;
            };

            let (agents, tools, session_unpriced_tokens) = aggregate_session_agents_and_tools(
                &session_path,
                &project_path,
                &session_id,
                Some(date),
                is_recent_enough(&session_path),
                &is_recent_enough,
            );
            report_unpriced_tokens += session_unpriced_tokens;

            // 그날 활동(usage 라인)이 전혀 없는 세션은 결과에서 제외한다.
            if agents.is_empty() {
                continue;
            }

            let total_tokens: u64 = agents.iter().map(|a| a.total_tokens).sum();
            let cost_usd: f64 = agents.iter().map(|a| a.cost_usd).sum();
            let title = first_user_title_from_file(&session_path);

            sessions.push(SessionDetail {
                session_id,
                project_key: project_key.clone(),
                title: if title.is_empty() {
                    "(제목 없음)".to_string()
                } else {
                    title
                },
                total_tokens,
                cost_usd,
                agents,
                tools,
            });
        }
    }

    sessions.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    report.total_tokens = sessions.iter().map(|s| s.total_tokens).sum();
    report.cost_usd = sessions.iter().map(|s| s.cost_usd).sum();
    report.sessions = sessions;
    report.unpriced_tokens = report_unpriced_tokens;
    report
}

/// 날짜가 아니라 세션 하나만 골라 집계한다 — 세션 상세 화면의 "에이전트 실행
/// 이력" 섹션 전용. `aggregate_daily_detail`과 핵심 집계
/// (`aggregate_session_agents_and_tools`)을 공유하지만 날짜 필터를 걸지 않아
/// 그 세션의 전체 기간을 본다.
///
/// 세션은 각각 `~/.claude/projects/<projectKey>/<sessionId>.jsonl` 파일 하나와
/// 1:1로 대응한다(`session_list::jsonl_scan::build_jsonl_row`가 같은 가정을
/// 쓴다) — 그 경로를 어느 프로젝트 디렉터리에 있는지 모르니 찾을 때까지
/// `~/.claude/projects/*`를 순회한다. 찾지 못하면(세션이 아직 jsonl을 만들지
/// 않은 draft, 혹은 존재하지 않는 sessionId) `None`을 돌려줘 "자료 없음"과
/// "집계했지만 에이전트·툴 사용이 0건"을 프론트가 구분할 수 있게 한다
/// (`unpriced_tokens`가 지키는 것과 같은 "정직한 상태 표시" 원칙).
pub(crate) fn aggregate_session_detail(session_id: &str) -> Option<SessionDetail> {
    let home = dirs::home_dir()?;
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.is_dir() {
        return None;
    }

    let project_entries = std::fs::read_dir(&projects_dir).ok()?;
    for project_entry in project_entries.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let session_path = project_path.join(format!("{session_id}.jsonl"));
        if !session_path.is_file() {
            continue;
        }
        let Some(project_key) = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
        else {
            continue;
        };

        // 날짜 필터 없음(전체 기간) + mtime 가드 없음(항상 연다) — 날짜별
        // 집계와 달리 "이 세션 하나"만 보므로 성능 가드가 필요 없다.
        let (agents, tools, _unpriced_tokens) = aggregate_session_agents_and_tools(
            &session_path,
            &project_path,
            session_id,
            None,
            true,
            &|_| true,
        );

        let total_tokens: u64 = agents.iter().map(|a| a.total_tokens).sum();
        let cost_usd: f64 = agents.iter().map(|a| a.cost_usd).sum();
        let title = first_user_title_from_file(&session_path);

        return Some(SessionDetail {
            session_id: session_id.to_string(),
            project_key,
            title: if title.is_empty() {
                "(제목 없음)".to_string()
            } else {
                title
            },
            total_tokens,
            cost_usd,
            agents,
            tools,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    // 회귀 방지 — 실제로 겪은 버그: 스트리밍 응답이 여러 JSONL 줄로 쪼개져 기록되며
    // 같은 message.id가 반복 등장하고 매번 같은 usage를 다시 싣는다(실측: 한
    // 세션에서 고유 메시지 124개인데 usage가 실린 줄은 238개 — 거의 2배). 같은
    // id를 두 번 세면 토큰·비용이 부풀려진다. 여기서는 그 스트리밍 중복 패턴을
    // 합성 fixture로 재현해 scan_usage_lines()가 id당 한 번만 세는지 고정한다.
    #[test]
    fn deduplicates_repeated_message_id_when_scanning_usage() {
        let tmp = std::env::temp_dir().join(format!(
            "malgn-vscode-usage-dedup-test-{}.jsonl",
            std::process::id()
        ));
        let content = concat!(
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:00.000Z","message":{"id":"msg_dup1","model":"claude-sonnet-4-6","usage":{"input_tokens":2,"output_tokens":100,"cache_creation_input_tokens":0,"cache_read_input_tokens":10000}}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:01.000Z","message":{"id":"msg_dup1","model":"claude-sonnet-4-6","usage":{"input_tokens":2,"output_tokens":100,"cache_creation_input_tokens":0,"cache_read_input_tokens":10000}}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:02.000Z","message":{"id":"msg_unique2","model":"claude-sonnet-4-6","usage":{"input_tokens":1,"output_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":5000}}}"#,
            "\n",
        );
        std::fs::write(&tmp, content).expect("fixture 파일 쓰기 실패");

        let mut tally = UsageTally::default();
        scan_usage_lines(&tmp, &mut tally);
        let _ = std::fs::remove_file(&tmp);

        // msg_dup1은 한 번만, msg_unique2는 한 번 — 총 2턴이어야 한다(3이면 중복 제거 실패).
        assert_eq!(tally.turns, 2, "중복 message.id가 두 번 세어졌습니다");
        assert_eq!(
            tally.tokens,
            (2 + 100 + 10000) + (1 + 50 + 5000),
            "중복 제거 후 토큰 합계가 예상과 다릅니다"
        );
    }

    // detail.rs가 summary.rs와 같은 단가 로직(pricing.rs)을 쓰는지, 그리고
    // 단가표에 없는 모델의 토큰이 unpriced_tokens로 잡히는지 확인한다.
    // claude-opus-5 줄 — input=100_000, output=40_000, cache_creation=0, cache_read=0
    //   = 0.1*5.0 + 0.04*25.0 = 0.5 + 1.0 = 1.5
    // 미등록 모델 줄("no-such-model") — 토큰만 세고 비용은 0, unpriced_tokens에 반영.
    #[test]
    fn scan_usage_lines_for_date_uses_shared_pricing_and_flags_unpriced_tokens() {
        let tmp = std::env::temp_dir().join(format!(
            "malgn-vscode-usage-detail-pricing-test-{}.jsonl",
            std::process::id()
        ));
        let content = concat!(
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:00.000Z","message":{"id":"msg_opus","model":"claude-opus-5","usage":{"input_tokens":100000,"output_tokens":40000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            "\n",
            r#"{"type":"assistant","timestamp":"2026-09-08T10:00:01.000Z","message":{"id":"msg_unmatched","model":"no-such-model","usage":{"input_tokens":10000,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            "\n",
        );
        std::fs::write(&tmp, content).expect("fixture 파일 쓰기 실패");

        let mut tally = UsageTally::default();
        let mut tool_counts = std::collections::HashMap::new();
        scan_usage_lines_for_date(&tmp, "2026-09-08", &mut tally, &mut tool_counts);
        let _ = std::fs::remove_file(&tmp);

        assert_eq!(tally.turns, 2);
        assert_eq!(tally.tokens, 100_000 + 40_000 + 10_000);
        assert!(
            (tally.cost - 1.5).abs() < 1e-9,
            "opus-5 비용만 반영돼야 합니다(미등록 모델은 0): {}",
            tally.cost
        );
        assert_eq!(
            tally.unpriced_tokens, 10_000,
            "미등록 모델의 토큰이 unpriced_tokens에 잡혀야 합니다"
        );
    }

    // 이 세션 자체가 지금 malgn-vscode 프로젝트에서 오늘 날짜의 활동을 만들어내고
    // 있으니, 오늘 날짜로 상세 집계를 요청하면 이 프로젝트 세션이 0보다 큰 토큰으로
    // 잡혀야 한다 — "하루만 스캔"하는 실제 파일 I/O 경로가 정말 동작함을 증명한다.
    // 머신 의존(오늘 날짜 실사용 로그가 있어야 함) — CI 러너에는 없어 #[ignore].
    // 로컬 실행: cargo test -- --ignored usage_stats::detail::tests::aggregates_daily_detail_for_today_from_real_data
    #[test]
    #[ignore]
    fn aggregates_daily_detail_for_today_from_real_data() {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let report = aggregate_daily_detail(&today);
        assert_eq!(report.date, today);
        assert!(
            !report.sessions.is_empty(),
            "오늘 날짜 상세 집계가 비어 있습니다"
        );
        assert!(report.total_tokens > 0, "오늘 날짜 총 토큰이 0입니다");
        assert!(
            report
                .sessions
                .iter()
                .any(|s| s.project_key == "-Users-hopegiver-workspace-malgn-vscode"),
            "이 프로젝트(malgn-vscode) 세션이 오늘 상세 집계에 없습니다"
        );
        // 하루치만 봤으니 각 세션의 tools는 상위 10개를 넘지 않아야 한다.
        assert!(
            report.sessions.iter().all(|s| s.tools.len() <= 10),
            "tools 상위 개수 제한이 지켜지지 않았습니다"
        );
    }

    // `get_session_detail`(세션별 상세)이 `get_daily_detail`과 공유하는
    // `aggregate_session_agents_and_tools`가 메인 트랜스크립트 + 서브에이전트
    // 트랜스크립트를 날짜 필터 없이(전체 기간) 올바르게 합산하는지 확인한다.
    // 비용 계산은 `scan_usage_lines_for_date_uses_shared_pricing_and_flags_unpriced_tokens`
    // 와 같은 pricing.rs 단가표를 쓴다(claude-sonnet-4-6: input 3.0 / output 15.0,
    // 1M 토큰당 USD) —
    //   main:   input=1_000, output=1_000 → tokens=2_000, cost=(1_000/1e6)*3.0+(1_000/1e6)*15.0=0.018
    //   backend-dev(서브): input=500, output=500 → tokens=1_000, cost=(500/1e6)*3.0+(500/1e6)*15.0=0.009
    #[test]
    fn aggregate_session_agents_and_tools_combines_main_and_subagent_across_all_time() {
        let base = std::env::temp_dir().join(format!(
            "malgn-vscode-session-detail-test-{}",
            std::process::id()
        ));
        let session_id = "sess-agent-history-test";
        let subagents_dir = base.join(session_id).join("subagents");
        std::fs::create_dir_all(&subagents_dir).expect("fixture 디렉터리 생성 실패");

        let session_path = base.join(format!("{session_id}.jsonl"));
        std::fs::write(
            &session_path,
            concat!(
                r#"{"type":"assistant","timestamp":"2026-09-01T10:00:00.000Z","message":{"id":"msg_main_1","model":"claude-sonnet-4-6","usage":{"input_tokens":1000,"output_tokens":1000,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"tool_use","name":"Read"}]}}"#,
                "\n",
            ),
        )
        .expect("main fixture 쓰기 실패");

        let agent_path = subagents_dir.join("agent-x.jsonl");
        std::fs::write(
            &agent_path,
            concat!(
                r#"{"type":"assistant","timestamp":"2026-09-01T10:05:00.000Z","message":{"id":"msg_sub_1","model":"claude-sonnet-4-6","usage":{"input_tokens":500,"output_tokens":500,"cache_creation_input_tokens":0,"cache_read_input_tokens":0},"content":[{"type":"tool_use","name":"Edit"},{"type":"tool_use","name":"Edit"}]}}"#,
                "\n",
            ),
        )
        .expect("subagent fixture 쓰기 실패");
        std::fs::write(
            subagents_dir.join("agent-x.meta.json"),
            r#"{"agentType":"backend-dev"}"#,
        )
        .expect("meta fixture 쓰기 실패");

        let (agents, tools, unpriced_tokens) = aggregate_session_agents_and_tools(
            &session_path,
            &base,
            session_id,
            None,
            true,
            &|_| true,
        );
        let _ = std::fs::remove_dir_all(&base);

        assert_eq!(unpriced_tokens, 0, "sonnet-4-6은 단가표에 있어야 합니다");

        assert_eq!(agents.len(), 2, "main + backend-dev 두 에이전트가 집계돼야 합니다");
        // total_tokens desc 정렬 — main(2000)이 backend-dev(1000)보다 앞선다.
        assert_eq!(agents[0].agent_type, "main");
        assert_eq!(agents[0].turns, 1);
        assert_eq!(agents[0].total_tokens, 2_000);
        assert!((agents[0].cost_usd - 0.018).abs() < 1e-9, "main 비용: {}", agents[0].cost_usd);
        assert_eq!(agents[1].agent_type, "backend-dev");
        assert_eq!(agents[1].turns, 1);
        assert_eq!(agents[1].total_tokens, 1_000);
        assert!(
            (agents[1].cost_usd - 0.009).abs() < 1e-9,
            "backend-dev 비용: {}",
            agents[1].cost_usd
        );

        // count desc 정렬 — Edit(2)이 Read(1)보다 앞선다.
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].tool_name, "Edit");
        assert_eq!(tools[0].count, 2);
        assert_eq!(tools[1].tool_name, "Read");
        assert_eq!(tools[1].count, 1);
    }

    // `get_session_detail`의 "자료 없음" 계약 — 어느 프로젝트 디렉터리에도
    // `<sessionId>.jsonl`이 없으면(이 테스트의 무작위 UUID는 실제로 존재할 수
    // 없다) 에러가 아니라 `None`이어야 한다(에러와 빈 데이터를 섞지 않는다).
    #[test]
    fn aggregate_session_detail_returns_none_when_session_file_not_found() {
        let result = aggregate_session_detail("00000000-nonexistent-session-id-fixture");
        assert!(result.is_none(), "존재하지 않는 세션은 None이어야 합니다");
    }

    // 이 머신의 실제 ~/.claude/projects 데이터로 세션 하나를 골라
    // `aggregate_session_detail`이 실제 파일 I/O 경로에서도 말이 되는 결과를
    // 내는지 확인한다(토큰 합이 agents 합과 일치, 비용이 음수가 아님, tools
    // 상위 개수 제한). 머신 의존 — CI 러너에는 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored usage_stats::detail::tests::aggregate_session_detail_matches_real_data_sanity
    #[test]
    #[ignore]
    fn aggregate_session_detail_matches_real_data_sanity() {
        let home = dirs::home_dir().expect("home dir를 찾을 수 없습니다");
        let projects_dir = home.join(".claude").join("projects");
        let project_entries = std::fs::read_dir(&projects_dir).expect("projects 디렉터리가 없습니다");

        let mut found_session_id: Option<String> = None;
        for project_entry in project_entries.flatten() {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            let Ok(file_entries) = std::fs::read_dir(&project_path) else {
                continue;
            };
            for file_entry in file_entries.flatten() {
                let path = file_entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        found_session_id = Some(stem.to_string());
                        break;
                    }
                }
            }
            if found_session_id.is_some() {
                break;
            }
        }

        let session_id = found_session_id.expect("세션 jsonl을 하나도 찾지 못했습니다");
        let detail = aggregate_session_detail(&session_id)
            .expect("방금 찾은 세션인데 None이 돌아왔습니다");

        assert_eq!(detail.session_id, session_id);
        let agents_total: u64 = detail.agents.iter().map(|a| a.total_tokens).sum();
        assert_eq!(
            detail.total_tokens, agents_total,
            "totalTokens가 agents 합과 일치해야 합니다"
        );
        assert!(detail.cost_usd >= 0.0, "비용이 음수일 수 없습니다: {}", detail.cost_usd);
        assert!(
            detail.tools.len() <= 10,
            "tools 상위 개수 제한이 지켜지지 않았습니다"
        );
    }
}
