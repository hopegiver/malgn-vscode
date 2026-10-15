// ==================== 앱 내장 Node 설치 러너(Runner::AppNode) 어댑터 ====================
// `node_install`(주입 가능한 설치 파이프라인)을 dev_tools의 계획/실행 틀(plan_id 게이트 ·
// 미리보기 · 실행 후 버전 재조회)에 끼우는 얇은 접착층이다. 다른 러너(brew/npm/…)는 외부
// 프로세스를 띄우지만 이 러너는 앱 안에서 직접 설치를 수행하므로, 실행 결과를 기존 실행기와
// 같은 모양(`ProcessRunOutput`)으로 바꿔 돌려준다 — 그러면 성공 판정(실행 전후 버전 재조회
// 델타, claimed ≠ verified)과 결과 메시지 조립을 기존 코드가 그대로 쓴다.
//
// 동의 ↔ 실행 일치: 설치 버전은 "미리보기 시점에 index.json으로 결정한 값"이고, 그 값이
// plan_id 해시에 들어간다(`bind_target`). 실행 직전에 같은 방식으로 다시 결정해 plan_id가
// 어긋나면(그 사이 새 LTS가 나옴) 재미리보기를 요구하므로, 사용자가 본 버전과 설치되는
// 버전이 갈라지지 않는다.
//
// macOS 전용 기능이다: Windows에서는 `runner_path()`가 항상 None이라 이 러너가 후보에서
// 빠지고 기존 winget 경로가 그대로 동작한다(회귀 없음).

use super::contract::DevToolPreview;
use super::diagnostics::compute_plan_id;
use super::node_install::{self, InstallEnv, InstallError, Layout, NodeArch, NodeVersion, RealEnv};
use super::platform::{platform_now, Platform};
use super::process::ProcessRunOutput;
use super::DevTool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 설치 계획의 리터럴 argv 토큰(plan_table.rs RUN_APP_NODE_*). 실제 설치 버전은 동적 값이라
/// argv가 아니라 `bind_target`으로 plan_id에만 묶는다.
pub(crate) const INSTALL_ARG: &str = "install-lts";

/// 사용자에게 보이는 설치 주체 라벨(`installer_label`, 메시지에 "…(으)로 설치되었습니다"로 쓰임).
pub(crate) const INSTALLER_LABEL: &str = "맑은에이전트";

// ---------------- 취소 ----------------

static CANCEL: AtomicBool = AtomicBool::new(false);

/// 진행 중인 설치를 취소한다(앱 종료 · `cancel_node_install` 커맨드). 다음 설치는 시작할 때
/// 플래그를 지우므로 취소가 이후 설치를 막지 않는다.
pub(crate) fn request_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn is_cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}

// ---------------- 배치(기준 폴더) ----------------

/// 이 PC 사용자의 기준 폴더 배치. 홈을 알 수 없으면 `None`.
pub(crate) fn current_layout() -> Option<Layout> {
    dirs::home_dir().map(|home| Layout::for_home(&home))
}

/// `ResolvedRunners.app_node`의 값. 사용 가능한 플랫폼(macOS)이고 홈이 있을 때만 Some —
/// 값 자체는 "이 러너가 관리하는 node 폴더" 경로로, plan_id·표시용이다(실행 파일이 아니다).
pub(crate) fn runner_path() -> Option<String> {
    runner_path_for(platform_now(), current_layout().as_ref())
}

pub(crate) fn runner_path_for(plat: Platform, layout: Option<&Layout>) -> Option<String> {
    if plat != Platform::Mac {
        return None;
    }
    layout.map(|l| l.node_root().to_string_lossy().to_string())
}

// ---------------- 계획 ↔ 버전 묶기 ----------------

/// 지금 설치할(= 동의 화면에 보여줄) LTS 버전을 정한다. 네트워크 1회(index.json).
pub(crate) fn resolve_target_tag() -> Result<String, String> {
    resolve_target_tag_with(&RealEnv::new(), NodeArch::current(), &is_cancelled)
}

pub(crate) fn resolve_target_tag_with(
    env: &dyn InstallEnv,
    arch: Option<NodeArch>,
    cancel: &dyn Fn() -> bool,
) -> Result<String, String> {
    let arch = arch.ok_or_else(|| {
        InstallError::UnsupportedPlatform(format!("지원하지 않는 CPU 구조({})", std::env::consts::ARCH)).to_string()
    })?;
    node_install::resolve_latest(env, arch, cancel)
        .map(|v| v.to_tag())
        .map_err(|e| e.to_string())
}

