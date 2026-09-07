// SIGN-R2 (docs/release-gates.md §7.6.2) — "릴리스 빌드는 인증서 해시가 직전 릴리스와
// 같은지 확인한다. 기대 해시를 빌드 시 상수로 두고 서명 후 대조, 불일치면 빌드 실패
// (fail-closed)." 이 파일이 그 판정의 정본이다.
//
// [왜 필요한가] G-2(자기서명 인증서) 분기에서 Squirrel.Mac의 designated requirement는
// **인증서 해시**다(빌드 내용과 무관하게 고정). 인증서가 바뀌면 이미 설치된 앱이 새
// 업데이트를 거부한다 — "빌드 1회 실패"가 아니라 "자동업데이트 채널 영구 정지"다. 그런데
// 지금 이것을 강제하는 장치가 0이다(release-gates.md §7.6.2 정직 표기). 이 모듈은 그
// 신설 대상 중 대조 로직 자체를 만든다.
//
// [지금 무엇이 실측 가능하고 무엇이 조건부 대기인가] 자기서명 인증서가 아직 발급되지
// 않았다(`compat/codesign-cert.json`의 `expectedCertificateHashSha256`가 null). 그래서
// 이 함수는 "기대값 없음"을 **통과가 아니라 명시적 실패 사유**로 던진다 — 최초 발급 시
// 사람이 이 상수를 채우는 것을 베이스라인 확립 행위로 강제하기 위해서다(§7.6.2 표
// SIGN-R2 "불일치면 빌드 실패"의 부분집합: "기준 자체가 없는데 서명하려는 것"도 실패
// 대상이다 — 조용한 통과를 만들지 않는다).

export class CertificateHashBaselineMissingError extends Error {
  constructor() {
    super(
      'SIGN-R2: compat/codesign-cert.json의 expectedCertificateHashSha256이 설정되지 않았습니다 — ' +
        '최초 서명 인증서 발급 시 그 인증서 해시를 이 상수에 기록해 베이스라인을 확립하십시오 ' +
        '(docs/release-gates.md §7.6.2). 기준 없이 서명 릴리스를 낼 수 없습니다.'
    );
    this.name = 'CertificateHashBaselineMissingError';
  }
}

export class CertificateHashMismatchError extends Error {
  constructor(public readonly expected: string, public readonly actual: string) {
    super(
      `SIGN-R2: 서명 인증서 해시 불일치 — 기대 ${expected} / 실제 ${actual}. ` +
        '무심코 인증서를 재발급했다면 자동업데이트 채널이 이미 정지 위험 상태입니다. ' +
        '의도된 교체라면 docs/release-gates.md §7.6.2 SIGN-R3(교체는 릴리스가 아니라 사건이다) ' +
        '절차를 밟은 뒤 compat/codesign-cert.json을 의도적으로 갱신하십시오.'
    );
    this.name = 'CertificateHashMismatchError';
  }
}

/**
 * 릴리스 서명 직후 호출 지점 — 실제로 서명에 쓰인 인증서 해시(`actualHashSha256`)를
 * 빌드 시 상수(`expectedHashSha256`, `compat/codesign-cert.json`에서 옴)와 대조한다.
 * 기대값이 없거나(null) 값이 다르면 던진다(fail-closed). 통과하면 아무것도 반환하지
 * 않는다(호출자는 예외가 없으면 빌드를 계속 진행한다).
 */
export function assertCertificateHashMatches(expectedHashSha256: string | null, actualHashSha256: string): void {
  if (expectedHashSha256 === null || expectedHashSha256.trim().length === 0) {
    throw new CertificateHashBaselineMissingError();
  }
  if (expectedHashSha256 !== actualHashSha256) {
    throw new CertificateHashMismatchError(expectedHashSha256, actualHashSha256);
  }
}
