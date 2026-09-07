// U-1 (docs/architecture.md §3.6.2) + K-3/K-3′ (docs/release-gates.md §7.6) — 앱에
// 내장되는 업데이트 서명 검증 공개키. Ed25519, JWK "OKP/Ed25519" x좌표(base64url,
// 패딩 없음) 형태로 담는다 — node:crypto가 원시 32바이트 공개키를 DER 껍데기 없이
// 그대로 받는 유일한 표준 형태다(`crypto.createPublicKey({ format: 'jwk', ... })`).
// 새 런타임 의존성을 들이지 않기 위한 선택이다(node 내장 crypto만으로 충분).
//
// [지금 이 배열이 비어 있는 이유 — 정직 표기] K-3(업데이트 서명 키)가 아직 발급되지
// 않았다(release-gates.md §7.6 표). 이 슬라이스의 범위는 "키가 생기면 검증할 코드"를
// 만드는 것이지 "키를 발급하는 것" 자체가 아니다.
//
// **빈 배열은 "검증 생략"이 아니라 "전면 거부"로 해석된다.** `ed25519Verify.ts`의
// `verifyEd25519`는 keys가 0개면 무조건 예외를 던진다(fail-closed) — 이 파일은 값만
// 담고, 그 규율은 검증 함수 쪽이 강제한다. 이 프로젝트가 이미 두 번 겪은 실패 패턴
// (`verified` 키 부재가 fail-open으로 새는 것, 신호 생산자 없이 신뢰값이 `true`로
// 굳는 것)을 세 번째로 반복하지 않기 위한 명시적 설계다. K-3가 실제로 발급되면
// 이 배열에 항목을 추가하는 것이 그 발급을 코드에 반영하는 유일한 지점이 된다.
export interface UpdatePublicKey {
  /** 키 식별을 위한 사람이 읽는 라벨(로그·에러 메시지 전용). 검증 로직은 이 값을
   * 신뢰 판단에 쓰지 않는다 — 라벨을 위조해도 서명 검증 자체를 우회할 수 없다. */
  readonly label: string;
  /** JWK OKP/Ed25519 x좌표, base64url(패딩 없음). */
  readonly x: string;
}

/**
 * production 채널 공개키 목록. K-3(주 키) + K-3′(예비 키 — release-gates.md §7.6
 * "앱은 공개키를 2개 내장하고 둘 중 하나로 검증이 통과하면 수락한다")를 담을 자리다.
 * 지금은 둘 다 발급 전이라 배열이 비어 있다.
 */
export const UPDATE_PUBLIC_KEYS: readonly UpdatePublicKey[] = [];
