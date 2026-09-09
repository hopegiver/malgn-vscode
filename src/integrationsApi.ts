// GitHub·Cloudflare·Jira 연동 설정 화면의 실제 데이터 소스.
//
// GitHub/Cloudflare: 이 앱은 토큰을 취급하지 않는다 — 상태 조회는 각 CLI(`gh`/
// `wrangler`)를 읽기 전용으로 호출하고, 연결/해제는 사용자가 직접 조작할 터미널
// 창을 여는 것뿐이다(github_integration.rs/cloudflare_integration.rs 참고).
// `opened`는 "로그인이 끝났다"가 아니라 "터미널 창을 여는 데 성공했다"는 뜻이다.
//
// Jira: 위임할 CLI가 없어 이 앱이 직접 사이트 URL·이메일·API 토큰을 받아 검증 후
// macOS 키체인에 저장한다(jira_integration.rs). 토큰은 저장 커맨드의 인자로만
// 넘어가고, 어떤 조회 커맨드도 토큰 원문을 돌려주지 않는다.
import { invoke } from '@tauri-apps/api/core';

export interface GithubStatus {
  readonly installed: boolean;
  readonly connected: boolean;
  readonly login: string | null;
  readonly name: string | null;
  readonly avatarUrl: string | null;
}

export interface TerminalLaunchResult {
  readonly opened: boolean;
  readonly message: string;
}

export async function fetchGithubStatus(): Promise<GithubStatus> {
  return invoke<GithubStatus>('github_status');
}

export async function connectGithub(): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('github_connect');
}

export async function disconnectGithub(): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('github_disconnect');
}

export interface CloudflareStatus {
  readonly installed: boolean;
  readonly connected: boolean;
  readonly email: string | null;
}

export async function fetchCloudflareStatus(): Promise<CloudflareStatus> {
  return invoke<CloudflareStatus>('cloudflare_status');
}

export async function connectCloudflare(): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('cloudflare_connect');
}

export async function disconnectCloudflare(): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('cloudflare_disconnect');
}

export interface JiraStatus {
  readonly connected: boolean;
  readonly site?: string;
  readonly host?: string;
  readonly email?: string;
  readonly accountId?: string | null;
  readonly displayName?: string | null;
}

export async function fetchJiraStatus(): Promise<JiraStatus> {
  return invoke<JiraStatus>('jira_status');
}

// site/email/token은 제출 시점에만 함수 인자로 넘어가고 호출이 끝나면 이 모듈
// 어디에도 남지 않는다 — state에 토큰을 보관하지 않는다.
export async function connectJira(site: string, email: string, token: string): Promise<JiraStatus> {
  return invoke<JiraStatus>('jira_connect', { site, email, token });
}

export async function disconnectJira(): Promise<void> {
  return invoke<void>('jira_disconnect');
}
