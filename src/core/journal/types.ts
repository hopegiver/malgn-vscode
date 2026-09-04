// 저널 엔트리 타입 — architecture.md §4.5 step ⑥(일반 apply 저널: `{ts, provider,
// changes, backupPath, diffHash}`) · §4.8.4 step 7 + policy-contract.md §4(install 저널
// 전용 스키마)의 정본을 TypeScript로 옮긴 것.
//
// 저널은 append-only 단일 스트림이라 서로 다른 두 스키마(일반 apply / install 전용)와
// W3 잔여 요구(§1.2 "①②③⑤는... 로그·진단 리포트에 남긴다")가 만드는 세 번째 종류가
// 한 파일에 섞여 쌓인다. `kind` 판별 필드는 원문 코드블록에 없던 필드다(W1이
// `Change.id`를 채운 것과 같은 종류의 설계 갭 채움 — 반환문에 명시) — 여러 스키마를
// 하나의 append-only 스트림에 안전하게 함께 쌓으려면 읽는 쪽이 형태를 판별할 수 있어야
// 한다.

import type { Change, ProviderId } from '../../providers/types.js';

// ---------------------------------------------------------------------------
// 공통 부속 타입 — policy-contract.md §4 원문 열거값
// ---------------------------------------------------------------------------

/** O-19 판정 — 해석 경로가 Cellar 또는 Caskroom 하위이면 `brew`로 판정한 결과
 * (정확한 glob 정본은 architecture.md §4.8.4 `detect` d1 — 이 주석은 `*``/` 조합을
 * 피해 JSDoc 블록 종료 오인을 막는다). */
export type InstallOrigin = 'brew' | 'native' | 'unknown';

/** §4.8.4 step 5 — "매니저가 거부"(null)·"전체 10분"(`total`)·"무출력 60초"(`idle`) */
export type TimeoutKind = 'total' | 'idle' | null;

/** §4.8.4 step 6② — macOS cask `codesign --verify` 결과 */
export type SignatureStatus = 'verified' | 'invalid' | 'unchecked' | 'n/a';

// ---------------------------------------------------------------------------
// ① 일반 apply 저널 — architecture.md §4.5 step ⑥
// ---------------------------------------------------------------------------

export interface ChangeJournalEntry {
  readonly kind: 'change';
  readonly ts: string; // ISO8601
  readonly provider: ProviderId;
  readonly changes: readonly Change[];
  /** 백업이 없으면 null — §4.5 step③ "실패 시 중단, 백업 없이 쓰지 않는다"가 지켜지면
   * 이 필드가 null인 채로 change 저널이 존재하는 일은 없어야 한다(방어적으로 nullable). */
  readonly backupPath: string | null;
  readonly diffHash: string;
}

// ---------------------------------------------------------------------------
// ② install 전용 저널 — policy-contract.md §4 원문 코드블록 그대로
// ---------------------------------------------------------------------------

export interface InstallJournalEntry {
  readonly kind: 'install';
  readonly ts: string;
  readonly tool: string;
  readonly strategy: string;
  readonly argv: readonly string[];
  readonly managerPath: string;
  readonly managerRealPath: string;
  readonly managerOwner: string;
  /** §3(installEnv) 가드 중 이번 실행에 **실제로 적용한** 집합(R-19 — 사후 대조 근거).
   * 값 정본은 `loadCodeConstants().installEnv`(compat/install-env.json)이고, 이 필드는
   * 그 값을 그대로 복사하는 게 아니라 "그 실행에 실제로 건 env"를 기록한다는 점이
   * 다르다 — 코드 상수가 바뀐 뒤에 남은 옛 저널을 재현할 때 이 구분이 근거가 된다. */
  readonly envSet: Readonly<Record<string, string>>;
  readonly targetVersion: string;
  readonly compatFloor: string;
  readonly exitCode: number | null;
  readonly timedOut: boolean;
  readonly timeoutKind: TimeoutKind;
  readonly resolvedPath: string;
  readonly installOrigin: InstallOrigin;
  readonly signature: SignatureStatus;
}

// ---------------------------------------------------------------------------
// ③ 동의 게이트 사고 신호 — architecture.md §1.2
// "①②③⑤는 MV_CONSENT_INVALID(high) — 사고 취급이라 조용히 재요청하지 않고
//  로그·진단 리포트에 남긴다." §1.2 원문에는 저널 스키마가 없다 — 이 슬라이스(W5)가
// "저널·진단"을 다루는 자리이므로 그 요구를 실행 가능한 형태로 채운 것(설계 갭 채움,
// 반환문에 명시). `MV_CONSENT_EXPIRED`(info, 정상 상황)는 여기 담지 않는다 — 담으면
// "diff를 꼼꼼히 읽고 몇 분 뒤 누르는 정상 행동"이 사고 신호와 같은 채널에 쌓여
// 변별력이 떨어진다(§1.2 "실패의 두 등급"과 같은 논리를 저널 계층에도 적용한 것).
// ---------------------------------------------------------------------------

export interface ConsentFailureJournalEntry {
  readonly kind: 'consentFailure';
  readonly ts: string;
  /** 항상 `MV_CONSENT_INVALID` — `MV_CONSENT_EXPIRED`는 이 엔트리 종류로 기록하지 않는다
   * (위 주석 참고). 문자열 타입으로 남겨 두 이유는 `ConsentGateError.code`를 그대로
   * 옮기는 배선이 한 곳(errors.ts 재수출)만 참조하게 하기 위해서다. */
  readonly code: string;
  readonly severity: 'high';
  readonly message: string;
  /** `ConsentGateError` 자체는 어느 provider의 시도였는지 모른다(§1.2 원문 타입에
   * 없음) — 호출자(미래 W7 orchestration, 그 시점에 `plan.providerId`를 쥐고 있다)가
   * 넘기지 않으면 null. */
  readonly providerId: ProviderId | null;
}

export type JournalEntry = ChangeJournalEntry | InstallJournalEntry | ConsentFailureJournalEntry;
