# 설치형 릴리스 + GitHub Releases 자동업데이트 전환 리뷰 보고서

리뷰 페르소나 패널(5명 — 발산형 1명 포함):
`personas/persona-release-artifact-trust-chain.md`(신규) ·
`personas/persona-claimed-vs-verified.md` ·
`personas/persona-heterogeneous-machine-operator.md` ·
`personas/persona-delegated-authority-blast-radius.md` ·
`personas/persona-zero-base-redesigner.md`(발산형)

target_id: `malgn-vscode-installer-release-autoupdate` — 1차(최초 리뷰, **풀패널**)
등급: **Sensitive** — 노출 범위 축소 **금지** 건(배포 파이프라인·시크릿 자체 변경). 축소 판정을 적용하지 않았다.
리스크 범주: 배포 파이프라인 / 코드서명 / 자동업데이트 신뢰체인
리뷰 대상: 미커밋 워킹트리 전체 — `git diff`(12파일) + 미추적 신규 3파일
(`.github/workflows/tauri-release-build.yml`, `src-tauri/tauri.windows.conf.json`, `src/updateApi.ts`).
`.claude/worktrees/`는 지시대로 열지 않았다.
리뷰 일자: 2026-09-18
**종합 판정: 🔴 Red**

## 요약 (2분 규칙)
파이프라인의 기본기(액션 SHA 고정, 최소 permissions, 시크릿 비노출, `max-parallel:1`로 latest.json 레이스 구조적 제거, 태그↔버전 대조, 최소 capability)는 좋고, 사용자 지시 위반(macOS Gatekeeper 우회)도 **없다**. 그러나 **지금 이 상태로 릴리스하면 첫 릴리스 코호트의 자동업데이트가 영구 불능**이 된다 — `pubkey: ""`는 fail-closed로 안전하긴 하지만(보안 구멍 아님) 설치 후 교체 불가한 값이어서 그 빌드를 받은 단말은 이후 어떤 버전도 자동으로 못 받고, 실패가 **화면에도 로그에도 남지 않는다**(C1). 그리고 이번 변경은 빌드타임 시크릿이 박힌 설치 파일의 배포 채널을 "레포 인증 필요"(`upload-artifact`)에서 **"전 인터넷 공개"**(public 저장소의 public Release 자산)로 넓히는데, 그 허용 여부가 아직 누구도 판정하지 않은 미결 사항이고 Publish 후에는 회수 불가다(C3). 쟁점 2의 캐리오버 자동설치는 트레이드오프 이전에 **설계 전제 자체가 깨진 경로**가 있다 — 로그인 화면에서는 버튼이 렌더되지 않는데 플래그는 기록되므로, 버튼을 한 번도 본 적 없는 사용자가 다음 실행에서 예고 없이 재시작을 맞는다(M1).

## 페르소나 재사용 판정 (산출물 게이트)
착수 전 `docs/reviewer/personas/INDEX.md`(13행)를 Read해 "역할개념" 열만 대조했다.

| 페르소나 | 판정 | 사유 |
|---|---|---|
| `persona-release-artifact-trust-chain.md` | **신규** | INDEX 13행 중 "빌드 완료 **이후**의 아티팩트 신뢰·배포 채널"을 보는 행이 없다. 가장 가까운 `persona-build-profile-gatekeeper.md`는 역할개념이 "개발 전용 코드를 막는 게이트가 컴파일 체인에서 닫히는가"로, **컴파일 시점까지**를 본다. 이번 리스크(공개키·코드서명·배포 채널 열람 범위·매니페스트 완결성)는 빌드가 끝난 뒤에 발생하는 새 표면이라 경계가 명확히 갈린다. |
| `persona-claimed-vs-verified.md` | 재사용 | 역할개념("코드가 자기 주석과 일치하는가")이 그대로 과녁 — 이번 산출물은 주석이 크레이트·러너 동작을 단정으로 서술한다. 6대 요소 무수정, 적용 이력만 append. |
| `persona-heterogeneous-machine-operator.md` | 재사용 | 역할개념("우리 조직 이기종 머신에서 실제로 뜨는가, 막다른 골목인가")이 MSI/NSIS·WebView2·무서명 dmg 판정에 그대로 적용. 무수정. |
| `persona-delegated-authority-blast-radius.md` | 재사용 | 역할개념("권한의 크기를 사전에 알고 사후에 확인할 수 있는가")이 쟁점 2(예고 없는 설치·종료) 그 자체. 무수정. |
| `persona-zero-base-redesigner.md` | 재사용 | 발산형 최소 1명 게이트 담당. 무수정. |
| (미투입) `persona-build-profile-gatekeeper.md` | 투입 안 함 | 이번 diff는 `build.rs`·`#[cfg(debug_assertions)]` 게이트를 건드리지 않는다(`git diff`에 `build.rs` 없음). 신규 페르소나 T3 기준이 시크릿 주입 경로를 대신 커버한다. |
| (미투입) `persona-process-execution-security.md` | 투입 안 함 | argv/셸/PATH 조립이 이번 diff에 없다(업데이터는 플러그인 내부에서 설치 프로그램을 스폰). blast-radius 페르소나가 프로세스 수명 쪽만 인수했다. |

