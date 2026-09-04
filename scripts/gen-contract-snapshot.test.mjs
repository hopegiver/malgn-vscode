// `pnpm contract:snapshot`(scripts/gen-contract-snapshot.mjs) 검증 — docs/policy-contract.md
// §8.5 D1(생성 전용 + 선행조건)·D2(반타우톨로지 규칙)·D4(무결성 — 값형 필드 부재/digest
// 재계산) 근거.

import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { extractPubClassIds, extractPubClassTableFullText, generateContractSnapshot, main } from './gen-contract-snapshot.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const SCRIPT = join(HERE, 'gen-contract-snapshot.mjs');

const DOC_A = `## 필드별 전수 검증표

| 필드 | 싱크 | 규칙 | 위반 시 |
|---|---|---|---|
| \`schemaVersion\` | S5 | 정수 | 거부 |
| \`agent.marketplace\` | S1 | ① 형식 ② **\`allowedMarketplaces\`** | 폐기 |

## 2. 코드 상수 계약

\`\`\`jsonc
// compat/compatibility.json  ← 이 값들의 유일한 정본
{ "allowedAuthorities": {
    "otel":      ["192.0.2.10:1"],
    "extension": ["*.a.example"] },
  "allowedKeychainItems": ["x"] }
\`\`\`
`;

const DOC_B = `## 필드별 전수 검증표

| 필드 | 싱크 | 규칙 | 위반 시 |
|---|---|---|---|
| \`schemaVersion\` | S5 | 정수 | 거부 |
| \`agent.marketplace\` | S1 | ① 형식 ② **\`allowedMarketplaces\`** | 폐기 |
| \`github.requiredScopes[]\` | S1 | ① 원소 ② **\`allowedGithubScopes\`** | 폐기 |

## 2. 코드 상수 계약

\`\`\`jsonc
// compat/compatibility.json  ← 이 값들의 유일한 정본
{ "allowedAuthorities": {
    "otel":      ["198.51.100.20:1"],
    "extension": ["*.b.example"],
    "identity":  ["c.example", "d.example"] },
  "allowedKeychainItems": ["y", "z"] }
\`\`\`
`;

describe('generateContractSnapshot — 스냅샷은 docs 텍스트를 따라간다', () => {
  it('doc 텍스트가 다르면 siteShape·docIdentifiers·leafRows가 그에 맞게 달라진다', () => {
    const snapA = generateContractSnapshot({ policyContractMdText: DOC_A, architectureMdText: '', extensionVersion: '0.1.0' });
    const snapB = generateContractSnapshot({ policyContractMdText: DOC_B, architectureMdText: '', extensionVersion: '0.1.0' });

    expect(snapA.siteShape.authorities.keys).toEqual(['otel', 'extension']);
    expect(snapA.siteShape.keychainItems.count).toBe(1);
    expect(snapA.leafRows).toEqual(['schemaVersion', 'agent.marketplace']);
    expect(snapA.docIdentifiers).toEqual(['allowedMarketplaces']);

    expect(snapB.siteShape.authorities.keys).toEqual(['otel', 'extension', 'identity']);
    expect(snapB.siteShape.authorities.counts.identity).toBe(2);
    expect(snapB.siteShape.keychainItems.count).toBe(2);
    expect(snapB.leafRows).toEqual(['schemaVersion', 'agent.marketplace', 'github.requiredScopes[]']);
    expect(snapB.docIdentifiers).toEqual(['allowedGithubScopes', 'allowedMarketplaces']);

    expect(snapA.digest).not.toBe(snapB.digest);
  });

  it('스냅샷에 실제 IP·도메인·keychain 항목 값이 담기지 않는다(이름·개수만)', () => {
    const snap = generateContractSnapshot({ policyContractMdText: DOC_A, architectureMdText: '', extensionVersion: '0.1.0' });
    const serialized = JSON.stringify(snap);
    expect(serialized).not.toContain('192.0.2.10');
    expect(serialized).not.toContain('a.example');
    expect(serialized).not.toContain('"x"');
  });

  it('digest는 재계산 시 결정적으로 같다(D4 self-digest 재계산의 전제)', () => {
    const snap1 = generateContractSnapshot({ policyContractMdText: DOC_A, architectureMdText: '', extensionVersion: '0.1.0' });
    const snap2 = generateContractSnapshot({ policyContractMdText: DOC_A, architectureMdText: '', extensionVersion: '0.1.0' });
    expect(snap1.digest).toBe(snap2.digest);
  });
});

