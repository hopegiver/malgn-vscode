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

/// Windows에서 `OpenProcess`(`PROCESS_QUERY_LIMITED_INFORMATION`) +
/// `GetExitCodeProcess`(`STILL_ACTIVE` 판정)로 실제 OS 레벨 생존 여부를
/// 확인한다.
///
/// 이 함수가 고치는 갭(예전에는 registry 항목 존재만으로 항상 `true`를
/// 반환했다): `read_claude_sessions()`의 1단계(ACTIVE_TURNS 제외)는 자식
/// `claude -p` 프로세스로 인한 같은 세션 중복 표시를 pid 생존 여부와
/// 무관하게 이미 막지만, 턴이 끝나 `finish_turn()`이 그 pid를
/// `ACTIVE_TURNS`에서 빼고 나면(취소·크래시로 프로세스가 죽은 뒤) 1단계도
/// 더는 걸러주지 못한다. 그 결과 registry 파일만 남은 죽은 자식 항목이
/// 2단계(이 함수)를 무조건 통과해 3단계(sessionId dedup, "최신 1건" 선택)의
/// 승자가 되어 원본 세션 행을 죽은 항목의 메타데이터(죽은 pid·턴 시작
/// 시각)로 덮어썼다(다음 `claude` 실행까지 고착). 아래 구현은 registry
/// 존재가 아니라 실제 OS 상태로 판정해 이 오탐을 없앤다.
///
/// 판단이 애매한 두 지점을 다음과 같이 처리한다:
/// - **pid 재사용(PID recycling)**: `OpenProcess`가 성공해도 그 순간 그
///   pid로 실행 중인 프로세스가 "우리가 원래 추적하던 그 프로세스"와
///   동일하다는 보장은 pid 하나만으로는 원천적으로 불가능하다 — 유닉스
///   구현(`kill(pid, 0)`, 11행)도 동일한 근본 한계를 이미 안고 있으므로
///   이 함수만의 새로운 약점은 아니다.
/// - **권한 부족으로 실패 vs 정말 없는 pid**: `OpenProcess` 실패의 두 원인을
///   에러 코드로 구분하지 않고 **둘 다 "죽음"(`false`)으로 기운다.** 근거:
///   이 함수의 호출부(`session_list::registry`의 죽은 pid 필터,
///   `session_chat::turn`의 유예 종료 확인)는 항상 "이 앱 자신이 spawn한
///   자식"만 다루므로(같은 사용자 세션·같은 무결성 수준) 실사용에서 권한
///   부족으로 `OpenProcess`가 실패할 일은 사실상 없다. 반대로 이 함수가
///   고치려는 원래 증상(오탐 `true`로 죽은 항목이 승자가 되는 것)을
///   재현하지 않으려면, 애매할 때 오탐(true)보다 미탐(false) 쪽이 안전한
///   실패 모드다 — 미탐의 대가는 "살아있는 세션이 한 번 더 걸러짐" 정도지만
///   오탐의 대가는 이 함수가 고치려는 버그 그 자체다.
#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // SAFETY: FFI 호출. PROCESS_QUERY_LIMITED_INFORMATION은 종료 코드 조회에
    // 필요한 최소 권한만 요청한다(Vista+). bInheritHandle=false로 핸들을
    // 자식 프로세스에 상속하지 않는다.
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        // 위 문서화한 근거대로: 존재하지 않는 pid든 권한 부족이든 구분하지
        // 않고 "죽음"으로 기운다.
        Err(_) => return false,
    };

    let mut exit_code: u32 = 0;
    // SAFETY: handle은 바로 위에서 OpenProcess가 성공 반환한 유효한 핸들이며,
    // 이 함수를 벗어나기 전에 반드시 CloseHandle로 해제한다.
    let query_result = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe {
        let _ = CloseHandle(handle);
    }

    match query_result {
        // STILL_ACTIVE(259)와 다르면 이미 종료된 것이다.
        Ok(()) => exit_code == STILL_ACTIVE.0 as u32,
        Err(_) => false,
    }
}