/// plan_id 입력: 리터럴 argv 뒤에 결정된 대상 버전을 붙인다.
pub(crate) fn bind_target(args: &[String], target_tag: &str) -> Vec<String> {
    let mut bound = args.to_vec();
    bound.push(target_tag.to_string());
    bound
}

pub(crate) fn command_display(target_tag: &str) -> String {
    format!("nodejs.org 공식 배포본 Node.js {target_tag} 설치 (관리자 권한 불필요)")
}

fn install_dir_display(target_tag: &str) -> String {
    format!("{}/node/{target_tag}", super::node_install::BASE_TILDE)
}

// ---------------- 미리보기 ----------------

/// 미설치 → 설치 미리보기. 버전을 못 정하면 `Err`(화면이 "미리보기를 가져오지 못했습니다"로 알린다).
pub(crate) fn build_install_preview(
    def: &DevTool,
    runner_path: &str,
    argv: &[String],
) -> Result<DevToolPreview, String> {
    let tag = resolve_target_tag().map_err(|e| format!("최신 LTS 버전을 확인하지 못했습니다 — {e}"))?;
    Ok(DevToolPreview {
        id: def.key.to_string(),
        plan_id: compute_plan_id(runner_path, &bind_target(argv, &tag), ""),
        will_run: true,
        command_display: command_display(&tag),
        affected: vec![def.label.to_string()],
        notes: format!(
            "https://nodejs.org/dist/ 에서 Node.js {tag}(LTS)을 내려받아 공식 체크섬(SHA256)과 Apple 코드서명(OpenJS Foundation, Team ID HX7739G8FX)을 확인한 뒤 {}에 설치합니다. 약 50MB를 받으며 관리자 권한이 필요 없고 셸 설정·~/.npmrc는 바꾸지 않습니다. 확인에 실패하면 아무것도 설치하지 않습니다.",
            install_dir_display(&tag)
        ),
        preview_reliable: true,
    })
}

// ---------------- 실행 ----------------

/// 고정(pinned)된 `target_tag`를 설치한다. 결과는 기존 실행기와 같은 `ProcessRunOutput`:
/// 성공 = exit 0 + 단계 로그(stdout), 실패 = exit 1 + 사유(stderr), 취소 = `aborted`.
pub(crate) fn run_pinned(target_tag: &str) -> ProcessRunOutput {
    CANCEL.store(false, Ordering::SeqCst);
    match (current_layout(), NodeArch::current(), NodeVersion::parse_tag(target_tag)) {
        (Some(layout), Some(arch), Some(version)) => {
            run_pinned_with(&RealEnv::new(), &layout, arch, version, &is_cancelled)
        }
        (None, _, _) => failed_output(
            InstallError::Io("홈 디렉터리를 알 수 없습니다".to_string()).to_string(),
            false,
        ),
        (_, None, _) => failed_output(
            InstallError::UnsupportedPlatform(format!("지원하지 않는 CPU 구조({})", std::env::consts::ARCH)).to_string(),
            false,
        ),
        (_, _, None) => failed_output(InstallError::BadVersion(target_tag.to_string()).to_string(), false),
    }
}

pub(crate) fn run_pinned_with(
    env: &dyn InstallEnv,
    layout: &Layout,
    arch: NodeArch,
    version: NodeVersion,
    cancel: &dyn Fn() -> bool,
) -> ProcessRunOutput {
    let started = Instant::now();
    match node_install::install_version(env, layout, arch, version, cancel) {
        Ok(report) => ProcessRunOutput {
            exit_code: Some(0),
            stdout: {
                let mut lines = vec![format!(
                    "Node.js {}{}",
                    report.version.to_tag(),
                    if report.already_installed { " — 이미 온전히 설치되어 있어 변경하지 않았습니다" } else { "" }
                )];
                if let Some(prev) = report.previous.filter(|p| *p != report.version) {
                    lines.push(format!("이전 버전 {}은(는) 한 단계 롤백용으로 남겨 둡니다.", prev.to_tag()));
                }
                lines.extend(report.log);
                lines.join("\n")
            },
            stderr: String::new(),
            timed_out: false,
            aborted: false,
            duration: started.elapsed(),
            spawn_error: None,
        },
        Err(e) => {
            let mut out = failed_output(e.to_string(), e.is_cancelled());
            out.duration = started.elapsed();
            out
        }
    }
}

