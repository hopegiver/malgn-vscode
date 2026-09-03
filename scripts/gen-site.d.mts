// scripts/gen-site.mjs의 타입 선언 — check11-siteFaceDiscipline.ts(TS)가 순수 함수를
// 재사용하기 위한 최소 선언. 정본 구현·주석은 .mjs 쪽에 있다.

export function resolveSiteHoles(
  node: unknown,
  siteData: Record<string, unknown>,
  errors: string[],
  path?: string
): unknown;
export function hasRemainingHole(node: unknown): boolean;
export function generateSiteModule(args: {
  readonly compatibilityJson: Record<string, unknown>;
  readonly siteJsonPath: string;
  readonly siteExampleJsonPath: string;
  readonly readFileFn?: (path: string, encoding: string) => string;
  readonly existsFn?: (path: string) => boolean;
}): { siteProfile: 'site' | 'example'; siteConstants: Record<string, unknown> };
export function main(rootOverride?: string): { siteProfile: 'site' | 'example'; siteConstants: Record<string, unknown> };