// ---------------- 프로세스 그룹 종료(SIGTERM → 3초 유예 → SIGKILL) ----------------
// `session_chat::turn`(채팅 턴 취소·앱 종료 정리)이 원래 갖고 있던 구현을
// 그대로 옮긴 것이다(로직 무변경). `claude_auth`(로그인 프로세스 취소·앱
// 자체 타임아웃·앱 종료 정리)가 같은 종료 정책을 필요로 하게 되면서,
// `pid_alive`와 같은 이유로 두 번째 사본을 복붙하지 않고 이 한 곳으로
// 모았다 — 이 함수를 다시 복붙하지 말 것.
#[cfg(unix)]
pub fn kill_process_group_with_grace(pid: u32) {
    unsafe {
        libc::kill(-(pid as i32), libc::SIGTERM);
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        if pid_alive(pid) {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    });
}

/// Windows에는 POSIX 프로세스 그룹이 없다. `taskkill /T /F`는 셸을 거치지
/// 않는 직접 프로세스 실행(S1 불변식 유지)이며 대상 트리를 강제 종료하므로
/// 별도의 유예 폴링이 필요 없다. `taskkill`을 bare-name으로 스폰하지
/// 않는다 — 이 앱의 Windows 배포물은 단일 포터블 exe라 보통 다운로드
/// 폴더에 놓이고, 거기 동명의 악성 실행파일이 있으면 bare-name spawn이
/// 그것을 대신 실행할 수 있다. 대신 `dev_tools::platform::windows_system_tool`로
/// `%SystemRoot%\System32\taskkill.exe` 절대경로를 조립해 실행한다.
///
/// taskkill 자체가 멈추는 경우(보안 제품의 개입, 시스템 과부하 등)에 호출자가 쥔
/// 락(plugins의 `PLUGIN_OP_LOCK` 등)이 함께 묶이지 않도록 `TASKKILL_TIMEOUT`을
/// 넘기면 taskkill을 `kill()`하고 반환한다. 정상적으로는 1초 안에 끝나므로 평소
/// 동작(끝날 때까지 기다린 뒤 반환, 결과 무시)은 그대로다.
#[cfg(windows)]
pub fn kill_process_group_with_grace(pid: u32) {
    use std::process::Stdio;

    let roots = crate::dev_tools::platform::EnvRoots::from_env();
    let taskkill = crate::dev_tools::platform::windows_system_tool(&roots, r"System32\taskkill.exe");
    let mut command = std::process::Command::new(taskkill);
    // `.output()`과 같이 stdin은 닫고 출력은 버린다(파이프를 만들면 읽는 쪽이 있어야 한다).
    command
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .silent()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(mut child) = command.spawn() {
        wait_or_kill(&mut child, TASKKILL_TIMEOUT);
    }
}

