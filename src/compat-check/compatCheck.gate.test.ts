// `pnpm compat:check`의 실제 진입점 — docs/policy-contract.md §6 "검사 3중" (a)의
// 차단성 검사 ①~⑪(신설 ⑨⑩⑪ 포함)을 실제 저장소 파일(docs/*.md, compat/*.json,
// package.json, src/**)에 적용한다. `vitest.compat.config.ts`가 이 파일만(그리고 순수
// 로직 유닛 테스트들을) 실행 대상으로 잡고, 메인 `vitest.config.ts`는 이 디렉터리 전체를
// 제외해 `pnpm test` 파이프라인 단계와 겹치지 않게 한다.
//
// **모드 판정(docs/policy-contract.md §8.3/§8.7 S4)**: 모드는 플래그가 아니라 `docs/`
// 디렉터리의 실재로 결정한다. `docs/`가 있으면(로컬 개발) **full 모드**로 ①B·⑤C·⑦까지
// 전부 돈다(엄격한 쪽). `docs/`가 없으면(CI — 이번 사고의 정확한 재현 조건) 그 셋은
// `it.runIf`로 건너뛴다 — "CI에서 강제할 필요가 없다"(§8.3 재배치표)는 뜻이지 "느슨하게
// 통과시킨다"는 뜻이 아니다: 정본 반전(§8.2 ①) 덕분에 코드가 계약을 벗어나는 경로 자체가
// 문서 유무와 무관하게 이미 막혀 있다.

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { loadCodeConstants } from '../core/policy/codeConstants.js';
import { GENERATOR_VERSION, extractPubClassTableFullText } from '../../scripts/gen-contract-snapshot.mjs';
import { checkExtensionVersionSync } from './checks/check1a-extensionVersionSync.js';
import { checkCompatibilityDocSync } from './checks/check1-compatibilityDocSync.js';
import { checkPolicyFixtureSchema } from './checks/check2-policyFixtureSchema.js';
import { checkFixtureLeafCoverage } from './checks/check3-fixtureLeafCoverage.js';
import { checkFieldTableCrossSync } from './checks/check3c-fieldTableCrossSync.js';
import { checkPluginJsonFieldAccess } from './checks/check4-pluginJsonFieldAccess.js';
import { checkAllowedIdentifiers } from './checks/check5-allowedIdentifiers.js';
import { checkSnapshotIdentifierSync } from './checks/check5a-snapshotIdentifierSync.js';
import { checkAllowedNonEmpty } from './checks/check5b-allowedNonEmpty.js';
import { checkInstallTargetColumns } from './checks/check6-installTargetColumns.js';
import { checkCrossDocReferences } from './checks/check7-crossDocReferences.js';
import { checkHrsEvidence, getCommitTimestamp } from './checks/check8-hrsEvidence.js';
import { checkSensitiveValueScan } from './checks/check9-sensitiveValueScan.js';
import { checkSnapshotIntegrity } from './checks/check10-snapshotIntegrity.js';
import { checkSiteFaceDiscipline } from './checks/check11-siteFaceDiscipline.js';
import type { CheckResult } from './types.js';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..');
const DOCS = join(ROOT, 'docs');
const COMPAT = join(ROOT, 'compat');

const PACKAGE_JSON = join(ROOT, 'package.json');
const COMPATIBILITY_JSON = join(COMPAT, 'compatibility.json');
const POLICY_SAMPLE_JSON = join(COMPAT, 'policy.sample.json');
const SITE_EXAMPLE_JSON = join(COMPAT, 'site.example.json');
const SENSITIVE_CLASSES_JSON = join(COMPAT, 'sensitive-classes.json');
const CONTRACT_SNAPSHOT_JSON = join(COMPAT, 'contract-snapshot.json');
const POLICY_CONTRACT_MD = join(DOCS, 'policy-contract.md');
const ARCHITECTURE_MD = join(DOCS, 'architecture.md');
const SECURITY_PLAN_MD = join(DOCS, 'security-plan.md');
const TECH_STACK_MD = join(DOCS, 'tech-stack.md');
const MALGN_AUTH_MD = join(DOCS, 'malgn-auth-requirements.md');
const MALGNAI_HUB_MD = join(DOCS, 'malgnai-hub-requirements.md');
const DOCS_README_MD = join(DOCS, 'README.md');
const INSTALL_TARGETS_JSON = join(COMPAT, 'install-targets.json');
const AGENT_INTERFACE_SPEC_JSON = join(COMPAT, 'agent-interface.spec.json');
const EVIDENCE_DIR = join(COMPAT, 'verification-evidence');
const SRC_DIR = join(ROOT, 'src');
const CHECK3_SOURCE = join(HERE, 'checks', 'check3-fixtureLeafCoverage.ts');
const FIELD_COVERAGE_TEST_SOURCE = join(ROOT, 'src', 'core', 'policy', 'fieldCoverage.test.ts');