## 지적 사항 (통합)

| # | 심각도 | 관점 | 위치 | 확인방법 | 문제 | 개선안 |
|---|---|---|---|---|---|---|
| C1 | 🔴 | 신뢰체인 | `src-tauri/tauri.conf.json:43` (`"pubkey": ""`) | 크레이트 소스 대조: `~/.cargo/registry/src/*/tauri-plugin-updater-2.11.0/src/updater.rs:740,1524-1542` + `src/updateApi.ts:147-152` | 공개키는 **빌드에 굳어 박히는 값**(설치 후 교체 불가)이다. 빈 문자열이면 `verify_signature` → `base64_to_string("")`→`""` → `PublicKey::decode("")` 실패 → `download()`가 항상 Err. 따라서 이 빌드를 설치한 단말은 **이후 모든 버전을 영구히 못 받는다**. 게다가 프런트가 download 실패를 조용히 삼켜(`updateApi.ts:149-152`) 사용자·운영자 누구도 알 수 없고, 자동업데이트로는 고칠 수 없다(신뢰 근거 자체가 빈 값) — 전 단말 수동 재설치만이 복구 경로. **fail-closed라 보안 구멍은 아니다**(무서명·오서명 아티팩트를 설치하지 않는다). | 릴리스 전 필수 선행조치 4단계: ①`pnpm tauri signer generate` ②출력 공개키를 `pubkey`에 커밋 ③개인키(+비밀번호)를 `TAURI_SIGNING_PRIVATE_KEY(_PASSWORD)` secret 등록 ④`verify-version` 잡에 `pubkey` 비어있음 검사 추가(`jq -er '.plugins.updater.pubkey | length > 0'`)해 같은 실수가 다시 태그를 통과하지 못하게 한다. |
| C3 | 🔴 | 신뢰체인 | `.github/workflows/tauri-release-build.yml:138-141`, `:246-248` + `src-tauri/build.rs:16-50` + `CLAUDE.md`("이 저장소는 **public**이다") | `build.rs`·워크플로·기존 워크플로(`tauri-portable-build.yml:159,186`) 대조 | 빌드타임에 4개 값(`GOOGLE_OAUTH_CLIENT_SECRET`, `GOOGLE_MCP_OAUTH_CLIENT_ID/SECRET`, `MALGN_OTEL_COLLECTOR_BASE`)이 `cargo:rustc-env`로 **바이너리에 컴파일된다**. 기존 포터블 파이프라인은 산출물을 `actions/upload-artifact`(레포 인증 필요)로만 올렸으나, 이번 워크플로는 **public 저장소의 public Release 자산**으로 설치 파일을 게시한다 → 이 값들의 노출 범위가 "레포 협업자"에서 "전 인터넷(누구나 다운로드 후 `strings` 1회)"으로 확장된다. Publish 이후엔 릴리스를 지워도 회수 불가. 이 확장이 허용되는지에 대한 판정 기록이 diff·주석·릴리스 노트 어디에도 없다. | 사람 승인 전에 **명시적으로 결정**할 것. 선택지: (a) 네 값이 "공개돼도 무해"임을 판정·기록하고 진행(데스크톱 OAuth 클라이언트 시크릿은 RFC 8252상 기밀이 아니지만, `MALGN_OTEL_COLLECTOR_BASE`가 가리키는 수집기에 인증이 있는지 별도 확인 필요), (b) 릴리스 자산·updater 엔드포인트를 사내 채널로 이전(R2), (c) 런타임 주입으로 전환. 어느 쪽이든 결정을 저장소에 남긴다. |
| M1 | 🟠 | blast-radius / 정직성 | `src/main.ts:263` + `src/main.ts:86-89` + `src/updateApi.ts:172` | 세 지점 호출사슬 손 추적 | 캐리오버 설계의 전제는 "**버튼을 보여준 시점**에 기록한다"인데, 실제로는 감지 시점에 무조건 기록한다. `renderApp()`은 `!state.authenticated`면 로그인 뷰만 그리고 **return**하므로 사이드바(=버튼)가 존재하지 않는다. 반면 `initUpdateCheck()`는 `bootstrap()` 끝에서 **인증 여부와 무관하게** 호출된다(주석 `updateApi.ts:182-186`은 "로그인 성공 후"라고 단언하지만 호출부가 다르다). 이 앱은 Google 로그인이 필수여서 **부팅 직후 = 로그인 화면**이 정상 흐름이다 → 버튼을 한 번도 보지 못한 사용자가 그냥 껐다 켜면 예고 없이 설치+재시작을 맞고, 그 타이밍은 하필 다음 로그인(OAuth 왕복) 진행 중일 가능성이 높다. | `writePendingVersion`을 "버튼이 실제로 렌더 가능한 상태"에서만 호출하도록 게이트(`state.authenticated`) 추가하거나, `initUpdateCheck()` 호출을 인증 성공 직후로 이동. 주석도 실제 호출부에 맞게 정정. |
| M2 | 🟠 | blast-radius | `src/updateApi.ts:156-169`(+`:35-38` 주석), `:137` vs `:148` | 코드 경로 추적 + PM 제공 크레이트 사실 | **쟁점 2 판정: 현재 형태의 캐리오버 자동설치는 수용 불가** — 단 요구사항 4를 포기할 필요는 없다. 사전 고지·취소 수단이 0이고, Windows에서 `install()`이 프로세스를 `exit(0)`시킨다. 주석은 "수 초 안에 끝난다"고 가정하지만 `download()`에는 **타임아웃도 진행 표시도 없다**(10초 타임아웃은 `check()`에만 붙어 있다) — 설치 파일 크기·망 상태에 따라 종료 시점이 부팅 후 수십 초~수 분으로 밀릴 수 있어 "사용자가 조작 전"이라는 가정이 코드로 보장되지 않는다. | 세 가지를 함께 적용하면 두 요구를 동시에 만족한다: (a) 적용 직전 **5초 비모달 카운트다운 토스트 + "나중에"**(누르면 버튼 경로로 강등 — 모달 아님, 요구사항 2 위반 아님), (b) 자동 적용 조건을 "다운로드 완료가 부팅 후 N초(예: 60초) 이내 **and** 그 사이 사용자 입력 이벤트 없음"으로 좁히고 그 외엔 버튼만 노출, (c) `state.*.saving` 계열 플래그가 켜져 있으면 연기. |
| M3 | 🟠 | 이기종 머신 | `src-tauri/tauri.windows.conf.json:4` (`targets: ["nsis","msi"]`) | `tauri-utils-2.9.3/src/config.rs:818-829` 실측 + 설치 모델 추론(latest.json 선택 규칙은 **미검증 추정**) | NSIS 기본 설치 모드는 `CurrentUser`(`%LOCALAPPDATA%`, UAC 불필요)이고 **MSI는 per-machine(Program Files, 관리자 필요)**다. 두 타깃 + `createUpdaterArtifacts:true`면 Windows 업데이터 아티팩트가 2개(`.nsis.zip`/`.msi.zip`) 생성되는데 `latest.json`의 `windows-x86_64` 항목은 하나만 담는다 → 어느 것이 실리는지 불확정. MSI가 실리면 자동업데이트마다 UAC 승격이 필요해 "조용한 업데이트"가 깨진다. 더 큰 위험: IT가 **MSI로 대량배포**(Program Files)하고 업데이터가 **NSIS(currentUser)**를 실행하면 두 벌이 병존하고, 바로가기는 구버전을 계속 띄워 **매 실행 업데이트 재감지 루프**에 갇힌다. | 업데이트 채널을 하나로 고정: `targets: ["nsis"]`. MSI가 IT 대량배포용으로 필요하면 별도 타깃/워크플로로 분리하고 "MSI 설치 단말은 자동업데이트 비대상"을 명문화한다. 어느 쪽이든 `bundle.windows.nsis.installMode`를 **명시**해(기본값 의존 금지) 의도를 코드에 남긴다. 검증 절차: 태그 1개로 파이프라인을 1회 돌려 draft의 `latest.json`에서 `platforms["windows-x86_64"].url`을 육안 확인. |
| M4 | 🟠 | 신뢰체인 | `tauri-release-build.yml:92`, `:115-125`, `:242-248` | 워크플로 전문 + 잡 구성 대조 | **쟁점 4 판정: draft→사람 Publish 게이트는 절반만 실효적이다.** 사람이 GitHub UI에서 확인할 수 있는 것은 파일 목록뿐이고, 정작 중요한 것(`pubkey` 비어있지 않음 / `.sig` 존재 / `latest.json`에 macOS·Windows **두 항목 모두** 존재 / Windows 서명 여부)은 눈에 보이지 않는다. 릴리스 노트도 "Actions 로그를 확인하세요"(:244)라고만 적어 판단을 사람의 성실성에 넘긴다. 게다가 `cancel-in-progress: true`(:92)는 순차 matrix 중간 취소 시 **한 플랫폼만 담긴 latest.json**이 draft에 남을 수 있고, 그 상태는 UI에서 구별이 거의 불가능하다. | (a) `release` 이후 `verify-release` 잡 추가 — `gh api`로 draft 릴리스의 `latest.json`을 받아 `platforms` 키 2개·각 `signature` 비어있지 않음·자산 내 `.sig` 존재를 검사하고 실패 시 워크플로 실패. (b) 릴리스 워크플로는 `cancel-in-progress: false`. (c) Windows 무서명이면 릴리스 제목/본문에 `[UNSIGNED]`를 워크플로가 **자동 기입**(현재는 로그에만 있어 놓치기 쉽다). |
| M5 | 🟠 | 이기종 머신 / 신뢰체인 | 설계 전반 — `tauri.conf.json:37-41`(엔드포인트), 워크플로에 롤백 절차 없음 | 업데이터 semver 비교 동작 + GitHub `/releases/latest` 시맨틱 | **쟁점 4(롤백) 판정: 되돌릴 경로가 없다.** 업데이터는 원격 버전이 현재보다 **클 때만** 적용하므로 구버전을 다시 Publish해도 이미 올라간 단말은 내려올 수단이 없고, `/releases/latest`는 최신 published 릴리스를 가리켜 "구버전 재게시 = 롤백"이 성립하지 않는다. 잘못된 릴리스가 전 단말에 퍼지면 각 단말 수동 재설치가 유일한 복구다. | (a) **롤포워드 전용** 정책을 문서화(잘못된 릴리스는 버전을 올린 수정판으로 덮는다). (b) 직전 정상 버전의 설치 파일을 릴리스 자산으로 상시 보존(수동 다운그레이드 경로 확보). (c) 전사 공지 전 IT 단말 1~2대 **파일럿 업데이트 왕복**을 필수 절차로 두고, 카나리아는 `prerelease: true`로 올려 `/latest`에 걸리지 않게 한다. |
| M6 | 🟠 | 신뢰체인 | `src-tauri/tauri.conf.json` `identifier: "com.malgn.vscode-mockup"` | `jq`로 실측 + 설치형 배포의 영속 상태 성격 | 포터블 배포에서는 identifier가 사실상 무해했지만, **설치형은 단말에 영속 상태를 남긴다**(macOS `CFBundleIdentifier`, Windows 설치 경로/언인스톨 레지스트리 항목). IT가 MDM Gatekeeper 예외를 이 bundle id로 등록한 뒤 나중에 `-mockup`을 떼면 **전 단말에서 예외 재등록 + 중복 설치 정리**가 필요해진다. 즉 이것도 "첫 배포 전에 확정해야 하는 값"이다. (Windows 레지스트리 키가 identifier 기반이라는 부분은 **미검증 추정** — macOS bundle id / MDM 쪽은 확실.) | 첫 설치형 릴리스 전에 identifier를 최종 값으로 확정해 커밋하고, IT에 전달하는 MDM 등록 값과 일치시킨다. 확정 뒤에는 변경 금지 항목으로 문서화. |
| m1 | 🟡 | 정직성 | `src-tauri/src/lib.rs:36-41` | `tauri-plugin-updater-2.11.0/src/updater.rs:740,1524-1542` 대조 | 주석의 단언 2곳이 사실과 다르다. ①"`check()`만 서명 검증 실패로 안전하게 no-op 처리된다" — `check()`는 `pubkey`를 전혀 참조하지 않는다(pubkey는 `download()`의 `verify_signature`에서만 쓰인다). 실제로 막히는 건 `download()`다. ②현재 엔드포인트 상태에서 `check()`는 no-op이 아니라 **reject**한다. 이 주석이 "빈 pubkey로도 문제없다"는 인상을 만들어 C1의 심각성을 가린다. | 주석을 실제 동작으로 정정: "빈 pubkey는 `download()`의 서명 검증에서 Err가 되어 업데이트가 **영구히 적용되지 않는다**(fail-closed). 릴리스 전 반드시 채워야 한다." |
| m2 | 🟡 | 정직성 | `src/updateApi.ts:182-186` | `src/main.ts:263` 대조 | "로그인 성공 후 정확히 한 번 호출한다"는 단언이 호출부와 불일치(인증 무관 호출). M1의 근본 원인 진술. | 주석과 코드 중 하나를 맞춘다 — M1 개선안과 동일 지점. |
| m3 | 🟡 | blast-radius | `src/updateApi.ts:52,137,148` | 코드 대조 | `download()`에 타임아웃·취소·진행 표시가 없다(10초 타임아웃은 `check()` 전용). 느린 망에서 사용자는 "무슨 일이 일어나는지" 알 수 없고, 그 세션 내 재시도도 없다 — 실패 시에도 `lastCheckedAt`을 갱신하므로(`:142-144`) 24시간 임계에 걸려 같은 세션에서 다시 시도하지 않는다. | `download(onProgress, AbortSignal)`로 타임아웃 부여 + 실패 시 `lastCheckedAt`을 갱신하지 않거나 짧은 백오프(예: 30분)로 분리. |
| m4 | 🟡 | 신뢰체인 | `src/updateApi.ts:138-141,149-152` | 실패 경로 전수 확인 | 업데이트 채널의 모든 실패가 조용히 삼켜진다. "강제 팝업 금지" 요구를 지키는 올바른 UX지만, **관측 수단이 전혀 없어** 전사가 조용히 구버전에 머무는 상황을 아무도 감지하지 못한다(C1이 7개월 늦게 발견되는 시나리오의 핵심 조건). | 사내 OTel(`MALGN_OTEL_COLLECTOR_BASE`)이 이미 있으므로 실패 사유별 카운터 또는 로그 1줄만 남긴다. 화면 방해 없이 IT가 채널 건강도를 볼 수 있다. |
| m5 | 🟡 | 이기종 머신 | `src-tauri/tauri.windows.conf.json:7` | 설정 값 확인 | `timestampUrl`이 단일 TSA(DigiCert)로 고정 — TSA 장애 시 `signtool`이 실패해 릴리스 빌드가 통째로 막힌다. 사내 CA 서명에서 타임스탬프는 신뢰상 필수가 아니다. | TSA 장애 시 대응(대체 TSA 또는 타임스탬프 생략 재실행)을 워크플로 주석/런북에 남긴다. |
| m6 | 🟡 | 정직성 | `tauri-release-build.yml:203-210,229-233` | 코드 확인, 실행 검증 불가 | `--config <파일>` 주입이 `bundle.windows`의 기존 키(`digestAlgorithm`·`timestampUrl`)를 **보존하며 병합**하는지 이 환경에서 확인할 수 없다(**미검증**). 덮어쓰기라면 타임스탬프·해시 설정이 조용히 사라진다. | 파이프라인 첫 실행 시 signtool 호출 로그에 `/td sha256`·`/tr`가 포함되는지 1회 확인하고 결과를 주석에 기록. |
| m7 | 🟡 | 이기종 머신 | `tauri-release-build.yml:245` | 릴리스 노트 문구 확인 | 무서명 macOS 산출물이 첫 실행에서 차단되는 사실이 **릴리스 노트 한 줄**에만 있다. 레포에 우회 로직을 넣지 않는 것은 지시대로 맞지만, 사용자가 받을 안내(IT의 MDM 처리 전제)가 저장소 어디에도 없어 파일럿 단계에서 혼선이 예상된다. | 레포에는 우회 로직 대신 "IT 전달 사항"(MDM Gatekeeper 예외 등록 대상 bundle id·경로)을 런북/릴리스 템플릿에 남긴다. |
| n1 | ⚪ | blast-radius | `src/sidebar.ts:207-217` | 코드 확인 | 업데이트 항목이 `div`+onclick으로 키보드 포커스·`aria`가 없다. 단 기존 `navItem`(`:220`) 선례와 동일하므로 이번 변경의 회귀는 아니다. | 사이드바 전반의 별건으로 백로그. |

