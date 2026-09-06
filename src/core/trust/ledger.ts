// 대상 폴더 신뢰 원장 — architecture.md §3.6.1 "[C-11 해소] Workspace Trust 소멸분의
// 대체 — 겸직을 풀어 넷으로 나눈다" · §6.1(배치표: "동의 기록·저널·firstRunAt·코호트·
// 알림 억제 / 대상 폴더 신뢰 원장 / 백업 파일 / heartbeat.json → 앱 데이터 디렉터리")
// 정본 구현.
//
// [C-11이 요구하는 성질] Workspace Trust가 사라지면 `workspaceTrusted` 신호의
// *생산자*(구 `package.json`의 `capabilities.untrustedWorkspaces` 선언)도 함께 사라진다.
// 빈 소켓에 꽂힐 "자연스러운" 값은 `true`("신뢰 개념이 없으니 신뢰됨")이고, 그 상태에서도
// 기존 신호-주입 테스트는 전부 통과한다 — "함수는 옳게 동작하고 아무도 그것을 호출하지
// 않는" 조용한 통제 소멸이 C-11의 실질이다. 이 클래스가 그 생산자를 대체한다:
// **기본값은 언제나 `untrusted`**이고, 원장에 없는 폴더·원장 파일 자체가 없는 최초
// 실행 상태 모두 `untrusted`로 읽힌다 — "기본 허용"은 이 클래스 어디에도 없다.
//
// [배치] 원장은 **앱 데이터 디렉터리**에 두고 **대상 폴더 안에 두지 않는다**(§3.6.1 ③
// 원문: "대상 폴더 안에 두지 않는다 — 안에 두면 저장소가 자기 신뢰 상태를 스스로
// 선언하게 된다"). `JournalStore`(`core/journal/store.ts`)와 동일한 패턴으로 `baseDir`를
// 생성자 주입받는 순수 Node 모듈이다 — 실제 앱 데이터 디렉터리 경로를 계산하는 것은
// 이 클래스의 책임이 아니다(호스트 어댑터/트레이, W-N1 범위).
//
// [T-4 — I-B 매니저 경로 화이트리스트의 대체물 절반] policy-contract.md §2.2가 명문화한
// 대로, 이 원장은 *"이 폴더에서 작업해도 되는가"*만 답한다. *"이 실행파일이 우리가 아는
// 그것인가"*(p1, `allowedManagerPaths` 화이트리스트)는 이 원장이 대체하지 않는다 — 두
// 질문이 달라 둘 다 필요하다(`core/journal/installErrorCodes.ts`의
// `resolveManagerPathAllowlist`가 그 절반을 맡는다).
//
// [`trust.grant`는 STOPPABLE_SURFACES 밖이다] `trust.grant`(아래 `grant()`)는 정지될 수
// 없다 — architecture.md §3.6.1 ③ 원문: "이 표면은 `trust.grant`이며 `STOPPABLE_SURFACES`에
// 넣지 않는다 — 넣으면 ③이 자기 해소 경로를 막아 스스로 풀 수 없는 정지가 된다." 이
// 클래스 자체는 그 규율을 타입으로 강제하지 않는다(순수 저장소이고 "정지 표면"이라는
// 개념은 `core/reconciler/stopGate.ts`에만 있다) — `STOPPABLE_SURFACES`의 원소 개수가
// 정확히 2(`provider.apply`·`consent.issue`)로 고정된 것 자체가 `trust.grant`가 그
// 목록에 없다는 것의 증거다(`stopGate.test.ts` "길이 2" 검사).

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';

export type TrustState = 'trusted' | 'untrusted';

export interface TrustLedgerOptions {
  /** 앱 데이터 디렉터리(§6.1) — 대상 폴더 안이 아니다. 호출자가 실제 경로(호스트
   * 어댑터가 계산한 네이티브 앱 데이터 경로)를 주입한다 — 이 클래스는 그 경로를 스스로
   * 계산하지 않는다(`JournalStore`와 동일한 주입 패턴). */
  readonly baseDir: string;
  readonly fileName?: string;
}

const DEFAULT_FILE_NAME = 'trust-ledger.json';

type LedgerFileShape = Readonly<Record<string, TrustState>>;

function isEnoent(error: unknown): boolean {
  return typeof error === 'object' && error !== null && 'code' in error && (error as { code?: string }).code === 'ENOENT';
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isTrustState(value: unknown): value is TrustState {
  return value === 'trusted' || value === 'untrusted';
}

/** 손상되었거나 낯선 형태의 원장 파일을 안전하게 읽는다. 파싱 실패·불리언이 아닌 값·
 * 알 수 없는 문자열은 그 항목을 **버린다**(조용히 `'trusted'`로 승격하지 않는다) —
 * "손상은 신뢰 안 함으로 읽는다"는 fail-closed 원칙(N-8과 같은 종류: 자동 복구·재작성을
 * 하지 않고, 판단이 안 서면 더 안전한 방향으로 접는다). */
function sanitize(raw: unknown): LedgerFileShape {
  if (!isPlainRecord(raw)) return {};
  const out: Record<string, TrustState> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (isTrustState(value)) out[key] = value;
  }
  return out;
}

