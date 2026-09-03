#!/usr/bin/env node
// `pnpm contract:snapshot` — docs/policy-contract.md §8.5 "스냅샷이 원본에서 벗어나는 것을
// 막는 장치" 정본 구현. 이름·형태·해시만 담은 `compat/contract-snapshot.json`을 emit한다
// (값 0 — 실제 authority·IP·keychain 항목명은 담지 않는다).
//
// D1(생성 전용 + 선행조건): 이 스크립트는 `docs/`가 없으면 거부한다(§8.5 D1) — 정본 문서가
// 로컬에만 있는 상태에서만 생성 가능하다.
// D2(반타우톨로지 규칙): `generateContractSnapshot()`의 시그니처는 **docs 텍스트만** 받는다
// — `compat/`의 어떤 경로도 파라미터로 받지 않는다. 이 함수가 `compat/`를 읽을 방법이
// 애초에 없다는 것이 "생성기는 emit하는 필드를 docs/에서만 읽는다"의 구조적 증거다
// (gen-contract-snapshot.test.mjs가 이를 행동으로도 증명한다).

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = join(HERE, '..');

export const GENERATOR_VERSION = '1';
export const SNAPSHOT_VERSION = 1;

function sha256(text) {
  return `sha256:${createHash('sha256').update(text, 'utf8').digest('hex')}`;
}

/** 정규화 self-digest — 키를 정렬해 순서 변화가 해시를 흔들지 않게 한다. */
function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    const keys = Object.keys(value).sort();
    return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalJson(value[k])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

const BACKTICK_ALLOWED_RE = /`(allowed[A-Za-z]+)(?:\.[A-Za-z]+)?`/g;

/** 문서 텍스트에서 backtick으로 등장하는 `allowed*` 식별자 이름만 뽑는다(값 아님). */
function extractDocIdentifiers(text) {
  const out = new Set();
  BACKTICK_ALLOWED_RE.lastIndex = 0;
  let m;
  while ((m = BACKTICK_ALLOWED_RE.exec(text)) !== null) {
    if (m[1]) out.add(m[1]);
  }
  return [...out].sort();
}

/**
 * "필드별 전수 검증표"(policy-contract.md 상단) 마크다운 표의 각 행 첫 컬럼을 이름만
 * 뽑는다(값 아님 — 규칙·위반 컬럼은 버린다). 백틱·볼드 마크업을 걷어내 사람이 읽는
 * 라벨 텍스트만 남긴다.
 */
