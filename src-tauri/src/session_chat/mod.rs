// ---------------- 세션 상세 = 실제 대화 + 이어쓰기 ----------------
// 정본: docs/design/session-chat.md §3(메시지 모델·접기)·§4(IPC 계약)·
// §5(보안 경계 S1~S13)·§7(비정상 케이스). 커맨드 3개(read_session_transcript /
// send_session_message / cancel_session_turn) + 이벤트 2개(session-chat-delta /
// session-chat-done)를 그 설계 그대로 구현한다.
//
// S1/S2 불변식: 셸을 거치지 않는다(std::process::Command + 인자 배열만, `sh -c`
// 없음). 사용자가 입력한 대화 텍스트는 **stdin으로만** 자식 프로세스에 전달하고
// argv에는 절대 넣지 않는다 — argv에 넣으면 사용자가 친 `--...`가 CLI 플래그로
// 해석된다(설계 §1-B7 실측).
//
// 서브모듈 구성(`autonomy/` 패턴을 따름):
// - `transcript`: jsonl → `ChatMessage` 변환(§3), session_id 검증(S5), 트랜스
//   크립트 파일 탐색(S6).
// - `turn`: `claude -p` 프로세스 spawn과 턴 수명주기(S8/S9), 취소·앱 종료 정리.
// - 이 파일(`mod.rs`): Tauri 커맨드 진입점(입력 검증 포함). `#[tauri::command]`가
//   생성하는 숨은 매크로(`__cmd__*`)는 모듈 비공개라 `pub use`로 재노출할 수
//   없으므로(실측: 재노출 시 `generate_handler!`가 컴파일 실패), `lib.rs`가
//   `session_chat::X` 형태로 직접 참조하는 커맨드 4개는 이 파일에 그대로 둔다
//   (autonomy/mod.rs와 동일한 패턴).
//
// ⚠️ 최근 사람 승인을 거친 보안 코드: `validate_project_path()`(TOCTOU 방지,
// 완전 일치 비교)와 `start_new_session_message()`의 spawn 직전 `is_dir()`
// 재확인은 내용을 한 글자도 바꾸지 않았다.

mod transcript;
mod turn;

use std::path::Path;

use transcript::SessionTranscript;

pub use turn::request_shutdown;

/// `lib.rs`의 `read_claude_sessions()`가 registry에서 우리 자식 프로세스(앱이
/// `send_session_message`로 스스로 띄운 `claude -p`) 항목을 먼저 제외할 수
/// 있도록 현재 진행 중인 턴들의 pid 집합을 노출한다.
pub(crate) use turn::active_turn_pids;

const MAX_INPUT_CHARS: usize = 32_000;

/// `dev_tools.rs`/`autonomy.rs`/`mcp_manager.rs`가 이미 정한 관례(상수를 공유
/// 하지 않고 각자 별도로 둔다, 회귀 위험 0)를 그대로 따른다.
const CLAUDE_PATH_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/claude",
    "/usr/local/bin/claude",
    "~/.local/bin/claude",
];

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SendStarted {
    pub turn_id: String,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewSessionStarted {
    pub session_id: String,
    pub turn_id: String,
}

/// `send_session_message`/`start_new_session_message` 공통 입력 검증(기존
/// 동작 그대로 — 문자열만 뽑아내 공유했을 뿐 판정 로직은 바뀌지 않았다).
fn validate_message_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("보낼 내용을 입력하세요.".to_string());
    }
    if text.chars().count() > MAX_INPUT_CHARS {
        return Err("한 번에 보낼 수 있는 길이를 초과했습니다(최대 32,000자).".to_string());
    }
    Ok(())
}

// ==================== (a) read_session_transcript ====================

