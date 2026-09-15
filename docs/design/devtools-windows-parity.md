# 개발 환경 화면 — Windows 완전 지원 설계 합의서

작성: architect / 대상: `src-tauri/src/dev_tools/**`, `src-tauri/src/cli_launcher.rs`, `src-tauri/src/process_util.rs`, `.github/workflows/` / 등급: **Sensitive**(실행 경계 이동 — 새 OS에서 프로세스 생성·설치 프로그램 실행)
WBS: `01m2htjqt56ss3mg0x2eh09zs1`(설계 합의) / malgnai-hub project: `01m1gng9ppnm67283p189pq3t7`
범위: **설계 결정만.** 이 문서를 만드는 과정에서 `src/`·`src-tauri/src/` 파일은 한 줄도 수정하지 않았다.

정본 관계: **`docs/design/devtools-install-matrix.md`(G1~G4 게이트 + 10칸 매트릭스 + §6.3 요구 1~3)의 후속이다.** 그 문서의 §5.1/§5.2에서 "Windows는 탐지조차 v2로 미룬다"로 내린 결론을 **뒤집는다.** 뒤집는 것은 그 한 가지뿐이고, G1~G4 게이트·리터럴 전용 argv 불변식(§1.1)·단일 정본 `resolve_install_plan`(§6.3)·프론트 계약 불변(§6.1)은 **전부 그대로 상속한다.**

---

## 0. 결정 요약 (한 줄씩)

| 항목 | 결정 |
|---|---|
| **A. 컴파일 검증** | 로컬 크로스체크는 **포기**(msvc는 실측 실패, gnu/xwin은 툴체인 설치 필요 + 출하 타깃과 불일치). **GitHub Actions `windows-latest`에서 `cargo test`를 돌리는 잡을 `ci.yml`에 신설**하고, 그 잡을 이 작업 전체의 선행 조건(Phase 0)으로 삼는다. 로컬 방어선은 **플랫폼을 `cfg`가 아니라 인자로 받는 순수함수화**(D)로 확보한다 |
| **B. 설치 경로** | **winget 러너를 신설한다.** 단 적용 대상은 **gh 하나**(+업데이트 국면의 winget 설치본). claude=npm, wrangler=pnpm→npm, pnpm/node/git=manual — **macOS와 run 3 / manual 3으로 정확히 동수**이며 이는 우연이 아니라 같은 G1~G4 게이트를 Windows 후보에 적용한 결과다 |
| **C. 게이트 해제** | **3단계(Phase 0 CI → Phase 1 탐지 → Phase 2 실행), 피처 플래그는 두지 않는다.** 중간 상태는 플래그가 아니라 **테이블에 행을 넣지 않는 것**으로 표현한다(기존 fail-closed 성질 재사용). 롤백은 포터블 exe 교체 + `git revert`로 충분 |
| **D. 탐지** | `dev_tools/platform.rs` 신설. `Platform`을 **인자로** 받는 순수함수 8개(경로 토큰 확장·확장자·PATH 합성·PATH 스캔·인용)로 Windows 로직 전체를 **macOS에서 `cargo test`로 실행 검증**한다. `cfg`는 `platform_now()`와 실제 syscall 2곳으로 격리. Windows에서는 **bare-name 프로브 실행을 폐지**하고 명시적 PATH 스캔으로 절대경로를 확정한다 |
| **E. 터미널** | `powershell.exe -NoLogo -NoExit -Command` + **`CREATE_NEW_CONSOLE`(0x10)** 명시. `process_util.rs`에 `silent()`의 형제 `windowed()`를 추가한다 — `f983216`과 **충돌하지 않는다**(그 커밋은 이 경로를 명시적으로 제외했고, 같은 플래그 계열의 반대 방향이다) |
| **F. 파일 분리** | 신규 `platform.rs`(~330줄) + `install_resolver.rs`(754줄, 증분 후 ~930줄 예상)를 **`runners.rs`로 분할**. 그 둘만 하면 나머지는 1,000줄 규율 안에 남는다 |
| **G. 보안** | 쟁점 8개. 가장 값어치 있는 결정 둘: ① **Windows bare-name 폴백 폐지**(CreateProcess의 CWD 검색으로 인한 PATH 하이재킹을 구조적으로 제거) ② **`rust-version` 하한 1.81 고정**(BatBadBut CVE-2024-24576/43402 완화가 `.cmd` 러너 실행의 전제) |

---

## 1. 내가 실제로 실행한 명령과 출력 (A의 근거)

### 1.1 msvc 크로스체크 — 재현했고, 실패한다

```
$ cd src-tauri && cargo check --target x86_64-pc-windows-msvc
...
  cargo:warning=/Users/hopegiver/.cargo/registry/src/index.crates.io-.../ring-0.17.14/include/ring-core/check.h:27:11: fatal error: 'assert.h' file not found
  cargo:warning=   27 | # include <assert.h>
  cargo:warning=1 error generated.

  --- stderr
  error occurred in cc-rs: command did not execute successfully (status code exit status: 1): LC_ALL="C" "cc" "-O3" ... "--target=x86_64-pc-windows-msvc" ...

$ cargo check --target x86_64-pc-windows-msvc 2>&1 | grep -E "^(error|warning: build failed)"
error: failed to run custom build command for `ring v0.17.14`
warning: build failed, waiting for other jobs to finish...
```

위임서의 실측과 동일하게 재현됐다. 원인은 rustup 타깃 부재가 아니라 **`ring`의 `build.rs`가 cc-rs로 C 코드를 컴파일하는데, 이 머신의 `cc`(Apple clang)에 MSVC CRT 헤더(`assert.h`)가 없기 때문**이다. `cargo check`는 링크는 하지 않지만 **빌드 스크립트의 C 컴파일은 수행**하므로 이 벽을 피할 수 없다.

`ring`은 회피 가능한 의존성이 아니다:
```
$ cargo tree -i ring
ring v0.17.14
├── jsonwebtoken v9.3.1  → tauri-app
├── rustls v0.23.44 → hyper-rustls → reqwest v0.12.28 → tauri-app
└── rustls-webpki → rustls
```
`jsonwebtoken`(Google OAuth JWKS 검증)과 `reqwest`의 `rustls-tls` 양쪽에서 들어온다.

### 1.2 대안 툴체인 — 이 머신에 하나도 없다

```
$ rustup target list --installed
aarch64-apple-darwin
x86_64-pc-windows-msvc

$ rustc -vV | head -3
rustc 1.98.1 (48a229cea 2026-09-01)
host: aarch64-apple-darwin

$ which x86_64-w64-mingw32-gcc clang-cl lld-link cargo-xwin
x86_64-w64-mingw32-gcc not found
clang-cl not found
lld-link not found
cargo-xwin not found

$ brew list --formula | grep -iE 'mingw|llvm'
(없음)
```
→ gnu 타깃도 xwin도 **툴체인 설치가 선행 조건**이다. 금지 범위(“brew/npm으로 새 툴체인을 설치하지 마라”)에 걸려 시도하지 않았다. **설치 필요**로 보고한다.

### 1.3 CI — 이미 Windows에서 Rust를 컴파일하고 있다 (결정적 사실)

```
$ gh repo view --json visibility,isPrivate
{"isPrivate":false,"visibility":"PUBLIC"}

$ gh run view 34895836500 --json jobs -q '.jobs[] | "\(.name) \(.conclusion) \(.startedAt) \(.completedAt)"'
build (macos-latest)    success start=2026-09-14T20:55:58Z end=2026-09-14T20:59:46Z   (3m48s)
build (windows-latest)  success start=2026-09-14T20:55:53Z end=2026-09-14T21:00:58Z   (5m05s)
```

`.github/workflows/tauri-portable-build.yml`이 `windows-latest`에서 `pnpm tauri build --no-bundle`을 돌린다 — 이건 **릴리스 프로필 전체 Rust 컴파일**이다. 즉 *프로덕션 코드*의 Windows 컴파일 검증은 **이미 존재하고, 실제로 매 main push마다 5분 만에 통과하고 있다**(`f983216`의 `CREATE_NO_WINDOW` 코드가 이 경로로 컴파일 검증됐다).

빠져 있는 것은 정확히 두 가지다:
1. **`#[cfg(test)]` 코드는 `cargo build`가 컴파일하지 않는다.** 그래서 `process_util.rs:134`의 `#[cfg(windows)] fn silent_does_not_break_spawning_on_windows`와 `config/user_config.rs:370`의 `#[cfg(windows)] fn validate_workspace_entries_strips_extended_length_prefix`는 **지금까지 단 한 번도 컴파일된 적이 없다.** 컴파일조차 안 되는 코드가 저장소에 "테스트"라는 이름으로 들어 있다.
2. 트리거가 `push: branches: [main]`뿐이다 — 병합 **후에야** 깨진 걸 안다.

