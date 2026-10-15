# R22 macOS Node 사용자 공간 설치 — 2차 재검토 보고서

리뷰 대상: target_id `r22-node-userspace-install` / 반영 범위 `d80a3a8..eec87ec` (cc710aa RV-001, 8e57e1b RV-003, ba758d3 RV-004, b0e0282 RV-005, a925b32 N-1, eec87ec RV-002) / 누적 제품 범위 `8dd994f..eec87ec` / 워크트리 `.claude/worktrees/r22-node`, 브랜치 `feat/r22-node-userspace`
리뷰 페르소나 패널(6, 전원 재사용): `persona-process-execution-security.md`, `persona-claimed-vs-verified.md`, `persona-heterogeneous-machine-operator.md`, `persona-release-artifact-trust-chain.md`, `persona-install-atomicity-and-archive-escape.md`, `persona-zero-base-redesigner.md`(발산형)
리스크 범주: 외부 바이너리 다운로드·무결성 검증·사용자 공간 설치 + 자식 프로세스 환경 주입(공급망/실행 경계)
등급/모드: Sensitive — 동일 대상 재검토, **증분(B)**. 판정 근거는 아래 "모드 판정"에 있다.
직전 리뷰: `docs/reviewer/review-r22-node-userspace-2026-10-15.md` (1차, 풀패널, Green)
리뷰 일자: 2026-10-15
종합 판정: 🟢 Green (Critical 0 · Major 0 · Minor 2 · Nit 5 · Rethink 1 신규)

## 요약 (2분 규칙)
1차 지적 RV-001~005와 N-1은 모두 코드와 테스트에서 해소됐다. 반영 커밋 HEAD(eec87ec)에서 `cargo test --lib`을 직접 돌려 1122개 통과, 실패 0을 확인했다(1차 대비 +19). RV-002의 `npm_config_prefix` 주입은 claude 실행 경로가 앱 `npm-global/` 아래일 때만 걸린다. Homebrew·`/usr/local`·네이티브(`~/.local/bin`)·`~/.npm-global` claude와 Windows에는 아무것도 주입하지 않는다. 앱 npm-global 후보는 4개 후보 중 마지막이라, 앱 설치본만 있는 기기에서만 이 경로가 선택된다. 새 Critical·Major는 없다.

남은 것은 Minor 2건이다.
- RV-006: 주입한 env가 claude 세션의 손자 프로세스(Bash 도구·MCP·hook)까지 내려간다. 그래서 세션 안의 `npm i -g`도 앱 npm-global로 떨어지고, nvm이 함께 깔린 기기에서는 `nvm use`가 거부될 수 있다(외부 도구 동작은 기억 기반이라 확인이 필요하다).
- RV-007: RV-002의 전제("claude 자기 갱신은 npm -g를 부른다")와 RV-005의 실물 양성 경로(진짜 node로 `npm-cli.js --version` 실행)는 아직 실측되지 않았다.