fn failed_output(message: String, cancelled: bool) -> ProcessRunOutput {
    ProcessRunOutput {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: message,
        timed_out: false,
        aborted: cancelled,
        duration: Duration::ZERO,
        spawn_error: None,
    }
}

/// 실패/취소 결과를 사용자 메시지로(액션 결과의 `message`).
pub(crate) fn failure_message(output: &ProcessRunOutput) -> String {
    if output.aborted {
        "Node.js 설치가 취소되었습니다. 기존 설치는 그대로입니다.".to_string()
    } else {
        format!("Node.js 설치에 실패했습니다 — {}", output.stderr.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    #[test]
    fn runner_path_exists_only_on_mac_with_a_home() {
        let layout = Layout::for_home(Path::new("/Users/x"));
        assert_eq!(
            runner_path_for(Platform::Mac, Some(&layout)),
            Some("/Users/x/.local/share/malgn-agent/node".to_string())
        );
        assert_eq!(runner_path_for(Platform::Win, Some(&layout)), None, "Windows는 이 러너를 쓰지 않는다");
        assert_eq!(runner_path_for(Platform::Mac, None), None);
    }

    #[test]
    fn plan_id_is_bound_to_the_target_version() {
        let args = vec![INSTALL_ARG.to_string()];
        let a = compute_plan_id("/r", &bind_target(&args, "v24.22.0"), "");
        let b = compute_plan_id("/r", &bind_target(&args, "v24.23.0"), "");
        let c = compute_plan_id("/r", &args, "");
        assert_ne!(a, b, "대상 버전이 다르면 plan_id도 달라야 합니다(동의-실행 일치)");
        assert_ne!(a, c);
        assert_eq!(a, compute_plan_id("/r", &bind_target(&args, "v24.22.0"), ""), "같은 입력은 안정적");
    }

    /// 항상 네트워크 오류를 내는 환경 — 실패/취소가 `ProcessRunOutput`으로 올바르게 옮겨지는지 본다.
    struct DeadEnv;
    impl InstallEnv for DeadEnv {
        fn http_get(
            &self,
            _url: &str,
            _max: u64,
            _sink: &mut dyn Write,
            _cancel: &dyn Fn() -> bool,
        ) -> Result<(), node_install::FetchErrorForTests> {
            Err(node_install::FetchErrorForTests::Network("연결 거부".to_string()))
        }
        fn create_file(&self, _p: &Path) -> std::io::Result<Box<dyn Write>> {
            Err(std::io::Error::other("unused"))
        }
        fn rename(&self, _a: &Path, _b: &Path) -> std::io::Result<()> {
            Ok(())
        }
        fn run_tool(&self, _p: &str, _a: &[String], _t: Duration, _c: &dyn Fn() -> bool) -> node_install::ToolRunForTests {
            Default::default()
        }
        fn free_space(&self, _p: &Path) -> Option<u64> {
            None
        }
    }

    #[test]
    fn failed_install_maps_to_a_failed_run_output_with_the_reason() {
        let home = std::env::temp_dir().join(format!("r22-app-node-{}", std::process::id()));
        let layout = Layout::for_home(&home);
        let v = NodeVersion::parse_tag("v24.22.0").unwrap();
        let out = run_pinned_with(&DeadEnv, &layout, NodeArch::Arm64, v, &|| false);
        assert_eq!(out.exit_code, Some(1));
        assert!(!out.aborted);
        assert!(out.stderr.contains("nodejs.org"), "{}", out.stderr);
        assert!(failure_message(&out).starts_with("Node.js 설치에 실패했습니다"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn cancelled_install_maps_to_an_aborted_run_output() {
        let home = std::env::temp_dir().join(format!("r22-app-node-cancel-{}", std::process::id()));
        let layout = Layout::for_home(&home);
        let v = NodeVersion::parse_tag("v24.22.0").unwrap();
        let out = run_pinned_with(&DeadEnv, &layout, NodeArch::Arm64, v, &|| true);
        assert!(out.aborted);
        assert_eq!(out.exit_code, Some(1));
        assert!(failure_message(&out).contains("취소"));
        assert!(!layout.base.exists(), "취소는 아무 흔적도 남기지 않아야 합니다");
    }
}
