# R22 macOS Node 사용자 공간 설치 — 리뷰 보고서

리뷰 대상: 워크트리 `.claude/worktrees/r22-node`, 브랜치 `feat/r22-node-userspace`, 커밋 범위 `8dd994f..d80a3a8` (ecabbe7 P2-3a, beb174f·b498320·d80a3a8 P2-3b, afcc4d4 P2-3c), target_id `r22-node-userspace-install`
리뷰 페르소나 패널(6): `persona-release-artifact-trust-chain.md`, `persona-install-atomicity-and-archive-escape.md`(신규), `persona-process-execution-security.md`, `persona-claimed-vs-verified.md`, `persona-heterogeneous-machine-operator.md`, `persona-zero-base-redesigner.md`(발산형)
리스크 범주: 외부 바이너리 다운로드·무결성 검증·사용자 공간 설치(공급망/실행 경계)
등급/모드: Sensitive — 최초 리뷰, 풀패널(축소 없음). 위임에 축소 근거 3값이 없어 축소 판정 대상 아님.
리뷰 일자: 2026-10-15
종합 판정: 🟢 Green (Critical 0 · Major 0 · Minor 5 · Nit 6 · Rethink 3)

## 요약 (2분 규칙)
위임된 불변식 1~5와 7은 코드와 테스트에서 성립한다. 출처는 `https://nodejs.org/dist/`로 고정돼 있고 첫 URL과 리다이렉트 목적지를 둘 다 정책 검사한다. 단계 순서는 SHA256 → tar 목록 검사 → 해제 → 트리 검사 → xattr → codesign 3중 → `node --version` → rename이고, 테스트가 호출 순서를 위치 비교로 단언한다. 실패 주입 테스트 23종은 모두 "이전 설치 보존 + 스테이징 잔여물 0"으로 끝나는 것을 확인했다. Major 이상 결함은 없다. 다만 취소 플래그가 저절로 내려가지 않는 잠복 결함(RV-001)이 있다. 지금은 취소 UI가 없어 사용자가 이 경로에 닿지 못하지만, 취소 버튼을 붙이기 전에 반드시 고쳐야 한다. 불변식 6(Windows 회귀)은 코드상 가드가 맞다. 로컬 교차 컴파일은 실패해 확인하지 못했으므로 CI windows-latest 결과로 확인해야 한다.

## 페르소나 재사용 판정 (산출물 게이트)
INDEX.md(22행)의 역할개념 열을 착수 전에 대조했다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| release-artifact-trust-chain | 재사용 | 역할개념 "클라이언트가 아티팩트를 무엇을 근거로 신뢰하는가"에서 이번 클라이언트는 앱 자신이고 아티팩트는 nodejs.org tarball이다. 6대 요소는 고치지 않았다. |
| install-atomicity-and-archive-escape | **신규** | 디스크 잔여물을 보는 기존 행은 credential-at-rest(자격증명 파일의 권한·수명) 하나뿐이다. "외부 압축을 펼치는 rename 원자성, 크래시 후 오인, tar 탈출"은 이 저장소에 처음 생긴 표면이다. 이전 설치는 모두 brew/npm/winget 외부 프로세스에 맡겼다. |
| process-execution-security | 재사용 | 새 도구 실행 4종, env 주입, 기본 PATH 확장이 역할개념(argv·env·PATH) 그대로다. |
| claimed-vs-verified | 재사용 | 테스트가 순서와 실패를 실제로 단언하는지, 목이 실패를 숨기는지가 역할개념 그대로다. |
| heterogeneous-machine-operator | 재사용 | Windows, Homebrew 유무, CPU 아키텍처 조합. |
| zero-base-redesigner (발산) | 재사용 | 구조 전제를 의심한다. |

