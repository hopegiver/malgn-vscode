// 마켓플레이스 로컬 체크아웃 읽기 — architecture.md §3.2.2 "1. `marketplace update` →
// `installLocation` 확인 → `marketplace.json`을 읽기 전용으로 파싱(PR-1이 금지하는
// 것은 쓰기다)". 실측 경로(`~/.claude/plugins/known_marketplaces.json` →
// `<installLocation>/.claude-plugin/marketplace.json`)를 그대로 따른다.
//
// 파일 읽기만 하고 절대 쓰지 않는다 — 이 모듈에 `writeFile`류 import가 없다는 것
// 자체가 PR-1(정본을 대체·복제하지 않는다, 쓰기가 아니라 호출만)의 구조적 증거다.

export type ReadTextFile = (path: string) => Promise<string>;

export interface KnownMarketplaceEntry {
  readonly repo: string | null;
  readonly installLocation: string;
}

/** `known_marketplaces.json`에서 `marketplaceName`(예: `malgnsoft-plugins`) 항목을 찾는다.
 * 실측 형태: `{ [name]: { source: {source:'github', repo}, installLocation, lastUpdated } }`.
 * 파일이 없거나 파싱 실패·항목 부재는 모두 `null`(=격하 판단은 호출자 몫)로 접는다. */
export async function readKnownMarketplaceEntry(
  claudeHomeDir: string,
  marketplaceName: string,
  readTextFile: ReadTextFile
): Promise<KnownMarketplaceEntry | null> {
  const path = `${claudeHomeDir}/plugins/known_marketplaces.json`;
  let raw: string;
  try {
    raw = await readTextFile(path);
  } catch {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null) return null;
  const entry = (parsed as Record<string, unknown>)[marketplaceName];
  if (typeof entry !== 'object' || entry === null) return null;
  const record = entry as Record<string, unknown>;
  if (typeof record.installLocation !== 'string') return null;
  const source = record.source;
  let repo: string | null = null;
  if (typeof source === 'object' && source !== null) {
    const repoValue = (source as Record<string, unknown>).repo;
    if (typeof repoValue === 'string') repo = repoValue;
  }
  return { repo, installLocation: record.installLocation };
}

/**
 * `<installLocation>/.claude-plugin/marketplace.json`을 읽어 `plugins[]`에서
 * `pluginName`(마켓플레이스 접미사 없는 이름)과 일치하는 원소를 찾는다. 원소를 찾지
 * 못하거나 파일을 읽을 수 없으면 `null`(fail-closed — 호출자가 apply를 진행하지
 * 못한다).
 */
export async function readMarketplaceEntryForPlugin(
  installLocation: string,
  pluginName: string,
  readTextFile: ReadTextFile
): Promise<Record<string, unknown> | null> {
  const path = `${installLocation}/.claude-plugin/marketplace.json`;
  let raw: string;
  try {
    raw = await readTextFile(path);
  } catch {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof parsed !== 'object' || parsed === null) return null;
  const plugins = (parsed as Record<string, unknown>).plugins;
  if (!Array.isArray(plugins)) return null;
  for (const item of plugins) {
    if (typeof item === 'object' && item !== null && (item as Record<string, unknown>).name === pluginName) {
      return item as Record<string, unknown>;
    }
  }
  return null;
}
