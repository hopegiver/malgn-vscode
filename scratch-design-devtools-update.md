# 개발 환경 실설치/업데이트 — 백엔드 설계 (리뷰용 임시본)

작성: architect / 대상: `src-tauri` 백엔드 / 등급: **Sensitive**(사용자 머신 상태를 비가역적으로 바꾸는 명령을 앱이 직접 실행)
범위: 설계 결정만. 구현 코드·파일 수정 없음.

---

## 0. 전제와 이 설계의 고유성

**비교 기준(현행 방식)**: 지금 사내 개발자는 터미널에서 `brew upgrade …` / `npm i -g …`를 **스스로 기억해서** 실행한다. 이때 실제로 나는 사고는 두 가지다 — ① 설치 경로를 모른 채 잘못된 명령을 실행(`brew upgrade node` ↔ 실제 formula는 `node@22`), ② 실행 후 "정말 올라갔는지" 확인하지 않음. 이 설계가 현행과 다르게 하는 지점은 정확히 그 둘이다: **(1) 실제 바이너리 경로를 canonicalize해 설치방식을 판별한 뒤 그 방식에 맞는 명령만 고르고, (2) 성공을 exit code가 아니라 실행 전후 버전 재조회(claimed≠verified)로 판정한다.** 나머지(버전 조회, 목록 렌더)는 표준이므로 얕게 다룬다.

> 참고: 이 저장소에는 `docs/prd.md`가 없다(`docs/`가 gitignore, CLAUDE.md 기재). 그래서 경쟁사 비교표 대신 위 "현행 방식" 축을 인용했다. 이 위임은 화면 1개의 백엔드 설계로 범위가 명시돼 있어 PRD 보강 에스컬레이션은 하지 않는다.

**현행 코드 선례(따를 것)**
- `src-tauri/src/lib.rs` L607-641 `CommandResult` + `run_claude_command()` — 셸 없이 argv 배열, stdout/stderr/exit code 그대로 반환.
- `src-tauri/src/cli_launcher.rs` L19 `resolve_binary(absolute_candidates, bare_name)` — GUI PATH 문제 기존 해법.
- `capabilities/default.json` — fs/shell 플러그인 없음. **유지**(이 설계는 새 permission을 하나도 추가하지 않는다).

---

## 결정 1. 설치방식 판별(install method detection)

### 결정
`resolve_binary()`로 얻은 실행 경로를 **`std::fs::canonicalize()`로 심볼릭 링크를 끝까지 해석한 뒤**, 그 절대경로의 **경로 컴포넌트 매칭**으로 6분류한다. Homebrew formula명은 **하드코딩 맵이 아니라 Cellar 경로에서 파싱**한다.

### 분류 규칙 (위에서부터 먼저 매치되는 것 채택)

| # | 분류 `InstallMethod` | 판별 규칙(canonical path 기준) | 추출 데이터 |
|---|---|---|---|
| 1 | `HomebrewFormula` | 컴포넌트에 `Cellar`가 있고, `Cellar`의 **부모**가 알려진 brew prefix(`/opt/homebrew`, `/usr/local`, `$HOMEBREW_PREFIX`) | `formula` = `Cellar` 다음 컴포넌트, `keg_version` = 그 다음 |
| 2 | `HomebrewCask` | 컴포넌트에 `Caskroom` | `cask` = 다음 컴포넌트 |
| 3 | `NpmGlobal` | 컴포넌트에 `node_modules`가 있고 그 앞이 `lib`(= `<prefix>/lib/node_modules/...`) 또는 `npm root -g` 출력의 prefix로 시작 | `package` = `node_modules` 다음 컴포넌트(`@scope/name`이면 2컴포넌트) |
| 4 | `ClaudeNative` | `~/.local/share/claude/versions/`, `~/.local/bin/claude`, `~/.claude/local/` 중 하나로 시작 | — |
| 5 | `SystemManaged` | `/usr/bin/`, `/bin/`, `/sbin/`, `/usr/sbin/`, `/Library/Developer/CommandLineTools/`, `/Applications/Xcode.app/` 로 시작 | — |
| 6 | `VersionManager(name)` | 경로에 `/.nvm/`, `/.fnm/`, `/.volta/`, `/.asdf/`, `/mise/`, `/n/versions/` 포함 | 매니저명 |
| 7 | `Unknown` | 위 어디에도 안 걸림 | 원본 경로 |

**실측 대조**(이 머신, macOS arm64): claude→4, pnpm→1(`pnpm`), gh→1(`gh`), node→1(**`node@22`**), git→5, wrangler→미설치. 규칙이 6종 전부를 예상대로 분류한다.