function extractLeafRows(policyContractMdText) {
  const headerIdx = policyContractMdText.indexOf('| 필드 | 싱크 | 규칙 | 위반 시 |');
  if (headerIdx === -1) return [];
  const rest = policyContractMdText.slice(headerIdx);
  const lines = rest.split('\n');
  const rows = [];
  // lines[0] = 헤더, lines[1] = 구분선(|---|...), lines[2..] = 데이터 행. `|`로 시작하지
  // 않는 첫 줄에서 멈춘다(표 끝).
  for (let i = 2; i < lines.length; i += 1) {
    const line = lines[i];
    if (!line || !line.trimStart().startsWith('|')) break;
    const cells = line.split('|');
    const firstCell = cells[1] ?? '';
    const label = firstCell
      .replace(/\*\*/g, '')
      .replace(/`/g, '')
      .replace(/\s+/g, ' ')
      .trim();
    if (label.length > 0) rows.push(label);
  }
  return [...new Set(rows)];
}

/** §2 JSONC 코드펜스(`compat/compatibility.json` 예시) 블록 텍스트를 찾는다. */
function extractSection2Block(policyContractMdText) {
  const fenceRe = /```jsonc\n([\s\S]*?)\n```/g;
  let m;
  while ((m = fenceRe.exec(policyContractMdText)) !== null) {
    if (m[1] && m[1].includes('compat/compatibility.json')) return m[1];
  }
  return null;
}

function stripJsonComments(text) {
  let out = '';
  let inString = false;
  let escapeNext = false;
  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    const next = text[i + 1];
    if (inString) {
      out += ch;
      if (escapeNext) escapeNext = false;
      else if (ch === '\\') escapeNext = true;
      else if (ch === '"') inString = false;
      continue;
    }
    if (ch === '"') {
      inString = true;
      out += ch;
      continue;
    }
    if (ch === '/' && next === '/') {
      while (i < text.length && text[i] !== '\n') i += 1;
      out += '\n';
      continue;
    }
    out += ch;
  }
  return out;
}

/**
 * §2 JSONC 예시에서 allowedAuthorities/allowedKeychainItems의 **형태만**(키 이름·배열
 * 길이) 뽑는다 — 값(실제 IP·도메인·항목명)은 절대 담지 않는다.
 */
function extractSiteShape(section2Text) {
  if (section2Text === null) return { authorities: { keys: [], counts: {} }, keychainItems: { count: 0 } };
  // §2 JSONC는 `$ref` 참조를 포함해 완전한 JSON이 아니다(install-targets.json 등을
  // $ref로 인용) — 부분 블록만 정규식으로 뽑아 그 부분만 파싱한다.
  // 이 블록 안에는 배열([...])과 주석만 있고 다른 객체({...})가 없으므로, non-greedy로
  // 첫 `}`까지 잡으면 정확히 닫는 중괄호에서 멈춘다(들여쓰기·같은 줄 여부에 기대지 않는다).
  const authoritiesMatch = /"allowedAuthorities":\s*\{([\s\S]*?)\}/.exec(section2Text);
  const keychainMatch = /"allowedKeychainItems":\s*(\[[^\]]*\])/.exec(section2Text);

  const authoritiesShape = { keys: [], counts: {} };
  if (authoritiesMatch?.[1]) {
    const body = stripJsonComments(authoritiesMatch[1]);
    const entryRe = /"([a-zA-Z]+)":\s*(\[[^\]]*\])/g;
    let em;
    while ((em = entryRe.exec(body)) !== null) {
      const key = em[1];
      const arrText = em[2];
      if (!key || !arrText) continue;
      let arr;
      try {
        arr = JSON.parse(arrText);
      } catch {
        continue;
      }
      authoritiesShape.keys.push(key);
      authoritiesShape.counts[key] = Array.isArray(arr) ? arr.length : 0;
    }
  }

  let keychainCount = 0;
  if (keychainMatch?.[1]) {
    try {
      const arr = JSON.parse(stripJsonComments(keychainMatch[1]));
      keychainCount = Array.isArray(arr) ? arr.length : 0;
    } catch {
      keychainCount = 0;
    }
  }

  return { authorities: authoritiesShape, keychainItems: { count: keychainCount } };
}

/**
 * D2(반타우톨로지) 정본 — 이 함수는 **compat/에 대한 어떤 경로도 파라미터로 받지 않는다**.
 * `input`에 다른 키(예: compatibilityJson)가 섞여 들어와도 무시한다(destructuring이
 * 명시된 키만 뽑는다) — 그것이 D2 단위 테스트가 증명하는 성질이다.
 */
export function generateContractSnapshot({ policyContractMdText, architectureMdText, extensionVersion }) {
  const docIdentifiers = [
    ...new Set([...extractDocIdentifiers(policyContractMdText), ...extractDocIdentifiers(architectureMdText)]),
  ].sort();
  const leafRows = extractLeafRows(policyContractMdText);
  const section2Block = extractSection2Block(policyContractMdText);
  const siteShape = extractSiteShape(section2Block);

  const sourceRegions = [
    { doc: 'policy-contract.md', region: '§2', sha256: sha256(section2Block ?? '') },
    { doc: 'policy-contract.md', region: '필드별 전수 검증표', sha256: sha256(leafRows.join('\n')) },
  ];

  const withoutDigest = {
    snapshotVersion: SNAPSHOT_VERSION,
    generatorVersion: GENERATOR_VERSION,
    extensionVersion,
    docIdentifiers,
    leafRows,
    siteShape,
    sourceRegions,
  };

  const digest = sha256(canonicalJson(withoutDigest));
  return { ...withoutDigest, digest };
}

export function main(rootOverride) {
  const root = rootOverride ?? process.env.MALGN_GEN_SITE_ROOT ?? DEFAULT_ROOT;
  const docsDir = join(root, 'docs');
  const policyContractMdPath = join(docsDir, 'policy-contract.md');
  const architectureMdPath = join(docsDir, 'architecture.md');
  const packageJsonPath = join(root, 'package.json');
  const outPath = join(root, 'compat', 'contract-snapshot.json');

  // D1 — docs/ 부재 시 거부(생성 전용 + 선행조건).
  if (!existsSync(docsDir) || !existsSync(policyContractMdPath) || !existsSync(architectureMdPath)) {
    throw new Error(`docs/ 부재: '${policyContractMdPath}' 또는 '${architectureMdPath}'를 찾을 수 없습니다 (§8.5 D1)`);
  }

  const policyContractMdText = readFileSync(policyContractMdPath, 'utf8');
  const architectureMdText = readFileSync(architectureMdPath, 'utf8');
  const extensionVersion = JSON.parse(readFileSync(packageJsonPath, 'utf8')).version;

  const snapshot = generateContractSnapshot({ policyContractMdText, architectureMdText, extensionVersion });
  writeFileSync(outPath, `${JSON.stringify(snapshot, null, 2)}\n`, 'utf8');
  console.log(`[contract:snapshot] → ${outPath}`);
  return snapshot;
}

const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isDirectRun) {
  try {
    main();
  } catch (error) {
    console.error(`[contract:snapshot] 생성 실패: ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
