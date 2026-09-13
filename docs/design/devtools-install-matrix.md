# 개발 환경 — 미설치 도구 설치 매트릭스 (도구 5 × 플랫폼 2)

작성: architect / 대상: `src-tauri/src/dev_tools.rs` / 등급: **Sensitive**(사용자 머신에 패키지를 실제로 설치하는 명령의 실행 표면 확대)
정본 관계: **`scratch-design-devtools-update.md`(결정 1~6 + 부록 A/B/C)의 확장이다.** 결정 1~6 중 뒤집는 것은 **하나**뿐이며 §1.3에 근거와 함께 명시한다.
범위: 설계 결정만. 이 문서 작성 과정에서 소스 파일은 한 줄도 수정하지 않았다.

---

## 0. 이 설계가 답해야 하는 것

`docs/reviewer/review-devtools-2026-09-09.md`가 "별도 Sensitive 리뷰 필요"로 올린 그 변경(`5251f97` Wrangler 실설치)이 이미 들어가 있다. 그 커밋은 원 설계의 **"미설치 도구는 항상 Manual"** 원칙에 도구 하나짜리 구멍을 냈고, 그 구멍을 `check_dev_tools_blocking`(1462~1468행)의 `if def.id == ToolId::Wrangler` 하드코딩으로 막아 두었다. 이 문서는 그 하드코딩을 **"어떤 도구가 왜 실행 가능한가"의 판정 규칙**으로 승격시키고, 나머지 5개 도구 × 2개 플랫폼 10칸을 그 규칙으로 채운다.

### 0.1 현행 대비 차별점(② 고유성 의무)

이 저장소에 `docs/prd.md`는 없다. 원 설계 §0과 같은 축, 즉 **현행 방식(개발자가 터미널에서 스스로 설치 명령을 기억해 실행)**을 비교 기준으로 인용한다. 현행에서 실제로 나는 사고는 설치 국면에서 업데이트 국면과 다르다:

| 현행(수동 설치)에서 나는 사고 | 이 설계가 다르게 하는 지점 |
|---|---|
| ① 이미 설치돼 있는 줄 모르고 다른 경로에 두 번째 사본을 설치한다(brew node + nvm node) | 설치 실행 전에 **앱이 어느 경로들을 뒤졌는지 프리뷰에 그대로 나열**하고, 그 목록으로 못 찾은 도구만 실행 대상으로 삼는다. 뒤질 수 없는 종류의 도구(버전매니저/corepack 관리 대상)는 애초에 `run` 후보에서 제외한다(§1.2 G4) |
| ② 블로그에서 본 설치 명령이 공식 문서와 다르다(`npm i -g pnpm`은 pnpm 공식 문서에서 이미 사라졌다 — §3.2) | argv는 **공식 문서에 고정 식별자로 명시된 것만** 리터럴로 테이블에 박는다. 고정 식별자가 없으면 그 자체가 `manual` 판정 근거다(§1.2 G1) |
| ③ 설치했는데 셸에서 안 보인다(PATH 미노출) | 설치 성공 판정을 exit code가 아니라 **설치 후 경로 재해석 + 버전 재조회 + `pathVisible` 재계산**으로 한다(결정 4 그대로 재사용) |

업데이트 국면의 차별점(경로 파싱으로 `node@22`를 알아낸다)은 원 설계가 이미 다뤘다. 설치 국면의 고유 문제는 **"파일시스템에 근거가 하나도 없는 상태에서 argv를 만들어야 한다"**는 것이고, §1.1이 그에 대한 구조적 답이다.

---

## 1. 판정 규칙

### 1.1 구조적 불변식 — "설치 argv는 리터럴 전용"

원 설계의 안전성 사슬(부록 C)은 **동적 argv 토큰이 파일시스템에서 읽은 canonical 경로에서만 유래한다**는 데 걸려 있다. 미설치 도구에는 그 canonical 경로가 없다. 따라서:

> **INSTALL_TABLE의 모든 행은 `Arg::Lit`만으로 구성된다. `Arg::Formula`/`Arg::Package`/`Arg::PackageLatest` 슬롯을 쓰는 행은 INSTALL_TABLE에 넣을 수 없다.**

이 한 줄이 "무엇을 `run`으로 둘 수 있는가"를 사실상 결정한다. 리터럴을 쓸 수 있다는 것은 곧 **공식 문서가 패키지 식별자를 고정했다**는 뜻이고, 고정돼 있지 않으면(=버전 접미, 채널 분기, 배포 채널이 여럿) 앱이 그 선택을 사용자 대신 하게 되므로 `manual`이다. 기존 `RUN_PNPM_GLOBAL_ADD`(553행)가 이미 이 형태다 — Wrangler가 `run`일 수 있었던 진짜 이유는 "특별 취급"이 아니라 **리터럴로 쓸 수 있었기 때문**이다.

부수 효과로, INSTALL_TABLE 행에서는 `resolve_args()`의 `validate_argv_token` 경로가 아예 활성화되지 않는다(리터럴은 검증 면제). 인젝션 표면이 0인 것이 아니라 **구조적으로 존재하지 않는다**.

### 1.2 `run` 게이트 4조건 (하나라도 못 넘으면 `manual`)

| 게이트 | 내용 | 탈락 사례 |
|---|---|---|
| **G1 고정 식별자** | 공식 문서가 패키지/포뮬러 식별자를 단일 리터럴로 명시한다. 버전 접미(`node@22`)·채널 분기(`claude-code` vs `claude-code@latest`)가 있으면 탈락 | Node.js(공식 다운로드 페이지가 유지하는 패키지 매니저 식별자 자체가 없음), Claude Code cask(채널 2종) |
| **G2 셸 불필요** | `std::process::Command`에 argv 배열로 그대로 넘길 수 있다. `curl \| bash`, `irm \| iex`, 대화형 `npx`는 탈락 | Claude 네이티브 설치, pnpm standalone(`npx get-pnpm`은 npx 자체가 `Ok to proceed? (y)`를 묻는다 → stdin=null이면 실패) |
| **G3 설치 후 자리 예측 가능** | 설치 결과 바이너리가 **기존 `path_candidates`가 이미 보는 자리**에 놓이고, 그 canonical 경로가 `classify_install_method`의 기존 분류에 물려 이후 업데이트 경로까지 성립한다 | pnpm standalone(PNPM_HOME + rc 파일 수정에 의존), Windows 전반(§5) |
| **G4 오탐 피해가 국소적** | 이미 설치돼 있는데 앱이 못 보고(Finder 실행 시 `launchctl` PATH에 아무것도 없다 — `cli_launcher.rs:5-7` 실측 주석) 설치를 실행해도 기존 환경이 깨지지 않는다 | Node.js·pnpm(nvm/fnm/volta/corepack 관리본은 `path_candidates`로 보이지 않는다 → 중복 설치가 PATH 우선순위를 뒤집어 기존 프로젝트를 깨뜨린다), Git(시스템 도구) |

