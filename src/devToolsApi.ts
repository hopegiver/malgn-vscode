// "개발 환경" 화면의 실제 데이터 소스. Rust 커맨드가 로컬 CLI 도구(claude/node/gh/
// git/pnpm/wrangler)의 설치 여부·버전·설치방식을 조회하고, 실행 가능하다고 판별된
// 도구에 한해 실제로 업데이트/설치 명령을 실행한다(src-tauri/src/lib.rs 참고).
//
// actionKind가 도구별로 무엇을 할 수 있는지를 결정한다: "run"은 이 앱이 직접 명령을
// 실행할 수 있는 경우, "manual"은 앱이 대신 실행하지 않고 안내만 하는 경우(예: git은
// macOS 시스템 도구라 앱이 건드릴 수 없음), "none"은 설치 경로 자체를 찾지 못해 아무
// 것도 할 수 없는 경우다. "run"인 도구도 곧바로 실행하지 않고 먼저
// preview_dev_tool_update로 무엇이 바뀔지 미리 보여준 뒤, 사용자가 확인한 계획
// (planId)과 실행 직전 재계산한 값이 일치할 때만 실제 명령을 실행한다.
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

export async function openManualInstruction(toolId: string): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('open_manual_instruction', { toolId });
}