## 기각된 지적
| 관점 | 지적 요지 | 처리 | 사유 |
|---|---|---|---|
| 이기종 머신 | "`InstallScope` 미설정으로 perMachine 설치가 되어 자동업데이트마다 UAC가 뜬다"(쟁점 3의 원 지적) | **기각(사실 오류)** — 단 실제 위험은 M3으로 재구성해 살렸다 | NSIS 기본 설치 모드는 `PerMachine`이 아니라 `CurrentUser`다 — `tauri-utils-2.9.3/src/config.rs:818-829`에서 `NSISInstallerMode`의 `#[default]`가 `CurrentUser`이고 "Administrator access가 필요하지 않은 디렉터리에 설치, 메타데이터는 HKCU"라고 명시. 즉 NSIS 경로만 쓰면 UAC는 뜨지 않는다. 진짜 문제는 `msi` 타깃이 함께 켜져 있는 것(M3). |
| 이기종 머신 | "`webviewInstallMode` 미설정이 문제" | **강등(⚪ 정보)** | 기본값은 `DownloadBootstrapper { silent: true }`(`tauri-utils-2.9.3/src/config.rs:998-1002`) — 조용히 설치되고 설치 시 인터넷만 필요하다. 사내 환경(상시 온라인, Win11은 WebView2 선탑재)에서 수용 가능. 오프라인 설치 요건이 생기면 그때 `offlineInstaller`로 명시. |
| 신뢰체인 | "`process:allow-exit` 미부여로 재시작이 실패할 수 있다" / "`updater:default`가 download·install을 못 덮을 수 있다" | **기각** | `tauri-plugin-updater-2.11.0/permissions/default.toml` 실측: `updater:default`는 `allow-check`·`allow-download`·`allow-install`·`allow-download-and-install` 4개를 모두 포함한다. 코드가 쓰는 것은 `relaunch()`뿐이므로 `process:allow-restart`만으로 충분하고 `allow-exit` 제외가 맞다 — 오히려 잘 된 점으로 기록했다. |
| 릴리스 운영 | "`workflow_dispatch` 입력이 태그 형식인지 검증하지 않아 브랜치명으로 릴리스를 만들 수 있다" | **강등(⚪)** | 손 추적 결과 사실상 막힌다 — 입력이 `main`이면 `TAG_VERSION="main"`이 되어 `tauri.conf.json`의 `0.1.0`과 불일치하고 `verify-version`(`:122-125`)이 실패한다. 형식 정규식 한 줄을 추가하면 의도가 더 분명해지겠으나 실효 위험은 없다. |

