// 진단 리포트 — architecture.md §1.1(`core/diagnostics` — "집계·리포트, 마스킹") ·
// §3.6 G-9("v1.0에는 원격 요청이 하나도 없어 관측은 진단 리포트뿐") ·
// policy-contract.md §4(필드 정본 — v0.5 확장, devops Q4·Q1) · §6.2(클립보드 마스킹).
//
// 필드 표(policy-contract.md §4)는 "v1.0의 유일한 원격 관측 수단이므로 여기 없는 정보는
// 진단이 불가능하다"고 명시한다 — 즉 이 표는 장식이 아니라 **상한**이다. 이 모듈은 그
// 표에 없는 필드를 `DiagnosticReport`에 추가하지 않는다(§1.2 잔여 요구 — 동의 게이트
// 사고 신호를 이 표에 실을 슬롯이 없는 문제 — 는 고치지 않고 반환문에 보고한다).

import type { StopDecision } from '../reconciler/stopGate.js';
import { maskDeepValues } from './mask.js';
import type { InstallJournalEntry, InstallOrigin, SignatureStatus, TimeoutKind } from '../journal/types.js';

/**
 * policy-contract.md §4 "진단 리포트" 표 — "install이 관여했을 때만" 실리는 블록.
 * `InstallJournalEntry`의 부분집합이다 — 저널에만 있고 리포트 표에는 없는 필드
 * (`argv`·`managerRealPath`·`managerOwner`·`envSet`·`installOrigin`)는 여기 담지 않는다.
 * `installOrigin`은 리포트의 "always" 필드 쪽에 이미 최상위로 있다(표 원문 그대로).
 */
export interface LastInstallAttempt {
  readonly tool: string;
  readonly strategy: string;
  readonly exitCode: number | null;
  readonly timedOut: boolean;
  readonly timeoutKind: TimeoutKind;
  readonly managerPath: string;
  readonly resolvedPath: string;
  readonly targetVersion: string;
  readonly compatFloor: string;
  readonly signature: SignatureStatus;
  readonly ts: string;
}

/**
 * `InstallJournalEntry`(저널의 install 전용 스키마, 필드가 더 많다)에서 리포트가 요구하는
 * 부분집합만 뽑는다 — **stderr 원문을 넣지 않는다**(policy-contract.md §4 "마스킹" 문단
 * 원문: "exitCode·타임아웃 종류 같은 비민감 메타데이터만 싣는다"). `InstallJournalEntry`
 * 자체에 stderr 필드가 없으므로(설계가 애초에 그 필드를 저널 스키마에 넣지 않았다) 이
 * 함수가 실수로 옮길 값도 없다 — 상한이 스키마 층에서부터 강제된다.
 */
export function toLastInstallAttempt(entry: InstallJournalEntry): LastInstallAttempt {
  return {
    tool: entry.tool,
    strategy: entry.strategy,
    exitCode: entry.exitCode,
    timedOut: entry.timedOut,
    timeoutKind: entry.timeoutKind,
    managerPath: entry.managerPath,
    resolvedPath: entry.resolvedPath,
    targetVersion: entry.targetVersion,
    compatFloor: entry.compatFloor,
    signature: entry.signature,
    ts: entry.ts,
  };
}

/**
 * policy-contract.md §4 "진단 리포트" 표의 "항상" 행 + 조건부 `lastInstallAttempt`.
 * `verdict`는 새 어휘를 만들지 않고 `StopDecision`(W4, `core/reconciler/stopGate.ts`)을
 * 그대로 재사용한다 — `StopReason.message`가 이미 §3.7.3·§3.6.1이 정한 "표시 문구"
 * 원문이므로(예: N-2 "오래된 정책으로 PC를 바꾸지 않습니다 — 진단과 되돌리기는 계속
 * 쓸 수 있습니다") 진단 리포트가 그 문구를 다시 짓지 않고 그대로 보여줄 수 있다
 * (W4가 남긴 이어받을 지점, 반환문 참고).
 */
export interface DiagnosticReport {
  readonly extensionVersion: string;
  readonly claudeVersion: string | null;
  readonly installOrigin: InstallOrigin | null;
  readonly agentVersion: string | null;
  readonly policySource: string | null;
  readonly verdict: StopDecision;
  readonly lastInstallAttempt?: LastInstallAttempt;
}

export interface DiagnosticReportInput {
  readonly extensionVersion: string;
  readonly claudeVersion: string | null;
  readonly installOrigin: InstallOrigin | null;
  readonly agentVersion: string | null;
  readonly policySource: string | null;
  readonly verdict: StopDecision;
  /** install이 이번 활성화 세션에서 관여하지 않았으면 생략 — 리포트에 필드 자체가
   * 실리지 않는다(표 원문 "install이 관여했을 때만"). */
  readonly lastInstallAttempt?: LastInstallAttempt;
}

/** 순수 조립 함수 — 네트워크·fs 없음. 호출자(향후 대시보드/커맨드 팔레트, W11)가 이미
 * 손에 쥔 값들을 표 형태로 모을 뿐이다. */
export function buildDiagnosticReport(input: DiagnosticReportInput): DiagnosticReport {
  const report: DiagnosticReport = {
    extensionVersion: input.extensionVersion,
    claudeVersion: input.claudeVersion,
    installOrigin: input.installOrigin,
    agentVersion: input.agentVersion,
    policySource: input.policySource,
    verdict: input.verdict,
  };
  if (input.lastInstallAttempt === undefined) return report;
  return { ...report, lastInstallAttempt: input.lastInstallAttempt };
}

/**
 * 진단 리포트를 클립보드로 내보내기 직전의 **유일한** 직렬화 지점(§6.2 원문: "진단
 * 리포트도 같은 필터를 통과한 뒤에만 클립보드로 나간다"). 호출자(`vscode.env.clipboard
 * .writeText`를 실제로 부르는 쪽, W11)는 이 함수의 반환값만 클립보드에 써야 한다 —
 * `JSON.stringify(report)`를 직접 호출해 이 함수를 우회하는 경로를 만들지 않는다
 * (`maskSinglePoint.test.ts`가 이 파일 안에 `maskDeepValues` 호출이 정확히 이 한 곳뿐임을
 * 고정한다).
 *
 * `maskDeepValues`를 리프 값에만 적용한 뒤 직렬화한다(`JSON.stringify` 이후 전체
 * 문자열에 `maskSensitive`를 거는 방식과 달리 필드 이름은 손대지 않는다) — 저널
 * (`core/journal/store.ts`)이 겪은 것과 같은 종류의 과잉 마스킹을 예방한다. 이
 * 리포트 스키마(policy-contract.md §4)에는 diffHash류 고정 길이 해시 필드가 없어
 * store.ts처럼 별도 필드 복원 예외가 필요 없다.
 */
export function renderDiagnosticReportForClipboard(report: DiagnosticReport): string {
  return JSON.stringify(maskDeepValues(report), null, 2);
}