### 1.4 Windows CI 테스트를 켜면 즉시 깨지는 기존 테스트 (비용 실측)

```
$ grep -rn "#\[test\]" src-tauri/src | wc -l          →  283
$ grep -rn -B1 "#\[test\]" src-tauri/src | grep -c "cfg(unix)"  →  4

$ cd src-tauri && cargo test     # (warm cache)
test result: ok. 279 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 2.76s
ELAPSED_SEC=4
```

**산술이 §1.3의 지적을 그대로 확인한다**: 소스에 `#[test]`가 **283개**인데 macOS에서 컴파일·수집된 것은 **281개**(279 passed + 2 ignored)다. 차이 **2개**가 정확히 `process_util.rs:134`와 `config/user_config.rs:370`의 `#[cfg(windows)]` 테스트 — **컴파일된 적이 한 번도 없는 코드**다. 또한 전체 테스트가 **2.76초**에 끝난다(웜 캐시 기준, 총 4초) → Windows 러너에서도 **테스트 실행 자체는 비용이 아니고, 비용은 전부 컴파일**이다. 캐시가 도는 한 §1.3의 5분 릴리스 빌드보다 싸질 것으로 본다(**미측정 — 추정**).

283개 중 `#[cfg(unix)]` 격리된 것은 4개뿐이다. POSIX 바이너리를 **실제로 spawn하는** 미격리 테스트를 찾으면:

| 파일:행 | 호출 | 조치 |
|---|---|---|
| `dev_tools/process.rs:391` | `run_process_with_timeout("/bin/echo", …)` | `#[cfg(unix)]` 격리 + Windows 대응본(`cmd /C echo`) |
| `dev_tools/process.rs:409` | `run_process_with_timeout("/bin/sleep", …)` | 동일(`timeout /T`) |
| `dev_tools/process.rs:429` | `run_process_with_timeout("/bin/cat", …)` | 동일(`more`) |
| `mcp_manager/mod.rs:277,289` | `Command::new("sh")` | `#[cfg(unix)]` 격리 |
| `process_util.rs:92,125`, `session_list.rs:694` | `sleep`/`echo` | **이미 `#[cfg(unix)]`** — 조치 불필요 |

→ **Phase 0의 실제 비용은 "테스트 5개 격리 + Windows 대응본 3개 추가"** 수준이다. 나머지 POSIX 문자열 리터럴(`classify.rs` 23건, `install_resolver.rs` 22건 등)은 전부 **순수 문자열 데이터**라 Windows에서도 그대로 통과한다(파일시스템을 건드리지 않는 분류 로직의 입력값이다).

### 1.5 `f983216`과의 관계 확인

```
$ git show f983216 --stat
 src-tauri/src/cli_launcher.rs | 13 ++++++-
 src-tauri/src/process_util.rs | 65 +++++++++++++++++++
 ... (8 files)
```
`cli_launcher.rs`의 13줄은 `resolve_binary`의 bare-name `--version` 프로브에 `.silent()`를 붙인 것이고, 같은 커밋의 주석이 **`open_terminal_command`(터미널 오픈 경로)는 의도적으로 제외한다**고 명시한다(`cli_launcher.rs:75-79`, `process_util.rs:41-45`). → E의 `windowed()` 추가는 그 주석이 이미 열어 둔 자리에 들어간다. **충돌 없음.**

---

## A. Windows 컴파일 검증을 어떻게 확보할 것인가

### A.1 선택지 비교

| # | 방법 | 이 머신에서의 실태 | 컴파일 검증 | **실행** 검증 | 도입 비용 | 신뢰도 |
|---|---|---|---|---|---|---|
| 1 | `cargo check --target x86_64-pc-windows-msvc` | **실패 확인**(§1.1). ring의 C 빌드가 MSVC 헤더 부재로 죽음 | ✗ | ✗ | — | — |
| 2 | msvc + **xwin/clang-cl** | `clang-cl`·`lld-link`·`cargo-xwin` 전부 부재(§1.2). brew llvm(~2GB) + xwin이 내려받는 MS CRT/SDK(~1GB, 라이선스 동의 필요) | △ (출하 타깃과 동일) | ✗ | **설치 필요**, ~3GB, CI에도 같은 셋업 필요 | 중 |
| 3 | `x86_64-pc-windows-gnu` + **mingw-w64** | `x86_64-w64-mingw32-gcc` 부재(§1.2). `brew install mingw-w64` 필요 | △ | ✗ | **설치 필요**, ~400MB | **낮음** — 출하 타깃(msvc)이 아니다. Tauri/`windows`/`webview2-com` 계열이 gnu에서 다르게 동작하거나 아예 빌드되지 않을 수 있어, **가짜 실패**로 개발을 막을 위험이 진짜 실패를 잡는 값어치보다 크다 |
| 4 | **GH Actions `windows-latest` + `cargo test`** | 러너 실재, **이미 Windows 컴파일이 5분에 통과 중**(§1.3). repo가 **PUBLIC**이라 표준 러너 분(minutes)이 무료 | ✓ **출하 타깃 그대로(msvc)** | ✓ **실제 Windows에서 테스트가 돈다** | 워크플로 잡 1개 + 기존 테스트 5개 격리(§1.4) | **높음** |
| 5 | 검증 포기 | — | ✗ | ✗ | 0 | Sensitive 등급에서 불가 |

### A.2 결정

> **#4를 채택한다.** `.github/workflows/ci.yml`에 `rust-test` 잡을 신설한다.
> - `strategy.matrix.os: [windows-latest, macos-latest]`, `fail-fast: false`
> - `cargo test --manifest-path src-tauri/Cargo.toml`(**debug 프로필** — 릴리스 LTO(`lto=true`, `codegen-units=1`)를 피해 컴파일 시간을 줄인다)
> - 트리거: `push: branches: ['**']` + `pull_request`, **`paths: ['src-tauri/**', '.github/workflows/ci.yml']`**
> - `Swatinem/rust-cache`를 `tauri-portable-build.yml`과 **같은 sha 핀으로** 재사용, `workspaces: src-tauri -> target`
> - 액션 sha 핀·`permissions: contents: read`·시크릿 미사용 — 기존 두 워크플로의 보안 원칙을 그대로 계승(`GOOGLE_OAUTH_CLIENT_SECRET`은 `build.rs`가 `option_env!()`로 읽으므로 **테스트 잡에는 주입하지 않는다**)

**macOS를 매트릭스에 같이 넣는 이유는 C(회귀 방어)다** — "macOS 동작이 한 줄도 안 바뀐다"는 주장을 사람의 기억이 아니라 CI가 판정하게 만든다. PUBLIC 저장소라 러너 분이 무료이므로 이 추가에 금전 비용이 없다.

**Rust 버전 고정을 함께 한다**: `src-tauri/Cargo.toml`에 `rust-version = "1.81"`(BatBadBut 완화 전제 — G 참조)을 명시하고, CI의 `dtolnay/rust-toolchain`은 기존대로 `stable` 핀을 쓴다. 지금 이 저장소에는 MSRV 선언이 없다.

### A.3 트레이드오프

- **포기한 것**: 로컬에서 Windows 컴파일 오류를 즉시 아는 능력. 피드백 루프가 "로컬 수초" → "CI 수분(콜드 캐시면 더)"으로 늘어난다.
- **감당 방안(핵심)**: **D의 순수함수화.** Windows 로직의 대부분을 `cfg`가 아니라 `Platform` 인자로 분기시켜, macOS `cargo test`가 Windows 분기의 **로직**까지 전부 실행하게 만든다. CI만이 검증할 수 있는 잔여 `cfg(windows)` 표면을 **의도적으로 50줄 미만의 얇은 syscall 껍데기로 묶는다**. 이게 A가 막혔을 때의 진짜 방어선이고, 순수함수 비중이 높을수록 CI 왕복 횟수가 줄어든다.
- **감당 방안(부차)**: CI 러너는 간헐적으로 인프라 실패한다(실측: run `34791700934`의 macOS 잡이 `Could not resolve host: github.com`으로 체크아웃 단계에서 실패 — 컴파일 오류가 아니었다). `fail-fast: false` + 재실행으로 대응하고, **인프라 실패를 코드 실패로 오독하지 않는다**.
- **여전히 못 하는 것**: `winget`의 실제 동작(설치 성공, UAC 승격, 설치 후 경로)은 CI의 GitHub 호스티드 러너에서도 **신뢰할 수 없다** — 러너는 관리자 권한으로 돌고 소프트웨어가 미리 깔려 있어 실제 직원 PC와 환경이 다르다. **Phase 2는 실기(사내 Windows PC) 검증을 통과해야만 머지한다**(C 참조).

### A.4 채택하지 않은 대안에 대한 추가 메모

