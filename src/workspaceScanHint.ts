// 프로젝트/새 세션/자율업무 세 화면이 "빈 상태"에서 공통으로 쓰는 안내 문구를
// 한 곳에 모은다 — 예전엔 세 곳에 `'~/workspace 아래 CLAUDE.md가 있는 폴더가
// malgn-agent 프로젝트로 표시됩니다.'`가 각각 하드코딩돼 있어, 실제 스캔 대상이
// (예: Windows의 `C:\workspace`) 달라도 항상 `~/workspace`라고 잘못 안내했다
// (Windows 실사용자 보고 — hub 이슈 01m2wm499xmh3046rnvx4cyn8n). 이 모듈은
// `state.malgnAgentConfig.status.workspaces`(전역 설정 정본)를 읽어 실제 값을
// 반영하고, 로딩/에러/빈 값일 때만 원래 하드코딩 문구로 폴백한다 — "대상을
// 아직 모른다"를 틀린 값으로 단정 짓지 않기 위함이다.
import { state } from './state';
import type { WorkspaceSkippedEntry } from './workspaceApi';

const FALLBACK_SCOPE_DESC = '~/workspace 아래 CLAUDE.md가 있는 폴더가 malgn-agent 프로젝트로 표시됩니다.';

// 실제 스캔 대상 경로를 반영한 안내 문구. 상태 미로드/에러/workspace 0개면
// 원래 기본 문구로 폴백한다(잘못된 값 단정보다 폴백이 안전하다).
export function describeWorkspaceScanScope(): string {
  const status = state.malgnAgentConfig.status;
  if (!status || !status.ok || status.workspaces.length === 0) return FALLBACK_SCOPE_DESC;
  return `${status.workspaces.join(', ')} 아래 CLAUDE.md가 있는 폴더가 malgn-agent 프로젝트로 표시됩니다.`;
}

// 후보 폴더를 찾았지만 제외된 경우 그 사실과 사유를 한 줄로 요약한다(과하지
// 않게 — noClaudeMd/rootUnreadable 두 사유만 사용자에게 실질적인 의미가 있어
// 그 둘만 문장화한다. hidden/notDirectory/invalidName은 대체로 잡음이라
// 문장에 넣지 않는다 — Rust 쪽 SkippedEntry에는 계속 기록되고, 필요해지면
// 여기 추가하면 된다). 제외된 항목이 없으면 null.
export function summarizeSkippedProjects(skipped: readonly WorkspaceSkippedEntry[]): string | null {
  const noClaudeMd = skipped.filter((s) => s.reason === 'noClaudeMd');
  const rootUnreadable = skipped.filter((s) => s.reason === 'rootUnreadable');
  if (noClaudeMd.length === 0 && rootUnreadable.length === 0) return null;

  const parts: string[] = [];
  if (noClaudeMd.length > 0) {
    const sampleNames = noClaudeMd.slice(0, 3).map((s) => s.name);
    const more = noClaudeMd.length > sampleNames.length ? ` 외 ${noClaudeMd.length - sampleNames.length}개` : '';
    parts.push(`폴더 ${noClaudeMd.length}개(${sampleNames.join(', ')}${more})를 찾았지만 CLAUDE.md가 없어 제외했습니다.`);
  }
  if (rootUnreadable.length > 0) {
    const roots = rootUnreadable.map((s) => s.path).join(', ');
    parts.push(`workspace 폴더를 읽을 수 없습니다: ${roots} (권한을 확인하세요).`);
  }
  return parts.join(' ');
}