### formula명: 파싱 > 하드코딩 맵 (근거)
`node`의 실제 Cellar 경로는 `/opt/homebrew/Cellar/node@22/22.23.1/bin/node`다. 바이너리명→formula명 하드코딩 맵은 `node`라고 답하고, `brew upgrade node`는 **다른 formula(최신 major)를 새로 설치**하거나 "not installed" 오류를 낸다. 즉 하드코딩 맵은 **조용히 잘못된 파괴적 명령**을 만든다. 경로 파싱은 "파일이 실제로 그 keg 안에 있다"는 사실 자체를 근거로 삼으므로 머신 상태와 절대 어긋나지 않는다. `python@3.13`, `openjdk@21` 등 버전 접미 formula 전반에 같은 문제가 있어 일반해가 필요하다.

- 기각한 대안 A: **바이너리명→formula 하드코딩 맵**. 기각 이유는 위. 포기한 것: 파일시스템 접근 없는 순수 상수 테이블(단순함). 감당: 파싱 코드는 컴포넌트 인덱싱 10줄이라 비용이 작다.
- 기각한 대안 B: **`brew list --formula`/`brew --prefix <f>`로 조회**. 정확도는 비슷하나 도구마다 brew 프로세스를 1회 더 띄워 화면 로딩이 6×수백ms 느려지고, brew 미설치 머신에서 무의미하다. 채택 안 함.
- 기각한 대안 C: **`command -v` 문자열만 보고 판별**. `/opt/homebrew/bin/node`는 심볼릭 링크라 formula를 알 수 없다(실측). canonicalize가 필수.

### 비정상 케이스
- canonicalize 실패(깨진 심볼릭 링크, 권한) → `Unknown`으로 강등(에러 아님).
- 파싱된 formula/package 문자열은 **argv에 들어가기 전 검증**: `^[A-Za-z0-9@][A-Za-z0-9@._+/-]*$`, `..` 불포함, 공백 불포함, `-`로 시작 금지. → `Cellar/--force/…` 같은 악의적/사고성 디렉터리명이 argv 플래그로 유입되는 경로 차단(**경로 컴포넌트를 통한 argv 인젝션**은 이 설계에서 유일하게 남는 문자열 유입 경로이므로 여기서 반드시 막는다).
- Homebrew prefix가 `/usr/local`인데 소유자가 root면 `brew upgrade`가 sudo를 요구한다 → 결정 3의 **사전 쓰기권한 검사**로 걸러 `Manual`로 강등.

---

## 결정 2. (도구 × 설치방식) → argv 테이블

### 결정
**정적 데이터 테이블 + 타입 있는 인자 슬롯**. `match` 분기로 명령을 조립하지 않는다. 명령 문자열이 바뀌면 **테이블 한 행의 리터럴만** 바뀌고 코드 구조는 그대로다.

```rust
// ── 도구 화이트리스트(프론트에서 오는 값은 이 id 하나뿐) ──
#[derive(Copy, Clone, PartialEq)]
enum ToolId { Claude, Node, Gh, Git, Pnpm, Wrangler }   // docker 완전 삭제
struct DevTool {
    id: ToolId,
    key: &'static str,              // "claude" — 프론트/JSON id
    label: &'static str,            // "Claude Code"
    version_args: &'static [&'static str],       // ["--version"]
    path_candidates: &'static [&'static str],    // "~" 확장 지원
}
static DEV_TOOLS: [DevTool; 6] = [ /* … */ ];

// ── argv 슬롯: 리터럴 + 판별결과에서만 채워지는 자리 ──
enum Arg {
    Lit(&'static str),
    Formula,          // 결정1이 Cellar 경로에서 파싱한 값
    Package,          // node_modules 경로에서 파싱한 값
    PackageLatest,    // "<package>@latest"
}

enum Runner { Brew, Npm, SelfBinary }   // 실행기도 절대경로 해석 대상(결정 5)

struct RunPlan {
    runner: Runner,
    args: &'static [Arg],
    preview_args: Option<&'static [Arg]>,  // brew --dry-run 등 (결정 부록 A)
    env: &'static [(&'static str, &'static str)],
    timeout_secs: u64,
}

enum ManualReason { XcodeClt, VersionManaged, NotWritable, UnknownMethod, NoRunner }
struct ManualPlan {
    reason: ManualReason,
    message_ko: &'static str,        // 화면에 그대로 보여줄 안내문
    copyable_command: Option<&'static str>, // 사용자가 터미널에서 직접 실행할 명령(표시/복사 전용)
    doc_url: Option<&'static str>,
}

enum Action { Run(RunPlan), Manual(ManualPlan) }

// ── 조회 테이블 ──
struct Row { tool: Option<ToolId>, method: MethodKind, action: Action }  // tool:None = 모든 도구에 적용
static UPDATE_TABLE: &[Row] = &[ /* 구체 행 → 와일드카드 행 순 */ ];
static INSTALL_TABLE: &[(ToolId, Action)] = &[ /* 미설치 도구용 */ ];
```