export class TrustLedger {
  readonly #filePath: string;
  #dirEnsured = false;

  constructor(options: TrustLedgerOptions) {
    this.#filePath = join(options.baseDir, options.fileName ?? DEFAULT_FILE_NAME);
  }

  /**
   * 대상 폴더의 신뢰 상태를 조회한다. **원장에 없으면(첫 대면) `'untrusted'`를
   * 반환한다** — C-11이 요구하는 "기본값 untrusted"의 실제 구현. 원장 파일 자체가
   * 없어도(최초 실행) 동일하게 `'untrusted'`다 — "원장이 비어 있다"와 "신뢰됐다"를
   * 구조적으로 구분한다. 경로는 `resolve()`로 정규화해 조회하므로 상대경로로 넘겨도
   * 일관되게 대조된다.
   */
  async get(targetFolderPath: string): Promise<TrustState> {
    const table = await this.#readAll();
    return table[resolve(targetFolderPath)] ?? 'untrusted';
  }

  /**
   * L2 동의 1회로 신뢰를 부여한다(§3.6.1 ③ "새 폴더가 처음 I-B 대상이 될 때 L2 동의
   * 1회, 절대경로 전문 표시"). 이 메서드를 호출하는 표면(`trust.grant`)은 정의상
   * 사람의 포그라운드 조작이고 `STOPPABLE_SURFACES` 밖이다 — 이 클래스는 동의를 받았는지
   * 확인하지 않는다(순수 저장소, 호출자가 이미 L2 동의를 받은 뒤에만 부른다는 계약).
   */
  async grant(targetFolderPath: string): Promise<void> {
    const key = resolve(targetFolderPath);
    const table = await this.#readAll();
    await this.#writeAll({ ...table, [key]: 'trusted' });
  }

  /**
   * 신뢰를 철회해 원장에서 지운다(다시 조회하면 기본값 `'untrusted'`로 돌아간다).
   * 운영 UI 표면(대시보드의 "신뢰 철회" 버튼 등)은 이 슬라이스 범위 밖이다 — 이
   * 메서드는 그 UI가 나중에 호출할 저장소 계층만 먼저 갖춘다.
   */
  async revoke(targetFolderPath: string): Promise<void> {
    const key = resolve(targetFolderPath);
    const table = await this.#readAll();
    if (!(key in table)) return;
    const rest: Record<string, TrustState> = { ...table };
    delete rest[key];
    await this.#writeAll(rest);
  }

  async #readAll(): Promise<LedgerFileShape> {
    let raw: string;
    try {
      raw = await readFile(this.#filePath, 'utf8');
    } catch (error) {
      if (isEnoent(error)) return {};
      throw error;
    }
    try {
      return sanitize(JSON.parse(raw));
    } catch {
      // 손상된 원장(파싱 실패) — 빈 원장(전부 untrusted)으로 취급한다. 자동
      // 재작성·복구는 하지 않는다(N-8 "자동 수정·재작성 금지"와 같은 원칙: 문제
      // 위치만 있는 그대로 두고 사람의 판단을 기다린다 — 여기서는 다음 `grant()`
      // 호출이 자연히 정상 파일로 덮어쓴다).
      return {};
    }
  }

  async #writeAll(table: LedgerFileShape): Promise<void> {
    if (!this.#dirEnsured) {
      await mkdir(dirname(this.#filePath), { recursive: true, mode: 0o700 });
      this.#dirEnsured = true;
    }
    await writeFile(this.#filePath, `${JSON.stringify(table, null, 2)}\n`, { encoding: 'utf8', mode: 0o600 });
  }
}

/**
 * `StopSignals.targetFolderTrusted`(core/reconciler/stopGate.ts)를 채우는 단일 변환
 * 지점 — 원장 조회 결과(`TrustState`)를 정지 게이트가 이해하는 불리언으로 옮긴다.
 * 이 함수를 거치지 않고 원장 조회 결과를 직접 다른 방식(예: 문자열 비교를 호출부마다
 * 반복)으로 불리언화하면 "trusted"/"Trusted"/"TRUSTED" 같은 표기 흔들림이 여러 곳에서
 * 각각 판단되며 갈릴 위험이 생긴다 — 안전 신호의 변환은 한 곳에서만 한다.
 */
export function computeTargetFolderTrusted(state: TrustState): boolean {
  return state === 'trusted';
}
