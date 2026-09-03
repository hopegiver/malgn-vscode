// 최소 semver 파서 — policy-contract.md §2.3(PR-9 좁히기 전용 규칙)이 요구하는 두 연산만
// 지원한다: 버전 비교, 그리고 "번들 범위 ∩ 정책 범위" 계산. 이 계약에 등장하는 range는
// 항상 `>=X.Y.Z` 또는 `>=X.Y.Z <A.B.C` 형태(공백 구분, 최대 2 comparator)뿐이므로
// 범용 semver 라이브러리를 새 의존성으로 들이지 않고 이 형태만 정확히 다룬다
// (pnpm 외 패키지 매니저를 쓰지 않는다는 제약과는 무관하게, 이 슬라이스에 새 런타임
// 의존성을 추가하지 않기 위한 선택이다).

export interface ParsedVersion {
  readonly major: number;
  readonly minor: number;
  readonly patch: number;
}

const VERSION_RE = /^(\d+)(?:\.(\d+))?(?:\.(\d+))?/;

/** "2.1.237" 형태만 인정한다. prerelease/build metadata(-beta, +build)는 버린다(무시). */
export function parseVersion(input: string): ParsedVersion | null {
  const trimmed = input.trim();
  const m = VERSION_RE.exec(trimmed);
  if (!m || m[1] === undefined) return null;
  const major = Number(m[1]);
  const minor = m[2] !== undefined ? Number(m[2]) : 0;
  const patch = m[3] !== undefined ? Number(m[3]) : 0;
  if (!Number.isInteger(major) || !Number.isInteger(minor) || !Number.isInteger(patch)) {
    return null;
  }
  return { major, minor, patch };
}

export function isValidSemver(input: string): boolean {
  return parseVersion(input) !== null;
}

export function compareVersions(a: ParsedVersion, b: ParsedVersion): -1 | 0 | 1 {
  if (a.major !== b.major) return a.major < b.major ? -1 : 1;
  if (a.minor !== b.minor) return a.minor < b.minor ? -1 : 1;
  if (a.patch !== b.patch) return a.patch < b.patch ? -1 : 1;
  return 0;
}

function formatVersion(v: ParsedVersion): string {
  return `${v.major}.${v.minor}.${v.patch}`;
}

export type RangeOp = '>=' | '>' | '<=' | '<';

export interface RangeBound {
  readonly op: RangeOp;
  readonly version: ParsedVersion;
}

export interface ParsedRange {
  /** 이 계약의 range는 항상 하한을 갖는다(§2 compatibility.json 관측 — 상한 없는 행은 있어도 하한 없는 행은 없다) */
  readonly lower: RangeBound;
  readonly upper?: RangeBound;
}

const COMPARATOR_RE = /^(>=|<=|>|<)(\S+)$/;

/** `">=1.8.24 <2.0.0"` / `">=2.1.237"` 형태만 파싱한다. 그 밖의 형태는 null(=거부 대상). */
export function parseRange(range: string): ParsedRange | null {
  const tokens = range.trim().split(/\s+/).filter(Boolean);
  if (tokens.length === 0 || tokens.length > 2) return null;

  let lower: RangeBound | undefined;
  let upper: RangeBound | undefined;
  for (const token of tokens) {
    const m = COMPARATOR_RE.exec(token);
    if (!m || m[1] === undefined || m[2] === undefined) return null;
    const op = m[1] as RangeOp;
    const version = parseVersion(m[2]);
    if (!version) return null;
    if (op === '>=' || op === '>') {
      if (lower) return null; // 하한 중복 지정은 이 계약 형태 밖이다
      lower = { op, version };
    } else {
      if (upper) return null; // 상한 중복 지정도 마찬가지
      upper = { op, version };
    }
  }
  if (!lower) return null;
  return upper ? { lower, upper } : { lower };
}

export function formatRange(range: ParsedRange): string {
  const lowerPart = `${range.lower.op}${formatVersion(range.lower.version)}`;
  if (!range.upper) return lowerPart;
  return `${lowerPart} ${range.upper.op}${formatVersion(range.upper.version)}`;
}

export interface NarrowRangeResult {
  /** 항상 "번들 ∩ 정책"이다 — widened가 true여도 effective는 이미 번들 값으로 clamp돼 있다 */
  readonly effective: string;
  /** 정책이 하한을 내리거나 상한을 올리려는 시도를 했는지 — true면 그 방향의 값은 폐기됐다 */
  readonly widened: boolean;
}

/**
 * PR-9(정책은 좁힐 수만 있다) 정본 구현. policy-contract.md §2.3 표와 정확히 대응한다:
 *   하한을 올림 / 상한을 내림 → 채택
 *   하한을 내림 / 상한을 올림 → 폐기(번들 값 유지) + widened=true
 * 정책 range 자체가 파싱 불가하면 null을 반환한다 — 호출자가 "필드 폐기"로 처리한다
 * (이 경우는 PR-9 위반이 아니라 단순 형식 오류이므로 widened로 취급하지 않는다).
 */
export function narrowRange(bundledRaw: string, policyRaw: string): NarrowRangeResult | null {
  const bundled = parseRange(bundledRaw);
  if (!bundled) {
    // 코드 상수 자체가 깨진 것은 이 함수의 책임 밖이다 — 호출자(loader)가 애초에
    // compat/compatibility.json을 신뢰 가능한 정본으로 취급하므로 여기 도달하면 버그다.
    throw new Error(`bundled range invalid: ${bundledRaw}`);
  }
  const policy = parseRange(policyRaw);
  if (!policy) return null;

  let widened = false;

  let effectiveLower = bundled.lower;
  const lowerCmp = compareVersions(policy.lower.version, bundled.lower.version);
  if (lowerCmp > 0) {
    effectiveLower = policy.lower; // 하한을 올림 = 채택
  } else if (lowerCmp < 0) {
    widened = true; // 하한을 내리려는 시도 = 폐기(번들 유지)
  }

  let effectiveUpper = bundled.upper;
  if (policy.upper) {
    if (!bundled.upper) {
      effectiveUpper = policy.upper; // 번들에 상한이 없으면 정책이 상한을 추가하는 것은 항상 좁히기
    } else {
      const upperCmp = compareVersions(policy.upper.version, bundled.upper.version);
      if (upperCmp < 0) {
        effectiveUpper = policy.upper; // 상한을 내림 = 채택
      } else if (upperCmp > 0) {
        widened = true; // 상한을 올리려는 시도 = 폐기(번들 유지)
      }
    }
  }

  const effective: ParsedRange = effectiveUpper
    ? { lower: effectiveLower, upper: effectiveUpper }
    : { lower: effectiveLower };
  return { effective: formatRange(effective), widened };
}
