// ---------------- 프로세스 생존 확인 공유 유틸 ----------------
// `lib.rs`(filter_and_dedup_sessions의 죽은 pid 필터)와 `session_chat.rs`
// (kill_process_group_with_grace)가 이 모듈 하나만 쓴다. 예전에는 같은 함수가
// session_chat.rs에만 있었는데, lib.rs의 read_claude_sessions()가 pid 생존
// 검사를 전혀 하지 않아 같은 registry 데이터에 대해 "생존"의 의미가 두 곳에서
// 갈라지는 문제가 있었다 — 이 모듈을 양쪽이 공유하게 해서 그 갈라짐 자체를
// 구조적으로 막는다. 이 함수를 복붙하지 말 것.

/// `pid`가 가리키는 프로세스가 아직 살아있는가.
#[cfg(unix)]
pub fn pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Windows에서 추가 의존성(crate) 없이 PID 생존 여부를 확인할 안전한 표준
/// API가 없어, registry 항목 존재만으로 항상 `true`(살아있음)를 반환한다.
///
/// 이 갭이 미치는 영향: `read_claude_sessions()`의 1단계(ACTIVE_TURNS 제외)가
/// 자식 `claude -p` 프로세스로 인한 같은 세션 중복 표시는 pid 생존 여부와
/// 무관하게 이미 완전히 막는다. 다만 턴이 끝나면 `finish_turn()`이 그 pid를
/// `ACTIVE_TURNS`에서 빼므로, 취소(SIGTERM/SIGKILL)나 크래시로 프로세스가
/// 죽은 뒤에는 1단계도 더는 걸러주지 못한다. 그 결과 registry 파일만 남은
/// 죽은 자식 항목이 2단계(이 함수)를 그대로 통과해 3단계(sessionId dedup)까지
/// 도달하고, 그 항목의 `startedAt`(턴 시작 시각)은 원본 항목보다 항상 최신이라
/// 3단계의 "최신 1건" 선택이 죽은 자식 항목을 승자로 골라 원본 세션 항목을
/// 밀어낸다. 즉 증상은 "유령 항목이 하나 더 보인다"가 아니라 **그 세션 행이
/// 죽은 항목의 메타데이터(죽은 pid·턴 시작 시각)로 바뀌는 것**이며, 다음
/// `claude` 실행(같은 pid가 재사용되며 registry가 갱신될 때)까지 그 상태로
/// 남는다. Windows용 OS 레벨 생존 확인(OpenProcess 등)은 이번 작업 범위가
/// 아니다.
#[cfg(windows)]
pub fn pid_alive(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_is_alive() {
        assert!(pid_alive(std::process::id()));
    }

    /// pid 1(init/launchd)은 존재하지만, 일반 사용자 프로세스는 macOS/Linux
    /// 모두에서 `kill(1, 0)`에 대해 EPERM(권한 없음)을 받는다 — `kill()`은
    /// 존재하지 않음(ESRCH)과 권한 없음(EPERM)을 리턴값만으로 구분하지 않고
    /// 둘 다 -1을 반환하므로, 이 구현은 "존재하지만 신호를 보낼 권한이 없는"
    /// 프로세스를 "죽음"으로 오판한다(kill(pid,0)==0일 때만 true). 그래서
    /// pid 1로 "살아있음"을 검증하지 않는다 — 대신 실제로 이 테스트 프로세스가
    /// 직접 띄우고 죽인 자식 프로세스로 살아있음/죽음 전이를 검증한다(우리
    /// 자신이 띄운 자식에는 항상 시그널 권한이 있다 — `session_chat.rs`가
    /// 실제로 다루는 대상도 항상 이런 자식 프로세스다).
    #[cfg(unix)]
    #[test]
    fn spawned_child_is_alive_then_dead_after_reaped() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .expect("보조 프로세스(sleep)를 띄우지 못했습니다");
        let pid = child.id();

        assert!(pid_alive(pid), "막 띄운 자식 프로세스는 살아있어야 합니다");

        child.kill().expect("자식 프로세스 종료 실패");
        child.wait().expect("자식 프로세스 회수(wait) 실패");

        assert!(
            !pid_alive(pid),
            "종료되고 회수(reap)된 자식 프로세스는 죽은 것으로 판정되어야 합니다"
        );
    }

    #[cfg(unix)]
    #[test]
    fn very_unlikely_pid_is_not_alive() {
        // i32::MAX에 가까운 pid는 실제 OS가 배정할 가능성이 사실상 없다.
        assert!(!pid_alive(u32::MAX - 1));
    }
}
