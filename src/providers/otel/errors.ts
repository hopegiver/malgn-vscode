// otel provider 오류/사유 코드 — architecture.md §4.2·§4.3 detect/복구 표 정본.
// `MV_POLICY_OTEL_*`(core/policy/errors.ts)는 "정책 문서가 거부됐다"는 뜻이고, 여기
// 코드들은 "이 PC의 실제 상태가 desired와 다르다/판단할 수 없다"는 뜻이다 — 같은
// 접두사(OTEL)를 쓰지만 서로 다른 계층의 신호라 재정의하지 않고 각자 신설한다.

/** §4.3 detect 표 1행 — 헤더 헬퍼 스크립트가 존재하지 않거나 실행 권한이 없음 */
export const MV_OTEL_HELPER_MISSING = 'MV_OTEL_HELPER_MISSING';

/** §4.3 detect 표 2행 — macOS가 아님(Windows/Linux는 R-2 실기 검증 전까지 blocked) */
export const MV_OTEL_HELPER_UNSUPPORTED_OS = 'MV_OTEL_HELPER_UNSUPPORTED_OS';

/** §4.3 detect 표 3행 — 헬퍼는 실행됐으나 exit!=0이거나 빈 Authorization을 반환함 */
export const MV_OTEL_SECRET_MISSING = 'MV_OTEL_SECRET_MISSING';

/** detect — `~/.claude/settings.json`을 읽거나 파싱할 수 없음(파일 없음은 "빈 env"로
 * 취급하고 이 코드를 쓰지 않는다 — 최초 실행의 정상 상태이기 때문이다) */
export const MV_OTEL_SETTINGS_UNREADABLE = 'MV_OTEL_SETTINGS_UNREADABLE';

/** detect — 코드 상수(allowedAuthorities.otel)가 비어 있어 desired 엔드포인트를 만들 수
 * 없음(구성 오류, PR-6 — 추측하지 않는다) */
export const MV_OTEL_NO_TARGET_CONFIGURED = 'MV_OTEL_NO_TARGET_CONFIGURED';

/** detect/plan/apply — 관측된 env가 desired와 완전히 같음 */
export const MV_OTEL_OK = 'MV_OTEL_OK';

/** detect — 하나 이상의 관리 대상 키가 desired와 다름(자동 판정, plan()이 실제 diff를
 * 만든다) */
export const MV_OTEL_DRIFT = 'MV_OTEL_DRIFT';

/** apply — 백업 또는 설정 파일 쓰기 실패 */
export const MV_OTEL_APPLY_FAILED = 'MV_OTEL_APPLY_FAILED';

/** verify — apply 직후 재확인이 desired와 어긋남 */
export const MV_OTEL_VERIFY_FAILED = 'MV_OTEL_VERIFY_FAILED';
