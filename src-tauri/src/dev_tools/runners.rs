// ==================== 실행기(runner) 해석(설계 §F.2 분할) ====================
// install_resolver.rs가 754줄까지 자란 데다 Windows 증분(winget 러너/후보/쓰기
// 권한 검사)이 거의 전부 이 영역에 떨어져 1,000줄 규율을 넘길 위험이 있었다
// (설계 §F.1). 판정 로직(G1~G4 게이트의 코드 표현)은 플랫폼과 무관하게 그대로
// install_resolver.rs에 남기고, "실행기 자체가 어디 있는지" 찾는 이 영역만
// 분리했다 — 이 선을 넘는 Windows 코드가 거의 전부 여기 떨어진다(설계 §F.2).

use super::plan_table::{is_writable_by_current_user, Runner};
use super::{resolve_tool_path, tool_definition, ToolId};
use crate::cli_launcher::resolve_binary_expand_home;
use std::path::Path;

const BREW_CANDIDATES: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];
// 뒤 2개(Windows, §B.5 "(러너) npm" 행)는 이 머신(Mac)에서는 절대 매칭되지
// 않는 리터럴 문자열로 harmless하게 통과한다(platform::expand_path_tokens가
// mac에서는 %토큰%이 없는 문자열을 그대로 반환하고, 그 문자열은 mac
// 파일시스템에 존재하지 않는다) — 무변경 보장은 platform.rs의 골든 테스트가
// 아니라 이 사실 자체(문자열이 절대 실존 경로와 겹치지 않음)에 있다.
const NPM_CANDIDATES: [&str; 5] = [
    "/opt/homebrew/bin/npm",
    "/usr/local/bin/npm",
    "~/.npm-global/bin/npm",
    r"C:\Program Files\nodejs\npm.cmd",
    r"%APPDATA%\npm\npm.cmd",
];
// Windows 전용(설계 §B.5 "(러너) winget" 행) — winget 앱 실행 별칭. mac에서는
// 위와 같은 이유로 항상 미스매치.
const WINGET_CANDIDATES: [&str; 1] = [r"%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe"];

/// `pub(crate)`인 이유: `install_resolver::resolve_plan`이 `Runner::SelfBinary`
/// 케이스(예: `claude update`가 자기 자신을 러너로 실행)에서 이미 알고 있는
/// `resolved_tool_path`를 `self_binary_path`로 직접 넘겨야 하는데,
/// `ResolvedRunners::path_for`는 그 인자를 받지 않는다(캐시 그릇의 계약과
/// 다르다) — 그래서 이 함수 자체를 크레이트 내부에 노출한다.
pub(crate) fn resolve_runner_path(runner: Runner, self_binary_path: Option<&str>) -> Option<String> {
    match runner {
        Runner::Brew => resolve_binary_expand_home(&BREW_CANDIDATES, "brew"),
        Runner::Npm => resolve_binary_expand_home(&NPM_CANDIDATES, "npm"),
        // pnpm 자신의 DevTool 정의(path_candidates)를 그대로 재사용한다 — 이미
        // 검증된 후보 목록(brew Cellar + standalone 두 경로, Windows 후보 포함)이
        // 있으므로 별도 PNPM_CANDIDATES 상수를 새로 만들지 않는다(중복 방지).
        Runner::Pnpm => resolve_tool_path(tool_definition(ToolId::Pnpm)),
        // 설계 §B.2: winget 해석은 gh 하나만 쓴다. 여기서는 러너 종류로만
        // 분기하고 소비처(install_candidates)가 어떤 도구에 쓸지 결정한다.
        Runner::Winget => resolve_binary_expand_home(&WINGET_CANDIDATES, "winget"),
        Runner::SelfBinary => self_binary_path.map(|s| s.to_string()),
    }
}

