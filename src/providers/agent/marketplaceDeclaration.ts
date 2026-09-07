// 마켓플레이스 항목 선언 대체 통제 — architecture.md §3.2.2 "대체 통제: 마켓플레이스
// 항목 자체를 읽고, 명령 선언은 fail-closed". `.claude-plugin/marketplace.json`의
// `plugins[]` 원소 하나(agent-interface.spec.json이 가리키는 그 파일)를 받아 §3.2.2 ②를
// 판정하는 **순수 함수**다 — 파일을 실제로 읽는 것은 `marketplaceReader.ts`(호출자)의
// 몫이다.
//
// [화이트리스트] 아카이브 취득만 하는 선언형: `source`가 문자열(저장소 내부 상대
// 경로, 실측 malgn-agent 항목이 이 형태 — `"./malgn-agent"`)이거나, 객체이고
// `source.source`가 `github`/`git-subdir`/`url`(claude-plugins-official 실측에서 관측된
// 세 형태) 중 하나. **그 밖의 모든 형태(미지 source.source 값 포함)는 차단**(PR-6).
// 형태와 무관하게 엔트리 어디에든 명령 실행형 키(`command`/`install`/`script`/
// `headersHelper`)가 있으면 그 자체로 차단 — §3.2.2 "headersHelper가 이 목록에 있어야
// R-20이 무해해진다"의 실행 지점.

const FORBIDDEN_COMMAND_KEY_RE = /^(command|install|script|headersHelper)$/i;
const KNOWN_SOURCE_KINDS = new Set(['github', 'git-subdir', 'url']);

export interface MarketplaceDeclarationVerdict {
  readonly allowed: boolean;
  /** 차단 사유 — `allowed === true`면 빈 배열 */
  readonly forbiddenKeyPaths: readonly string[];
  readonly unknownSourceKind: string | null;
}

/** 엔트리 전체(문자열 리프 값의 "키 이름"만)를 재귀 순회해 명령 실행형 키를 전부
 * 찾는다 — `core/policy/sinkGuards.ts`의 `findSecretLikeKeyPath`와 같은 패턴(하나
 * 찾으면 멈추지 않고 전량을 모은다: 사람에게 "무엇을 원문에서 봤는지" 전부 보여주기
 * 위해서다, §3.2.2 "읽은 선언 원문을 보여주며"). */
function findForbiddenCommandKeyPaths(value: unknown, path: readonly string[] = []): string[] {
  if (value === null || typeof value !== 'object') return [];
  const found: string[] = [];
  if (Array.isArray(value)) {
    value.forEach((item, index) => found.push(...findForbiddenCommandKeyPaths(item, [...path, String(index)])));
    return found;
  }
  for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
    const nextPath = [...path, key];
    if (FORBIDDEN_COMMAND_KEY_RE.test(key)) {
      found.push(nextPath.join('.'));
    }
    found.push(...findForbiddenCommandKeyPaths(child, nextPath));
  }
  return found;
}

/**
 * `entry.source`의 형태를 판정한다. 문자열(상대 경로)은 항상 허용, 객체는
 * `source.source`가 화이트리스트 안일 때만 허용, 그 밖(숫자·null·배열·미지 값)은
 * 미지 형태로 차단한다.
 */
function evaluateSourceKind(source: unknown): { readonly allowed: boolean; readonly unknownKind: string | null } {
  if (typeof source === 'string') return { allowed: true, unknownKind: null };
  if (source !== null && typeof source === 'object' && !Array.isArray(source)) {
    const kind = (source as Record<string, unknown>).source;
    if (typeof kind === 'string' && KNOWN_SOURCE_KINDS.has(kind)) {
      return { allowed: true, unknownKind: null };
    }
    return { allowed: false, unknownKind: typeof kind === 'string' ? kind : JSON.stringify(kind) };
  }
  return { allowed: false, unknownKind: JSON.stringify(source) };
}

/**
 * §3.2.2 ② 판정의 정본 구현. `entry`는 `marketplace.json`의 `plugins[]` 원소 하나
 * (파싱된 JS 객체) — 파일 읽기·JSON.parse는 호출자 책임이다(순수 함수 유지).
 */
export function evaluateMarketplaceEntryDeclaration(entry: Record<string, unknown>): MarketplaceDeclarationVerdict {
  const forbiddenKeyPaths = findForbiddenCommandKeyPaths(entry);
  const sourceVerdict = evaluateSourceKind(entry.source);

  return {
    allowed: forbiddenKeyPaths.length === 0 && sourceVerdict.allowed,
    forbiddenKeyPaths,
    unknownSourceKind: sourceVerdict.unknownKind,
  };
}