// --- §12.4 B1/B2 — security-plan.md §11.5 부류표 바인딩 ---
const SECURITY_PLAN_DOC = `## 11.5 Q9-4 [핵심 산출] public 저장소 반입 금지 부류 정의표

| 부류 | 판정 질문 (Yes = 이 부류) | 해당 값 (이 프로젝트 실례) | public git | 위반 시 조치 |
|---|---|---|---|---|
| **PUB-X**<br>절대 금지 | ①이 값을 아는 것만으로 | GitHub PAT | **금지** | 즉시 폐기 |
| **PUB-A**<br>개인식별 | ②이 값이 개인을 식별 | 사번·실명 | **금지** | 회전 불가 |
| **PUB-B**<br>내부 지형 | ③이 값이 내부 지형 | 역할↔호스트 매핑 | **기본 금지** | 반입 전 ④ 판정 |
| **PUB-C**<br>공개 계약 | ④이 값이 모든 사용자 기기에 배포 | allowedAuthorities | **반입 가능** | — |
| **PUB-D**<br>무해 | 위 넷 모두 No | 로그 레벨 | 반입 가능 | 없음 |

### 다음 절
`;

describe('extractPubClassIds/extractPubClassTableFullText — §11.5 표 바인딩(B1/B2)', () => {
  it('부류 id 5종만 뽑는다(값·질문·실례는 버린다)', () => {
    expect(extractPubClassIds(SECURITY_PLAN_DOC)).toEqual(['PUB-A', 'PUB-B', 'PUB-C', 'PUB-D', 'PUB-X']);
  });

  it('표가 없으면 빈 배열을 반환한다', () => {
    expect(extractPubClassIds('아무 내용 없음')).toEqual([]);
  });

  it('전문 해시는 표 내용이 바뀌면 함께 바뀐다(B5 전제)', () => {
    const fullTextA = extractPubClassTableFullText(SECURITY_PLAN_DOC);
    const changed = SECURITY_PLAN_DOC.replace('GitHub PAT', 'GitHub PAT(변경됨)');
    const fullTextB = extractPubClassTableFullText(changed);
    expect(fullTextA).not.toEqual(fullTextB);
    // id 집합 자체는 문구 변경으로 흔들리지 않는다(id 해시와 전문 해시가 각기 다른 축을 잡는다는 증거).
    expect(extractPubClassIds(SECURITY_PLAN_DOC)).toEqual(extractPubClassIds(changed));
  });
});

describe('generateContractSnapshot — security-plan.md §11.5를 sourceRegions에 담는다(B2)', () => {
  it('§11.5 부류 id·전문 두 region이 sourceRegions에 있다', () => {
    const snap = generateContractSnapshot({
      policyContractMdText: DOC_A,
      architectureMdText: '',
      securityPlanMdText: SECURITY_PLAN_DOC,
      extensionVersion: '0.1.0',
    });
    const idRegion = snap.sourceRegions.find((r) => r.doc === 'security-plan.md' && r.region === '§11.5 부류 id');
    const fullRegion = snap.sourceRegions.find((r) => r.doc === 'security-plan.md' && r.region === '§11.5 전문');
    expect(idRegion).toBeDefined();
    expect(fullRegion).toBeDefined();
    expect(idRegion.sha256).toMatch(/^sha256:[0-9a-f]{64}$/);
    // 스냅샷 전체 직렬화에 실제 예시 값(GitHub PAT 등)이 값으로는 안 남는다 — 해시로만 존재.
    expect(JSON.stringify(snap)).not.toContain('GitHub PAT');
  });
});