#[tauri::command]
pub fn read_session_transcript(session_id: String) -> Result<SessionTranscript, String> {
    let path = transcript::resolve_transcript_path(&session_id)?;
    let (messages, truncated, cwd) = transcript::read_transcript_messages(&path)?;
    let active_turn_id = turn::active_turn_id_for_session(&session_id);

    Ok(SessionTranscript {
        session_id,
        cwd: cwd.unwrap_or_default(),
        transcript_path: path.to_string_lossy().to_string(),
        messages,
        truncated,
        active_turn_id,
    })
}

// ==================== (b) send_session_message ====================

#[tauri::command]
pub fn send_session_message(
    app: tauri::AppHandle,
    session_id: String,
    text: String,
) -> Result<SendStarted, String> {
    validate_message_text(&text)?;

    let transcript_path = transcript::resolve_transcript_path(&session_id)?;
    let cwd = transcript::read_cwd_from_transcript(&transcript_path)?;
    if !Path::new(&cwd).is_dir() {
        return Err(format!("세션의 작업 폴더를 찾을 수 없습니다: {cwd}"));
    }

    // §6-① 갱신: 세션목록에 뜨는 세션은 §1-E 실측대로 예외 없이 전부 "지금
    // 다른 창에서 실행 중"이라 원래의 하드 게이트(무조건 거부)를 두면 입력창이
    // 항상 비활성화되어 재개 기능 자체가 성립하지 않았다. 실측 결과
    // `claude -p --resume`은 원본 session_id를 유지하며 원본 jsonl에 그대로
    // append한다(대화 분기는 일어나지 않는다) — 그래서 차단도, 경고도 두지
    // 않는다.
    let claude_path = crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
        .ok_or_else(|| {
            "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).".to_string()
        })?;

    let turn_id = turn::generate_uuid_v4();
    turn::register_turn(&turn_id, &session_id)?;

    let path_env = crate::dev_tools::build_child_path_env(Some(&claude_path));
    let session_id_for_thread = session_id.clone();
    let turn_id_for_thread = turn_id.clone();

    std::thread::spawn(move || {
        turn::run_turn(
            app,
            claude_path,
            path_env,
            cwd,
            session_id_for_thread,
            turn_id_for_thread,
            text,
            true, // resume: 기존 세션 재개 경로 — 회귀 금지 대상
        );
    });

    Ok(SendStarted { turn_id })
}

// ==================== (b') start_new_session_message (S4) ====================
//
// 프로젝트 카드 "새 세션" 버튼용. 버튼 클릭 시점에는 spawn하지 않는다 — 프론트가
// draft 상태만 만들고, 사용자가 첫 메시지를 보낼 때 이 커맨드가 호출되어 실제로
// spawn한다.