## 페르소나별 관점

### [릴리스 아티팩트 신뢰체인 감사자] — 판정: 🔴 Red
T1 **미충족**(C1: `pubkey` 빈 값 — fail-closed임은 크레이트 소스로 확인했으나, 신뢰 근거 자체가 없는 빌드를 내보내는 것은 T1 정의상 실패), T2 **미충족**(설치 후 교체 불가 값 3종 중 공개키 미확정·identifier `-mockup` 잠정 — M6), T3 **미충족**(C3: 배포 채널 열람 범위가 내장 값의 허용 노출 범위보다 넓고, 그 허용 범위가 판정되지 않았다), T4 **충족**(시크릿은 `secrets.*`로만 참조, `echo` 없음, PFX 임시 파일을 임포트 직후 삭제 — `:196,201`), T5 미충족(M4), T6 미충족(m4), T7 미충족(M5).

**설치 후 교체 불가 — 첫 배포 전 확정 필요 목록**

| 값 | 위치 | 현재 | 확정 여부 |
|---|---|---|---|
| updater 공개키 | `tauri.conf.json:43` | `""` | ❌ **미확정 — 릴리스 차단 사유** |
| 번들 identifier | `tauri.conf.json` | `com.malgn.vscode-mockup` | ❌ 잠정값으로 보인다(M6) |
| updater 엔드포인트 | `tauri.conf.json:37-40` | `github.com/hopegiver/malgn-vscode/releases/latest/download/latest.json` | ⚠️ C3/R2의 채널 결정에 종속 — 나중에 사내 채널로 옮기려면 재배포 필요 |