조회는 `UPDATE_TABLE`을 위에서 순회하며 `(tool 일치 or None) && method 일치` 첫 행 채택, 없으면 `Action::Manual(UnknownMethod)`. **셀이 비어 있어도(=행이 없어도) 시스템이 성립한다** — 없는 조합은 자동으로 안전한 `Manual`로 떨어진다.

### 잠정 셀 값 (researcher 결과로 교체될 자리 — 지금은 전부 "잠정")

| 도구 | 설치방식 | Action (잠정) |
|---|---|---|
| (any) | HomebrewFormula | `Run{ Brew, [Lit("upgrade"), Lit("--formula"), Formula], preview: [Lit("upgrade"), Lit("--dry-run"), Lit("--formula"), Formula] }` |
| (any) | NpmGlobal | `Run{ Npm, [Lit("install"), Lit("-g"), PackageLatest] }` |
| Claude | ClaudeNative | `Run{ SelfBinary, [Lit("update")] }` |
| Git | SystemManaged | `Manual{ XcodeClt, "Git은 macOS 명령어 도구(Xcode CLT)가 제공합니다. 앱이 수정할 수 없습니다.", copyable: "softwareupdate --list" }` |
| Node | VersionManager | `Manual{ VersionManaged, "node가 버전 매니저(nvm 등)로 관리되고 있습니다. 매니저에서 직접 올려주세요." }` |
| (any) | HomebrewCask | `Manual{ NotWritable, "캐스크 앱은 관리자 권한이 필요할 수 있어 앱이 실행하지 않습니다.", copyable: "brew upgrade --cask <name>" }` |
| (any) | Unknown / 그 외 | `Manual{ UnknownMethod, … }` (와일드카드 최종행) |

### "실행 불가"를 1급 개념으로 두는 게 맞는가 — 그렇다
6종 중 **git은 구조적으로 영구히 실행 불가**(`/usr/bin`, root:wheel)이고, node는 사내 다른 머신에서 언제든 버전매니저 관리로 바뀔 수 있다. 즉 실행 불가는 예외·에러가 아니라 **정상적이고 항상 존재하는 상태**다. 이를 `Result::Err`나 `success:false`로 표현하면 UI가 "실패(빨강, 재시도 버튼)"로 그리게 되어 사용자가 무한 재시도한다. `Action::Manual`을 별도 variant로 두면 UI는 **버튼 자체를 "안내 보기"로 바꿔** 애초에 못 누르게 만들 수 있다 — 잘못된 상태를 표현 불가능하게 만드는(make illegal states unrepresentable) 쪽이다.
- 기각한 대안: `Option<RunPlan>` + `None`이면 일반 에러 문구. 기각 이유: 사유별 안내문(Xcode CLT vs 버전매니저 vs 권한없음)이 전부 달라야 하는데 `None`은 사유를 못 담는다. 포기한 것: 타입 1개 줄이기.

---

## 결정 3. 판별 실패 시 기본 전략 → **(b) 실행 거부 + 안내**, 단 "막다른 골목"이 아닌 형태로

### 결정
`Unknown`/권한없음/실행기 미해석이면 **어떤 명령도 실행하지 않는다**. 대신 ① 감지된 실제 경로를 보여주고 ② 사용자가 터미널에서 직접 실행할 명령을 복사 가능하게 제시하고 ③ (선택) 기존 `cli_launcher::open_terminal_command()`로 **터미널 창을 열어준다**(사용자가 보고 조작하는 진짜 TTY).

