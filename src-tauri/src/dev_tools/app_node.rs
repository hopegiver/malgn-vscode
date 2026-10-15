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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 설치 계획의 리터럴 argv 토큰(plan_table.rs RUN_APP_NODE_*). 실제 설치 버전은 동적 값이라
/// argv가 아니라 `bind_target`으로 plan_id에만 묶는다.
pub(crate) const INSTALL_ARG: &str = "install-lts";
/// 업데이트 계획의 리터럴 argv 토큰(plan_table.rs RUN_APP_NODE_UPDATE).
pub(crate) const UPDATE_ARG: &str = "update-lts";

/// 사용자에게 보이는 설치 주체 라벨(`installer_label`, 메시지에 "…(으)로 설치되었습니다"로 쓰임).
pub(crate) const INSTALLER_LABEL: &str = "맑은에이전트";

/// npm 전역 prefix로 설치된 Claude Code의 위치(`~/` 표기). `claude`를 직접 찾는 모든 후보 목록
/// (dev_tools DEV_TOOLS · plugins · autonomy · mcp_manager · session_chat · claude_auth)이 이
/// 값을 포함해야 앱이 설치한 Claude를 앱의 모든 기능이 찾는다 — `claude_candidate_lists_all_
/// include_the_app_npm_global_location`(아래 테스트)이 여섯 목록의 누락을 잡는다.
pub(crate) const NPM_GLOBAL_CLAUDE_CANDIDATE: &str = "~/.local/share/malgn-agent/npm-global/bin/claude";

// ---------------- 취소 ----------------

/// 세대 카운터 방식의 취소 스위치. "취소됨"을 불리언으로 들고 있으면 누가 언제 내리는지가
/// 문제가 된다(내리는 곳이 한 곳뿐이면 그 전 단계가 영구히 실패하고, 진행 중 작업이 없을 때의
/// 취소가 다음 작업까지 막는다). 그래서 상태를 두지 않고 세대 번호만 올린다:
///   - 작업은 시작할 때 현재 세대를 기억한 토큰(`token()`)을 받는다.
///   - `cancel()`은 세대를 1 올린다 — 이미 시작한 토큰은 "취소됨"이 되고, 그 뒤에 시작하는
///     토큰은 새 세대를 기억하므로 영향이 없다.
///   - 진행 중인 작업이 없을 때의 `cancel()`은 아무 토큰도 취소하지 않아 무해하다.
/// 따라서 되돌리는(리셋) 코드가 필요 없고, 취소는 호출 시점에 진행 중이던 작업만 멈춘다.
pub(crate) struct CancelSwitch {
    generation: AtomicU64,
}

pub(crate) struct CancelToken<'a> {
    switch: &'a CancelSwitch,
    started_at: u64,
}

impl CancelSwitch {
    pub(crate) const fn new() -> Self {
        Self { generation: AtomicU64::new(0) }
    }

    pub(crate) fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn token(&self) -> CancelToken<'_> {
        CancelToken { switch: self, started_at: self.generation.load(Ordering::SeqCst) }
    }
}

impl CancelToken<'_> {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.switch.generation.load(Ordering::SeqCst) != self.started_at
    }
}

static CANCEL: CancelSwitch = CancelSwitch::new();

/// 호출 시점에 진행 중인 Node 설치·버전 확인을 취소한다(앱 종료 · `cancel_node_install`
/// 커맨드). 이후에 시작하는 미리보기·설치는 영향을 받지 않고, 진행 중인 작업이 없으면 아무
/// 일도 일어나지 않는다. 한계: 한 번의 실행이 "버전 결정 → 설치" 두 단계로 나뉘어 각자 토큰을
/// 받으므로, 두 단계 사이의 짧은 틈에 도착한 취소는 놓칠 수 있다(취소 UI를 붙일 때 같은
/// 토큰을 두 단계에 넘기도록 확장한다).
pub(crate) fn request_cancel() {
    CANCEL.cancel();
}

// ---------------- 배치(기준 폴더) ----------------

/// 이 PC 사용자의 기준 폴더 배치. 홈을 알 수 없으면 `None`.
pub(crate) fn current_layout() -> Option<Layout> {
    dirs::home_dir().map(|home| Layout::for_home(&home))
}