- **#2 xwin을 "나중에라도" 쓸 이유**: 컴파일 피드백을 로컬로 되돌리고 싶어질 때의 유일한 현실적 경로다(출하 타깃과 동일). 지금 도입하지 않는 이유는 ①금지 범위의 설치가 필요 ②Phase 0이 이미 더 높은 신뢰도(실행 검증 포함)를 더 싸게 준다 ③D의 순수함수화가 로컬 피드백 필요를 크게 줄인다. **승격 트리거**: 잔여 `cfg(windows)` 표면이 계획(50줄)을 크게 넘겨 CI 왕복이 개발을 실제로 막기 시작하면 재검토한다.
- **#3 gnu를 배제한 진짜 이유**는 설치 비용이 아니라 **검증 대상 불일치**다. gnu에서 통과해도 msvc에서 깨질 수 있고(반대도 마찬가지), 그러면 "검증됐다"는 말이 거짓이 된다. 정직 규율상 **틀린 대상에 대한 녹색 신호가 무검증보다 나쁘다.**

---

## B. 도구 × 플랫폼 설치/업데이트 확정 매트릭스

판정 규칙은 새로 만들지 않는다 — `devtools-install-matrix.md` §1.2의 **G1(고정 식별자) / G2(셸 불필요) / G3(설치 후 자리 예측 가능) / G4(오탐 피해 국소적)** 를 Windows 후보에 그대로 적용한다.

### B.1 결정 — winget 러너를 신설한다. 단 **gh 하나**에만 쓴다