/// S4 안전장치 ①②: 프론트가 넘긴 `project_path`를 문자 그대로 신뢰하지
/// 않는다. **요청 시점에** `scan_workspace_projects()`를 다시 실행해(캐시된
/// 값이나 프론트 상태 불신 — TOCTOU 방지), 그 결과의 실제 프로젝트 path
/// 집합과 정확히 일치하는 항목이 있는지만 확인한다. `crate::scan_workspace_projects()`가
/// 이미 `~/workspace` 등 설정된 workspace 루트 바로 아래 1단계 + `CLAUDE.md`
/// 존재를 요구하므로, 이 매치에 실패하면 `/etc`·`~/.ssh`·`..` 트래버설·존재하지
/// 않는 경로가 전부 구조적으로 걸러진다(스캔 결과에 없으므로).
fn validate_project_path(project_path: &str) -> Result<(), String> {
    let projects = crate::scan_workspace_projects();
    let matched = projects.iter().any(|p| p.path == project_path);
    if !matched {
        return Err("워크스페이스에서 확인되지 않은 프로젝트 경로입니다.".to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn start_new_session_message(
    app: tauri::AppHandle,
    project_path: String,
    text: String,
) -> Result<NewSessionStarted, String> {
    validate_message_text(&text)?;

    // S4 ①②: 재스캔 매치.
    validate_project_path(&project_path)?;
    // S4 ③: 매치 후 spawn 직전 재확인(기존 send_session_message와 동일 깊이 —
    // 재스캔과 spawn 사이에 디렉터리가 사라지는 TOCTOU 잔여 창을 좁힌다).
    if !Path::new(&project_path).is_dir() {
        return Err(format!("프로젝트 폴더를 찾을 수 없습니다: {project_path}"));
    }

    let claude_path = crate::cli_launcher::resolve_binary_expand_home(&CLAUDE_PATH_CANDIDATES, "claude")
        .ok_or_else(|| {
            "claude 실행 파일을 찾을 수 없습니다(알려진 설치 경로와 PATH 모두 실패).".to_string()
        })?;

    // 세션 생성 방식: --session-id를 앱이 사전 생성해 CLI에 넘긴다(사후 캡처
    // 방식이 아니다 — run_turn()이 stream-json의 system/init 이벤트에서
    // session_id를 읽지 않는 기존 잠재 버그를 이 방식으로 우회한다).
    let session_id = turn::generate_uuid_v4();
    let turn_id = turn::generate_uuid_v4();
    turn::register_turn(&turn_id, &session_id)?;

    let path_env = crate::dev_tools::build_child_path_env(Some(&claude_path));
    let session_id_for_thread = session_id.clone();
    let turn_id_for_thread = turn_id.clone();
    let cwd = project_path;

    std::thread::spawn(move || {
        turn::run_turn(
            app,
            claude_path,
            path_env,
            cwd,
            session_id_for_thread,
            turn_id_for_thread,
            text,
            false, // resume: 첫 실행이므로 --resume을 붙이지 않는다
        );
    });

    Ok(NewSessionStarted { session_id, turn_id })
}

// ==================== (c) cancel_session_turn ====================

#[tauri::command]
pub fn cancel_session_turn(turn_id: String) -> Result<(), String> {
    turn::cancel_turn(&turn_id);
    Ok(())
}

// ==================== 단위 테스트 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ==================== S4: start_new_session_message 안전장치 ====================

    // 보안 케이스 1: ~/workspace 밖의 임의 경로(/etc, ~/.ssh)는 거부된다 —
    // scan_workspace_projects()의 실제 결과에 없으므로 매치에 실패한다.
    #[test]
    fn validate_project_path_rejects_paths_outside_workspace_root() {
        assert!(validate_project_path("/etc").is_err());
        let ssh_dir = dirs::home_dir().map(|h| h.join(".ssh").to_string_lossy().to_string());
        if let Some(ssh_dir) = ssh_dir {
            assert!(validate_project_path(&ssh_dir).is_err());
        }
    }

    // 보안 케이스 2: `..`를 포함한 경로 traversal 시도는 거부된다 — 스캔
    // 결과는 항상 절대경로 문자열이라 상대 표기가 그대로 일치할 수 없다.
    #[test]
    fn validate_project_path_rejects_traversal_attempt() {
        assert!(validate_project_path("../../etc/passwd").is_err());
        assert!(validate_project_path("/Users/hopegiver/workspace/malgn-vscode/../../etc").is_err());
    }

    // 보안 케이스 3: 스캔 결과에 없는(존재하지 않는) 경로는 거부된다.
    #[test]
    fn validate_project_path_rejects_path_not_in_scan_results() {
        assert!(validate_project_path("/tmp/definitely-not-a-scanned-project-xyz123").is_err());
    }

    // 보안 케이스 4: scan_workspace_projects()가 실제로 찾아낸 정상 프로젝트
    // 경로는 통과한다(이 저장소 자신 — CLAUDE.md가 있어 항상 스캔된다).
    #[test]
    fn validate_project_path_accepts_a_real_scanned_project_path() {
        let projects = crate::scan_workspace_projects();
        let project = projects
            .iter()
            .find(|p| p.name == "malgn-vscode")
            .expect("malgn-vscode 프로젝트가 스캔 결과에 없습니다");
        assert!(validate_project_path(&project.path).is_ok());
    }

    // ==================== QA 실데이터 검증(#[ignore] — 이 머신 의존, CI에서 실행 안 함) ====================
    // 실행: cargo test -- --ignored --nocapture session_chat::tests::real_world_transcripts
    // 목적: 합성 fixture가 아니라 ~/.claude/projects/**/*.jsonl 실파일에 대해
    // read_session_transcript()를 프로덕션 코드 그대로(수정 없이) 호출해
    // 패닉/빈 결과/상한 위반 여부를 눈으로 확인한다. 읽기 전용이라 부작용 없음.
    #[test]
    #[ignore]
    fn real_world_transcripts_parse_without_panicking_or_garbage() {
        let home = dirs::home_dir().expect("home dir");
        let projects_dir = home.join(".claude").join("projects");
        let mut candidates: Vec<(PathBuf, u64)> = Vec::new();
        for entry in std::fs::read_dir(&projects_dir).expect("read projects dir").flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    candidates.push((p, size));
                }
            }
        }
        candidates.sort_by_key(|(_, size)| *size);
        let n = candidates.len();
        assert!(n >= 10, "실 jsonl 파일이 10개 미만입니다: {n}");

        // 크기 다양성: 가장 작은 4개 + 중간 4개 + 가장 큰 4개 (총 12개, 중복 제거)
        let mut picked: Vec<&(PathBuf, u64)> = Vec::new();
        picked.extend(candidates.iter().take(4));
        picked.extend(candidates.iter().skip(n / 2).take(4));
        picked.extend(candidates.iter().rev().take(4));
        picked.sort_by_key(|(p, _)| p.clone());
        picked.dedup_by_key(|(p, _)| p.clone());

        eprintln!("=== 실데이터 검증 대상 {}개 (전체 후보 {n}개 중) ===", picked.len());

        let mut empty_sessions: Vec<String> = Vec::new();
        for (path, size) in &picked {
            let sid = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
            let sid_for_panic = sid.clone();
            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(move || read_session_transcript(sid_for_panic));
            let elapsed = start.elapsed();
            match result {
                Ok(Ok(t)) => {
                    eprintln!(
                        "OK  sid={sid} size={size}B elapsed={elapsed:?} messages={} truncated={} cwd={}",
                        t.messages.len(),
                        t.truncated,
                        t.cwd
                    );
                    if t.messages.is_empty() {
                        empty_sessions.push(format!("{sid} ({path:?}, {size}B)"));
                    }
                    for m in t.messages.iter().take(3) {
                        let preview: String = m.text.chars().take(100).collect();
                        eprintln!(
                            "    [{}] toolCount={} {}",
                            m.kind,
                            m.tool_count,
                            preview.replace('\n', " ")
                        );
                    }
                    assert!(
                        t.messages.len() <= transcript::MAX_MESSAGES,
                        "400개 상한을 넘었습니다: sid={sid} len={}",
                        t.messages.len()
                    );
                    for m in &t.messages {
                        assert!(
                            m.text.chars().count() <= transcript::MAX_MESSAGE_CHARS + 1,
                            "8000자 컷이 걸리지 않았습니다: sid={sid} len={}",
                            m.text.chars().count()
                        );
                    }
                    // 접기 규칙(§3-3) 위반 검증: tool_count>=3인 줄이 연속 3개 tool
                    // 원본 없이 하나로 접혔는지는 여기서 직접 재현하기 어려우므로,
                    // 대신 "tool" 종류인데 tool_count==0인 비정상 상태만 방어적으로 체크.
                    for m in &t.messages {
                        if m.kind == "tool" {
                            assert!(m.tool_count >= 1, "tool 메시지의 tool_count가 0입니다: sid={sid}");
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("ERR sid={sid} error={e}");
                }
                Err(_) => {
                    panic!("PANIC while parsing sid={sid} path={path:?}");
                }
            }
        }

        eprintln!(
            "=== 빈 세션 {}개/{} ===\n{}",
            empty_sessions.len(),
            picked.len(),
            empty_sessions.join("\n")
        );
    }
}