/// 앱 관리 배치가 의미 있는 플랫폼(macOS)에서만 Some. 쓰기권한 검사·러너 선택처럼 "이 PC에 앱
/// 관리 Node가 있을 수 있는가"를 묻는 곳은 이 함수를 쓴다 — Windows는 항상 None이라 기존
/// 동작이 그대로다.
pub(crate) fn mac_layout() -> Option<Layout> {
    if platform_now() == Platform::Mac {
        current_layout()
    } else {
        None
    }
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
    npm_prefix_env_for(mac_layout().as_ref(), runner, runner_path)
}

/// 앱이 띄우는 `claude` 자식 프로세스(세션 채팅·자율업무·MCP·플러그인·인증 조회)에 줄 환경변수.
/// `claude`가 앱의 npm 전역 prefix(`npm-global/…`)에서 실행되는 경우에만
/// `npm_config_prefix=<npm-global>`을 준다.
///
/// 이유: 앱 Node로 `npm i -g @anthropic-ai/claude-code`를 한 Claude는 자기 갱신 때 `npm install -g`를
/// 부르는데, 그 npm의 기본 prefix는 실행 파일 기준 `node/<버전>`이라(P2-3b 실측) 새 사본이 앱이
/// 찾는 `npm-global/bin/claude`가 아니라 `node/<버전>/bin/claude`로 떨어진다 — 앱은 옛 버전을
/// 계속 보고, Node가 갱신되면 그 사본은 정리로 사라진다. 갱신이 같은 prefix에 쓰이도록 개발 도구
/// 화면의 설치·갱신(`npm_prefix_env`)과 같은 값을 준다.
///
/// 조건이 "claude 실행 파일의 위치"인 이유: Homebrew/기존 Node의 npm 전역이나 네이티브 설치
/// (`~/.local/bin/claude`) Claude는 이 폴더 밖이라 아무것도 주입하지 않아 기존 동작 그대로다.
/// env로만 주입하므로 `~/.npmrc`·셸 rc는 바뀌지 않고, 앱이 띄운 프로세스 트리에만 미친다.
/// (사용자가 터미널에서 직접 띄우는 claude에는 닿지 않는다 — 그 환경의 npm 설정은 사용자 소유다.)
pub(crate) fn claude_child_env_for(layout: Option<&Layout>, claude_path: &str) -> Vec<(String, String)> {
    let Some(layout) = layout else { return Vec::new() };
    if !layout.contains_npm_global_path(Path::new(claude_path)) {
        return Vec::new();
    }
    vec![("npm_config_prefix".to_string(), layout.npm_global().to_string_lossy().to_string())]
}

/// `claude_child_env_for`의 실제 홈 기준 버전(macOS 외에서는 항상 빈 목록).
pub(crate) fn claude_child_env(claude_path: &str) -> Vec<(String, String)> {
    claude_child_env_for(mac_layout().as_ref(), claude_path)
}

/// 이 runner_path가 앱 관리 Node 아래인가(쓰기권한 검사를 npm 전역 prefix 기준으로 바꾸는 데 쓴다).
pub(crate) fn is_app_managed_runner(layout: Option<&Layout>, runner_path: &str) -> bool {
    layout.map(|l| l.contains_node_path(Path::new(runner_path))).unwrap_or(false)
}

/// 이 실행 파일이 앱의 npm 전역 prefix(`npm-global/…`) 안에 있는가. pnpm 러너 판정에 쓴다:
/// `npm i -g pnpm`으로 들어온 pnpm은 `pnpm add -g`가 전역 bin 디렉터리를 PATH에 요구하고
/// (`ERR_PNPM_GLOBAL_BIN_DIR_NOT_IN_PATH`, 샌드박스 실측) 그 설정(`pnpm setup`)은 셸 rc를
/// 수정한다 — 이 기능은 rc 불변이므로 그런 pnpm은 설치 러너로 쓰지 않고 npm으로 간다.
pub(crate) fn is_in_app_npm_global(layout: Option<&Layout>, path: &str) -> bool {
    layout.map(|l| l.contains_npm_global_path(Path::new(path))).unwrap_or(false)
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
    let token = CANCEL.token();
    resolve_target_tag_with(&RealEnv::new(), NodeArch::current(), &|| token.is_cancelled())
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
    Ok(install_preview_for_tag(def, runner_path, argv, &tag))
}

/// 설치됨 → 업데이트 미리보기. `installed_plain`은 `normalize_version`을 거친 현재 버전("24.21.0").
pub(crate) fn build_update_preview(
    def: &DevTool,
    runner_path: &str,
    argv: &[String],
    installed_plain: &str,
) -> Result<DevToolPreview, String> {
    let tag = resolve_target_tag().map_err(|e| format!("최신 LTS 버전을 확인하지 못했습니다 — {e}"))?;
    update_preview_for_tag(def, runner_path, argv, installed_plain, &tag)
}