## 모드 판정 (재검토 3요소 대조)
| 단계 | 판정 | 근거 |
|---|---|---|
| 재검토 3요소 | 모두 있음 | target_id·직전 경로·리스크 범주가 위임에 명시돼 있다 |
| 풀패널 강제 승격 조건 | 해당 없음 | 직전 Critical·Major는 0건이다. 새 표면은 "claude 자식 env" 하나다(호출 지점은 6곳이지만 같은 빌더를 쓰는 단일 표면이라 "다수"가 아니다). 다른 Sensitive 하위도메인 진입은 없다. 2차 재검토다. 명시적 풀패널 요청은 없다. |
| 동일 대상 인정 4조건 | 충족 | 같은 target_id다. 직전 리뷰와 같은 날이라 7일 이내다. 범주는 직전의 "공급망/실행 경계"를 그대로 두고 env 주입을 덧붙인 것이다. env 주입은 직전에도 불변식 4로 다룬 축이라 도메인 변경이 아니다. 파일도 실질적으로 겹친다(app_node.rs·node_install/* 중심). |
| 모드 | **증분(B)** | 새 리스크 표면(claude 자식 env)이 있어 축소(C)가 아니라 증분을 택했다. 발산형을 포함해 직전 패널 6명을 그대로 투입했다. |

## 페르소나 재사용 판정 (산출물 게이트)
착수 전에 INDEX.md(23행)의 역할개념 열을 대조했다. 신규 페르소나는 없다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| process-execution-security | 재사용 | RV-002가 claude 자식 6개 지점의 env를 바꾼다. 역할개념(argv·env·PATH) 그대로다. |
| claimed-vs-verified | 재사용 | 반영 커밋의 "해소" 주장과 실측 여부를 대조한다. |
| heterogeneous-machine-operator | 재사용 | Homebrew·기존 Node·네이티브 claude·Windows 조합에서 주입이 무해한지 본다. |
| release-artifact-trust-chain | 재사용 | RV-003(요구식의 실물 codesign 음성 검증)을 본다. |
| install-atomicity-and-archive-escape | 재사용 | RV-005(멱등 판정)와 N-1(임시 링크 쓸기)을 본다. |
| zero-base-redesigner (발산) | 재사용 | "env를 자식마다 붙이는 구조가 옳은가"를 묻는다. |

`persona-delegated-authority-blast-radius.md`도 후보로 봤지만 제외했다. 주입하는 값이 권한(토큰·승인 범위)이 아니라 설치 위치 설정이라, 역할개념과 맞지 않는다.

## 직접 실행한 확인
| 항목 | 결과 |
|---|---|
| `cargo test --lib` (워크트리 `src-tauri/target` 재사용, `CARGO_TARGET_DIR` 미설정 확인, HEAD eec87ec) | **1122 passed, 0 failed, 24 ignored** (8.2초) |
| `cargo test --lib tests_codesign` | 8/8 통과. 무시된 테스트 없음. 실제 `/usr/bin/codesign`을 호출한다. |
| `cargo test --lib app_node::tests` | 20/20 통과(RV-001·002·004·005 신규 9종 포함) |
| `cargo build --lib` 경고 | 0건 |
| 테스트 전후 실제 홈 `~/.local/share/malgn-agent` | 실행 전후 모두 없음(No such file) |
| 테스트 후 `$TMPDIR` 안 `r22-*` 잔여 | 0개. codesign·app_node 테스트의 임시 폴더는 모두 지워졌다. |
| `lipo -detailed_info /bin/ls` | x86_64 16384+48128, arm64e 65536+89088. N-8 판단 근거. |
| Windows 교차 컴파일 | **하지 않음**. 1차에서 ring C 헤더 부재로 이 환경에서는 불가함을 확인했다. CI windows-latest가 대체 수단이다. |
| 네트워크·#[ignore]·nodedl 폴더·실제 rc/npm 전역 | 위임 지시대로 하지 않았다 |

## RV별 해소 판정
| ID | 판정 | 근거(코드·테스트) |
|---|---|---|
| RV-001 취소 플래그 고착 | ✅ 해소 | `app_node.rs:52-90`: 불리언을 세대 카운터 `CancelSwitch`로 바꿨다. 토큰은 시작 시점의 세대를 기억하고, `cancel()`은 증가만 한다. 리셋 코드가 필요 없다. `resolve_target_tag`(:206)와 `run_pinned`(:313)가 각자 토큰을 받는다. 테스트 4종이 있다. "진행 중 작업만 취소", "작업 없을 때 무해", "취소 뒤 버전 조회가 취소로 보고되지 않음"(DeadEnv가 이제 취소 신호를 먼저 보도록 바뀌어 회귀를 실제로 잡는다), "전역 `request_cancel` 뒤 설치 비오염". 주석 3곳(`mod.rs:360-363` 등)도 실제 동작과 맞다. 남은 한계(두 단계 사이의 틈)는 주석 `:84-87`에 정직하게 적혀 있다(N-11). |
| RV-002 claude 자기 갱신 위치 | ✅ 해소(코드) / ⚠ 전제 미실측(RV-007) | `app_node.rs:164-175` `claude_child_env_for`는 `layout.contains_npm_global_path`(`layout.rs:80`, 성분 단위 `starts_with` + 기준 폴더 canonical 비교)일 때만 `npm_config_prefix=<npm-global>`을 준다. 적용 지점 6곳을 grep으로 전수 대조했다. autonomy/runner.rs:125, session_chat/mod.rs:137·209 → turn.rs `command.envs`, claude_auth.rs:384·464·612, plugins/mod.rs:286, mcp_manager/process.rs:59. `Command::new`·`run_process_with_timeout*` 전수 grep에서 빠진 claude 실행 지점은 없다(dev_tools 내부의 `claude --version` 조회만 제외됐고, 이는 갱신을 일으키지 않는다). 음성 표 테스트(`/opt/homebrew`, `/usr/local`, `~/.local/bin`, `~/.npm-global`, 앱 `node/current/bin`, `npm-global-evil`, 맨 이름, 빈 문자열, 홈 없음)와 실제 자식 프로세스 env 도달 테스트, src 전체 스캔 가드가 있다. |
| RV-003 요구식 실물 음성 검증 | ✅ 해소 | `tests_codesign.rs`(macOS 한정, 비-ignore) 8종이 실제 codesign을 쓴다. 요구식 문자열을 `developer_id_requirement()`(`mod.rs:146`)로 추출하고 리터럴을 고정 단언한다. `/bin/ls`(Apple 플랫폼)와 ad-hoc 사본은 **종료 코드 3 + "failed to satisfy"**로 거부되고, 구문 오류(1, "invalid or corrupted")는 명시적으로 배제한다. 양성 대조가 둘 있다. 같은 템플릿에 OU=Apple Software를 넣으면 통과하고, OU 조건을 지우면 통과한다. 그러니 거부는 OU 조건 덕분이라는 것이 증명된다. `verify_node_signature`의 RealEnv 경로(TeamIdMismatch·변조 시 CodeSign)도 확인한다. 임시 파일은 `std::env::temp_dir()` 아래 `r22-codesign-<tag>-<pid>`에만 만들고 Drop으로 지운다(실행 후 잔여 0 확인). 남은 공백: N-8, 그리고 아래 "관점별" 절의 ③단계 경유 음성. |
| RV-004 "롤백용" 약속 | ✅ 해소 | 결과 문구는 `previous_kept_line`(`app_node.rs:374`) "한 단계 보관합니다(다음 업데이트 때 정리됩니다)", 미리보기 문구는 "한 단계 보관하며 다음 업데이트 때 정리됩니다"다. src 전체 grep에서 사용자에게 보이는 "롤백"은 0건이다(남은 것은 주석과 rename 되돌림 테스트 이름뿐). 샌드박스 단언도 따라 바뀌었다(tests_sandbox.rs:173). 문구 테스트가 있다. |
| RV-005 멱등 판정·"온전히" | ✅ 해소 | `existing_install_is_healthy`(`node_install/mod.rs:293-322`)의 순서는 필수 파일 → **codesign 3중(:307)** → 버전(:312) → `check_npm_loads`(:317)다. node 실행은 서명·버전 확인 **뒤에만** 일어난다. 실패하면 `Ok(false)`로 재설치 경로를 탄다. 테스트 `tests.rs:124`가 npm-cli.js를 손상시키면 재다운로드·교체되고 다시 멱등이 됨을 확인한다. 가짜 환경은 실제 파일 내용으로 판정한다(`testkit.rs:401-410`). 문구 `ALREADY_INSTALLED_NOTE`(:370)는 검사한 것만 말한다. |
| N-1 `.current-new-*` 잔여 | ✅ 해소 | `sweep_stale_staging(layout)`(`layout.rs:137-158`)가 잠금 아래에서 `node/.current-new-*`를 **심볼릭 링크일 때만** 지운다. 테스트 `tests.rs:613`이 같은 접두의 디렉터리는 보존됨을 확인한다. `staging_leftovers`가 두 접두를 모두 보도록 넓어져, 기존 실패 주입 테스트 전부의 사후 단언도 강화됐다. |

## 위임 확인 항목 판정
| 질문 | 판정 | 근거 |
|---|---|---|
| Homebrew/기존 Node·네이티브 claude 사용자 영향 | 없음 | 후보 순서가 `/opt/homebrew/bin` → `/usr/local/bin` → `~/.local/bin` → 앱 npm-global이다(claude_auth.rs:84-90, autonomy·mcp·session 동일 관례). 앞의 셋 중 하나라도 있으면 그것이 선택되고 주입은 0이다. 음성 표 테스트로 고정돼 있다. |
| 경로 판정 우회·오판정 — 유사 이름 | 안전 | `Path::starts_with`는 성분 단위라 `npm-global-evil`은 걸리지 않는다(테스트 있음). |
| — 심볼릭 링크 | 안전(비회귀) | claude 경로 자체는 canonicalize하지 않는다. 바깥 링크(예: 사용자가 만든 `/usr/local/bin/claude → npm-global/...`)는 주입 없이 RV-002 이전 동작으로 남는다. 반대로 `npm-global` 자체를 링크로 바꿔 바깥을 가리키게 해도, 주입값은 원래 npm-global 경로라 해가 없다. 둘 다 같은 OS 사용자만 만들 수 있는 상태다(기각 표 참조). |
| — `..` 성분 | 도달 불가 | 어휘 비교라 `npm-global/../..`도 참이 된다. 하지만 claude 경로는 고정 후보 4개를 `~` 확장한 값뿐이라 `..`가 들어올 수 없다(기각 표). |
| Windows 회귀 | 코드상 없음 / 컴파일 미확인 | `claude_child_env`는 `mac_layout()`이 Windows에서 None이라 항상 빈 Vec을 돌려준다. 6개 지점은 빈 `envs`/`env_refs(&[])`가 되어 이전 `&[]`와 같다. 새 테스트 중 경로 리터럴을 쓰는 것은 `#[cfg(unix)]`이고, 가드·취소 테스트는 플랫폼 중립이다(기존 `run_pinned_with` 테스트와 같은 패턴). CI windows-latest 녹색 확인이 여전히 머지 조건이다. |
| RV-005 node 실행이 codesign 뒤인가 | 예 | `mod.rs:307 → :312 → :317` 순서다. 새 설치 경로(:453 이후)에는 `check_npm_loads`가 없다(SHA256로 덮이므로 타당). |
| RV-003 실제 codesign·temp 한정 | 예 | `CODESIGN`(= `/usr/bin/codesign`)을 RealEnv로 호출한다. 쓰기는 temp 하위 폴더 3곳(`req-adhoc`, `anchor`, `verify-*`)뿐이다. 실행 후 잔여 0이다. |

## 지적 사항 (신규, 통합)
| ID | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-006 | 🟡 | 프로세스 실행 / 이기종 | `app_node.rs:162`(주석 "앱이 띄운 프로세스 트리에만 미친다"); `session_chat/turn.rs` `command.envs(child_env)`; `autonomy/runner.rs:125` | Read + 6개 지점 grep. nvm 동작은 **기억 기반, 이 PC에 nvm이 없어 실측 못 함 — 확인 필요** | `npm_config_prefix`는 claude 본인만이 아니라 claude 세션이 띄우는 **Bash 도구 명령·MCP 서버·hook까지** 상속된다. 그래서 앱 설치 claude 세션에서 에이전트가 `npm i -g X`를 돌리면 사용자가 아는 위치가 아니라 앱 npm-global로 떨어진다. 앱 PATH에서는 보이지만 사용자 터미널에서는 보이지 않는다(1차 RT-1과 같은 뿌리). 또 사용자가 앱 Node를 깐 뒤 nvm을 들였다면, nvm.sh는 `npm_config_prefix` env가 nvm 폴더 밖을 가리킬 때 `nvm use`를 거부하는 것으로 알려져 있다. 그 경우 셸 rc를 읽는 Bash 도구 안에서 nvm 노드 전환이 실패할 수 있다. 주석의 "프로세스 트리에만"은 사실이지만, 그 트리가 사용자 작업 전체(세션)라는 함의가 빠져 있다. 발생 조건(앱 Node 설치본만 있다가 nvm 추가)이 좁아 Minor로 둔다. | (a) 주석과 개발 도구 화면 안내에 "앱에서 연 Claude 세션 안의 전역 npm 설치도 앱 폴더로 간다"를 한 줄 적는다. (b) nvm 공존 시 동작을 샌드박스 1회로 실측한다. (c) 구조적 해법은 RT-4(env 대신 앱 Node 내장 npmrc). |
| RV-007 | 🟡 | 주장대비검증 | `app_node.rs:152-158`(전제를 사실로 서술); `node_install/mod.rs:241-266`(`check_npm_loads`), 테스트는 `testkit.rs:401-410` 가짜뿐 | 커밋 eec87ec 본문 "어긋남이 **추정**됐다"와 주석 대조. 1차 샌드박스 로그는 RV-005 이전 실행이다. | ① 1차 권고 3은 "샌드박스에서 `claude update` 설치 위치를 먼저 실측한 뒤 어긋나면 RT-2"였는데, 실측 없이 수정이 들어갔다. 주석은 "Claude는 자기 갱신 때 `npm install -g`를 부르는데"라고 단정한다(P2-3b 실측은 npm 기본 prefix에 대한 것이지 Claude 갱신 방식에 대한 것이 아니다). 전제가 틀려도 주입은 무해한 방향(설치 위치 설정일 뿐)이라 위험은 낮다. ② RV-005의 양성 경로(실제 Node로 `node npm-cli.js --version`, PATH=/usr/bin:/bin)는 비-ignore 테스트에서 가짜로만 돈다. 실물에서 이 검사가 늘 실패하면 fail-safe이긴 하다. 하지만 모든 재실행이 "깨짐 → 약 50MB 재다운로드"가 되어 멱등이 조용히 무너진다. | PM이 #[ignore] 샌드박스(`tests_sandbox.rs`)를 1회 재실행해 [3] 재실행 멱등이 여전히 `already_installed`인지 확인한다(RV-005 실물 양성). 같은 샌드박스에서 앱 PATH·env로 `claude update`(또는 자동 갱신) 후 설치 위치를 확인하고, 결과에 맞춰 주석을 "실측" 또는 "추정"으로 고친다. |

### Minor 미만 (Nit)
| ID | 심각도 | 위치 | 내용 / 개선안 |
|---|---|---|---|
| N-7 | ⚪ | `app_node.rs:538-569` | 가드 테스트는 **파일 단위** `contains`다. 이미 `claude_child_env(`를 가진 파일(claude_auth.rs는 3지점)에 4번째 spawn이 생겨도 잡지 못한다. `split("#[cfg(test)]")`는 본문 위쪽에 테스트 전용 항목이 있으면 그 아래를 통째로 놓친다. 또 `dev_tools/` 전체를 제외한다. 지금 6지점은 수동 grep으로 전부 맞음을 확인했다. 개선안: `build_child_path_env(Some(` 출현 수와 `claude_child_env(` 출현 수를 파일별로 같게 단언한다. |
| N-8 | ⚪ | `tests_codesign.rs:136` | 변조 위치 `len/2`가 `/bin/ls`의 슬라이스 배치에 의존한다. 이 PC에서는 arm64e 슬라이스 앞쪽(코드 영역)에 떨어져 결정적이다. 다만 슬라이스 사이 정렬 패딩(현재 64512~65536, 약 1KB)에 떨어지는 OS 버전이면 `--verify --strict`가 통과해 테스트가 오탐 실패할 수 있다. 개선안: `lipo -thin`으로 단일 아키 사본을 만들거나, `__TEXT` 오프셋을 계산해 변조한다. |
| N-9 | ⚪ | `node_install/mod.rs:307-317`, `tests.rs:124` | "npm 로드 검사는 codesign·버전 뒤" 순서가 코드로는 맞다. 하지만 멱등 경로에 대한 순서 단언 테스트가 없다(새 설치 경로에는 `pos(a)<pos(b)` 단언이 있다). 재배치 회귀를 잡도록 같은 방식의 단언 1개를 권장한다. |
| N-10 | ⚪ | `node_install/mod.rs:244` | 주석 "읽기만 하는 호출이다(설정·캐시 쓰기 없음)". npm 9+는 `--version`에서도 load 단계에서 `~/.npm/_logs`에 디버그 로그를 쓸 수 있다(**미실측, 확인 필요**). 무해하지만 단정을 "설정 변경 없음"으로 낮추거나 RV-007 샌드박스에서 확인한다. |
| N-11 | ⚪ | `app_node.rs:84-87`, `actions.rs:139→155`, `:305→344` | 버전 결정과 설치가 서로 다른 토큰이라, 두 단계 사이에 도착한 취소(앱 종료 포함)는 놓친다. 주석에 정직하게 적혀 있고 취소 UI가 없어 현재 경로도 없다. 1차 권고 2와 같이 **취소 버튼을 붙이기 전 선결 조건**으로 이월한다. |

## 기각된 지적
| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 프로세스 실행 | `path_is_under`가 어휘 비교라 `.../npm-global/../../x/claude`도 참이 되어 오주입된다 | 기각 | claude 경로의 출처는 고정 후보 4개의 `~` 확장뿐이다(`cli_launcher.rs:94-103`, 각 모듈의 `CLAUDE_PATH_CANDIDATES`). `..`가 들어올 경로가 없다. 오주입돼도 결과는 같은 사용자 소유 폴더로의 설치 위치 변경이다. |
| 프로세스 실행 | 바깥 심볼릭 링크로 앱 npm claude를 가리키면 주입이 빠져 RV-002가 재발한다 | 기각(비회귀) | 직접 만든 링크라는 사용자 행위가 필요하다. 그때 동작은 RV-002 이전과 같다. 앞 후보(`/usr/local/bin`)가 선택된 정상 경로다. |
| 신뢰체인 | `verify_node_signature`의 ③단계(-R)는 실물 codesign으로 음성 검증되지 않는다(실물 음성 표본이 모두 ②에서 걸림) | 기각 | ③이 쓰는 문자열은 `developer_id_requirement()` 하나다. 그 문자열의 실물 평가는 `check_requirement`로 음성·양성 모두 증명됐고, 함수 호출 자체는 FakeEnv 테스트(-R 호출 삭제 시 실패)가 고정한다. Team ID가 HX7739G8FX인데 Apple 체인이 없는 실물 표본은 구할 수 없다. |
| 원자성 | `check_npm_loads`가 서명 밖 JS(npm-cli.js)를 실행한다 | 기각 | 1차 기각(TOCTOU·NODE_OPTIONS)과 같은 위협 모델이다. 그 파일을 바꿀 수 있는 주체는 같은 OS 사용자뿐이고, 실행하는 node는 서명 검증을 통과한 바이너리다. |
| 이기종 | 사용자가 나중에 Homebrew Node를 깔면 claude 자기 갱신이 brew npm + 앱 prefix 조합이 된다 | 기각 | PATH 순서상 그렇게 된다. 하지만 설치 위치가 앱이 찾는 npm-global로 일관되고, shim은 `#!/usr/bin/env node`로 PATH의 node를 쓴다. 어긋남이 생기지 않으니 오히려 RV-002가 의도한 결과다. |

## 페르소나별 관점

### 프로세스 실행 보안 — 🟢
- 주입 조건이 "러너 종류"가 아니라 **claude 실행 파일 위치** 하나로 정해진다. 6개 지점이 같은 빌더(`claude_child_env`)를 쓰고 `env_refs`로 모양만 바꾼다. 지점마다 로직을 복제하지 않았다.
- PATH(전역 빌더)와 prefix(조건부)의 적용 범위가 이제 claude 자식에서도 맞는다. 1차 RT-2의 절반이 실현됐다.
- 남은 것: RV-006(손자 상속의 함의).

### 주장 대 검증 — 🟢
- 해소 주장 6건을 모두 코드와 테스트로 확인했다. 테스트 수(1122)는 직접 실행 결과다.
- RV-001 회귀 테스트가 "빈 깡통"이 아님을 확인했다. DeadEnv가 취소 신호를 먼저 보도록 바뀌어, 옛 불리언 구현이라면 `version_lookup_after_a_prior_cancel...`이 "취소" 문구로 실패한다.
- 남은 것: RV-007(전제 미실측 + 실물 양성 미실측), N-7(가드 공허성 일부).

### 이기종 머신 운영자 — 🟢 (Windows 컴파일 미확인)
- 후보 순서가 앱 npm-global을 최후순위로 둔다. 다른 claude가 하나라도 있으면 주입이 0이라, 기존 사용자에게 영향이 없다.
- nvm 공존 기기는 RV-006으로 따로 적었다.
- Windows: 코드상 빈 주입이다. CI windows-latest 확인 전에는 머지하지 말 것(1차와 같은 조건).

### 릴리스 아티팩트 신뢰체인 — 🟢
- RV-003 테스트의 설계가 좋다. 종료 코드 3과 1을 구분하므로 "문자열이 깨져서 우연히 거부"되는 경우를 통과시키지 않는다. "OU 조건을 지우면 통과"라는 반대 증명까지 있어, 요구식 각 절이 실제로 일하고 있음을 보인다.
- 남은 것: N-8(변조 오프셋의 환경 의존).

### 설치 원자성·압축 해제 탈출 — 🟢
- 1차 크래시 지점 표의 `flip_current symlink~rename 사이` 행이 "무해, 남음"에서 "다음 설치에서 쓸기"로 바뀌었다. 링크가 아닌 동명 항목은 건드리지 않는다.
- 멱등 판정이 "파일이 있다"에서 "npm이 로드된다"로 한 단계 강해졌고, 손상 시 복구 경로가 생겼다.
- 남은 것: N-9(순서 단언), RV-007②.

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵
| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| RT-4 (신규) | prefix를 **자식 프로세스마다 env로** 붙인다. 지점이 dev_tools 1곳과 claude 6곳이고, 누락은 소스 스캔 가드로 막는다. env는 손자(Bash 도구·nvm)까지 새어 나간다(RV-006). 사용자 터미널에서 앱 node를 직접 쓰면 prefix가 다시 `node/<ver>`로 돌아간다(RT-1과 충돌). | 앱 Node 설치본 안의 npm **builtin config**(`node/<ver>/lib/node_modules/npm/npmrc`)에 `prefix=<npm-global>` 한 줄을 설치 시 써 넣는다. 스테이징에서 검증을 마친 뒤 rename 전에 쓰고, 멱등 판정이 그 줄을 확인한다. 그러면 env 주입 7지점과 가드 테스트를 걷어낼 수 있다. | "앱 Node의 npm이면 어디서 불리든 같은 prefix"가 **npm 자신의 설정**으로 성립한다. 주입 지점 누락이라는 결함 부류가 사라지고, 다른 npm(brew·nvm)과 손자 프로세스에는 영향이 없다(RV-006 해소). RT-1(터미널 opt-in)을 택해도 일관된다. `~/.npmrc`·셸 rc는 여전히 불변이다. | 저~중. builtin npmrc는 배포판이 prefix를 바꿀 때 쓰는 표준 장치지만, 우선순위가 env > 사용자 `~/.npmrc` > 전역 > builtin이다. 그래서 `~/.npmrc`에 `prefix=`가 있는 사용자는 그 값을 따른다(지금 env 방식은 그것을 이긴다). 이 차이를 수용할지 정해야 한다. 검증된 트리에 앱이 파일 1개를 더하므로, 무결성 서술("tarball 그대로")도 고쳐야 한다. **npm 우선순위는 기억 기반이라 확인이 필요하다.** |

1차 Rethink 재평가: RT-1(터미널 opt-in)은 미착수다. RT-2(env 일원화)는 claude 자식 범위에서 실현됐다(RV-002). RT-3(SHASUMS GPG)은 보류가 그대로 유지된다.

## 트레이드오프
- **env 주입(현행) 대 builtin npmrc(RT-4)**: 현행은 사용자 `~/.npmrc`의 prefix보다 강하게 이긴다. 그래서 "앱이 찾는 위치 = 설치 위치"가 확실하다. 대신 손자 프로세스로 새어 나간다. RT-4는 새지 않지만, 사용자 npmrc가 있으면 그쪽을 따른다. → **권고: 지금은 현행 유지(머지 가능).** RV-006 문구 보강과 RV-007 실측 뒤에 RT-4를 백로그에서 재평가한다. 근거: 현행의 부작용은 발생 조건이 좁고(앱 Node 단독 설치 + nvm 추가) 위험이 없는 방향(설치 위치)이다.

## 잘 된 점
- **상태를 없애서 고친 RV-001**: 리셋 위치를 하나 더 다는 대신 세대 카운터로 "누가 언제 내리나" 문제 자체를 지웠다. 남은 한계(두 단계 틈)도 숨기지 않고 주석에 적었다.
- **RV-003의 반대 증명**: "거부된다"만이 아니라 "OU를 빼면 통과한다"를 함께 단언해, 테스트가 요구식의 의미를 고정한다.
- **RV-002 음성 표**: 유사 이름(`npm-global-evil`), 앱 `node/current/bin`, 빈 문자열, 홈 없음까지 한 표로 고정했다. 실제 자식 프로세스 env 도달도 확인한다.
- N-1 쓸기를 "링크일 때만"으로 한정하고, 동명 디렉터리 보존을 테스트로 고정했다.
- `staging_leftovers` 확장으로 기존 23종 실패 주입 테스트의 사후 불변식이 저비용으로 함께 강해졌다.

## 평가기준 충족 현황
| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| 1차 RV-001~005·N-1 해소 | 주장대비검증 | 필수 | ✅ | 6/6 |
| 주입 조건이 앱 npm-global 한정 | 프로세스 | 필수 | ✅ | 음성 표 테스트 |
| 기존 brew/native 사용자 무영향 | 이기종 | 필수 | ✅ | 후보 순서 |
| node 실행은 서명 검증 뒤 | 원자성 | 필수 | ✅ | N-9(테스트 고정 없음) |
| 실물 codesign 음성 | 신뢰체인 | 필수 | ✅ | N-8 |
| 실제 홈·temp 잔여 0 | 원자성 | 필수 | ✅ | 직접 확인 |
| 전제·양성 경로 실측 | 주장대비검증 | 권장 | ⚠ | RV-007 |
| Windows 회귀 없음 | 이기종 | 필수 | ⚠ | CI 확인 대기 |

## 실행 경계
이번 리뷰에서 소스 수정, 커밋, push, 실제 홈·셸 rc·전역 npm 변경, 네트워크 다운로드, #[ignore] 테스트 실행, scratchpad nodedl 폴더 접근은 **하지 않았다**. 실행한 것은 워크트리 `src-tauri/target`을 쓴 `cargo test --lib`(3회, 필터 포함)과 `cargo build --lib` 1회, 읽기 전용 명령(`lipo`, `ls`, `grep`)뿐이다. 작성·변경한 파일은 다음뿐이다(커밋 안 함): 이 보고서, 재사용 페르소나 6개의 적용 이력 append, `personas/INDEX.md` 6개 행의 "최근 재사용" 덧붙임.

## PM에게 권고
1. **머지 가능(Green)**. 조건은 1차와 같다: CI `rust-test (windows-latest)` 녹색 확인.
2. RV-007: 머지 전 또는 직후에 #[ignore] 샌드박스를 1회 재실행한다. 확인할 것은 [3] 멱등이 `already_installed`로 유지되는지(RV-005 실물 양성)와 claude 자기 갱신의 설치 위치(RV-002 전제)다. 결과에 맞춰 `app_node.rs:152-158` 주석의 "실측/추정" 표기를 고친다.
3. RV-006: 문구 1줄(세션 안 전역 npm 설치도 앱 폴더로 간다)을 넣는다. nvm 공존 동작은 샌드박스 실측 대상에 추가한다.
4. N-7~N-10은 다음 라운드 백로그에 넣는다. 모두 저비용으로, 테스트나 주석 각 1곳 수정이다(근거: 위치 표에 수정 지점이 한 곳씩). N-11은 취소 UI 착수 시 선결 조건으로 유지한다.
5. RT-4는 RV-006·007 결과를 보고 재평가한다. npm 설정 우선순위 실측이 선행 조건이다.
