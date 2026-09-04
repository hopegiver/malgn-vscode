// 싱크별 공통 가드 — policy-contract.md §0 (S1~S6 싱크 분류)의 "분류에 따라 자동으로
// 요구되는 통제" 열을 재사용 가능한 함수로 옮긴 것. 필드 검증기(loader.ts)가 이 함수들을
// 호출해 같은 싱크의 여러 필드에 항상 같은 통제를 적용하게 한다 — 필드마다 따로
// 구현하면 다음 필드에서 또 하나를 빠뜨리는 것이 이 프로젝트의 반복된 실패 패턴이었다.

/** S1(프로세스 argv) 공통 가드 ③ — 공백·`;`·`&`·`|`·`$`·백틱·개행 */
const SHELL_META_RE = /[\s;&|$`\r\n]/;

export function hasShellMetacharacters(value: string): boolean {
  return SHELL_META_RE.test(value);
}

/** S1 공통 가드 ③ — 선행 `-`(플래그로 오인되는 인자) 거부 */
export function hasLeadingDash(value: string): boolean {
  return value.startsWith('-');
}

/** S1 값이 argv에 실리기 전 마지막 방어선. 하나라도 걸리면 값 전체를 거부한다. */
export function isSafeArgvValue(value: string): boolean {
  return !hasShellMetacharacters(value) && !hasLeadingDash(value);
}

/**
 * S2(네트워크 목적지) 공통 가드 ① — `https:` 강제. URL 파싱 실패도 거부(null)로 취급한다.
 * 반환값은 "host" 또는 "host:port"(포트가 명시된 경우) — allowedAuthorities 매칭 단위.
 */
export function extractHttpsAuthority(url: string): string | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== 'https:') return null;
  return parsed.port ? `${parsed.hostname}:${parsed.port}` : parsed.hostname;
}

/**
 * S2 공통 가드 ② — 완전일치 또는 `*.suffix`(포트 포함 가능). §2.3: "같은 규칙이
 * 모든 화이트리스트에 적용된다 — 정책은 목록을 줄일 수는 있어도 늘릴 수 없다"의
 * 실행 지점이 바로 이 멤버십 검사다(코드 상수 화이트리스트 자체를 정책이 못 건드리므로
 * "늘릴 수 없다"는 이 함수가 항상 코드 상수만을 allowList로 받는 것으로 강제된다).
 */
export function authorityAllowed(authority: string, allowList: readonly string[]): boolean {
  return allowList.some((entry) => {
    if (entry.startsWith('*.')) {
      const suffix = entry.slice(1); // ".suffix"
      return authority.length > suffix.length && authority.endsWith(suffix);
    }
    return authority === entry;
  });
}

/** S3(자격증명 조회 키) 공통 가드 ② */
const S3_KEY_RE = /^[A-Za-z0-9._-]{1,64}$/;

export function isValidCredentialLookupKey(value: string): boolean {
  return S3_KEY_RE.test(value);
}

/** PR-5(정책 무비밀) — 문서 전체에 적용되는 유일한 파일 전체 거부급 키 이름 검사.
 * A-33(F-5, security-report.md) — 원래 정규식은 `apiKey`·`apiToken`은 `token` 부분
 * 매칭으로 잡았지만 `credential`·`privateKey`·`passwd`·`pwd`·`bearer`·`passphrase`는
 * 놓쳤다(그릇 이름만 바꾸면 통과). `cert`는 `certificate` 같은 비-비밀 필드명과 충돌할
 * 위험이 있어 이번 확장에서 제외한다(security-report.md §2 F-5 "선택" 표기). */
const SECRET_KEY_RE = /token|secret|password|authorization|credential|private[_-]?key|passwd|pwd|bearer|passphrase/i;

/**
 * 파싱된 정책 객체를 재귀적으로 순회해 시크릿 형태의 "키 이름"이 하나라도 있으면
 * 그 경로를 반환한다(없으면 null). 값이 아니라 **키 이름**만 검사한다 — PR-5의
 * 취지는 "정책 파일이 비밀을 나르는 그릇이 되지 않는다"이며 그런 그릇은 키 이름에서
 * 먼저 드러난다.
 */
export function findSecretLikeKeyPath(value: unknown, path: readonly string[] = []): string | null {
  if (value === null || typeof value !== 'object') return null;
  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i += 1) {
      const found = findSecretLikeKeyPath(value[i], [...path, String(i)]);
      if (found) return found;
    }
    return null;
  }
  for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
    const nextPath = [...path, key];
    if (SECRET_KEY_RE.test(key)) return nextPath.join('.');
    const found = findSecretLikeKeyPath(child, nextPath);
    if (found) return found;
  }
  return null;
}
