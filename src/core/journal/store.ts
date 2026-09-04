// 저널 저장소 — architecture.md §1.1(`core/journal` — "append-only") · §6.1
// (`globalStorageUri`, 비밀 아님·머신 로컬). 실제 `vscode.ExtensionContext.globalStorageUri`
// 배선은 이 슬라이스가 아니다(extension.ts는 W1 이후 손대지 않는다 — W2~W4와 동일한
// 판단, 반환문 참고) — 이 클래스는 `baseDir`를 주입받는 순수 Node 모듈이라
// `context.globalStorageUri.fsPath`를 그대로 넘기면 그대로 동작한다.
//
// [단일 지점] 파일에 실제로 바이트를 쓰는 곳은 `#appendLine` 한 곳이고, 그 안에서
// `maskSensitive()`를 거치지 않은 문자열은 `appendFile`에 닿지 않는다. 세 개의 공개
// `append*` 메서드는 형태만 다를 뿐 전부 이 한 곳으로 수렴한다.

import { mkdir, appendFile, readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { maskDeepValues } from '../diagnostics/mask.js';
import type { ChangeJournalEntry, ConsentFailureJournalEntry, InstallJournalEntry, JournalEntry } from './types.js';

export interface JournalStoreOptions {
  readonly baseDir: string;
  readonly fileName?: string;
}

const DEFAULT_FILE_NAME = 'journal.jsonl';

export class JournalStore {
  readonly #filePath: string;
  #dirEnsured = false;

  constructor(options: JournalStoreOptions) {
    this.#filePath = join(options.baseDir, options.fileName ?? DEFAULT_FILE_NAME);
  }

  /** §4.5 step⑥ — 일반 apply 저널(`{ts, provider, changes, backupPath, diffHash}`). */
  async appendChange(entry: Omit<ChangeJournalEntry, 'kind'>): Promise<void> {
    await this.#appendLine({ kind: 'change', ...entry });
  }

  /** §4.8.4 step7·policy-contract.md §4 — install 전용 저널. `envSet`이 R-19의
   * 사후 대조 근거다. */
  async appendInstall(entry: Omit<InstallJournalEntry, 'kind'>): Promise<void> {
    await this.#appendLine({ kind: 'install', ...entry });
  }

  /** §1.2 — `MV_CONSENT_INVALID`(high) 사고 신호. `MV_CONSENT_EXPIRED`(info)는 호출자가
   * 애초에 이 메서드를 부르지 않는다(`consentFailureRecorder.ts`가 그 구분을 담당). */
  async appendConsentFailure(entry: Omit<ConsentFailureJournalEntry, 'kind'>): Promise<void> {
    await this.#appendLine({ kind: 'consentFailure', ...entry });
  }

  /**
   * [단일 지점] 이 클래스에서 `appendFile`을 호출하는 유일한 곳. 세 공개 메서드가 전부
   * 여기로 수렴하므로 "마스킹을 거치지 않고 저널에 쓰는 경로"가 이 클래스 안에 없다.
   */
  async #appendLine(entry: JournalEntry): Promise<void> {
    if (!this.#dirEnsured) {
      await mkdir(dirname(this.#filePath), { recursive: true, mode: 0o700 });
      this.#dirEnsured = true;
    }
    // 방어심층 — 저널 필드는 설계상 비밀을 담지 않지만(argv·envSet은 매니저 경로·상수
    // 플래그뿐), 자유 텍스트 필드(Change.rationale 등)로 자격증명 형태 값이 실수로
    // 흘러드는 경로를 열어두지 않는다. `maskDeepValues`는 **리프 문자열 값에만** 적용돼
    // 키 이름(예: 38자인 `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK`)은 건드리지 않는다
    // (mask.ts 주석 참고 — 이 슬라이스의 `store.test.ts`가 처음 이 실패 형태로 재현했다).
    //
    // `diffHash`(sha256 64-hex)는 예외로 원문을 복원한다: 32자 이상 순수 영숫자라
    // HIGH_ENTROPY_RE에 항상 걸리지만 **비밀이 아니라 내용 해시**이고, §1.2 "diffHash
    // 재계산 비교"·R-19류 사후 대조의 유일한 근거라 마스킹하면 진단 기능 자체가
    // 무너진다. 예외는 이 필드 하나뿐이다 — "단일 지점" 알고리즘(`maskDeepValues`)은
    // 그대로 두고, 알고리즘이 훼손하는 유일한 비밀-아닌 구조적 필드만 복원한다.
    const masked = maskDeepValues(entry);
    const restored: JournalEntry =
      entry.kind === 'change' ? { ...(masked as ChangeJournalEntry), diffHash: entry.diffHash } : masked;
    const line = `${JSON.stringify(restored)}\n`;
    await appendFile(this.#filePath, line, { encoding: 'utf8', mode: 0o600 });
  }

  /** 저장된 전체 저널을 읽는다. 파일이 아직 없으면(첫 실행) 빈 배열 — 정상 상황이라
   * 예외로 표현하지 않는다. */
  async readAll(): Promise<readonly JournalEntry[]> {
    let raw: string;
    try {
      raw = await readFile(this.#filePath, 'utf8');
    } catch (error) {
      if (isEnoent(error)) return [];
      throw error;
    }
    return raw
      .split('\n')
      .filter((line) => line.length > 0)
      .map((line) => JSON.parse(line) as JournalEntry);
  }
}

function isEnoent(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'code' in error && (error as { code?: string }).code === 'ENOENT';
}