fn install_preview_for_tag(def: &DevTool, runner_path: &str, argv: &[String], tag: &str) -> DevToolPreview {
    DevToolPreview {
        id: def.key.to_string(),
        plan_id: compute_plan_id(runner_path, &bind_target(argv, tag), ""),
        will_run: true,
        command_display: command_display(tag),
        affected: vec![def.label.to_string()],
        notes: format!(
            "https://nodejs.org/dist/ 에서 Node.js {tag}(LTS)을 내려받아 공식 체크섬(SHA256)과 Apple 코드서명(OpenJS Foundation, Team ID HX7739G8FX)을 확인한 뒤 {}에 설치합니다. 약 50MB를 받으며 관리자 권한이 필요 없고 셸 설정·~/.npmrc는 바꾸지 않습니다. 확인에 실패하면 아무것도 설치하지 않습니다.",
            install_dir_display(tag)
        ),
        preview_reliable: true,
    }
}

fn update_preview_for_tag(
    def: &DevTool,
    runner_path: &str,
    argv: &[String],
    installed_plain: &str,
    tag: &str,
) -> Result<DevToolPreview, String> {
    let target = NodeVersion::parse_tag(tag).ok_or_else(|| format!("내부 오류: 버전 형식 {tag}"))?;
    // 현재 버전을 읽지 못했거나 형식이 다르면 "새 버전이 있다"고 단정하지 않고 실행 가능으로 둔다
    // (실행해도 같은 버전이면 변경 없이 끝난다).
    let installed = NodeVersion::parse_plain(installed_plain);
    let newer_available = installed.map(|cur| cur < target).unwrap_or(true);
    let current_label = installed.map(|v| v.to_tag()).unwrap_or_else(|| installed_plain.to_string());
    let notes = if newer_available {
        format!(
            "새 LTS {tag}이(가) 있습니다(현재 {current_label}). nodejs.org 공식 배포본을 내려받아 SHA256과 코드서명(Team ID HX7739G8FX)을 확인한 뒤 {}에 설치하고 `current`를 새 버전으로 전환합니다. 이전 버전은 한 단계 보관하며 다음 업데이트 때 정리됩니다. 확인에 실패하면 현재 설치를 그대로 둡니다.",
            install_dir_display(tag)
        )
    } else {
        format!("이미 최신 LTS입니다(현재 {current_label}, 최신 {tag}). 실행해도 변경되지 않습니다.")
    };
    Ok(DevToolPreview {
        id: def.key.to_string(),
        plan_id: compute_plan_id(runner_path, &bind_target(argv, tag), installed_plain),
        will_run: newer_available,
        command_display: command_display(tag),
        affected: vec![def.label.to_string()],
        notes,
        preview_reliable: true,
    })
}

// ---------------- 실행 ----------------

