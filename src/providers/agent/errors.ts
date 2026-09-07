// agent provider 오류/사유 코드 — architecture.md §3.2·§3.2.2·§3.2.3 정본. `policy/errors.ts`
// (정책 검증 코드)와 `consent/errors.ts`(동의 게이트 코드)에 이미 있는 두 코드
// (`MV_AGENT_TARGET_DENIED`)는 재정의하지 않고 그대로 재수출한다 — 이 provider가
// detect/plan/apply/verify 각 단계에서 새로 필요로 하는 코드만 여기서 신설한다.

export { MV_AGENT_TARGET_DENIED } from '../../core/policy/errors.js';

/** detect — `claude` 자체가 PATH에 없어 agent 상태를 전혀 읽을 수 없다 */
export const MV_AGENT_CLAUDE_UNAVAILABLE = 'MV_AGENT_CLAUDE_UNAVAILABLE';

/** plan — compat/compatibility.json의 known[] 목록에 verdict:'block-apply'로 등재된
 * 버전이 관측됨(§3.5 "known 결함으로 apply가 차단됩니다") */
export const MV_AGENT_KNOWN_VERSION_BLOCKED = 'MV_AGENT_KNOWN_VERSION_BLOCKED';

/** detect/plan — `claude mcp get`이 보고하는 마켓플레이스 등록 저장소가 O-1이 고정한
 * `malgnsoft/claude-plugins`와 다름(같은 이름으로 다른 저장소를 등록한 신호, high) */
export const MV_AGENT_MARKETPLACE_REPO_MISMATCH = 'MV_AGENT_MARKETPLACE_REPO_MISMATCH';

/** apply §3.2.2 ② — 마켓플레이스 항목 선언에 명령 실행형 키(`command`/`install`/`script`/
 * `headersHelper`) 또는 화이트리스트 밖의 미지 `source` 형태가 있어 `-y` 없이 중단 */
export const MV_AGENT_MARKETPLACE_DECLARES_COMMAND = 'MV_AGENT_MARKETPLACE_DECLARES_COMMAND';

/** apply §3.2.2 ④ — TOCTOU: 읽기 시점 마켓플레이스 체크아웃 HEAD와 install 완료 후
 * HEAD가 다름(탐지지 예방이 아니다) */
export const MV_AGENT_SHA_MISMATCH = 'MV_AGENT_SHA_MISMATCH';

/** apply — `marketplace add|update`/`plugin install|update`/`plugin enable` 중 하나가
 * 비정상 종료(exit!=0) 또는 타임아웃 */
export const MV_AGENT_APPLY_FAILED = 'MV_AGENT_APPLY_FAILED';

/** apply — 120초 전체 타임아웃(§3.2.2 "타임아웃 120초") */
export const MV_AGENT_APPLY_TIMEOUT = 'MV_AGENT_APPLY_TIMEOUT';

/** verify — apply 직후 `claude plugin list --json` 재확인이 기대 버전/활성 상태와
 * 어긋남 */
export const MV_AGENT_VERIFY_FAILED = 'MV_AGENT_VERIFY_FAILED';

// ---------------------------------------------------------------------------
// detect — 상태 보고 코드(사고 신호가 아니라 정상적으로 관측 가능한 상태들)
// ---------------------------------------------------------------------------

export const MV_AGENT_OK = 'MV_AGENT_OK';
export const MV_AGENT_NOT_INSTALLED = 'MV_AGENT_NOT_INSTALLED';
export const MV_AGENT_DISABLED = 'MV_AGENT_DISABLED';
export const MV_AGENT_MARKETPLACE_MISSING = 'MV_AGENT_MARKETPLACE_MISSING';
export const MV_AGENT_OUTDATED = 'MV_AGENT_OUTDATED';
