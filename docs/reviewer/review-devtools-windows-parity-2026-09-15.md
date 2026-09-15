# 개발 환경 화면 Windows 완전 지원 리뷰 보고서

리뷰 페르소나 패널(5명, 전원 재사용 / 신규 0):
`persona-process-execution-security.md`, `persona-claimed-vs-verified.md`,
`persona-heterogeneous-machine-operator.md`, `persona-design-contract-auditor.md`,
`persona-zero-base-redesigner.md`(발산형)

리뷰 대상: 브랜치 `feat/devtools-windows-parity` (main 대비 5커밋 `ce9b23e`…`ff5ce31`, 21파일 +3,400/−456)
설계 정본: `docs/design/devtools-windows-parity.md`
target_id: `devtools-windows-parity` — **최초 리뷰(풀패널)**
작업 등급: Sensitive (노출 범위 축소 미적용 — 풀패널 필수 요건 충족)
리스크 범주: 로컬 실행 경계 / 설치 프로그램 구동
리뷰 일자: 2026-09-15

**종합 판정: 🟡 Amber** — Critical 0건, **Major 3건**. 머지 차단 사유: **있음(M1·M2·M3)**.

---

## 요약 (2분 규칙)

플랫폼을 `cfg`가 아니라 `Platform` 인자로 받는 순수함수로 떼어낸 전략은 **실제로 작동한다** — Windows 로직의 대부분이 macOS `cargo test`에서 진짜로 실행되고, 구현자가 경고한 `Path`/`join` 함정의 잔재도 이번 diff에서 발견되지 않았다. 다만 그 전략에 **세 개의 구멍**이 있다: ①합성 골든 테스트가 "자기가 정한 규칙을 자기가 확인"하는 자리가 한 곳 실재하고(M1 — `DEV_TOOLS`의 Windows 후보 경로와 Windows 분류기가 pnpm 레이아웃에 대해 **서로 모순**되어, 어느 쪽이 맞든 pnpm 설치 wrangler의 업데이트 경로가 깨진다), ②`preview_reliable:false`를 화면이 "실패/시간 초과"라고 **거짓 서술**하며(M2 — 이 작업이 없애려던 거짓 표시와 정확히 같은 종류), ③B1 보안수정의 Windows 쪽 인용 규칙이 **어디에서도 독립 검증되지 않는다**(M3 — windows-latest CI 러너가 이미 있는데도 그 위에서 도는 것은 동어반복 등식 테스트뿐).

세 건 모두 처방이 국소적이다. M1은 실기 검증 전이라도 구조적으로 닫아야 하고, M2·M3은 각각 프런트 2줄 / 테스트 1개로 닫힌다.

---

## 페르소나 재사용 판정 (산출물 게이트)

