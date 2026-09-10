// "MCP 관리" 화면의 실제 데이터 소스. Rust 커맨드 mcp_*가 `claude mcp` CLI를
// 위임 실행해 등록된 MCP 서버 목록/상세를 조회하고 추가/삭제한다. 모델을
// 호출하지 않는 순수 헬스체크라 빠르고 무료다 — 화면에서 이 사실을 안내한다.
//
// 카탈로그(mcp_catalog_list/mcp_install): 잘 알려진 공개 MCP 서버(Gmail 등)를
// 원클릭 등록한다. 넷 다 OAuth 로그인이 필요해서 이 앱이 로그인을 대신 처리하지
// 않는다 — GitHub/Cloudflare 연동과 동일한 정책으로, 백엔드가 터미널 창을 열어
// `claude mcp add`+`claude mcp login`을 이어서 실행하고 사용자가 그 창에서 직접
// 인증을 마친다. `installed`는 이미 mcp 목록에 등록돼 있는지를 뜻할 뿐, 로그인
// 완료 여부는 이 화면이 알 수 없다(사용자가 "새로고침"을 눌러야 반영된다).
import { invoke } from '@tauri-apps/api/core';
import type { TerminalLaunchResult } from './integrationsApi';

export type McpTransport = 'stdio' | 'http' | 'sse' | 'unknown';

export interface McpServerSummary {
  name: string;
  target: string; // URL 또는 실행 커맨드
  transport: McpTransport;
  connected: boolean;
  statusLabel: string; // 예: "✔ Connected", "⏸ Pending approval"
}

export interface McpServerDetail {
  name: string;
  scope: string;
  status: string;
  transport: string;
  target: string;
  oauth: string | null;
}

export async function fetchMcpServers(): Promise<McpServerSummary[]> {
  return invoke<McpServerSummary[]>('mcp_list');
}

export async function fetchMcpServerDetail(name: string): Promise<McpServerDetail> {
  return invoke<McpServerDetail>('mcp_get', { name });
}

export interface McpEnvVar {
  key: string;
  value: string;
}

export interface AddMcpServerInput {
  name: string;
  transport: McpTransport;
  target: string;
  args: string[]; // stdio 전용 — http/sse는 빈 배열
  header: string | null; // http/sse 전용("Key: Value") — stdio는 null
  env: McpEnvVar[]; // stdio 전용 — http/sse는 빈 배열
  oauthClientId: string | null; // http/sse 전용, 내부(사내) MCP 서버용 — stdio는 null
  oauthClientSecret: string | null; // http/sse 전용, 비밀값 — stdio는 null
  oauthCallbackPort: number | null; // http/sse 전용 — stdio는 null
}

export async function addMcpServer(input: AddMcpServerInput): Promise<void> {
  return invoke<void>('mcp_add', {
    name: input.name,
    transport: input.transport,
    target: input.target,
    args: input.args,
    header: input.header,
    env: input.env,
    oauthClientId: input.oauthClientId,
    oauthClientSecret: input.oauthClientSecret,
    oauthCallbackPort: input.oauthCallbackPort,
  });
}

export async function removeMcpServer(name: string): Promise<void> {
  return invoke<void>('mcp_remove', { name });
}

// 등록된 서버 재로그인/최초 로그인. 모든 http/sse 서버 행에서 언제든 누를 수
// 있다(카탈로그 원클릭 설치와 동일하게, 이 앱은 로그인을 대신 완료하지 않고
// 터미널 창을 여는 데까지만 관여한다).
export async function loginMcpServer(name: string): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('mcp_login', { name });
}

export interface McpCatalogEntry {
  readonly id: string; // "gmail" | "google-drive" | "google-calendar" | "atlassian"
  readonly label: string; // 표시 이름, 예: "Gmail", "Atlassian (Jira/Confluence)"
  readonly transport: string; // "http" | "sse"
  readonly target: string; // URL
  readonly installed: boolean; // 이미 mcp 목록에 등록돼 있으면 true
}

export async function fetchMcpCatalog(): Promise<McpCatalogEntry[]> {
  return invoke<McpCatalogEntry[]>('mcp_catalog_list');
}

export async function installMcpCatalogEntry(catalogId: string): Promise<TerminalLaunchResult> {
  return invoke<TerminalLaunchResult>('mcp_install', { catalogId });
}
