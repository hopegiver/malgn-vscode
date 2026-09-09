// "개발 환경" 화면의 실제 데이터 소스. Rust 커맨드가 로컬 CLI 도구(claude/node/gh/
// git/pnpm/wrangler)의 설치 여부·버전·설치방식을 조회하고, 실행 가능하다고 판별된
// 도구에 한해 실제로 업데이트/설치 명령을 실행한다(src-tauri/src/dev_tools.rs 참고).
//
// actionKind가 도구별로 무엇을 할 수 있는지를 결정한다: "run"은 이 앱이 직접 명령을
// 실행할 수 있는 경우, "manual"은 앱이 대신 실행하지 않고 안내만 하는 경우(예: git은
// macOS 시스템 도구라 앱이 건드릴 수 없음), "none"은 설치 경로 자체를 찾지 못해 아무
// 것도 할 수 없는 경우다. "run"인 도구는 개별 실행 시 preview_dev_tool_update로
// 무엇이 바뀔지 화면에 보여주고 사용자 확인을 받은 뒤에만 그 계획(planId)으로
// 실행한다(devTools.ts handleConfirmRun). "전체 업데이트"는 버튼 클릭 자체를
// 일괄 동의로 보고 각 도구의 미리보기를 화면 노출·개별 확인 없이 순차 실행하되,
// 미리보기가 실패/타임아웃해 신뢰할 수 없는 항목(DevToolPreview.previewReliable
// === false)이나 영향 대상이 1개보다 많은 항목은 배치에서 제외하고 개별 확인
// 대기로 남긴다(devTools.ts handleUpdateAll).
//
// Tauri invoke는 이 모듈에서만 호출한다 — 화면(views/*.ts)은 이 모듈이 내보내는
// 함수를 통해서만 백엔드를 부른다("*Api.ts 래퍼" 패턴, catalogApi.ts/usageApi.ts와
// 동일). openManualInstruction의 execute 파라미터: false면 어떤 명령이 실행될지만
// 반환하고 아무 것도 실행하지 않는다(미리보기 전용), true면 실제로 터미널을 열어
// 그 명령을 실행한다.
import { invoke } from '@tauri-apps/api/core';

export type DevToolActionKind = 'run' | 'manual' | 'none';

export interface DevToolStatus {
  readonly id: string;
  readonly name: string;
  readonly installed: boolean;
  readonly version: string | null;
  readonly path?: string | null;
  readonly installMethod?: string | null;
  readonly actionKind: DevToolActionKind;
  readonly manualHint?: string | null;
}

export interface DevToolPreview {
  readonly id: string;
  readonly planId: string;
  readonly willRun: boolean;
  readonly commandDisplay: string;
  readonly affected: readonly string[];
  readonly notes: string;
  // false면 dry-run 미리보기가 spawn 실패 또는 타임아웃했다는 뜻이며,
  // affected/notes가 실제 영향 범위를 반영하지 못한다(안전을 위한 길이-1 기본값일
  // 뿐). "전체 업데이트" 배치는 이 값이 false인 항목을 자동 실행 대상에서 제외해야
  // 한다(devTools.ts handleUpdateAll).
  readonly previewReliable: boolean;
}

export type DevToolOutcome = 'updated' | 'alreadyLatest' | 'unknownAfter' | 'failed' | 'timedOut' | 'notSupported';

export interface DevToolActionResult {
  readonly id: string;
  readonly outcome: DevToolOutcome;
  readonly verified: boolean;
  readonly versionBefore?: string | null;
  readonly versionAfter?: string | null;
  readonly normalizedBefore?: string | null;
  readonly normalizedAfter?: string | null;
  readonly installMethod: string;
  readonly ranCommand?: string | null;
  readonly exitCode?: number | null;
  readonly durationMs: number;
  readonly message: string;
  readonly logTail: string;
  readonly pathVisible: boolean;
  readonly pathHint?: string | null;
  readonly pathHintTarget?: string | null;
}

export interface TerminalLaunchResult {
  readonly opened: boolean;
  readonly message: string;
}

export async function fetchDevTools(): Promise<DevToolStatus[]> {
  return invoke<DevToolStatus[]>('check_dev_tools');
}

export async function previewDevToolUpdate(toolId: string): Promise<DevToolPreview> {
  return invoke<DevToolPreview>('preview_dev_tool_update', { toolId });
}

export async function updateDevTool(toolId: string, planId: string): Promise<DevToolActionResult> {
  return invoke<DevToolActionResult>('update_dev_tool', { toolId, planId });
}

export async function installDevTool(toolId: string, planId: string): Promise<DevToolActionResult> {
  return invoke<DevToolActionResult>('install_dev_tool', { toolId, planId });
}

// execute=false: 어떤 명령이 실행될지만 반환하고 아무 것도 실행하지 않는다
// (미리보기 전용). execute=true: 실제로 터미널을 열어 그 명령을 실행한다.
export async function openManualInstruction(toolId: string, execute: boolean): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('open_manual_instruction', { toolId, execute });
}
