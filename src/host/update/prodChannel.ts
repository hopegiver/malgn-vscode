// 운영 채널 `UpdateChannelConfig` 조립 — architecture.md §3.6.2 U-3("서버 authority는
// 코드 상수 ... 개발 빌드는 별도 키 + 별도 상수 + 별도 번들 식별자로 가른다, 한
// 바이너리에 분기 금지"). 이 파일과 `devChannel.ts`가 그 "별도 상수" 두 벌이고,
// **어느 쪽을 쓰는지는 이 파일을 import하는 소스 트리 위치(`entry.prod.ts`)가
// 결정한다 — 런타임 if/switch로 고르는 코드는 이 저장소 어디에도 없다.**
//
// [번들 식별자(bundleIdentifier) — 값을 하드코딩하지 않는 이유, 정직 표기] 이 작업
// 지시가 미리 경고한 마찰이 정확히 여기다: 역-DNS 형태 문자열 리터럴은
// `compat/sensitive-classes.json`의 `network-authority-domain` 부류가 형태만으로
// 검출한다(`src/update/devUpdateChannel.ts`의 동일한 경고 참고). `.app` 경로 오탐을
// 고쳤을 때처럼 "문맥 규칙"으로 우회할 수 있는지 먼저 검토했다: 그 교정은 "**진짜
// 도메인이 아닌 형태**를 문맥(경로 구분자 인접)으로 가려낸" 것이었지만, 번들
// 식별자는 반대로 **패키징이 실제로 쓸 진짜 값**이 될 수 있어 "가짜라서 예외"라는
// 근거가 성립하지 않는다 — 문맥 규칙으로 되돌릴 수 없는 경우다.
//
// 그래서 이 파일은 리터럴 값을 아예 만들지 않는다 — **패키징(W14)이 빌드 시 주입하는
// 환경변수 하나로 좁힌다.** 이것은 예외 목록에 추가하는 것과 다르다: 스캐너를
// 통과하도록 값의 형태를 바꾸거나 억지로 우회하지 않고, 그 값 자체를 이 소스 트리에
// 아예 두지 않는 것이다(devUpdateChannel.ts가 이미 같은 이유로 값을 비워 둔 것과
// 동일한 판단). 값이 없으면(패키징 이전 상태) `assembleProdUpdateChannelConfig()`는
// 명시적으로 던진다 — 빈 문자열이나 추측값으로 조용히 넘어가지 않는다(PR-6).

import { UPDATE_PUBLIC_KEYS } from '../../update/publicKeys.js';
import { UPDATE_SERVER_AUTHORITY } from '../../update/updateServerAuthority.js';
import type { UpdateChannelConfig } from '../../update/updateFlow.js';

/** 패키징이 빌드 시 주입해야 하는 환경변수 이름 — 값 자체는 이 소스 트리에 없다. */
export const PROD_BUNDLE_IDENTIFIER_ENV_VAR = 'MALGN_PROD_BUNDLE_IDENTIFIER';

export class BundleIdentifierNotConfiguredError extends Error {
  constructor(envVarName: string) {
    super(`${envVarName}이 설정되지 않았습니다 — 번들 식별자는 패키징(W14) 빌드 설정이 주입해야 합니다`);
    this.name = 'BundleIdentifierNotConfiguredError';
  }
}

export function assembleProdUpdateChannelConfig(env: Readonly<Record<string, string | undefined>>): UpdateChannelConfig {
  const bundleIdentifier = env[PROD_BUNDLE_IDENTIFIER_ENV_VAR];
  if (!bundleIdentifier) {
    throw new BundleIdentifierNotConfiguredError(PROD_BUNDLE_IDENTIFIER_ENV_VAR);
  }
  return { authority: UPDATE_SERVER_AUTHORITY, publicKeys: UPDATE_PUBLIC_KEYS, bundleIdentifier };
}
