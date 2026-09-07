// 정책 소스 경로 해석 — architecture.md §3.7.0(544행) "정책 출처는 마켓플레이스 체크아웃
// 안의 malgn-agent/workstation-profile.json" · §3.7.2(552행) "파일: <체크아웃|installPath>
// /workstation-profile.json" · O-1/O-2(§0.4) "설치 정본은 ~/.claude/plugins/
// installed_plugins.json" · 318행 "marketplace add|update는 저장소를 로컬에 체크아웃하므로
// known_marketplaces.json(→ installLocation) → <installLocation>/.claude-plugin/
// marketplace.json의 plugins[]".
//
// [정직 표기 — 실측 필요] `known_marketplaces.json`·`installed_plugins.json`의 **정확한
// JSON 스키마**는 이 저장소 문서 어디에도 원문 그대로 실려 있지 않다(O-1/O-2는 "무엇을
// 담는지"만 산문으로 서술한다). 아래 파서는 그 산문 서술과 가장 자연스럽게 부합하는
// 형태(마켓플레이스 짧은 이름 → `{installLocation}` / 플러그인 식별자 → 스코프별
// `{installPath}`)를 가정한다 — `tech-stack.md`의 "미확인" 표기·O-19("brew cask 경로는
// macOS에서조차 검증된 적이 없다")와 같은 종류의 미실측 가정이다. **실제 파일과
// 어긋나면 이 함수들은 예외를 던지지 않고 조용히 null을 반환해 다음 순위로
// 폴백한다**(N-1 표 "1순위 없음/파싱 실패 → 2순위" 원칙) — 가정이 틀렸을 때의 대가가
// "체크아웃을 못 읽음"(안전한 방향, 다음 순위로) 이상으로 커지지 않는다. devops가
// 실제 파일을 실측하면 이 파일의 파싱 로직만 교체하면 된다(호출자 계약은 그대로).

import { join } from 'node:path';

export interface ReadTextFile {
  (path: string): Promise<string>;
}

const CHECKOUT_MARKETPLACE_SHORT_NAME = 'malgnsoft-plugins';
const INSTALLED_PLUGIN_ID = 'malgn-agent@malgnsoft-plugins';
const INSTALLED_SCOPE = 'user';
const POLICY_FILE_NAME = 'workstation-profile.json';
const PLUGIN_FOLDER_NAME = 'malgn-agent';

export function computePluginsDir(claudeHomeDir: string): string {
  return join(claudeHomeDir, 'plugins');
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * `known_marketplaces.json` → `installLocation` → 체크아웃 안의 정책 파일 경로.
 * 가정 형태: `{ "<marketplace-short-name>": { "installLocation": "<절대경로>" } }`.
 */
export async function resolveCheckoutPolicyPath(claudeHomeDir: string, readTextFile: ReadTextFile): Promise<string | null> {
  const knownMarketplacesPath = join(computePluginsDir(claudeHomeDir), 'known_marketplaces.json');
  let raw: string;
  try {
    raw = await readTextFile(knownMarketplacesPath);
  } catch {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isPlainObject(parsed)) return null;
  const entry = parsed[CHECKOUT_MARKETPLACE_SHORT_NAME];
  if (!isPlainObject(entry) || typeof entry.installLocation !== 'string' || entry.installLocation.length === 0) {
    return null;
  }
  return join(entry.installLocation, PLUGIN_FOLDER_NAME, POLICY_FILE_NAME);
}

/**
 * `installed_plugins.json` → `installPath`(scope='user') → 설치본 안의 정책 파일 경로.
 * 가정 형태: `{ "<plugin-id>": { "<scope>": { "installPath": "<절대경로>" } } }`.
 */
export async function resolveInstalledPolicyPath(claudeHomeDir: string, readTextFile: ReadTextFile): Promise<string | null> {
  const installedPluginsPath = join(computePluginsDir(claudeHomeDir), 'installed_plugins.json');
  let raw: string;
  try {
    raw = await readTextFile(installedPluginsPath);
  } catch {
    return null;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isPlainObject(parsed)) return null;
  const pluginEntry = parsed[INSTALLED_PLUGIN_ID];
  if (!isPlainObject(pluginEntry)) return null;
  const scopeEntry = pluginEntry[INSTALLED_SCOPE];
  if (!isPlainObject(scopeEntry) || typeof scopeEntry.installPath !== 'string' || scopeEntry.installPath.length === 0) {
    return null;
  }
  return join(scopeEntry.installPath, POLICY_FILE_NAME);
}