**G4가 이 설계에서 가장 값어치 있는 게이트다.** 위임문이 말한 "설치 경로가 사용자 환경마다 갈려 자동 설치가 오히려 환경을 망가뜨릴 수 있는 도구"가 정확히 G4 탈락군이고, 그 판정 근거가 `InstallMethod::VersionManager`/`ManualReason::CorepackManaged`라는 **이미 코드에 있는 개념**이다. 즉 "node는 버전매니저로 관리될 수 있다"를 아는 코드가, "그러니 node는 우리가 설치하면 안 된다"까지 일관되게 말하게 된다.

### 1.3 원 설계에서 뒤집는 결정 — 하나

`dev_tools.rs:674-678` 주석의 **"미설치 도구용 안내(v1: 항상 Manual)"** 를 다음으로 교체한다:

> 미설치 도구는 §1.2의 G1~G4를 **모두** 통과할 때만 `run`이고, 그 외에는 `manual`이다. 통과 여부는 INSTALL_TABLE에 리터럴 전용 행이 존재하는지로 표현된다.

**뒤집는 근거**: 원 주석의 논리는 "formula명을 추측할 근거가 없다"였는데, 그 논리는 *추측이 필요한 경우*에만 성립한다. `gh`·`@anthropic-ai/claude-code`는 공식 문서가 식별자를 고정해 두어 추측이 개입하지 않는다. 실제로 이 원칙은 `5251f97`에서 Wrangler 하나에 이미 깨졌고, 지금 그것이 **규칙 없는 예외(하드코딩 `if`)** 로 남아 있는 상태가 원 설계의 정신("잘못된 상태를 표현 불가능하게")에 더 어긋난다. 규칙으로 승격시키면 다음 도구가 추가될 때 판단 기준이 코드에 남는다.

뒤집지 않는 것: 결정 1(경로 파싱 판별), 결정 3(판별 실패 시 실행 거부), 결정 4(버전 델타 성공 판정), 결정 5(절대경로/자식 PATH), 결정 6(비동기·타임아웃·프로세스그룹), 부록 A(plan_id 게이트), 부록 B(stdin null·쓰기권한 사전검사·env), 부록 C(프론트 계약) — **전부 그대로 상속한다.**

---

## 2. 10칸 매트릭스 (핵심 산출물)

표기: **미검증** = 이 개발 머신(macOS/Apple Silicon, 5개 도구 전부 이미 설치됨, winget 없음)에서 실행 검증이 불가능한 항목.

| # | 도구 | OS | (a) action_kind + 근거 | (b) argv (셸 없이 그대로) / 실행파일 절대경로 후보 | (c) 설치 후 놓이는 경로 (= `path_candidates` 요구값) | (d) manual 안내문 / 공식 URL | (e) Wrangler 패턴 대비 |
|---|---|---|---|---|---|---|---|
| 1 | Claude Code | macOS | **run** — G1~G4 통과. npm 패키지명 `@anthropic-ai/claude-code`가 공식 문서에 고정 | `["install","-g","@anthropic-ai/claude-code"]`<br>runner: `/opt/homebrew/bin/npm`, `/usr/local/bin/npm`, `~/.npm-global/bin/npm` (기존 `NPM_CANDIDATES` 재사용) | `<npm prefix>/bin/claude` (실측: `npm prefix -g` = `/opt/homebrew`) → 기존 후보에 이미 있음. **추가 필요**: `~/.npm-global/bin/claude`, `~/.claude/local/claude` | (runner 미해석 시) "npm을 찾을 수 없습니다. 먼저 Node.js를 설치해주세요." + `brew install --cask claude-code` / https://code.claude.com/docs/en/setup | **재사용하되 pnpm 폴백을 뺀다.** Wrangler는 pnpm→npm 순서지만 Claude는 **npm 전용**. 사유 §3.1 |
| 2 | Claude Code | Windows | **manual** — G2 탈락(`irm \| iex`), winget 경로는 §5에서 v1 제외. **미검증** | — | (참고) `%USERPROFILE%\.local\bin\claude.exe`(네이티브), `%APPDATA%\npm\claude.cmd`(npm), `%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe`(winget) | "Claude Code가 설치되어 있지 않습니다. PowerShell에서 아래 명령을 실행해주세요." / `winget install Anthropic.ClaudeCode` / https://code.claude.com/docs/en/setup | 재사용 없음(실행 경로 자체가 없음) |
| 3 | pnpm | macOS | **manual** — G2·G3·G4 탈락. 공식 설치가 `npx get-pnpm`(대화형 npx + 원격 스크립트)뿐이고, 설치기가 `PNPM_HOME`과 셸 rc 파일을 고쳐 앱이 미리보기도 검증도 못 한다. corepack 관리본은 앱에 보이지 않는다 | — | (참고) `~/Library/pnpm/pnpm`, `~/.local/share/pnpm/pnpm`, `/opt/homebrew/bin/pnpm` — 이미 `DEV_TOOLS`에 있음 | "pnpm이 설치되어 있지 않습니다. Homebrew가 있으면 아래 명령이 가장 간단합니다." / `brew install pnpm` (brew 미해석 시 `npx get-pnpm`) / https://pnpm.io/installation | 재사용 없음. Wrangler가 넘은 G3(설치 후 자리 예측)를 pnpm은 못 넘는다 — 설치기가 환경변수·rc까지 건드린다 |
| 4 | pnpm | Windows | **manual** — 위와 같고, 공식 문서가 Windows에서 standalone을 Defender 문제로 비권장한다. **미검증** | — | (참고) `%LOCALAPPDATA%\pnpm\pnpm.exe`, `C:\Program Files\nodejs\pnpm.cmd` | "pnpm이 설치되어 있지 않습니다. PowerShell에서 아래 명령을 실행해주세요." / `npx get-pnpm` / https://pnpm.io/installation | 재사용 없음 |
| 5 | Node.js | macOS | **manual** — G1·G4 탈락. 공식 다운로드 페이지가 유지·보증하는 패키지 매니저 식별자가 없고(설치 스크립트는 "Node.js 프로젝트가 유지하지 않음"이라고 명시), brew의 `node` / `node@22`는 채널 선택을 앱이 대신하는 꼴이다. 게다가 nvm/fnm/volta 관리본은 GUI 실행 시 앱에 보이지 않는다 → 중복 설치 위험 | — | (참고) `/opt/homebrew/bin/node`, `/usr/local/bin/node` — 이미 `DEV_TOOLS`에 있음 | "Node.js가 설치되어 있지 않습니다. Homebrew가 있으면 아래 명령으로 설치할 수 있습니다. 이미 nvm/fnm 등으로 관리 중이라면 그쪽에서 설치해주세요." / `brew install node` / https://nodejs.org/en/download | 재사용 없음. **이 셀이 유일하게 재론 여지가 있다** — 승격 조건 §3.3 |
| 6 | Node.js | Windows | **manual** — 위와 같음. **미검증** | — | (참고) `C:\Program Files\nodejs\node.exe`, `%LOCALAPPDATA%\Programs\nodejs\node.exe` | "Node.js가 설치되어 있지 않습니다. 공식 설치 프로그램(.msi)을 받아 실행해주세요." / https://nodejs.org/en/download | 재사용 없음 |
| 7 | GitHub CLI (gh) | macOS | **run** — G1~G4 통과. 공식 문서(cli.github.com 및 저장소 설치 문서)가 `brew install gh` 하나로 고정. 버전 접미 formula가 없고, gh를 버전매니저로 관리하는 관행이 없다 실행 `["install","-y","--formula","gh"]` / 프리뷰 `["install","-n","--formula","gh"]`<br>runner: `/opt/homebrew/bin/brew`, `/usr/local/bin/brew` (기존 `BREW_CANDIDATES` 재사용) | `<brew prefix>/bin/gh` → 기존 후보(`/opt/homebrew/bin/gh`, `/usr/local/bin/gh`)에 이미 있음. canonical은 `<prefix>/Cellar/gh/<v>/bin/gh` → `HomebrewFormula{formula:"gh"}` → 이후 업데이트도 기존 경로로 성립 | (brew 미해석/쓰기불가 시) 기존 `MANUAL_NO_RUNNER` / `MANUAL_NOT_WRITABLE` 재사용 + `brew install gh` / https://cli.github.com/ | **다르다.** Wrangler는 npm/pnpm runner + 프리뷰 없음이지만 gh는 brew runner + **`brew install -n` 프리뷰 필수**. 사유: brew install은 의존성을 함께 설치·업그레이드한다(§3.4 실측) |
| 8 | GitHub CLI (gh) | Windows | **manual** — G2 탈락(§5). **미검증** | — | (참고) `C:\Program Files\GitHub CLI\gh.exe`, `%LOCALAPPDATA%\Microsoft\WinGet\Links\gh.exe` | "GitHub CLI가 설치되어 있지 않습니다. PowerShell에서 아래 명령을 실행해주세요." / `winget install --id GitHub.cli --source winget` / https://cli.github.com/ | 재사용 없음 |
| 9 | Git | macOS | **manual** — G4 탈락(시스템 도구). 해결책 `xcode-select --install`이 **GUI 대화상자**를 띄우므로 앱이 몰아서 실행할 수도, 결과를 검증할 수도 없다. brew git을 대신 설치하면 `/usr/bin/git`과 두 개가 되어 PATH 순서에 따라 동작이 갈린다 | — | (참고) `/Library/Developer/CommandLineTools/usr/bin/git`(실측: `xcode-select -p` = `/Library/Developer/CommandLineTools`), `/usr/bin/git`(스텁), `/opt/homebrew/bin/git` | 기존 `MANUAL_XCODE_CLT` 문구 유지 + copyable `xcode-select --install` / https://git-scm.com/downloads | 재사용 없음. **추가 요구**: §4.3의 스텁 회피 규칙 |
| 10 | Git | Windows | **manual** — G2 탈락(설치 프로그램/winget). **미검증** | — | (참고) `C:\Program Files\Git\cmd\git.exe`, `C:\Program Files (x86)\Git\cmd\git.exe`, `%LOCALAPPDATA%\Programs\Git\cmd\git.exe` | "Git이 설치되어 있지 않습니다. PowerShell에서 아래 명령을 실행하거나 설치 프로그램을 받아주세요." / `winget install --id Git.Git -e --source winget` / https://git-scm.com/install/windows | 재사용 없음 |