// --- D2 반타우톨로지: 생성기는 compat/를 입력으로 받지 않는다(구조적 증거 + 행동 증거) ---
describe('D2 — 생성기는 docs/에서만 읽는다(compat/를 읽지 않는다)', () => {
  it('함수 시그니처가 docs 관련 키만 받는다(compat 경로 파라미터가 없다)', () => {
    // generateContractSnapshot({...})의 유일한 파라미터는 객체 하나이며, 그 구현은
    // `policyContractMdText`/`architectureMdText`/`extensionVersion`만 destructuring한다.
    // compat 관련 키를 추가로 넣어도(아래 테스트) 결과에 영향이 없다는 것이 그 증거다.
    expect(generateContractSnapshot.length).toBe(1);
  });

  it('입력 객체에 compat 형태의 가짜 데이터를 얹어도 무시된다 — siteShape은 여전히 docs 텍스트만 반영한다', () => {
    const maliciousCompatData = {
      allowedAuthorities: { otel: ['192.0.2.60:1', '192.0.2.70:1', '192.0.2.80:1'] }, // doc과 다른 개수/값
      allowedKeychainItems: ['fabricated-1', 'fabricated-2', 'fabricated-3', 'fabricated-4'],
    };
    const snap = generateContractSnapshot({
      policyContractMdText: DOC_A,
      architectureMdText: '',
      extensionVersion: '0.1.0',
      // D2가 실제로 성립한다면 아래 두 키는 함수 내부에서 아예 읽히지 않는다.
      compatibilityJson: maliciousCompatData,
      compatibilityJsonPath: '/compat/compatibility.json',
    });
    // doc A의 실제 형태(otel 카운트 1, keychainItems 카운트 1)가 그대로 나와야 한다 —
    // 가짜 compat 데이터(otel 카운트 3, keychainItems 카운트 4)가 반영되면 D2 위반이다.
    expect(snap.siteShape.authorities.counts.otel).toBe(1);
    expect(snap.siteShape.keychainItems.count).toBe(1);
  });
});

describe('scripts/gen-contract-snapshot.mjs — main() D1(docs/ 부재 시 거부)', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });

  function makeTempRepo({ withDocs }) {
    const dir = mkdtempSync(join(tmpdir(), 'malgn-snapshot-'));
    dirs.push(dir);
    mkdirSync(join(dir, 'compat'), { recursive: true });
    writeFileSync(join(dir, 'package.json'), JSON.stringify({ version: '0.1.0' }), 'utf8');
    if (withDocs) {
      mkdirSync(join(dir, 'docs'), { recursive: true });
      writeFileSync(join(dir, 'docs', 'policy-contract.md'), DOC_A, 'utf8');
      writeFileSync(join(dir, 'docs', 'architecture.md'), '', 'utf8');
      writeFileSync(join(dir, 'docs', 'security-plan.md'), '', 'utf8');
    }
    return dir;
  }

  it('docs/가 없으면 main()이 던진다(D1)', () => {
    const repo = makeTempRepo({ withDocs: false });
    expect(() => main(repo)).toThrow(/docs\/ 부재/);
  });

  it('docs/가 있으면 compat/contract-snapshot.json을 생성한다', () => {
    const repo = makeTempRepo({ withDocs: true });
    const snap = main(repo);
    expect(snap.siteShape.authorities.keys).toEqual(['otel', 'extension']);
    const written = JSON.parse(readFileSync(join(repo, 'compat', 'contract-snapshot.json'), 'utf8'));
    expect(written.digest).toBe(snap.digest);
  });

  it('실제 CLI(node scripts/gen-contract-snapshot.mjs)도 docs/ 없으면 0이 아닌 종료 코드로 실패한다', () => {
    const repo = makeTempRepo({ withDocs: false });
    expect(() =>
      execFileSync('node', [SCRIPT], { env: { ...process.env, MALGN_GEN_SITE_ROOT: repo }, stdio: 'pipe' })
    ).toThrow();
  });
});
