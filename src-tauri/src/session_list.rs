// ---------------- 세션 목록/제목 ----------------
// `~/.claude/sessions/*.json` 메타데이터를 읽어 목록 커맨드로 반환한다. 원래
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
// `extract_text_from_content`/`truncate_title`는 `usage_stats::detail`의
// `first_user_title_from_file`도 재사용한다(그래서 `pub(crate)`).

use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// 세션 JSON의 `cwd`를 `~/.claude/projects/<이 값>/` 디렉터리명으로 바꾼다 — 이
/// 프로젝트가 실제로 쓰는 규칙(`/`를 전부 `-`로 치환)을 그대로 따른다. 슬래시를
/// 전부 지우기 때문에 결과에는 경로 구분자가 하나도 남지 않는다 — `cwd`에 `..`가
/// 있어도 항상 하나의 평평한(flat) 디렉터리명 문자열이 될 뿐 상위 디렉터리로
/// 빠져나갈 수 없다(경로 트래버설이 구조적으로 불가능하다).
fn sanitize_cwd_for_project_dir(cwd: &str) -> String {
    cwd.replace('/', "-")
}

pub(crate) fn truncate_title(text: &str, max_chars: usize) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.chars().count() <= max_chars {
        cleaned.to_string()
    } else {
        let truncated: String = cleaned.chars().take(max_chars).collect();
        format!("{}…", truncated.trim_end())
    }
}

/// `message.content`에서 제목으로 쓸 텍스트를 뽑는다. 보통 문자열이지만 배열(툴
/// 결과 등 콘텐츠 블록)일 수도 있어 첫 텍스트 블록을 찾는다.
pub(crate) fn extract_text_from_content(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(items) => items.iter().find_map(|item| {
            let text = item.get("text").and_then(|t| t.as_str())?.trim();
            if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            }
        }),
        _ => None,
    }
}

/// 세션의 `sessionId`/`cwd`로 대응하는 대화 로그(jsonl)를 찾아 첫 `"type":"user"`
/// 줄의 메시지 텍스트를 제목으로 뽑는다. 파일이 없거나, user 줄을 못 찾거나,
/// 텍스트를 못 뽑으면 `None` — 호출자가 세션 JSON의 `name` 필드로 폴백한다.
///
/// 파일 전체를 읽지 않는다 — `BufReader`로 줄 단위로 읽다가 첫 `"type":"user"` 줄을
/// 찾는 즉시 멈춘다(대화 로그는 최대 수 MB라 전체 로드를 피한다).
fn find_session_title(session: &Value) -> Option<String> {
    let session_id = session.get("sessionId").and_then(|v| v.as_str())?;
    let cwd = session.get("cwd").and_then(|v| v.as_str())?;
    let home = dirs::home_dir()?;

    let sanitized = sanitize_cwd_for_project_dir(cwd);
    let jsonl_path = home
        .join(".claude")
        .join("projects")
        .join(&sanitized)
        .join(format!("{session_id}.jsonl"));

    let file = std::fs::File::open(&jsonl_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
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

        // 첫 "user" 타입 줄을 찾았다 — 텍스트 추출 성공 여부와 무관하게 여기서 멈춘다.
        return value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(extract_text_from_content)
            .map(|t| truncate_title(&t, 46));
    }
    None
}

/// registry에서 읽은 원본 세션 목록을 sessionId 단위로 정리하는 순수 함수
/// (fs 접근 없음 — 유닛 테스트 대상). **아래 3단계 적용 순서를 반드시 지켜야
/// 한다**(실측으로 증명됨 — 반대 순서면 회귀가 난다):
///
/// 1. `exclude_pids`(앱이 스스로 띄운 자식 `claude -p`의 pid, 즉
///    `session_chat::active_turn_pids()`) 제외.
/// 2. 죽은 pid 필터(`process_util::pid_alive`).
/// 3. 같은 `sessionId`는 `startedAt` 최신 1건만 남긴다(dedup).
///
/// 순서가 중요한 이유: 앱이 `send_session_message`로 띄운 자식은 registry에
/// 자기 pid로 항목을 하나 더 만들고, 그 항목의 `startedAt`은 (턴을 시작한
/// 시점이라) 원본 IDE 세션 항목보다 항상 더 최신이다. 1번(자식 제외)보다 3번
/// (dedup)을 먼저 하면 "최신 1건만 남긴다"는 규칙이 원본이 아니라 우리 자식을
/// 선택해버리는 역전이 생긴다 — 1번을 먼저 해서 자식을 아예 후보에서 빼야 이
/// 역전이 원천 차단된다.
fn filter_and_dedup_sessions(
    sessions: Vec<Value>,
    exclude_pids: &std::collections::HashSet<u32>,
) -> Vec<Value> {
    // 1) ACTIVE_TURNS(우리 자식) 제외. pid 필드가 없는 항목은 판단 불가이므로
    //    보수적으로 통과시킨다(원래도 없는 형태였으면 걸러낼 근거가 없다).
    let step1: Vec<Value> = sessions
        .into_iter()
        .filter(|s| match s.get("pid").and_then(|v| v.as_u64()) {
            Some(pid) => !exclude_pids.contains(&(pid as u32)),
            None => true,
        })
        .collect();

    // 2) 죽은 pid 필터.
    let step2: Vec<Value> = step1
        .into_iter()
        .filter(|s| match s.get("pid").and_then(|v| v.as_u64()) {
            Some(pid) => crate::process_util::pid_alive(pid as u32),
            None => true,
        })
        .collect();

    // 3) sessionId dedup: startedAt 최신 1건만. sessionId가 없는 항목은
    //    dedup 키가 없으므로 그대로 통과시킨다(고유 취급).
    let mut by_session_id: std::collections::HashMap<String, Value> =
        std::collections::HashMap::new();
    let mut no_session_id: Vec<Value> = Vec::new();
    for value in step2 {
        let Some(session_id) = value.get("sessionId").and_then(|v| v.as_str()) else {
            no_session_id.push(value);
            continue;
        };
        let started_at = value.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
        match by_session_id.get(session_id) {
            Some(existing) => {
                let existing_started_at =
                    existing.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
                if started_at > existing_started_at {
                    by_session_id.insert(session_id.to_string(), value);
                }
            }
            None => {
                by_session_id.insert(session_id.to_string(), value);
            }
        }
    }
    let mut result: Vec<Value> = by_session_id.into_values().collect();
    result.extend(no_session_id);
    result
}

