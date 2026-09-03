# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## 새 세션 부트스트랩 (읽기 순서 = 토큰 예산)
- **L0 (자동 주입):** `STATUS.md`(라이브 상태, **3,000바이트 이내 유지** — 토큰은 세션에서 셀 수 없어 지킬 수단이 없지만 바이트는 셀 수 있다. 3,000바이트면 전부 한글이어도 1,000토큰 안에 들어온다. 고친 직후 크기를 검사한다 — 검사 커맨드는 malgn-agent의 `project-standards` 스킬 §3이 정본이고, 세션에 "STATUS.md 크기 확인해줘"라고 요청하면 그 스킬이 실행한다) + 이 `CLAUDE.md`(구조·규칙). → 대부분의 경우 이것만으로 충분.
- **L1 (필요할 때만 호출):** malgnai-hub `project_get_context`(project_id) 등 — L0로 충분하면 호출하지 않는다. 불필요한 호출은 토큰 낭비.
- **L2 (깊은 작업만):** `docs/README.md` 지도 → 필요한 문서만.

**STATUS.md 재작성은 다음 6가지 상황으로 제한한다** — 그 외 평범한 진행 중에는 건드리지 않는다:
①중요한 작업 완료 ②WBS 단계 변경 ③중요한 설계 결정 ④blocker 발생/해결 ⑤세션 종료 ⑥context compact 직전.
그 외에는 malgnai-hub `work_record`/`decision_record`/`issue_record`에만 기록하고 STATUS.md는 그대로 둔다 — STATUS.md는 "현재 스냅숏"이지 "매 턴 로그"가 아니다.

**필수 규율:** 주요 결정/이슈/교훈은 malgnai-hub에 기록.

## Project Overview
malgn-vscode — 사내 개발자 워크스테이션 프로비저닝 VSCode 확장. malgn-agent의 GUI 프론트엔드.
설계 정본은 `docs/architecture.md`(**통독 금지** — 목차로 필요한 절만).

## Tech Stack
TypeScript + esbuild + Vitest, pnpm 전용. 정본은 `docs/tech-stack.md`.

## Architecture
- `src/providers/` — 모든 provider가 `detect/plan/apply/verify` 4단계 균일 인터페이스를 따른다. `apply()`는 `ConsentToken`을 **타입으로** 요구한다 — 동의 없는 변경 금지를 코드리뷰가 아니라 타입체커가 지킨다. 이 시그니처를 느슨하게 만들면 그 위의 안전장치가 전부 무의미해진다.
- `src/core/policy/` — 원격 정책의 **신뢰 경계**. 정책은 제안만 하고(PR-4) 좁히기만 가능하며(PR-9) 권능 상한은 코드 상수다(PR-10). **키·행·값의 부재는 통과가 아니라 차단이다** — 이 결함이 세 번 재발했다.
- `compat/` — 코드 상수 fixture(빌드에 번들). 정본은 `docs/policy-contract.md`이며 정책이 이 값을 넓힐 수 없다.

## git 워크플로
- 로컬에서 검토·병합 후 `origin main`에 직접 push한다. GitHub 강제 브랜치 보호·CODEOWNERS를 쓰지 않는다.
- 고위험 표면의 게이트는 HRS(서명·표면검사 CI·force-push 금지·증거 바인딩)로 집행한다 → `docs/architecture.md` §0.3.1·§7.3.1.
