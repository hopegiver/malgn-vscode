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
use super::plan_table::Runner;
use super::platform::{platform_now, Platform};
use super::process::ProcessRunOutput;
use super::DevTool;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 설치 계획의 리터럴 argv 토큰(plan_table.rs RUN_APP_NODE_*). 실제 설치 버전은 동적 값이라
/// argv가 아니라 `bind_target`으로 plan_id에만 묶는다.
pub(crate) const INSTALL_ARG: &str = "install-lts";

/// 사용자에게 보이는 설치 주체 라벨(`installer_label`, 메시지에 "…(으)로 설치되었습니다"로 쓰임).
pub(crate) const INSTALLER_LABEL: &str = "맑은에이전트";

/// npm 전역 prefix로 설치된 Claude Code의 위치(`~/` 표기). `claude`를 직접 찾는 모든 후보 목록
/// (dev_tools DEV_TOOLS · plugins · autonomy · mcp_manager · session_chat · claude_auth)이 이
/// 값을 포함해야 앱이 설치한 Claude를 앱의 모든 기능이 찾는다 — `claude_candidate_lists_all_
/// include_the_app_npm_global_location`(아래 테스트)이 여섯 목록의 누락을 잡는다.
pub(crate) const NPM_GLOBAL_CLAUDE_CANDIDATE: &str = "~/.local/share/malgn-agent/npm-global/bin/claude";

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

// ---------------- npm 전역 prefix (P2-3b) ----------------

/// 앱이 설치한 Node의 `npm`을 자식 프로세스로 실행할 때 주입할 환경변수. 앱 관리 Node의 기본
/// 전역 prefix는 `execPath` 기준 버전 폴더 안(`node/<버전>/lib/node_modules`)이라 `current`를
/// 새 버전으로 전환하면 전역 패키지가 사라진다(실측) — 그래서 `npm_config_prefix`를 node/ 밖
/// 형제 폴더(`npm-global`)로 돌린다. env로만 주입하므로 `~/.npmrc`는 바뀌지 않고, env가
/// 사용자 npmrc의 `prefix=`보다 우선한다(실측). 다른 npm(Homebrew 등)이나 다른 러너에는
/// 아무것도 주입하지 않는다.
pub(crate) fn npm_prefix_env_for(
    layout: Option<&Layout>,
    runner: Runner,
    runner_path: &str,
) -> Vec<(String, String)> {
    let Some(layout) = layout else { return Vec::new() };
    if runner != Runner::Npm || !layout.contains_node_path(Path::new(runner_path)) {
        return Vec::new();
    }
    vec![(
        "npm_config_prefix".to_string(),
        layout.npm_global().to_string_lossy().to_string(),
    )]
}

pub(crate) fn npm_prefix_env(runner: Runner, runner_path: &str) -> Vec<(String, String)> {
    if platform_now() != Platform::Mac {
        return Vec::new();
    }
    npm_prefix_env_for(current_layout().as_ref(), runner, runner_path)
}

/// 이 runner_path가 앱 관리 Node 아래인가(쓰기권한 검사를 npm 전역 prefix 기준으로 바꾸는 데 쓴다).
pub(crate) fn is_app_managed_runner(layout: Option<&Layout>, runner_path: &str) -> bool {
    layout.map(|l| l.contains_node_path(Path::new(runner_path))).unwrap_or(false)
}

