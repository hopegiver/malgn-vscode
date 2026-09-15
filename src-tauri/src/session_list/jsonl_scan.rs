use super::find_session_title;
use super::path_filter::{
    allowed_project_dir_prefixes, allowed_workspace_roots, is_allowed_project_dir,
    is_cwd_within_allowed_workspace,
};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// mtime 30일 컷 + 개수 상한. 순서(전량 stat → mtime 내림차순 → 상위 100 →
/// head 파싱)를 지키기 위해 상한값도 스캔 함수와 같은 곳에 둔다.
///
/// m6: 이름은 "30일 컷 + 100건 상한"이지만, 개수 상한(100)이 mtime 내림차순
/// 정렬 후 먼저 잘라내므로 실제 체감 시간창은 30일보다 훨씬 짧다 — 이 머신
/// 실측으로는 약 10.6일(활동량에 따라 다른 머신에서는 달라질 수 있다). "30일
/// 이내 전부"를 보장하는 값이 아니라 "최근 활동이 많으면 100건 상한이 먼저
/// 걸린다"는 두 캡의 조합으로 이해해야 한다.
const JSONL_SCAN_MAX_AGE_DAYS: u64 = 30;
const JSONL_SCAN_MAX_ROWS: usize = 100;

/// S6과 동일한 순회 규약(`session_chat::transcript::resolve_transcript_path`
/// 참조): `~/.claude/projects/*/` **1단계 자식 디렉터리에서만** `*.jsonl`
/// 파일을 본다. 재귀하지 않으므로 `<sid>/subagents/agent-*.jsonl`(서브에이전트
/// 트랜스크립트, depth 2 이상)은 애초에 `read_dir` 대상 자체가 되지 않는다.
fn scan_jsonl_candidate_paths() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let projects_dir = home.join(".claude").join("projects");
    let Ok(project_dir_entries) = std::fs::read_dir(&projects_dir) else {
        return Vec::new();
    };

    let allowed_prefixes = allowed_project_dir_prefixes();
    let mut candidates = Vec::new();
    for project_entry in project_dir_entries.flatten() {
        let project_dir = project_entry.path();
        if !project_dir.is_dir() {
            continue;
        }
        let Some(dir_name) = project_dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_allowed_project_dir(dir_name, &allowed_prefixes) {
            continue;
        }

        let Ok(file_entries) = std::fs::read_dir(&project_dir) else {
            continue;
        };
        for file_entry in file_entries.flatten() {
            let path = file_entry.path();
            if !path.is_file() {
                continue; // 서브에이전트 폴더 등 하위 디렉터리는 여기서 자연히 제외된다.
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            candidates.push(path);
        }
    }
    candidates
}

/// `workspace::mtime_millis`와 동일한 로직(모듈이 달라 여기서는 그대로 복제한다
/// — 10줄 이내 순수 유틸을 위해 모듈 간 결합을 늘리지 않는다).
fn jsonl_mtime_millis(path: &Path) -> Option<i64> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

/// 상한 적용: 전량 `stat` → mtime 30일 컷 → mtime 내림차순 정렬 → 상위 100건.
/// `head` 파싱(cwd/title/startedAt/version)은 이 함수가 돌려준 결과에 대해서만
/// 호출자가 수행한다 — 캡 밖으로 밀려날 파일까지 미리 열어보지 않는다.
fn cap_recent_jsonl_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(60 * 60 * 24 * JSONL_SCAN_MAX_AGE_DAYS))
        .unwrap_or(std::time::UNIX_EPOCH);

    let mut stated: Vec<(PathBuf, std::time::SystemTime)> = paths
        .into_iter()
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
            if modified < cutoff {
                return None;
            }
            Some((path, modified))
        })
        .collect();

    stated.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    stated.truncate(JSONL_SCAN_MAX_ROWS);
    stated.into_iter().map(|(path, _)| path).collect()
}

/// m5: "head 스캔"이라는 이름과 달리 이전에는 둘 다 못 찾으면(예: timestamp/
/// version 필드가 끝까지 없는 파일) 실질적으로 파일 전체를 읽을 수 있었다 —
/// 진짜 head(앞부분)만 보도록 줄 수 상한을 둔다.
const JSONL_HEAD_SCAN_MAX_LINES: usize = 200;

/// startedAt/version을 위한 head 스캔. 첫 `timestamp`(파싱 가능한 것)와 첫
/// `version` 필드를 각각 찾는 즉시 기록하고, 둘 다 찾으면 더 읽지 않고
/// 멈춘다(`find_session_title`/`read_cwd_from_transcript`와 같은 "찾는 즉시
/// 중단" 관용구). `timestamp`는 `usage_stats::parse_iso_timestamp`(RFC3339)를
/// 재사용해 epoch ms로 바꾼다 — 새 파서를 만들지 않는다. 둘 다 못 찾아도
/// `JSONL_HEAD_SCAN_MAX_LINES`줄을 넘기면 멈춘다(m5: 진짜 "head"만 본다).
fn read_jsonl_head_meta(path: &Path) -> (Option<i64>, Option<String>) {
    let Ok(file) = std::fs::File::open(path) else {
        return (None, None);
    };
    let reader = BufReader::new(file);

    let mut started_at: Option<i64> = None;
    let mut version: Option<String> = None;

    for (line_no, line) in reader.lines().enumerate() {
        if line_no >= JSONL_HEAD_SCAN_MAX_LINES {
            break;
        }
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if started_at.is_none() {
            if let Some(ts) = value.get("timestamp").and_then(|v| v.as_str()) {
                if let Some(dt) = crate::usage_stats::parse_iso_timestamp(ts) {
                    started_at = Some(dt.timestamp_millis());
                }
            }
        }
        if version.is_none() {
            if let Some(v) = value.get("version").and_then(|v| v.as_str()) {
                version = Some(v.to_string());
            }
        }
        if started_at.is_some() && version.is_some() {
            break;
        }
    }
    (started_at, version)
}

