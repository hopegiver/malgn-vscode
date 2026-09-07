// mcp provider 오류/사유 코드 — architecture.md §5.2·§5.3 표 정본.

/** 플러그인(agent provider의 대상) 자체가 설치돼 있지 않음 — REQ-3로 위임(§5.3 표 2행) */
export const MV_MCP_AGENT_NOT_INSTALLED = 'MV_MCP_AGENT_NOT_INSTALLED';

/** 설치됐으나 `enabledPlugins`에 없음/false — `claude plugin enable`로 복구 가능(L2) */
export const MV_MCP_PLUGIN_DISABLED = 'MV_MCP_PLUGIN_DISABLED';

/** 활성이나 `claude mcp get`이 서버를 찾지 못함(전파 지연 등, 자동 조치 없음) */
export const MV_MCP_NOT_REGISTERED = 'MV_MCP_NOT_REGISTERED';

/** 활성이나 OAuth 미인증/만료 — 브라우저 로그인 안내만(L0, 토큰을 대신 갱신하지 않는다) */
export const MV_MCP_OAUTH_REQUIRED = 'MV_MCP_OAUTH_REQUIRED';

/** `claude mcp get`의 URL이 O-14와 다름 — 보고만, 자동 정정·삭제 금지(high) */
export const MV_MCP_HUB_URL_MISMATCH = 'MV_MCP_HUB_URL_MISMATCH';

/** health check 실패(연결은 됐다고 응답했으나 상태가 비정상) — 상태만 보고 */
export const MV_MCP_HEALTH_CHECK_FAILED = 'MV_MCP_HEALTH_CHECK_FAILED';

export const MV_MCP_OK = 'MV_MCP_OK';

/** apply — `plugin enable` 실행 실패 */
export const MV_MCP_ENABLE_FAILED = 'MV_MCP_ENABLE_FAILED';

/** verify — enable 직후 재확인 실패 */
export const MV_MCP_VERIFY_FAILED = 'MV_MCP_VERIFY_FAILED';
