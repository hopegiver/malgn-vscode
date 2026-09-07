// U-3 개발 채널 분기 요구(docs/architecture.md §3.6.2) — "개발 빌드는 별도 키 + 별도
// 상수 + 별도 번들 식별자로 가른다(한 바이너리에 분기 금지)." 이 파일은 그 세 가지의
// 개발 채널 값을 담는다. production 채널 값(`updateServerAuthority.ts`·`publicKeys.ts`의
// `UPDATE_SERVER_AUTHORITY`·`UPDATE_PUBLIC_KEYS`)과는 이름부터 다른 식별자를 쓴다 —
// 같은 식별자 하나를 두고 조건에 따라 값을 고르면 그 조건 분기 자체가 "한 바이너리
// 안의 분기"가 되어 U-3이 금지한 형태가 재현된다.
//
// [분기의 위치 — 소스 트리 레벨, 실행 코드 레벨이 아니다] 어느 채널로 동작할지는
// 이 모듈이 정하지 않는다. 앱을 조립하는 자리(composition root — 호스트 어댑터,
// W-N1)가 "이 모듈을 import하느냐 `updateServerAuthority.ts`/`publicKeys.ts`를
// import하느냐"로 결정한다. 즉 분기는 "어느 모듈을 쓰는 빌드인가"에 있고, 런타임
// if/switch로 두 상수 중 하나를 고르는 코드는 이 저장소 어디에도 없어야 한다.
//
// [번들 식별자 — 경계 표기, 이 슬라이스에서 값을 고정하지 않는 이유] Electron 앱의
// 번들 식별자(macOS bundle ID·Windows AppId)는 패키징 도구(electron-builder 등)
// 설정값이다 — 순수 TS 코어가 소유할 대상이 아니라 패키징 파이프라인(W-N1/W14)의
// 소유물이다. 이 슬라이스에서 마침표로 구분된 역방향 표기(관례상 마지막 마디가
// 흔한 도메인 접미사와 같은 모양이 되는 식별자 형식)의 구체적인 문자열을 여기
// 상수로 박아 두면 두 가지 문제가 생긴다: ① 패키징이 아직 존재하지 않는 상태
// (§7.5.1)에서 값을 지어내는 것이 되고 ② 그 형태는
// `compat/sensitive-classes.json`의 `network-authority-domain` 부류가 "인용부호 안의
// 도메인 형태 리터럴"로 정확히 잡아낸다(실측 확인됨) — 실제 도메인이 아니어도 그
// 부류는 형태만으로 판정하므로, 이 코어가 스스로 그 부류에 걸리는 값을 만들어
// 넣는 것은 부적절하다. 그래서 `UpdateChannelConfig.bundleIdentifier`
// (updateFlow.ts) 자리만 코드 계약으로 두고, 개발/운영 두 채널의 실제 문자열
// 값은 W-N1/W14가 패키징 설정(electron-builder의 앱 식별자 필드 등)을 만드는
// 시점에 그 설정 파일 쪽에서 채운다 — 이 파일은 그 값을 채울 자리가 있다는 것만
// 문서화한다.
import type { UpdatePublicKey } from './publicKeys.js';

export const DEV_UPDATE_HOST = 'updates-dev.example.com';

/** 개발 채널 서명 검증 공개키 — 개발용 K-3 등가물도 아직 발급 전이라 비어 있다.
 * production과 마찬가지로 **비어 있음은 전면 거부**를 뜻한다(`ed25519Verify.ts`). */
export const DEV_UPDATE_PUBLIC_KEYS: readonly UpdatePublicKey[] = [];