### [정직성 감사자 (claimed ≠ verified)] — 판정: 🟠 Amber
V4 **미충족** — 사실 오류 2건(m1)과 주석·호출부 불일치 1건(m2)이 있고, 그중 m1은 단순 오기가 아니라 **C1의 위험을 축소해 보이게 만드는** 방향의 오류다. 반면 워크플로 주석은 모범적이다: 액션이 "대신 해주지 않는" 3항을 소스 실측으로 확인했다고 밝히고, 확인 못 한 항목은 스스로 `— claimed`로 표기했다(`:24`). 그 표기 덕분에 리뷰에서 무엇을 다시 봐야 하는지가 즉시 드러났다(m6이 그 항목).

### [이기종 머신 운영 현실주의자] — 판정: 🟠 Amber
O1 충족(설치 모드 판별이 설정으로 추적 가능), O2 **미충족**(M5: 잘못된 릴리스를 받은 단말은 수동 재설치 외 출구가 없다 — 전형적 막다른 골목), O3 **미충족**(M3: MSI 설치 + NSIS 업데이트 조합에서 매 실행 같은 업데이트를 재감지하는 루프), O4 충족(오탐 경고 없음). 조직 단말 관점에서 가장 큰 미지수는 "IT가 MSI로 깔 것인가 NSIS로 깔 것인가"이고, 그 답이 정해지지 않은 채 두 타깃이 모두 켜져 있다.