**요약: 10칸 중 `run` 2칸(1번 Claude/macOS, 7번 gh/macOS), `manual` 8칸. `none`은 0칸** — §6.2 참조(현재 `none`이 쓰이던 자리가 전부 `manual`로 바뀐다).

### 2.1 `run`/`manual`을 가른 한 줄 (도구별)

- **Claude Code**: 공식 npm 패키지명이 고정돼 있고 설치 결과가 기존 `NpmGlobal` 판별기에 그대로 물린다 → `run`.
- **gh**: 공식 formula명이 `gh` 하나뿐이고 버전 접미가 없다 → `run`(단 의존성 연쇄 때문에 프리뷰 필수).
- **pnpm**: 공식 설치기가 셸 스크립트이고 rc 파일까지 고친다 → 앱이 미리보기도 되돌리기도 못 한다 → `manual`.
- **Node.js**: 공식이 고정 식별자를 주지 않고, 버전매니저 관리본을 앱이 볼 수 없어 중복 설치가 기존 환경을 깨뜨린다 → `manual`.
- **Git**: 시스템(Xcode CLT) 소유이고 해결책이 GUI 대화상자다 → `manual`.

---

## 3. `run` 셀 상세와 트레이드오프(① 의무)

### 3.1 Claude Code — 왜 npm 전용이고 Wrangler처럼 pnpm 폴백을 두지 않는가

**선택**: `npm install -g @anthropic-ai/claude-code` (runner = npm만).
**검토한 대안 A — Wrangler와 동일하게 pnpm 우선, npm 폴백**: 기각. Claude Code npm 패키지는 플랫폼별 optional dependency(`@anthropic-ai/claude-code-darwin-arm64` 등)를 끌어와 **postinstall 단계에서 네이티브 바이너리를 링크**한다(공식 문서 명시: "npm pulls the binary in through a per-platform optional dependency … and a postinstall step links it into place", "Your package manager must allow optional dependencies"). pnpm v10 계열은 의존성의 lifecycle script를 기본 차단하므로 **exit 0인데 바이너리가 없는 조용한 실패**가 날 수 있다 — 결정 4의 버전 재조회가 이를 `UnknownAfter`(verified=false)로 잡아내긴 하지만, 잡아낸 뒤 사용자가 할 수 있는 게 없다. Wrangler는 이 postinstall 링크 단계가 없어 pnpm으로 이미 검증됐다(이 머신 `~/Library/pnpm/bin/wrangler` 실측).
> **미검증**: pnpm v10의 postinstall 차단이 이 패키지에 실제로 영향을 주는지는 이 머신에서 확인하지 못했다(claude가 이미 네이티브 설치돼 있어 재설치 실험이 금지 범위). 확인되지 않은 리스크를 감수하지 않는 쪽을 택했다.

**검토한 대안 B — `brew install --cask claude-code`**: 기각. ① 공식이 `claude-code`(stable)와 `claude-code@latest`(latest) **두 채널**을 제공하므로 앱이 사용자 대신 채널을 고르게 된다(G1 위반). ② 설치 후 `HomebrewCask`로 분류되는데 현행 UPDATE_TABLE은 캐스크를 `MANUAL_CASK_NOT_WRITABLE`로 보낸다 → **앱이 설치해 놓고 자기가 업데이트 못 하는 상태**를 스스로 만든다(G3 위반).
**검토한 대안 C — 네이티브 설치기(`curl … | bash`)**: 기각. 셸 경유 금지(제약 ②) 정면 위반.