### 근거 (반박 검토 포함)
- 이 기능의 blast radius는 "사용자 머신의 전역 개발도구"다. 추정이 틀렸을 때의 결과가 *실패*가 아니라 **다른 것이 설치/교체됨**이다(`brew upgrade node` → node@25 설치, PATH 우선순위 뒤집힘). 되돌리기 어렵고, 사용자는 자기가 뭘 눌렀는지도 모른다.
- 비대화형 자식 프로세스에는 TTY가 없다. 추정 명령이 sudo/확인 프롬프트를 만나면 **조용히 멈춘다**(결정 6의 stdin=null로 즉시 실패로 바꾸지만, 그건 "실패"일 뿐 성공이 아니다).
- (a)를 지지하는 유일한 논거는 "사용자가 그냥 되길 원한다"인데, **터미널 창을 열어주는 절충안이 그 욕구를 대부분 충족**시키면서 실행 주체를 사람으로 되돌린다. 게다가 이 앱에는 이미 그 선례(`github_connect`가 `gh auth login`을 터미널로 위임)가 있어 UX 일관성도 얻는다. → 사용자의 기본 입장 (b)를 유지하되, **(b)를 "안내 문구만 띄우는 dead-end"가 아니라 "터미널 위임"으로 강화**하는 것이 이 코드베이스에 맞는 답이다.
- 포기한 것: "모두 업데이트" 한 방에 전부 처리되는 경험. 감당: 안내 카드에 정확한 경로/명령을 넣어 복사 1회로 끝나게 한다.

---

## 결정 4. 성공 판정 프로토콜 (claimed ≠ verified)

### 프로토콜
1. `resolve_binary()`로 경로 해석 → `version_before` 캡처(정규화 포함).
2. 설치방식 판별 → 테이블 조회 → `Action` 결정. `Manual`이면 여기서 종료(`NotSupported`).
3. (brew 등 preview 있는 행) dry-run 실행 → 사용자 확인 → `plan_id` 대조(부록 A).
4. 실제 실행. exit code / stdout / stderr / 소요시간 수집.
5. **경로를 캐시하지 않고 처음부터 다시 해석**해 `version_after` 재조회. (brew는 relink로 심볼릭 링크 대상이 바뀌고, claude 네이티브는 `versions/<new>`로 갈아탄다 — 기존 canonical 경로는 **사라져 있을 수 있다**.)
6. 3상태 판정.

| 조건 | `outcome` | `verified` |
|---|---|---|
| exit==0 && before≠after (정규화 비교) | `Updated` | true |
| exit==0 && before==after | `AlreadyLatest` | true |
| exit==0 && after 조회 실패 | `UnknownAfter` | **false** ("명령은 성공했다고 보고했으나 확인하지 못했습니다") |
| exit!=0 | `Failed`(version 변화가 있으면 메시지에 병기) | false |
| 타임아웃으로 강제 종료 | `TimedOut` | false ("상태 불명 — 다시 확인 필요") |
| `Action::Manual` | `NotSupported` | — |

exit code는 **주장(claimed)**, 버전 델타가 **확인(verified)**이다. `AlreadyLatest`도 verified인 이유: "안 바뀐 것"을 실제로 관측했기 때문. 반면 `UnknownAfter`는 성공으로 칠 수 없다.

### 반환 타입
```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DevToolActionResult {
    id: String,                     // "pnpm"
    outcome: Outcome,               // serde: "updated"|"alreadyLatest"|"unknownAfter"|"failed"|"timedOut"|"notSupported"
    verified: bool,                 // 버전 재조회로 확인했는가
    version_before: Option<String>, // 표시용 원문 "gh version 2.95.0 (2026-06-17)"
    version_after: Option<String>,
    normalized_before: Option<String>, // 비교에 실제 쓴 값 "2.95.0"
    normalized_after: Option<String>,
    install_method: String,         // "homebrewFormula(node@22)" 등 디버그·표시용
    ran_command: Option<String>,    // 표시 전용 argv join — 다시 파싱하거나 실행하지 않는다
    exit_code: Option<i32>,
    duration_ms: u64,
    message: String,                // 한국어 요약(사용자 노출)
    log_tail: String,               // stdout+stderr 마지막 N줄(기본 4000자 절단)
}
```
기존 `CommandResult{success,message}`를 그대로 쓰지 않는 이유: `success: bool`은 3상태를 2상태로 뭉개서 "이미 최신"과 "업데이트됨"을 구분 못 하고, 무엇보다 **"확인 못 함"을 표현할 자리가 없다**(제약 4 위반). `CommandResult`는 기존 플러그인 커맨드용으로 그대로 두고 이 구조체를 추가한다.

### 버전 문자열 비교 시 주의점 (실측 기반)
실제 출력: `claude` → `2.1.252 (Claude Code)`, `node` → `v22.23.1`, `gh` → `gh version 2.95.0 (2026-06-17)` + **둘째 줄에 릴리스 URL**, `git` → `git version 2.50.1 (Apple Git-155)`, `pnpm` → `11.9.0`.
- 정규화 규칙: **첫 줄만** 취한다 → 정규식 `\d+(\.\d+)+([-+.][0-9A-Za-z.]+)?`의 **첫 매치**를 뽑는다 → 그것만 비교.
  - `gh`의 `(2026-06-17)`는 첫 매치가 아니므로 무시된다(날짜 형식 `2026-06-17`도 `\d+(\.\d+)+`에 안 걸림 — 구분자가 `-`).
  - `git`의 `Apple Git-155`도 첫 매치(`2.50.1`) 뒤라 무시.
  - `node`의 `v` 접두는 숫자 매치라 자연히 제거.
