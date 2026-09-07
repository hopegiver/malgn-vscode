// U-3 (docs/architecture.md §3.6.2) — "서버 authority는 코드 상수. 정책·설정·환경변수·
// argv가 못 바꾼다. 개발용 오버라이드를 만들지 않는다 ... 개발 편의용 오버라이드가
// 곧 공격 경로다." `src/architecture-tests/updateChannelBoundary.ts`의 AT-U5가 이
// 식별자(`UPDATE_SERVER_AUTHORITY`)의 정의 지점이 저장소 전체에서 정확히 1개이고,
// 그 값에 재대입·환경변수·argv 유래 경로가 없음을 CI에서 강제한다 — 이 파일이 그
// 유일한 정의 지점이다. 개발 채널의 별도 상수는 `devUpdateChannel.ts`에 있다(같은
// 식별자를 두고 조건부로 고르지 않는다 — 그 자체가 "한 바이너리 안의 분기"다).
//
// [값 — 정직 표기] 실제 배포 호스트가 아직 프로비저닝되지 않았다(K-3 업데이트 서명
// 키도 마찬가지 — `publicKeys.ts` 참조). 존재하지 않는 서버를 코드가 가리키게
// 적으면 이 상수 자체가 거짓 표기가 된다. 그래서 예약 네임스페이스(RFC 2606,
// `compat/sensitive-classes.json`의 `reservedNamespaceAllowlist`와 같은 대역)를 쓴다 —
// 실제 배포 호스트가 정해지면 이 한 곳(정의 지점 1개)만 바꾼다.
export const UPDATE_SERVER_AUTHORITY = 'updates.example.com';