const CURRENT_EXTENSION_VERSION = (JSON.parse(readFileSync(PACKAGE_JSON, 'utf8')) as { version: string }).version;

/** docs/ 실재 여부만으로 모드를 정한다 — 플래그·환경변수는 쓰지 않는다(§8.3/§8.7 S4). */
const DOCS_PRESENT = existsSync(DOCS);

function reportOf(result: CheckResult): string {
  return result.violations.map((v) => `  - [${v.ref}] ${v.message}`).join('\n');
}

describe(`pnpm compat:check — policy-contract.md §6 검사 ①~⑪ (모드: ${DOCS_PRESENT ? 'full(docs/ 실재)' : 'ci(docs/ 부재)'})`, () => {
  it('①A compatibility.json.extensionVersion == package.json.version (PR-7)', () => {
    const result = checkExtensionVersionSync({ packageJsonPath: PACKAGE_JSON, compatibilityJsonPath: COMPATIBILITY_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it.runIf(DOCS_PRESENT)('①B 문서 사본 ↔ compatibility.json 실값 일치 (로컬 full 전용)', () => {
    const result = checkCompatibilityDocSync({
      compatibilityJsonPath: COMPATIBILITY_JSON,
      policyContractMdPath: POLICY_CONTRACT_MD,
      architectureMdPath: ARCHITECTURE_MD,
    });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('② 정책 fixture(compat/policy.sample.json)의 스키마 통과', () => {
    const result = checkPolicyFixtureSchema({
      policySampleJsonPath: POLICY_SAMPLE_JSON,
      currentExtensionVersion: CURRENT_EXTENSION_VERSION,
    });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('③ 정책 fixture의 모든 리프 경로가 전수 검증표의 행을 갖는지 (PR-11①)', () => {
    const result = checkFixtureLeafCoverage({ policySampleJsonPath: POLICY_SAMPLE_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('③C check3 리프 목록 ↔ fieldCoverage.test.ts 독립 사본 코드 대 코드 대조', () => {
    const result = checkFieldTableCrossSync({ check3SourcePath: CHECK3_SOURCE, fieldCoverageTestSourcePath: FIELD_COVERAGE_TEST_SOURCE });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('④ plugin.json 접근 경로가 spec fields 안인지', () => {
    const result = checkPluginJsonFieldAccess({ srcDir: SRC_DIR, agentInterfaceSpecPath: AGENT_INTERFACE_SPEC_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑤A 스냅샷 docIdentifiers == 코드 상수 allowed* 키 집합 (양방향, §8.5 D3)', () => {
    const result = checkSnapshotIdentifierSync({ contractSnapshotJsonPath: CONTRACT_SNAPSHOT_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑤B 모든 allowed* 키가 비어 있지 않고 플레이스홀더가 없는지 (문서 언급 무관)', () => {
    const result = checkAllowedNonEmpty();
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it.runIf(DOCS_PRESENT)('⑤C 문서의 allowed* 식별자 ⊆ 코드 상수 키 집합 (로컬 full 전용, PR-11②)', () => {
    const result = checkAllowedIdentifiers({ docPaths: [POLICY_CONTRACT_MD, ARCHITECTURE_MD] });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑥ allowedInstallTargets 격자 — 열 스펙 전량 + verified 불리언 리터럴 (PR-11③)', () => {
    const result = checkInstallTargetColumns({ installTargetsJsonPath: INSTALL_TARGETS_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it.runIf(DOCS_PRESENT)('⑦ 위성 문서의 architecture.md 절 참조가 실재 헤딩을 가리키는지 (로컬 full 전용, §7, M-18)', () => {
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

  it('⑨ 민감값 스캔 — 추적 트리 전체, 오프라인 패턴 + allowlist', () => {
    const result = checkSensitiveValueScan({ repoRoot: ROOT, sensitiveClassesJsonPath: SENSITIVE_CLASSES_JSON });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it('⑩ 스냅샷 무결성·신선도 (§8.5 D4 / §12.4 B3)', () => {
    const result = checkSnapshotIntegrity({
      contractSnapshotJsonPath: CONTRACT_SNAPSHOT_JSON,
      packageJsonPath: PACKAGE_JSON,
      currentGeneratorVersion: GENERATOR_VERSION,
      sensitiveClassesJsonPath: SENSITIVE_CLASSES_JSON,
    });
    expect(result.violations, reportOf(result)).toEqual([]);
  });

  it.runIf(DOCS_PRESENT)('⑩(f) 부류표 전문 해시 대조 — security-plan.md §11.5 실물 대조(B5, 로컬 full 전용)', () => {
    // B5(§12.4) — id 집합은 같아도 부류의 *정의 문구*가 바뀐 경우를 잡는다. 이 표는
    // 그 자체로 PUB-B(호스트·포트·항목명 실례)를 예시로 담고 있어 CI가 아니라 로컬
    // full 모드(docs/ 실재 = 관리자 기기)에서만 대조한다 — id 해시(B3)는 CI, 전문
    // 해시(B5)는 로컬. 대조 대상은 스냅샷에 이미 박제된 해시뿐이고, 표 원문 자체는
    // 이 테스트 실행 중에도 트리 밖으로 나가지 않는다(로컬 프로세스 메모리 안에서만 사용).
    const securityPlanMdText = readFileSync(SECURITY_PLAN_MD, 'utf8');
    const recomputed = `sha256:${createHash('sha256').update(extractPubClassTableFullText(securityPlanMdText), 'utf8').digest('hex')}`;
    const snapshot = JSON.parse(readFileSync(CONTRACT_SNAPSHOT_JSON, 'utf8')) as {
      sourceRegions?: readonly { doc?: unknown; region?: unknown; sha256?: unknown }[];
    };
    const fullRegion = (snapshot.sourceRegions ?? []).find((r) => r.doc === 'security-plan.md' && r.region === '§11.5 전문');
    expect(fullRegion, 'contract-snapshot.json에 security-plan.md §11.5 전문 region이 없습니다').toBeDefined();
    expect(fullRegion?.sha256, '표 문구가 바뀌었는데 스냅샷이 낡았습니다 — `pnpm contract:snapshot`을 다시 실행하십시오').toBe(recomputed);
  });

  it.runIf(DOCS_PRESENT)('A-34 — pre-push 훅이 무장되어 있는지(core.hooksPath, 로컬 full 전용, F-6 보강)', () => {
    // §8.6 2차선(pre-push 훅)은 로컬 git config라 다른 클론·다른 기기에는 따라가지
    // 않는다(security-report.md F-6 "통제를 용기에 걸었다"). 릴리스 빌드를 하는 관리자
    // 기기(docs/ 실재)에서만 강제해 CI를 방해하지 않는다.
    const hooksPath = execFileSync('git', ['config', '--get', 'core.hooksPath'], { cwd: ROOT, encoding: 'utf8' }).trim();
    expect(hooksPath, 'git config core.hooksPath가 .githooks가 아닙니다 — pre-push 민감값 스캔이 무장되지 않았습니다(AP-18)').toBe('.githooks');
  });

  it('⑪ 사이트면 규율 — 예시면은 예약 네임스페이스만 + 형태 일치 + 구멍 무잔존', () => {
    const result = checkSiteFaceDiscipline({
      siteExampleJsonPath: SITE_EXAMPLE_JSON,
      compatibilityJsonPath: COMPATIBILITY_JSON,
      contractSnapshotJsonPath: CONTRACT_SNAPSHOT_JSON,
      sensitiveClassesJsonPath: SENSITIVE_CLASSES_JSON,
    });
    expect(result.violations, reportOf(result)).toEqual([]);
  });
});