- 매치가 없으면 **첫 줄 trim 원문**으로 폴백 비교(그래도 없으면 `UnknownAfter`).
- 잡음 주의: gh/npm은 "A new release is available" 같은 **업데이트 권유 배너**를 stdout/stderr에 끼워 넣는다 → 첫 줄 + 첫 매치 규칙이 이걸 배제한다. 배너가 첫 줄에 오는 도구가 나오면 "버전 토큰을 포함한 첫 줄"로 규칙을 좁힌다(테이블에 도구별 `version_line_hint` 추가 여지).
- 비교는 **문자열 동등성**만 쓴다. semver 대소 비교를 하지 않는 이유: 다운그레이드/리라이트/날짜버전(`2.1.252`)이 섞여 "커졌는지"를 판단할 근거가 약하고, 우리가 알고 싶은 건 "바뀌었는가"뿐이다. (기각한 대안: semver 크레이트 도입 — 의존성 추가 대비 이득 없음.)

---

## 결정 5. GUI PATH 문제

### 결정
1. **`check_dev_tool_version`의 bare `Command::new(binary)`를 `resolve_binary()` 경유로 교체한다.** 이건 새 기능이 아니라 **현행 버그 수정(P0)** 이다 — Finder로 띄운 `.app`은 `/opt/homebrew/bin`이 PATH에 없어(cli_launcher.rs L5-7 실측) 지금 패키징 빌드에서 6종 중 git 빼고 전부 "설치 안 됨"으로 표시될 것이다. 업데이트 기능과 무관하게 먼저 고쳐야 한다.
2. **후보 경로는 `DEV_TOOLS` 테이블의 `path_candidates` 필드**에 둔다(결정 2). `github_integration.rs`의 `GH_CANDIDATES` / `cloudflare_integration.rs`의 `WRANGLER_CANDIDATES` 같은 모듈별 상수를 늘리지 않고 도구 정의 한 곳에 모은다. 기존 두 상수는 그대로 두고(회귀 위험 0), 신규 코드만 테이블을 참조한다.
3. **실행기(runner) 자체도 같은 방식으로 해석한다** — 놓치기 쉬운 핵심.
   ```
   Brew : ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]        // 실측: 심볼릭 아닌 실파일
   Npm  : ["/opt/homebrew/bin/npm", "/usr/local/bin/npm", "~/.npm-global/bin/npm"]
   SelfBinary : 대상 도구 자신의 해석된 경로 재사용(claude update)
   ```
   실행기가 해석되지 않으면 `Action::Manual(NoRunner)`.
4. **자식 프로세스 PATH를 명시적으로 세팅한다.** 절대경로 실행만으로는 부족하다 — 실측: `/opt/homebrew/bin/npm`의 첫 줄은 `#!/usr/bin/env node`다. PATH에 node가 없으면 **`env: node: No such file or directory`로 즉사**한다. brew도 내부에서 git/curl/ruby를 부른다.
   → `PATH = "<brew_prefix>/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"`를 자식 env로 주입(부모 env 상속 + PATH만 덮어쓰기). `HOME`은 상속(brew/npm 캐시·설정이 필요).
5. `path_candidates`의 `~`는 `dirs::home_dir()`로 확장한 뒤 `resolve_binary`에 넘긴다(`resolve_binary` 시그니처는 `&[&str]`이므로 확장된 `String`들을 만들어 `&[&str]`로 넘기는 얇은 래퍼를 `cli_launcher`에 추가). claude 후보에 `~/.local/bin/claude`가 반드시 필요하다(실측: `/opt/homebrew/bin/claude → ~/.local/bin/claude`).

- 기각한 대안 A: **로그인 셸을 띄워 PATH를 상속**(`/bin/zsh -lc 'command -v pnpm'`). 기각 이유: **셸을 경유하는 순간 제약 2(셸 금지)를 깬다.** 사용자 `.zshrc`가 무엇을 하는지 앱이 통제할 수 없고, 셸 시작이 느리며(수백ms×6), 문자열 인용 문제가 생긴다.
- 기각한 대안 B: 앱 시작 시 `launchctl`/로그인 셸에서 PATH를 1회 추출해 프로세스 전역 `env::set_var("PATH", …)`. 셸 경유 문제는 같고, 전역 변경이라 부작용 범위가 넓다. (다만 "PATH를 자식에만 주입"하는 4번은 이 아이디어의 안전한 축소판이다.)