**포기한 것**: npm/node가 없는 머신에서는 Claude Code를 설치할 수 없다. **감당**: `Manual(NoRunner)`로 강등하되 문구를 도구별로 구체화해 "먼저 Node.js를 설치해주세요"까지 말한다(§4.1 부트스트랩 체인).

**업데이트 경로와의 정합(중요)**: npm 전역 설치 후 `<prefix>/bin/claude`는 `lib/node_modules/...` 아래를 가리키는 심볼릭 링크이므로 `classify_install_method`가 `NpmGlobal{package}`로 분류한다. 여기서 **파싱된 package명이 `@anthropic-ai/claude-code-darwin-arm64`(플랫폼 optional dep)일 위험**이 있다 — postinstall이 링크를 어디로 걸었느냐에 달려 있고 이 머신에서 확인 불가(**미검증**). 잘못 파싱되면 `npm install -g @anthropic-ai/claude-code-darwin-arm64@latest`라는 **틀린 업데이트 명령**이 나온다.
→ **처방**: `UPDATE_TABLE`에 `Row{ tool: Some(Claude), method: NpmGlobal, action: Run(RUN_NPM_CLAUDE_LATEST) }`를 **기존 제네릭 `NpmGlobal` 행(현재 634행)보다 앞에** 넣고, argv를 `["install","-g","@anthropic-ai/claude-code@latest"]` 리터럴로 고정한다. 공식 문서가 업그레이드 명령을 그대로 이 문자열로 지정하고 있고("To upgrade an npm installation, run `npm install -g @anthropic-ai/claude-code@latest`. Avoid `npm update -g`"), 리터럴이므로 파싱 오류가 개입할 여지가 없다.
- 트레이드오프: 결정 1의 "파싱 > 하드코딩 맵" 원칙에 대한 **좁은 예외**다. 원칙의 근거는 "버전 접미 formula명을 추측할 수 없다"였는데, 여기서는 추측이 아니라 **공식 문서가 명시한 고정 문자열**이고, 오히려 파싱 쪽이 틀릴 위험이 크다. 예외 범위를 `(Claude, NpmGlobal)` 한 칸으로 한정해 일반 규칙은 건드리지 않는다.

### 3.2 pnpm — 공식 문서가 바뀌었다(설계 반영 필요)

원 설계와 기존 코드는 `npm install -g pnpm` 계열을 전제하지만, **pnpm 공식 설치 문서는 현재 "Using npm" 탭에 `npx get-pnpm`만 제시하며 `npm install -g pnpm`도 corepack도 언급하지 않는다.** 따라서:
- `install_manual_plan(Pnpm)`의 copyable을 `brew install pnpm`(brew 해석 가능 시) / `npx get-pnpm`(그 외)로 두고 `doc_url`에 https://pnpm.io/installation 을 채운다. 현재 이 함수의 Pnpm 분기는 `doc_url: None`이라 사용자가 공식 문서로 갈 길이 없다.
- `npx get-pnpm`을 앱이 직접 실행하지 않는 추가 근거: npx는 미설치 패키지에 대해 `Ok to proceed? (y)`를 묻는다 → 부록 B.1의 `stdin(Stdio::null())` 하에서 **확정 실패**다. 실패가 정지보다 낫다는 원칙 덕에 매달리진 않지만, 확정 실패할 명령을 계획으로 만들지는 않는다(부록 B.2의 "sudo가 필요한 계획은 애초에 만들지 않는다"와 같은 정신).

### 3.3 Node.js — `manual`로 두되, 승격 조건을 명시한다

이 셀은 10칸 중 유일하게 합리적인 반론이 가능한 자리다. 반론: "brew가 있는 머신에서 `brew install -y --formula node`는 리터럴이고, 설치 후 `HomebrewFormula{formula:"node"}`로 분류돼 업데이트까지 성립한다."
그럼에도 `manual`로 두는 결정적 근거는 **G4**다: Finder로 띄운 `.app`은 `launchctl` PATH를 상속하므로 nvm/fnm/volta/asdf/mise가 관리하는 node를 **구조적으로 볼 수 없다**(`cli_launcher.rs:5-7` 실측 주석). 즉 "미설치"라는 판정 자체가 이 도구에 대해서만 신뢰도가 낮고, 그 낮은 신뢰도 위에서 실행되는 설치는 사용자의 기존 toolchain에 두 번째 node를 얹는다. `InstallMethod::VersionManager`가 코드에 존재한다는 사실 자체가 "이 도구는 그렇게 관리되는 일이 흔하다"는 우리 조직의 인정이다.

**승격 트리거(이 중 하나라도 성립하면 `run` 재검토)**: ① 사내 표준 node major가 문서로 확정되어 `node@<major>` 리터럴을 쓸 수 있게 될 때, ② 버전매니저 관리본을 셸 없이 탐지하는 수단(예: `~/.nvm/versions/node/*/bin/node` 글롭을 `path_candidates`에 추가)이 들어와 G4의 전제가 바뀔 때.

### 3.4 gh — `brew install -n` 프리뷰가 필수인 이유 (실측)

