# 리뷰 보고서 — 개발 환경 "미설치 도구 설치" 확장 (dev_tools.rs)

- **target_id**: `malgn-vscode-devtools-real-update` / **2차** (1차: `docs/reviewer/review-devtools-2026-09-09.md`)
- **일자**: 2026-09-10 / **리뷰어**: reviewer (풀패널, 발산형 포함)
- **위임 등급**: Sensitive — 축소 없음. **풀패널 사유**: 새 실행경로 다수 등장(설치 실행 대상 1개→3개, brew install dry-run 파서 신설, 설치 판정 로직 전면 재구성)으로 "풀패널 강제 승격 조건"에 해당. 축소 모드(증분/축소) 미적용.
- **리스크 범주**: 사용자 머신 비가역 변경(로컬 전역 개발도구 설치·업데이트 명령 실행) — 1차와 동일, 범주 불변.
- **리뷰 대상 스냅숏**: `scratchpad/dev_tools.SNAPSHOT.rs` (3,683줄, md5 `981212e9a881362c414a8e688f335396`). **모든 라인번호는 이 스냅숏 기준.**
- **판정 정본**: `docs/design/devtools-install-matrix.md`(340줄) + 복구된 원 설계 `scratchpad/scratch-design-devtools-update.md`.

## 종합 판정: 🟡 **Amber**