/// `~/.claude/sessions/*.json` 메타데이터를 원본 그대로 읽는다(정리 전). 파일
/// 하나가 없거나 깨져 있어도(JSON 파싱 실패) 그 항목만 건너뛰고 전체 목록은
/// 계속 만든다 — 세션 메타데이터는 외부 프로세스가 계속 쓰고 있을 수 있는 값이라
/// 언제든 깨진 상태로 읽힐 수 있다고 가정한다.
fn read_raw_registry_sessions() -> Vec<Value> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let dir = home.join(".claude").join("sessions");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        sessions.push(value);
    }
    sessions
}

// ==================== jsonl 기반 목록 행 (architect C안) ====================
//
// 목록 행의 소스를 registry(`~/.claude/sessions/`) 단독에서 jsonl
// (`~/.claude/projects/**/<sessionId>.jsonl`)로 옮긴다. registry는 "지금 실행
// 중"이라는 사실 하나만 얹는 live 오버레이로 강등한다(아래 `overlay_running_state`).
//
// 안전장치 ①: `/private/tmp` 유래 프로젝트 디렉터리(`-private-tmp-...`,
// `-tmp-...`, 대소문자 무관) 제외.
// 안전장치 ②: `crate::scan_workspace_projects()` 결과에 있는 프로젝트로
// 한정. 2단계로 나뉜다 — (a) `dir_name` 문자열 접두사로 거르는 싼 프리필터
// (`is_allowed_project_dir`, 아래), (b) 프리필터를 통과한 파일에 한해 실제
// `cwd`를 경로 컴포넌트 단위로 대조하는 권위 검사(`is_cwd_within_allowed_workspace`).
// (a)만으로는 `sanitize_cwd_for_project_dir`가 `/`를 `-`로 뭉개 정보를 버리는
// 탓에 `foo`가 허용이면 `foo-bar`(별개 디렉터리)까지 문자열 접두사로 통과시키는
// 결함이 있다 — 그래서 (b)가 최종 권위를 가진다.
// registry 오버레이 경로(`read_claude_sessions`의 live_registry)도 이제 (b)를
// 통과해야만 목록에 노출된다(m3: 이전에는 registry 유래 폴백 행이 ①②를 전혀
// 거치지 않아 워크스페이스 밖 cwd가 새어나갈 수 있었다 — 게이트를 추가해
// 닫았다).
// 셋 다 사람 승인 조건이라 이 함수들에서 빠지면 안 된다.

/// 안전장치 ①의 판정 근거: 이 프로젝트의 디렉터리명 규칙(`/`→`-`)에서
/// `/private/tmp/...` 경로는 반드시 이 접두사로 시작한다(실측 확인 — S6 근처
/// 실측과 동일한 방식). 대소문자 구분 없이(`eq_ignore_ascii_case`) 비교한다
/// (m4: macOS 파일시스템은 기본적으로 대소문자를 구분하지 않으므로 `/Private/Tmp`
/// 등도 걸러야 한다).
const PRIVATE_TMP_PROJECT_DIR_PREFIX: &str = "-private-tmp-";

/// m4: `/tmp/...`(중간에 `private`을 거치지 않는 경로)도 `-tmp-...`로
/// sanitize되므로 별도 접두사로 제외한다.
const BARE_TMP_PROJECT_DIR_PREFIX: &str = "-tmp-";