---

## 결정 6. 장시간 실행 / UI 블로킹

### 결정
| 항목 | 결정 |
|---|---|
| 커맨드 형태 | `#[tauri::command] async fn update_dev_tool(...)` + 본문은 `tauri::async_runtime::spawn_blocking`으로 프로세스 실행 |
| 타임아웃 | **테이블 행별 `timeout_secs`**. 잠정: brew 600s, npm 300s, claude update 300s, `--version` 조회 10s |
| 취소 | **v1 미지원**. 타임아웃 강제종료만 |
| 동시 실행 | **전역 뮤텍스로 1건씩**. "모두 업데이트"는 프론트에서 **순차** 호출 |
| 진행 표시 | 스피너 + 경과 초. 로그 스트리밍은 v1 범위 밖(필요해지면 `app.emit`으로 라인 이벤트 추가 — 반환 타입 변경 없음) |

### 현행 `run_claude_command`의 동기 `.output()`을 그대로 따를 때의 리스크
Tauri v2에서 **`async`가 아닌 커맨드는 메인 스레드에서 실행된다**. `brew upgrade`가 60초 걸리면 **창 전체가 60초간 얼어붙는다**(리사이즈·클릭·다른 화면 이동 전부 불가, macOS가 "응답 없음"으로 표시할 수 있음). 지금 `update_plugin`이 이미 이 리스크를 갖고 있으나 `claude plugin update`가 대개 수 초라 드러나지 않았을 뿐이다. brew는 수 분이라 반드시 드러난다. → **동기 선례는 여기서 따르지 않는다.** (`run_claude_command`의 *argv 배열·stdout 그대로 반환* 패턴은 그대로 따르고, *동기 실행*만 바꾼다.)

### 파이프 교착(deadlock) — 반드시 처리
`.output()`은 내부적으로 안전하지만, 타임아웃을 걸려면 `spawn()` + `try_wait()` 폴링으로 바꿔야 하고 그 순간 **stdout/stderr 파이프 버퍼(64KB)가 차면 자식이 write에서 멈춰 영원히 끝나지 않는다**. brew는 출력이 수백 KB 나온다.
→ `spawn()` 직후 stdout/stderr 각각에 **읽기 전용 std::thread를 붙여** 버퍼로 빨아들이고, 메인은 100ms 간격 `try_wait()` 폴링으로 데드라인을 감시한다. 종료 후 리더 스레드 join.

### 강제 종료는 프로세스 그룹 단위
`brew`는 curl/git/ruby 손자 프로세스를 낳는다. `child.kill()`은 직속 자식만 죽여 **손자가 계속 다운로드**한다.
→ `std::os::unix::process::CommandExt::process_group(0)`으로 새 프로세스 그룹을 만들고, 타임아웃 시 `kill(-pgid, SIGTERM)` → 3초 유예 → `SIGKILL`. (`libc` 의존성 1개 추가 필요 — 대안: 유예 없이 `child.kill()`만 하고 고아 프로세스를 감수. 이 앱 규모에선 후자도 수용 가능하나, 다운로드 좀비가 남아 다음 brew 실행을 락으로 막을 수 있어 그룹 kill 권장.)

### 취소를 v1에서 빼는 근거
`brew upgrade` 중간에 죽이면 keg가 반쯤 설치된 상태 + `.brew` 락 파일이 남아 다음 실행이 막힐 수 있다. 즉 **취소는 "안전한 되돌리기"가 아니라 또 하나의 파괴적 행위**다. 타임아웃(정말 매달린 경우)만 그 리스크를 감수할 가치가 있다. 사용자에겐 "실행 중에는 창을 닫지 마세요"를 안내한다.
- 감당 방안: 타임아웃 결과는 `TimedOut`(verified=false)으로 **상태 불명을 명시**하고, "다시 확인"을 눌러 실제 버전을 재조회하도록 유도한다.
- 앱 종료 시: 실행 중이면 종료를 막지 않되, 자식이 프로세스 그룹으로 분리돼 있으므로 계속 진행된다(brew를 반쯤 죽이는 것보다 낫다). 다음 실행 때 버전 재조회로 결과가 드러난다.

### 멱등성
모든 액션은 재실행 안전하다(`brew upgrade`/`npm i -g`/`claude update` 모두 이미 최신이면 no-op → `AlreadyLatest`). 실패 후 재시도에 별도 처리가 필요 없다. 단 **동시 실행만은 금지**(brew 전역 락 충돌 → 무의미한 `Failed`).