/// `canonical` 경로의 도구가 앱의 npm 전역 prefix 안에 있으면(= 앱 npm으로 설치된 전역 패키지)
/// 그 갱신에 써야 할 앱 관리 `npm` 경로를 돌려준다. Homebrew npm이 먼저 잡히는 기기에서도
/// 앱 prefix의 패키지는 앱 npm으로 갱신해 "두 번째 사본이 다른 prefix에 설치되는" 어긋남을 막는다.
pub(crate) fn app_npm_for_global_package(layout: Option<&Layout>, canonical: &Path) -> Option<String> {
    let layout = layout?;
    if !layout.contains_npm_global_path(canonical) {
        return None;
    }
    let npm = layout.node_bin_dir().join("npm");
    super::platform::path_exists(&npm.to_string_lossy()).then(|| npm.to_string_lossy().to_string())
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

    // ── P2-3b ──

    #[test]
    fn npm_prefix_is_injected_only_for_the_app_managed_npm() {
        let layout = Layout::for_home(Path::new("/Users/x"));
        let app_npm = "/Users/x/.local/share/malgn-agent/node/current/bin/npm";
        let injected = npm_prefix_env_for(Some(&layout), Runner::Npm, app_npm);
        assert_eq!(
            injected,
            vec![("npm_config_prefix".to_string(), "/Users/x/.local/share/malgn-agent/npm-global".to_string())]
        );
        // Homebrew/시스템 npm은 건드리지 않는다.
        for other in ["/opt/homebrew/bin/npm", "/usr/local/bin/npm", "/Users/x/.npm-global/bin/npm", "npm"] {
            assert!(npm_prefix_env_for(Some(&layout), Runner::Npm, other).is_empty(), "{other}");
        }
        // 이름만 비슷한 형제 폴더(성분 단위 비교)도 아니다.
        assert!(npm_prefix_env_for(Some(&layout), Runner::Npm, "/Users/x/.local/share/malgn-agent/node-evil/bin/npm").is_empty());
        // 다른 러너(pnpm·brew)는 같은 폴더 아래여도 주입하지 않는다.
        assert!(npm_prefix_env_for(Some(&layout), Runner::Pnpm, app_npm).is_empty());
        assert!(npm_prefix_env_for(Some(&layout), Runner::Brew, app_npm).is_empty());
        assert!(npm_prefix_env_for(None, Runner::Npm, app_npm).is_empty());
    }

    #[test]
    fn npm_prefix_lives_outside_the_versioned_node_dir_so_it_survives_node_upgrades() {
        let layout = Layout::for_home(Path::new("/Users/x"));
        let prefix = layout.npm_global();
        assert!(!prefix.starts_with(layout.node_root()), "전역 prefix가 node/ 안이면 current 전환 때 패키지가 사라진다");
        assert_eq!(prefix.parent(), layout.node_root().parent(), "node/와 형제 폴더");
    }

    #[test]
    fn app_npm_is_used_for_packages_living_in_the_app_npm_global_prefix() {
        let tmp = std::env::temp_dir().join(format!("r22-app-npm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let layout = Layout::for_home(&tmp);
        std::fs::create_dir_all(layout.node_root().join("v24.22.0").join("bin")).unwrap();
        std::fs::write(layout.node_root().join("v24.22.0").join("bin").join("npm"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("v24.22.0", layout.current_link()).unwrap();
        let pkg = layout.npm_global().join("lib").join("node_modules").join("@anthropic-ai").join("claude-code");
        let found = app_npm_for_global_package(Some(&layout), &pkg);
        #[cfg(unix)]
        assert_eq!(found, Some(layout.node_bin_dir().join("npm").to_string_lossy().to_string()));
        // 다른 위치의 패키지(Homebrew 등)는 해당 없음.
        assert_eq!(app_npm_for_global_package(Some(&layout), Path::new("/opt/homebrew/lib/node_modules/pnpm")), None);
        assert_eq!(app_npm_for_global_package(None, &pkg), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // 도구 위치 지식이 여러 곳에 복붙된 `claude` 후보 목록은 하나라도 빠지면 "앱이 설치한 Claude를
    // 어떤 기능은 못 찾는" 조용한 회귀가 된다(dev_tools 상수 비공유 관례 때문에 목록이 여섯 곳).
    // src 전체를 훑어 `claude` mac 후보 목록을 가진 파일은 전부 이 상수를 참조해야 한다 — 일곱 번째
    // 목록이 생겨도 자동으로 걸린다.
    #[test]
    fn every_claude_candidate_list_includes_the_app_npm_global_location() {
        fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().map(|e| e == "rs").unwrap_or(false) {
                    out.push(p);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&root, &mut files);
        // cli_launcher.rs는 후보 목록이 아니라 확장 동작 테스트에서 경로 문자열을 예로 쓸 뿐이다.
        let allow_listed = ["cli_launcher.rs", "app_node.rs"];
        let mut checked = Vec::new();
        for f in files {
            let name = f.file_name().unwrap().to_string_lossy().to_string();
            if allow_listed.contains(&name.as_str()) || name.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&f).unwrap();
            if text.contains("\"/opt/homebrew/bin/claude\"") && text.contains("\"~/.local/bin/claude\"") {
                assert!(
                    text.contains("NPM_GLOBAL_CLAUDE_CANDIDATE"),
                    "{}의 claude 후보 목록에 app_node::NPM_GLOBAL_CLAUDE_CANDIDATE가 없습니다 — 앱이 npm 전역 prefix에 설치한 Claude를 이 기능이 못 찾습니다",
                    f.display()
                );
                checked.push(name);
            }
        }
        assert!(checked.len() >= 6, "claude 후보 목록 파일을 충분히 찾지 못했습니다(가드가 공허해짐): {checked:?}");
    }
}
