// "MCP 관리" 화면의 실제 데이터 소스. Rust 커맨드 mcp_*가 `claude mcp` CLI를
// 위임 실행해 등록된 MCP 서버 목록/상세를 조회하고 추가/삭제한다. 모델을
// 호출하지 않는 순수 헬스체크라 빠르고 무료다 — 화면에서 이 사실을 안내한다.
import { invoke } from '@tauri-apps/api/core';

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

export interface AddMcpServerInput {
  name: string;
  transport: McpTransport;
  target: string;
  args: string[]; // stdio 전용 — http/sse는 빈 배열
  header: string | null; // http/sse 전용("Key: Value") — stdio는 null
}

export async function addMcpServer(input: AddMcpServerInput): Promise<void> {
  return invoke<void>('mcp_add', {
    name: input.name,
    transport: input.transport,
    target: input.target,
    args: input.args,
    header: input.header,
  });
}

export async function removeMcpServer(name: string): Promise<void> {
  return invoke<void>('mcp_remove', { name });
}