| 도구 | 설치(미설치 → 설치) | 업데이트(설치됨 → 최신) | 판정 | 게이트 근거 |
|---|---|---|---|---|
| **gh** | `winget install --id GitHub.cli -e --source winget --accept-source-agreements --accept-package-agreements --disable-interactivity --silent` | `winget upgrade --id GitHub.cli -e --source winget --accept-source-agreements --disable-interactivity --silent` | **run** (winget 해석 성공 시) / winget 없으면 **manual** | G1 ✓ `GitHub.cli` 고정 id / G2 ✓ winget.exe는 실행파일, argv 배열 / G3 ✓ `%LOCALAPPDATA%\Microsoft\WinGet\Links\gh.exe` + `C:\Program Files\GitHub CLI\gh.exe` 예측 가능 / G4 ✓ gh를 버전매니저로 관리하는 관행 없음 |
| **Claude Code** | `npm install -g @anthropic-ai/claude-code` (러너: `%APPDATA%\npm\npm.cmd` 등) | ① npm 설치본 → `npm install -g @anthropic-ai/claude-code@latest`(리터럴 고정) ② 네이티브 설치본(`%USERPROFILE%\.local\bin\claude.exe`) → `claude update`(`Runner::SelfBinary`, 기존 `RUN_CLAUDE_UPDATE` 재사용) | **run** (npm 해석 시) / 아니면 **manual** | G1 ✓ 패키지명 고정 / G2 ✓ (`.cmd` 경유는 G 참조) / G3 ✓ / G4 ✓ — **단 신규 설치는 winget/네이티브가 아니라 npm으로만 한다**(사유 B.2) |
| **Wrangler** | `pnpm add -g wrangler` → 실패 시 `npm install -g wrangler`(기존 `install_candidates` 테이블 **무변경**, 러너 경로만 Windows 해석) | 기존 `PnpmGlobalPackage`/`NpmGlobal` 행 그대로 | **run** | macOS와 동일 |
| **pnpm** | — | — | **manual** (`winget install pnpm.pnpm` 안내) | G3·G4 탈락. winget `pnpm.pnpm`은 `PNPM_HOME`/PATH 갱신이 설치기 몫이라 앱이 결과 자리를 보증할 수 없고, 업스트림에 **패키지 id 불일치·업그레이드 실패 이슈가 보고돼 있다**(winget-cli #4751, pnpm #9933, winget-cli #3616 "pnpm missing from path after updating"). corepack 관리본은 애초에 앱에 안 보인다 |
| **Node.js** | — | — | **manual** (`winget install OpenJS.NodeJS.LTS` 안내) | G1 탈락(`OpenJS.NodeJS` vs `.LTS` — **앱이 사용자 대신 채널을 고르게 된다**) + G4 탈락(nvm-windows/fnm/volta 관리본을 앱이 못 본다 → 두 번째 node가 PATH를 뒤집는다). macOS의 §3.3 판단과 **완전히 같은 이유** |
| **Git** | — | — | **manual** (`winget install --id Git.Git -e --source winget` 안내) | G4 탈락(시스템 도구) + `Git.Git`은 머신 스코프 MSI라 **UAC 승격 창**이 뜬다 |

**결과: Windows도 run 3 / manual 3.** macOS(§2 매트릭스: Claude·gh·Wrangler run, pnpm·Node·Git manual)와 **도구 단위까지 정확히 일치한다.** 이게 "macOS와 동일한 완전 지원"의 구체적 정의다 — 같은 도구가 같은 이유로 자동이고, 같은 도구가 같은 이유로 수동이다.

### B.2 왜 gh만 winget인가 (핵심 트레이드오프)

**선택**: winget은 **gh 전용 러너**로 도입한다. claude/wrangler는 Windows에서도 npm/pnpm 러너를 쓴다.

**대안 A — 전 도구를 winget으로 통일**: 기각. ① Claude Code의 winget 경로는 공식 문서가 권장하는 설치 수단이 아니다(공식 Windows 권장은 `irm https://claude.ai/install.ps1 | iex` = **G2 정면 위반**, 차선이 npm). ② winget으로 설치하면 `classify_install_method`가 `WingetPackage`로 분류하고, 그 다음 업데이트는 `winget upgrade`가 되는데 **Claude Code는 자기 자신을 갱신하는 `claude update`가 정본**이라 두 갱신 경로가 경쟁한다(§3.1이 brew cask를 기각한 것과 같은 함정: "앱이 설치해 놓고 자기가 업데이트 못 하는/이중으로 하는 상태"). ③ wrangler는 winget 패키지가 존재하지 않는다.

**대안 B — winget도 쓰지 않고 gh도 manual 유지**: 기각. 그러면 Windows는 run 2 / manual 4가 되어 "macOS와 동일"이 깨진다. 그리고 gh는 **G1~G4를 전부 통과하는데도** 막는 셈이라, 게이트를 규칙으로 승격시킨 §1.3의 취지(“예외가 아니라 규칙의 결과여야 한다”)에 어긋난다.

**포기한 것**: winget이 없는 머신(Windows 10 1809 미만, 또는 App Installer 미등록 상태 — winget은 Microsoft Store가 비동기로 등록하므로 **최초 로그인 직후엔 없을 수 있다**)에서는 gh 자동 설치가 불가능하다.
**감당**: `ResolvedRunners`에 `winget` 슬롯을 추가하고, 해석 실패 시 **기존 `MANUAL_NO_RUNNER` 경로로 자동 강등**된다(신규 분기 없음). 문구만 Windows 전용으로 구체화한다 — "winget(앱 설치 관리자)을 찾을 수 없습니다. Microsoft Store에서 '앱 설치 관리자'를 업데이트하거나 https://cli.github.com 에서 직접 설치해주세요."

### B.3 winget 호출의 비정상 케이스 (③ 의무)

| 케이스 | 설계 |
|---|---|
| **최초 실행 시 소스 약관 동의 프롬프트** | `--accept-source-agreements`를 **install·upgrade·show 모든 argv에 리터럴로** 넣는다. 추가로 `--disable-interactivity`로 남은 프롬프트를 차단한다. 우리 자식은 `stdin(Stdio::null())`이라 프롬프트가 뜨면 **정지가 아니라 즉시 실패**한다(부록 B.1 원칙 유지) |
| **패키지 EULA 동의** | 설치에만 `--accept-package-agreements` 추가(업그레이드에는 불필요) |
| **이미 최신인데 `winget upgrade`를 부름** | winget은 `0x8A15002B`(= **exit code `-1978335189`**, `APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE`, "No applicable update found")를 반환한다. 이걸 `Failed`로 두면 화면이 거짓을 말한다 → **`Outcome::AlreadyLatest`로 매핑한다.** 계약 변경 없음(변형이 이미 있다). 그 외 `0x8A15xxxx` 대역은 `Failed` |
| **UAC 승격** | `Git.Git`류 머신 스코프 설치에서 발생. **`GitHub.cli`는 user 스코프 설치가 가능한 것으로 알려져 있으나 미검증** — Phase 2 실기 검증 항목 1번이다. 앱은 **절대 스스로 승격하지 않는다**(`runas` 금지, G 참조). 승격 창을 사용자가 거부하면 winget이 non-zero로 끝나고 `Failed` + `log_tail`로 드러난다 |
| **타임아웃 시 손자 프로세스** | `process.rs:160-171`의 `#[cfg(windows)] force_kill_process_group`은 **직속 자식만** 죽인다(주석이 "손자를 남기는 경우가 드물어 MVP에서 충분"이라고 자인). **winget은 이 가정을 깬다** — winget은 msiexec/설치 EXE를 별도 프로세스로 띄우므로 타임아웃 kill 후에도 설치가 계속될 수 있다. → 타임아웃을 600초로 두고(brew와 동일), `TimedOut` 메시지에 **"설치 프로그램이 백그라운드에서 계속 진행 중일 수 있습니다. 잠시 후 새로고침해주세요."**를 명시한다. Job Object 도입은 새 의존성(`windows` crate)이라 범위 밖 — **미해결 쟁점 목록에 올린다** |
| **프리뷰(무엇이 함께 바뀌나)** | **winget에는 `brew install -n`에 해당하는 dry-run이 없다.** → `winget show --id <ID> -e --source winget --accept-source-agreements --disable-interactivity`(타임아웃 120초)로 버전·설치기 정보만 보여주고, **`preview_reliable: false`** 로 내려보낸다. 계약에 이미 있는 필드이며(`contract.rs:35`), 프론트는 이 값이 false면 "전체 업데이트" 배치에서 제외한다. `notes`에 "winget은 사전 시뮬레이션을 제공하지 않아 함께 변경될 항목을 미리 확인할 수 없습니다"를 명시 — **거짓 안심을 주지 않는 것이 이 필드의 존재 이유다** |
| **환경변수** | `run_process_with_timeout`은 `env_clear()`를 하지 않고 PATH만 덮어쓴다(`process.rs:213-217`) → `SystemRoot`/`TEMP`/`APPDATA`/`PATHEXT`가 상속된다(winget의 네트워크·설치 동작에 필수). **이 성질에 의존한다는 사실을 주석으로 못 박는다.** winget 전용 env는 추가하지 않는다(`NO_COLOR`/`TERM=dumb`는 winget에 효과가 없고 무해) |
| **stdout 인코딩** | winget 출력은 UTF-8이지만 콘솔 코드페이지에 따라 깨질 수 있다. 현행 `String::from_utf8_lossy`가 그대로 감당한다(패닉 없음). **미검증** |

### B.4 신규 `InstallMethod::WingetPackage`

Windows 설치본의 업데이트 경로가 성립하려면 분류가 필요하다.

```
WingetPackage { id: String }   // id는 경로 파싱이 아니라 (ToolId → 고정 리터럴) 맵에서 온다
```
**중요**: 경로에서 winget 패키지 id를 파싱하는 것은 **불가능하고 시도하지도 않는다**(`...\WinGet\Links\gh.exe`에 `GitHub.cli`가 적혀 있지 않다). 대신 §1.1 리터럴 전용 불변식 그대로, `UPDATE_TABLE`에 `(tool=Gh, method=WingetPackage) → Run(RUN_WINGET_UPGRADE_GH)` 행을 두고 argv를 전부 `Arg::Lit`으로 박는다. → **동적 argv 토큰이 0이므로 인젝션 표면이 구조적으로 존재하지 않는다**(§1.1과 동일한 성질).

분류 규칙(경로 기반, 순수함수):
- `%LOCALAPPDATA%\Microsoft\WinGet\Links\` 또는 `%LOCALAPPDATA%\Microsoft\WinGet\Packages\` 아래 → `WingetPackage`
- `%APPDATA%\npm\` 아래 → `NpmGlobal`(기존 변형 재사용)
- `%LOCALAPPDATA%\pnpm\` 아래 → `PnpmStandalone` / `PnpmGlobalPackage`(기존)
- `%USERPROFILE%\.local\bin\claude.exe` → `ClaudeNative`(기존)
- `C:\Program Files\Git\`, `C:\Program Files\nodejs\` 등 → `SystemManaged`
- `%USERPROFILE%\.nvm`, `\fnm\`, `\volta\` 세그먼트 → `VersionManager`(G4 근거)
- 그 외 → `Unknown` → 기존 와일드카드가 `Manual(UnknownMethod)`로 보낸다(**fail-closed 유지**)

### B.5 Windows `path_candidates` (탐지 대상, 전부 **미검증**)

| 도구 | Windows 후보(우선순위 순) |
|---|---|
| claude | `%USERPROFILE%\.local\bin\claude.exe`, `%APPDATA%\npm\claude.cmd`, `%LOCALAPPDATA%\Microsoft\WinGet\Links\claude.exe` |
| node | `C:\Program Files\nodejs\node.exe`, `%LOCALAPPDATA%\Programs\nodejs\node.exe`, `%APPDATA%\nvm\...`(버전매니저 감지용) |
| gh | `C:\Program Files\GitHub CLI\gh.exe`, `%LOCALAPPDATA%\Microsoft\WinGet\Links\gh.exe` |
| git | `C:\Program Files\Git\cmd\git.exe`, `C:\Program Files (x86)\Git\cmd\git.exe`, `%LOCALAPPDATA%\Programs\Git\cmd\git.exe` |
| pnpm | `%LOCALAPPDATA%\pnpm\pnpm.exe`, `%APPDATA%\npm\pnpm.cmd` |
| wrangler | `%LOCALAPPDATA%\pnpm\wrangler.cmd`, `%APPDATA%\npm\wrangler.cmd` |
| (러너) npm | `C:\Program Files\nodejs\npm.cmd`, `%APPDATA%\npm\npm.cmd` |
| (러너) winget | `%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe` |

절대 후보가 전부 빗나가면 **PATH 스캔**(D.3)이 받는다. macOS 후보 배열은 **한 글자도 건드리지 않는다.**

---

## C. cfg 게이트 해제 순서와 롤백 안전성

### C.1 결정 — 3단계, 피처 플래그 없음

현재 게이트는 **5곳**이다(실측):
```
query.rs:51                        if !cfg!(target_os = "macos")   → 화면 데이터 전체를 manual로 고정
mod.rs:187  preview_dev_tool_update    동일
mod.rs:201  update_dev_tool            동일
mod.rs:215  install_dev_tool           동일
mod.rs:236  open_manual_instruction    동일
```

| Phase | 내용 | 해제하는 게이트 | 끝났을 때 Windows 사용자가 얻는 것 | 머지 조건 |
|---|---|---|---|---|
| **0** | CI `rust-test` 잡 신설 + POSIX 가정 테스트 5개 격리 + `rust-version = "1.81"` | **없음** | 없음(동작 변화 0) | CI 녹색 |
| **1** | `platform.rs` 신설, `path_candidates`/PATH/확장자/`classify`/`compute_path_visibility` Windows 분기, `open_terminal_command` Windows 분기 | `query.rs:51`, `mod.rs:187`(preview), `mod.rs:236`(open_manual) | **정확한 탐지** + 정확한 수동 안내(winget 명령 복사·PowerShell 창에서 실행). `UPDATE_TABLE`/`install_candidates`에 Windows 행이 **아직 없으므로** 전 도구가 `Manual` | CI 녹색 + 실기에서 **탐지 결과 6개가 실제와 일치** |
| **2** | winget 러너 + `WingetPackage` 분류 + Windows RunPlan 행 추가 | `mod.rs:201`(update), `mod.rs:215`(install) | **자동 설치·업데이트** | CI 녹색 + **실기 Windows PC에서 gh 설치→탐지→업데이트 왕복 성공** |

**Phase 1만으로 위임서가 지목한 결함("실제로는 cmd에서 정상 동작하는 도구들이 전부 '설치 안 됨'으로 거짓 표시")은 완전히 해소된다.** 즉 사용자 가치의 대부분이 Phase 1에 있고, 위험의 대부분은 Phase 2에 있다. 이 비대칭이 단계를 나누는 이유다.

### C.2 왜 피처 플래그를 두지 않는가

**대안 — 런타임 플래그(`WINDOWS_EXEC_ENABLED: bool` 상수 또는 설정 파일 키)**: 기각. 근거 셋:
1. **롤백 수단이 이미 있다.** Windows 배포물은 `tauri-portable-build.yml`이 만드는 **단일 포터블 exe**이고 자동 업데이트가 없다(워크플로 주석: `--no-bundle`, 원본 exe만 업로드). 사용자가 이전 exe로 파일을 되돌리는 것이 곧 완전한 롤백이다. 개발 측은 `git revert`.
2. **플래그를 끈 상태가 곧 "지금의 거짓 표시 상태"** 라서 되돌릴 값어치가 낮다. 되돌리고 싶은 대상은 Phase 2(실행)뿐인데, 그건 **테이블 행을 넣지 않는 것**으로 이미 표현된다.
3. **테스트 매트릭스가 2배가 된다**(macOS×2 × Windows×2). Phase 0에서 어렵게 확보한 CI 검증의 효용을 스스로 절반으로 깎는 셈이다. "확장성"이 아니라 부채다.

**대신 채택한 것 — 데이터 게이팅**: `install_candidates()`가 빈 슬라이스를 돌려주고 `UPDATE_TABLE`에 해당 행이 없으면, 최종 와일드카드가 `Manual(UnknownMethod)`로 보낸다. 이 fail-closed 성질은 `install_resolver.rs:110-124` 주석이 이미 설명하는 기존 성질이고, **새 개념을 하나도 추가하지 않는다.** Phase 1과 Phase 2의 차이는 "테이블에 행이 있느냐"뿐이며, Phase 2를 되돌리려면 그 행을 지우면 된다.

**트레이드오프**: 현장에서 Windows 실행이 오작동할 때 **재배포 없이 끌 수단이 없다.** 감당: ①대상이 사내 <50인이고 배포물이 파일 하나라 재배포가 곧 "파일 다시 받기"다 ②Phase 2 머지 조건에 실기 검증을 넣어 오작동 확률 자체를 낮춘다 ③오작동의 최악 결과가 "설치가 실패하고 로그가 남는다"이지 "기존 환경이 깨진다"가 아니다(G4 게이트가 그 종류의 도구를 애초에 배제했다).

### C.3 macOS 무변경 보장 — 무엇으로 증명하는가

주장이 아니라 **기계가 판정할 수 있는 형태**로만 둔다.

1. **기존 테스트를 한 줄도 고치지 않는다.** `Platform` 파라미터화는 **기존 공개 함수 시그니처를 유지**하고(`build_child_path_env(runner_path)`), 내부에서 `platform_now()`를 넘기는 얇은 래퍼로 만든다. `process.rs:444/456/462`의 세 테스트가 **무수정으로 통과**하는 것이 곧 무변경 증거다. 기존 테스트를 손대야 한다면 그건 이미 회귀다.
2. **골든 테스트를 추가한다.** 현행 `build_child_path_env_prioritizes_runner_bin_dir`는 `starts_with`/`contains`만 본다 — 중간에 디렉터리가 끼어들어도 통과한다. `compose_path_env(Mac, Some("/opt/homebrew/bin/brew"))`의 **전체 문자열 완전 일치** 테스트를 추가한다. `DEV_TOOLS`의 macOS `path_candidates` 6×N 배열도 같은 방식으로 고정한다.
3. **CI `macos-latest` 잡**(Phase 0) — 로컬에서만 돌던 283개 테스트를 자동 게이트로 승격.
4. **화면 회귀**: macOS에서 개발 환경 화면 스크린샷을 Phase 1 전후로 대조한다(Skill `common-screen-verification-and-capture`).
5. **`capabilities/default.json` 무변경**을 diff로 확인한다 — 이 작업은 새 Tauri 플러그인을 쓰지 않으므로 권한 표면이 넓어질 이유가 없다. 이 파일에 변경이 생겼다면 설계에서 벗어난 것이다.

---

## D. 탐지(PATH 해석) 설계 — **A가 막혔을 때의 주요 방어선**

### D.1 핵심 결정: `cfg`가 아니라 **인자**로 분기한다

이 코드베이스에는 이미 선례가 있다 — `classify_install_method(canonical, home, pnpm_home, brew_prefixes)`(`classify.rs:87`)는 홈·환경변수·brew prefix를 **인자로 받는 순수함수**여서, 이 머신에 없는 설치 형태(pnpm standalone 등)까지 합성 경로로 테스트한다(`classify.rs:353` 테스트 이름이 그 사실을 그대로 말한다: `..._but_are_unverified_on_this_machine`). **같은 기법을 플랫폼 축으로 확장하는 것이 이 설계의 전부다.**

```rust
// dev_tools/platform.rs (신규)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Platform { Mac, Win }

/// 이 크레이트에서 cfg!(target_os)를 읽는 **유일한 자리**.
pub(crate) fn platform_now() -> Platform { … }

/// 경로 템플릿 확장에 쓰는 뿌리들 — 전부 주입 가능(테스트가 가짜 값을 넣는다).
pub(crate) struct EnvRoots {
    pub home: Option<PathBuf>, pub appdata: Option<PathBuf>,
    pub local_appdata: Option<PathBuf>, pub program_files: Option<PathBuf>,
    pub program_files_x86: Option<PathBuf>, pub pnpm_home: Option<PathBuf>,
}
impl EnvRoots { pub fn from_env() -> Self { … } }   // ← 여기만 실제 환경을 읽는다
```

### D.2 순수함수 목록 — **macOS `cargo test`에서 Windows 분기까지 100% 실행된다**

| 함수 | 역할 | macOS에서 검증되는가 |
|---|---|---|
| `expand_path_tokens(plat, template, &roots) -> Option<String>` | `~/`, `%USERPROFILE%`, `%APPDATA%`, `%LOCALAPPDATA%`, `%ProgramFiles%`, `%ProgramFiles(x86)%` 확장. **셸 미경유, `std::env::var`만** | ✓ 가짜 roots 주입 |
| `exe_extensions(plat) -> &'static [&'static str]` | Mac `[""]` / Win `[".exe", ".cmd", ".bat"]` (**이 순서** — `.exe`가 `.cmd`보다 안전하므로 우선) | ✓ |
| `path_separator(plat) -> char` | `:` / `;` | ✓ |
| `default_path_dirs(plat, &roots) -> Vec<String>` | Mac: 현행 6개 리터럴 그대로. Win: `%SystemRoot%\system32`, `%SystemRoot%`, `%SystemRoot%\System32\Wbem`, `%APPDATA%\npm`, `%LOCALAPPDATA%\pnpm`, `%LOCALAPPDATA%\Microsoft\WindowsApps` | ✓ 골든 테스트 |
| `compose_path_env(plat, runner_path, &roots) -> String` | `build_child_path_env`의 정본. 빈 항목 배제 규칙 유지(POSIX에서 빈 항목=CWD — Windows도 동일 위험) | ✓ **기존 테스트 3개 무수정 통과 + 신규 골든** |
| `path_scan_candidates(plat, path_var, name) -> Vec<String>` | `which`/`where` 대체. PATH를 split → 각 디렉터리 × `exe_extensions` 조합을 **경로 문자열로만** 생성 | ✓ 가짜 PATH 문자열 주입 |
| `dir_in_path_var(plat, dir, path_var) -> bool` | `compute_path_visibility`의 Windows 정본. Win은 대소문자 무시 + 후행 `\` 정규화 | ✓ |
| `quote_token(plat, token) -> String` / `build_terminal_command_line(plat, program, args) -> String` | E 참조 | ✓ |

**잔여 `cfg(windows)` 표면(CI만이 검증 가능) — 목표 50줄 미만:**
1. `platform_now()` (3줄)
2. `path_exists()` — Windows는 `symlink_metadata().is_ok()`를 쓴다. `%LOCALAPPDATA%\Microsoft\WindowsApps\winget.exe`는 **앱 실행 별칭(APPEXECLINK reparse point)** 이라 `Path::is_file()`이 false를 줄 수 있다(**미검증 — Phase 1 실기 검증 항목**). `symlink_metadata`는 reparse point를 따라가지 않으므로 존재를 본다 (~8줄)
3. `SilentCommand::windowed()` — `CREATE_NEW_CONSOLE` (~6줄, E)
4. 기존 `is_writable_by_current_user`(`plan_table.rs:466`) — **이미 존재, 재사용, 무변경**
5. 기존 `force_kill_process_group`(`process.rs:167`) — **이미 존재, 무변경**(단 손자 제약은 B.3 참조)

### D.3 `resolve_binary`의 Windows 분기 — **bare-name 폴백을 폐지한다**

현행(`cli_launcher.rs:21-37`): 절대 후보 `is_file()` → 전부 실패하면 `Command::new(bare_name).arg("--version").silent().output()`로 **프로세스를 띄워** 존재를 확인한다.

Windows에서 이 폴백은 세 가지를 동시에 잘못한다:
1. **PATH 하이재킹** — Windows `CreateProcess`의 기본 검색 순서에 **현재 디렉터리가 포함**된다. 악성 `npm.exe`가 CWD에 있으면 그게 실행된다(G 참조).
2. **콘솔 창** — `.silent()`가 막아주지만, 애초에 안 띄우면 되는 프로세스다(`f983216`이 고친 문제의 근원 제거).
3. **타임아웃 없음** — 이 폴백에는 타임아웃이 없다(기존 지적 #12). 도구 6개 + 러너 4개면 최악의 경우 10회 무제한 대기.

**결정**:
```rust
fn resolve_binary_with(
    plat: Platform, candidates: &[&str], roots: &EnvRoots,
    bare_name: &str, path_var: &str,
    exists: &dyn Fn(&str) -> bool,          // ← 주입: 테스트는 가짜 파일시스템을 넣는다
) -> Option<String>
```
- **Mac**: 절대 후보 → 실패 시 **기존 bare-name `--version` 폴백 그대로**(변경 없음). GUI 앱은 PATH가 비어 있어 스캔이 무의미하고, 이 폴백이 의미 있는 건 터미널에서 띄운 개발 모드뿐이라 현행이 이미 옳다. **회귀를 만들지 않는 것이 여기서는 최선이다.**
- **Win**: 절대 후보 → 실패 시 **`path_scan_candidates`로 PATH×확장자 조합을 만들어 `exists`로 확인**. **프로세스를 하나도 띄우지 않는다.** 찾으면 절대경로를 돌려주므로 이후 실행이 CWD 검색을 타지 않는다.

기존 `resolve_binary`/`resolve_binary_expand_home`은 **시그니처를 유지**한 채 이 함수를 `platform_now()`·`EnvRoots::from_env()`·실제 `exists`로 호출하는 래퍼가 된다 → 6개 호출부(`dev_tools/mod.rs:159`, `github_integration`, `cloudflare_integration`, `plugins/mod.rs`, `install_resolver.rs`)가 **한 글자도 바뀌지 않는다.**

### D.4 `.cmd` 러너를 그대로 실행하기로 한 결정

npm/pnpm/wrangler의 Windows 전역 shim은 `.cmd` 배치 파일이다. Rust `std::process::Command`는 `.bat`/`.cmd` 프로그램을 **암묵적으로 `cmd.exe`를 경유해** 실행한다 → 원 설계의 "셸 미경유" 불변식이 문자 그대로는 깨진다.

**선택**: `.cmd`를 그대로 실행한다.
**대안 — `node.exe <prefix>\node_modules\npm\bin\npm-cli.js …`로 우회**: 기각. 러너 경로를 npm 내부 구조에서 **파생**해야 하는데, 그건 §1.1 리터럴 전용 불변식이 금지하는 종류의 동적 경로 조립이고, npm 버전에 따라 깨진다.
**안전성 근거**: ① 이 경로로 넘어가는 argv는 **전부 `Arg::Lit` 정적 리터럴**이라 사용자 입력이 0이다(§1.1) ② `validate_argv_token`이 여전히 적용된다 ③ Rust 1.77.2가 배치 파일 인자 이스케이프를 고쳤고(CVE-2024-24576 BatBadBut, CVSS 10.0), 1.81.0이 파일명 후행 공백·점 우회까지 막았다(CVE-2024-43402). **현재 툴체인 1.98.1은 둘 다 포함**한다.
**감당**: A.2의 `rust-version = "1.81"` 선언이 이 근거를 **컴파일러가 강제하는 사실**로 바꾼다.

### D.5 그 외 Windows 분기가 필요한 자리

| 자리 | 조치 |
|---|---|
| `classify_install_method`(`classify.rs:87`) | 세그먼트 분리를 `split(['/', '\\'])`로, Windows는 **대소문자 무시 비교**. `dunce`(이미 의존성)로 `\\?\` 확장 길이 접두 제거 — `config/user_config.rs:370` 주석이 이미 이 함정을 기록해 두었다. 기존 macOS 테스트 23건이 무수정 통과해야 한다 |
| `compute_path_visibility`(`diagnostics.rs:238`) | 현행은 `/etc/paths` + 셸 rc 파일을 읽는다(POSIX 전용). Windows는 **프로세스 PATH(`std::env::var("PATH")`)에 그 디렉터리가 있는지**로 판정한다 — Windows GUI 앱은 macOS의 launchd와 달리 로그인 시점 사용자 PATH를 상속하므로 이게 타당한 근사다. `path_hint`는 "새 터미널을 열거나 로그아웃 후 다시 로그인하면 PATH가 갱신됩니다", `path_hint_target`은 `None`. **레지스트리(`HKCU\Environment`)는 읽지 않는다** — `winreg` 크레이트 추가가 필요하고, 얻는 정확도가 힌트 문구 하나에 비해 과하다 |
| `is_git_stub_without_clt`(`install_resolver.rs:286`) | macOS 전용 개념(`/usr/bin/git` xcrun 스텁). Windows에서는 **항상 false**를 돌려주게 `plat` 인자를 받는다 |
| `install_prefix_writable`(`install_resolver.rs:231`) | `Runner::Winget`에는 prefix 개념이 없다 → **검사를 건너뛴다**(없는 경로에 대해 fail-closed로 오작동하는 것을 막는 기존 처리와 같은 정신). npm 러너는 `%APPDATA%\npm`을 검사 — 기존 `#[cfg(windows)] is_writable_by_current_user`(프로브 파일 생성/삭제)를 그대로 쓴다 |

