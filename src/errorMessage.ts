// 사용자에게 보여줄 에러 문구를 만드는 공용 헬퍼.
//
// 왜 필요한가: Tauri invoke 실패는 Error 인스턴스가 아니라 문자열(Rust가 돌려준
// Err(String))이나 평범한 객체로 reject된다. 그래서 `err instanceof Error ?
// err.message : '일반 문구'` 형태로 쓰면 백엔드가 준 구체적 원인이 통째로 버려진다.
// 이 헬퍼는 reject 값이 문자열/Error/객체 어느 쪽이든 원문을 꺼내 쓰고, 원문이
// 비었을 때만 호출부가 준 일반 문구로 폴백한다.
//
// 화면에 찍기 전 반드시 거치는 두 가지 방어:
//   1) 민감값 마스킹 — 백엔드/CLI stderr 원문에는 토큰·키가 섞일 수 있고 화면 캡처가
//      사내에 공유될 수 있다. 알려진 토큰 형태와 `key=value` 형태의 비밀값을 가린다.
//      로컬 경로(홈 디렉터리 등)는 원인 파악에 필요하므로 일부러 가리지 않는다.
//   2) 길이 제한 — 수 KB짜리 stderr가 토스트·배너를 깨지 않도록 앞부분과 끝부분만
//      남긴다(CLI 에러는 보통 마지막 줄에 실제 원인이 있어 끝을 버리지 않는다).
//
// ⚠ 마스킹은 "best effort" 방어선이다 — 모든 비밀 형태를 보증하지 않는다. 새 토큰
// 형식이 생기면 SECRET_PATTERNS에 추가한다.

const DEFAULT_FALLBACK = '알 수 없는 오류가 발생했습니다.';
const MASK = '***';

// 길이 제한 — 머리 + 꼬리를 남기고 가운데를 생략한다.
const HEAD_CHARS = 320;
const TAIL_CHARS = 160;
const MAX_CHARS = HEAD_CHARS + TAIL_CHARS + 40;
const OMIT_MARK = '\n… (중략) …\n';

// 마스킹 전 거친 절단 — 이 길이를 넘는 원문은 머리·꼬리만 남기고 마스킹한다.
const PRE_CUT_CHARS = 16000;
const PRE_HEAD = 12000;
const PRE_TAIL = 4000;

// 비밀값 키 이름(접두 허용: ANTHROPIC_API_KEY, x-api-key, access_token …).
const SECRET_KEY =
  '([A-Za-z0-9_.-]*(?:token|secret|password|passwd|pwd|api[_-]?key|apikey|access[_-]?key|private[_-]?key|credential|authorization|cookie))';

// 값 쪽 구분자: 공백·따옴표·콤마·세미콜론·& · 닫는 괄호류에서 끝난다.
const VALUE = '(?:(?:Bearer|Basic|Token)\\s+)?[^\\s"\',;&}\\])]{6,}';

// key=value 규칙의 치환 — 키 이름은 남기고 값만 가리되 비밀이 아닌 두 형태는 보존한다:
//   - 순수 숫자 값이 붙은 `*token` 키(`"max_token": 1234567` 같은 개수·한도 수치)
//   - `PWD=/Users/...`처럼 경로가 붙은 `pwd` 키(작업 디렉터리 변수이지 비밀번호가 아니다)
// 나머지(`password=12345678` 포함)는 그대로 가린다.
function maskKeyValue(match: string, prefix: string, key: string, value: string): string {
  if (/token$/i.test(key) && /^\d+$/.test(value)) return match;
  if (/^pwd$/i.test(key) && /^(?:[\\/~]|[A-Za-z]:[\\/])/.test(value)) return match;
  return `${prefix}${MASK}`;
}

type Replacement = string | ((match: string, ...groups: string[]) => string);

