// "마지막 요청만 유효" 게이트 — 비동기 응답이 순서 뒤바뀌어 도착해도 가장 최근에
// 시작한 요청의 응답만 반영하게 한다(예: 날짜를 빠르게 바꾸는 사용량 상세).
export interface LatestGate {
  /** 새 요청을 시작하고 그 요청의 토큰을 돌려준다(이전 토큰은 무효가 된다). */
  begin(): number;
  /** 진행 중인 요청을 모두 무효화한다(접기/화면 이탈 등). */
  invalidate(): void;
  /** 이 토큰이 아직 가장 최근 유효 요청인지. */
  isLatest(token: number): boolean;
}

export function createLatestGate(): LatestGate {
  let current = 0;
  return {
    begin: () => ++current,
    invalidate: () => {
      current++;
    },
    isLatest: (token) => token === current,
  };
}