---

## E. 터미널 열기 — Windows 분기

### E.1 결정

```rust
// cli_launcher.rs
#[cfg(windows)] → powershell.exe
    args: ["-NoLogo", "-NoExit", "-Command", <command_line>]
    creation_flags: CREATE_NEW_CONSOLE (0x0000_0010)   // ← process_util::SilentCommand의 형제 windowed()
```

**무엇을 띄우는가**: **Windows PowerShell(`powershell.exe`, 5.1)**.
- **대안 `wt.exe`(Windows Terminal)**: 기각 — Windows 10 구형에 없다. 없는 경우 폴백이 필요해 분기가 2배가 된다.
- **대안 `pwsh.exe`(PowerShell 7)**: 기각 — 별도 설치물이라 부재 가능.
- **대안 `cmd.exe /K`**: 기각 — 인용 규칙이 PowerShell보다 나쁘다(`start "" cmd /K "…"`의 첫 인용 인자가 **창 제목**으로 먹히는 고전적 함정). `powershell.exe`는 `-Command` 하나로 끝난다.
- `powershell.exe`는 Windows 10 1809 이상 전 버전에 내장 → **winget보다 가용성이 넓다**(winget이 없어도 안내 창은 뜬다).

**어떻게 창이 뜨는가**: `f983216`이 도입한 `CREATE_NO_WINDOW`의 **정반대**다. GUI(콘솔 없는) 부모가 콘솔 자식을 띄우면 Windows가 콘솔을 자동 생성하므로 `.silent()`를 **안 붙이기만 해도** 창이 뜨지만, 암묵적 동작에 기대지 않고 `CREATE_NEW_CONSOLE`을 **명시**한다.