// [정규식, 치환] — 위에서부터 순서대로 적용한다.
const SECRET_PATTERNS: ReadonlyArray<readonly [RegExp, Replacement]> = [
  // PEM 개인키 블록(끝 표식이 잘려도 끝까지 가린다)
  [/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)/g, '[개인키 숨김]'],
  // URL userinfo — https://user:pass@host
  [/(:\/\/[^\s/:@]+:)[^\s@/]+@/g, `$1${MASK}@`],
  // Authorization 헤더류 — Bearer/Basic 뒤 값
  [/\b(Bearer|Basic)\s+[A-Za-z0-9._~+/=-]{8,}/g, `$1 ${MASK}`],
  // key=value / "key": "value" / KEY: value — 키 이름은 남기고 값만 가린다
  [new RegExp(`(\\b${SECRET_KEY}["']?\\s*[:=]\\s*["']?)(${VALUE})`, 'gi'), maskKeyValue],
  // CLI 플래그 — --token abc / --api-key abc
  [/(--[\w-]*(?:token|secret|password|api-?key)\s+)[^\s-][^\s]*/gi, `$1${MASK}`],
  // 접두가 알려진 토큰들
  [/\bsk-ant-[A-Za-z0-9_-]{6,}/g, `sk-ant-${MASK}`], // Anthropic
  [/\bsk-[A-Za-z0-9_-]{16,}/g, `sk-${MASK}`], // OpenAI 류
  [/\bmhd_[A-Za-z0-9_-]{6,}/g, `mhd_${MASK}`], // 허브 기기 토큰
  [/\bgh[pousr]_[A-Za-z0-9]{16,}/g, `gh_${MASK}`], // GitHub
  [/\bgithub_pat_[A-Za-z0-9_]{16,}/g, `github_pat_${MASK}`],
  [/\bxox[abprs]-[A-Za-z0-9-]{10,}/g, `xox-${MASK}`], // Slack
  [/\bAIza[0-9A-Za-z_-]{30,}/g, `AIza${MASK}`], // Google API key
  [/\bya29\.[0-9A-Za-z_-]{20,}/g, `ya29.${MASK}`], // Google OAuth access token
  [/\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/g, `AKIA${MASK}`], // AWS access key id
  [/\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]*/g, `[JWT ${MASK}]`], // JWT
];

// 접두 없는 긴 불투명 토큰(Cloudflare API 토큰 등) — 40자 이상이며 대문자·소문자·숫자를
// 모두 포함할 때만. 경로 조각·파일명(앞이 / \ . 영숫자 -)과 소문자 hex 해시(커밋 SHA,
// 세션 UUID)는 대문자가 없어 걸리지 않는다. 접두가 없어 형태만으로 추정하는 규칙이라
// 오탐이 있을 수 있다 — 그래서 URL 쿼리값(OAuth code_challenge·state 등)을 복사해야
// 하는 로그 블록에서는 이 규칙만 끌 수 있게 따로 둔다.
const OPAQUE_TOKEN_PATTERN =
  /(?<![\w/\\.-])(?=[A-Za-z0-9_-]*[a-z])(?=[A-Za-z0-9_-]*[A-Z])(?=[A-Za-z0-9_-]*\d)[A-Za-z0-9_-]{40,}(?![\w/\\.-])/g;

