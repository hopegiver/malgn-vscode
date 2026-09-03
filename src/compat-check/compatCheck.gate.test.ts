// `pnpm compat:check`의 실제 진입점 — docs/policy-contract.md §6 "검사 3중" (a)의
// 차단성 검사 ①~⑧을 실제 저장소 파일(docs/*.md, compat/*.json, package.json, src/**)에
// 적용한다. `vitest.compat.config.ts`가 이 파일만(그리고 순수 로직 유닛 테스트들을)
// 실행 대상으로 잡고, 메인 `vitest.config.ts`는 이 디렉터리 전체를 제외해 `pnpm test`
// 파이프라인 단계와 겹치지 않게 한다(tech-stack.md §5.4 파이프라인: vitest → compat:check).
//
// 검사마다 개별 `it()`로 나눠 CI 로그에서 "어느 검사가 실패했는지"가 바로 보이게 한다
// (작업 지시 완료판정 #2: "각 검사가 문서의 어느 항목인지 코드에서 추적 가능해야").

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from '../core/policy/codeConstants.js';
import { checkCompatibilityDocSync } from './checks/check1-compatibilityDocSync.js';
import { checkPolicyFixtureSchema } from './checks/check2-policyFixtureSchema.js';
import { checkFixtureLeafCoverage } from './checks/check3-fixtureLeafCoverage.js';
import { checkPluginJsonFieldAccess } from './checks/check4-pluginJsonFieldAccess.js';
import { checkAllowedIdentifiers } from './checks/check5-allowedIdentifiers.js';
import { checkInstallTargetColumns } from './checks/check6-installTargetColumns.js';
import { checkCrossDocReferences } from './checks/check7-crossDocReferences.js';
import { checkHrsEvidence, getCommitTimestamp } from './checks/check8-hrsEvidence.js';
import type { CheckResult } from './types.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..');
const DOCS = join(ROOT, 'docs');
const COMPAT = join(ROOT, 'compat');

const PACKAGE_JSON = join(ROOT, 'package.json');
const COMPATIBILITY_JSON = join(COMPAT, 'compatibility.json');
const POLICY_CONTRACT_MD = join(DOCS, 'policy-contract.md');
const ARCHITECTURE_MD = join(DOCS, 'architecture.md');
const TECH_STACK_MD = join(DOCS, 'tech-stack.md');
const MALGN_AUTH_MD = join(DOCS, 'malgn-auth-requirements.md');
const MALGNAI_HUB_MD = join(DOCS, 'malgnai-hub-requirements.md');
const DOCS_README_MD = join(DOCS, 'README.md');
const INSTALL_TARGETS_JSON = join(COMPAT, 'install-targets.json');
const AGENT_INTERFACE_SPEC_JSON = join(COMPAT, 'agent-interface.spec.json');
const EVIDENCE_DIR = join(COMPAT, 'verification-evidence');
const SRC_DIR = join(ROOT, 'src');

const CURRENT_EXTENSION_VERSION = (JSON.parse(readFileSync(PACKAGE_JSON, 'utf8')) as { version: string }).version;

function reportOf(result: CheckResult): string {
  return result.violations.map((v) => `  - [${v.ref}] ${v.message}`).join('\n');
}

describe('pnpm compat:check — policy-contract.md §6 검사 ①~⑧', () => {
  it('① compatibility.json ↔ package.json·문서 일치', () => {
    const result = checkCompatibilityDocSync({
      packageJsonPath: PACKAGE_JSON,
      compatibilityJsonPath: COMPATIBILITY_JSON,
      policyContractMdPath: POLICY_CONTRACT_MD,
      architectureMdPath: ARCHITECTURE_MD,
    });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('② 정책 fixture의 스키마 통과', () => {
    const result = checkPolicyFixtureSchema({
      policyContractMdPath: POLICY_CONTRACT_MD,
      currentExtensionVersion: CURRENT_EXTENSION_VERSION,
    });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('③ 정책 fixture의 모든 리프 경로가 전수 검증표의 행을 갖는지 (PR-11①)', () => {
    const result = checkFixtureLeafCoverage({ policyContractMdPath: POLICY_CONTRACT_MD });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('④ plugin.json 접근 경로가 spec fields 안인지', () => {
    const result = checkPluginJsonFieldAccess({ srcDir: SRC_DIR, agentInterfaceSpecPath: AGENT_INTERFACE_SPEC_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑤ allowed* 식별자 ⊆ 코드 상수 키 집합 + 값 실재 + 플레이스홀더 부재 (PR-11②)', () => {
    const result = checkAllowedIdentifiers({ docPaths: [POLICY_CONTRACT_MD, ARCHITECTURE_MD] });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑥ allowedInstallTargets 격자 — 열 스펙 전량 + verified 불리언 리터럴 (PR-11③)', () => {
    const result = checkInstallTargetColumns({ installTargetsJsonPath: INSTALL_TARGETS_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑦ 위성 문서의 architecture.md 절 참조가 실재 헤딩을 가리키는지 (§7, M-18)', () => {
    const result = checkCrossDocReferences({
      targetDocPaths: [POLICY_CONTRACT_MD, TECH_STACK_MD, MALGN_AUTH_MD, MALGNAI_HUB_MD, DOCS_README_MD],
      docRegistryPaths: {
        'architecture.md': ARCHITECTURE_MD,
        'policy-contract.md': POLICY_CONTRACT_MD,
        'tech-stack.md': TECH_STACK_MD,
        'malgn-auth-requirements.md': MALGN_AUTH_MD,
        'malgnai-hub-requirements.md': MALGNAI_HUB_MD,
        'README.md': DOCS_README_MD,
      },
      architectureMdFileName: 'architecture.md',
    });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑧ HRS-E — verified:true 행 ↔ 검증 증거 파일 바인딩', () => {
    const constants = loadCodeConstants();
    const result = checkHrsEvidence({
      installTargets: constants.allowedInstallTargets,
      constants,
      evidenceDir: EVIDENCE_DIR,
      commitTimestamp: getCommitTimestamp(ROOT),
    });
    expect(result.violations, reportOf(result)).toEqual([]);

    // 완료판정 #6 — "현재 상태에서 pnpm compat:check가 통과한다": 전 행 verified:false라
    // 검사 대상이 0건이어야 한다. 이 assert가 없으면 "검사를 통째로 건너뛰어서
    // 우연히 통과"하는 상태와 "정말로 0건을 확인하고 통과"하는 상태를 구분할 수 없다.
    const verifiedCount = constants.allowedInstallTargets.filter((r) => r.verified === true).length;
    expect(verifiedCount).toBe(0);
  });
});
