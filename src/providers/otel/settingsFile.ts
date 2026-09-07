// `~/.claude/settings.json` 읽기/쓰기 — architecture.md §4.2 O-4 "OTel 세팅의 실체는
// ~/.claude/settings.json의 env 13키 + 최상위 otelHeadersHelper 경로". detect()는 이
// 파일을 읽기 전용으로만 다루고(agent/mcp와 같은 `ReadTextFile` DI 재사용 — 타입을
// 다시 정의하지 않는다), apply()만 실제로 쓴다.
//
// [쓰기는 최소 침습] 이 슬라이스는 `jsonc-parser`(주석·키 순서 보존 편집)를 새 의존성으로
// 들이지 않는다 — 실측(`~/.claude/settings.json`)이 주석 없는 순수 JSON이라 `JSON.parse`
// 왕복이 안전하기 때문이다. 원문 키 순서는 `JSON.parse`가 프로퍼티 삽입 순서를 보존하므로
// `env`가 아닌 다른 키는 그대로 유지된다 — 다만 사람이 수기로 넣은 주석이 있다면(이
// 파일 형식은 표준 JSON이라 원래 주석을 지원하지 않는다) 사라질 수 있다는 점은 정직하게
// 남겨 둔다(반환문에 명시할 설계 범위 축소, architecture.md §4.2의 "jsonc-parser edit API"
// 이상향과의 차이).

import { access, constants as fsConstants, mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { ReadTextFile } from '../agent/marketplaceReader.js';

/** 헤더 헬퍼 스크립트 경로가 실행 가능한지(존재 + x비트) 확인하는 DI 계약 —
 * `detect.ts`가 이 타입을 재수출해 쓴다(정의는 fs 어댑터가 모이는 이 파일에 둔다). */
export type PathExecutable = (path: string) => Promise<boolean>;

/** 실제 Node `fs.access(path, X_OK)`로 감싼 프로덕션 배선용 구현. */
export const nodePathExecutable: PathExecutable = async (path) => {
  try {
    await access(path, fsConstants.X_OK);
    return true;
  } catch {
    return false;
  }
};

export type { ReadTextFile };
export type WriteTextFile = (path: string, content: string) => Promise<void>;

export const SETTINGS_FILE_NAME = 'settings.json';

export function claudeSettingsPath(claudeHomeDir: string): string {
  return join(claudeHomeDir, SETTINGS_FILE_NAME);
}

export interface ClaudeSettingsSnapshot {
  /** 최상위 `env` 객체의 원문 스냅샷(전부 — 모르는 키까지) — plan()이 desired와 3-way
   * 비교할 때 "로컬에만 있는 키"(L0, 보고만)를 판정하려면 관리 대상 밖 키도 필요하다. */
  readonly env: Readonly<Record<string, string>>;
  readonly otelHeadersHelper: string | null;
}

export type ReadSettingsOutcome =
  | { readonly kind: 'ok'; readonly snapshot: ClaudeSettingsSnapshot }
  /** 파일 자체가 없음 — 최초 실행의 정상 상태(빈 env로 취급, 에러가 아니다) */
  | { readonly kind: 'missing' }
  | { readonly kind: 'unreadable'; readonly message: string };

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** env 객체 중 문자열 값만 추린다 — 형태가 이상한 값(숫자·객체 등)은 조용히 버린다
 * (그 자리는 "관측 안 됨"으로 취급되어 plan()이 add로 판단한다, fail-closed와 같은 결). */
function sanitizeEnv(raw: unknown): Record<string, string> {
  if (!isPlainObject(raw)) return {};
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (typeof value === 'string') out[key] = value;
  }
  return out;
}

export async function readClaudeSettings(path: string, readTextFile: ReadTextFile): Promise<ReadSettingsOutcome> {
  let raw: string;
  try {
    raw = await readTextFile(path);
  } catch (error) {
    const code = (error as { code?: string } | undefined)?.code;
    if (code === 'ENOENT') return { kind: 'missing' };
    return { kind: 'unreadable', message: error instanceof Error ? error.message : String(error) };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    return { kind: 'unreadable', message: `JSON 파싱 실패: ${error instanceof Error ? error.message : String(error)}` };
  }
  if (!isPlainObject(parsed)) {
    return { kind: 'unreadable', message: 'settings.json 최상위가 객체가 아닙니다' };
  }
  const otelHeadersHelper = typeof parsed.otelHeadersHelper === 'string' ? parsed.otelHeadersHelper : null;
  return { kind: 'ok', snapshot: { env: sanitizeEnv(parsed.env), otelHeadersHelper } };
}

/**
 * `settings.json`의 `env` 객체에서 `updates`의 키만 병합해 쓴다(다른 최상위 키·`env`의
 * 다른 키는 그대로 보존). 쓰기 직전 현재 내용을 `backupsDir/<timestamp>/settings.json`에
 * 백업하고(NT-R20, `writeBackupFile`과 동일 원칙 — 이 파일은 그 함수를 재사용하지 않고
 * 직접 구현한다: 백업 대상 내용을 이미 이 함수가 갖고 있어 재조회가 필요 없기 때문이다),
 * 임시 파일에 쓴 뒤 원자적으로 `rename`한다. 파일이 아예 없으면 새로 만든다(최초 실행).
 */
export async function writeClaudeSettingsEnvAtomic(options: {
  readonly path: string;
  readonly readTextFile: ReadTextFile;
  readonly updates: Readonly<Record<string, string>>;
  readonly backupsDir: string;
  readonly now: () => Date;
}): Promise<{ readonly backupPath: string | null }> {
  const currentRaw = await readRawObject(options.path, options.readTextFile);
  const fileExisted = currentRaw !== null;
  const base = currentRaw ?? {};

  let backupPath: string | null = null;
  if (fileExisted) {
    const ts = options.now().toISOString().replace(/[:.]/g, '-');
    const dir = join(options.backupsDir, ts);
    await mkdir(dir, { recursive: true, mode: 0o700 });
    backupPath = join(dir, SETTINGS_FILE_NAME);
    await writeFile(backupPath, `${JSON.stringify(base, null, 2)}\n`, { encoding: 'utf8', mode: 0o600 });
  }

  const nextEnv = { ...(isPlainObject(base.env) ? base.env : {}), ...options.updates };
  const next = { ...base, env: nextEnv };

  const tmpPath = `${options.path}.tmp-${process.pid}-${Date.now()}`;
  await writeFile(tmpPath, `${JSON.stringify(next, null, 2)}\n`, 'utf8');
  await rename(tmpPath, options.path);

  return { backupPath };
}

/** 원문 그대로(sanitize 없이) 읽는다 — 백업·병합 둘 다 "모르는 키까지 그대로 보존"이
 * 필요해서다. 파일이 없으면 `null`(최초 실행, 백업 대상 자체가 없다는 신호). 파싱
 * 실패·객체 아님은 빈 객체로 접는다(§4.2 병합 규칙이 요구하는 "쓰기는 최소 범위만"의
 * fail-closed 쪽 — 손상된 파일을 그대로 베껴 쓰지 않는다). */
async function readRawObject(path: string, readTextFile: ReadTextFile): Promise<Record<string, unknown> | null> {
  let raw: string;
  try {
    raw = await readTextFile(path);
  } catch {
    return null;
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    return isPlainObject(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

/** 실제 Node `readFile`를 `ReadTextFile` 계약으로 감싼다 — 프로덕션 배선용(테스트는
 * 가짜 `ReadTextFile`을 직접 주입한다, agent/mcp와 동일 관용구). */
export const nodeReadTextFile: ReadTextFile = (path) => readFile(path, 'utf8');
