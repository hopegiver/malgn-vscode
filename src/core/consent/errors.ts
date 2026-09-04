// 동의 게이트 에러 코드 — architecture.md §1.2 원문 코드블록에 그대로 등장하는 두 상수.
// (§0.5 공통 규약 `MV_<PROVIDER|AREA>_<REASON>`을 따르는 "doc-exact" 상수 — 임의로 바꾸지
// 않는다. src/core/policy/errors.ts의 정책 에러 코드와 module 경계가 달라 별도 파일로 둔다.)

/**
 * ④ 만료(now >= expiresAt) — §1.2 "실패의 두 등급": 이것은 **정상 상황**이다
 * ("diff를 꼼꼼히 읽고 몇 분 뒤 누르는 정상 행동"). severity는 `info`로 별도 분류한다 —
 * ①②③⑤(사고 신호)와 같은 `high`로 묶으면 진짜 사고 신호의 변별력이 떨어진다.
 */
export const MV_CONSENT_EXPIRED = 'MV_CONSENT_EXPIRED';

/**
 * ①providerId 불일치 · ②diffHash 재계산 불일치 · ③extensionVersion 불일치 · ⑤nonce 재사용 —
 * 넷 모두 사고 취급이라 severity `high`로 묶는다. 조용히 재요청하지 않고 로그·진단
 * 리포트에 남긴다(§1.2).
 */
export const MV_CONSENT_INVALID = 'MV_CONSENT_INVALID';