## 직접 실행한 확인
| 항목 | 결과 |
|---|---|
| `cargo test --lib` (워크트리 `src-tauri/target` 재사용, HEAD d80a3a8) | **1103 passed, 0 failed, 24 ignored** |
| 테스트 전후 실제 홈 `~/.local/share/malgn-agent` | 실행 후에도 없음(`ls` → No such file). 실제 홈은 바뀌지 않았다. |
| PM 확보 로그 `r22-logs/real-home-before.txt` 대 `after.txt` | 타임스탬프 줄을 빼면 같다(`.npmrc`·`.zshrc` mtime·크기 그대로) |
| `sandbox-3c.log` (실제 nodejs.org 왕복, #[ignore], PM 실행) | v24.21.0 설치 → npm i -g pnpm(prefix=npm-global 확인) → v24.22.0 갱신 → pnpm 생존 → 재실행 멱등(already_installed) 순서가 모두 통과했다. 실제 codesign 3중의 **양성** 경로가 여기서만 실증된다. |
| `node --test scripts/tests/r22InstallMethodLabel.test.mjs` | pass 3 / fail 0 |
| `tsc --noEmit` | 오류 없음 |
| `cargo check --target x86_64-pc-windows-msvc --tests` | **실패(환경)**: ring이 C 헤더(`assert.h`)를 요구한다. 코드 오류가 아니라 교차 툴체인 부재다. 이 Windows용 산출물(target 하위)은 확인 뒤 지웠다. |
| 네트워크 실다운로드·#[ignore] 재실행 | 위임 지시대로 하지 않았다 |

## 불변식 판정
| # | 불변식 | 판정 | 근거 |
|---|---|---|---|
| 1 | 출처 고정·리다이렉트·버전 규칙 | ✅ | `types.rs:18` `DIST_BASE`. `fetch.rs:76-84` scheme·host·port·userinfo·fragment·`/dist/` 접두를 검사하고, `fetch.rs:129`(첫 URL)와 `:202-212`(리다이렉트, 최대 3회)에 둘 다 적용한다. URL은 `NodeVersion` 재조립 값으로만 만든다(`release.rs:12-31`). 버전은 `parse_component`로 엄격하게 검증한다(선행 0·접미·경로 문자 거부, `types.rs:97-110`). 하드코딩 버전은 없고 메이저 하한만 있다(`types.rs:22`). reqwest에 gzip 기능이 없어(`Cargo.toml:44`) Content-Length 대조가 왜곡되지 않는다. |
| 2 | 검증 순서, 통과 전 실행 없음 | ✅ | `mod.rs:339-420` 순서가 주석(`:5-15`)과 일치한다. 테스트 `tests.rs:42-56`이 위치 비교로 순서를 단언하고, 실패 테스트마다 `ran_no_tools`/`!ran_program("node --version")`을 확인한다(`:148, :188, :207, :217, :226-227, :407-410`). 목이 실패를 숨기지 않는지도 봤다. `-R` 호출을 지우면 `team_id_matching_but_apple_chain_requirement_failing_is_rejected`(`:202`)가 실패하고, Team ID 파싱을 지우면 `:181`이 실패한다. 남은 공백은 RV-003(요구식 문자열 자체는 실물 codesign으로 음성 검증된 적이 없음). |
| 3 | 실패 시 부분 설치 없음·보존·정리 | ✅ | 반영은 rename 둘뿐이다(`mod.rs:424-447`, 되돌림 포함). 스테이징은 RAII로 지운다(`layout.rs:123-127`). 잠금 아래에서 크래시 잔여물을 쓸어낸다(`layout.rs:131-139`, `mod.rs:299-300`). 실패 주입은 해시 2·Team ID 3·codesign·xattr·버전·중단·리다이렉트·상태코드·디스크 3·취소 4·tar 9·rename 4·잠금·잔여물 쓸기를 덮고, 모두 `assert_prior_preserved_and_clean`으로 끝난다. 크래시 지점 표는 아래 페르소나 섹션에 있다. |
| 4 | rc·`~/.npmrc` 불변, prefix는 앱 자식만 | ✅ | diff 전체에서 rc나 `.npmrc`에 쓰는 코드는 없다(grep 결과 주석·문구·샌드박스 단언만 걸림). 주입은 `actions.rs:81-82`(`spawn_plan_process`) 한 지점뿐이고 조건은 `Runner::Npm` + 앱 node 경로(`app_node.rs:95-108`)다. 테스트 `actions.rs:672-702`이 실제 자식 env로 확인한다. 범위가 좁은 데서 생기는 부작용은 RV-002. |
| 5 | 시크릿·capabilities·커맨드 입력 | ✅ | `capabilities/`·`tauri.conf.json`·`Cargo.toml` diff는 0줄이다. 새 커맨드는 인자 없는 `cancel_node_install`(`mod.rs:365`) 하나다. 설치 버전은 웹뷰 입력이 아니라 백엔드가 다시 결정해 plan_id와 대조한다(`actions.rs:305-318`). 시크릿 패턴 grep 결과도 0이다. |
| 6 | Windows 회귀 | ✅(코드) / ⚠ 미검증(컴파일) | `runner_path_for`가 Mac이 아니면 None(`app_node.rs:80-85`), `mac_layout`도 Mac만(`:66-72`), `default_path_dirs`는 Mac 분기에만 추가(`platform.rs:227-228`)한다. 테스트(`install_resolver.rs` 신규 3종)가 winget 경로가 그대로임을 고정한다. 로컬 교차 컴파일은 환경 문제로 실패했으니 CI windows-latest `cargo test` 녹색 확인이 머지 조건이다. |
| 7 | npm JS는 SHA256만 보호, GPG 미검증 | ✅ 수용 | 아래 트레이드오프 절. <50명 사내 도구 기준으로 수용한다. 결정 기록으로 남길 것을 권고한다. |

## 지적 사항 (통합)
| ID | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| RV-001 | 🟡 | 주장대비검증 / 원자성 | `app_node.rs:44-54, :143, :249`; `mod.rs:361-363`; `devToolsApi.ts:107-110`; `fetch.rs:132-134` | 코드 경로 추적(Read) | 취소 플래그 `CANCEL`을 `false`로 되돌리는 곳은 `run_pinned` 시작(`:249`) 하나뿐이다. 그런데 `perform_install`/`perform_update`는 `run_pinned`에 닿기 **전에** `resolve_target_tag()`를 부르고(`actions.rs:139, :305`), 이 함수가 같은 플래그를 본다(`:143`). 따라서 (a) 설치 도중 취소하면 끝난 뒤에도 플래그가 `true`로 남는다. (b) 설치가 없을 때 `cancel_node_install`을 호출해도 남는다. 두 경우 모두 이후 미리보기와 설치가 계속 "설치가 취소되었습니다"로 실패하고, 앱을 재시작해야 풀린다. 문서 주석 3곳의 "진행 중인 설치가 없으면 아무 일도 하지 않는다", "다음 설치는 시작할 때 플래그를 지운다"는 사실과 다르다. 지금은 취소 UI가 없고 release 빌드에는 devtools도 없어 사용자 경로가 없다. 그래서 Major에서 강등했다(기각 게이트: 현재 영향 경로 없음). | 진행 중인 설치가 있을 때만 플래그를 세우거나(설치 중 표시 AtomicBool + compare_exchange), 세대 카운터(설치 시작 시 증가, 취소는 현재 세대에만 적용)로 바꾼다. 최소한 `perform_install`/`perform_update`의 AppNode 분기 첫 줄에서 리셋한다. **취소 버튼을 연결하기 전 선결 조건**이다. |
| RV-002 | 🟡 (확인 필요·추정) | 프로세스 실행 / 발산 | `actions.rs:81-82`(주입 유일 지점); `app_node.rs:40`, `autonomy/runner.rs`·`session_chat`·`claude_auth`·`mcp_manager` 후보 목록 | grep `npm_prefix_env\(` → 호출부 1곳 | `npm_config_prefix`는 개발 도구 화면의 설치·갱신에만 주입된다. 앱이 띄우는 `claude`(세션·자율업무·MCP)에는 주입되지 않는다. Claude Code가 자기 자동 업데이트에서 `npm install -g`를 부르면, 그 npm의 prefix는 실행 파일 기준 `node/<ver>`가 된다(샌드박스 로그 `[1b] 대조`가 이 기본값을 실측). 그러면 새 사본이 `node/<ver>/bin/claude`에 떨어지는데, 이 위치는 어느 후보 목록에도 없다. 앱은 옛 버전을 계속 보고, Node가 갱신되면 그 사본은 prune으로 사라진다. **Claude 자동 업데이트의 실제 동작은 확인하지 못했다.** | 샌드박스에서 앱 PATH·env로 `claude update`를 돌려 설치 위치를 확인한다. 위치가 어긋나면 앱 관리 Node를 쓰는 모든 자식 프로세스에 같은 prefix를 주입하도록 `build_child_path_env`와 짝을 이루는 env 빌더로 일원화한다. |
| RV-003 | 🟡 | 공급망 / 주장대비검증 | `mod.rs:193-195`; `testkit.rs:381-400` | grep `anchor apple` → 본문 1곳, 테스트 0곳 | 가짜 환경은 `codesign`의 인자 내용을 보지 않고 플래그로만 성공·실패를 정한다. 그래서 요구식 문자열(`=anchor apple generic and certificate leaf[subject.OU] = "HX7739G8FX"`)이 너무 느슨해지는 회귀가 생겨도(예: OU 조건 삭제) 어떤 비-ignore 테스트도 실패하지 않는다. 양성 경로는 샌드박스가 실증했지만 실물 codesign의 **음성** 경로는 실증되지 않았다. | macOS 전용 비-ignore 테스트를 둔다. `RealEnv`로 `verify_node_signature`를 Apple 플랫폼 바이너리(`/bin/ls`, TeamIdentifier=not set)와 ad-hoc 서명한 임시 복사본에 돌려 `TeamIdMismatch`/`CodeSign`인지 단언한다. 네트워크가 필요 없어 CI macos 러너에서 돈다. 요구식 문자열 자체에 대한 스냅숏 단언도 추가한다. |
| RV-004 | 🟡 | 주장대비검증 | `app_node.rs:284`; `app_node.rs:227`(미리보기 문구 "이전 버전은 한 단계 남겨 둡니다") | grep `rollback\|롤백` → 문구 1곳, 실행 경로 0 | 결과 메시지가 "이전 버전 vX는 한 단계 **롤백용**으로 남겨 둡니다"라고 하지만, 롤백 액션(current를 이전 버전으로 되돌리는 커맨드·UI)은 없다. 사용자에게는 쓸 수 없는 약속이다. | 문구를 "이전 버전을 한 단계 보관합니다(다음 업데이트 때 정리)"로 낮추거나, 롤백 액션을 백로그로 등록한다(구현은 `flip_current(prev)` 한 번이라 저비용 — 확인 근거: `layout.rs:219-233`가 이미 범용). |
| RV-005 | 🟡 | 원자성 | `mod.rs:260-284`, `:236-256`; `app_node.rs:281` | Read | 멱등 판정(`existing_install_is_healthy`)은 `bin/node` 서명·버전과 필수 파일 3개만 본다. 같은 버전 디렉터리에서 npm 본체 JS가 깨져 있으면(디스크 오류·사용자 실수) "이미 **온전히** 설치되어 있어 변경하지 않았습니다"로 끝나고 복구 경로가 없다. 위협 모델상 악의적 변조는 같은 사용자라 범위 밖이고, 이 지적은 복구 가능성에 관한 것이다. | 문구에서 "온전히"를 "서명·버전 확인됨"으로 정확히 바꾼다. 선택 사항: 설치 시 `.malgn-install.json`(tarball SHA256 + 파일 수)을 남기고 멱등 판정 때 파일 수를 대조한다. |

### Minor 미만 (Nit)
| ID | 심각도 | 위치 | 내용 / 개선안 |
|---|---|---|---|
| N-1 | ⚪ | `layout.rs:219-233`, `:131-139` | `flip_current`의 임시 링크 `.current-new-*`는 `node/` 아래에 생기는데 쓸기는 `base/.staging-*`만 본다. symlink()와 rename() 사이에서 크래시하면 링크가 영구히 남는다(무해: prune이 형식 다른 이름을 무시). 쓸기 대상에 추가 권장. |
| N-2 | ⚪ | `layout.rs:131` 호출부 `mod.rs:300` | 크래시 잔여 스테이징(최대 약 280MB)은 **다음 설치 시작 때만** 쓸린다. 설치를 다시 하지 않으면 계속 남는다. 앱 시작 시 잠금을 잡을 수 있으면 한 번 쓸기 권장. |
| N-3 | ⚪ | `classify.rs:144` 부근 대 `layout.rs:83-91` | 분류는 `canonical.starts_with(node_root)`를 어휘 비교만 하고, `path_is_under`(원문+canonical 디렉터리)를 쓰지 않는다. 홈 경로가 링크를 거치는 기기에서는 AppManagedNode가 Unknown으로 떨어져 업데이트 행이 안 뜬다. `path_is_under`로 통일 권장. |
| N-4 | ⚪ | `plan_table.rs:751`, `actions.rs:136-141` | UPDATE_TABLE의 AppManagedNode 행과 `run_pinned`에는 플랫폼 가드가 없다. Windows에서 그 경로에 node가 있을 가능성은 사실상 0이고, 있더라도 `/usr/bin/tar` 실행 실패로 fail-closed된다. `runner_path()`처럼 Mac 가드를 하나 두면 의도가 드러난다. |
| N-5 | ⚪ | `types.rs:35-41`; `.github/workflows/tauri-release-build.yml:166` | 릴리스는 `macos-latest`(arm64)에서 `--target` 없이 빌드하므로 `NodeArch::X64` 분기는 배포물에서 실행될 일이 없다. 테스트 픽스처도 Arm64뿐이다. 유니버설 빌드로 바꿀 때 X64 실측 필요. 메모만. |
| N-6 | ⚪ | `archive.rs:134-136` | 트리 안 하드링크(nlink>1)를 모두 거부한다. 공식 tarball에는 하드링크가 0개라(샌드박스 5888항목 통과) 지금은 맞지만, 상류가 하드링크를 도입하면 설치가 막힌다(fail-closed). 그때 "링크 둘 다 트리 안"이면 허용하도록 완화. |

## 기각된 지적
| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 원자성 | 스테이징~최종 위치 사이 TOCTOU(검증 후 `bin/node` 바꿔치기) | 기각 | 스테이징은 0700(`layout.rs:110-114`)이고 사용자 홈 안이다. 바꿔치기할 수 있는 주체는 같은 OS 사용자뿐이라 위협 모델 밖이다(페르소나 "의도적으로 무시"). |
| 프로세스 실행 | `node --version`이 앱 env(NODE_OPTIONS 등)를 물려받아 서명 검증 뒤 임의 코드가 실행될 수 있다 | 기각 | `process.rs`가 PATH만 바꾸고 나머지 env를 물려받는 것은 맞다(Read). 그러나 서명 통과한 공식 바이너리이고, env를 조작할 수 있는 주체는 같은 사용자다. 재현 경로를 대지 못한다. |
| 공급망 | `-R` 요구식이 Developer ID CA OID까지 고정하지 않아 같은 팀의 다른 인증서 종류(Apple Development 등)도 통과한다 | 기각(Nit도 아님) | OU=HX7739G8FX를 발급받을 수 있는 주체는 OpenJS뿐이고 `anchor apple generic`이 Apple 루트를 강제한다. 실질적인 신뢰 경계가 같다. |
| 이기종 | 사내 TLS 가로채기 프록시가 있으면 rustls(webpki roots)가 실패한다 | 기각 | 조직에 MITM 프록시가 있다는 근거가 없다. 실패하더라도 fail-closed(Network 오류)이고 이전 설치는 보존된다. |
| 원자성 | 반영 중 손상본을 치운 뒤 크래시하면 Node가 일시적으로 없다 | 강등 → 표에만 기록 | 다음 실행에서 쓸기 → 재설치로 복구된다(아래 크래시 표). 대상은 이미 손상된 같은 버전이라 잃는 것이 없다. |

## 페르소나별 관점

### 릴리스 아티팩트 신뢰체인 감사자 — 🟢
- 신뢰 근거는 둘이고 서로 독립이다. ① HTTPS로 받은 SHASUMS256.txt의 정확한 파일명 행 SHA256(부분·접두 일치 거부, 중복 행이 서로 다르면 거부 — `release.rs:87-114`). ② 실행 파일의 Apple 코드서명(Team ID 문자열과 `-R` 체인). 둘 중 하나만 깨져도 설치하지 않고, 테스트가 각각 주입해 확인한다.
- 해시는 받은 메모리가 아니라 **디스크 파일을 다시 읽어** 계산한다(`mod.rs:358`, `sha256_file`). 쓰기 중 손상까지 잡는다.
- 끊김의 관측성: 모든 실패가 구체적 사유(기대/실제 해시, 찾은 Team ID)를 담은 사용자 문구로 나온다(`types.rs:151-174`). 조용한 실패가 없다.
- 남은 공백: RV-003(요구식 음성 실측), 트레이드오프 절(npm JS).

### 설치 원자성·압축 해제 탈출 감사자 (신규) — 🟢
크래시 지점 표 (`mod.rs:422-453`):

| 크래시 지점 | 남는 디스크 상태 | 다음 실행 판정 |
|---|---|---|
| 다운로드·해제·검증 중 | `.staging-*`만, 이전 버전과 current 그대로 | 잠금 아래에서 쓸기 → 정상 설치 |
| 손상본 치움(`:429`) 직후 | 같은 버전 디렉터리 없음, current 끊김(같은 버전일 때) | 쓸기 → healthy=false → 재설치(복구) |
| 새 버전 rename(`:432`) 직후 | 새 버전 디렉터리 있음, current는 이전 버전 | healthy=true → current만 전환(다운로드 없음) |
| `flip_current` symlink~rename 사이 | `node/.current-new-*` 고아(N-1) | 무해, 남음 |
| prune 중 | 일부 옛 버전만 남음 | 다음 설치의 prune이 마저 정리 |

- tar는 세 겹으로 막는다. 해제 전 목록(`archive.rs:24-53`: 최상위 고정, `..`/`.`/빈 성분/절대경로 거부, 개행 밀반입 거부), bsdtar 기본 보안 플래그(-P 없음), 해제 후 `symlink_metadata` 트리 검사(어휘 + canonical 링크 검사, 하드링크·특수 파일·setuid, 크기·개수·깊이 상한 — `:66-204`). 테스트가 **실제 bsdtar**로 탈출 픽스처를 푼다(`testkit.rs:366-373` `really_run`).
- 잠금은 `flock`이라 프로세스가 죽으면 자동으로 풀린다. fork~exec 창 오탐은 1초 유예로 흡수한다(`layout.rs:150-191`). 드롭 순서는 스테이징 → 잠금이다(`mod.rs:299, :335`: `_lock`이 먼저 선언됨). 맞다.

### 프로세스 실행 보안 — 🟢
- 외부 도구는 모두 절대경로(`/usr/bin/tar|xattr|codesign`)로 셸 없이 띄우고 `PATH=/usr/bin:/bin`을 준다(`env.rs:119-128`). argv의 경로 인자는 UTF-8 경로만 받는다(`mod.rs:92-96`).
- mac 기본 PATH에 추가한 두 디렉터리는 시스템과 Homebrew 디렉터리 **뒤**에 붙는다(`platform.rs:227-228`). 그래서 기존 도구를 가리지(shadow) 않는다. `npm-global/bin`에 사용자가 패키지를 깔아도 `/usr/bin` 이하 이름은 가로채지 못한다.
- 앱 npm 전역 prefix에 있는 pnpm은 러너에서 제외한다(`install_resolver.rs` b498320). rc 수정을 요구하는 `pnpm setup` 경로를 원천 차단한 판단이 좋다.
- 남은 범위 공백: RV-002.

### 주장 대 검증 — 🟢
- 테스트 수와 결과는 직접 실행으로 확인했다(1103/0). 순서 단언은 "있다"가 아니라 `pos(a) < pos(b)` 비교 9개다(`tests.rs:45-56`).
- 목이 실패를 숨기지 않는지: 기본값이 성공인 플래그(codesign·xattr·chain)마다 반대값 테스트가 있어, 해당 호출을 지우는 변이는 잡힌다. 다만 인자 내용은 보지 않는다(RV-003).
- 문서와 코드 불일치: RV-001(취소 주석 3곳), RV-004(롤백), RV-005("온전히").

### 이기종 머신 운영자 — 🟢 (Windows 컴파일 미검증)
- Homebrew가 있으면 AppNode 후보를 건너뛴다(`install_resolver.rs` resolve_install_plan). 그래서 brew 사용자의 기존 동작(Manual `brew install node`)은 바뀌지 않는다. 테스트로 고정돼 있다.
- 나중에 Homebrew를 깔면 탐지는 brew node를 먼저 잡는다. 하지만 앱 npm-global의 패키지는 `resolve_runner_for_tool`이 앱 npm으로 갱신하므로 "두 번째 사본" 어긋남이 생기지 않는다(`runners.rs` 신규 함수). 좋은 설계다.
- Windows: 코드 가드는 맞다(위 불변식 6). 로컬 교차 컴파일은 ring C 헤더 부재로 불가했다. **CI windows-latest 결과로 확인하지 않은 채 머지하지 말 것.**

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵
| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| RT-1 | rc를 바꾸지 않아 앱이 깐 `node`/`npm`/`claude`가 **사용자 터미널에서는 보이지 않는다**(앱 자식 PATH에만 있음). 같은 앱이 "터미널에서 직접 실행" 안내(`plan_table.rs` copyable_command, `open_manual_instruction`)도 내보낸다. | 개발 도구 화면에 "터미널에서도 쓰기" 1회 동의 액션을 둔다. `~/.zprofile`에 표식 블록 한 줄(`export PATH="$HOME/.local/share/malgn-agent/npm-global/bin:$HOME/.local/share/malgn-agent/node/current/bin:$PATH"`)을 넣고, 같은 화면에서 되돌릴 수 있게 한다. 또는 최소한 안내 문구에 그 한 줄을 복사 가능하게 보여준다. | "rc 불변"은 안전 기본값으로 옳다. 다만 사용자는 Claude Code를 터미널에서 쓰는 경우가 많아, 앱 밖에서는 "설치했는데 command not found"가 된다. 동의형 opt-in이면 기본 불변식을 지키면서 막다른 골목을 없앤다. | 저~중. 표식 블록 추가·제거 + 동의 문구. rc 쓰기라 별도 Sensitive 리뷰가 필요하다. |
| RT-2 | `npm_config_prefix` 주입이 dev_tools 실행 한 지점에만 있다(RV-002). | "앱 관리 Node 사용 중이면 모든 자식에 같은 env 세트(PATH + prefix)"를 만드는 단일 빌더 하나(`build_child_env`)로 PATH와 prefix를 함께 일원화한다. | 지금은 PATH(전역 일원화)와 prefix(국소)의 적용 범위가 다르다. Claude 자동 업데이트·MCP `npx -g` 같은 앱 밖 npm 호출이 다른 prefix로 새는 부류 전체를 구조적으로 닫는다. | 저. dev_tools 밖 6개 모듈(claude_auth·plugins 2·mcp_manager·autonomy·session_chat)이 이미 `build_child_path_env`를 쓴다(grep -rln 확인). |
| RT-3 | npm JS(약 230MB 중 대부분)의 신뢰 근거는 "같은 출처의 HTTPS SHASUMS" 하나다. codesign은 `bin/node`만 덮는다. | 차기에 `SHASUMS256.txt.sig`(Node 릴리스 키 OpenPGP 서명)를 검증하고 키 지문을 앱에 고정한다(순수 Rust `pgp`/`sequoia` crate 또는 `/usr/bin` 밖 도구 불사용 원칙 유지). | nodejs.org 오리진·CDN이 침해돼 "진짜 서명된 node + 변조 npm + 갱신된 SHASUMS"로 재포장되는 시나리오를 막는다. 지금은 이 경우 codesign이 통과한다. | 중. 의존성 추가와 키 교체 운영 부담. 아래 트레이드오프에서 지금은 보류를 권고한다. |

## 트레이드오프 (페르소나 간 충돌)
- **GPG 미검증(불변식 7)**: 신뢰체인 감사자는 "npm JS는 서명 밖"(RT-3)이라고 보고, 발산형과 운영 관점은 "사내 <50명 + 비용 비례"(메모리 규약)라고 본다. → **권고: 수용.** 근거는 셋이다. ① 동일 위협에서 Homebrew·nvm·공식 설치 안내 대부분도 HTTPS 체크섬 수준이다. ② 오리진 침해는 nodejs.org 전 사용자 규모의 사고라 탐지·공지가 빠르다. ③ 실행 파일 자체는 독립 서명으로 막힌다. 수용 결정을 hub `decision_record`로 남기고, RT-3을 재평가 조건(외부 배포·사용자 규모 확대·Node 오리진 사고 공지)과 함께 백로그에 둔다.
- **rc 불변 대 사용성(RT-1)**: 보안 관점은 불변을 지지하고 발산형은 터미널 가시성을 요구한다. → 기본값은 불변으로 유지하고, 동의형 opt-in으로 푼다.

## 잘 된 점
- **검증된 것만 담는 타입**(`NodeVersion`: 재조립 문자열이 원문과 같아야 생성 — `types.rs:64-110`)으로 버전 문자열을 URL·경로·argv에 쓰는 경로 전부를 한 지점에서 막았다.
- **동의와 실행의 일치**: 설치 버전을 plan_id에 묶어(`app_node.rs:159-164`, `actions.rs:305-318`) 미리보기 뒤 새 LTS가 나오면 재미리보기를 요구한다. 사용자가 본 버전과 설치 버전이 갈라지지 않는다.
- **부수효과 주입 경계**(`InstallEnv`)와 실제 FS·bsdtar를 쓰는 실패 주입: rename 원자성을 가짜 FS가 아니라 실제 FS로 증명한다.
- **실제 홈 오염 0을 증거로 남김**: before/after 스냅숏과 샌드박스 HOME 분리. 이번 리뷰의 테스트 실행 뒤에도 재확인했다.
- 해시를 디스크에서 다시 읽기, 리다이렉트 정책을 첫 URL에도 적용, Content-Length 사전·사후 대조, 정체(stall)와 전체 제한시간 분리.

## 평가기준 충족 현황
| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| T1 신뢰 근거 존재·fail-closed | 신뢰체인 | 필수 | ✅ | 해시·Team ID·체인 모두 실패 시 중단 |
| A1 rename 반영 | 원자성 | 필수 | ✅ | |
| A2 되돌림 + 주입 테스트 | 원자성 | 필수 | ✅ | rename 실패 4종 |
| A3 크래시 잔여물 정리 | 원자성 | 필수 | ✅(부분) | N-1·N-2 |
| A4 tar 목록+트리 검사 | 원자성 | 필수 | ✅ | |
| A5 멱등 = 검증 통과 | 원자성 | 필수 | ✅ | 범위 한정(RV-005) |
| 절대경로·셸 없음·PATH 고정 | 프로세스 | 필수 | ✅ | |
| env 주입 범위 최소 | 프로세스 | 필수 | ✅ | 부작용 RV-002 |
| 순서·실패 단언 | 주장대비검증 | 필수 | ✅ | RV-003 |
| Windows 회귀 없음 | 이기종 | 필수 | ⚠ | CI 확인 대기 |

## 실행 경계
이번 리뷰에서 소스 수정, 커밋, push, 실제 홈 변경, 네트워크 다운로드, #[ignore] 테스트 재실행은 **하지 않았다**. 작성·변경한 파일은 다음뿐이다(커밋 안 함): 이 보고서, 신규 페르소나 1개, 재사용 페르소나 5개의 적용 이력 append, `personas/INDEX.md`(행 1개 추가, 재사용 5행 갱신). Windows 교차 확인이 만든 `src-tauri/target/x86_64-pc-windows-msvc`는 지웠다.

## PM에게 권고
1. **머지 가능(Green)**. 조건 하나: CI `rust-test (windows-latest)` 녹색 확인(로컬 교차 컴파일 불가).
2. 취소 UI(버튼)를 붙이는 후속 작업 전에 **RV-001을 반드시 선행**한다. 지금은 잠복 상태지만 버튼을 다는 순간 "한 번 취소하면 재시작 전까지 Node 설치 불가"가 된다.
3. RV-002는 샌드박스 1회 실측(`claude update`의 설치 위치)으로 확인한 뒤, 어긋나면 RT-2 방식으로 고친다.
4. RV-003·004·005와 Nit은 다음 라운드 백로그에 넣는다(모두 저비용. 근거: RV-003은 네트워크가 필요 없는 테스트 1개, RV-004·005는 문구 수정).
5. 불변식 7(GPG 미검증) 수용을 hub decision으로 기록하고 RT-3 재평가 조건을 함께 남긴다.
