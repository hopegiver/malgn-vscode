// 마스킹 필터 — architecture.md §6.2 원문 그대로의 **단일 지점**.
//
// "마스킹 필터는 로그 싱크에 단일 지점으로 건다(`gh[pousr]_[A-Za-z0-9]{20,}`,
// `Bearer\s+\S+`, `Basic\s+\S+`, `[A-Za-z0-9_-]{32,}`). SecretStorage에서 읽은 값은
// read 시점에 `Secret<string>`으로 감싸 `toString()`이 `***`를 반환하게 하고, 진단
// 리포트도 같은 필터를 통과한 뒤에만 클립보드로 나간다."
//
// 이 파일이 그 "단일 지점"이다 — 네 정규식은 여기 한 곳에만 존재한다(다른 파일이
// 같은 정규식을 복붙하면 두 구현이 갈릴 위험이 생긴다 — 이 프로젝트가 검사 ⑨
// `redactMatch`에서 이미 겪은 실패 패턴). 로그 싱크(`ui/log.ts`)는 자유 텍스트용
// `maskSensitive`를, 저널(`core/journal/store.ts`)·진단 리포트 클립보드 렌더
// (`core/diagnostics/report.ts`)는 구조화 값용 `maskDeepValues`를 가져다 쓴다 — 둘 다
// 아래 같은 네 정규식 위에서 동작하는 이 파일의 함수이며, 각자 정규식을 새로 정의하지
// 않는다. Secret 브랜드 타입(`secrets/vault.ts`)은 `MASK_MARKER`만 공유한다.
// `maskSinglePoint.test.ts`가 이 성질을 grep으로 고정한다(다른 파일에 같은 정규식
// 조각이 나타나거나 이 두 함수 밖에서 별도 마스킹 로직이 생기면 실패).

/** SecretStorage 브랜드 타입(`secrets/vault.ts`)과 로그 마스킹이 공유하는 유일한
 * 치환 문자열. 두 메커니즘이 서로 다른 마커를 쓰면 "이 값이 마스킹된 것인지"를
 * 사람이 두 가지 형태로 배워야 한다 — 하나로 통일한다. */
export const MASK_MARKER = '***';

// 순서가 결과에 영향을 준다: `Bearer`/`Basic` 프리픽스를 먼저 치환해 맥락을 보존한
// 뒤(`Bearer ***`), 마지막으로 남은 고엔트로피 토큰을 일괄 치환한다. gh 토큰 패턴은
// 접두사가 있는 특수 사례라 먼저 처리한다 — 순서를 바꿔도 최종 결과(마스킹 여부)는
// 같지만 원문 손상 없이 맥락(`Bearer `)을 남기는 쪽이 로그 가독성에 낫다.
const GH_TOKEN_RE = /gh[pousr]_[A-Za-z0-9]{20,}/g;
const BEARER_RE = /Bearer\s+\S+/g;
const BASIC_RE = /Basic\s+\S+/g;
const HIGH_ENTROPY_RE = /[A-Za-z0-9_-]{32,}/g;

/**
 * 자유 텍스트(로그 라인) 안에 섞여 있을 수 있는 자격증명 형태 값을 전부 `***`로
 * 치환한다. 이 함수를 통과하지 않은 텍스트는 로그 싱크로 나가지 않는다는 것이
 * `ui/log.ts`의 불변량이다.
 *
 * 구조적으로 비밀이 아닌 값(정책의 `gh[pousr]_...` 형태가 아닌 토큰, 32자 미만
 * 문자열)은 손대지 않는다 — 과도한 마스킹은 진단 리포트를 무용하게 만든다
 * (devops-review.md Q4가 지적한 반대 방향의 실패: "정보가 없어 진단이 불가능").
 *
 * **구조화된 데이터(저널 엔트리·진단 리포트 객체)에는 이 함수를 직접 쓰지 않는다.**
 * `HIGH_ENTROPY_RE`는 32자 이상의 영숫자/`_`/`-` 나열이면 값이든 **키 이름**이든
 * 구분하지 않고 치환한다 — `JSON.stringify(entry)`로 만든 문자열을 통째로 이 함수에
 * 넣으면 `HOMEBREW_NO_INSTALLED_DEPENDENTS_CHECK`(38자, 실제 §3 installEnv 키) 같은
 * **비밀이 아닌 필드 이름까지 지워진다** — 마스킹이 과잉 적용되어 R-19가 요구하는
 * "env 집합의 사후 대조"를 오히려 불가능하게 만드는 실제 결함이다(이 슬라이스의
 * `store.test.ts`가 처음 이 형태로 재현해 발견했다, 반환문에 명시). 구조화된 값은
 * 아래 `maskDeepValues()`로 **리프 문자열 값만** 마스킹한 뒤 직렬화한다.
 */
export function maskSensitive(text: string): string {
  return text
    .replace(GH_TOKEN_RE, MASK_MARKER)
    .replace(BEARER_RE, `Bearer ${MASK_MARKER}`)
    .replace(BASIC_RE, `Basic ${MASK_MARKER}`)
    .replace(HIGH_ENTROPY_RE, MASK_MARKER);
}

/**
 * 구조화된 값(저널 엔트리·진단 리포트 객체)을 재귀 순회해 **리프 문자열 값에만**
 * `maskSensitive`를 적용한 새 값을 반환한다. 객체의 **키 이름**·배열 인덱스·불리언·
 * 숫자·null은 절대 건드리지 않는다 — `JSON.stringify` 이후의 평문 전체를
 * `maskSensitive`에 넣는 것과 달리, 키 이름이 우연히 32자 이상이어도 지워지지 않는다.
 * 저널 저장(`core/journal/store.ts`)·진단 리포트 클립보드 렌더(`core/diagnostics/
 * report.ts`)는 `JSON.stringify` 직전에 이 함수를 거친다 — 이것도 "같은 필터"의
 * 구조화 버전일 뿐 별도 정규식을 갖지 않는다(위 네 정규식을 그대로 재사용).
 */
export function maskDeepValues<T>(value: T): T {
  if (typeof value === 'string') {
    return maskSensitive(value) as unknown as T;
  }
  if (Array.isArray(value)) {
    return value.map((item) => maskDeepValues(item)) as unknown as T;
  }
  if (value !== null && typeof value === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
      result[key] = maskDeepValues(child);
    }
    return result as unknown as T;
  }
  return value; // number · boolean · null · undefined — 그대로
}
