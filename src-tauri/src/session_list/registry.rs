use serde_json::Value;

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
pub(super) fn filter_and_dedup_sessions(
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
pub(super) fn overlay_running_state(mut jsonl_rows: Vec<Value>, live_registry: Vec<Value>) -> Vec<Value> {
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
pub(super) fn build_live_registry(
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

#[cfg(test)]
mod tests {
    use super::*;

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

    // 크로스플랫폼(`process_util::pid_alive`가 Windows에서도 OpenProcess+
    // GetExitCodeProcess로 실제 OS 레벨 생존을 확인하도록 고쳐졌다 —
    // process_util.rs 참조). u32::MAX-1은 Windows pid 공간에서도 실제
    // 배정 가능성이 사실상 없어 이 테스트의 전제가 두 플랫폼 모두에서
    // 성립한다. 예전에는 Windows의 `pid_alive`가 registry 존재만으로 항상
    // true였던 설계상 미구현이라 `#[cfg(unix)]`로 막아 뒀었다.
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
}