착수 전 `docs/reviewer/personas/INDEX.md`를 Read해 "역할개념" 열을 스크리닝했다. 이번 라운드의 리스크 표면 4종(실행 argv/셸 안전성, 주장 대 관측, 이기종 머신 현실성, 설계 조항 대조)이 모두 기존 역할개념에 이미 대응되어 **신규 페르소나 0개**다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-process-execution-security.md` | **재사용** | 역할개념("이 앱이 실행하는 명령의 argv·셸·env·프로세스 수명이 안전한가")이 그대로 과녁. B1/B2/B3 3건이 전부 이 축. 6대 요소 무수정. |
| `persona-claimed-vs-verified.md` | **재사용** | 위임서 제1질문("순수함수 테스트가 진짜 Windows 동작을 등가로 모사하는가, 동어반복인가")이 이 페르소나의 V1·V4 기준 자체. 6대 요소 무수정. |
| `persona-heterogeneous-machine-operator.md` | **재사용** | "우리 조직 이기종 머신에서 실제로 뜨는가"가 이번 작업의 동기(직원 90% Windows)와 정확히 일치. 6대 요소 무수정. |
| `persona-design-contract-auditor.md` | **재사용** | `docs/design/devtools-windows-parity.md` 577줄이 판정 정본으로 지정됐고, 그 §B.2 "run 3 / manual 3 도구 단위 일치" 단언 대조가 이 페르소나의 역할. 6대 요소 무수정. |
| `persona-zero-base-redesigner.md` (발산) | **재사용** | 발산형 최소 1명 규칙 담당. "플랫폼 축을 하나 더 늘리는 것이 애초에 옳은 구조인가"를 묻는다. 6대 요소 무수정. |

신규 0건이므로 INDEX.md 행 추가는 없고 "최근 재사용" 열만 갱신했다.

---

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| M1 | 🟠 Major | 설계조항 / 이기종머신 | `dev_tools/mod.rs:186-189` ↔ `dev_tools/classify.rs:323-341` ↔ `dev_tools/plan_table.rs:414-418,432-436,466-474` | 4개 파일 Read + `grep -n PnpmStandalone src-tauri/src/dev_tools/*.rs`로 UPDATE_TABLE 행 전수 | Wrangler의 Windows 후보는 `%LOCALAPPDATA%\pnpm\wrangler.cmd`(`bin` 없음)인데 Windows 분류기는 `…\pnpm\bin\` 하위일 때만 `PnpmGlobalPackage`로 판정한다 → 실제 탐지 경로는 `PnpmStandalone`이 되고, `PnpmStandalone` 행은 `tool: Some(Pnpm)` 하나뿐이라 `lookup_action(Wrangler, …)`가 폴백 `MANUAL_UNKNOWN_METHOD`로 떨어진다. 설계 §B.2(180·185행)가 단언한 "Wrangler = run, 도구 단위까지 macOS와 일치"가 성립하지 않는다 | ①`\bin\` 유무가 아니라 **파일명**(pnpm 자신 vs 그 외)으로 Standalone/GlobalPackage를 가른다 ②`win_first_segment` 결과에서 `.exe/.cmd/.bat`를 제거한다(npm shim 분기와 동일 처리) ③**교차 불변식 테스트**: 6개 도구 × 각 `windows_path_candidates`를 분류기에 먹여 `Unknown`/미매칭으로 떨어지는 조합이 0인지 검사 |
| M2 | 🟠 Major | 주장vs관측 / 이기종머신 | `dev_tools/query.rs:186-208` ↔ `src/views/devTools.ts:300-304, 480-486` | 세 파일 Read | winget은 dry-run이 없어 `preview_reliable=false`를 **정상 상태로** 내는데, 프런트는 그 값을 "미리보기 확인에 **실패했거나 시간이 초과되어**"·"미리보기를 확인하지 못했습니다(**실패/시간 초과**)"로 렌더한다. 실패한 적이 없는데 실패했다고 말하는 것이며, 바로 아래 `notes`("winget은 사전 시뮬레이션을 제공하지 않아…")와 화면 안에서 자기모순이다. gh는 Windows run 3종 중 하나라 이 배너가 **항상** 뜨고, 배치 업데이트에서도 같은 거짓 사유로 상시 제외된다 | 최소안(계약 불변): 배너·토스트 문구를 원인 중립("미리보기로 영향 범위를 확인할 수 없습니다")으로 바꾸고 구체 사유는 이미 정확한 `notes`가 말하게 한다 — 프런트 2줄. 근본안: 계약에 `previewUnavailableReason`(`"failed"`/`"notOffered"`) 1필드 추가 |
| M3 | 🟠 Major | 실행보안 / 주장vs관측 | `dev_tools/platform_tests.rs:426-432, 513-524` + `.github/workflows/ci.yml:63-88` | 두 파일 Read + `grep -rn "cfg(windows)" -A2 src-tauri/src/`로 Windows 전용 테스트 전수(총 3개, 전부 dev_tools 무관) | B1 수정의 Windows 쪽 검증이 `assert_eq!(win, format!("'{}'", case.replace('\'', "''")))` — **함수의 구현식을 테스트가 그대로 재작성한 동어반복**이다. 같은 루프에서 Mac은 실제 `sh -c` 왕복으로 실측한다. 3계층(Rust argv 이스케이프 → `powershell.exe -Command` 명령행 파싱 → PowerShell 스크립트 파싱) 중 앞 두 계층은 아무도 보지 않았고, 하필 그 입력이 B1이 지목한 외부 출처 데이터(MCP 서버 이름)다. `ce9b23e`가 만든 windows-latest 러너 위에서도 이 동어반복이 그대로 돌 뿐이다 | `#[cfg(windows)]` 왕복 테스트 추가 — mac 테스트(`platform_tests.rs:488-511`)와 동형으로 실제 `powershell.exe -NoProfile -Command <line>`을 실행해 악성 토큰이 인자 하나로 복원되고 마커 파일이 생기지 않는지 확인. CI 러너가 이미 있어 추가 비용 0. 근본 대안: `-Command` 대신 `-EncodedCommand`(UTF-16LE base64)로 넘겨 바깥 두 계층의 인용 모호성을 통째로 제거 |
| m4 | 🟡 Minor | 설계조항 | `dev_tools/plan_table.rs:393-399` | 파일 Read + `grep -n MANUAL_NO_RUNNER` | `MANUAL_NO_RUNNER`가 "필요한 실행 도구(brew/npm/**winget**)…"로 단일 상수다. 바로 아래 `install_manual_plan`(488-493행)은 mac/win을 분리했는데 이 상수만 합쳐져 있어, macOS 사용자에게 존재하지 않는 도구 이름을 안내한다 — 이번 작업이 없앤 거짓 안내의 역방향 | `install_manual_plan`과 동일하게 상수 2개로 분리 |
| m5 | 🟡 Minor | 실행보안 | `dev_tools/plan_table.rs:262-275`(install) vs `277-291`(upgrade) | 두 상수 Read | install에는 `--accept-package-agreements`가 있고 upgrade에는 없다. 둘 다 `--disable-interactivity --silent`라 동의가 필요한 순간 프롬프트를 못 띄우고 non-zero로 끝나 화면이 "업데이트 실패"로 표시된다. 비대칭에 대한 설명이 주석에 없다 | 의도적 비대칭이면 근거 주석 1줄, 아니면 upgrade에도 추가 |
| m6 | 🟡 Minor | 실행보안 | `dev_tools/platform.rs:202-205` (`windows_parent_dir`) | 파일 Read + `is_absolute_dir`(279-293행) 대조 | `rfind('\\')`만 본다. 그런데 `is_absolute_dir`은 `C:/…`(정방향 슬래시)를 **절대경로로 받아준다** — 그렇게 통과한 항목이 `compose_path_env`(211-234행)로 오면 bin 디렉터리를 못 뽑아 자식 PATH의 최우선 항목이 통째로 빠진다. 현재 모든 후보 리터럴이 `\`이고 `dunce::canonicalize`가 `\`로 정규화해 실제 도달 가능성은 낮다 | `rfind(['\\', '/'])` — 한 글자 수정 |
| m7 | 🟡 Minor | 이기종머신 | `dev_tools/actions.rs:100-125` + `plan_table.rs:277-291` | 두 파일 Read | UAC 승격을 사용자가 **거부**했을 때의 winget 종료코드가 별도 매핑 없이 일반 실패 분기로 떨어진다(`WINGET_ALREADY_LATEST_EXIT_CODE`만 특별 취급). 사용자는 "내가 취소한 것"과 "설치가 깨진 것"을 구분할 수 없다 | 승격 거부에 해당하는 `APPINSTALLER_CLI_ERROR_*` 종료코드를 `AlreadyLatest`와 같은 방식으로 별도 문구에 매핑(구체 값은 실기/문서 확인 필요 — 이 리뷰에서 단정하지 않는다) |
| m8 | 🟡 Minor | 이기종머신 | `dev_tools/plan_table.rs:544-549`(Node), `550-557`(Gh) vs `559-564`(Git) | 세 상수 Read | Windows 매뉴얼 안내 중 Git 문구만 "관리자 권한 승인 창이 뜰 수 있습니다"를 담았다. Node·gh의 `winget install`도 머신 스코프라 동일하게 UAC가 뜨는데 예고가 없다 | 세 문구의 UAC 예고를 통일 |
| n9 | ⚪ Nit | 주장vs관측 | `dev_tools/platform.rs:149-157`, `dev_tools/classify.rs:246-256` 주석 | 두 주석 Read | "`Path`/`PathBuf`는 **컴파일 호스트**의 구분자 규칙을 따른다"는 서술은 엄밀히는 **타깃**이 맞다(Windows 타깃 빌드에서는 `\`가 정상 구분자). 주석이 지시하는 대응(문자열 조립)은 두 타깃 모두에서 옳으므로 결함은 아니지만, 이 주석을 읽고 판단할 다음 사람이 "Windows 빌드에서도 `Path`가 깨진다"고 오해할 수 있다 | "타깃"으로 표현 정정 |

---

## 기각된 지적

| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 실행보안 | `build_command_display`(`install_resolver.rs:248-252`)가 `Path::new(runner_path).file_name()`을 써서 Windows 경로 전체가 파일명으로 잡힌다 | **기각** | `std::path`는 컴파일 호스트가 아니라 **타깃**에 따라 분기한다 — Windows 타깃 빌드에서 `\`는 정상 구분자다. macOS 테스트 바이너리에서만 어긋나며 이 함수는 골든 테스트 대상이 아니다. (오해의 근원인 주석 표현은 n9로 강등해 남김) |
| 이기종머신 | 설치는 성공했는데 PATH 갱신이 실행 중 프로세스에 반영되지 않아 "실패"로 표시될 것이다 | **기각** | 설치 후 재확인은 `resolve_tool_path`(`mod.rs:215-217`)의 **절대경로 후보**로 이뤄지고, gh/claude/wrangler의 Windows 설치 결과 자리가 전부 `windows_path_candidates`에 실재한다(`mod.rs:120-124, 92-97, 186-189`) — PATH에 의존하지 않는다. 오히려 `compute_path_visibility_windows`(`diagnostics.rs:238-258`)가 이 상황을 "PATH엔 아직 안 보임 + 새 터미널을 열어라"로 정확히 서술한다 |
| 주장vs관측 | B1 시그니처 변경으로 테스트 7개가 삭제돼 회귀 커버리지가 줄었다 | **기각** | 삭제분은 `shell_single_quote`의 `printf %s` 왕복 4개 + `build_install_command` 문자열 고정 2개 + 인젝션 1개다. 대체물이 **더 강하다** — `platform_tests.rs:488-511`은 악성 이름을 실제 `sh -c`로 실행해 부작용(마커 파일 생성)이 없는지까지 본다(삭제된 `printf` 왕복보다 상위 검증). Windows 쪽 공백만 M3로 분리 유지 |
| 설계조항 | `session_list.rs` 1,174줄이 Rust 1,000줄 규율 위반 | **스코프 밖** | main에 이미 있던 기존 위반이며 이번 diff와 무관(위임서 명시 제외) |

---

## 페르소나별 관점

### [실행 프로세스 보안] — 판정: 🟡 Amber
B1/B2/B3 수정은 **모든 경로에 일관 적용됐고 우회로가 남지 않았다**. 근거:
- B1 — `open_terminal_command` 시그니처가 `&'static str`(`cli_launcher.rs:163`)로 좁혀졌고, 남은 호출부는 `actions.rs:378` 단 하나다(`grep -rn open_terminal_command src-tauri/src/` 전수). 그 인자는 `ManualPlan.copyable_command: Option<&'static str>`이라 **타입상 런타임 값이 흘러들 수 없다**. 동적 값이 있던 5개 호출부(`github_integration.rs:95,116` / `cloudflare_integration.rs:139,157` / `mcp_manager/mod.rs:220,257`)는 전부 argv 경유 진입점으로 이관됐고 `shell_single_quote`는 삭제됐다. 주석이 아니라 타입으로 강제한 것이 이 수정의 핵심 값어치다.
- B2 — `powershell.exe`(`cli_launcher.rs:125-128`)와 `taskkill.exe`(`session_chat/turn.rs:172-177`) 둘 다 `windows_system_tool`로 `%SystemRoot%` 절대경로 조립 + `-NoProfile`. 나머지 bare-name spawn은 `resolve_binary`의 mac 전용 `--version` 프로브(`cli_launcher.rs:58-70`)뿐이고, 이는 `plat == Platform::Mac` 가드 안에 있다.
- B3 — `is_absolute_dir`(`platform.rs:279-293`)이 빈 항목·`.`·상대명·`C:`(드라이브 상대)를 전부 거부하고, 골든 테스트 3개(`platform_tests.rs:284-330`)가 UNC 허용까지 고정한다. **트림 전 원본으로 판정**하는 순서까지 주석에 근거가 남아 있다.
- 새 결함 없음. N1(`child_current_dir`로 CWD를 `%SystemRoot%`에 고정, `process.rs:221-231`)은 `.cmd` shim이 `cmd.exe`를 경유한다는 별개 규칙을 정확히 겨냥했고, mac은 `None`이라 무변경임이 실행 테스트(`process.rs:465-478`, `/bin/pwd` 실측)로 고정됐다.
- 남은 지적: M3(Windows 인용 무검증), m5, m6.

### [주장 vs 관측] — 판정: 🟠 Amber(Major 2건)
위임서가 신뢰하라고 한 측정치를 **직접 재검증했고 전부 정확했다**: `cargo test` 347 passed / 0 failed / 2 ignored(직접 실행), `dev_tools/contract.rs` diff 0, `capabilities/default.json` diff 0.

순수함수 전략 자체는 **동어반복이 아니다** — `default_path_dirs_win_…`(`platform_tests.rs:153-167`), `compose_path_env_win_…`(211-222행)은 전체 문자열을 고정하는 진짜 골든 테스트이고, 특히 `win_join`/`windows_parent_dir` 주석(`platform.rs:149-157, 197-205`)이 기록한 "최초 구현에서 `C:\Windows/system32`가 나왔다"는 실측은 이 테스트가 실제로 버그를 잡았다는 증거다. 위임서가 경고한 `Path::join`/`parent`/`strip_prefix` 잔재를 Windows 경로를 다루는 전 함수에서 훑었고 **남은 것이 없었다**(`classify_install_method_windows`는 전부 문자열 연산, `compose_path_env`는 Mac 분기에서만 `Path` 사용).

동어반복이 **실재하는 자리는 두 곳**이고 둘 다 Major로 올렸다: M3(PowerShell 인용 등식)과 M1(합성 골든 테스트가 앱 자신의 후보 경로표와 어긋난 가정을 고정하고 있다 — `classify.rs:793-803`의 테스트 입력 `…\pnpm\bin\wrangler.cmd`는 이 앱의 탐지기가 **절대 만들어내지 않는 경로**다). M2는 백엔드가 정직하게 만든 값을 프런트가 거짓 사유로 번역하는, 계층 경계에서의 거짓이다.

### [이기종 머신 운영자] — 판정: 🟠 Amber
이 화면이 Windows에서 **더 이상 전부 "설치 안 됨"으로 거짓말하지 않는다**는 1차 목표는 코드상 달성됐다: `cfg!(target_os = "macos")` 게이트 5곳(`mod.rs`의 커맨드 4개 + `query.rs`의 화면 진입)이 전부 제거됐고 6개 도구 모두 Windows 후보가 최소 1개 있다(`mod.rs:311-321` 테스트가 강제).

막다른 골목 점검: 실행기를 못 찾으면 `MANUAL_NO_RUNNER`, 분류 실패면 `MANUAL_UNKNOWN_METHOD`로 안전하게 강등되고, Windows 매뉴얼 문구는 전부 winget/npm 명령으로 교체됐다(brew 문구 누출 없음 — `plan_table.rs:814-846` 테스트가 고정). 타임아웃 시 손자 프로세스 문제도 **숨기지 않고 메시지로 알린다**(`actions.rs:33-40`).

그러나 **M1이 정확히 막다른 골목을 만든다**: pnpm으로 wrangler를 설치한 Windows 사용자는 업데이트 버튼 대신 "설치 방식을 확인할 수 없어 앱이 자동으로 실행하지 않습니다"만 보게 되고, 그 문구에는 `copyable_command`도 `doc_url`도 없다(`plan_table.rs:387-392`) — 다음 행동이 0개다. M2는 gh 사용자에게 상시 거짓 경고를 띄운다. m7·m8은 UAC 국면의 설명 공백이다.

### [설계 계약 감사] — 판정: 🟠 Amber
조항 단위 대조에서 **조용한 누락 1건**이 나왔다.

| 설계 조항 | 구현 위치 | 상태 |
|---|---|---|
| §B.2 winget은 gh 전용 러너 | `install_resolver.rs:132-134`, `plan_table.rs:444-448` | 구현 |
| §B.2 `ResolvedRunners`에 winget 캐시 필드 없음(기존 테스트 0개 수정 유지) | `runners.rs:63-89` | 구현 — 설계 스케치를 **명시적으로 기각**하고 근거를 주석에 남김 |
| §B.3 winget "이미 최신" 종료코드 매핑 | `plan_table.rs:301`, `actions.rs:105-115` | 구현 + 값 검증 테스트(`plan_table.rs:805-811`, `0x8A15002B` 양방향) |
| §B.3 winget 프리뷰는 `preview_reliable:false` | `query.rs:186-208` | 백엔드 구현 — **프런트 표현에서 의미가 뒤집힘(M2)** |
| §B.2 표: Wrangler = Windows **run**, "run 3 / manual 3 도구 단위 일치"(180·185행) | — | **조용한 누락(M1)**. 어떤 테스트도 이 단언을 검사하지 않는다 |
| §D.2 `default_path_dirs` 골든 테스트 | `platform_tests.rs:153-182` | 구현 |
| §D.4 BatBadBut 완화를 `rust-version`으로 강제 | `Cargo.toml:7-11` | 구현 — **강제 수단이 사람의 주의력이 아니라 컴파일러**다. 모범 사례 |
| §1.1 winget argv 전부 `Arg::Lit` | `plan_table.rs:874-931` | 구현 — 손으로 나열하지 않고 `UPDATE_TABLE` + `DEV_TOOLS`를 **구조적으로 순회**하는 가드(N3 보강). 새 winget 행이 어디에 추가되든 자동으로 걸린다 |

§B.2의 "도구 단위까지 정확히 일치"는 이 설계가 "완전 지원"을 정의한 **핵심 단언**인데, 그것을 강제하는 수단이 없어 실제로 어긋났다. M1의 처방 ③(교차 불변식 테스트)이 이 조항의 강제 수단이 되어야 한다.

---

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R-1 | 도구 하나의 사실이 **5곳**에 흩어져 있다 — `DEV_TOOLS.windows_path_candidates`(`mod.rs:186`), 분류기 접두사 분기(`classify.rs:323`), `UPDATE_TABLE` 행(`plan_table.rs:432`), `install_candidates()`(`install_resolver.rs:104-134`), `install_manual_plan_windows`(`plan_table.rs:541`) | 도구당 레코드 하나로 접는다: `{ detect: [경로…], classify: [경로패턴→method], update: Plan, install: [Candidate…], manual: Plan }` + "이 레코드의 `detect` 경로를 분류기에 먹이면 반드시 이 레코드가 선언한 method가 나온다"는 불변식 테스트 1개 | **M1은 이 다섯 중 둘이 서로 모순됐는데 아무 테스트도 그 둘을 마주 놓지 않아 생긴 결함이다.** 레코드로 접으면 그 모순이 잠복 버그가 아니라 테스트 실패가 된다. 플랫폼을 하나 더 늘릴 때 고칠 자리가 5→1 | 중간. **이번 브랜치에는 넣지 말 것** — "기존 테스트 0개 수정"이 이번 무변경 증명의 유일한 기계적 수단인데(설계 §C.3.1) 이 리팩터는 그 테스트들을 대거 건드린다. Windows 실기 검증이 끝난 **다음 단계**가 제자리 |
| R-2 | 설치방식을 **디렉터리 접두사로 역추정**한다(`classify.rs` 전체) | 패키지 매니저에게 직접 묻는다 — `winget list --id <id>`, `npm ls -g --depth 0 --json`, `pnpm ls -g --json` | 현재 추정은 **이 팀이 소유하지 않은 머신의 레이아웃에 대한 가정**이라 원리적으로 검증할 수 없고, M1이 그 가정이 실제로 틀릴 수 있음을 보여준다. 패키지 매니저의 답은 권위 있고 플랫폼 중립적이다 | 탐지 중 프로세스를 띄우게 되어 이 설계가 의도적으로 지킨 G-1(탐지는 스폰하지 않는다)과 충돌하고 화면 로드가 느려진다. **권고는 전면 대체가 아니라 폴백** — 경로 추정이 `Unknown`/미매칭으로 떨어졌을 때만 물어본다. 그러면 M1 같은 경우에 "확인할 수 없습니다"로 끝내지 않고 정답을 얻는다. 비용 낮음 |
| R-3 | **없는 머신(Windows)을 우회**하려고 `Platform` 인자화를 했다 | 우회는 유지하되, **있는 머신(windows-latest 러너)을 실제로 쓴다**. 테스트를 2층으로: ①순수 로직층(현행, 양 OS 공통) ②`#[cfg(windows)]` OS 상호작용층 — PowerShell 인용 왕복(M3), `.cmd` shim 실행, `path_exists`의 APPEXECLINK 동작, winget 존재 시 종료코드 | `ce9b23e` 이후 조직은 Windows 러너를 **이미 갖고 있는데**, 그 위에서 도는 Windows 전용 테스트는 총 3개뿐이고(`process_util.rs:163-173, 189-199`, `config/user_config.rs:372`) **dev_tools와 무관하다**. 즉 CI가 실질적으로 보증하는 것은 컴파일뿐이다. 설계 §8의 "미검증" 항목 중 여럿이 실기 PC 없이 닫힌다 | 낮음. 러너는 이미 있고 잡 설정 변경도 불필요(`cargo test`가 `cfg(windows)` 테스트를 자동으로 집어간다) |

---

## 트레이드오프 (페르소나 간 충돌)

1. **계약 동결(`contract.rs` diff 0) vs 화면 정직성** — 설계계약감사는 계약 불변을 이번 작업의 모범 사례로 평가했고(프런트 회귀 위험 0), 주장vs관측은 바로 그 동결 때문에 의미가 이미 고정된 boolean(`preview_reliable`)을 재사용해 M2가 생겼다고 본다.
   → **권고: 이번 브랜치는 계약을 유지하고 프런트 문구만 원인 중립으로 고친다(2줄).** 계약 필드 추가는 R-1 리팩터와 함께 다음 단계에서. 문구 수정만으로 거짓 서술은 완전히 사라지고, 정확한 사유는 이미 `notes`가 말하고 있다.

2. **"기존 테스트 0개 수정"(무변경 증명) vs 구조 개선** — 설계계약감사는 이 제약이 macOS 무변경을 증명하는 유일한 기계적 수단이라 평가하고(실제로 `ResolvedRunners`에 winget 필드를 넣지 않은 판단의 근거), 발산형은 바로 그 제약이 R-1(5곳 분산)을 고착시킨다고 본다.
   → **권고: 이번 브랜치에서는 제약을 지킨다.** 단 M1의 교차 불변식 테스트는 **기존 테스트를 수정하지 않고 추가만 하는** 형태라 제약과 충돌하지 않는다 — 지금 넣어야 한다.

3. **실기 검증 전 배포 vs 지연** — 이기종머신은 90%의 직원이 현재 거짓 화면을 보고 있으므로 빠른 배포를 지지하고, 주장vs관측은 Windows 실행 경로 중 실제로 관측된 것이 0이라는 점을 지적한다.
   → **권고: M1~M3 수정 후 1~2명 대상 선배포.** 화면이 이미 `installMethod`와 해석된 경로를 노출하므로(`src/views/devTools.ts:385`), M1류 오분류를 사용자가 스크린샷 한 장으로 보고할 수 있다. 전사 배포는 그 보고를 받은 뒤.

---

## 위임서 질의에 대한 답

**Q1. 새 구현이 다른 방식으로 거짓을 말할 여지는 없는가**
세 곳에서 말한다. ①**M2** — `preview_reliable:false`를 화면이 "실패/시간 초과"로 번역(가장 확실하고 상시 발생). ②**M1** — pnpm 설치 wrangler에 "설치 방식을 확인할 수 없습니다"(오분류에서 오는 거짓). ③**m7** — UAC 거부를 "실패"와 구분하지 못함. 반면 **PATH 미갱신으로 인한 거짓 실패 표시는 없다**(기각 항목 2 참조) — 설치 후 재확인이 절대경로 후보로 이뤄져 PATH에 의존하지 않고, PATH 가시성은 `diagnostics.rs:238-258`이 별도로 정직하게 서술한다. winget "이미 최신" 종료코드 오분류도 `WINGET_ALREADY_LATEST_EXIT_CODE` 매핑으로 닫혀 있다(값 자체는 실기 미검증이나 문서값과의 일치는 테스트로 고정).

**Q2. macOS 회귀 — 삭제된 7개 테스트의 대체 커버리지는 유지됐는가**
**유지됐고, 오히려 강화됐다.** 근거는 기각 항목 3. 추가로 macOS 무변경을 증명하는 골든 테스트가 새로 들어왔다: `macos_path_candidates_are_frozen_exactly`(`mod.rs:293-310`, 부분 매치가 아니라 배열 전체 고정), `compose_path_env_mac_matches_full_string_exactly`(`platform_tests.rs:186-208`), `build_terminal_command_line_mac_matches_existing_format_string_byte_for_byte`(380-385행), `install_manual_plan_mac_variant_keeps_brew_wording`(`plan_table.rs:766-773`), `run_process_with_timeout_does_not_pin_current_dir_on_mac`(`process.rs:465-478`, `/bin/pwd` 실행 실측). 기존 Rust 테스트 수정 0건이라는 사실 자체가 무변경의 기계적 증거다.

**Q3. `--scope user` 미지정 판단은 타당한가**
**타당하다.** 실패 모드를 비교하면 명확하다 — scope를 고정했을 때의 실패("No applicable installer found")는 **100% 재현되고 사용자가 손쓸 수 없다**(argv가 앱에 고정돼 있어 바꿀 방법이 없다). 미지정 시 최악은 UAC 프롬프트이고, 거부하면 non-zero로 화면에 드러난다. 근거도 주석(`plan_table.rs:245-261`)에 명시돼 있고 미검증임을 밝혔다. 다만 **남는 실패 모드 1건이 m7**이다(UAC 거부와 설치 실패가 화면에서 구분되지 않음) — 이걸 닫으면 판단이 완결된다. 가드 테스트가 `--scope machine` 명시를 금지하고 있는 것(`plan_table.rs:911-918`)도 이 판단과 일관된다.

**Q4. UAC가 Phase 1에서 뜨는 것이 납득 가능한가**
**납득 가능하다.** `execute=false`면 아무것도 실행하지 않고 문구만 돌려주고(`actions.rs:372-377`), `execute=true`는 사용자가 "터미널에서 실행"을 명시적으로 누른 결과다 — **동의가 클릭으로 선행한다.** 열리는 것은 눈에 보이는 대화형 창이고 UAC는 OS가 띄운다(앱이 `runas`/`ShellExecute` 승격을 직접 호출하지 않는다는 G-4도 지켜졌다). 다만 "Phase 1은 탐지뿐"이라는 표현은 부정확하며 `790c66c`가 설계서를 이미 정정한 것이 맞는 조치다. 남는 것은 **예고의 비일관성(m8)** — Git 문구에만 UAC 예고가 있다.

**Q5. 롤백 가능성 — 피처 플래그 없이 3단계로 간 결정은 타당한가**
**타당하다.** 플래그를 뒀다면 **off 상태가 곧 "전부 설치 안 됨 거짓 표시"로 복귀**하는 것이라 플래그의 안전 상태 자체가 버그다 — 보호할 대상이 없다. 롤백 단위는 브랜치 전체이지만 아직 main 미병합이고, 배포물이 단일 포터블 exe라 되돌리기 = 이전 exe 재배포로 끝난다(`CLAUDE.md` git 워크플로: 브랜치 보호·CODEOWNERS 없음). 실질적 약점은 롤백 **판단 신호**이지 롤백 **수단**이 아니다 → 트레이드오프 3의 권고(1~2명 선배포 + 화면의 `installMethod`/경로를 보고 채널로)로 닫는다.

**Q6. CI 잡(`ce9b23e`)이 실제로 검증 백본 역할을 하는가 — 무엇을 잡고 무엇을 못 잡는가**
**잡는 것**: Windows 타깃 **컴파일**(이게 가장 큰 값어치 — `#[cfg(windows)]` 블록 6곳이 이제 매 push마다 컴파일된다. 이 잡이 없으면 `session_chat/turn.rs:172-177`의 `windows_system_tool` 호출처럼 이번에 새로 추가된 크로스모듈 참조가 깨져도 아무도 모른다), 양 OS 순수함수 회귀 347건, macOS 무변경 골든 테스트.
**못 잡는 것**: OS 상호작용 **전부**. windows-latest에서 실제로 도는 Windows 전용 테스트는 총 3개뿐이고(`process_util.rs:163-173, 189-199` = `.silent()`/`.windowed()` spawn, `config/user_config.rs:372`) **dev_tools와 무관하다**(`grep -rn "cfg(windows)" -A2 src-tauri/src/` 전수 확인). 즉 PowerShell 인용(M3), `.cmd` shim 실행, `path_exists`의 APPEXECLINK 동작, winget 종료코드, UAC, `is_writable_by_current_user`의 프로브 파일 방식, `taskkill` 절대경로 — 전부 미검증이다. **"검증 백본"이라기보다 현재는 "컴파일 백본"이다.** R-3이 이 격차를 닫는 처방이고, M3이 그 첫 항목이다.

---

## 잘 된 점 (유지할 패턴)

1. **전제를 타입으로 강제한다** — B1 수정이 주석("리터럴만 넘긴다")이 아니라 `&'static str` 시그니처(`cli_launcher.rs:163`)로 강제했다. 동적 조립 호출부는 컴파일이 깨져 **구조적으로** 안전한 진입점으로 이관됐다. 같은 정신이 `Cargo.toml:7-11`의 `rust-version = "1.81"`(BatBadBut 완화를 컴파일러가 강제)에도 있다. 강제 수단이 사람의 주의력이 아닌 사례들이다.
2. **가드 테스트를 손으로 나열하지 않고 구조적으로 순회한다** — `all_winget_run_plans_in_install_and_update_tables_pass_security_gate`(`plan_table.rs:874-931`)는 N3 지적을 받고 하드코딩 배열을 `UPDATE_TABLE` + `DEV_TOOLS` 순회로 바꿨고, "winget 플랜이 0개면 가드가 무의미해진다"는 **가드의 가드**까지 넣었다. M1의 처방 ③이 따라야 할 정확한 모델이다.
3. **모르는 것을 지어내지 않는다** — `expand_path_tokens`(`platform.rs:87-118`)는 필요한 뿌리가 없으면 그럴듯한 경로를 만들지 않고 `None`을 돌려주고, `win_strip_dir_prefix`(`classify.rs:277-288`)는 UTF-8 경계가 아니면 패닉 대신 분류 실패로 간다. `InstallMethod::WingetPackage`가 설계 스케치의 `{ id: String }`을 **의도적으로 기각**하고 데이터를 갖지 않는 것(`classify.rs:37-44`)도 같은 규율이다 — 쓰이지 않을 값을 파싱해내지 않는다.
4. **설계 스케치를 근거와 함께 기각한 자리가 여럿이다** — `ResolvedRunners`의 winget 캐시 필드(`runners.rs:56-62`), `WingetPackage`의 id 필드, `--scope user` 고정. 설계서를 맹종하지 않고 기각 사유를 코드 주석에 남겼다. 이것이 리뷰를 가능하게 만든다.
5. **실측이 주석에 남아 있다** — `platform.rs:149-157`·`197-205`의 "`C:\Windows/system32`가 나왔다", "`.parent()`가 빈 문자열을 반환했다"는 **테스트가 실제로 잡아낸 버그의 기록**이다. 다음 사람이 같은 함정에 빠지지 않는다.
6. **한계를 숨기지 않고 UI 문구로 올린다** — winget 타임아웃 시 손자 프로세스가 살아남는 구조적 한계를 주석으로만 덮지 않고 사용자 메시지에 담았다(`actions.rs:33-40`, `process.rs:157-172`). "구조적으로 죽일 수 없다"와 "드물게 남는다"를 구분해 서술한 것도 정확하다.
7. **1,000줄 규율을 지키되 정본을 쪼개지 않았다** — `platform_tests.rs` 분리(`platform.rs:470-475`), `runners.rs` 신설(`runners.rs:1-6`). 테스트만 옮기고 순수함수 정본은 한곳에 남겼다.

---

## 평가기준 충족 현황

| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| B1/B2/B3이 모든 경로에 일관 적용 | 실행보안 | 필수 | ✅ | grep 전수 + 타입 강제 확인 |
| B1/B2/B3 수정이 새 결함을 만들지 않음 | 실행보안 | 필수 | ✅ | m6은 기존 코드가 아니라 신규 함수의 사소한 누락 |
| `cfg(windows)` 잔여 표면이 syscall 껍데기인가 | 주장vs관측 | 필수 | ✅ | 6곳 전수 — `path_exists`(2줄), `silent/windowed`(각 2줄), `pid_alive`(1줄), `force_kill`(2줄), `is_writable`(프로브 파일). 논리 분기 없음 |
| `Path`/`join`/`parent` 호스트 구분자 버그 잔재 | 주장vs관측 | 필수 | ✅ | Windows 경로 취급 함수 전수 — 전부 문자열 연산 |
| 순수함수 테스트가 동어반복이 아닌가 | 주장vs관측 | 필수 | ⚠️ | 대부분 진짜 골든 테스트. **2곳 예외 → M3, M1** |
| macOS 동작 무변경 | 이기종머신 | 필수 | ✅ | 기존 테스트 수정 0건 + 신규 골든 5건 |
| 화면이 거짓을 말하지 않는가 | 주장vs관측 | 필수 | ❌ | **M2**(상시 발생), M1, m7 |
| 설계 §B.2 "run 3 / manual 3 도구 단위 일치" | 설계조항 | 필수 | ❌ | **M1** — 강제 수단 없음 |
| winget argv가 전부 리터럴 + 위험 플래그 부재 | 실행보안 | 필수 | ✅ | 구조적 순회 가드 |
| 실패 시 막다른 골목이 아닌가 | 이기종머신 | 권장 | ⚠️ | M1 경로만 다음 행동 0개 |
| CI가 Windows 코드를 검증하는가 | 주장vs관측 | 권장 | ⚠️ | 컴파일만 — Q6 참조 |
| 미검증 항목이 정직하게 표기됐는가 | 주장vs관측 | 필수 | ✅ | 주석·문서 전반에 "미검증(실기 필요)" 일관 표기 |

---

## PM에게 권고

**머지 차단 사유: 있음.** M1·M2·M3 세 건을 수정한 뒤 병합을 권고한다. 세 건 모두 처방이 국소적이고 기존 테스트를 수정하지 않는다(= 이번 무변경 증명 전략과 충돌하지 않는다).

**우선순위 1 (머지 전 필수)**
- **M1** — 분류기와 후보 경로표의 모순 해소 + 교차 불변식 테스트 추가. 셋 중 유일하게 **기능이 실제로 안 되는** 건이고, 설계의 핵심 단언을 무너뜨린다. 처방 ③(교차 테스트)은 실기 검증 없이도 넣을 수 있고, 앞으로 같은 종류의 모순을 전부 잡는다.
- **M2** — 프런트 문구 2줄. 가장 싸고, 이 작업의 존재 이유("화면이 거짓말을 했다")와 정면으로 닿는다.
- **M3** — `#[cfg(windows)]` PowerShell 왕복 테스트 1개. CI 러너가 이미 있어 추가 비용 0이고, 차단급 보안수정의 Windows 쪽에 검증을 처음으로 붙인다.

**우선순위 2 (머지 후 첫 배포 전)** — m7(UAC 거부와 실패 구분), m8(UAC 예고 통일). 실기 배포 시 사용자가 가장 먼저 부딪히는 두 지점이다.

**우선순위 3 (백로그)** — m4, m5, m6, n9. 전부 수 줄 수정이나 사용자 영향이 작다.

**배포 방식** — M1~M3 수정 후 **1~2명 선배포**를 권고한다(트레이드오프 3). 화면이 이미 `installMethod`와 해석된 경로를 노출하므로(`src/views/devTools.ts:385`) 오분류를 스크린샷 한 장으로 회수할 수 있다. 전사(90% Windows) 배포는 그 피드백 이후.

**다음 단계 방향** — R-3(CI Windows 상호작용 테스트층)을 먼저, R-1(도구당 1레코드)을 실기 검증 이후에. R-2는 R-1의 폴백으로 함께 검토.

---

## 정직 보고 — 이 리뷰가 확인하지 못한 것

- **Windows 실기 동작은 이 리뷰도 전혀 검증하지 못했다.** 이 보고서의 모든 판단은 코드·테스트·설계문서를 읽어 도출한 것이며, "Windows에서 동작한다/안 한다"를 단정한 곳은 없다. M1은 **코드 내부의 모순**(두 표가 서로 어긋남)에 근거하므로 실기 없이도 성립하지만, **어느 쪽 표가 실제 pnpm 레이아웃과 맞는지는 판정하지 못했다** — 그래서 처방을 "둘 다 받아들이고 파일명으로 가른다"는 방어적 형태로 냈다.
- **화면 캡처 없음.** UI 지적(M2, m8)은 `src/views/devTools.ts` 소스를 읽어 도출했고 렌더링 결과를 보지 않았다 — Windows 빌드를 이 머신에서 실행할 수 없고, macOS에서는 winget 경로 자체가 도달 불가능해 M2의 배너를 화면으로 재현할 수단이 없다. 코드 기반 추정임을 명시한다.
- **m5(winget 동의 플래그), m7(UAC 거부 종료코드)은 winget 실행 없이는 확정할 수 없다.** 비대칭·공백의 존재는 코드로 확인했으나, 그것이 실제 실패로 이어지는지는 미확인이다 — 그래서 Major가 아닌 Minor로 두었다.
- **실행 액션 없음.** 이 리뷰는 읽기 전용이었다. 파일 수정·git 상태 변경·병합·배포를 **하지 않았다**. 실행한 것은 `cargo test`(읽기 전용, 347 passed 재확인)와 `git log/show/diff`, 그리고 이 보고서와 페르소나 적용 이력 파일 작성뿐이다.