### [위임 권한 blast-radius 감사자] — 판정: 🟠 Amber
D3(실행 유발 직전 고지) **미충족** — 캐리오버 경로는 고지·취소가 0이고(M2), D6(화면 밖에서 도는 실행의 인지·중단) **미충족** — 다운로드 중 표시도 취소도 없다(m3). 반면 버튼 클릭 경로는 이 페르소나 기준으로 깔끔하다: 사용자가 직접 트리거하고, 중복 클릭을 `state.update.installing`과 함수 내부 가드로 이중 차단하며(`sidebar.ts:214-215`, `updateApi.ts:119`), 실패만 토스트로 알린다. 즉 **문제는 자동 경로 하나에 집중돼 있다** — 그래서 M2의 개선안도 그 경로만 손대면 된다.

## 구조적 제언 (Rethink) — 발산형 페르소나 🔵

| # | 현재 구조 | 제안 구조 | 왜 더 나은가 | 예상 비용/리스크 |
|---|---|---|---|---|
| R1 | localStorage `pendingVersion` 플래그로 세션을 넘나드는 상태기계 + 부팅 시 무고지 자동설치 | 캐리오버를 **삭제**하고 평소엔 버튼만. 대신 서버(이미 연동된 malgnai-hub)가 내려주는 `minSupportedVersion`보다 낮을 때만 차단형 화면("업데이트가 필요합니다" + 버튼)으로 승격 | 이 상태기계는 플러그인에 없는 능력(install-on-quit)을 프런트에서 흉내내려는 **부산물**이다. 지워도 남아야 하는 것은 한 문장 — "직원이 구버전에 오래 머물지 않는다". 서버 값 하나면 그 목적을 더 확실히 달성하면서 예고 없는 종료가 **구조적으로 사라지고**, 급한 보안 릴리스는 IT가 값 하나로 강제할 수 있다(지금은 강제 수단이 없다) | 서버 필드 1개 + 프런트 분기 1개. **비용 근거**: malgnai-hub 연동이 이미 존재(`CLAUDE.md` L1 부트스트랩)하므로 새 인프라가 필요 없다 — 단 서버측 필드 추가 범위는 미확인 추정. 포기하는 것: "언제나 조용히 최신" → 대신 IT가 통제권을 갖는다 |
| R2 | 사내 전용 도구의 설치 파일 + 빌드타임 시크릿을 **공개** GitHub Releases로 배포 | 릴리스 자산과 updater 엔드포인트를 사내 채널(MDM/사내 파일서버/사설 오브젝트 스토리지)로 옮기고, GitHub은 소스·태그·빌드만 담당 | "사내 50인 전용"이라는 제품 전제와 "전 인터넷 공개 배포"라는 채널이 **구조적으로 모순**이다. `build.rs`가 값을 바이너리에 굳히는 설계는 채널이 좁다는 가정 위에 서 있었는데, 이번 변경이 그 가정만 조용히 바꿨다(C3). 채널을 옮기면 C3·m7이 동시에 사라지고, 저장소를 나중에 private으로 돌릴 자유도 생긴다(현재 엔드포인트는 public이어야만 동작한다) | updater는 임의 HTTPS 엔드포인트를 지원하므로 기술 장벽은 낮다. 포기하는 것: `tauri-action`이 대신 해주던 릴리스 생성·`latest.json` 병합 편의 → 업로드 스텝을 직접 써야 한다(소~중). 사내 호스팅 운영 부담은 신규 |
| R3 | "사람이 draft를 Publish한다"를 승인 게이트로 사용 | 게이트를 **체크리스트 통과 + 파일럿 1대 실제 업데이트 왕복 확인**으로 재정의하고, Publish는 그 체크리스트의 마지막 버튼으로만 남긴다 | 지금 게이트가 붙잡는 것은 "사람이 UI를 봤다"이고, 정작 중요한 것(공개키·서명·매니페스트 완결성·실제 왕복)은 그 화면에서 **보이지 않는다**(M4). 게이트를 옮기면 승인이 의식(ceremony)에서 실제 검증으로 바뀐다. 이 저장소는 2차 리뷰어가 없다는 전제(솔로 워크플로)이므로, 통제는 사람이 아니라 기계 게이트에 걸어야 한다 | `verify-release` 잡 1개 + 런북 1페이지(소). 포기하는 것: 릴리스 1건당 수 분 추가 |