```rust
// process_util.rs — 기존 trait에 메서드 하나 추가
pub trait SilentCommand {
    fn silent(&mut self) -> &mut Self;     // 기존
    fn windowed(&mut self) -> &mut Self;   // 신규: Windows에서 CREATE_NEW_CONSOLE, 그 외 no-op
}
```
**충돌 검사 결과**: `process_util.rs:41-45`와 `cli_launcher.rs:75-79`의 주석이 **"사용자가 직접 보고 조작하도록 여는 창은 이 trait을 쓰지 않는다"** 고 이미 명시해 두었다. `windowed()`는 그 주석이 비워 둔 자리를 채우는 것이지 뒤집는 것이 아니다. **충돌 없음.**

### E.2 인자 인용 규칙 (순수함수, macOS에서 테스트)

```rust
pub(crate) fn quote_token(plat: Platform, token: &str) -> String
pub(crate) fn build_terminal_command_line(plat: Platform, program: &str, args: &[&str]) -> String
```
- **Win**: PowerShell 작은따옴표. 내부 `'`는 `''`로 이스케이프. 프로그램은 **항상** 인용하고 호출 연산자 `&`를 붙인다 → `& 'C:\Program Files\GitHub CLI\gh.exe' 'auth' 'login'`. 작은따옴표 안에서는 PowerShell이 변수 확장·서브식을 하지 않으므로 `$`/`` ` ``가 무해해진다 — **이게 작은따옴표를 고른 이유다.**
- **Mac**: **토큰이 `[A-Za-z0-9._/:@=-]+`이면 인용하지 않는다.** → `/opt/homebrew/bin/gh` + `["auth","login"]` = `"/opt/homebrew/bin/gh auth login"` 으로 **현행과 바이트 단위 동일**. 공백/특수문자가 있을 때만 POSIX 작은따옴표(`'` → `'\''`).

### E.3 호출부 변경 (구조화)

현행 `github_integration.rs:95`는 `open_terminal_command(&format!("{gh} auth login"))`로 **문자열을 조립**한다 — Windows 경로에 공백이 있으면(`C:\Program Files\…`) 그대로 깨진다.

→ **구조화된 진입점을 추가**한다(기존 것은 남긴다):
```rust
pub fn open_terminal_program(program: &str, args: &[&str]) -> Result<(), String>   // 신규
pub fn open_terminal_command(shell_command: &str) -> Result<(), String>            // 기존 유지
```
- `github_connect/disconnect`, `cloudflare_connect/disconnect` 4곳 → `open_terminal_program`으로 전환. **macOS 출력 문자열은 E.2의 무인용 규칙 덕에 바이트 동일**(회귀 없음).
- `perform_open_manual_instruction`(`actions.rs:351`)이 넘기는 `copyable_command`는 **테이블의 플랫폼별 리터럴**(Windows면 `winget install --id Git.Git -e --source winget`)이라 문자열 그대로 `-Command`에 전달하면 된다 → `open_terminal_command` 유지.

### E.4 실패 경로

`powershell.exe` spawn 실패 시 `TerminalLaunchResult { opened: false, message: "터미널을 열지 못했습니다. 아래 명령을 복사해 PowerShell에 직접 붙여넣어 주세요: …" }`. **계약(`opened: bool` + `message: String`) 불변** — 프론트 변경 0.

---

## F. 파일 분리

### F.1 증분 추정 (구현 + 테스트 포함)

| 파일 | 현재 | 증분 | 예상 | 판정 |
|---|---|---|---|---|
| `dev_tools/platform.rs` | — | +330 | **330** | **신규** |
| `dev_tools/install_resolver.rs` | 754 | +180 (winget 러너·후보·테스트) | **~930** | ⚠️ **위험** |
| `dev_tools/plan_table.rs` | 622 | +160 (winget RunPlan 3종, Windows ManualPlan 6종, 테스트) | ~780 | OK |
| `dev_tools/classify.rs` | 560 | +200 (Windows 분류 + 테스트) | ~760 | OK |
| `dev_tools/diagnostics.rs` | 467 | +110 | ~580 | OK |
| `dev_tools/process.rs` | 466 | −40 (PATH 로직이 platform.rs로 이동) | ~430 | OK |
| `dev_tools/mod.rs` | 255 | +90 (Windows path_candidates) | ~345 | OK |
| `dev_tools/query.rs` | 465 | −25 (게이트·`windows_unsupported_…` 제거) | ~440 | OK |
| `cli_launcher.rs` | 87 | +90 | ~180 | OK |
| `process_util.rs` | 145 | +25 | ~170 | OK |

### F.2 결정 — 분리는 **둘**뿐

1. **`dev_tools/platform.rs` 신규**(~330줄). 경계: **플랫폼에 따라 달라지는 순수 결정 전부**(토큰 확장·확장자·구분자·기본 PATH·PATH 합성·PATH 스캔·PATH 가시성 판정·인용·터미널 명령행 조립) + 그 테스트. 이 경계는 임의로 그은 게 아니라 **D의 "순수함수로 떼어내야 macOS에서 검증되는 것"과 정확히 같은 선**이다 — 파일 하나가 곧 "CI 없이도 검증되는 영역"이 된다.
2. **`dev_tools/install_resolver.rs` → `install_resolver.rs` + `runners.rs` 분할.** 경계:
   - `runners.rs`(~350): `Runner` 해석 전부 — `BREW_CANDIDATES`/`NPM_CANDIDATES`/신규 `WINGET_CANDIDATES`, `resolve_runner_path`, `ResolvedRunners`(+`winget` 슬롯), `install_prefix_writable` + 테스트
   - `install_resolver.rs`(~580): 판정 로직 — `validate_argv_token`, `resolve_plan`, `install_candidates`, `resolve_install_plan`, `resolve_args`, `build_command_display`, `is_git_stub_without_clt` + 테스트
   - 이 선을 고른 이유: **Windows 증분이 거의 전부 `runners.rs` 쪽에 떨어진다**(winget 러너 해석·후보·쓰기권한). 판정 규칙(G1~G4의 코드 표현)은 플랫폼과 무관하게 그대로 남아 diff가 작아지고 리뷰가 쉬워진다.

**나머지는 분리하지 않는다.** `classify.rs` 760줄·`plan_table.rs` 780줄은 규율(1,000) 안이고, 지금 나누면 "Windows 때문에 나눴다"는 인위적 경계가 생겨 다음 사람이 어디를 봐야 할지 헷갈린다. **분리는 규율을 넘길 때 또는 책임이 실제로 갈릴 때만 한다.**

### F.3 트레이드오프

`platform.rs`를 두면 `Platform` 인자가 호출 사슬을 타고 번진다(`classify_install_method`, `resolve_binary_with`, `compute_path_visibility` …). 인자 하나가 늘어나는 비용을 **"Windows 분기를 macOS에서 실행 검증할 수 있음"**과 맞바꾼 것이다. A가 막힌 상황에서는 이 교환이 압도적으로 유리하다. 번짐을 억제하기 위해 **공개 시그니처는 유지**하고 파라미터화된 함수는 `pub(crate)`로만 노출한다(D.3).

---

## G. 보안 표면 — 쟁점 목록 (별도 security 리뷰 대상)

| # | 쟁점 | 완화책 | 비고 |
|---|---|---|---|
| **G-1** | **PATH 하이재킹(CWD 검색)** — Windows `CreateProcess`의 기본 검색 경로에 현재 디렉터리가 포함된다. bare-name 실행은 CWD의 악성 `npm.exe`를 집을 수 있다 | **Windows bare-name 폴백 폐지**(D.3) — PATH 스캔으로 절대경로를 확정한 뒤에만 실행. 이 설계에서 가장 값어치 있는 보안 결정이다 | 구조적 제거 |
| **G-2** | **`.cmd` 러너의 인자 인젝션(BatBadBut)** — `.cmd` 실행은 `cmd.exe`를 경유하며 인용 규칙이 복잡하다(CVE-2024-24576 CVSS 10.0, 후속 CVE-2024-43402) | ①argv 전부 `Arg::Lit` 정적 리터럴(§1.1) ②`validate_argv_token` 유지 ③**`rust-version = "1.81"` 하한 선언**으로 std 완화를 컴파일러가 강제 | D.4 |
| **G-3** | **PATH 환경변수 주입** — `compose_path_env`가 만드는 자식 PATH에 쓰기 가능한 디렉터리가 앞에 오면 위 G-1이 재발한다 | 러너 bin 디렉터리를 최우선으로 두는 현행 규칙 유지 + **빈 항목 배제**(Windows에서도 빈 PATH 항목은 CWD를 의미) + 기본 디렉터리는 `%SystemRoot%` 계열 리터럴만 | 기존 테스트가 이미 빈 항목을 잡는다 |
| **G-4** | **UAC 권한 상승** | **앱은 절대 스스로 승격하지 않는다**(`runas` verb·`ShellExecute` 금지를 코드 주석으로 명문화). winget이 승격을 요구하면 OS가 사용자에게 묻고, 거부하면 non-zero로 실패한다. 프리뷰 `notes`에 "관리자 권한 승인 창이 뜰 수 있습니다"를 미리 표시 | Git/Node는 애초에 manual(B.1) |
| **G-5** | **winget 소스·패키지 오지정** | `--source winget` 고정(**`msstore` 배제**), `-e`(exact) + `--id` 고정 리터럴로 이름 모호성 제거. **`--ignore-security-hash` 사용 금지**를 명문화 | 오설치 = 임의 코드 실행 |
| **G-6** | **타임아웃 후 잔존 설치 프로세스** — Windows kill은 직속 자식만 죽인다(`process.rs:160-171`). winget은 msiexec을 손자로 띄운다 | 600초 타임아웃 + `TimedOut` 메시지에 "백그라운드에서 계속 진행 중일 수 있습니다" 명시. Job Object 도입은 **미해결 쟁점**으로 남김 | 기존 주석의 "손자는 드물다" 가정이 winget에서는 성립하지 않는다 |
| **G-7** | **터미널 창 명령 주입** — `-Command`에 넘어가는 문자열이 곧 사용자 눈앞에서 실행된다 | PowerShell **작은따옴표** 인용(변수·서브식 확장 차단) + 순수함수 `quote_token`에 대한 악성 입력 테스트. 넘어가는 값은 ①테이블 리터럴 ②`resolve_binary`가 파일시스템에서 확인한 절대경로 둘뿐 | E.2 |
| **G-8** | **로그 유출** — winget stdout에 사용자명 포함 경로가 찍혀 `log_tail`(4,000자)로 프론트에 간다 | 기존 macOS와 동일 수준(brew/npm도 홈 경로를 찍는다). **표면 확대 아님** — 신규 완화 불필요 | 기록만 |

**권한 표면 불변 확인**: 이 설계는 Tauri 플러그인을 추가하지 않는다 → **`capabilities/default.json` 변경 0**. 새로 생기는 실행 능력은 전부 **이미 존재하는 커스텀 Rust 커맨드 4개 안**에서 일어난다. CLAUDE.md의 "권한 표면을 의도적으로 좁게 유지한다" 원칙과 충돌하지 않는다.

---

## 8. 미검증 목록 (실기 Windows PC 필요) — **정직 규율**

이 문서의 Windows 관련 주장 중 **실행으로 확인된 것은 하나도 없다.** 이 머신은 macOS다. 아래는 Phase 1/2 머지 전에 실기에서 반드시 확인할 항목이다.

| # | 미검증 항목 | 확인 방법 | 어느 Phase |
|---|---|---|---|
| 1 | `Path::is_file()`이 `WindowsApps\winget.exe`(앱 실행 별칭)에 대해 무엇을 반환하는가 / `symlink_metadata()`가 대안이 되는가 | 실기에서 두 API를 나란히 호출 | 1 |
| 2 | B.5의 `path_candidates` 8줄이 실제 설치 위치와 맞는가 | 실기에서 `where <도구>` 결과와 대조 | 1 |
| 3 | `.cmd` shim(`npm.cmd`)을 `Command::new(절대경로)`로 실행했을 때 stdout이 정상 캡처되는가 | 실기에서 `npm --version` 왕복 | 1 |
| 4 | `command.env("PATH", …)`가 Windows의 `Path`(대소문자 다름)를 실제로 덮어쓰는가 | 실기에서 자식 PATH 출력 | 1 |
| 5 | `CREATE_NEW_CONSOLE`로 뜬 PowerShell 창에서 `gh auth login` 브라우저 인증이 완주되는가 | 실기 수동 | 1 |
| 6 | `GitHub.cli`가 user 스코프로 설치되는가, UAC가 뜨는가 | 실기에서 `winget install --id GitHub.cli -e --scope user` | **2** |
| 7 | `winget upgrade`가 최신일 때 `-1978335189`를 실제로 반환하는가 | 실기 | **2** |
| 8 | 설치 후 새 바이너리가 **재조회(`resolve_tool_path`)로 즉시 잡히는가** (PATH 갱신이 프로세스에 반영되지 않아 못 찾을 수 있다 — 결정 4의 "exit code는 주장, 재조회가 확인"이 Windows에서 `UnknownAfter`를 남발할 위험) | 실기에서 설치→즉시 재조회 | **2** |
| 9 | winget stdout의 콘솔 코드페이지/인코딩 | 실기 | 2 |

**항목 8이 가장 위험하다** — 정상 설치인데 `verified: false`로 표시되면 사용자가 "실패했나?"라고 읽는다. 완화안(설계 예비): 재조회 시 **`path_scan_candidates`가 쓰는 PATH를 프로세스 PATH가 아니라 갓 확장한 `EnvRoots` 기반 절대 후보 우선으로** 보게 하여 PATH 갱신 지연의 영향을 받지 않게 한다. Phase 2 실기 결과에 따라 확정한다.

## 9. 미해결 쟁점 (이 문서가 답하지 않은 것)

1. **Job Object로 손자 프로세스까지 종료할 것인가**(G-6). `windows` 크레이트 신규 의존이 필요하다. 현 판단은 "범위 밖, 메시지로 감당". Phase 2 실기에서 타임아웃이 실제로 문제를 일으키면 재검토.
2. **Windows에서 `EXECUTION_LOCK` 직렬화가 충분한가** — winget이 내부적으로 자체 뮤텍스를 쓰므로 동시 실행 시 우리 쪽 락이 아니라 winget이 거부할 수 있다. 실패 메시지 매핑 미정.
3. **`open_terminal_command`의 macOS AppleScript 이스케이프**(`cli_launcher.rs:78`)는 `\`와 `"`만 처리한다. E.3으로 4개 호출부가 구조화 경로로 옮겨가면 남는 소비처는 테이블 리터럴뿐이라 현 상태로 충분하지만, **이 정리를 Phase 1에 포함할지 별건으로 뺄지** PM 판단 필요.
4. **Phase 1 이후 `MACOS_ONLY_MESSAGE` 상수의 처분** — 게이트 3곳이 사라지면 Phase 1에서는 `update`/`install` 2곳만 쓴다. Phase 2에서 0곳이 되므로 그때 삭제한다(Phase 1에서 미리 지우지 않는다).
5. **실기 검증을 누가/어느 PC에서 하는가** — Sensitive 등급의 Phase 2 머지 조건이므로 담당자·일정이 정해지지 않으면 Phase 2를 착수하지 않는다.

