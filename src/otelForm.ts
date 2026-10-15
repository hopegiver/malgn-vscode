// 설정 > OTel 폼의 "무엇을 저장하는가" 규칙 — DOM·Tauri 없이 값만 다루는 순수 함수(scripts/tests/otelForm.test.mjs).
//
// 왜 필요한가: settings.json은 폼 밖에서도 바뀐다. 허브 수집키 자동 연동이 발급·회전 201의 서버 권장값 6개를 같은 파일에
// 쓴다. 폼이 (1) 그 변경을 다시 읽지 않거나 (2) 저장 때 입력칸 전부를 되돌려 보내면, 서버가 바꾼 값이 무관한 필드를 고쳐
// 저장하는 것만으로 조용히 옛 값으로 돌아간다. 그래서 폼은 "사용자가 고친 필드"와 "파일에 아직 없어 기본값을 보여 주는 필드"만
// 저장하고, 파일에 이미 있는 고치지 않은 필드는 보내지 않는다(파일이 이긴다).

/** 폼이 보는 설정의 일부 — `OtelSettings`의 부분집합(테스트가 전체 객체를 만들지 않아도 된다). */
export interface FormSettings {
  readonly managedKeys: readonly string[];
  readonly values: Readonly<Record<string, string>>;
  readonly defaults: Readonly<Record<string, string>>;
}

/**
 * 입력칸 초기값: 파일 값 → 기본값 → 빈 값. `skip`(직접 입력칸이 없는 키: 직원 식별·수집키)은 뺀다.
 */
export function baselineValues(settings: FormSettings, skip: readonly string[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const key of settings.managedKeys) {
    if (skip.includes(key)) continue;
    out[key] = key in settings.values ? settings.values[key] : key in settings.defaults ? settings.defaults[key] : '';
  }
  return out;
}

/**
 * 파일이 바뀐 뒤(자동 연동의 서버 값 적용, 점검 결과 등) 폼 드래프트를 새 설정에 맞춘다.
 * 사용자가 고친 입력칸(드래프트 ≠ 이전 기준값)은 그대로 두고, 고치지 않은 칸은 새 파일 값으로 갱신한다.
 * `previous`가 없으면(첫 초기화) 새 기준값 그대로다.
 */
export function mergeDraftOnRefresh(
  previous: FormSettings | null,
  next: FormSettings,
  draft: Readonly<Record<string, string>>,
  skip: readonly string[]
): Record<string, string> {
  const nextBase = baselineValues(next, skip);
  if (!previous) return nextBase;
  const prevBase = baselineValues(previous, skip);
  const out: Record<string, string> = {};
  for (const key of Object.keys(nextBase)) {
    const edited = key in draft && key in prevBase && draft[key] !== prevBase[key];
    out[key] = edited ? draft[key] : nextBase[key];
  }
  return out;
}

/**
 * 저장 payload의 `values`. 키를 넣는 조건:
 *   - 읽기 전용 키가 아니고,
 *   - 사용자가 고쳤거나(드래프트 ≠ 폼이 그려진 때의 기준값),
 *   - 파일에 아직 없다(기본값을 보여 주던 칸 — 저장하면 그 값이 기록된다는 안내를 지킨다).
 * 파일에 이미 있는데 사용자가 고치지 않은 칸은 보내지 않는다 — 그 사이 자동 연동이 바꾼 값을 되돌리지 않는다.
 * `fileValues`는 저장 직전에 다시 읽은 파일 값이다.
 */
export function valuesToSave(opts: {
  readonly keys: readonly string[];
  readonly readOnlyKeys: readonly string[];
  readonly draft: Readonly<Record<string, string>>;
  readonly baseline: Readonly<Record<string, string>>;
  readonly fileValues: Readonly<Record<string, string>>;
}): Record<string, string> {
  const out: Record<string, string> = {};
  for (const key of opts.keys) {
    if (opts.readOnlyKeys.includes(key)) continue;
    const value = opts.draft[key];
    if (value === undefined) continue;
    const edited = key in opts.baseline && value !== opts.baseline[key];
    const onFile = key in opts.fileValues;
    if (edited || !onFile) out[key] = value;
  }
  return out;
}

/** 두 설정이 폼에 보이는 내용(파일 값·기본값·수집키 상태)까지 같은가 — 같으면 폼을 다시 그릴 이유가 없다. */
export function sameFormContent(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