// ANSI 색상·커서 제어 시퀀스 — CLI stderr가 그대로 들어오면 화면에 깨진 글자로 보인다.
// eslint-disable-next-line no-control-regex
const ANSI_PATTERN = /\u001b\[[0-9;?]*[ -/]*[@-~]/g;

function applyPatterns(text: string, includeOpaque: boolean): string {
  let out = text;
  for (const [pattern, replacement] of SECRET_PATTERNS) out = out.replace(pattern, replacement as string);
  return includeOpaque ? out.replace(OPAQUE_TOKEN_PATTERN, MASK) : out;
}

/** 문자열에서 토큰·키류 비밀값을 가린다(경로는 건드리지 않는다). */
export function maskSecrets(text: string): string {
  return applyPatterns(text, true);
}

// 로그 블록(여러 줄 원문 출력)용 — 렌더마다 호출되므로 ①입력 상한(머리·꼬리만 남김)
// ②같은 입력의 결과 캐시로 비용을 묶는다. 원문 로그를 복사해 쓰는 화면이라 완전한
// 요약은 하지 않는다.
const LOG_CACHE_MAX = 8;
const logMaskCache = new Map<string, string>();
// eslint-disable-next-line no-control-regex
const URL_TOKEN = /(https?:\/\/[^\s"'<>\u001b\u0007]+)/g;

/**
 * 로그 원문 블록을 화면용으로 가린다: 길이 상한 + 비밀값 마스킹 + 결과 캐시.
 * `keepUrls`가 true면 `https://…` URL 안에서는 접두 없는 불투명 토큰 규칙을 적용하지 않아
 * OAuth 로그인 URL(code_challenge·state 등 공개 쿼리값)을 원형 그대로 복사할 수 있다.
 * sk-ant-/Bearer/ghp_ 등 접두가 알려진 비밀과 `token=` 류 key=value는 URL 안에서도 가린다.
 */
export function maskLogText(text: string | null | undefined, keepUrls = false): string {
  if (!text) return '';
  const key = `${keepUrls ? 'u' : 'p'}${text}`;
  const hit = logMaskCache.get(key);
  if (hit !== undefined) return hit;
  const bounded =
    text.length > PRE_CUT_CHARS ? `${text.slice(0, PRE_HEAD)}\n… (중간 생략) …\n${text.slice(-PRE_TAIL)}` : text;
  const masked = keepUrls
    ? bounded
        .split(URL_TOKEN)
        .map((part, i) => applyPatterns(part, i % 2 === 0)) // split 캡처 그룹: 홀수 인덱스가 URL
        .join('')
    : applyPatterns(bounded, true);
  if (logMaskCache.size >= LOG_CACHE_MAX) logMaskCache.delete(logMaskCache.keys().next().value as string);
  logMaskCache.set(key, masked);
  return masked;
}

function clampLength(text: string): string {
  if (text.length <= MAX_CHARS) return text;
  return `${text.slice(0, HEAD_CHARS).trimEnd()}${OMIT_MARK}${text.slice(-TAIL_CHARS).trimStart()}`;
}

// reject 값 → 원문 문자열. Tauri의 Err(String)은 문자열, 직렬화된 Rust 에러 enum은
// 객체({ message } 또는 { kind, message })로 오는 경우가 있다.
function extractRaw(err: unknown): string {
  if (err === null || err === undefined) return '';
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (typeof err === 'number' || typeof err === 'boolean') return String(err);
  if (typeof err === 'object') {
    const o = err as Record<string, unknown>;
    for (const key of ['message', 'error', 'msg', 'reason', 'detail']) {
      const v = o[key];
      if (typeof v === 'string' && v.trim() !== '') return v;
      if (v && typeof v === 'object') {
        const nested = extractRaw(v);
        if (nested !== '') return nested;
      }
    }
    try {
      const json = JSON.stringify(err);
      return json === undefined || json === '{}' || json === '[]' ? '' : json;
    } catch {
      return '';
    }
  }
  return '';
}

/**
 * 백엔드가 준 텍스트(이미 문자열인 ok:false 결과의 message 등)를 화면용으로 정리한다:
 * ANSI 제거 → 비밀값 마스킹 → 길이 제한. 정리 결과가 비면 빈 문자열.
 */
export function sanitizeBackendText(text: string | null | undefined): string {
  if (!text) return '';
  // 마스킹 정규식 비용을 상한 안에 두려고 수십 KB 원문은 먼저 거칠게 줄인다(머리·꼬리만).
  const bounded = text.length > PRE_CUT_CHARS ? `${text.slice(0, PRE_HEAD)}\n${text.slice(-PRE_TAIL)}` : text;
  return clampLength(cleanForDisplay(bounded));
}

// 플러그인 커맨드 실패 메시지 끝에 백엔드가 붙이는 진단 블록의 시작 표식.
const DIAGNOSTIC_MARK = '\n\n[진단 정보';

function cleanForDisplay(text: string): string {
  return maskSecrets(text.replace(ANSI_PATTERN, '')).replace(/\r\n?/g, '\n').replace(/\n{3,}/g, '\n\n').trim();
}

/**
 * 결과 노트·배너처럼 화면에 남아 있는 곳용 정리. `sanitizeBackendText`와 같되 백엔드가 붙인
 * `[진단 정보 …]` 블록(주입 PATH 전문, ProgramFiles 등 — 사용자가 그대로 회신해야 하는 값)은
 * 길이 제한 없이 전부 보여준다(마스킹은 유지). 본문(블록 앞)은 기존처럼 길이를 제한한다.
 * 토스트처럼 잠깐 뜨는 곳은 계속 `sanitizeBackendText`를 쓴다.
 */
export function sanitizeBackendTextPersistent(text: string | null | undefined): string {
  if (!text) return '';
  const idx = text.indexOf(DIAGNOSTIC_MARK);
  if (idx < 0) return sanitizeBackendText(text);
  const head = sanitizeBackendText(text.slice(0, idx));
  const diag = text.slice(idx + 2);
  // 진단 블록은 백엔드가 만든 짧은 고정 형식(PATH 포함 수 KB)이다. 병적으로 큰 입력만 상한을 둔다.
  const boundedDiag = diag.length > PRE_CUT_CHARS ? `${diag.slice(0, PRE_CUT_CHARS)}\n… (이하 생략) …` : diag;
  const cleanDiag = cleanForDisplay(boundedDiag);
  return head !== '' ? `${head}\n\n${cleanDiag}` : cleanDiag;
}

/**
 * catch 블록에서 받은 reject 값(문자열/Error/객체/그 외)을 사용자에게 보여줄 문구로
 * 바꾼다. 원문이 있으면 정리해 그대로 쓰고, 비어 있을 때만 `fallback`(기본: 일반
 * 문구)을 쓴다.
 */
export function errorMessage(err: unknown, fallback: string = DEFAULT_FALLBACK): string {
  const text = sanitizeBackendText(extractRaw(err));
  return text !== '' ? text : fallback;
}

/** `errorMessage`와 같되 결과 노트처럼 지속 표시되는 곳용 — 진단 블록을 자르지 않는다. */
export function errorMessagePersistent(err: unknown, fallback: string = DEFAULT_FALLBACK): string {
  const text = sanitizeBackendTextPersistent(extractRaw(err));
  return text !== '' ? text : fallback;
}
