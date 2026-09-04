# malgn-vscode

사내 개발 환경을 표준 정책대로 맞춰 주는 VS Code 확장.

## 먼저 읽을 것 (새 PC / 신선 클론)

| 자리 | 무엇 | 추적 |
|---|---|---|
| `CLAUDE.md` | 개요·구조·규칙 | 추적 |
| `src/README.md` | 개발 가이드(설치·빌드·검증·민감값 분리) | 추적 |
| `docs/` | 설계 정본(`architecture.md` 외) | **비추적 — 클론에는 없다** |

`docs/`는 `.gitignore`에 있어 **다른 PC에서 클론하면 존재하지 않는다.** 신선 클론에서도
`pnpm install && pnpm run build`는 그대로 돌아간다(아래 사이트면 참고).

---

## ⚠️ `site/site.json` — 백업은 사람의 몫이다

`compat/compatibility.json`의 사내 인프라 값(수집기 authority·도메인·keychain 항목명)은
`{"$site":"..."}` **구멍**으로만 저장소에 들어 있다. 실값을 담은 **사이트면**
`site/site.json`은 **이 저장소에 없고 앞으로도 들어오지 않는다**(`.gitignore`의 `site/`,
영구 비추적). 빌드 시 `pnpm gen:site`가 구멍을 채운다 — 자세한 동작은 `src/README.md`
"민감값 분리".

**보관·배포 경로는 관리자(1인) 로컬 단독으로 확정됐다**(설계 근거: `docs/architecture.md`
§7.4.1-17). 사내 비밀 저장소는 지금 두지 않는다. 그 대가로 **파일을 잃으면 릴리스 빌드를
할 수 없다.**

### 백업 지시

- `site/site.json`을 만들었다면 **로컬의 안전한 곳(외장 디스크·암호화 볼륨 등)에 별도로
  한 벌 보관한다.** 저장소·이슈·채팅·붙여넣기 공유는 금지 — 그 경로로 새는 것이 이
  파일을 분리해 둔 이유다.
- PC 교체·초기화 전에 **백업본이 실제로 열리는지 먼저 확인하고** 옮긴다.

### 현재 상태 (사이트면 없음)

**이 저장소에는 아직 `site/site.json`이 만들어진 적이 없다.** 따라서 지금은 모든 빌드가
`compat/site.example.json`(예약 네임스페이스 값 전용)으로 채워진 **example면**이다:

- `src/generated/siteConstants.ts`의 `siteProfile`이 `'example'`.
- 개발·테스트·`compat:check`는 정상. 그러나 **어떤 provider도 `apply()`하지 못하고**
  (`src/core/policy/siteProfileGuard.ts`) **패키징이 실패한다**
  (`scripts/assert-site-profile-for-packaging.mjs`). 즉 **실 릴리스 빌드는 아직 불가**이고,
  사이트면을 만들기 전까지는 그것이 정상 동작이다.

### 유실 시 복구 경로

백업도 없이 사이트면을 잃었을 때, 순서대로:

1. **필요한 키 목록·형태는 항상 복구된다.** `compat/site.example.json`과
   `compat/contract-snapshot.json`의 `siteShape`가 추적되므로 "무엇을 채워야 하는지"는
   잃지 않는다. **잃는 것은 값뿐이다.**
2. **이전에 `siteProfile: 'site'`로 만든 산출물이 남아 있으면 값을 역추출한다** — `.vsix`,
   `dist/extension.cjs`, `src/generated/siteConstants.ts`에 번들 상수로 박혀 있다. 다만
   이들도 전부 비추적이라 **디스크가 통째로 날아간 경우에는 사이트면과 함께 사라진다.**
3. 그 경우 남는 경로는 **사내 인프라에서 값을 다시 확인해 재작성**하는 것뿐이다(수집기
   주소·도메인·keychain 항목명의 원천 확인).

**3까지 불가능하면 복구 불가다** — 릴리스 빌드를 되살릴 방법이 없다. 또한 3으로 재작성한
경우에도 **값이 원본과 같은지 검증해 줄 정본이 없다.** 형태 검사(검사 ⑪(b))는 키 개수와
배열 길이만 보므로 **틀린 값도 통과시킨다.** 재작성 후에는 사람이 값을 직접 대조해야 한다.