/// `taskkill` 한 번이 끝나기를 기다리는 상한. 평소 1초 미만이라 5초면 정상 경로에는
/// 닿지 않는다. 이 값은 호출자 락의 최대 추가 보유 시간이 된다.
#[cfg(windows)]
const TASKKILL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// `child`가 `limit` 안에 끝나면 `true`. 넘기거나 상태 조회가 실패하면 `kill()` 후
/// 수거하고 `false`를 돌려준다. 폴링 간격이 짧아(20ms) 빨리 끝나는 자식에 지연이 없다.
/// (Windows에서만 쓰지만 로직이 플랫폼 중립이라 유닉스 테스트로 검증한다.)
#[cfg(any(windows, test))]
fn wait_or_kill(child: &mut std::process::Child, limit: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

// ---------------- 존재 확인용 프로브(상한 대기) ----------------
/// 자식을 띄워 상한 안에 끝나는지만 본다. 끝나면 `true`(종료 코드는 보지 않는다 —
/// 실행 자체가 가능했다는 뜻), spawn 실패이거나 상한을 넘기면 자식을 정리하고 `false`.
///
/// `dev_tools::process::run_process_with_timeout*`를 재사용하지 않는 이유(이 호출은
/// "이 이름이 실행되는가"만 알면 된다): (1) 그쪽은 stdout/stderr를 파이프로 받아
/// 리더 스레드가 상한 없는 `Vec`에 쌓는다 — 버릴 출력을 위해 스레드와 메모리를
/// 쓸 이유가 없어 여기서는 세 표준 스트림을 전부 null로 연결한다(파이프를 물고
/// 있는 자손 때문에 기다릴 일도 사라진다). (2) 그쪽은 PATH를 인자로 덮어쓰는데
/// 이 프로브는 상속된 PATH 그대로의 이름 해석이 목적이다. (3) 그쪽의 정리는
/// SIGTERM→3초 유예→SIGKILL + 리더 수거 2초라 호출자 락의 추가 보유 시간이
/// `limit`에 최대 5초가 더해진다 — 프로브는 곧바로 SIGKILL하고 수거도 `REAP_LIMIT`로
/// 끊는다. (4) 모듈 의존 방향: `cli_launcher`(저수준 유틸)가 `dev_tools`(상위)를
/// 거꾸로 부르지 않고 이미 쓰는 `process_util`에 머문다.
///
/// 유닉스에서는 새 프로세스 그룹으로 띄워 상한 초과 시 그룹 전체를 SIGKILL한다
/// (셸 shim 뒤의 손자까지 정리). Windows는 직속 자식만 `kill()`한다 — 이 프로브를
/// 쓰는 `cli_launcher::resolve_binary`의 이름 프로브는 macOS 분기에서만 실행된다.
pub fn command_completes_within(command: &mut std::process::Command, limit: std::time::Duration) -> bool {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .silent();
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => break,
        }
    }

    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    // 수거도 상한을 둔다: 커널 대기(D 상태)에 걸린 자식을 무기한 `wait()`하면 이 함수를
    // 부른 쪽 락이 영구히 잡힌다. 못 거둬도 좀비 하나가 남을 뿐이다.
    let reap_deadline = Instant::now() + PROBE_REAP_LIMIT;
    while Instant::now() < reap_deadline {
        if matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// `command_completes_within`이 kill 뒤 자식 수거를 기다리는 최대 시간.
const PROBE_REAP_LIMIT: std::time::Duration = std::time::Duration::from_secs(1);

// ---------------- Windows 콘솔 창 억제 ----------------
// GUI(콘솔 없는) 부모 프로세스가 콘솔 서브프로세스(claude/gh/wrangler 등)를
// spawn하면 Windows는 그 자식을 위해 새 콘솔 창을 자동으로 띄운다(부모에게
// 콘솔이 없으므로 자식이 붙을 콘솔을 새로 만드는 것). 배경에서 조용히 실행돼야
// 하는 호출부(버전 조회·상태 조회·claude 턴 실행 등)마다 이 대응을 19곳에
// 복붙하지 않도록, `Command`에 `.silent()`를 추가하는 확장 trait 하나로
// 모은다. Windows가 아닌 플랫폼에서는 완전한 no-op이라 크로스 플랫폼 호출부가
// 분기 없이 그대로 체이닝할 수 있다.
//
// 적용 대상이 아닌 것: 사용자가 "직접 보고 조작하도록" 여는 창(예:
// `cli_launcher::open_terminal_command`가 여는 macOS Terminal.app) — 그런
// 호출부는 이 trait을 쓰지 않는다.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------------- Windows 콘솔 창 명시(터미널 열기 전용) ----------------
// dev_tools 설계 §E: 사용자가 직접 보고 조작하도록 여는 터미널 창(PowerShell)은
// `silent()`의 정반대가 필요하다 — GUI 부모가 콘솔 자식을 띄우면 Windows가
// 콘솔을 자동 생성하므로 `.silent()`를 안 붙이기만 해도 창이 뜨지만, 암묵적
// 동작에 기대지 않고 `CREATE_NEW_CONSOLE`을 명시한다. `f983216`(`CREATE_NO_WINDOW`
// 도입)이 `cli_launcher::open_terminal_command` 경로를 의도적으로 제외해 둔
// 자리를 채우는 것이지, 그 결정을 뒤집는 것이 아니다(process_util.rs:41-45의
// 기존 주석이 이미 이 예외를 명시해 두었다).
#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// 백그라운드로 조용히 실행할 자식 프로세스에 붙이는 확장.
pub trait SilentCommand {
    /// Windows에서 `CREATE_NO_WINDOW`를 적용해 콘솔 창이 뜨지 않게 한다.
    /// 다른 플랫폼에서는 아무 일도 하지 않는다.
    fn silent(&mut self) -> &mut Self;

    /// Windows에서 `CREATE_NEW_CONSOLE`을 적용해 사용자가 보고 조작할 새 콘솔
    /// 창을 명시적으로 띄운다. 다른 플랫폼에서는 아무 일도 하지 않는다(그
    /// 플랫폼들은 애초에 별도 터미널 앱을 여는 방식이 다르다 —
    /// `cli_launcher::open_terminal_command`의 macOS 분기가 `osascript`로
    /// Terminal.app을 연다).
    fn windowed(&mut self) -> &mut Self;
}

impl SilentCommand for std::process::Command {
    #[cfg(windows)]
    fn silent(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(CREATE_NO_WINDOW)
    }

    #[cfg(not(windows))]
    fn silent(&mut self) -> &mut Self {
        self
    }

    #[cfg(windows)]
    fn windowed(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(CREATE_NEW_CONSOLE)
    }

    #[cfg(not(windows))]
    fn windowed(&mut self) -> &mut Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 상한을 넘긴 자식은 `kill()`되어 상한 부근에서 반환하고(`false`), 상한 안에
    /// 끝나는 자식은 기다리지 않고 바로 반환한다(`true`). Windows의 `taskkill`
    /// 대기가 쓰는 `wait_or_kill`의 로직 검증 — 플랫폼 중립이라 유닉스 자식으로 본다.
    #[cfg(unix)]
    #[test]
    fn wait_or_kill_bounds_a_hanging_child_and_returns_fast_for_a_quick_one() {
        use std::time::{Duration, Instant};

        let mut hanging = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("보조 프로세스(sleep)를 띄우지 못했습니다");
        let pid = hanging.id();
        let started = Instant::now();
        let finished = wait_or_kill(&mut hanging, Duration::from_millis(300));
        let elapsed = started.elapsed();
        assert!(!finished, "상한을 넘긴 자식은 false여야 한다");
        assert!(
            elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(3),
            "상한 부근에서 반환해야 한다: {elapsed:?}"
        );
        assert!(
            !pid_alive(pid),
            "상한을 넘긴 자식은 kill되고 수거되어 있어야 한다"
        );

        let mut quick = std::process::Command::new("/usr/bin/true")
            .spawn()
            .expect("보조 프로세스(true)를 띄우지 못했습니다");
        let started = Instant::now();
        assert!(wait_or_kill(&mut quick, Duration::from_secs(10)));
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "빨리 끝나는 자식을 상한까지 기다리면 안 된다: {:?}",
            started.elapsed()
        );
    }

    /// 존재 확인 프로브: 끝나는 자식은 `true`(종료 코드 무관), 멈추는 자식은 상한 부근에서
    /// `false`로 반환하고 자식이 정리되며, 없는 프로그램은 `false`다.
    #[cfg(unix)]
    #[test]
    fn command_completes_within_bounds_a_hanging_child_and_cleans_it_up() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        let started = Instant::now();
        assert!(command_completes_within(
            &mut Command::new("/usr/bin/true"),
            Duration::from_secs(10)
        ));
        assert!(
            command_completes_within(
                Command::new("/bin/sh").args(["-c", "exit 7"]),
                Duration::from_secs(10)
            ),
            "종료 코드가 0이 아니어도 '실행되었다'는 뜻이므로 true"
        );
        assert!(started.elapsed() < Duration::from_secs(3), "빨리 끝나는 자식을 상한까지 기다리면 안 된다");

        assert!(!command_completes_within(
            &mut Command::new("/definitely/not/a/real/program"),
            Duration::from_secs(10)
        ));

        // 손자(sleep)가 stdout을 물고 남는 셸 shim을 흉내낸다: 셸이 손자를 낳고 기다린다.
        let pid_file = std::env::temp_dir().join(format!(
            "malgn_vscode_probe_pid_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let started = Instant::now();
        let finished =
            command_completes_within(Command::new("/bin/sh").args(["-c", &script]), Duration::from_millis(400));
        let elapsed = started.elapsed();
        assert!(!finished, "상한을 넘긴 자식은 false여야 한다");
        assert!(
            elapsed >= Duration::from_millis(350) && elapsed < Duration::from_secs(3),
            "상한 부근에서 반환해야 한다: {elapsed:?}"
        );

        let grandchild: u32 = std::fs::read_to_string(&pid_file)
            .expect("손자 pid 파일")
            .trim()
            .parse()
            .expect("손자 pid 파싱");
        let _ = std::fs::remove_file(&pid_file);
        // 프로세스 그룹 kill이면 손자도 죽는다(init이 거두는 시간을 잠시 기다린다).
        let deadline = Instant::now() + Duration::from_secs(3);
        while pid_alive(grandchild) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!pid_alive(grandchild), "손자 프로세스까지 정리되어야 한다");
    }

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

    // 크로스플랫폼(unix에는 원래부터 있던 테스트, windows는 이번에 pid_alive를
    // 실구현으로 바꾸며 같은 전제가 성립하게 됐다 — u32::MAX-1은 Windows
    // pid 공간에서도 실제 배정 가능성이 사실상 없어 `OpenProcess`가
    // ERROR_INVALID_PARAMETER로 실패하고, 위 문서화한 정책대로 `false`를
    // 반환해야 한다).
    #[test]
    fn very_unlikely_pid_is_not_alive() {
        // i32::MAX에 가까운 pid는 실제 OS가 배정할 가능성이 사실상 없다.
        assert!(!pid_alive(u32::MAX - 1));
    }

    /// Windows 대응(unix의 `spawned_child_is_alive_then_dead_after_reaped`와
    /// 동일한 구조): 실제로 프로세스를 띄우고, 살아있음을 확인한 뒤,
    /// 종료·회수하고 죽었음을 확인한다. "죽은 pid"를 임의로 지어내지 않고
    /// 우리가 직접 spawn→kill→wait한 실제 프로세스를 쓴다(위임 지시의
    /// 권고 — 취약한 단정 회피). `powershell.exe`는 GitHub Actions
    /// `windows-latest` 러너에 기본 내장돼 있다. 이 테스트는 `#[cfg(windows)]`라
    /// 이 저장소의 macOS 개발 머신에서는 컴파일조차 되지 않는다 — Windows
    /// CI(`windows-latest`, debug/release 양쪽)만 이 로직을 실제로 검증한다.
    #[cfg(windows)]
    #[test]
    fn spawned_child_is_alive_then_dead_after_reaped_on_windows() {
        let mut child = std::process::Command::new("powershell.exe")
            .args(["-NoLogo", "-NoProfile", "-Command", "Start-Sleep -Seconds 5"])
            .spawn()
            .expect("보조 프로세스(powershell Start-Sleep)를 띄우지 못했습니다");
        let pid = child.id();

        assert!(pid_alive(pid), "막 띄운 자식 프로세스는 살아있어야 합니다");

        child.kill().expect("자식 프로세스 종료 실패");
        child.wait().expect("자식 프로세스 회수(wait) 실패");

        assert!(
            !pid_alive(pid),
            "종료되고 회수(wait)된 자식 프로세스는 죽은 것으로 판정되어야 합니다"
        );
    }

    /// `.silent()`를 체이닝해도 정상적으로 spawn·실행된다 — 크로스플랫폼에서
    /// (Windows는 `CREATE_NO_WINDOW`를 실제로 적용하고, 그 외 플랫폼은
    /// no-op이라) 회귀 없이 통과해야 한다. Windows에서 플래그 비트 자체를
    /// 검증하려면 자식의 콘솔 유무를 관찰해야 하는데 CI 환경에 따라
    /// 신뢰하기 어려워, 여기서는 "이 확장을 붙여도 정상 동작한다"는 계약만
    /// 고정한다.
    #[cfg(unix)]
    #[test]
    fn silent_does_not_break_spawning_on_unix() {
        let output = std::process::Command::new("echo")
            .arg("ok")
            .silent()
            .output()
            .expect("echo 실행 실패");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
    }

    #[cfg(windows)]
    #[test]
    fn silent_does_not_break_spawning_on_windows() {
        let output = std::process::Command::new("cmd")
            .args(["/C", "echo ok"])
            .silent()
            .output()
            .expect("cmd 실행 실패");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
    }

    /// 신규: `.windowed()`도 `.silent()`와 같은 계약(체이닝해도 정상 spawn·실행)을
    /// 지켜야 한다. non-Windows에서는 no-op이라 회귀 없이 통과해야 한다.
    #[cfg(unix)]
    #[test]
    fn windowed_does_not_break_spawning_on_unix() {
        let output = std::process::Command::new("echo")
            .arg("ok")
            .windowed()
            .output()
            .expect("echo 실행 실패");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
    }

    #[cfg(windows)]
    #[test]
    fn windowed_does_not_break_spawning_on_windows() {
        let output = std::process::Command::new("cmd")
            .args(["/C", "echo ok"])
            .windowed()
            .output()
            .expect("cmd 실행 실패");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ok");
    }
}