/// 고정(pinned)된 `target_tag`를 설치한다. 결과는 기존 실행기와 같은 `ProcessRunOutput`:
/// 성공 = exit 0 + 단계 로그(stdout), 실패 = exit 1 + 사유(stderr), 취소 = `aborted`.
pub(crate) fn run_pinned(target_tag: &str) -> ProcessRunOutput {
    let token = CANCEL.token();
    match (current_layout(), NodeArch::current(), NodeVersion::parse_tag(target_tag)) {
        (Some(layout), Some(arch), Some(version)) => {
            run_pinned_with(&RealEnv::new(), &layout, arch, version, &|| token.is_cancelled())
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
                    if report.already_installed { ALREADY_INSTALLED_NOTE } else { "" }
                )];
                if let Some(prev) = report.previous.filter(|p| *p != report.version) {
                    lines.push(previous_kept_line(prev));
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

/// 멱등 경로의 결과 문구. 검사한 것만 말한다(서명·버전·npm 실행) — 파일 전체가 온전하다고
/// 단정하지 않는다(`check_npm_loads`는 npm이 로드되는 수준까지만 본다). 깨진 설치는 이 경로에
/// 오지 않고 다시 설치된다.
const ALREADY_INSTALLED_NOTE: &str = " — 이미 설치되어 있고 서명·버전·npm 실행이 확인되어 변경하지 않았습니다";

/// 업데이트 뒤 이전 버전을 남겨 두었다는 안내. 되돌리는(롤백) 액션은 없으므로 "롤백용"이라고
/// 약속하지 않고, 보관 사실과 정리 시점만 말한다(정리는 다음 설치의 `prune_versions`).
fn previous_kept_line(prev: NodeVersion) -> String {
    format!("이전 버전 {}은(는) 한 단계 보관합니다(다음 업데이트 때 정리됩니다).", prev.to_tag())
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

    #[cfg(unix)]
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

    /// 항상 네트워크 오류를 내는(취소 신호가 서 있으면 취소로 끝나는) 환경 — 실패/취소가 `ProcessRunOutput`으로 올바르게 옮겨지는지 본다.
    struct DeadEnv;
    impl InstallEnv for DeadEnv {
        fn http_get(
            &self,
            _url: &str,
            _max: u64,
            _sink: &mut dyn Write,
            cancel: &dyn Fn() -> bool,
        ) -> Result<(), node_install::FetchErrorForTests> {
            // 실제 HTTP 클라이언트처럼 취소 신호를 먼저 본다.
            if cancel() {
                return Err(node_install::FetchErrorForTests::Cancelled);
            }
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

    // ── RV-002: 앱이 설치한 claude의 자기 갱신이 앱이 찾는 위치(npm-global)에 떨어지게 한다 ──

    #[cfg(unix)]
    #[test]
    fn claude_child_env_points_updates_of_an_app_installed_claude_at_the_app_npm_prefix() {
        let layout = Layout::for_home(Path::new("/Users/x"));
        let expected = vec![("npm_config_prefix".to_string(), "/Users/x/.local/share/malgn-agent/npm-global".to_string())];
        // 앱 npm 전역에 설치된 claude(shim 경로와 패키지 실물 경로 모두).
        assert_eq!(
            claude_child_env_for(Some(&layout), "/Users/x/.local/share/malgn-agent/npm-global/bin/claude"),
            expected
        );
        assert_eq!(
            claude_child_env_for(
                Some(&layout),
                "/Users/x/.local/share/malgn-agent/npm-global/lib/node_modules/@anthropic-ai/claude-code/cli.js"
            ),
            expected
        );
        // 그 외 환경에는 아무것도 주입하지 않는다 — Homebrew/기존 Node의 npm 전역, 네이티브 설치,
        // 앱 Node 폴더 안, 이름만 비슷한 형제 폴더, 홈을 모르는 경우.
        for other in [
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
            "/Users/x/.local/bin/claude",
            "/Users/x/.npm-global/bin/claude",
            "/Users/x/.local/share/malgn-agent/node/current/bin/claude",
            "/Users/x/.local/share/malgn-agent/npm-global-evil/bin/claude",
            "claude",
            "",
        ] {
            assert!(claude_child_env_for(Some(&layout), other).is_empty(), "{other:?}");
        }
        assert!(claude_child_env_for(None, "/Users/x/.local/share/malgn-agent/npm-global/bin/claude").is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn claude_child_env_reaches_a_real_child_process_through_env_refs() {
        let layout = Layout::for_home(Path::new("/Users/x"));
        let env = claude_child_env_for(Some(&layout), "/Users/x/.local/share/malgn-agent/npm-global/bin/claude");
        let out = super::super::process::run_process_with_timeout(
            "/bin/sh",
            &["-c".to_string(), "printf %s \"$npm_config_prefix\"".to_string()],
            "/usr/bin:/bin",
            &super::super::process::env_refs(&env),
            Duration::from_secs(5),
        );
        assert_eq!(out.stdout, "/Users/x/.local/share/malgn-agent/npm-global");
    }

    // 앱이 `claude`를 직접 띄우는 모든 지점(PATH를 `build_child_path_env`로 만드는 파일)은 같은 규칙으로
    // env도 만들어야 한다 — 한 곳이라도 빠지면 그 경로로 뜬 claude의 자기 갱신만 어긋난다.
    // 일곱 번째 지점이 생겨도 자동으로 걸리도록 src 전체를 훑는다.
    #[test]
    fn every_claude_spawn_site_applies_the_claude_child_env() {
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
        let dev_tools_dir = root.join("dev_tools");
        let mut files = Vec::new();
        walk(&root, &mut files);
        let mut checked = Vec::new();
        for f in files {
            if f.starts_with(&dev_tools_dir) {
                continue;
            }
            let text = std::fs::read_to_string(&f).unwrap();
            // 테스트 모듈 이전 본문만 본다.
            let body = text.split("#[cfg(test)]").next().unwrap_or(&text);
            if body.contains("build_child_path_env(Some(") {
                assert!(
                    body.contains("claude_child_env("),
                    "{}: claude를 띄우면서 claude_child_env를 적용하지 않았습니다 — 앱 npm 전역에 설치된 claude의 자기 갱신이 앱이 찾는 위치에서 어긋납니다",
                    f.display()
                );
                checked.push(f.file_name().unwrap().to_string_lossy().to_string());
            }
        }
        assert!(checked.len() >= 5, "claude 실행 지점을 충분히 찾지 못했습니다(가드가 공허해짐): {checked:?}");
    }

    // ── RV-004: 실행 경로가 없는 기능(롤백)을 약속하지 않는다 ──

    #[test]
    fn previous_version_texts_do_not_promise_a_rollback_feature() {
        let prev = NodeVersion::parse_tag("v24.21.0").unwrap();
        let line = previous_kept_line(prev);
        assert!(line.contains("v24.21.0") && line.contains("보관"), "{line}");
        assert!(!line.contains("롤백"), "{line}");
        let args = vec![UPDATE_ARG.to_string()];
        let preview = update_preview_for_tag(node_def(), "/r", &args, "24.21.0", "v24.22.0").unwrap();
        assert!(preview.notes.contains("보관"), "{}", preview.notes);
        assert!(!preview.notes.contains("롤백"), "{}", preview.notes);
    }

    #[test]
    fn already_installed_note_states_only_what_was_checked() {
        assert!(!ALREADY_INSTALLED_NOTE.contains("온전"), "{ALREADY_INSTALLED_NOTE}");
        for checked in ["서명", "버전", "npm"] {
            assert!(ALREADY_INSTALLED_NOTE.contains(checked), "{checked}");
        }
    }

    // ── RV-001: 취소는 호출 시점에 진행 중이던 작업만 멈춘다 ──

    #[test]
    fn cancel_stops_the_operation_in_progress_but_not_ones_started_after() {
        let sw = CancelSwitch::new();
        let running = sw.token();
        assert!(!running.is_cancelled());
        sw.cancel();
        assert!(running.is_cancelled(), "진행 중이던 작업은 취소된다");
        // 취소 뒤에 시작하는 작업(다음 미리보기·설치)은 영향이 없다 — 리셋 호출 없이도.
        let next = sw.token();
        assert!(!next.is_cancelled());
        assert!(running.is_cancelled(), "이미 취소된 토큰이 되살아나지 않는다");
        // 두 작업이 겹쳐 진행될 때의 취소는 둘 다 멈춘다(앱 종료 시 일괄 정리와 같은 의미).
        sw.cancel();
        assert!(next.is_cancelled());
    }

    #[test]
    fn cancel_with_nothing_in_progress_is_harmless() {
        let sw = CancelSwitch::new();
        sw.cancel();
        sw.cancel();
        assert!(!sw.token().is_cancelled(), "진행 중인 작업이 없을 때의 취소가 다음 작업을 막으면 안 된다");
    }

    #[test]
    fn version_lookup_after_a_prior_cancel_is_not_reported_as_cancelled() {
        // 회귀: 예전에는 한 번 취소하면 플래그가 남아 resolve_target_tag 단계가 앱 재시작까지
        // "취소되었습니다"로 실패했다. 취소 뒤 새로 시작한 조회는 취소가 아닌 실제 결과(여기서는
        // 네트워크 오류)로 끝나야 한다.
        let sw = CancelSwitch::new();
        sw.cancel();
        let fresh = sw.token();
        let err = resolve_target_tag_with(&DeadEnv, Some(NodeArch::Arm64), &|| fresh.is_cancelled()).unwrap_err();
        assert!(!err.contains("취소"), "{err}");
        assert!(err.contains("nodejs.org"), "{err}");
        // 반대로 조회 도중(시작 뒤)의 취소는 취소로 보고된다.
        let started = sw.token();
        sw.cancel();
        let err = resolve_target_tag_with(&DeadEnv, Some(NodeArch::Arm64), &|| started.is_cancelled()).unwrap_err();
        assert!(err.contains("취소"), "{err}");
    }

    // 전역 스위치 경로(request_cancel → 이후 시작한 작업). 이 테스트만 request_cancel을 부른다.
    #[test]
    fn global_request_cancel_does_not_poison_later_operations() {
        request_cancel();
        let after = CANCEL.token();
        assert!(!after.is_cancelled());
        let home = std::env::temp_dir().join(format!("r22-app-node-after-cancel-{}", std::process::id()));
        let layout = Layout::for_home(&home);
        let v = NodeVersion::parse_tag("v24.22.0").unwrap();
        let out = run_pinned_with(&DeadEnv, &layout, NodeArch::Arm64, v, &|| after.is_cancelled());
        assert!(!out.aborted, "취소 뒤 새로 시작한 설치가 취소로 보고되면 안 된다: {}", out.stderr);
        let _ = std::fs::remove_dir_all(&home);
    }

    // ── P2-3b ──

    #[cfg(unix)]
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

    // ── P2-3c: 미리보기(순수 부분) ──

    fn node_def() -> &'static DevTool {
        super::super::tool_definition(super::super::ToolId::Node)
    }

    #[test]
    fn update_preview_offers_the_run_only_when_a_newer_lts_exists() {
        let args = vec![UPDATE_ARG.to_string()];
        let newer = update_preview_for_tag(node_def(), "/r", &args, "24.21.0", "v24.22.0").unwrap();
        assert!(newer.will_run);
        assert!(newer.notes.contains("새 LTS v24.22.0"), "{}", newer.notes);
        assert!(newer.notes.contains("24.21.0"));
        assert!(newer.preview_reliable);

        let same = update_preview_for_tag(node_def(), "/r", &args, "24.22.0", "v24.22.0").unwrap();
        assert!(!same.will_run);
        assert!(same.notes.contains("이미 최신"));
        // 현재가 더 높은 경우(예: 이후 LTS가 회수됨)도 업그레이드를 권하지 않는다.
        let ahead = update_preview_for_tag(node_def(), "/r", &args, "24.30.0", "v24.22.0").unwrap();
        assert!(!ahead.will_run);
        // 현재 버전을 해석할 수 없으면 새 버전이 있다고 단정하지 않되 실행은 열어 둔다.
        let unknown = update_preview_for_tag(node_def(), "/r", &args, "알 수 없음", "v24.22.0").unwrap();
        assert!(unknown.will_run);
    }

    #[test]
    fn update_and_install_previews_bind_the_target_version_into_plan_id() {
        let args = vec![UPDATE_ARG.to_string()];
        let a = update_preview_for_tag(node_def(), "/r", &args, "24.21.0", "v24.22.0").unwrap();
        let b = update_preview_for_tag(node_def(), "/r", &args, "24.21.0", "v24.23.0").unwrap();
        let c = update_preview_for_tag(node_def(), "/r", &args, "24.20.0", "v24.22.0").unwrap();
        assert_ne!(a.plan_id, b.plan_id, "대상 버전이 바뀌면 plan_id가 달라야 재미리보기를 강제한다");
        assert_ne!(a.plan_id, c.plan_id, "현재 버전이 바뀌어도 plan_id가 달라야 한다");
        let install = install_preview_for_tag(node_def(), "/r", &[INSTALL_ARG.to_string()], "v24.22.0");
        assert!(install.will_run);
        assert!(install.notes.contains("관리자 권한이 필요 없고"));
        assert!(install.notes.contains("HX7739G8FX"));
        assert!(install.command_display.contains("v24.22.0"));
    }

    // 실제 홈 기준 앱 설치 경로를 가진 Node는 앱 러너(Run)로 해석된다 — 설치방식 분류와 계획 해석이 이어지는지.
    #[cfg(target_os = "macos")]
    #[test]
    fn resolve_plan_routes_an_app_installed_node_to_the_app_runner() {
        use super::super::install_resolver::{resolve_plan, ResolvedAction};
        let Some(layout) = current_layout() else { return };
        if platform_now() != Platform::Mac {
            return;
        }
        let node_bin = layout.node_bin_dir().join("node").to_string_lossy().to_string();
        let resolved = resolve_plan(super::super::ToolId::Node, &node_bin);
        assert_eq!(resolved.method, super::super::classify::InstallMethod::AppManagedNode);
        match resolved.action {
            ResolvedAction::Run { plan, runner_path } => {
                assert_eq!(plan.runner, Runner::AppNode);
                assert_eq!(plan.args.len(), 1);
                assert_eq!(runner_path, layout.node_root().to_string_lossy());
            }
            ResolvedAction::Manual(_) => panic!("앱이 설치한 Node는 Manual이 아니라 앱 러너로 갱신돼야 합니다"),
        }
    }
}