/// brew/npm/pnpm/winget 실행기 경로를 화면 1회 로드에서 한 번만 해석해 담아
/// 두는 그릇(요구 4 — resolve_binary의 bare-name PATH 폴백은 타임아웃이 없어
/// 도구 개수만큼 반복 호출하면 그만큼 블로킹 비용이 커진다. 지적 #12 자체의
/// 수정은 이 범위 밖이지만 호출 횟수를 1회로 묶어 그 결함의 노출을 늘리지
/// 않는다).
///
/// winget도 다른 셋과 똑같이 이 구조체의 필드로 캐시한다(CI 8건 조사,
/// review-devtools-windows-parity로 뒤집힌 원래 설계 결정 — 이전 주석은
/// "리터럴로 구성하는 기존 테스트를 고치지 않기 위해 필드를 두지 않고
/// `path_for`가 winget만 그 자리에서 새로 해석한다"고 적었었다. 그런데 그
/// "그 자리에서 새로 해석"이 정확히 문제였다: `resolve_install_plan_falls_
/// back_to_manual_when_no_runner_resolved` 테스트가 `ResolvedRunners { brew:
/// None, npm: None, pnpm: None }`으로 "실행기가 하나도 없다"를 주입해도,
/// winget은 이 struct를 거치지 않고 `windows-latest` 러너의 진짜 파일시스템을
/// 다시 읽어 — 그 러너에는 winget이 실제로 설치돼 있어 — Some을 돌려줬다.
/// "러너가 없으면 Manual"이라는 불변식을 테스트가 통제할 수 없게 만든 것이다.
/// 필드를 추가해 브루/npm/pnpm과 동일하게 주입 가능하게 만드는 편이 "캐시 필드
/// 안 만들기"보다 이 struct의 원래 목적(요구 1의 단일 정본을 향한 입력 스냅숏)
/// 에 더 맞는다.
///
/// 비용 정정(4차 리뷰 V5): Windows에서는 WINGET_CANDIDATES 파일 존재 확인만으로
/// 끝나 스폰이 없다. 그러나 **macOS에서는 그 후보가(Windows 전용 토큰 문자열이라)
/// 구조적으로 항상 미스매치**이고, `resolve_binary`가 Mac 분기에서 bare-name
/// 폴백으로 `Command::new("winget").arg("--version")`을 실제로 spawn한다(즉시
/// ENOENT로 실패). 이전 이 필드가 없던 시절에는 gh의 후보 순서([Brew, Winget])상
/// brew가 먼저 잡혀 이 경로에 사실상 도달하지 않았지만, `resolve()`가 네 필드를
/// 무조건 전부 채우는 지금은 화면 1회 로드(query.rs)·설치/터미널 액션마다
/// macOS에 존재하지 않는 바이너리 spawn 시도가 1회 새로 생긴다(브루/npm/pnpm과
/// 같은 "파일 존재 확인" 비용급이 아니다). 실해는 미미하다(ENOENT 즉시 실패,
/// 도구 루프 밖에서 1회) — 그래도 "macOS 무변경"을 주장하려면 이 사실을
/// 숨기면 안 된다.
pub(crate) struct ResolvedRunners {
    pub(crate) brew: Option<String>,
    pub(crate) npm: Option<String>,
    pub(crate) pnpm: Option<String>,
    pub(crate) winget: Option<String>,
}

impl ResolvedRunners {
    pub(crate) fn resolve() -> Self {
        ResolvedRunners {
            brew: resolve_runner_path(Runner::Brew, None),
            npm: resolve_runner_path(Runner::Npm, None),
            pnpm: resolve_runner_path(Runner::Pnpm, None),
            winget: resolve_runner_path(Runner::Winget, None),
        }
    }

    pub(crate) fn path_for(&self, runner: Runner) -> Option<String> {
        match runner {
            Runner::Brew => self.brew.clone(),
            Runner::Npm => self.npm.clone(),
            Runner::Pnpm => self.pnpm.clone(),
            Runner::Winget => self.winget.clone(),
            // SelfBinary는 설치 후보에 쓰이지 않는다(설치 시점엔 자기 자신의
            // 바이너리가 아직 없다) — install_candidates()의 어떤 행도 이
            // 변형을 쓰지 않으므로 항상 None으로 충분하다.
            Runner::SelfBinary => None,
        }
    }
}