## 트레이드오프 (페르소나 간 충돌)
- **신뢰체인 감사자 ↔ 이기종 머신 운영자: Windows 무서명 계속 진행 정책(`:211-215`)**. 감사자는 "무서명 설치 파일이 draft에 올라갈 수 있고 사람이 Job Summary를 놓치면 그대로 배포된다"며 즉시 실패를 원한다. 운영자는 "인증서 발급 전에 파이프라인을 한 번도 못 돌려보는 게 더 위험하다"며 현 정책을 지지한다. → **권고: 현 정책 유지**(운영자 손). 대신 감사자의 우려를 기계로 흡수 — 무서명이면 릴리스 **제목**에 `[UNSIGNED]`를 워크플로가 자동으로 붙여 눈으로 놓칠 수 없게 한다(M4-c). 근거: 이 정책의 위험은 "모른 채 배포"인데, 그건 경고의 위치를 로그에서 릴리스 제목으로 옮기면 사라진다.
- **blast-radius 감사자 ↔ 사용자 요구 4("껐다 켜도 결과가 같아야")**. 감사자는 예고 없는 종료를 D3 위반으로 본다. 요구 4는 무개입 도달을 원한다. → **권고: M2의 (a)+(b)+(c) 조합**. 5초 카운트다운은 모달이 아니라 토스트이므로 "강제 팝업 금지"(요구 2)를 위반하지 않고, "부팅 직후 미조작 상태"에 한정하면 요구 4의 실질(사용자가 아무 것도 안 해도 결국 최신)이 유지된다. 두 요구를 동시에 만족시키는 해가 존재하므로 이 충돌은 트레이드오프가 아니라 **설계 미스**로 처리해야 한다.
- **발산형 R2 ↔ 현 스코프**. 채널 이전은 이번 릴리스 범위를 넘는다. → **권고: 이번엔 C3의 (a)(판정·기록 후 진행)로 통과시키고, R2는 차기 결정 항목으로 등재**. 단 (a)를 고를 때 `MALGN_OTEL_COLLECTOR_BASE`가 가리키는 수집기에 인증이 있는지 확인은 이번에 해야 한다(공개 후 회수 불가).

