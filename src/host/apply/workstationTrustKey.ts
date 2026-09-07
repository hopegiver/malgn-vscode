// 순수 함수만 — `wireApplyMenu.ts`가 Electron(`Menu`/`Tray`/`powerMonitor`)을 top-level에서
// import하기 때문에(다른 host/electron 어댑터들과 같은 이유로 vitest 대상이 아니다) 단위
// 테스트가 필요한 순수 조각만 이 파일로 분리한다.

/** §3.6.1 ③ — "대상 폴더" 개념이 없는 provider(agent·mcp는 워크스페이스에 매이지
 * 않는다)에 대해 이 원장을 "이 워크스테이션 자체를 신뢰하는가"로 재해석한다(설계
 * 갭 채움, 반환문에 명시 — 원문은 I-B처럼 폴더 단위 대상이 있는 provider만 상정했다).
 * 키는 홈 디렉터리 절대경로로 고정해 이 PC에서 항상 같은 항목을 가리키게 한다. */
export function resolveWorkstationTrustKey(homeDir: string): string {
  return homeDir;
}