/// depth-1 jsonl 파일 하나를 목록 행 하나로 바꾼다. `cwd`를 못 구하면(파일이
/// 깨졌거나 cwd 필드가 끝까지 없음) 이 행은 만들 수 없으므로 `None` — 호출자가
/// 그 파일만 건너뛰고 전체 스캔은 계속한다. M1: 실제 `cwd`가
/// `is_cwd_within_allowed_workspace`(권위 검사, `allowed_roots` 기준)를
/// 통과하지 못하면(디렉터리명 프리필터는 통과했지만 실제로는 다른 프로젝트인
/// 경우, 예: `foo` 허용인데 `foo-bar` 유래) 마찬가지로 `None`을 반환해 이 행을
/// 만들지 않는다.
///
/// `cwd`는 `session_chat::read_cwd_from_transcript`(재노출), `title`은 이 파일의
/// `find_session_title`을 그대로 재사용한다 — 새 스캔 로직을 발명하지 않는다.
fn build_jsonl_row(path: &Path, allowed_roots: &[PathBuf]) -> Option<Value> {
    let session_id = path.file_stem().and_then(|s| s.to_str())?.to_string();
    let cwd = crate::session_chat::read_cwd_from_transcript(path).ok()?;
    if !is_cwd_within_allowed_workspace(&cwd, allowed_roots) {
        return None;
    }
    let (started_at, version) = read_jsonl_head_meta(path);
    let updated_at = jsonl_mtime_millis(path).unwrap_or(0);
    let title = find_session_title(&serde_json::json!({
        "sessionId": session_id,
        "cwd": cwd,
    }))
    .unwrap_or_default();

    // m8: `startedAt`을 못 구했을 때 `0`으로 채우면 프론트가 이를 epoch 0으로
    // 렌더해 "1970-01-01"이 보인다 — `Value::Null`로 남겨 프론트의 폴백 표시
    // 로직이 정상 동작하게 한다. `started_at`이 `Option<i64>`라 `json!` 매크로가
    // `None`을 자동으로 `null`로 직렬화한다.
    let mut row = serde_json::json!({
        "sessionId": session_id,
        "cwd": cwd,
        "title": title,
        "startedAt": started_at,
        "updatedAt": updated_at,
    });
    if let Some(version) = version {
        if let Value::Object(map) = &mut row {
            map.insert("version".to_string(), Value::String(version));
        }
    }
    Some(row)
}

/// jsonl 기반 목록 행 전체를 만든다: 후보 수집(depth-1 + 안전장치 2개 (a)단계)
/// → 상한 적용(mtime 30일 컷 + 100건) → 상한을 통과한 파일만 head 파싱 + 안전
/// 장치 ② (b)단계(cwd 권위 검사).
pub(super) fn build_jsonl_rows() -> Vec<Value> {
    let candidates = scan_jsonl_candidate_paths();
    let capped = cap_recent_jsonl_paths(candidates);
    let allowed_roots = allowed_workspace_roots();
    capped
        .iter()
        .filter_map(|path| build_jsonl_row(path, &allowed_roots))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // depth-1만 스캔한다: 이 머신의 실제 ~/.claude/projects에는 서브에이전트
    // 트랜스크립트(`<sid>/subagents/agent-*.jsonl`, depth 2 이상)가 실제로
    // 존재하는데(실측: 1,000건 이상), 후보 목록에 하나도 섞여 있으면 안 된다.
    #[test]
    fn scan_candidates_exclude_subagent_transcripts_depth1_only() {
        let candidates = scan_jsonl_candidate_paths();
        assert!(
            !candidates.is_empty(),
            "이 머신에서 depth-1 jsonl 후보를 하나도 찾지 못했습니다"
        );
        assert!(
            candidates
                .iter()
                .all(|p| !p.to_string_lossy().contains("/subagents/")),
            "서브에이전트 트랜스크립트 경로가 depth-1 스캔 결과에 섞여 있습니다"
        );
    }

    // 개수 상한 100건: 150개 후보를 넣으면 정확히 100개로 잘려야 한다.
    #[test]
    fn caps_candidate_paths_at_100() {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-session-list-cap-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("임시 디렉터리 생성 실패");

        let mut paths = Vec::new();
        for i in 0..150 {
            let path = dir.join(format!("{i}.jsonl"));
            std::fs::write(&path, "{}").expect("임시 파일 쓰기 실패");
            paths.push(path);
        }

        let capped = cap_recent_jsonl_paths(paths);
        assert_eq!(capped.len(), JSONL_SCAN_MAX_ROWS, "100건 상한이 걸려야 합니다");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