/// `dir_name`이 `prefix`로 시작하는지 대소문자 구분 없이 판정한다. 문자열
/// 슬라이싱이 UTF-8 문자 경계를 벗어나 패닉하지 않도록 `get(..)`으로 안전하게
/// 접근한다.
fn starts_with_ignore_case(dir_name: &str, prefix: &str) -> bool {
    dir_name
        .get(..prefix.len())
        .map(|head| head.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

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

/// 안전장치 ②의 (a)단계(프리필터): `scan_workspace_projects()`가 찾은 각
/// 프로젝트 경로를 이 파일이 이미 쓰는 `sanitize_cwd_for_project_dir` 규칙으로
/// 치환해 접두사 목록을 만든다. `crate::scan_workspace_projects()`를 그대로
/// 재사용한다(복붙하지 않는다) — `WorkspaceProject::path`는 이미 `pub(crate)`로
/// 열려 있다. 최종 권위는 `is_cwd_within_allowed_workspace`(실제 `cwd` 컴포넌트
/// 대조)에 있다.
fn allowed_project_dir_prefixes() -> Vec<String> {
    crate::scan_workspace_projects()
        .iter()
        .map(|p| sanitize_cwd_for_project_dir(&p.path))
        .collect()
}

/// `dir_name`(예: `-Users-hopegiver-workspace-malgn-vscode-src-tauri-src`)이
/// 안전장치 ①과 안전장치 ②의 (a)단계(싼 프리필터)를 통과하는지 판정하는 순수
/// 함수(fs 접근 없음 — 유닛 테스트 대상). `cwd`가 프로젝트 루트 자신이면
/// 접두사와 완전히 같고, 프로젝트 내부 하위 디렉터리면 접두사 뒤에 `-`가
/// 이어진다.
///
/// **이 함수만으로는 최종 판정이 아니다.** 문자열 접두사 검사라 `foo`가
/// 허용이면 `foo-bar`(별개 디렉터리)까지 통과시킨다 — 상한을 통과한 파일에
/// 한해서만 `is_cwd_within_allowed_workspace`(실제 `cwd`를 경로 컴포넌트
/// 단위로 대조하는 권위 검사)가 최종 판정을 내린다.
fn is_allowed_project_dir(dir_name: &str, allowed_prefixes: &[String]) -> bool {
    if starts_with_ignore_case(dir_name, PRIVATE_TMP_PROJECT_DIR_PREFIX)
        || starts_with_ignore_case(dir_name, BARE_TMP_PROJECT_DIR_PREFIX)
    {
        return false;
    }
    allowed_prefixes
        .iter()
        .any(|prefix| dir_name == prefix || dir_name.starts_with(&format!("{prefix}-")))
}

/// 안전장치 ②의 권위 검사(M1): `cwd`가 실제로 `scan_workspace_projects()`가
/// 찾은 프로젝트 경로 중 하나의 자기 자신이거나 그 하위인지 **경로 컴포넌트
/// 단위**로 대조하는 순수 함수(fs 접근 없음 — 유닛 테스트 대상).
/// `is_allowed_project_dir`의 문자열 접두사 프리필터는 `foo`가 허용일 때
/// `foo-bar`(별개 디렉터리, 허용 안 됨)까지 통과시키는 결함이 있다 —
/// `sanitize_cwd_for_project_dir`가 `/`를 `-`로 뭉개 경로 구분자 정보를 버려서
/// 문자열만으로는 `foo-bar`와 `foo/bar`를 구분할 수 없기 때문이다.
/// `Path::starts_with`는 컴포넌트 경계를 지키므로(`foo-bar`는 `foo`의
/// 컴포넌트 하위가 아니다) 이 역전을 막는다.
fn is_cwd_within_allowed_workspace(cwd: &str, allowed_roots: &[PathBuf]) -> bool {
    let cwd_path = Path::new(cwd);
    allowed_roots.iter().any(|root| cwd_path.starts_with(root))
}

/// `is_cwd_within_allowed_workspace`가 대조할 실제 워크스페이스 프로젝트 경로
/// 목록. `crate::scan_workspace_projects()`를 그대로 재사용한다(복붙하지
/// 않는다).
fn allowed_workspace_roots() -> Vec<PathBuf> {
    crate::scan_workspace_projects()
        .iter()
        .map(|p| PathBuf::from(&p.path))
        .collect()
}

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
fn build_jsonl_rows() -> Vec<Value> {
    let candidates = scan_jsonl_candidate_paths();
    let capped = cap_recent_jsonl_paths(candidates);
    let allowed_roots = allowed_workspace_roots();
    capped
        .iter()
        .filter_map(|path| build_jsonl_row(path, &allowed_roots))
        .collect()
}

/// jsonl 행에 registry 기반 "지금 실행 중" 오버레이를 얹는 순수 함수(fs 접근
/// 없음 — 유닛 테스트 대상). `live_registry`는 "지금 실행 중"으로 표시할
/// registry 항목 전체다 — `filter_and_dedup_sessions()`를 통과한 결과에
/// `build_live_registry()`로 우리 자식(신규 세션) 항목을 되살린 것이어야
/// 한다(M2). `filter_and_dedup_sessions()` 자체의 본체·시그니처·순서는 이
/// 함수가 건드리지 않는다 — 이미 나온 결과를 입력으로만 받는다.
///
/// - jsonl 행의 `sessionId`가 `live_registry`에도 있으면 `running: true`,
///   없으면 `running: false`.
/// - `live_registry`에는 있는데 대응하는 jsonl 행이 없는 `sessionId`(세션
///   생성 직후 수 초라 아직 jsonl이 안 생겼을 때 — 신규 세션의 첫 턴이 여기
///   해당한다, M2)는 registry 항목을 그대로 행으로 추가한다(`running: true`,
///   `title`이 없으면 빈 문자열로 채운다 — 출력 계약상 `title` 키는 항상
///   있어야 한다).
fn overlay_running_state(mut jsonl_rows: Vec<Value>, live_registry: Vec<Value>) -> Vec<Value> {
    let jsonl_session_ids: std::collections::HashSet<String> = jsonl_rows
        .iter()
        .filter_map(|v| v.get("sessionId").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    let running_ids: std::collections::HashSet<String> = live_registry
        .iter()
        .filter_map(|v| v.get("sessionId").and_then(|v| v.as_str()).map(str::to_string))
        .collect();

    for row in jsonl_rows.iter_mut() {
        let running = row
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(|sid| running_ids.contains(sid))
            .unwrap_or(false);
        if let Value::Object(map) = row {
            map.insert("running".to_string(), Value::Bool(running));
        }
    }

    for mut value in live_registry {
        let has_jsonl_row = value
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(|sid| jsonl_session_ids.contains(sid))
            .unwrap_or(false);
        if has_jsonl_row {
            continue;
        }
        if let Value::Object(map) = &mut value {
            map.insert("running".to_string(), Value::Bool(true));
            if !map.contains_key("title") {
                map.insert("title".to_string(), Value::String(String::new()));
            }
        }
        jsonl_rows.push(value);
    }

    jsonl_rows
}

/// M2: 표시용 "live" registry 목록을 만드는 순수 함수(fs 접근 없음 — 유닛
/// 테스트 대상). `filtered_registry`는 `filter_and_dedup_sessions()`가 이미
/// 만든 결과(자식 제외 완료, 그 본체·시그니처·순서는 여기서 건드리지 않는다),
/// `raw_registry`는 그 이전의 원본 목록, `active_session_ids`는
/// `session_chat::active_turn_session_ids()`(현재 앱이 진행 중인 턴들의
/// session_id — pid 제외 이전에 이미 알고 있는 값)다.
///
/// 신규 세션은 그 `sessionId`를 등록한 프로세스가 우리 자식 하나뿐이라
/// `filter_and_dedup_sessions()`의 1단계(pid 제외)에서 `filtered_registry`
/// 밖으로 완전히 빠진다 — "지금 실행 중"이라는 사실 자체는 `ACTIVE_TURNS`가
/// 이미 알고 있으므로, `filtered_registry`에 없는 `active_session_ids`만
/// `raw_registry`에서 다시 찾아 표시용으로 되살린다(dedup 로직 자체는 손대지
/// 않는다 — 이미 나온 `filtered_registry` 결과에 항목을 추가할 뿐이다). 같은
/// session_id의 `raw_registry` 후보가 여럿이면 `startedAt`이 가장 큰 것을
/// 쓴다(다른 dedup 규칙과 동일한 기준).
fn build_live_registry(
    filtered_registry: Vec<Value>,
    raw_registry: &[Value],
    active_session_ids: &std::collections::HashSet<String>,
) -> Vec<Value> {
    let filtered_session_ids: std::collections::HashSet<String> = filtered_registry
        .iter()
        .filter_map(|v| v.get("sessionId").and_then(|v| v.as_str()).map(str::to_string))
        .collect();

    let mut live_registry = filtered_registry;
    for session_id in active_session_ids {
        if filtered_session_ids.contains(session_id) {
            continue;
        }
        let best = raw_registry
            .iter()
            .filter(|v| v.get("sessionId").and_then(|v| v.as_str()) == Some(session_id.as_str()))
            .max_by_key(|v| v.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0))
            .cloned();
        if let Some(value) = best {
            live_registry.push(value);
        }
    }
    live_registry
}

/// 목록 행을 만드는 진입점. 소스는 jsonl(`build_jsonl_rows`)이고, registry
/// (`~/.claude/sessions/*.json`)는 `filter_and_dedup_sessions()`(순서 고정:
/// 자식 제외 → 죽은 pid 필터 → sessionId dedup, 본체·시그니처 변경 금지)를
/// 통과한 뒤 `build_live_registry()`로 신규 세션(우리 자식만 등록된 세션,
/// M2)의 표시를 되살리고, m3: 그 결과를 `is_cwd_within_allowed_workspace`로도
/// 걸러(registry 오버레이 경로도 안전장치 ②를 통과해야 한다) `overlay_running_state`로
/// "지금 실행 중" 오버레이를 얹는다.
fn read_claude_sessions() -> Vec<Value> {
    let jsonl_rows = build_jsonl_rows();

    let raw_registry = read_raw_registry_sessions();
    let exclude_pids = crate::session_chat::active_turn_pids();
    let filtered_registry = filter_and_dedup_sessions(raw_registry.clone(), &exclude_pids);
    let active_session_ids = crate::session_chat::active_turn_session_ids();
    let mut live_registry = build_live_registry(filtered_registry, &raw_registry, &active_session_ids);

    // m3: registry 오버레이 경로(특히 위에서 되살린 신규 세션 항목과, jsonl이
    // 아직 없어 폴백 행이 될 항목)는 이전에 안전장치 ①②를 전혀 거치지 않았다
    // — 워크스페이스 밖 cwd(예: 스크래치패드)가 목록에 새어나갈 수 있었다.
    // `is_cwd_within_allowed_workspace`(M1의 권위 검사, 실제 워크스페이스
    // 루트만 통과)로 게이트를 추가해 닫는다. `cwd` 필드가 없는 항목은 판단
    // 근거가 없으므로 보수적으로 제외한다(다른 안전장치들과 반대로 여기는
    // fail-closed다 — 이 경로는 "새로 추가하는 게이트"라 기존 관용구를
    // 그대로 따를 이유가 없다).
    let allowed_roots = allowed_workspace_roots();
    live_registry.retain(|value| {
        value
            .get("cwd")
            .and_then(|c| c.as_str())
            .is_some_and(|cwd| is_cwd_within_allowed_workspace(cwd, &allowed_roots))
    });

    for value in live_registry.iter_mut() {
        if let Some(title) = find_session_title(value) {
            if let Value::Object(map) = value {
                map.insert("title".to_string(), Value::String(title));
            }
        }
    }

    overlay_running_state(jsonl_rows, live_registry)
}

/// 예전엔 sync였다(P0 버그와 동일 계열 — non-async `#[tauri::command]`는
/// 메인 스레드에서 돈다). `dev_tools.rs`의 `check_dev_tools`가 정한 관용구
/// (async + `spawn_blocking`)를 그대로 따른다 — 프론트 `invoke()` 계약은
/// 항상 Promise라 시그니처가 바뀌지 않는다.
#[tauri::command]
pub async fn list_claude_sessions() -> Vec<Value> {
    tauri::async_runtime::spawn_blocking(read_claude_sessions)
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 이 머신의 실제 ~/.claude/sessions/*.json을 읽어 최소 1건 이상 파싱되는지
    // 확인한다(이 프로젝트 세션 자체의 메타데이터 파일이 그 디렉토리에 있어야 한다).
    // GUI 없는 환경에서 네이티브 창을 스크린샷할 수 없을 때 이 fs 읽기 로직 자체가
    // 실제로 동작함을 증명하는 자동화된 근거로 쓴다.
    #[test]
    fn finds_at_least_one_real_session_file() {
        let sessions = read_claude_sessions();
        assert!(
            !sessions.is_empty(),
            "~/.claude/sessions/*.json에서 세션을 하나도 찾지 못했습니다"
        );
        assert!(
            sessions.iter().any(|s| s.get("sessionId").is_some()),
            "sessionId 필드를 가진 세션이 하나도 없습니다"
        );
    }

    /// 테스트용 세션 registry 항목을 만든다. `pid_alive()`가 실제 OS 시그널을
    /// 쓰므로 "살아있는" pid로는 현재 테스트 프로세스 자신의 pid(`std::process::id()`)를,
    /// "죽은" pid로는 OS가 배정할 가능성이 사실상 없는 `u32::MAX - 1`을 쓴다
    /// (process_util.rs의 자체 테스트와 동일한 접근).
    fn fixture_session(pid: u32, session_id: &str, started_at: i64) -> Value {
        serde_json::json!({
            "pid": pid,
            "sessionId": session_id,
            "startedAt": started_at,
            "cwd": "/tmp/fixture",
        })
    }

    // 역전 방지 회귀 테스트: 같은 sessionId로 원본(IDE) 항목과 우리 자식
    // (`claude -p`) 항목이 둘 다 registry에 있을 때, 자식이 startedAt이 더
    // 최신이라도 1단계(exclude_pids)에서 먼저 빠지므로 3단계 dedup이 원본을
    // 밀어내지 않아야 한다. dedup을 exclude보다 먼저 적용하면 이 테스트가
    // 실패한다(자식의 최신 startedAt이 선택되어버림).
    #[cfg(unix)]
    #[test]
    fn active_turn_child_excluded_before_dedup_keeps_original() {
        // "원본"은 현재 테스트 프로세스 자신의 pid(항상 살아있고 자기 자신에게는
        // 신호 권한이 있다). "자식"은 실제로 띄운 보조 프로세스의 pid를 써서
        // 반드시 살아있게 만든다 — 이 테스트가 검증하려는 것은 "죽은 프로세스라
        // 걸러졌다"가 아니라 "exclude_pids 제외가 dedup보다 먼저 적용돼야
        // 한다"이므로, 자식도 살아있는 채로 exclude에만 넣는다(pid 1을 쓰지
        // 않는 이유: process_util 테스트 주석 참조 — 일반 사용자는 pid 1에
        // 신호를 보낼 권한이 없어 kill(1,0)이 EPERM으로 "죽음"처럼 보인다).
        let mut helper = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("보조 프로세스(sleep)를 띄우지 못했습니다");
        let original_pid = std::process::id();
        let child_pid = helper.id();
        let session_id = "shared-session-id";

        // 자식 항목은 같은 sessionId, 더 최신 startedAt(실제로 턴 시작 시점이
        // 원본 세션 시작 시점보다 항상 나중이라 그렇다), 그리고 exclude_pids에
        // 포함된 pid.
        let original = fixture_session(original_pid, session_id, 1_000);
        let child = fixture_session(child_pid, session_id, 9_999);

        let mut exclude = std::collections::HashSet::new();
        exclude.insert(child_pid);

        let sessions = vec![original.clone(), child];
        let result = filter_and_dedup_sessions(sessions, &exclude);

        assert_eq!(result.len(), 1, "정리 후 세션이 정확히 1건 남아야 합니다");
        assert_eq!(
            result[0].get("startedAt").and_then(|v| v.as_i64()),
            Some(1_000),
            "원본(startedAt=1000)이 남아야 하는데 자식(startedAt=9999)이 남았습니다 — \
             적용 순서가 뒤집혔을 가능성이 있습니다"
        );

        // 순서가 실제로 중요함을 직접 대조 검증한다: dedup을 exclude보다
        // 먼저 적용하면(반대 순서) 더 최신인 자식이 dedup에서 살아남고,
        // 그 다음에야 exclude로 제거되어 원본까지 함께 사라진다 — 즉 결과가
        // 0건이 되어 원본이 통째로 유실된다. 이 프로젝트의 실제 구현은 이
        // 순서를 쓰지 않지만, 반대 순서가 실제로 다른(더 나쁜) 결과를 낳는다는
        // 것을 명시적으로 남겨 "순서가 중요하다"는 요구사항 자체를 고정한다.
        let reversed_order_result: Vec<Value> = {
            // dedup 먼저
            let mut by_session_id: std::collections::HashMap<String, Value> =
                std::collections::HashMap::new();
            for value in [
                fixture_session(original_pid, session_id, 1_000),
                fixture_session(child_pid, session_id, 9_999),
            ] {
                let sid = value.get("sessionId").and_then(|v| v.as_str()).unwrap().to_string();
                let started = value.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
                match by_session_id.get(&sid) {
                    Some(existing) => {
                        let existing_started =
                            existing.get("startedAt").and_then(|v| v.as_i64()).unwrap_or(0);
                        if started > existing_started {
                            by_session_id.insert(sid, value);
                        }
                    }
                    None => {
                        by_session_id.insert(sid, value);
                    }
                }
            }
            // 그 다음 exclude 적용
            by_session_id
                .into_values()
                .filter(|s| match s.get("pid").and_then(|v| v.as_u64()) {
                    Some(pid) => !exclude.contains(&(pid as u32)),
                    None => true,
                })
                .collect()
        };
        assert!(
            reversed_order_result.is_empty(),
            "반대 순서(dedup 먼저)였다면 원본까지 유실되어 0건이어야 하는데 \
             {reversed_order_result:?}가 남았습니다 — 순서가 중요하다는 전제 자체가 \
             깨졌으니 이 테스트를 다시 검토해야 합니다"
        );

        let _ = helper.kill();
        let _ = helper.wait();
    }

    #[test]
    fn dead_pid_session_is_filtered_out() {
        let dead_pid = u32::MAX - 1;
        let sessions = vec![fixture_session(dead_pid, "dead-session", 1_000)];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());
        assert!(
            result.is_empty(),
            "죽은 pid의 세션 항목은 걸러져야 하는데 남아있습니다: {result:?}"
        );
    }

    #[test]
    fn duplicate_alive_non_child_sessions_keep_latest_started_at() {
        let my_pid = std::process::id();
        let session_id = "duplicate-session-id";
        let older = fixture_session(my_pid, session_id, 1_000);
        let newer = fixture_session(my_pid, session_id, 2_000);

        let sessions = vec![older, newer];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(result.len(), 1, "같은 sessionId는 1건으로 합쳐져야 합니다");
        assert_eq!(
            result[0].get("startedAt").and_then(|v| v.as_i64()),
            Some(2_000),
            "startedAt이 더 최신인 항목이 남아야 합니다"
        );
    }

    #[test]
    fn single_normal_session_passes_through_unchanged() {
        let my_pid = std::process::id();
        let session = fixture_session(my_pid, "solo-session", 1_000);

        let sessions = vec![session.clone()];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(result.len(), 1);
        assert_eq!(result[0], session);
    }

    // 회귀 방지: `pid` 필드가 없는 registry 항목은 1단계(exclude_pids)와 2단계
    // (죽은 pid 필터) 모두 "판단 불가 → 보수적으로 통과"를 명시적으로 선택한
    // 결과다(위 함수 주석의 "판단 불가이므로 보수적으로 통과시킨다" 참조).
    // 이 항목이 dead-pid 취급으로 걸러지지 않아야 한다는 것이 의도된 규칙이며,
    // 나중에 누가 "안전하게" fail-closed로 뒤집으면 이 테스트가 실패해야 한다.
    #[test]
    fn session_missing_pid_field_is_intentionally_kept_not_excluded() {
        let session = serde_json::json!({
            "sessionId": "no-pid-session",
            "startedAt": 1_000,
            "cwd": "/tmp/fixture",
        });

        let sessions = vec![session.clone()];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(
            result.len(),
            1,
            "pid 필드가 없는 항목은 판단 불가로 보수적으로 통과해야 하는데 걸러졌습니다: {result:?}"
        );
        assert_eq!(result[0], session);
    }

    // 회귀 방지: `sessionId` 필드가 없는 registry 항목은 3단계 dedup의 그룹핑
    // 키 자체가 없으므로 "고유 취급"해 서로 dedup되지 않고 둘 다 통과해야
    // 한다(위 함수 주석의 "dedup 키가 없으므로 그대로 통과시킨다(고유 취급)"
    // 참조). 이 규칙은 의도된 것이며, 나중에 누가 sessionId 부재 항목끼리도
    // 병합하도록 "정리"하면 이 테스트가 실패해야 한다.
    #[test]
    fn sessions_missing_session_id_field_are_intentionally_treated_as_unique_not_deduped() {
        let my_pid = std::process::id();
        let first = serde_json::json!({
            "pid": my_pid,
            "startedAt": 1_000,
            "cwd": "/tmp/fixture-a",
        });
        let second = serde_json::json!({
            "pid": my_pid,
            "startedAt": 2_000,
            "cwd": "/tmp/fixture-b",
        });

        let sessions = vec![first, second];
        let result = filter_and_dedup_sessions(sessions, &std::collections::HashSet::new());

        assert_eq!(
            result.len(),
            2,
            "sessionId가 없는 항목끼리는 dedup 키가 없어 병합되지 않고 둘 다 남아야 \
             하는데 결과가 다릅니다: {result:?}"
        );
    }

    // 실제 세션 중 최소 하나는 대화 로그(jsonl)에서 제목을 뽑아낼 수 있어야 한다 —
    // 이 세션 자체가 malgn-vscode 프로젝트에서 지금 실행 중이라 그 jsonl이 실제로
    // 존재하고 자라고 있다.
    #[test]
    fn extracts_title_for_at_least_one_real_session() {
        let sessions = read_claude_sessions();
        assert!(
            sessions.iter().any(|s| s
                .get("title")
                .and_then(|t| t.as_str())
                .is_some_and(|t| !t.is_empty())),
            "실제 세션 중 제목을 추출한 것이 하나도 없습니다"
        );
    }

    // ==================== jsonl 기반 목록 (architect C안) ====================

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

    // 안전장치 ①: `/private/tmp` 유래 프로젝트 디렉터리명은 허용 접두사와 무관하게
    // 항상 제외된다.
    #[test]
    fn private_tmp_derived_project_dir_is_always_excluded() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(!is_allowed_project_dir(
            "-private-tmp-claude-501--Users-hopegiver-workspace-malgn-vscode-scratchpad",
            &allowed
        ));
    }

    // m4: 대소문자가 달라도(`-Private-Tmp-...`) 제외되어야 한다.
    #[test]
    fn private_tmp_derived_project_dir_is_excluded_case_insensitively() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(!is_allowed_project_dir(
            "-Private-Tmp-claude-501--Users-hopegiver-workspace-malgn-vscode-scratchpad",
            &allowed
        ));
        assert!(!is_allowed_project_dir(
            "-PRIVATE-TMP-claude-501--Users-hopegiver-workspace-malgn-vscode-scratchpad",
            &allowed
        ));
    }

    // m4: `/private`를 거치지 않는 순수 `/tmp/...` 경로(`-tmp-...`로 sanitize됨)도
    // 대소문자 무관하게 제외되어야 한다.
    #[test]
    fn bare_tmp_derived_project_dir_is_excluded_case_insensitively() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(!is_allowed_project_dir("-tmp-some-scratch-dir", &allowed));
        assert!(!is_allowed_project_dir("-Tmp-some-scratch-dir", &allowed));
    }

    // 안전장치 ②의 (a)단계(프리필터): 스캔된 워크스페이스 프로젝트 목록에 없는
    // 디렉터리명은 제외된다. 프로젝트 루트 자신과 그 하위 디렉터리(cwd가
    // 서브폴더인 세션)는 모두 허용되어야 한다. **이 단계만으로는 `foo-bar`
    // (별개 디렉터리)가 `foo` 허용에 의해 통과된다** — 그 결함은 아래
    // `is_cwd_within_allowed_workspace`(권위 검사) 테스트가 고정한다.
    #[test]
    fn only_dirs_matching_scanned_workspace_projects_are_allowed() {
        let allowed = vec!["-Users-hopegiver-workspace-malgn-vscode".to_string()];
        assert!(is_allowed_project_dir(
            "-Users-hopegiver-workspace-malgn-vscode",
            &allowed
        ));
        assert!(is_allowed_project_dir(
            "-Users-hopegiver-workspace-malgn-vscode-src-tauri-src",
            &allowed
        ));
        assert!(!is_allowed_project_dir(
            "-Users-hopegiver-workspace-some-other-project",
            &allowed
        ));
    }

    // M1 완료 판정: `foo`가 허용일 때 `foo-bar`(별개 디렉터리) 유래 세션은
    // 권위 검사에서 거부되어야 한다 — 프리필터(`is_allowed_project_dir`)는
    // 문자열 접두사라 이 역전을 막지 못하지만, 실제 `cwd`를 경로 컴포넌트
    // 단위로 대조하는 `is_cwd_within_allowed_workspace`는 막는다.
    #[test]
    fn cwd_authority_check_rejects_sibling_dir_with_shared_prefix() {
        let allowed_roots = vec![PathBuf::from("/Users/hopegiver/workspace/foo")];

        assert!(
            is_cwd_within_allowed_workspace("/Users/hopegiver/workspace/foo", &allowed_roots),
            "허용 루트 자신은 통과해야 합니다"
        );
        assert!(
            is_cwd_within_allowed_workspace(
                "/Users/hopegiver/workspace/foo/sub",
                &allowed_roots
            ),
            "허용 루트의 하위 디렉터리는 통과해야 합니다"
        );
        assert!(
            !is_cwd_within_allowed_workspace(
                "/Users/hopegiver/workspace/foo-bar",
                &allowed_roots
            ),
            "foo-bar는 foo의 컴포넌트 하위가 아니므로 거부되어야 하는데 통과했습니다 — \
             문자열 접두사와 경로 컴포넌트를 혼동했을 가능성이 있습니다"
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

    // registry에 살아있는(filter_and_dedup_sessions를 통과한) sessionId와 같은
    // jsonl 행에는 running=true가 붙어야 한다.
    #[test]
    fn overlay_marks_jsonl_row_matching_registry_as_running_true() {
        let jsonl_rows = vec![serde_json::json!({
            "sessionId": "shared-sid",
            "cwd": "/tmp/fixture",
            "title": "제목",
            "startedAt": 1_000,
            "updatedAt": 2_000,
        })];
        let filtered_registry = vec![serde_json::json!({
            "sessionId": "shared-sid",
            "pid": std::process::id(),
            "cwd": "/tmp/fixture",
            "startedAt": 1_000,
        })];

        let result = overlay_running_state(jsonl_rows, filtered_registry);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].get("running").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    // registry에 대응 항목이 없는 jsonl 행은 running=false여야 한다(끝난 세션).
    #[test]
    fn overlay_marks_jsonl_row_without_registry_match_as_running_false() {
        let jsonl_rows = vec![serde_json::json!({
            "sessionId": "finished-sid",
            "cwd": "/tmp/fixture",
            "title": "",
            "startedAt": 1_000,
            "updatedAt": 2_000,
        })];

        let result = overlay_running_state(jsonl_rows, Vec::new());
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].get("running").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    // 폴백: registry에는 있지만 대응하는 jsonl 행이 아직 없는 sessionId(세션
    // 생성 직후 수 초)는 registry 항목 그대로 행으로 추가되고 running=true다.
    #[test]
    fn registry_only_session_without_jsonl_row_is_added_as_fallback_row() {
        let jsonl_rows: Vec<Value> = Vec::new();
        let filtered_registry = vec![serde_json::json!({
            "sessionId": "brand-new-sid",
            "pid": std::process::id(),
            "cwd": "/tmp/fixture",
            "startedAt": 9_000,
        })];

        let result = overlay_running_state(jsonl_rows, filtered_registry);
        assert_eq!(result.len(), 1, "폴백 행 하나가 추가되어야 합니다");
        assert_eq!(
            result[0].get("sessionId").and_then(|v| v.as_str()),
            Some("brand-new-sid")
        );
        assert_eq!(
            result[0].get("running").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            result[0].get("title").and_then(|v| v.as_str()),
            Some(""),
            "title 키는 출력 계약상 항상 있어야 하며, 못 뽑았으면 빈 문자열이어야 합니다"
        );
    }

    // ==================== M2: 신규 세션 표시 (build_live_registry) ====================

    // M2 완료 판정: 신규 세션은 그 sessionId를 등록한 프로세스가 우리 자식
    // 하나뿐이라 `filter_and_dedup_sessions()`의 1단계(pid 제외)에서
    // `filtered_registry` 밖으로 완전히 빠진다(여기서는 그 결과를 그대로
    // 재현하려고 `filtered_registry`를 빈 벡터로 둔다). 하지만 `ACTIVE_TURNS`는
    // 그 턴이 진행 중임을 알고 있으므로(`active_session_ids`), `build_live_registry`가
    // `raw_registry`에서 그 항목을 되살려야 하고, 그 결과를 `overlay_running_state`에
    // 넘기면 신규 세션이 목록에 행으로 존재하고 `running=true`여야 한다.
    #[test]
    fn brand_new_session_excluded_from_filtered_registry_still_becomes_live_fallback_row_running_true(
    ) {
        let filtered_registry: Vec<Value> = Vec::new(); // 1단계(pid 제외)로 이미 빠진 상태를 재현
        let raw_registry = vec![serde_json::json!({
            "sessionId": "brand-new-sid",
            "pid": 12_345, // 우리 자식의 pid(exclude_pids에 포함되어 filtered_registry에서 빠졌다)
            "cwd": "/tmp/fixture",
            "startedAt": 9_000,
        })];
        let mut active_session_ids = std::collections::HashSet::new();
        active_session_ids.insert("brand-new-sid".to_string());

        let live_registry =
            build_live_registry(filtered_registry, &raw_registry, &active_session_ids);
        assert_eq!(
            live_registry.len(),
            1,
            "ACTIVE_TURNS에 있는 신규 세션은 live_registry에 되살아나야 합니다"
        );

        // jsonl은 아직 생성되지 않은 상태(세션 생성 직후 수 초) 그대로 재현한다.
        let jsonl_rows: Vec<Value> = Vec::new();
        let result = overlay_running_state(jsonl_rows, live_registry);

        assert_eq!(
            result.len(),
            1,
            "신규 세션의 턴이 진행 중일 때 그 세션이 목록에 행으로 존재해야 합니다"
        );
        assert_eq!(
            result[0].get("sessionId").and_then(|v| v.as_str()),
            Some("brand-new-sid")
        );
        assert_eq!(
            result[0].get("running").and_then(|v| v.as_bool()),
            Some(true),
            "신규 세션의 턴이 진행 중이면 running=true여야 합니다"
        );
    }

    // build_live_registry는 filtered_registry에 이미 있는 session_id는 중복
    // 추가하지 않는다(raw_registry에서 다시 찾아 되살릴 필요가 없다).
    #[test]
    fn build_live_registry_does_not_duplicate_session_already_in_filtered_registry() {
        let existing = serde_json::json!({
            "sessionId": "already-present-sid",
            "pid": std::process::id(),
            "cwd": "/tmp/fixture",
            "startedAt": 1_000,
        });
        let filtered_registry = vec![existing.clone()];
        let raw_registry = vec![existing];
        let mut active_session_ids = std::collections::HashSet::new();
        active_session_ids.insert("already-present-sid".to_string());

        let live_registry =
            build_live_registry(filtered_registry, &raw_registry, &active_session_ids);
        assert_eq!(live_registry.len(), 1, "이미 있는 session_id는 중복 추가되면 안 됩니다");
    }

    // ==================== QA 실데이터 검증(#[ignore] — 이 머신 의존) ====================
    // 실행: cargo test -- --ignored --nocapture session_list::tests::real_scan
    // 목적: 서브에이전트 유령 세션(실측 1,000건 이상)이 섞이지 않는지, 그리고
    // registry에서 사라진 뒤로 목록에 영영 안 보이던 끝난 세션이 이제 보이는지
    // 실제 데이터로 확인한다.
    #[test]
    #[ignore]
    fn real_scan_excludes_subagent_ghosts_and_surfaces_previously_hidden_finished_session() {
        let sessions = read_claude_sessions();
        assert!(
            sessions.len() < 1_000,
            "서브에이전트 유령 세션이 섞였을 가능성이 있습니다: {}건",
            sessions.len()
        );
        assert!(
            sessions.iter().any(|s| s
                .get("sessionId")
                .and_then(|v| v.as_str())
                == Some("7b57eff2-8ea4-450d-b3b7-45510a499bf3")),
            "이전에 registry에서 사라져 목록에 안 보이던 끝난 세션이 여전히 보이지 않습니다"
        );
    }
}