---

## 부록 A. `brew upgrade`의 의존성 연쇄 업그레이드 리스크

**문제**: 사용자가 "pnpm 업데이트"를 눌렀는데 `brew upgrade --formula pnpm`이 pnpm의 의존성(예: node)까지 올리고, 나아가 그 의존성을 쓰는 다른 formula를 재빌드할 수 있다. 사용자 기대("pnpm만 바뀐다")와 실제가 어긋나며, 이건 **되돌리기 어려운 변경**이다.

**좁힐 수단과 한계**
1. `--formula` 명시 → 캐스크(앱) 오염 차단. (효과 확실, 비용 0)
2. **`--dry-run` 프리뷰 → 사용자 확인 → 실행** (권장, v1 포함). `brew upgrade --dry-run --formula <f>`가 "무엇이 바뀔지" 목록을 낸다. 그 목록을 화면에 그대로 보여주고, 목록이 대상 formula 1개가 아니면 UI가 경고를 띄운다. 테이블에 `preview_args` 필드 하나만 추가하면 되므로 구조 비용이 작다.
3. `HOMEBREW_NO_INSTALL_CLEANUP=1` → 성공 후 옛 keg를 지우지 않아 롤백 여지를 남긴다(brew는 진짜 다운그레이드를 지원하지 않지만, 옛 keg가 남아 있으면 `brew link` 등 수동 복구 여지가 생긴다).
4. **한계(정직하게)**: brew의 모델상 "의존성은 절대 건드리지 말고 이것만 올려라"는 옵션이 **없다**. 완전 차단은 불가능하다. 따라서 이 설계의 입장은 **차단이 아니라 사전 공개(dry-run) + 사후 기록(log_tail)** 이다.

**`plan_id` 대조 (승인 게이트를 플래그가 아니라 대상으로 검증)**
프리뷰 응답에 `plan_id = sha256(runner_path ‖ argv ‖ formula ‖ normalized_before)`를 담고, 실행 커맨드는 이 값을 필수 인자로 받아 **실행 직전 재계산해 대조**한다. 불일치(그 사이 brew 인덱스/설치 상태가 바뀜)면 실행을 거부하고 재프리뷰를 요구한다. "확인 눌렀음(bool)"만 보는 게이트는 사용자가 본 것과 다른 것이 실행될 여지를 남긴다. `sha2`는 이미 의존성에 있다.

---

## 부록 B. 비대화형 실행 중 프롬프트 정지 위험과 차단책

**위험**: TTY 없는 자식이 `Password:`, `Are you sure? [y/N]`, `Do you want to continue?`를 만나면 **입력을 기다리며 영원히 멈춘다**(타임아웃까지 UI가 "업데이트 중…"으로 남는다).

**차단책 (다층)**
1. **`stdin(Stdio::null())`** — 가장 중요. 프롬프트가 즉시 EOF를 받아 **정지 대신 실패**한다. 실패는 타임아웃보다 훨씬 좋은 결과다(빠르고, 로그가 원인을 말해준다).
2. **sudo가 필요한 계획은 애초에 만들지 않는다.** 사전 검사: 대상 prefix(`<brew_prefix>/Cellar`, `npm root -g`, claude 설치 디렉터리)가 **현재 uid로 쓰기 가능한지** 확인 → 불가면 `Manual(NotWritable)`. (실측: `/opt/homebrew`는 사용자 소유라 sudo 불필요, `/usr/bin`은 root:wheel이라 애초에 `SystemManaged`로 분류돼 실행 대상이 아니다.)
3. **환경변수로 대화·잡음 제거** (테이블 행의 `env` 필드에 데이터로 둔다 — 잠정값):
   - 공통: `NO_COLOR=1`, `TERM=dumb`, `CI=1`
   - brew: `NONINTERACTIVE=1`, `HOMEBREW_NO_ENV_HINTS=1`, `HOMEBREW_NO_ANALYTICS=1`, `HOMEBREW_NO_INSTALL_CLEANUP=1`
   - npm: `npm_config_yes=true`, `npm_config_fund=false`, `npm_config_audit=false`, `npm_config_progress=false`
4. **`HOMEBREW_NO_AUTO_UPDATE`는 의도적으로 설정하지 않는다(=자동 갱신 켜둠).** 끄면 오래된 인덱스로 "이미 최신"이라는 **거짓 판정**이 나온다(제약 4 정신 위배). 대신 그 시간을 타임아웃 예산(600s)에 넣고, UI에 "Homebrew 인덱스 갱신 때문에 첫 실행은 오래 걸릴 수 있습니다"를 표시한다.
   - 기각한 대안: `brew update`를 별도 버튼/단계로 분리. 단계가 늘어 UX가 복잡해지고, 사용자가 건너뛰면 같은 거짓 판정이 난다.