---

## 10. 자기 검증 (설계 4대 의무)

- **① 트레이드오프**: A(5개 선택지 비교표 + 포기한 것/감당), B.2(대안 A·B 기각 근거), C.2(플래그 기각 3근거), D.3/D.4(대안 기각), E.1(터미널 3대안 기각), F.3 — **주요 선택마다 대안 ≥1개와 포기한 것을 명시했다.**
- **② 프로젝트 고유성**: 이 저장소에 `docs/prd.md`는 없다. 비교 기준은 선행 설계와 같은 축 — **현행 방식(사용자가 스스로 설치 명령을 찾아 실행)** 이며, 이 설계가 다르게 하는 지점은 §B(같은 G1~G4 게이트를 Windows에 적용해 run/manual이 macOS와 도구 단위로 일치)와 §D(플랫폼을 `cfg`가 아니라 인자로 — 이 코드베이스가 `classify_install_method`에서 이미 쓰고 있는 기법의 확장)다. 표준 CRUD·3계층은 이 작업에 없고, 분량은 **검증 불가라는 이 프로젝트 고유 제약(A·8절)** 에 집중시켰다.
- **③ 비정상 케이스**: B.3(약관 프롬프트·EULA·이미 최신·UAC·타임아웃 손자·프리뷰 부재·env 상속·인코딩 8종), G-6, 8절 항목 8(설치 후 재조회 실패). 멱등성: `perform_install`의 "이미 설치됨 → update 위임"(§6.3 요구 2)을 그대로 상속한다.
- **④ 완결성**: 모든 실행 argv를 리터럴로 확정했고(B.1), 신규 `InstallMethod` 변형의 분류 규칙·업데이트 행·fail-closed 경로를 지정했으며(B.4), 프론트 계약 변경이 0임을 필드 단위로 확인했다(`preview_reliable`·`AlreadyLatest`·`TerminalLaunchResult` 재사용). 파일 크기는 F.1에서 줄 단위로 추정했다.
- **보안 구조적 결정 대조**(Skill `domain-backend-api-security` "언제 정하는가"): 이 작업은 네트워크 API가 아니라 **로컬 프로세스 실행**이다. 해당 절의 구조적 항목 중 적용되는 것은 ①신뢰 경계(프론트→IPC 커맨드: `toolId` + `planId` 해시만 넘어오는 기존 계약 유지) ②입력 검증 계층(`validate_argv_token` + 리터럴 전용 불변식) ③권한 경계(`capabilities/default.json` 무변경, 새 플러그인 없음) — 셋 다 **값으로 확정했다.** 인증/테넌시/CORS/레이트리밋은 이 도메인에 해당 사항 없음. 로직 강도(에러 메시지 상세도, 로그 마스킹 수준)는 구현 단계로 넘긴다.

