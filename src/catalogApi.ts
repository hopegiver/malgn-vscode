// 카탈로그의 실제 데이터 소스 — ~/.claude/plugins/installed_plugins.json(scope
// "user"만) + 각 플러그인 installPath 아래 agents/*.md·skills/*/·knowledge/*/
// 실물, 그리고 ~/.claude/plugins/known_marketplaces.json. "최신 버전"은 네트워크
// 호출이 필요해 다루지 않는다 — 설치된 버전만 실제 값이다.
import { invoke } from '@tauri-apps/api/core';

export interface CatalogEntryItem {
  readonly id: string;
  readonly name: string;
  readonly description: string;
}

export interface InstalledPlugin {
  readonly id: string;
  readonly name: string;
  readonly displayName: string | null;
  readonly version: string;
  readonly description: string;
  readonly installPath: string;
  readonly agents: readonly CatalogEntryItem[];
  readonly skills: readonly CatalogEntryItem[];
  readonly knowledge: readonly CatalogEntryItem[];
}

export async function fetchInstalledPlugins(): Promise<InstalledPlugin[]> {
  return invoke<InstalledPlugin[]>('list_installed_plugins');
}

export interface MarketplaceInfo {
  readonly id: string;
  readonly repo: string | null;
  readonly lastUpdated: string | null;
}

export async function fetchKnownMarketplaces(): Promise<MarketplaceInfo[]> {
  return invoke<MarketplaceInfo[]>('list_known_marketplaces');
}

export interface CommandResult {
  readonly success: boolean;
  readonly message: string;
}

// 사용자가 명시적으로 승인한 실제 실행 — `claude plugin update <id>`를 그대로
// 호출한다. id는 항상 fetchInstalledPlugins()가 돌려준 값만 넘어간다(자유 입력
// 필드 없음). 성공해도 Claude Code 재시작 전까지는 적용되지 않는다.
export async function updatePlugin(pluginId: string): Promise<CommandResult> {
  return invoke<CommandResult>('update_plugin', { pluginId });
}

export async function refreshMarketplaces(): Promise<CommandResult> {
  return invoke<CommandResult>('refresh_marketplaces');
}