/// M2(devtools-install-matrix §4.2, 직전 리뷰 지적) — 설치 경로 쓰기권한
/// 사전검사. gh(brew)는 `<prefix>/Cellar`·`<prefix>/bin`을, Claude(npm)는
/// `<npm prefix>/lib/node_modules`(없으면 `<prefix>/lib`, 그마저 없으면 검사
/// 생략)를 본다 — `plan_table::compute_action`(update 경로)이 이미 하는 검사를
/// 설치 경로에도 재사용한다. prefix는 runner 경로의 조부모다
/// (`/opt/homebrew/bin/brew` → `/opt/homebrew`).
///
/// §4.2 단서 그대로: 없는 경로에 대해 `access(W_OK)`가 무조건 false를 주므로,
/// "아직 없는 경로"를 "쓰기 불가"로 오판해 fail-closed로 기능을 죽이지 않도록
/// 경로가 존재할 때만 검사한다(존재하지 않으면 통과시키고, 실제로 못 쓰면
/// exit code로 드러난다). 설계 §4.2 표는 gh·Claude 두 행만 요구하므로 그 외
/// 조합(Wrangler의 pnpm/npm, gh의 winget)은 대상이 아니다 — winget은 애초에
/// "prefix" 개념이 없어(winget이 스스로 설치 경로를 관리) 와일드카드가
/// `true`(검사 생략)로 처리한다(설계 §D.5, 새 분기 불필요).
pub(crate) fn install_prefix_writable(tool: ToolId, runner: Runner, runner_path: &str) -> bool {
    let Some(prefix) = Path::new(runner_path).parent().and_then(Path::parent) else {
        return true;
    };

    match (tool, runner) {
        (ToolId::Gh, Runner::Brew) => {
            let cellar = prefix.join("Cellar");
            let bin = prefix.join("bin");
            (!cellar.exists() || is_writable_by_current_user(&cellar))
                && (!bin.exists() || is_writable_by_current_user(&bin))
        }
        (ToolId::Claude, Runner::Npm) => {
            let node_modules = prefix.join("lib").join("node_modules");
            if node_modules.exists() {
                is_writable_by_current_user(&node_modules)
            } else {
                let lib = prefix.join("lib");
                !lib.exists() || is_writable_by_current_user(&lib)
            }
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // M2(직전 라운드) — gh 설치 경로의 쓰기권한 사전검사가 실제로 동작하는지
    // 실측한다. Cellar/bin 모두 쓰기 가능/불가 두 경우를 확인한다.
    #[test]
    #[cfg(unix)]
    fn install_prefix_writable_detects_readonly_cellar() {
        use std::os::unix::fs::PermissionsExt;

        let Some(home) = dirs::home_dir() else {
            return;
        };
        let tmp_prefix = home.join(format!(
            "malgn_vscode_test_runners_prefix_{}",
            std::process::id()
        ));
        let cellar = tmp_prefix.join("Cellar");
        let bin = tmp_prefix.join("bin");
        std::fs::create_dir_all(&cellar).expect("create synthetic Cellar dir");
        std::fs::create_dir_all(&bin).expect("create synthetic bin dir");
        let runner_path = tmp_prefix.join("bin").join("brew");

        assert!(install_prefix_writable(
            ToolId::Gh,
            Runner::Brew,
            &runner_path.to_string_lossy()
        ));

        let mut perms = std::fs::metadata(&cellar)
            .expect("stat cellar")
            .permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&cellar, perms).expect("chmod cellar read-only");

        let result = install_prefix_writable(ToolId::Gh, Runner::Brew, &runner_path.to_string_lossy());

        let mut restore = std::fs::metadata(&cellar)
            .expect("stat cellar for restore")
            .permissions();
        restore.set_mode(0o755);
        let _ = std::fs::set_permissions(&cellar, restore);
        let _ = std::fs::remove_dir_all(&tmp_prefix);

        assert!(!result, "Cellar가 쓰기 불가면 false여야 합니다");
    }

    #[test]
    fn install_prefix_writable_skips_check_for_nonexistent_paths() {
        assert!(
            install_prefix_writable(
                ToolId::Gh,
                Runner::Brew,
                "/opt/homebrew_does_not_exist_malgn/bin/brew"
            ),
            "Cellar/bin이 아직 없으면 쓰기 불가로 오판하지 않고 통과시켜야 합니다"
        );
        assert!(
            install_prefix_writable(
                ToolId::Claude,
                Runner::Npm,
                "/opt/homebrew_does_not_exist_malgn/bin/npm"
            ),
            "lib/node_modules와 lib이 모두 없으면 검사를 건너뛰고 통과시켜야 합니다"
        );
    }

    // winget에는 prefix 개념이 없다 — 어떤 경로를 줘도 항상 true(검사 생략).
    #[test]
    fn install_prefix_writable_always_passes_for_winget() {
        assert!(install_prefix_writable(
            ToolId::Gh,
            Runner::Winget,
            r"C:\Users\hopegiver\AppData\Local\Microsoft\WindowsApps\winget.exe"
        ));
    }

    #[test]
    fn resolved_runners_resolve_does_not_panic_on_this_machine() {
        let runners = ResolvedRunners::resolve();
        // 이 머신에 brew가 있어야 나머지 회귀 테스트들이 전제로 삼는다(기존
        // install_resolver.rs 테스트들과 동일한 전제).
        assert!(runners.brew.is_some() || runners.npm.is_some());
    }

    // 플랫폼 전제: 이 머신이 Mac이라 winget 후보가 항상 미스매치라는 전제 —
    // CI의 windows-latest에서는 컴파일을 건너뛴다. winget이 이제 다른 러너와
    // 같이 `resolve()` 시점에 캐시되므로(§ 위 struct 주석) 테스트 이름에서
    // "fresh each call"을 뺐다 — 검증 내용(Mac에서는 항상 None) 자체는 그대로다.
    #[cfg(target_os = "macos")]
    #[test]
    fn path_for_winget_is_none_on_this_mac_machine() {
        let runners = ResolvedRunners::resolve();
        // 이 머신은 Mac이므로 winget 후보(Windows 전용 토큰 문자열)는 항상
        // 미스매치 — None이어야 한다(패닉 없이).
        assert_eq!(runners.path_for(Runner::Winget), None);
    }

    #[test]
    fn path_for_self_binary_is_always_none() {
        let runners = ResolvedRunners::resolve();
        assert_eq!(runners.path_for(Runner::SelfBinary), None);
    }
}
