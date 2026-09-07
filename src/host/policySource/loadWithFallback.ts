// 정책 로드 3순위 폴백 오케스트레이션 — architecture.md §2.2 ③ "정책 로드(체크아웃 →
// 설치본 → 번들 내장 ... 셋 중 무엇을 썼는지 항상 UI에 표기)" · §3.7.3 표(N-1) ·
// §3.6.1 매트릭스 ④(체크아웃 14일 노후, N-2).
//
// `core/policy/loader.ts`(`loadPolicyFromText`)는 순수 함수라 "정책 원문 텍스트를 이미
// 손에 쥔" 상태만 다룬다 — 그 텍스트를 어디서 가져오는지(파일 3곳 순회)는 이 파일의
// 책임이다. 각 순위에서 "파일 없음(ENOENT)"·"크기 초과"·"JSON 파싱 실패"·"schemaVersion
// 범위 밖" 전부 "그 순위 실패"로 묶어 다음 순위로 넘어간다(N-1 "부분 적용 없음") — 단,
// **성공(status:'ok')**만 최종 결과로 채택하고, 실패든 성공이든 그 순위에서 발생한
// `issues`는 최종 반환값에 함께 실어 진단에 남긴다(어느 순위가 왜 실패했는지 추적
// 가능하게).

import { loadCodeConstants } from '../../core/policy/codeConstants.js';
import { loadPolicyFromText, POLICY_MAX_BYTES } from '../../core/policy/loader.js';
import type { PolicyLoadResult } from '../../core/policy/types.js';
import { buildBundledDefaultPolicyText } from './bundledDefaultPolicy.js';
import { resolveCheckoutPolicyPath, resolveInstalledPolicyPath } from './checkoutAndInstalledPaths.js';
import type { ReadTextFile } from './checkoutAndInstalledPaths.js';

export type PolicySourceKind = 'checkout' | 'installed' | 'bundled';

export interface LoadEffectivePolicyWithFallbackResult {
  readonly source: PolicySourceKind;
  /** 실제로 읽은 파일 경로. `source==='bundled'`이면 파일이 아니라 이 프로세스 내부
   * 상수이므로 null이다. */
  readonly sourcePath: string | null;
  readonly result: PolicyLoadResult;
  /** 시도했지만 실패해 다음 순위로 넘어간 순위들의 진단 — UI가 "왜 이 값이지"를
   * 추적할 수 있게 한다(§2.2 ③ "항상 UI에 표기"). */
  readonly skippedSources: readonly { readonly source: PolicySourceKind; readonly reason: string }[];
}

export interface LoadEffectivePolicyWithFallbackOptions {
  readonly claudeHomeDir: string;
  readonly readTextFile: ReadTextFile;
  readonly currentExtensionVersion: string;
}

async function tryLoadFromPath(
  path: string,
  readTextFile: ReadTextFile,
  currentExtensionVersion: string
): Promise<{ readonly ok: true; readonly result: PolicyLoadResult } | { readonly ok: false; readonly reason: string }> {
  let raw: string;
  try {
    raw = await readTextFile(path);
  } catch (error) {
    return { ok: false, reason: `읽기 실패: ${error instanceof Error ? error.message : String(error)}` };
  }
  if (Buffer.byteLength(raw, 'utf8') > POLICY_MAX_BYTES) {
    return { ok: false, reason: `상한(${POLICY_MAX_BYTES}바이트) 초과` };
  }
  const constants = loadCodeConstants();
  const result = loadPolicyFromText(raw, constants, { currentExtensionVersion });
  if (result.status === 'rejected') {
    return { ok: false, reason: `${result.code}: ${result.message}` };
  }
  return { ok: true, result };
}

/**
 * §3.7.3 표 그대로: ① 체크아웃 ② 설치본 ③ 번들 내장 순으로 시도하고, 처음 성공한
 * 순위를 채택한다. 3순위(번들 내장)는 `buildBundledDefaultPolicyText()`가 항상 유효한
 * 최소 정책을 만들어 반환하므로 **구조적으로 실패하지 않는다**(N-1 "셋 다 실패는
 * 구조상 불가"의 실제 구현 근거).
 */
export async function loadEffectivePolicyWithFallback(
  options: LoadEffectivePolicyWithFallbackOptions
): Promise<LoadEffectivePolicyWithFallbackResult> {
  const skipped: { readonly source: PolicySourceKind; readonly reason: string }[] = [];

  const checkoutPath = await resolveCheckoutPolicyPath(options.claudeHomeDir, options.readTextFile);
  if (checkoutPath) {
    const attempt = await tryLoadFromPath(checkoutPath, options.readTextFile, options.currentExtensionVersion);
    if (attempt.ok) {
      return { source: 'checkout', sourcePath: checkoutPath, result: attempt.result, skippedSources: skipped };
    }
    skipped.push({ source: 'checkout', reason: attempt.reason });
  } else {
    skipped.push({ source: 'checkout', reason: '체크아웃 위치를 찾을 수 없음(마켓플레이스 미등록 또는 known_marketplaces.json 부재)' });
  }

  const installedPath = await resolveInstalledPolicyPath(options.claudeHomeDir, options.readTextFile);
  if (installedPath) {
    const attempt = await tryLoadFromPath(installedPath, options.readTextFile, options.currentExtensionVersion);
    if (attempt.ok) {
      return { source: 'installed', sourcePath: installedPath, result: attempt.result, skippedSources: skipped };
    }
    skipped.push({ source: 'installed', reason: attempt.reason });
  } else {
    skipped.push({ source: 'installed', reason: '설치본 위치를 찾을 수 없음(플러그인 미설치 또는 installed_plugins.json 부재)' });
  }

  const bundledText = buildBundledDefaultPolicyText();
  const constants = loadCodeConstants();
  const bundledResult = loadPolicyFromText(bundledText, constants, { currentExtensionVersion: options.currentExtensionVersion });
  return { source: 'bundled', sourcePath: null, result: bundledResult, skippedSources: skipped };
}

/**
 * §3.6.1 매트릭스 ④ / N-2 — 체크아웃이 실제로 쓰였을 때만 노후 판정이 의미가 있다
 * (설치본·번들 내장은 "체크아웃 갱신"이라는 해소 경로 자체가 없다). `source`가
 * `'checkout'`이 아니면 `null`을 반환해 호출자가 이 판정을 건너뛰게 한다.
 */
export async function resolveCheckoutMtimeIfApplicable(
  loadResult: Pick<LoadEffectivePolicyWithFallbackResult, 'source' | 'sourcePath'>,
  statMtime: (path: string) => Promise<Date | null>
): Promise<Date | null> {
  if (loadResult.source !== 'checkout' || loadResult.sourcePath === null) return null;
  return statMtime(loadResult.sourcePath);
}