---

### 출처 (외부 사실 근거)

- [winget `install` 명령 옵션(Microsoft Learn)](https://learn.microsoft.com/en-us/windows/package-manager/winget/install) — `--accept-source-agreements`, `--accept-package-agreements`, `--disable-interactivity`, `-e/--exact`, `--id`, `--source`, `--scope`, `-h/--silent`
- [WinGet 사용 요건(Microsoft Learn)](https://learn.microsoft.com/en-us/windows/package-manager/winget/) — Windows 10 1809(빌드 17763) 이상, App Installer 경유, 최초 로그인 후 Store가 비동기 등록
- [winget 반환 코드(microsoft/winget-cli)](https://github.com/microsoft/winget-cli/blob/master/doc/windows/package-manager/winget/returnCodes.md) — `0x8A15002B` = `-1978335189` = `APPINSTALLER_CLI_ERROR_UPDATE_NOT_APPLICABLE`
- [winget-cli #4751](https://github.com/microsoft/winget-cli/issues/4751), [pnpm #9933](https://github.com/pnpm/pnpm/issues/9933), [winget-cli #3616](https://github.com/microsoft/winget-cli/issues/3616) — `pnpm.pnpm` 업그레이드·PATH 이슈(pnpm을 manual로 두는 근거)
- [CVE-2024-24576 (BatBadBut)](https://blog.rust-lang.org/2024/09/04/cve-2024-43402/) 및 후속 [CVE-2024-43402](https://blog.rust-lang.org/2024/09/04/cve-2024-43402/) — Rust std의 Windows 배치 파일 인자 이스케이프, 1.77.2 / 1.81.0에서 수정
- [Claude Code Windows 설치(공식 권장은 PowerShell 네이티브 설치기, 차선이 npm)](https://github.com/anthropics/claude-code/issues/14902) — `irm | iex`가 G2 탈락인 근거
