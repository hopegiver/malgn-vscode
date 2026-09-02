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
malgn-vscode — VSCode 확장 프로젝트

## Tech Stack
- (채우기)

## Architecture
- (코드를 읽으면 그대로 나오는 나열 말고 읽어도 모르는 것 — 디렉터리의 책임, 그렇게 나뉜 이유, 손대면 안 되는 곳 — 을 적는다)