**Critical 0 / Major 5 / Minor 8 / Nit 2 / Rethink 2.**
(a)의 구조적 불변식은 **전수 확인 결과 위반 0건**이고, 직전 리뷰의 막다른 골목(#1·#2·#3)은 실제로 닫혔다. 그러나 ① "전체 업데이트" 버튼 한 번이 미설치 도구를 개별 확인 없이 **실설치**하게 된 점, ② 설계 §4.2가 요구한 설치 경로 쓰기권한 사전검사가 **기록 없이 사라진** 점 두 가지가 이 등급의 승인 전 조치 대상이다. Red가 아닌 이유: 실행되는 명령이 전부 공식 패키지 매니저의 멱등 명령이고, sudo/UAC를 유발하지 않으며, 사용자가 누른 화면에 "설치/업데이트도 실제로 실행됩니다"가 상시 표기돼 있어 동의가 전무하지는 않다.

**스냅숏 정합성**: 리뷰 종료 시점 `diff` 결과 라이브 파일 `src-tauri/src/dev_tools.rs`와 스냅숏이 **완전히 동일**(md5 일치). 리뷰 도중 대상 파일 변동 없음 — 1차에서 프로세스 문제로 올렸던 상황은 이번엔 재현되지 않았다.

---

## 1. 페르소나 재사용 판정

| 페르소나 | 유형 | 판정 | 사유 |
|---|---|---|---|
| `persona-process-execution-security.md` | 수렴 | **재사용** | INDEX.md 역할개념 열 대조 결과 동일("실행 명령의 argv·셸·env·프로세스 수명"). 실행 대상 개수만 늘었지 안전성 축은 불변. 본문 6대 요소 무수정, 적용 이력만 append |
| `persona-claimed-vs-verified.md` | 수렴 | **재사용** | 동일("주장 vs 관측, 코드↔주석 일치"). 직전 Major #4 재발 판정이 이 페르소나의 V4 기준 그 자체 |
| `persona-heterogeneous-machine-operator.md` | 수렴 | **재사용** | 동일("이기종 머신에서 실제로 뜨는가, 막다른 골목인가"). 직전 #2·#3 현재 상태 판정 담당 |
| `persona-zero-base-redesigner.md` | **발산** | **재사용** | 동일. 발산형 최소 1명 규칙 충족. +1,035줄 증가로 "복잡도가 가치의 원천인가" 질문의 재료가 늘었다 |
| `persona-design-contract-auditor.md` | 수렴 | **신규** | **중복 없음 근거**: 착수 전 `INDEX.md`를 Read해 4개 행의 역할개념을 대조했고, "구현이 설계 정본 조항을 조항 단위로 지켰는가"에 해당하는 행이 없었다. 1차 리뷰에는 판정 기준이 될 설계 정본 자체가 없었다(원 설계는 저장소에서 삭제된 상태) — 2차에서 `devtools-install-matrix.md` 340줄이 정본으로 지정되며 **처음 생긴 리스크 표면**이다. 가장 인접한 `claimed-vs-verified`는 코드↔주석·코드↔계약 일치를 보지 설계서 조항 대조를 보지 않는다(그 파일 §2 관심사·§3 V1~V6에 설계 조항 항목 없음) |

산출물 게이트: `docs/reviewer/personas/persona-*.md` **5개 실재**(`ls` 확인). INDEX.md 5행으로 갱신.

**동시 편집 주의**: 리뷰 중 다른 세션(`malgn-vscode-session-chat`)이 같은 INDEX.md와 2개 페르소나 파일을 갱신했다. 그 세션의 이력을 덮어쓰지 않고 "최근 재사용" 열에 병기했으며, INDEX.md 상단에 병기 규칙을 추가했다.

---

## 2. 위임자 지정 4문항 — 답

### (a) 불변식 준수 — "설치 argv는 `Arg::Lit` 전용" ✅ **위반 0건**

`install_candidates()`(1015~1046)가 반환하는 **모든** RunPlan의 args를 전수 인용해 대조했다. 샘플링 없음.

| 도구 | 후보 | RunPlan | `args` 슬롯 전수 | `preview_args` 슬롯 전수 |
|---|---|---|---|---|
| Gh | 1/1 | `RUN_BREW_INSTALL_GH`(609) | `Lit("install")`, `Lit("-y")`, `Lit("--formula")`, `Lit("gh")` — **4/4 Lit** | `Lit("install")`, `Lit("-n")`, `Lit("--formula")`, `Lit("gh")` — **4/4 Lit** |
| Claude | 1/1 | `RUN_NPM_INSTALL_CLAUDE`(634) | `Lit("install")`, `Lit("-g")`, `Lit("@anthropic-ai/claude-code")` — **3/3 Lit** | `None` |
| Wrangler | 1/2 | `RUN_PNPM_GLOBAL_ADD`(586) | `Lit("add")`, `Lit("-g")`, `Lit(WRANGLER_PACKAGE)` — **3/3 Lit** (`WRANGLER_PACKAGE`는 585행 `&'static str` 상수, 리터럴) | `None` |
| Wrangler | 2/2 | `RUN_NPM_GLOBAL_INSTALL_WRANGLER`(593) | `Lit("install")`, `Lit("-g")`, `Lit(WRANGLER_PACKAGE)` — **3/3 Lit** | `None` |
| Node / Git / Pnpm | — | 후보 배열 `&[]`(1044) — RunPlan 0개 | 해당 없음 | 해당 없음 |

`Arg::Formula` / `Arg::Package` / `Arg::PackageLatest`는 **한 칸도 섞이지 않았다.** Critical 아님. 테스트 `resolve_install_plan_picks_run_with_expected_literal_argv`(3530)가 실행 argv 결과값을 리터럴 문자열로 고정해 뒷받침한다(실행 확인: 통과).

**다만 이 불변식을 강제하는 자동 수단이 없다** → 별도 지적 **M5**. 그리고 위반 시 실패 방향이 슬롯 배열에 따라 갈린다: `args` 위반은 `resolve_install_plan`의 `if let Ok(argv)`(1105)가 후보를 건너뛰어 Manual로 **fail-closed**되지만, `preview_args`에만 위반이 생기면 Run이 반환된 뒤 `build_install_preview`가 2094행 `.expect()`로 **panic**한다.

### (b) `perform_install` → `perform_update` 위임의 안전성 ✅ **안전. 직전 #3은 소멸했다.**

| 점검 항목 | 판정 | 근거 |
|---|---|---|
| 무한 재귀 | **없음** | `perform_update`(2205~2326) 전체를 읽어 `perform_install` 호출이 0건임을 확인. 위임은 단방향 1회 |
| 락 교착 | **없음** | 위임이 `EXECUTION_LOCK.try_lock()`(2361) **이전**인 2357행에서 일어난다. `std::sync::Mutex`는 재진입 불가라 순서가 반대였으면 항상 자기 자신과 충돌했을 것 — 2353~2356 주석이 이 이유를 명시 |
| plan_id 대조 일관성 | **일관됨** | 위임 대상 케이스(`resolve_tool_path`=Some)에서 프리뷰도 `perform_preview`(2170~2175)의 Some 분기 → `build_run_preview` → `compute_plan_id(runner, args, normalized_before)`를 쓴다. 위임 후 `perform_update`(2238)가 같은 식으로 재계산 → **일치**. `installed:false`(버전 조회 실패) 케이스에서는 양쪽 모두 `normalized_before = ""`로 같은 값이 나온다 |
| 권한 우회 | **없음** | 위임 후 경로는 `perform_update`의 기존 게이트를 그대로 탄다 — `resolve_plan` → `compute_action`의 Cellar/bin 쓰기권한 검사(908) 포함. 우회 분기 없음 |
| 의도치 않은 업그레이드 | **구조적으로 불가** | 프리뷰~실행 사이에 도구가 설치돼 위임 경로로 바뀌면, 설치용 plan_id는 `compute_plan_id(runner, install_argv, "")`이고 업데이트용은 `compute_plan_id(runner, update_argv, ver)`다. 3개 도구 전부 install/update argv의 첫 토큰부터 다르다(`install`↔`upgrade`, `wrangler`↔`wrangler@latest`, `…claude-code`↔`…claude-code@latest`) → **해시 충돌 불가** → 항상 "다시 미리보기를 요청해주세요" Err로 끝난다. fail-closed |

**직전 지적 #3(막다른 골목 영구 실패 루프) 판정: 소멸.** 원인이었던 "프리뷰는 update용 plan_id를 주고 실행은 install용으로 대조하는" 분기 이원화가 `resolve_install_plan` 단일 정본 + 위임으로 제거됐다. 우회가 아니라 원인 제거다. 재현 경로를 손으로 추적한 결과(`installed:false` + 경로 존재 → 버튼 "설치" → `previewDevToolUpdate` → path Some → `build_run_preview` → `installDevTool` → 2357행 위임 → plan_id 일치 → 실행) 한 번에 성공한다.

한 가지 부수 효과: 이 케이스에서 버튼 라벨은 "설치"(devTools.ts:373)인데 실제 실행 명령은 `brew upgrade …`다. 확인 패널이 `commandDisplay`로 실제 명령을 그대로 보여주므로 사용자가 속지는 않는다 → 🟡 m7.

### (c) Windows fail-closed 🟠 **"구조적"이라는 코드의 주장은 사실이 아니다. 다만 실제 게이트는 다른 곳에 있고 유효하다.**

세 층으로 나눠 답한다.

**1층 — 코드가 근거로 든 문장(997~1003행): 틀렸다.**
주석은 "후보가 전부 POSIX 절대경로(`BREW_CANDIDATES`/`NPM_CANDIDATES`/pnpm `path_candidates`)라서 Windows 빌드에서는 **구조적으로 전부 None**이 되어 Run으로 해석될 수 없다"고 단언한다. 역추적하면 그렇지 않다:
`resolve_runner_path`(925) → `resolve_binary_expand_home`(cli_launcher.rs:37) → `resolve_binary`(cli_launcher.rs:19~31). 절대경로 후보가 **전부 실패하면 bare-name PATH 폴백**이 돈다:
```rust
if Command::new(bare_name).arg("--version").output().is_ok() { return Some(bare_name.to_string()); }
```
Windows에 `pnpm.exe`가 PATH에 있으면 `Some("pnpm")`이 되고, `resolve_install_plan(Wrangler)`는 `Run`을 반환한다. 즉 **위임자의 가설대로 "런타임 해석 실패에 기댄 fail-closed"**이며, 그마저도 항상 실패하지는 않는다.

**2층 — 실제로 막고 있는 것: `check_dev_tools_blocking`(1888)의 플랫폼 게이트. 이건 진짜다.**
```rust
if !cfg!(target_os = "macos") { return windows_unsupported_dev_tools_status(); }
```
Windows에서는 6개 도구가 전부 `action_kind:"manual"` + "이 화면은 현재 macOS만 지원합니다"로 고정된다(1869~1885). 프론트는 `actionKind === 'run'`일 때만 `handleRequestPreview`/`handleUpdateAll`을 호출하므로(devTools.ts:372, 249, 292) **UI에서 설치 실행에 도달하는 경로가 없다.** `cfg!`는 컴파일 타임 상수라 분기 자체가 최적화로 사라진다 — 이 층은 구조적이라 부를 만하다.

**3층 — 커맨드 계층에는 게이트가 없다. 방어선이 UI 1겹이다.**
`grep -n 'cfg!(target_os\|cfg(target_os'` 전체 파일 결과 **1888행 1건뿐**. `preview_dev_tool_update`(2199) / `update_dev_tool`(2329) / `install_dev_tool`(2501) / `open_manual_instruction`(2518) 어디에도 플랫폼 분기가 없다. 이 커맨드들은 Tauri IPC로 노출돼 있고 `check_dev_tools`의 결과를 신뢰하지 않는다 — Windows에서 `install_dev_tool("wrangler", …)`이 직접 호출되면 `resolve_install_plan`이 Run을 줄 수 있고 실행까지 간다.
(실행이 성공하지는 않을 공산이 크다: `build_child_path_env`(1444)가 `:` 구분자와 POSIX 표준 디렉터리로 PATH를 통째로 덮어쓰므로 Windows에서는 쓸모없는 PATH가 되고 spawn/실행이 깨진다. 그러나 이건 **우연한 방어**이지 설계된 게이트가 아니다.)

**판정**: "Windows에서 설치 실행이 **결코** 일어나지 않는다"고는 말할 수 없다. 실사용 위험은 낮지만(화면 게이트가 유효), 코드가 자기 안전성 근거로 적어 둔 문장이 사실과 달라 다음 사람에게 "여긴 안전하니 손대도 된다"고 잘못 말한다 → **M3(🟠 Major)**. 직전 Major #4(주석-동작 불일치)와 **동종의 결함이 새 코드에서 재발**한 사례다.

### (d) brew dry-run 파서 분리 ✅ **분리됐고, 실측 출력을 정확히 파싱한다. env 의존도 없다.**

**분리 여부**: `parse_brew_dry_run_affected`(1548, update 전용 — `이름 old -> new` 형태만)와 `parse_brew_install_dry_run_affected`(1610, install 전용 — 헤더 기반)는 별개 함수이고 서로 호출하지 않는다. 호출부도 분리돼 있다: update 프리뷰는 `build_run_preview`(2027), install 프리뷰는 `build_install_preview`(2121). 입력 형식 오염 없음.

**위임자 실측 출력 직접 대입 검증** (파서 로직을 손으로 한 줄씩 돌림):

| 입력 줄 | `collecting` | 처리 | 결과 |
|---|---|---|---|
| `gnupg 2.5.20 is already installed but outdated…` | false(초기) | 1619행 `!collecting` → skip | — |
| `==> Would install 1 formula:` | → **true** | 1615~1617 헤더 인식 | — |
| `gnupg` | true | 1631 `name == target` → skip | 대상 제외 ✅ |
| `==> Would upgrade 4 dependencies for gnupg:` | → true | 헤더 | — |
| `p11-kit` / `libgcrypt` / `libksba` / `pinentry` | true | 수집 | 4건 ✅ |
| `==> Would upgrade 3 dependents of upgraded formula:` | → true | 헤더 | — |
| ``Disable this behaviour by setting `HOMEBREW_NO_…`.`` | true | 1622 백틱 + 1623 prefix 이중 필터 → skip | 산문 배제 ✅ |
| ``Hide these hints with `HOMEBREW_NO_ENV_HINTS=1`…`` | true | 동일 | 산문 배제 ✅ |
| `gpgme    2.1.2   -> 2.2.0` | true | 1628 `split_whitespace().next()` → `gpgme` | `->` 형태 처리 ✅ |
| `gpgmepp  2.1.0 -> 2.2.0` / `poppler  26.06.0 -> 26.09.0` | true | 동일 | 2건 ✅ |
| (블록 전체 반복) | — | 1634 중복 제거 | 반복 흡수 ✅ |

**최종 = `[p11-kit, libgcrypt, libksba, pinentry, gpgme, gpgmepp, poppler]` (7건).** 위임자가 지적한 주의점 ①`->` 형태 dependents ②산문 줄 삽입 ③블록 반복 **셋 다 정확히 처리한다.**

**테스트 픽스처가 이 출력을 쓰는가**: ✅ `parses_real_measured_brew_install_dry_run_output_with_dependents_fixture`(3088~3137)가 위임자 실측 출력을 **원문 그대로**(반복 블록 포함) 담고 기대값 7건을 고정한다. `cargo test --lib dev_tools::` 실행 결과 통과 확인.

**env 의존 여부**: ✅ **의존하지 않는다.** 파서는 헤더(`==>` + "Would install"/"Would upgrade") 기반이라 `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK`가 먹든 안 먹든 동일하게 정확하다. dependents 섹션이 사라진 경우도 `parses_brew_install_dry_run_output_without_dependents_section`(3143)이 별도로 고정한다. 515행 주석이 이 성질을 명시적으로 약속하고 있고, 코드가 그 약속을 지킨다. **지적 없음** — 위임자가 우려한 "env 적용 실패 시 조용히 틀린 목록"은 발생하지 않는다.

파서 관련 잔여 지적은 🟡 m3(차이를 못 보여주는 테스트)·m4(산문 필터가 블랙리스트)·m5(target을 프리뷰 argv가 아닌 실행 argv에서 뽑음) 셋뿐이며 모두 Minor다.

---

## 3. 직전 리뷰 Major #1~#4의 현재 상태

| # | 직전 지적 | 현재 상태 | 근거 |
|---|---|---|---|
| **#1** | 자식 PATH 빈 엔트리(=CWD) | ✅ **해결 + 회귀 테스트 있음** | `build_child_path_env`(1444) 1449~1454에 `if !s.is_empty()` 필터 + 이유 주석. 회귀 테스트 2개: `build_child_path_env_excludes_empty_entry_for_bare_name`(3238), `_for_none`(3244). 실행 확인: 둘 다 통과 |
| **#2** | brew 쓰기권한 검사 대상이 `<prefix>`라 Intel 맥에서 기능 전멸 | ✅ **update 경로 해결** / 🟠 **install 경로에는 검사 자체가 없음** | `compute_action`(878) 908행이 `<prefix>/Cellar`와 `<prefix>/bin`을 각각 검사하도록 수정됐고, 왜 두 곳인지(keg 해제는 되고 링크만 실패하는 부분 실패) 주석에 남았다. 테스트 `compute_action_checks_prefix_cellar_and_bin_not_prefix_itself`(3342) 통과. **그러나 신규 설치 경로 `resolve_install_plan`(1096~1115)에는 쓰기권한 검사가 0줄** — 같은 실수의 반복은 아니지만 같은 자리가 비었다 → **M2** |
| **#3** | `installed:false + actionKind:"run"` plan_id 영구 불일치 | ✅ **구조적으로 소멸** | 위 (b) 참조. 단일 정본 + install→update 위임. 다만 이를 지키는 회귀 테스트(3671)가 위임 유무를 실제로 구별하지 못한다 → 🟡 m1 |
| **#4** | "전체 업데이트"가 무확인 실행인데 주석은 "항상 확인"이라 단언 | ✅ **원래 지적은 해소** / 🟠 **동종 결함이 새 코드에 재발** | 프론트 주석(devTools.ts:4~11, 236~247)이 "버튼 클릭 자체를 일괄 동의로 보고 개별 확인 없이 순차 실행하되 previewReliable/affected 조건으로 제외한다"로 **정확하게** 고쳐졌다 — 더 이상 거짓 단언이 아니다. 그러나 (i) 스냅숏 997~1003의 Windows fail-closed 근거 주석이 사실과 다르고(**M3**), (ii) 배치의 실제 범위가 "업데이트"에서 "설치 포함"으로 넓어졌는데 라벨·주석은 여전히 "업데이트"만 말한다(**M1**) |

---

## 4. 지적 사항

심각도: 🔴 Critical / 🟠 Major / 🟡 Minor / ⚪ Nit / 🔵 Rethink

### 🔴 Critical — **없음**

### 🟠 Major

#### M1. "전체 업데이트" 버튼 한 번이 미설치 도구를 개별 확인 없이 **실설치**한다 (신규 — 이번 변경이 만들었다)
- **페르소나**: process-execution-security(실행 표면·동의 범위) / claimed-vs-verified(V4)
- **위치**: `src/views/devTools.ts:249` × 스냅숏 1905~1910
- **확인방법**: devTools.ts 236~287 정독 → `targets = state.devTools.items.filter((t) => t.actionKind === 'run')`. 스냅숏 1895~1921에서 미설치 gh/Claude/Wrangler가 `"run"`을 받는 것을 확인. `build_install_preview`(2136~2149)의 `preview_args: None` 분기가 `affected = vec![def.label]`(길이 1) + `preview_reliable: true`를 반환하는 것을 확인.
- **문제**: 변경 전에는 미설치 non-Wrangler 도구가 `actionKind:"none"`이라 배치 대상에서 자동 제외됐다. 이번 변경으로 gh·Claude·Wrangler가 **미설치 상태에서 `"run"`**이 되면서 `handleUpdateAll`의 targets에 그대로 들어온다. 배치의 두 가지 제외 조건을 뚫는다:
  - `!preview.previewReliable` → Claude/Wrangler는 `preview_args: None`이라 항상 `true`. 통과.
  - `preview.affected.length > 1` → 같은 분기에서 `affected`가 길이 1로 고정. 통과.
  결과적으로 **`npm install -g @anthropic-ai/claude-code`와 `pnpm add -g wrangler`가 개별 확인 화면을 거치지 않고 실행된다.** gh도 brew dry-run에 추가 항목이 없으면 동일하게 통과한다.
- **왜 Major인가**: 사용자가 누른 버튼은 "전체 업데이트"이고(devTools.ts:305), 백엔드·프론트 주석 전부가 일관되게 "업데이트"라고만 말한다. 동의한 범위(있는 것들을 최신화)와 실행 범위(없는 것을 새로 설치)가 어긋난다 — 이 위임의 리스크 범주("사용자 머신 비가역 변경") 정중앙이다. 설계 정본에도 이 상호작용 분석이 없다: `devtools-install-matrix.md` §6.2는 `none`→`manual` 전환만 다루고, 실제로 일어난 `none`→`run` 전환이 배치 경로에 미치는 영향을 언급하지 않는다.
- **Critical이 아닌 이유**: 실행되는 것이 공식 패키지 매니저의 멱등 설치 명령이고 sudo를 유발하지 않으며 되돌릴 수 있다. 화면 부제(devTools.ts:297)에 "설치/업데이트도 실제로 실행됩니다"가 상시 표기돼 동의가 전무하지는 않다.
- **개선안**: `handleUpdateAll`의 필터에 `&& t.installed` 추가(1줄) — 배치는 업데이트만, 신규 설치는 언제나 개별 확인. 라벨 의미와도 맞는다. (대안: 버튼을 "전체 설치/업데이트"로 바꾸고 미설치 항목은 확인 대기로 남긴다.)

#### M2. 설치 경로 쓰기권한 사전검사 부재 — 설계 §4.2의 2개 조항이 **기록 없이 사라졌다**
- **페르소나**: design-contract-auditor(D3 조용한 누락) / heterogeneous-machine-operator(O2 막다른 골목)
- **위치**: 스냅숏 1096~1115(`resolve_install_plan` — 검사 0줄). 대조군: 878~915(`compute_action`은 update 경로에서 검사함)
- **확인방법**: `grep -n "is_writable_by_current_user"` 전수 — 생산 코드 사용처는 **908행 한 곳뿐**(나머지는 정의 2건 + 테스트 3건). 설계 §4.2 표 3·4행과 대조.
- **문제**: 설계 §4.2는 두 행을 명시적으로 요구했다 — "gh 설치 + `<prefix>/Cellar` 또는 `<prefix>/bin` 쓰기 불가 → `Manual(NotWritable)`, prefix는 brew runner 경로의 조부모로 구한다", "Claude 설치 + `<npm prefix>/lib/node_modules` 쓰기 불가 → `Manual(NotWritable)`, 없는 경로면 `<prefix>/lib`를 보고 그마저 없으면 건너뛴다". **둘 다 구현되지 않았고, 설계 문서에도 코드 주석에도 "이 요구를 뒤집는다"는 기록이 없다.** 조용한 누락이다.
- **영향(이기종 머신 관점)**: Intel 맥 `/usr/local`이 root:wheel인 구성, npm prefix가 root 소유인 구성에서 화면은 "설치"(run) 버튼을 제시하고, 사용자가 프리뷰 확인까지 누른 **뒤에야** EACCES/`Permission denied`로 `Failed`가 난다. 앱이 "내가 할 수 있다"고 말한 뒤 못 하는 것이라 O2 위반. 직전 #2와 동일한 사용자 경험이 설치 국면에서 반복된다.
- **완화 요인**: `stdin(Stdio::null())`(1366) 덕에 sudo 프롬프트로 앱이 멈추지는 않고, `log_tail`에 원문이 실려 원인은 보인다. 그래서 Critical이 아니다.
- **개선안**: `resolve_install_plan`이 후보를 채택하기 직전에 runner_path 조부모 기준 대상 디렉터리의 쓰기권한을 확인하고 실패 시 `MANUAL_NOT_WRITABLE`로 강등. §4.2가 명시한 "없는 경로면 상위, 그마저 없으면 검사 생략" 규칙까지 그대로 옮긴다(`compute_action` 908행 패턴 재사용 가능).

#### M3. Windows fail-closed의 근거로 코드가 단언한 메커니즘이 사실과 다르고, 커맨드 계층에는 게이트가 없다
- **페르소나**: claimed-vs-verified(V4) / process-execution-security(S3·실행 표면)
- **위치**: 스냅숏 997~1003(주석) vs `src-tauri/src/cli_launcher.rs:19~31`(`resolve_binary` bare-name 폴백). 게이트 실재 위치: 스냅숏 1888. 게이트 부재 위치: 2199·2329·2501·2518
- **확인방법**: 주석 문장 인용 → `resolve_runner_path`(925) → `resolve_binary_expand_home`(cli_launcher.rs:37) → `resolve_binary`(cli_launcher.rs:27 `Command::new(bare_name).arg("--version").output().is_ok()`) 역추적. `grep -n 'cfg!(target_os\|cfg(target_os'` 전수 = 1건.
- **문제·판정**: 위 (c) 전문 참조. 요약 — 주장된 "구조적" fail-closed는 실재하지 않고, 실재하는 게이트(1888)는 화면 계층 1겹뿐이며 커맨드 계층은 무방비다.
- **개선안**: (a) 997~1003 주석에서 "구조적으로 전부 None" 문장을 **삭제**하고 실제 게이트(1888행)를 근거로 다시 쓴다. (b) `perform_install`/`perform_preview` 첫 줄에 `if !cfg!(target_os = "macos")` 조기 반환을 추가해 커맨드 계층에도 같은 게이트를 둔다(각 2줄, 계약 변경 없음 — `not_supported_result` 재사용).

#### M4. `open_manual_instruction`이 non-async인데 이번 변경으로 **하위 프로세스 spawn 경로가 새로 생겼다** (메인 스레드 블로킹 신규 회귀)
- **페르소나**: process-execution-security(S5 프로세스 수명) / claimed-vs-verified(V4 자기모순)
- **위치**: 스냅숏 2517~2536. 신규 호출은 2531행 `ResolvedRunners::resolve()`
- **확인방법**: `git diff -- src-tauri/src/dev_tools.rs` 해당 hunk 확인 — 변경 **전**은 `None => Some(install_manual_plan(tool))`(순수 테이블 조회, 프로세스 0개), **후**는 `ResolvedRunners::resolve()` 호출.
- **문제**: `ResolvedRunners::resolve()`(1060)는 brew/npm/pnpm 3종을 해석하고, 각 해석은 절대경로 후보가 전부 없을 때 `Command::new(bare).arg("--version").output()`을 **타임아웃 없이** 실행한다(직전 리뷰 지적 #12). 그런데 이 커맨드는 `#[tauri::command] pub fn`(async 아님, `spawn_blocking` 없음)이라 **메인 스레드에서 돈다**. 같은 파일 1964~1967행이 "Tauri v2에서 non-async 커맨드는 메인 스레드에서 돈다(P0 버그)"라고 스스로 적고 `check_dev_tools`를 async로 옮겼는데, 옆의 이 커맨드는 non-async인 채로 그 위험 호출을 새로 얻었다 — 파일이 자기 원칙을 자기가 깬다.
- **도달 경로**: manual 카드의 "터미널에서 실행" → `openManualInstruction(tool.id, false)`(devTools.ts:200). 미설치 pnpm/node/git 카드에서 매번 탄다. 최악의 경우 앱 UI가 응답 없이 멈춘다.
- **개선안**: `check_dev_tools`와 동일하게 `async` + `tauri::async_runtime::spawn_blocking`으로 옮긴다. 프론트 계약 불변(`invoke()`는 이미 Promise).

#### M5. §1.1 불변식을 강제하는 자동 가드가 없고, `preview_args` 위반은 fail-closed가 아니라 **panic**이다
- **페르소나**: design-contract-auditor(D2 강제 수단 / D4 실패 방향)
- **위치**: 스냅숏 1011~1046(테이블), 1105(`args` fail-closed 경로), 2092~2094(`preview_args` `.expect()`)
- **확인방법**: 위 (a) 전수 대조 + 테스트 어서션 본문 확인 — `install_candidates_match_new_run_manual_policy`(3472)는 후보의 **유무만**, `resolve_install_plan_picks_run_with_expected_literal_argv`(3530)는 실행 argv **결과값만** 본다. `preview_args`의 슬롯 종류를 검사하는 테스트는 47개 중 0개.
- **문제**: 현재 상태는 불변식을 지킨다(위반 0건). 문제는 **강제 수단이 "사람의 주의력"뿐**이라는 것이다. 이 파일 전체가 설계상 인젝션 방어를 이 불변식 하나에 걸어 뒀는데(§1.1 "인젝션 표면이 구조적으로 존재하지 않는다"), 정작 그 불변식만 검증되지 않는다. 게다가 실패 방향이 비대칭이다: `args`에 비-Lit이 섞이면 1105행이 후보를 건너뛰어 Manual로 안전하게 떨어지지만, `preview_args`에만 섞이면 `resolve_install_plan`은 Run을 반환하고 `build_install_preview`가 `.expect("설치 프리뷰 인자는 전부 리터럴이라 실패할 수 없습니다(§1.1)")`로 **패닉**한다. 위임자가 이 불변식 위반을 Critical로 규정한 만큼, 그 위반을 잡는 장치가 없는 것 자체가 Major다.
- **개선안**: (a) `install_candidates()`의 모든 도구를 순회하며 `plan.args`와 `plan.preview_args`의 모든 원소가 `matches!(a, Arg::Lit(_))`인지 단언하는 테스트 1개 추가(≈10줄). (b) 2094행 `.expect()`를 `Manual(install_manual_plan(tool))` 강등으로 바꿔 실패 방향을 `args` 쪽과 통일한다.

### 🟡 Minor

| # | 지적 | 위치 / 확인방법 | 개선안 |
|---|---|---|---|
| m1 | `perform_install_delegates_to_update_when_already_installed`가 **위임을 검증하지 못한다** — bogus plan_id는 위임 유무와 무관하게 Err라, 위임 2줄을 지워도 통과한다. 테스트 주석 스스로 인정(3674~3676). 이름이 어서션보다 많이 주장한다 | 스냅숏 3671~3682 / 어서션 본문 정독 + 두 경로의 Err 발생 조건 대조 | 위임 시에만 성립하는 값을 대조(예: update용 plan_id로 `install_dev_tool` 호출이 **성공 경로**에 드는지), 또는 `resolve_tool_path`를 주입 가능하게 리팩터 |
| m2 | `check_dev_tools_blocking_follows_run_manual_policy_and_never_returns_none`의 핵심 분기(`if tool.path.is_none()`)가 **이 머신에서 한 번도 실행되지 않는다**(6개 도구 전부 설치됨 — 위임서 명시). 실제 검증되는 건 `action_kind != "none"` 뿐 | 스냅숏 3611~3636 / 어서션 본문 + 머신 전제 대조 | 정책 검증 부분을 `install_candidates` 기반 순수 함수 테스트로 분리(3530이 이미 머신 독립적으로 담당하므로 중복 제거 겸) |
| m3 | `install_dry_run_parser_differs_from_update_parser_on_no_arrow_lines`가 두 파서 **둘 다 빈 배열**이라고 단언한다 — 주석이 말하는 "차이"를 어서션이 보여주지 않는다. `parse_brew_install_dry_run_affected`를 `\|_,_\| vec![]`로 바꿔도 통과 | 스냅숏 3160~3167 | target과 다른 이름이 섞인 샘플을 써서 실제로 결과가 갈리게 한다 |
| m4 | `parse_brew_install_dry_run_affected`의 산문 필터가 화이트리스트가 아니라 **블랙리스트**(백틱 포함 / "Disable this behaviour" / "Hide these hints"). 수집 구간에 백틱 없는 다른 산문(`Warning:`, `Error:` 등)이 오면 첫 토큰이 이름으로 수집된다 | 스냅숏 1622~1627 | 이름 후보를 `validate_argv_token` 유사 문자클래스로 화이트리스트 검사. **다만 오탐 방향이 안전하다** — 쓰레기 항목이 늘면 `affected.length > 1`로 배치에서 제외되는 쪽으로 작동한다 |
| m5 | dry-run 파서의 `target`을 **실행 argv**의 마지막 토큰에서 뽑는다 — 프리뷰 argv가 아니다. 현재 두 배열의 마지막 토큰이 우연히 같아서 맞는다(2117~2119 주석이 자인) | 스냅숏 2120 | `preview_argv.last()`를 쓴다(1줄) |
| m6 | **설계 정본 내부 모순**: §4.2 마지막 행("설치 직전 이미 설치됨 → **실행하지 않고** `AlreadyLatest`")과 §6.3 요구 2("update로 위임")가 서로 다른 것을 지시한다. 구현은 §6.3을 택했고 그 선택이 문서에 기록되지 않았다 | `devtools-install-matrix.md` §4.2 표 5행 vs §6.3 요구 2 / 스냅숏 2357 | 실질 피해는 없다(위 (b)대로 plan_id 불일치 Err로 끝난다). 설계 문서에 어느 쪽이 정본인지 한 줄 추가 |
| m7 | `installed:false` + 경로 존재 케이스에서 버튼 라벨은 "설치"인데 실제 명령은 `brew upgrade …`다 | devTools.ts:373 / 스냅숏 2357 | 확인 패널이 `commandDisplay`로 실제 명령을 보여주므로 오인 위험은 낮다. 라벨을 `tool.path ? '복구/업데이트' : '설치'`로 세분하면 정확해진다 |
| m8 | Windows에서 manual 카드의 "터미널에서 실행"이 `osascript` spawn 실패로 기술적 에러 문자열(`터미널을 여는 데 실패했습니다: …`)을 뱉는다. 설계 §5.1이 권고한 `TerminalLaunchResult{opened:false, message:"PowerShell에 붙여넣어…"}`는 미구현 | `cli_launcher.rs:66~76` / 스냅숏 2548 | §5.2를 택했으므로 화면 안내 자체는 거짓이 아니다. `#[cfg(windows)]` 분기로 안내 문구를 돌려주면 2줄 |

### ⚪ Nit

- **n1**. `RUN_PNPM_GLOBAL_ADD`가 `Arg::Lit(WRANGLER_PACKAGE)` 형태로 상수를 참조한다 — `&'static str` 리터럴 상수라 §1.1 불변식 준수. **문제 없음**(전수 확인했다는 기록용으로 남긴다).
- **n2**. `install_manual_plan(Claude)`의 `doc_url`이 `https://docs.claude.com/en/docs/claude-code/setup`(스냅숏 800)인데 설계 §2 표는 `https://code.claude.com/docs/en/setup`을 적었다. 어느 쪽이 현행인지 **확인 못 했다** — 빠르게 변하는 외부 서비스의 사실이라 기억으로 판정하지 않는다. **확인 필요**로 남긴다(둘 중 하나가 리다이렉트일 가능성이 높으나 미확인).

### 🔵 Rethink (발산형 — RAG 판정에 영향 없음)

#### R1. 1급 개념을 "도구 6개 × 설치/업데이트"에서 **"이 머신이 조직 표준 개발환경에 부합하는가"**로 바꾼다

| 축 | 현재 구조 | 제안 구조 |
|---|---|---|
| 1급 개념 | 도구 6개 각각의 상태 카드 | 표준 환경 체크리스트 1개 + 미충족 항목별 "해결 방법" 카드 |
| 앱이 실행하는 것 | 도구별 설치/업데이트 명령을 조용히 spawn (실행 엔진 보유) | **부트스트랩 명령 하나를 사용자가 보는 터미널 창에서 연다**(`github_integration.rs`의 `gh auth login` 터미널 위임 선례와 동일 철학) |
| 승인 대상 | plan_id로 붙잡은 argv | 터미널 창에 실제로 찍히는 명령과 출력 |
| 코드 규모 | 게이트 G1~G4 + INSTALL/UPDATE 테이블 + dry-run 파서 2종 + 락·타임아웃·프로세스그룹 kill·plan_id 해시 (≈900줄) | 표준 목록 테이블 + 터미널 위임 1함수 |

**왜 더 나은가**: 설계 §2가 채운 10칸 중 앱이 실제로 실행하는 것은 **2칸(+Wrangler)뿐**이고 나머지 8칸은 전부 "안내"다. 그 2칸을 위해 실행 엔진 전체를 유지하고, 그 엔진이 이번 라운드 Major 5건 중 4건(M1·M2·M3·M4)의 발생지다. 사내 40인 규모 도구(사용자 메모리: "엄격도는 규모에 비례해서만", "MVP는 며칠 안에")에서 이 복잡도는 **가치의 원천이 아니라 "앱이 대신 실행한다"는 선택의 부산물**이다. 터미널 위임으로 통일하면 verified 판정의 상당 부분이 불필요해진다 — 사용자가 결과를 직접 보기 때문이다.
**포기하는 것**: 앱 안의 진행률 표시, 결과 패널, `verified` 3상태 판정, "전체 업데이트" 일괄 실행.
**감당**: 터미널 창이 그 역할을 대신한다. 화면 새로고침(`↻ 다시 확인`)으로 사후 상태를 확인하는 흐름은 그대로 유지된다. 이 코드베이스에 이미 같은 철학의 선례가 둘(`gh auth login`, `wrangler login`) 있어 정합적이다.
**리스크**: 이미 구현된 900줄을 버리는 매몰비용. 그래서 이건 "지금 되돌려라"가 아니라 **다음에 이 화면을 손댈 때의 방향 제안**이다.

#### R2. plan_id가 붙잡는 것을 **argv에서 "영향 목록"으로** 바꾼다

**현재**: `compute_plan_id(runner_path, argv, normalized_before)` — argv를 붙잡는다. 설계 부록 A가 "plan_id는 argv를 붙잡지 영향 목록을 붙잡지 않는다"를 한계로 자인했고, 1차 리뷰 #14도 같은 것을 지적했다.
**문제의 재정의**: 이번 변경으로 사용자가 실제로 감수하는 것은 "brew가 gh와 **함께** 무엇을 올릴 것인가"다(§3.4 실측: gnupg 설치 하나에 7개가 딸려 올라간다). 그건 argv가 아니라 **dry-run 출력 목록**이다. 지금은 프리뷰 이후 brew 인덱스가 갱신돼 영향 범위가 완전히 달라져도 argv는 그대로라 plan_id가 일치하고, **사용자가 본 것과 다른 것이 설치된다.**
**제안**: plan_id 해시 입력에 `affected`의 정렬 목록을 포함시킨다. 범위가 달라지면 plan_id가 어긋나 자동으로 재확인을 요구한다.
**포기하는 것**: 재확인 빈도 증가(brew 인덱스가 자주 바뀌면 체감된다).
**감당**: 재확인은 dry-run 1회(이미 120초 예산이 잡혀 있다)로 끝난다. 지금은 이 사고를 **아무것도** 잡지 못한다.
**비용**: R1을 택하지 않는다면 이건 저비용 개선이다 — 해시 입력에 1줄 추가 + 프리뷰/실행 양쪽에서 같은 값을 재계산하는 경로 확보. (다만 실행 직전 dry-run을 한 번 더 돌려야 하므로 실제로는 "1줄"보다 크다 — **미확인 추정치**: 실행 경로에 프리뷰 재실행 1회를 넣는 구조 변경이 필요하다.)

---

## 5. 페르소나 간 충돌 / 트레이드오프

| 충돌 지점 | A 입장 | B 입장 | 권고 |
|---|---|---|---|
| M1 배치 설치 | **process-execution-security**: 비가역 실행이 개별 확인 없이 일어나는 것은 그 자체로 결함 | **heterogeneous-machine-operator**: 신규 머신 셋업에서 6개를 하나씩 확인하는 것이야말로 이 앱의 존재 이유다. 배치가 편의의 핵심 | **B의 목적을 A의 방식으로** — 배치를 없애지 말고 `&& t.installed`로 좁힌 뒤, 미설치 항목은 "N개를 설치합니다" **요약 확인 1회**로 묶어 편의를 유지한다. 확인 0회와 6회 사이에 1회가 있다 |
| M2 쓰기권한 사전검사 | **design-contract-auditor**: 설계 조항이므로 구현하거나 명시적으로 기각하거나 둘 중 하나 | **zero-base-redesigner**: 검사를 또 늘리는 것은 R1이 지적한 복잡도 누적 그 자체. 어차피 exit code로 드러난다 | **A 채택하되 최소 형태로** — 기존 `is_writable_by_current_user`를 그대로 재사용하는 2~4줄이면 되고 새 개념이 늘지 않는다. 사용자에게 "누르기 전에" 말해주는 값어치가 이 비용보다 크다 |
| M3 커맨드 계층 게이트 | **claimed-vs-verified**: 주석만 고치면 된다(코드는 이미 화면 게이트로 막혀 있다) | **process-execution-security**: 방어선이 UI 1겹인 것 자체가 결함. IPC 커맨드는 화면을 신뢰하면 안 된다 | **둘 다** — 주석 수정은 필수(거짓 근거 제거), 커맨드 게이트는 각 2줄이라 비용이 사실상 0이다. 굳이 고르지 않는다 |
| R1 터미널 위임 전면 전환 | **zero-base-redesigner**: 900줄이 2칸을 위해 존재한다 | **claimed-vs-verified**: 터미널 위임은 `verified` 판정을 포기하는 것이고, 그건 이 프로젝트가 의식적으로 얻어낸 자산이다(결정 4) | **지금은 전환하지 않는다.** 이미 구현됐고 47개 테스트가 붙어 있다. R1은 다음에 이 화면을 크게 손댈 때의 방향으로 보존한다 |

---

## 6. 잘 된 점

1. **직전 Major #1이 정확히 닫혔다** — 필터(1449~1454) + 이유 주석 + 회귀 테스트 2개(3238·3244). 지적 → 수정 → 회귀 테스트의 완결된 사이클.
2. **직전 #2가 `<prefix>` → `<prefix>/Cellar` + `<prefix>/bin`으로 고쳐졌고**, "왜 두 곳을 다 보는가"(keg 해제는 성공하고 링크만 실패하는 부분 실패)까지 주석에 남았다(890~906). 수정만 하고 이유를 안 남기는 흔한 패턴을 피했다.
3. **직전 #3을 우회가 아니라 원인 제거로 닫았다** — `resolve_install_plan` 단일 정본으로 세 호출부의 분기 이원화 자체를 없앴다. "잘못된 상태를 표현 불가능하게"라는 원 설계 정신에 부합한다.
4. **Wrangler 하드코딩(`if def.id == ToolId::Wrangler`)이 규칙으로 승격돼 사라졌다** — 다음 도구가 추가될 때 판단 기준이 코드(G1~G4 게이트 표)에 남는다.
5. **brew install dry-run 파서가 위임자 실측 출력을 원문 그대로 픽스처로 고정했다**(3089~3117, 반복 블록까지). dependents `->` 섹션·산문 줄·반복·target 제외 4가지를 모두 통과하고, 설계 §3.4의 부정확한 단언("install dry-run에 `->`가 안 나온다")을 **코드 주석이 명시적으로 교정**해 뒀다(1567~1571). 설계서가 틀렸을 때 구현이 조용히 따라가지 않고 실측으로 바로잡은 사례다.
6. **설계 문서가 gitignore 대상이라 사라질 것을 알고 판정 근거를 코드 주석으로 보존했다**(968~1003의 G1~G4 게이트 표 + 도구별 판정 한 줄). 문서가 사라져도 "왜 gh는 run이고 node는 manual인가"가 코드에 남는다.
7. **git 스텁 가드가 프로세스를 하나도 띄우지 않는 순수 파일 검사다**(1123~1127) — `/usr/bin/git` 경로 일치 + CLT/Xcode 디렉터리 존재 여부만 본다. 호출 순서도 `check_tool_version` **이전**(1926)이라 GUI 대화상자 유발 경로가 실제로 차단된다. 게다가 `path_candidates` 재배치(93~98)로 정상 머신에서는 스텁에 도달조차 하지 않는다 — 이중 방어.
8. **Windows 분기를 순수 함수로 분리해 macOS에서도 테스트 가능하게 했다**(`windows_unsupported_dev_tools_status`, 1869 / 테스트 3642). 플랫폼 분기를 검증 불가 영역으로 방치하지 않았다.
9. **`BREW_INSTALL_ENV` 주석(509~515)이 "파서는 이 env가 먹히는지에 의존하지 않는다"를 약속하고, 코드가 그 약속을 실제로 지킨다** — 위임자가 우려한 실패 모드가 설계 단계에서 이미 차단됐다.
10. **프론트 `none` 분기가 지시대로 방어적으로 보존됐다**(devTools.ts:381) — 백엔드에 생산 경로가 없어졌는데도 제거하지 않았다.
11. **47개 테스트 전부 통과**(실행 확인: `cargo test --lib dev_tools::` → 47 passed, 0 failed).

---

## 7. 기각·강등한 지적

| 원 지적 | 처리 | 사유 |
|---|---|---|
| "`build_install_preview`의 `.expect()`(2094)가 패닉을 일으킬 수 있다 → 🔴 Critical" | **🟠로 강등**(M5에 흡수) | 현재 테이블이 불변식을 지키고 있어 **재현 경로가 없다**. 재현 경로를 못 대는 🔴는 강등한다는 원칙 적용. 잠재 결함으로는 유효해 M5에 남겼다 |
| "`Arg::Package`가 `#[allow(dead_code)]`로 남아 있어 죽은 코드다" | **기각** | 3015행 테스트가 이 슬롯의 동작을 실제로 검증하고, 주석(420~426)이 유지 사유(결정 2의 슬롯 구조 완결성)를 명시한다. 정당한 유지다 |
| "`brew install` 프리뷰에 `HOMEBREW_NO_AUTO_UPDATE`가 없어 120초 예산을 넘길 수 있다" | **기각** | 원 설계 부록 B.4가 의도적 결정으로 명시했고(끄면 오래된 인덱스로 거짓 판정) 주석 496~498이 그 이유를 적었다. 타임아웃 시 `preview_reliable:false`로 fail-closed되므로 실패 방향도 안전하다 |
| "`is_git_stub_without_clt`가 `/Applications/Xcode.app`을 하드코딩해 다른 위치의 Xcode를 놓친다" | **기각** | 놓치면 가드가 **더 자주** 트립해 `manual`로 강등되는 쪽이라 fail-safe 방향이다. 지적 가치 없음 |
| "`describe_install_method` 문자열이 프론트에 그대로 노출돼 계약이다" | **기각** | `install_method`는 표시 전용 `String`이고 프론트가 분기에 쓰지 않는다(devTools.ts에서 조건 분기 사용처 0건 확인). 계약 리스크 아님 |

---

## 8. 검증 범위와 정직 보고 — **하지 않은 것**

- **화면 캡처 없음.** 이 리뷰는 백엔드 로직·설계 대조 리뷰이고, 위임 범위가 `dev_tools.rs` 변경분 하나로 못박혔다. 프론트는 계약 확인 목적(`devTools.ts`·`devToolsApi.ts` 읽기)으로만 열었다 — 허용된 범위 그대로다. **UI 렌더 결과는 검증하지 않았다.**
  - 단, M1(배치 설치)과 m7(버튼 라벨)은 화면 동작에 대한 지적이므로 **코드 경로 추적에 근거한 판정**이며 실제 렌더로 확인하지 않았다. 후속 Step에서 이 화면을 UI 리뷰할 때 재확인이 필요하다.
- **설치 명령을 실행하지 않았다.** 이 머신에 6개 도구가 전부 설치돼 있어 실행 검증이 불가능하고 금지 범위다. `brew install -n` 조차 돌리지 않았다 — 위임자가 제공한 실측 출력을 파서에 **손으로 대입**해 검증했다((d) 표).
- **Windows 동작 전부 미검증.** macOS 머신이다. (c)의 판정은 코드 경로 추적과 `resolve_binary` 구현 독해에 근거한 **추론**이다. "Windows에서 `Command::new("pnpm")`이 `pnpm.exe`를 찾는다"는 것도 Rust `std::process::Command`의 Windows 동작에 대한 **미검증 추정**이다 — 실제로는 `.cmd` 확장자 처리 때문에 실패할 수도 있다. 그러나 **M3의 논지는 이 추정에 의존하지 않는다**: 주석이 "구조적으로 불가능"이라 단언했는데 폴백 경로가 존재한다는 사실 자체가 지적이고, 그건 코드로 확정된다.
- **Intel 맥 미검증.** M2의 영향 서술(`/usr/local`이 root:wheel)은 직전 리뷰와 같은 이유로 **미검증 추정**이다.
- **범위 밖 파일 미리뷰.** `mcp_manager.rs`·`autonomy/`·`config/`·`cloudflare_integration.rs` 및 프론트 나머지는 다른 세션 작업분이라 지시대로 열지 않았다.
- **n2(doc_url)는 확인 못 했다** — 외부 서비스 URL이라 기억으로 판정하지 않고 "확인 필요"로 남겼다.
- **실행 액션 없음.** 코드를 한 줄도 수정하지 않았고, `git add`/`commit`/`push`를 하지 않았다. 설치 명령을 실행하지 않았다. 실행한 것은 읽기 전용 조회(`ls`, `grep`, `diff`, `md5`, `git diff`)와 **단위 테스트 1회**(`cargo test --lib dev_tools::` — `/bin/echo`·`/bin/sleep`·`/bin/cat`·각 도구의 `--version`만 spawn하며, 설치/업데이트 명령은 stale plan_id 거부로 실행 전에 차단됨을 코드로 확인한 뒤 실행)뿐이다. 이 리뷰는 **사람 승인의 전제조건**이며, 승인 자체는 PM이 받아야 한다.

---

## 9. PM 권고

**승인 전 조치(🟠 5건) — 우선순위 순**

1. **M1** `handleUpdateAll` 필터에 `&& t.installed` 추가 (1줄). 가장 값싸고 가장 중요하다 — 사용자 머신 비가역 변경의 동의 범위 문제다.
2. **M2** `resolve_install_plan`에 쓰기권한 사전검사 추가 (2~4줄, 기존 `is_writable_by_current_user` 재사용). 설계 조항 복원.
3. **M3** 997~1003 주석의 거짓 근거 삭제 + `perform_install`/`perform_preview`에 플랫폼 게이트 추가 (각 2줄).
4. **M4** `open_manual_instruction`을 `async` + `spawn_blocking`으로 (3줄).
5. **M5** 불변식 전수 테스트 1개 추가 + 2094행 `.expect()` → Manual 강등 (≈12줄).

다섯 건 모두 **국소 수정**이고 구조 변경이 아니다 — 합계 25줄 안팎으로 추정한다(관련 유틸 `is_writable_by_current_user`·`not_supported_result`·`install_manual_plan`이 이미 존재함을 확인한 근거 위의 추정치다. 테스트 추가분 제외).

**승인 후 백로그**
- 🟡 m1~m3(테스트 3건이 이름만큼 검증하지 못함) — 저비용, 테스트 파일만 손댐.
- 🟡 m4·m5 — 파서 견고성, 각 1~3줄.
- 🟡 m6 — 설계 문서 §4.2/§6.3 모순 해소 1줄.
- ⚪ n2 — Claude Code 공식 문서 URL 확인.
- 🔵 R2 — plan_id에 영향 목록 포함. **미확인 추정치**: 실행 경로에 dry-run 재실행 1회를 넣는 구조 변경이 필요해 "1줄"이 아니다. 착수 전 실측 필요.
- 🔵 R1 — 다음에 이 화면을 크게 손댈 때의 방향으로만 보존. 지금 착수 권고하지 않는다.

**재호출 필요 지점**: 이 리뷰는 화면을 렌더해 보지 않았다. M1 수정 후 "전체 업데이트"의 실제 동작과 확인 흐름은 **UI 리뷰로 별도 확인**이 필요하다 — 그 Step이 생략되면 M1 수정이 실제로 사용자에게 어떻게 보이는지 아무도 확인하지 않은 채 완료된다.
