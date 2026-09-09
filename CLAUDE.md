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
malgn-vscode("맑은에이전트") — 사내 개발자 워크스테이션 프로비저닝 **Tauri 데스크톱 앱**(macOS/Windows). malgn-agent의 GUI 프론트엔드.
2026-09-08 Electron 기반에서 Tauri로 전면 전환했다 — 이전 Electron 구현 전체는 `electron` 브랜치에 보존돼 있다.
설계 문서(`docs/`)는 전환 과정의 스캐폴딩 사고로 유실되어 아직 없다 — malgnai-hub 결정/이슈 기록이 당분간의 정본이다(project_id: `01m1gng9ppnm67283p189pq3t7`).

## Tech Stack
- 프론트엔드: TypeScript + Vite, `src/`에 뷰(`src/views/`)와 각 실기능별 API 모듈(`*Api.ts`)이 있다.
- 백엔드: Rust(`src-tauri/src/lib.rs`) — Tauri 커맨드로 로컬 파일시스템 읽기(`~/workspace`, `~/.claude/*`), 프로세스 실행(`claude`/`node`/`gh` 등 버전조회, 플러그인 업데이트), Google OAuth(PKCE+JWKS 검증) 등을 구현한다.
- `capabilities/default.json`은 fs/shell 플러그인을 쓰지 않고 커스텀 Rust 커맨드만 노출한다 — 권한 표면을 의도적으로 좁게 유지한다. 새 실기능을 추가할 때도 이 패턴을 기본으로 삼는다.
- pnpm 전용.

## git 워크플로
- 로컬에서 검토·병합 후 `origin main`에 직접 push한다. GitHub 강제 브랜치 보호·CODEOWNERS를 쓰지 않는다.
- 이 저장소는 **public**이다 — 시크릿(API 토큰 등)은 절대 소스에 리터럴로 커밋하지 않는다. 빌드타임에 필요한 값은 `src-tauri/.env`(gitignore됨, 로컬 전용) + `build.rs`의 `option_env!()` 패턴으로 주입하고, CI는 GitHub Actions repo secret을 쓴다(`google_oauth_login` 관련 값이 실례).