5. `claude update`의 프롬프트 유무는 **미확인(researcher 확인 대상)**. 확인 전까지 stdin=null + 타임아웃에 의존한다.

---

## 부록 C. 커맨드 표면(프론트 계약)과 화이트리스트 검증

```
check_dev_tools() -> Vec<DevToolStatus>            // 기존 이름 유지, 필드 확장
  DevToolStatus { id, name, installed, version,
                  path?, installMethod?, actionKind: "run"|"manual"|"none", manualHint? }

preview_dev_tool_update(toolId: String) -> DevToolPreview
  DevToolPreview { id, planId, willRun: bool, commandDisplay, affected: Vec<String>, notes }

update_dev_tool(toolId: String, planId: String) -> DevToolActionResult
install_dev_tool(toolId: String, planId: String) -> DevToolActionResult   // INSTALL_TABLE 사용
open_manual_instruction(toolId: String) -> TerminalLaunchResult           // 서버측 고정 문자열만
```

**임의 문자열이 argv로 유입되는 경로 = 0**의 근거 체인:
1. 프론트가 보내는 값은 `toolId`(+`planId` 해시)뿐이다.
2. `toolId`는 `DEV_TOOLS`의 `key` 6개와 **정확히 일치**해야 한다(`from_key()`가 `None`이면 즉시 `Err`). 일치한 순간 이후 코드는 `ToolId` enum만 들고 다니고 **입력 문자열 자체를 버린다**.
3. argv의 리터럴은 `&'static str`, 동적 슬롯은 **파일시스템 canonical 경로에서 파싱된 값**뿐이며 결정 1의 정규식 검증을 통과해야 한다.
4. 셸 미경유(`std::process::Command`), `capabilities/default.json` 변경 없음(shell/fs 플러그인 미사용 유지).
5. `planId`는 해시 비교에만 쓰이고 argv에 들어가지 않는다.

**프론트 변경 필요 사항**(다른 에이전트 위임용 메모)
- `src/mockData.ts` `MOCK_DEV_TOOL_META` **삭제** — 하드코딩된 `latestVersion`/`updateAvailable`(제약 4 위반)을 제거하면 "N개 업데이트 가능" 배지와 "업데이트 → vX" 라벨이 성립하지 않는다. 버튼은 **"업데이트"**(실행 후 판정)로 바뀌고, 헤더는 "모두 업데이트 (N)" → **"전체 업데이트"**(순차 실행)로 바뀐다.
- `docker` 행 제거: `lib.rs` `DEV_TOOLS`(7→6), `mockData.ts` L193.
- `state.devTools.mockUpdatedVersion` 삭제 → 실행 결과의 `versionAfter`로 대체.

---

## 자기검증 (설계 4대 의무)
- ① 트레이드오프: 결정 1(하드코딩맵/brew조회/문자열판별 기각), 2(`Option<RunPlan>` 기각), 3(추정실행 기각 + 터미널 위임 절충), 4(semver 비교 기각, `CommandResult` 재사용 기각), 5(로그인셸·전역PATH 기각), 6(동기 선례 기각), 부록 A(4번 항목에서 한계 자인), B(auto-update 끄기 기각) — 모든 결정에 대안·포기한 것·감당 방안 명시.
- ② 고유성: §0에 현행(수동 터미널) 대비 차별점 2가지와 그로부터 나온 구조(경로 파싱 판별 / 실행 후 재조회 판정)를 인용해 연결. `node@22` 실측 사례가 이 설계 고유의 근거.
- ③ 비정상: 타임아웃·프로세스그룹 kill·파이프 교착·프롬프트 정지·sudo·부분성공(`UnknownAfter`)·멱등성·동시실행 락·경로 소실(업데이트 후 canonical 경로 증발)·argv 인젝션(경로 컴포넌트) 전부 다룸.
- ④ 완결성: 모든 커맨드에 시그니처·반환 struct 필드·에러/거부 조건·검증 규칙(화이트리스트, 정규식) 명시. 테이블 셀은 "잠정" 표기로 비워도 성립.
- **미확인(researcher 대기)**: 각 도구의 정확한 업데이트 argv, `claude update`의 대화형 프롬프트 유무, `pnpm self-update` 사용 가능 여부(brew 설치본과의 충돌), wrangler의 사내 표준 설치 경로(brew vs npm -g).
