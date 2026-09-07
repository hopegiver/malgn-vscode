// `claude plugin ...` CLI 얇은 래퍼 — architecture.md §3.2 표(detect/plan/apply/verify)의
// 실행 열. argv 조립과 출력 파싱만 담당하는 **순수 함수**이고, 실제 실행(`platform/exec.ts`
// 경유)은 `detect.ts`/`apply.ts`가 한다 — 이 분리 덕에 실측 출력(fixture)만으로 파서를
// 단위 테스트할 수 있다.
//
// [실측 근거] 이 파일의 파서는 2.1.252 로컬 실측 출력 형태를 기준으로 한다:
//   `claude plugin list --json` → 배열, 각 원소 {id, version, scope, enabled, installPath, ...}
//   `claude plugin marketplace list --json` → 배열, 각 원소 {name, source, repo, installLocation}
// 문서(architecture.md §3.2)가 언급하는 `gitCommitSha` 필드는 이 실측에 없었다 — SHA 대조는
// `marketplaceSha.ts`가 별도 경로(체크아웃 디렉터리의 git HEAD 직접 조회)로 구현한다
// (반환문에 명시할 설계 이탈).

export const AGENT_EXEC_TIMEOUT_MS = 120_000;

/** `agent.plugin`(policy-contract.md 형식 `<name>@<marketplace>`)에서 이름만 뗀다.
 * `claude mcp get`이 보고하는 이름은 `plugin:<name>:<mcpServerName>`이라 마켓플레이스
 * 접미사가 없다(O-3 실측, §5.2). 형식이 어긋나면 원문을 그대로 돌려준다(추측하지 않는다). */
export function pluginNameOnly(pluginId: string): string {
  const at = pluginId.indexOf('@');
  return at === -1 ? pluginId : pluginId.slice(0, at);
}

/** `<name>@<marketplace>`에서 마켓플레이스 이름만 뗀다(실측: `claude plugin marketplace
 * list --json`의 `name` 필드와 정확히 같은 문자열, 예: `malgnsoft-plugins`). */
export function marketplaceNameFromPluginId(pluginId: string): string | null {
  const at = pluginId.indexOf('@');
  return at === -1 ? null : pluginId.slice(at + 1);
}

export function buildPluginListArgv(): { readonly file: string; readonly args: readonly string[] } {
  return { file: 'claude', args: ['plugin', 'list', '--json'] };
}

export function buildMarketplaceListArgv(): { readonly file: string; readonly args: readonly string[] } {
  return { file: 'claude', args: ['plugin', 'marketplace', 'list', '--json'] };
}

export function buildMarketplaceAddArgv(marketplaceRepo: string): { readonly file: string; readonly args: readonly string[] } {
  return { file: 'claude', args: ['plugin', 'marketplace', 'add', marketplaceRepo] };
}

export function buildMarketplaceUpdateArgv(marketplaceName: string): { readonly file: string; readonly args: readonly string[] } {
  return { file: 'claude', args: ['plugin', 'marketplace', 'update', marketplaceName] };
}

/** §3.2 "`-y`가 삼키는 것은 마켓플레이스 항목이 선언한 임의 명령의 실행 동의" — 이
 * 플래그는 §3.2.2의 명령-선언 검사를 통과했을 때만(`declaresCommand === false`) 붙인다.
 * 호출자(`apply.ts`)가 그 판정을 하고, 이 함수는 판정 결과를 argv로 옮기기만 한다. */
export function buildPluginInstallArgv(
  pluginId: string,
  scope: string,
  acceptDeclaredCommand: boolean
): { readonly file: string; readonly args: readonly string[] } {
  const args = ['plugin', 'install', pluginId, '--scope', scope];
  if (acceptDeclaredCommand) args.push('-y');
  return { file: 'claude', args };
}

export function buildPluginUpdateArgv(
  pluginId: string,
  scope: string,
  acceptDeclaredCommand: boolean
): { readonly file: string; readonly args: readonly string[] } {
  const args = ['plugin', 'update', pluginId, '--scope', scope];
  if (acceptDeclaredCommand) args.push('-y');
  return { file: 'claude', args };
}

export function buildPluginEnableArgv(pluginId: string): { readonly file: string; readonly args: readonly string[] } {
  return { file: 'claude', args: ['plugin', 'enable', pluginId] };
}

// ---------------------------------------------------------------------------
// 파서 — 전부 "파싱 불가는 null/빈 배열로 접는다"(PR-8과 같은 정신: 추측하지 않는다).
// ---------------------------------------------------------------------------

export interface PluginListEntry {
  readonly id: string;
  readonly version: string;
  readonly scope: string;
  readonly enabled: boolean;
  /** 실측에 없을 수 있는 선택 필드 — 있으면 SHA 대조 보조 신호로 쓴다(마켓플레이스
   * 체크아웃 HEAD 직접 조회가 정본, 이 필드는 있으면 교차 확인용). */
  readonly gitCommitSha?: string;
}

export function parsePluginListJson(stdout: string): readonly PluginListEntry[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: PluginListEntry[] = [];
  for (const item of parsed) {
    if (typeof item !== 'object' || item === null) continue;
    const record = item as Record<string, unknown>;
    if (typeof record.id !== 'string' || typeof record.version !== 'string' || typeof record.scope !== 'string' || typeof record.enabled !== 'boolean') {
      continue;
    }
    out.push({
      id: record.id,
      version: record.version,
      scope: record.scope,
      enabled: record.enabled,
      gitCommitSha: typeof record.gitCommitSha === 'string' ? record.gitCommitSha : undefined,
    });
  }
  return out;
}

export interface MarketplaceListEntry {
  readonly name: string;
  readonly source: string;
  readonly repo?: string;
  readonly installLocation: string;
}

export function parseMarketplaceListJson(stdout: string): readonly MarketplaceListEntry[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(stdout);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: MarketplaceListEntry[] = [];
  for (const item of parsed) {
    if (typeof item !== 'object' || item === null) continue;
    const record = item as Record<string, unknown>;
    if (typeof record.name !== 'string' || typeof record.source !== 'string' || typeof record.installLocation !== 'string') {
      continue;
    }
    out.push({
      name: record.name,
      source: record.source,
      repo: typeof record.repo === 'string' ? record.repo : undefined,
      installLocation: record.installLocation,
    });
  }
  return out;
}