## 잘 된 점 (유지할 패턴)
- **최소 권한 capability가 의도와 실제로 일치한다.** `updater:default`가 필요한 4커맨드를 정확히 덮고(`permissions/default.toml` 실측) `process:allow-restart`만 추가, `allow-exit`은 쓰지 않으니 제외 — 주석의 설명과 코드가 일치하는 드문 사례.
- **레이스를 재시도로 덮지 않고 구조적으로 제거.** `max-parallel: 1` + 업스트림 이슈 번호(tauri-action#1270) 인용(`:30-39`). `retryAttempts: 3`을 "그래도 방어적으로"로 명시해 역할을 혼동하지 않게 했다.
- **설정 실수를 침묵시키지 않는다.** PFX 시크릿 "하나만 등록"을 즉시 실패로 처리(`:216-219`) — 조용히 잘못 서명하다 알 수 없는 오류로 새는 경로를 선제적으로 닫았다.
- **fail-closed 신뢰체인 + 그것을 존중하는 프런트.** 서명 검증 실패 시 버튼을 아예 띄우지 않아(`:149-152`) 검증되지 않은 아티팩트가 사용자 선택지로 노출되지 않는다.
- **사용자 지시 준수(쟁점 5 판정: 통과).** 신규 워크플로 전문에 `xattr`·`spctl`·`APPLE_*`가 **전무**하고(전문 Read로 확인), 기존 `tauri-portable-build.yml`의 `xattr -cr` 안내문(`:15,118,122,150`)은 이번 변경이 **손대지 않았다**(`git status`에 해당 파일 없음 = 무변경). 목적이 다른 두 워크플로를 파일로 분리한 판단도 옳다.
- **주석이 "확인한 것"과 "주장"을 구분한다**(`:24`의 `— claimed`). 이 습관이 이번 리뷰에서 재검증 지점을 바로 찾게 해줬다.
- **버튼 경로의 이중 가드**와 조용한 실패 정책 — 요구사항 2(강제 팝업 금지)를 정확히 지켰다.

## 평가기준 충족 현황
| 기준 | 관점 | 중요도 | 충족 | 비고 |
|---|---|---|---|---|
| T1 서명 공개키 존재·fail-closed | 신뢰체인 | 필수 | ❌ | C1 — fail-closed는 확인, 값이 없음 |
| T2 설치 후 교체 불가 값 확정 | 신뢰체인 | 필수 | ❌ | C1·M6 |
| T3 채널 열람 범위 ≤ 내장 값 허용 범위 | 신뢰체인 | 필수 | ❌ | C3 |
| T4 서명 시크릿 비노출 | 신뢰체인 | 필수 | ✅ | `secrets.*`만, echo 없음, PFX 즉시 삭제 |
| T5 매니페스트 기계 검사 | 신뢰체인 | 권장 | ❌ | M4 |
| T6 실패 관측 수단 | 신뢰체인 | 권장 | ❌ | m4 |
| T7 롤백/롤포워드 정책 | 신뢰체인 | 권장 | ❌ | M5 |
| V4 주석 단언 = 실제 분기 | 정직성 | 필수 | ❌ | m1·m2 |
| V5 실패 시 진단 도달 | 정직성 | 권장 | ❌ | m4 |
| O1 설치방식 판별 추적 가능 | 이기종 | 필수 | ✅ | 설정으로 추적 가능 |
| O2 실패 시 다음 행동 존재 | 이기종 | 필수 | ❌ | M5 |
| O3 같은 실패 반복 루프 없음 | 이기종 | 필수 | ❌ | M3 |
| O4 오탐 경고 없음 | 이기종 | 권장 | ✅ | — |
| D1/D2 권한 범위 특정·명시 | blast-radius | 필수 | ✅ | capability 4개로 특정, 주석에 사유 |
| D3 실행 직전 고지 | blast-radius | 권장 | ❌ | M2 |
| D6 화면 밖 실행 인지·중단 | blast-radius | 권장 | ❌ | m3 |
| R1~R5 발산형 대안 강제 | 발산 | — | ✅ | R1~R3 모두 대안·포기항목 명시 |

## 화면 근거 (생략 사유 명시)
UI 변경이 포함돼 있으나 **스크린샷을 남기지 않았다** — 정직 보고: 업데이트 버튼은 `state.update.available === true`에서만 렌더되고(`sidebar.ts:207`), 그 상태를 만들려면 게시된 릴리스 또는 캡처 픽스처 주입이 필요하다. 이번 위임의 리스크 범주는 배포 신뢰체인이고 시각 변경은 CSS 3규칙(`styles.css:279-285`)이 기존 토큰·애니메이션을 재사용하는 수준이라, 캡처 대신 **코드 대조로 대체**했다: `live-pulse` 키프레임 실재(`styles.css:653`), `.sidebar-footer`·`clickable`·`showToast`(`dom.ts:190`) 실재 확인. 버튼이 실제로 어떻게 보이는지는 **미확인**이며, 릴리스 전 파일럿 단계에서 눈으로 확인할 것을 권고한다.

## PM에게 권고

**지금 이 상태로 사람 승인에 올리는 것은 권고하지 않는다**(승인·완료 선언은 하지 않는다 — 판단과 게이트는 PM 소관).

릴리스 **차단** 사유 2건 — 선행조치가 끝나야 승인 안건이 성립한다:
1. **C1** `pubkey` 생성·커밋·secret 등록 + 워크플로에 빈 값 검사 게이트 추가. (이 조치 없이 태그를 push하면 그 빌드를 받은 단말은 영구히 자동업데이트 불능이고, 자동업데이트로 고칠 수 없다.)
2. **C3** 빌드타임 4개 값의 인터넷 공개 허용 여부를 **사람이 결정하고 기록**. 최소한 `MALGN_OTEL_COLLECTOR_BASE` 수집기의 인증 유무는 이번에 확인해야 한다(Publish 후 회수 불가).

승인 안건에 올릴 때 **함께 처리 권고**(Major, 코드 수정은 담당 에이전트):
- **M1**(frontend-dev): 미인증 화면에서 캐리오버 플래그가 기록되는 경로 차단 — 저비용·고효과, C1과 독립.
- **M2**(frontend-dev): 캐리오버 자동설치에 카운트다운+취소, 부팅 직후 미조작 조건, saving 가드.
- **M3**(backend-dev/devops): Windows 업데이트 채널을 NSIS 하나로 고정 + `installMode` 명시. IT의 배포 방식(MSI vs NSIS) 결정이 선행 입력이다.
- **M4**(devops): `verify-release` 잡 + `cancel-in-progress: false` + `[UNSIGNED]` 자동 표기.
- **M5/M6**(PM 결정 + devops): 롤포워드 전용 정책 문서화·파일럿 절차 신설, identifier 확정.

Minor(m1~m7)는 같은 파일을 만질 때 함께 고치면 되고, m1·m2는 주석 정정만이라 M1 작업에 끼워 넣을 수 있다.

재검토 시: 이 문서를 직전 리뷰로 지정하고 `target_id`·리스크 범주를 그대로 유지하면 **증분/축소 모드**로 진행 가능하다(페르소나 5명 전원 재사용). 단 배포 채널을 사내로 옮기는 결정(R2)이 채택되면 새 리스크 표면이므로 증분 모드 + 신규 페르소나 1명이 필요하다.