읽기 전용 실측(`brew install -n --formula wget`, 이 머신):
```
==> Would install 1 formula:
wget
==> Would install 1 dependency for wget:
libpsl
==> Would upgrade 1 dependency for wget:
openssl@3
   (동일 3개 섹션이 한 번 더 반복 출력됨)
```
여기서 세 가지가 확정된다.
1. **`brew install`은 설치만 하지 않는다** — 의존성을 새로 설치하고, 이미 있는 의존성을 **업그레이드**한다(`openssl@3`). 사용자가 "gh 설치"를 눌렀을 때 openssl이 올라가는 것은 부록 A가 업데이트 국면에서 우려한 것과 정확히 같은 문제이므로, 같은 처방(dry-run 프리뷰 + plan_id 게이트)을 설치 국면에도 적용한다.
2. **기존 `parse_brew_dry_run_affected`(1216행)를 그대로 쓰면 안 된다.** 그 파서는 `이름 old -> new` 형태만 인식하는데 **install dry-run 출력에는 `->`가 한 번도 나오지 않는다** → 파싱 결과가 빈 배열 → `affected = [label]`(길이 1) → 프론트의 `affected.length !== 1` 경고가 뜨지 않는다. 즉 **openssl@3이 업그레이드되는데 화면은 "gh 하나만 바뀝니다"라고 말하게 된다**(claimed≠verified 위반).
   → **신규 함수 `parse_brew_install_dry_run_affected(stdout)`**: `==>` 로 시작하고 `Would install`/`Would upgrade`를 포함하는 헤더 줄을 만나면 그 다음 비어있지 않은 · `==>`로 시작하지 않는 줄들을 이름으로 수집하고, **중복 제거**한다(출력이 두 번 반복된다). 대상 도구 자신은 목록에서 빼고 개수를 세어 notes를 만든다(리뷰 지적 #8의 off-by-one을 처음부터 만들지 않는다).
3. **`brew install --help` 실측 확인**: `-n, --dry-run`과 `-y, --no-ask, --yes`가 실재한다("Ask mode is the default"). 따라서 실행 argv는 `["install","-y","--formula","gh"]`.

**추가 env(설치 전용)**: `brew install`은 도움말에 따르면 `$HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK`가 없으면 **outdated dependents에 대해 `brew upgrade`를 돌린다**. 설치 하나가 무관한 패키지 업그레이드로 번지는 것을 좁히기 위해 설치 전용 env 상수를 둔다:
```
BREW_INSTALL_ENV = BREW_ENV + [("HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK","1")]
```
`HOMEBREW_NO_AUTO_UPDATE`는 부록 B.4대로 **여전히 설정하지 않는다**(끄면 오래된 인덱스로 거짓 판정). 타임아웃은 업데이트와 동일하게 600초, 프리뷰는 120초.

**포기한 것**: 의존성 업그레이드를 완전히 막을 수단은 brew에 없다(부록 A.4에서 이미 자인). **감당**: 프리뷰에 정확한 목록을 띄우고 plan_id로 그 시점 계획을 붙잡는다.

---

## 4. 의존성 순서와 부트스트랩 체인(③ 비정상 케이스)

### 4.1 체인

```
Xcode CLT (git) ──► Homebrew ──► node ──► npm ──┬──► Claude Code   (앱이 실행)
                        │                        └──► Wrangler      (앱이 실행, 기존)
                        └──────────────────────────► gh             (앱이 실행)
                        └──────────────────────────► pnpm / node    (앱은 안내만)
```

**설계 입장**: 이 앱은 체인의 **뿌리(CLT/git, Homebrew, node)를 설치하지 않는다.** 뿌리는 원래도 사람이 한 번 해야 하는 일이고(Homebrew 설치 자체가 `curl | bash`다), 앱은 뿌리가 갖춰진 머신에서 **중간(gh, Claude Code, Wrangler)** 을 자동화한다. 이 경계가 §1.2의 게이트 결과와 정확히 일치하는 것은 우연이 아니다 — G4(오탐 피해 국소성)가 곧 "뿌리인가 잎인가"를 묻는 질문이기 때문이다.

### 4.2 전제 미충족 시 동작 (표)

| 상황 | 현재 동작 | 이 설계의 요구 |
|---|---|---|
| Claude 설치 요청 + npm 미해석 | (신규) | `Manual(NoRunner)` — 문구를 **도구별로** 구체화: "npm을 찾을 수 없습니다. Node.js를 먼저 설치해주세요." + `brew install --cask claude-code` 대안 제시. 현행 `MANUAL_NO_RUNNER` 단일 문구("필요한 실행 도구(brew/npm)를 찾을 수 없어…")는 무엇을 해야 하는지 말하지 않는다 |
| gh 설치 요청 + brew 미해석 | (신규) | `Manual(NoRunner)` + "Homebrew가 필요합니다" + https://brew.sh 안내 |
| gh 설치 요청 + `<prefix>/Cellar` 또는 `<prefix>/bin` 쓰기 불가 | (신규) | `Manual(NotWritable)`. prefix는 **brew runner 경로의 조부모**로 구한다(`/opt/homebrew/bin/brew` → `/opt/homebrew`) — 미설치 도구에는 classify가 준 prefix가 없기 때문 |
| Claude 설치 요청 + `<npm prefix>/lib/node_modules` 쓰기 불가 | 리뷰 지적 #7(업데이트 경로에도 검사 없음) | `Manual(NotWritable)`. npm prefix는 **npm runner 경로의 조부모**로 구한다(실측 대조: `/opt/homebrew/bin/npm` → `/opt/homebrew`, `npm prefix -g` 출력과 일치). `<prefix>/lib/node_modules`가 아직 없으면 `<prefix>/lib`를 검사하고, 그마저 없으면 검사를 건너뛴다(없는 경로에 대해 `access(W_OK)`가 false를 주므로 fail-closed로 오작동하는 것을 막는다). 이 경우 EACCES는 exit code로 빠르게 드러난다 |
| 설치 실행 직전 그 도구가 이미 설치됨(다른 창에서 설치했거나 최초 탐지가 오탐) | (신규) | 실행하지 않고 `Outcome::AlreadyLatest` + "이미 설치되어 있습니다(경로: …)" — **신규 Outcome 변형을 만들지 않는다**(계약 유지, §6) |

### 4.3 Git 특유의 비정상 케이스 — `/usr/bin/git` 스텁

macOS의 `/usr/bin/git`은 CLT 미설치 상태에서도 **파일로 존재하는 xcrun 스텁**이다. 따라서 `resolve_tool_path`는 항상 Some을 돌려주고, `check_tool_version`이 매 화면 로드마다 `/usr/bin/git --version`을 실행한다. CLT가 없는 머신에서 이 호출은 **"명령어 개발자 도구를 설치하시겠습니까?" 시스템 GUI 대화상자를 띄울 수 있다** — 사용자가 앱 목록을 열었을 뿐인데 OS 설치 마법사가 뜨는 것은 이 앱이 낼 수 있는 가장 나쁜 종류의 부작용이다(**미검증**: 이 머신은 CLT가 설치돼 있어 재현 불가. `xcode-select -p` = `/Library/Developer/CommandLineTools` 실측).

**처방(둘 다 적용)**
1. `Git`의 `path_candidates`를 `["/opt/homebrew/bin/git", "/usr/local/bin/git", "/Library/Developer/CommandLineTools/usr/bin/git", "/usr/bin/git"]` 순으로 바꾼다 — 실제 바이너리를 먼저 잡아 정상 머신에서 스텁을 건드리지 않게 한다. 앞의 셋은 canonical이 각각 `Cellar`/CLT 경로이므로 기존 분류(`HomebrewFormula` / `SystemManaged`)가 그대로 동작한다.
2. **스텁 가드**: 해석된 경로가 정확히 `/usr/bin/git`이고 `/Library/Developer/CommandLineTools`와 `/Applications/Xcode.app` **둘 다 없으면**, 버전 조회를 실행하지 않고 `installed:false` + `actionKind:"manual"`(XcodeClt 안내)로 보고한다. 순수 파일 존재 검사만 쓰므로 프로세스를 하나도 띄우지 않는다.

### 4.4 설치 국면의 성공 판정(결정 4 재사용)

설치도 업데이트와 동일하게 **exit code는 주장, 재조회가 확인**이다. `perform_wrangler_install`이 이미 그 형태이므로(1884행~) 일반화만 한다:
- 실행 후 `resolve_tool_path`를 **처음부터 다시** 해석 → `check_tool_version` → `normalized_after`.
- exit 0 + after 있음 → `Updated`(메시지는 "설치되었습니다"), exit 0 + after 없음 → `UnknownAfter`(verified=false), exit≠0 → `Failed`(spawn_error가 있으면 그 원문을 메시지와 `log_tail`에 실어 리뷰 지적 #6을 신규 코드에서 반복하지 않는다), 타임아웃 → `TimedOut`.
- `compute_path_visibility`로 `pathVisible`/`pathHint`를 채운다 — 설치 직후 "설치는 됐는데 터미널에서 안 보인다"가 가장 흔한 사고이므로 설치 결과에서 특히 값어치가 있다.

---

## 5. Windows 범위 판단 (권고)

### 5.1 권고: **탐지는 이번 범위에 넣고, 실행은 넣지 않는다.**

**넣는 쪽(탐지, `path_candidates`)의 근거**
- 지금 `path_candidates`는 전부 POSIX 절대경로다. Windows 빌드에서는 절대경로 후보가 전부 실패하고 bare-name PATH 폴백만 남는데, 그 폴백은 `.cmd`/`.exe` 확장 문제와 GUI PATH 문제를 함께 안는다 → **설치돼 있는 도구를 "설치 안 됨"으로 표시**한다. 이는 단순한 기능 부재가 아니라 앱이 **거짓을 말하는 상태**이고, 그 거짓 위에서 "설치하세요" 안내까지 띄운다.
- 비용이 데이터에 한정된다: 도구별 후보 배열의 `#[cfg(windows)]` 분기 + `resolve_binary_expand_home`의 확장 토큰 확대(`~/` 외에 `%USERPROFILE%`, `%LOCALAPPDATA%`, `%APPDATA%`, `%ProgramFiles%` — `std::env::var` 조회, 셸 미경유) + `build_child_path_env`의 구분자(`:` → `;`)와 기본 디렉터리 분기.
- **탐지만 추가해도 실행 경로가 열리지 않는다(fail-closed 성질)**: `classify_install_method`에는 Windows 경로 규칙이 없으므로 모든 Windows 경로가 `Unknown`으로 떨어지고, `UPDATE_TABLE`의 최종 와일드카드가 `Manual(UnknownMethod)`로 보낸다. 즉 탐지 확장은 구조적으로 새 실행 표면을 만들 수 없다. 다만 `Manual(UnknownMethod)` 문구("설치 방식을 확인할 수 없어…")는 Windows에서 정확하되 불친절하므로, **`#[cfg(windows)]` 한정 도구별 안내(winget upgrade 명령 + 공식 URL)** 를 얹기를 권고한다.

**안 넣는 쪽(실행)의 근거 — 셋 다 기존 불변식을 건드린다**
1. **셸 경유**: Windows의 npm/pnpm은 `npm.cmd`/`pnpm.cmd` 배치 파일이고, Rust `std::process::Command`는 배치 파일을 **`cmd.exe`를 통해** 실행한다. 제약 ②(셸 미경유)를 정면으로 깬다. 우회로(`node.exe <prefix>\node_modules\npm\bin\npm-cli.js …`)는 존재하지만 검증 불가한 새 복잡도다.
2. **권한 상승**: winget의 다수 패키지는 머신 스코프 설치라 UAC 승격이 필요하고, 승격 프롬프트는 stdin=null 자식이 응답할 수 없다(부록 B.2 "sudo가 필요한 계획은 애초에 만들지 않는다"의 Windows판). 첫 실행 시 source agreement 동의도 요구한다.
3. **검증 불가**: 이 머신에 winget이 없다(macOS). 위임문의 실측 사실대로 Windows 경로는 **한 줄도 실행 검증할 수 없다.** Sensitive 등급에서 검증 없이 실행 표면을 넓히지 않는다.

**추가로 필요한 것**: `open_terminal_command`는 `osascript` 기반 macOS 전용이다(`cli_launcher.rs:66`). Windows에서 manual 카드의 "터미널에서 열기"가 무엇을 해야 하는지 정의가 없다.
→ **권고**: `#[cfg(windows)]`에서는 터미널을 열지 않고 `TerminalLaunchResult{ opened:false, message:"명령을 복사해 PowerShell에 붙여넣어 실행해주세요." }`를 반환한다. `TerminalLaunchResult`는 이미 `opened:bool + message:String`이므로 **계약 변경이 없다.** (PowerShell을 새로 spawn하는 방안은 실행 표면 확대라 배제.)

### 5.2 범위를 더 줄이는 대안도 정당한가

정당하다. **"Windows 탐지조차 v2로 미룬다"** 를 택할 경우의 최소 요건: Windows 빌드에서 개발 환경 화면 전체에 "이 화면은 현재 macOS만 지원합니다"를 표시해 **거짓 표시를 하지 않는 상태**로 만들 것. 아무 표시 없이 전부 "설치 안 됨"으로 두는 현행만은 남기지 않기를 권고한다 — 그건 MVP 속도가 아니라 오보다.

---

## 6. 프론트엔드 계약 판단

### 6.1 결론: **계약 변경 불필요.** 부록 C의 `DevToolStatus`/`DevToolPreview`/`DevToolActionResult`를 그대로 유지한 채 10칸 전부를 표현할 수 있다.

| 필요한 표현 | 기존 필드로 어떻게 | 확인한 소비처 |
|---|---|---|
| 미설치 + 앱이 설치 실행 가능 | `installed:false` + `actionKind:"run"` → 버튼 라벨 "설치" | `devTools.ts:372-374` |
| 미설치 + 안내만 | `installed:false` + `actionKind:"manual"` + `manualHint` → "안내 보기" 버튼 + 안내 패널 | `devTools.ts:356, 376-379, 446-` |
| 안내문 + 공식 문서 URL | `manual_display_message()`가 URL을 메시지에 접어 넣는 기존 방식 그대로(198행) | 동일 |
| 설치 프리뷰(무엇이 함께 바뀌나) | `DevToolPreview.affected` + `notes` + `previewReliable` | `devTools.ts:394-` |
| "앱이 뒤진 경로 목록" | `DevToolPreview.notes` 문자열에 포함 | 동일 |
| 설치 결과 | `DevToolActionResult` + 기존 `Outcome` 6종 재사용(`Updated`/`UnknownAfter`/`Failed`/`TimedOut`/`AlreadyLatest`) — **신규 variant 없음** | `devTools.ts:151` |
| Windows 터미널 열기 불가 | `TerminalLaunchResult{opened:false, message}` | 기존 |

`Outcome::AlreadyLatest`를 "이미 설치되어 있습니다"에 재사용하는 것이 유일하게 어색한 지점이다. 대안(`Outcome::AlreadyInstalled` 신설)은 프론트 `notifyOutcome`/`renderResultPanel`/타입 정의를 함께 고쳐야 해 계약 변경이 되고, 얻는 것은 문구 정확도뿐이다. `message`가 한국어로 정확히 말해 주므로 재사용을 택한다.

### 6.2 계약을 안 바꾸면서 **프론트 동작이 달라지는 지점** (구현자 주의)

- 지금 미설치 non-Wrangler 도구는 `actionKind:"none"` → 비활성 "실행 불가" 버튼이 뜬다(`devTools.ts:381-382`). 그런데 `install_manual_plan()`(679행)에는 도구별 안내문과 `doc_url`이 이미 정성껏 채워져 있는데 **화면에 도달하는 경로가 없다** — `renderManualPanel`은 `actionKind === 'manual'`일 때만 열리기 때문이다. 이번 변경으로 `"none"` → `"manual"`이 되면 그 죽어 있던 안내가 살아난다. 계약 변경이 아니라 **기존 계약의 미사용 분기를 쓰기 시작하는 것**이다.
- 결과적으로 `actionKind:"none"`을 반환하는 코드 경로는 사라진다. 프론트의 `none` 처리는 방어적으로 남겨 둔다(제거하지 말 것).

### 6.3 리뷰 지적 #3(막다른 골목)을 구조적으로 없애는 요구사항

`runPlan`은 `tool.installed ? updateDevTool : installDevTool`로 분기한다(`devTools.ts:148`). 반면 `installed`는 `version.is_some()`이라(1483행) "바이너리는 있는데 `--version`이 실패"하면 `installed:false` + `actionKind:"run"`이 되어 프리뷰는 update용 plan_id를, 실행은 install용 대조를 쓰게 되고 **영구 불일치**가 난다.

**요구 1 — 단일 정본 함수**: 설치 계획을 만드는 경로를 하나로 모은다.
```rust
enum InstallResolution {
    Run { runner_path: String, argv: Vec<String>, plan: RunPlan,
          installer_label: &'static str, searched: Vec<String> },
    Manual(ManualPlan),
}
fn resolve_install_plan(tool: ToolId, runners: &ResolvedRunners) -> InstallResolution
```
`check_dev_tools_blocking`(actionKind 산출), `perform_preview`(미설치 분기), `perform_install`(실행 직전 재계산)이 **모두 이 함수만** 호출한다. 세 곳이 각자 분기하지 않으므로 plan_id 종류가 어긋날 수 없다. plan_id 규약은 Wrangler 현행 그대로 — `Run`이면 `compute_plan_id(runner_path, argv, "")`, `Manual`이면 `compute_plan_id_for_manual(tool, plan)`.

**요구 2 — install→update 위임**: `perform_install`은 첫 줄에서 `resolve_tool_path(def).is_some()`이면 `perform_update(tool_id, plan_id)`로 위임한다. 프리뷰가 update 경로에서 나왔다면 plan_id가 그대로 맞아떨어져 한 번에 성공하고, 위 불일치 조합이 소멸한다.

**요구 3 — 하드코딩 제거**: `check_dev_tools_blocking`의 `if def.id == ToolId::Wrangler`(1464~1468행)를 `matches!(resolve_install_plan(...), InstallResolution::Run{..})` 판정으로 교체한다. Wrangler는 규칙의 결과가 되고 예외가 아니게 된다.

**주의(성능)**: `resolve_install_plan`은 brew/npm/pnpm runner 해석을 필요로 한다. 화면 1회 로드에서 도구 6개마다 각각 해석하면 `resolve_binary`의 bare-name 폴백(`cli_launcher.rs:27` — **타임아웃 없음**, 리뷰 지적 #12)이 최대 수십 회 실행될 수 있다.
→ **요구 4**: `check_dev_tools_blocking` 진입 시 brew/npm/pnpm을 **1회만** 해석해 `ResolvedRunners` 구조체로 만들어 내려보낸다. 지적 #12 자체(폴백에 타임아웃·stdin 차단이 없음)의 수정은 이 위임 범위 밖이지만, **이번 변경이 그 결함의 노출 횟수를 늘리지 않게** 하는 것은 이 설계의 책임이다.

---

## 7. 신설·수정되는 데이터 (구현자용 요약)

```rust
// ── 신규 RunPlan (전부 Arg::Lit — §1.1 불변식) ──
const RUN_BREW_INSTALL_GH: RunPlan = RunPlan {
    runner: Runner::Brew,
    args:         &[Lit("install"), Lit("-y"), Lit("--formula"), Lit("gh")],
    preview_args: Some(&[Lit("install"), Lit("-n"), Lit("--formula"), Lit("gh")]),
    env: &BREW_INSTALL_ENV,      // BREW_ENV + HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK=1
    timeout_secs: 600,           // 프리뷰는 호출부에서 120s
};
const RUN_NPM_INSTALL_CLAUDE: RunPlan = RunPlan {
    runner: Runner::Npm,
    args:         &[Lit("install"), Lit("-g"), Lit("@anthropic-ai/claude-code")],
    preview_args: None,          // 근거 §7.1
    env: &NPM_ENV,
    timeout_secs: 300,
};
// ── UPDATE_TABLE에 추가(제네릭 NpmGlobal 행보다 앞) — §3.1 ──
const RUN_NPM_CLAUDE_LATEST: RunPlan = RunPlan {
    runner: Runner::Npm,
    args: &[Lit("install"), Lit("-g"), Lit("@anthropic-ai/claude-code@latest")],
    preview_args: None, env: &NPM_ENV, timeout_secs: 300,
};

// ── INSTALL_TABLE: (도구) → Action. 없는 도구는 install_manual_plan()으로 ──
static INSTALL_TABLE: &[(ToolId, RunPlan)] = &[
    (ToolId::Gh,       RUN_BREW_INSTALL_GH),
    (ToolId::Claude,   RUN_NPM_INSTALL_CLAUDE),
    (ToolId::Wrangler, RUN_PNPM_GLOBAL_ADD),   // npm 폴백은 기존 로직 유지
];
// 이 테이블은 #[cfg(target_os = "macos")]에서만 비어 있지 않다(§5).
```

`path_candidates` 변경(설치 후 경로 = (c)열 요구):
- `Claude`: `~/.npm-global/bin/claude`, `~/.claude/local/claude` **추가**(npm 커스텀 prefix / 레거시 로컬 설치를 못 보고 중복 설치하는 것을 막는다 — G4 완화).
- `Git`: 순서 재배치 + CLT 경로 추가(§4.3).
- 나머지 3개(node/pnpm/gh): `run` 셀의 설치 결과가 이미 후보 안에 들어오므로 **변경 없음**.

`install_manual_plan()` 문구·URL 보강: pnpm(§3.2), Node(§3.3), Git, 그리고 Windows용 `#[cfg]` 분기(§2 표의 (d)열 문자열을 그대로 사용).

### 7.1 Claude 설치에 프리뷰를 두지 않는 이유

`npm install -g --dry-run`이 존재하긴 하나, ① 전역 npm 설치는 의존성이 그 패키지 트리 안에서만 끝나 brew처럼 **다른 도구를 업그레이드하지 않는다**(부록 A가 걱정한 종류의 blast radius가 없다), ② dry-run에 레지스트리 전체 해석 시간이 그대로 들어 확인 단계가 수십 초 늘어난다, ③ 기존 Wrangler 설치가 동일하게 프리뷰 없이 동작 중이라 일관된다.
대신 **프리뷰 `notes`에 "앱이 뒤진 경로 목록"을 채워** 사용자가 "나 이미 깔려 있는데?"를 스스로 잡아낼 수 있게 한다(§0.1 사고 ①에 대한 처방). `will_run:true`인 프리뷰가 여전히 존재하므로 plan_id 게이트와 사용자 확인 단계는 유지된다.

---

## 8. 비정상 케이스 총괄(③ 의무 — 이번 확장분만)

| 케이스 | 처리 |
|---|---|
| 설치 중 다른 설치/업데이트 동시 요청 | 기존 `EXECUTION_LOCK.try_lock()` 재사용, 즉시 거부(대기 없음) |
| 설치 프리뷰(brew `-n`)와 실행 사이에 brew 인덱스가 갱신됨 | plan_id 불일치 → 재프리뷰 요구(부록 A). **한계 그대로 상속**: plan_id는 argv를 붙잡지 영향 목록을 붙잡지 않는다(리뷰 #14 = 설계 한계). 설치 프리뷰 패널에도 "실행 시 목록이 달라질 수 있습니다"를 표기 |
| 설치 명령이 exit 0인데 바이너리가 없음(pnpm postinstall 차단 등) | `UnknownAfter` + `verified:false`. 성공으로 뭉개지지 않는다 |
| 설치 성공했으나 셸 PATH에 없음 | `pathVisible:false` + `pathHint` — 결과 패널에서 안내 |
| 설치 도중 타임아웃 | `TimedOut`(verified=false) + 프로세스 그룹 kill(SIGTERM→3s→SIGKILL). npm 전역 설치가 반쯤 끝난 상태가 남을 수 있음 → 재실행이 멱등(같은 명령 재실행이 복구 경로) |
| 멱등성 | `brew install -y --formula gh`·`npm install -g <pkg>`는 이미 설치돼 있으면 no-op/재설치로 안전. 재시도에 별도 처리 불필요 |
| rate limit / 네트워크 실패 | exit≠0 + `log_tail`에 원문. `spawn_error`가 있으면 메시지로 승격(지적 #6을 신규 코드에서 반복하지 않음) |
| 미설치 도구인데 화면 로드마다 프로세스가 뜬다 | §6.2 요구 4(runner 1회 해석) + §4.3(git 스텁 가드)로 제한 |

---

## 9. 미검증 항목 (정직 보고)

이 머신은 macOS/Apple Silicon이고 **5개 도구가 전부 이미 설치돼 있어** 설치 실행 자체를 재현할 수 없었다. 아래는 문서·읽기전용 조회에 근거한 설계이며 실행 검증되지 않았다.

1. **Windows 10칸 전부** — winget 부재로 명령·경로·권한 동작 전부 미검증. §2 표의 Windows 경로는 공식 문서(Claude Code 제거 안내의 `%USERPROFILE%\.local\bin\claude.exe`, Git for Windows `winget install --id Git.Git -e --source winget`, GitHub CLI `winget install --id GitHub.cli --source winget`)와 일반적 기본 설치 위치에서 온 값이다.
2. **`npm install -g @anthropic-ai/claude-code` 실행 후 `/opt/homebrew/bin/claude`의 심볼릭 링크 대상** — 플랫폼 optional dep을 가리키는지 메인 패키지를 가리키는지 확인 못 함. §3.1의 리터럴 업데이트 행이 이 불확실성에 대한 방어다.
3. **pnpm v10의 postinstall 차단이 Claude Code 패키지에 실제로 영향을 주는지** — 미확인. 영향받지 않는다면 Claude 설치에도 pnpm 폴백을 추가할 수 있다(보수적으로 뺐다).
4. **CLT 미설치 머신에서 `/usr/bin/git --version`이 GUI 대화상자를 띄우는지** — 이 머신은 CLT가 있어 재현 불가(§4.3). 가드는 프로세스를 안 띄우는 순수 파일 검사라 이 가정이 틀려도 손해가 없다.
5. **`brew install -n --formula gh` 출력 형식** — `wget`으로 대리 실측했다(gh는 이미 설치돼 있어 "already installed" 경고만 나온다). 섹션 헤더 형식이 formula마다 다를 가능성은 낮지만 실측된 것은 wget 케이스다.
6. **Intel 맥(`/usr/local` prefix)** — 리뷰 지적 #2와 같은 이유로 미검증. 설치 경로의 쓰기권한 검사는 §4.2대로 `<prefix>/Cellar`·`<prefix>/bin`을 본다.

---

## 10. 자기검증 (설계 4대 의무)

- **① 트레이드오프**: §3.1(pnpm 폴백/cask/네이티브 설치기 3안 기각), §3.1 말미(파싱 원칙에 대한 좁은 예외와 그 범위 한정), §3.3(Node `run` 승격 반론과 기각 근거 + 승격 트리거), §3.4(프리뷰 필수화와 brew의 구조적 한계 자인), §5.1~5.2(Windows 넣는 쪽/빼는 쪽/더 줄이는 쪽 3안), §6.1(`AlreadyLatest` 재사용 vs 신규 variant), §7.1(프리뷰 생략) — 모든 주요 선택에 대안·포기한 것·감당 방안 명시.
- **② 고유성**: §0.1에 현행(수동 설치) 대비 3개 축을 인용하고 각각을 구조로 연결(뒤진 경로 나열 / 공식 고정 식별자만 리터럴 / 설치 후 재조회+pathVisible). 표준 부분(테이블 조회, 결과 렌더)은 얕게 두고, 이 프로젝트 고유 문제인 **"파일시스템 근거 없이 argv를 만들어야 한다"**에 §1.1~1.2를 집중했다.
- **③ 비정상**: §4.2(전제 미충족 5종) · §4.3(git 스텁 GUI 대화상자) · §4.4(성공 판정) · §8(동시실행·인덱스 변동·조용한 실패·PATH 미노출·타임아웃·멱등성·spawn 실패·프로세스 폭증) — 설치 국면 고유의 케이스를 전부 다룸.
- **④ 완결성**: §2의 10칸이 (a)~(e) 전부 채워짐. 신규 argv는 §7에 실행/프리뷰/env/타임아웃까지 코드 형태로 명시. `path_candidates` 변경분 명시. 계약 필드별 매핑표(§6.1)와 소비처 라인번호 대조. 미검증 항목 6건을 §9에 분리 표기.
- **보안 규약 대조**(Skill `domain-backend-api-security` "언제 정하는가" — 구조적 결정만 확정, 로직 강도는 구현으로): 이 산출물은 HTTP API가 아니라 로컬 프로세스 실행 표면이므로 해당 절의 항목 중 **입력 신뢰 경계**(프론트 입력은 `toolId`+`planId` 뿐 → 화이트리스트 enum 전환 후 원본 폐기, 부록 C 그대로)와 **권한 경계**(새 permission 0건, `capabilities/default.json` 무변경, sudo/UAC 필요한 계획은 생성 금지)를 값으로 확정했다. 인젝션 방어 강도는 §1.1의 리터럴 전용 불변식으로 **구조적으로** 해결돼 런타임 검증 강도 조절이 필요 없다.
