// 개발 도구 일괄 설치/전체 업데이트의 "실행 순서"와 "선행 도구 미확인 시 건너뜀"
// 판정 — 부수효과 없는 순수 함수만 둔다(화면·invoke 의존 없음, 하네스가 그대로
// import해 검증한다).
//
// 의존 관계의 정본은 백엔드 DevTool.install_depends_on(src-tauri/src/dev_tools/
// mod.rs)이고, 이 모듈은 check_dev_tools가 내려주는 `installDependsOn`만 입력으로
// 받는다 — 도구 id 목록을 여기 하드코딩하지 않는다.

export interface OrderableTool {
  readonly id: string;
  // 이 도구를 새로 설치하기 전에 설치돼 있어야 하는 도구 id. 누락(구버전
  // 응답·테스트 스텁)은 의존 없음으로 본다.
  readonly installDependsOn?: readonly string[];
}

// 안정적 위상 정렬 — 선행 도구가 항상 먼저 오고, 의존 관계가 없는 도구끼리는
// 입력 순서를 유지한다. 입력에 없는 id를 가리키는 의존(이번 배치 밖의 도구)은
// 순서에는 영향을 주지 않는다(건너뜀 판정은 unmetInstallDependencies 몫).
// 순환은 정본(Rust 테스트)이 막지만, 방어적으로 순환에 걸린 나머지는 입력 순서
// 그대로 뒤에 붙여 반환한다(무한 루프·도구 누락 방지).
export function orderByInstallDependency<T extends OrderableTool>(tools: readonly T[]): T[] {
  const inBatch = new Set(tools.map((t) => t.id));
  const emitted = new Set<string>();
  const remaining = [...tools];
  const ordered: T[] = [];
  while (remaining.length > 0) {
    const idx = remaining.findIndex((t) => (t.installDependsOn ?? []).every((dep) => !inBatch.has(dep) || emitted.has(dep) || dep === t.id));
    if (idx === -1) {
      ordered.push(...remaining);
      break;
    }
    const [next] = remaining.splice(idx, 1);
    ordered.push(next);
    emitted.add(next.id);
  }
  return ordered;
}

// 이 도구의 선행 도구 중 "설치가 확인되지 않은" id 목록. confirmedInstalled는
// (a) 배치 시작 전부터 installed===true였던 도구 + (b) 이번 배치에서 실제로
// updated/alreadyLatest(= 설치 재조회로 확인됨)로 끝난 도구의 id 집합이다 —
// 실패·unknownAfter·timedOut·미리보기 보류(개별 확인 대기)·사용자가 설치 요약을
// 거절해 아예 실행되지 않은 도구는 여기에 들어가지 않는다("확인 안 됨").
export function unmetInstallDependencies(tool: OrderableTool, confirmedInstalled: ReadonlySet<string>): string[] {
  return (tool.installDependsOn ?? []).filter((dep) => !confirmedInstalled.has(dep));
}

// 실행 결과가 "설치됨이 확인됨"인지 — exit code가 아니라 outcome(백엔드가 버전
// 재조회로 판정)만 본다.
export function isInstallConfirmedOutcome(outcome: string): boolean {
  return outcome === 'updated' || outcome === 'alreadyLatest';
}

export function skipReasonForUnmet(unmetNames: readonly string[]): string {
  return `선행 도구(${unmetNames.join(', ')}) 설치가 확인되지 않아 건너뜀`;
}

// ---------------- 선행 도구 설치 확인 뒤 "이어서 설치" (R14 P2-9) ----------------
// 아래는 전부 부수효과 없는 순수 함수다(화면·invoke 의존 없음). 정본 입력은 역시 백엔드가
// 내려주는 목록(installed/actionKind/path/installDependsOn)뿐이다.

export interface ChainTool extends OrderableTool {
  readonly name: string;
  readonly installed: boolean;
  readonly actionKind: string;
  readonly path?: string | null;
}

// 선행 도구 중 "설치가 확인되지 않은" 것을 사람이 읽는 사유로 바꾼다. 두 갈래를 구분한다:
//  - 경로도 없다 → 정말 설치가 확인되지 않음(기존 문구 그대로).
//  - 경로는 있는데 installed=false → 백엔드는 `--version` 응답이 없을 때 이렇게 내려준다
//    (query.rs: installed = version.is_some(), path는 따로 채움). 이때 "설치가 확인되지
//    않았다"는 사실이지만 "없다"는 뜻은 아니다 — 있는 것을 없다고 말하거나(거짓 문장) 있다고
//    믿고 의존 도구를 실행하지 않도록(미확인) 문구로 구분해 말한다.
export function describeUnmetDependencies(unmetIds: readonly string[], items: readonly ChainTool[]): string {
  const byId = new Map(items.map((t) => [t.id, t]));
  const missing: string[] = [];
  const unverifiable: string[] = [];
  for (const id of unmetIds) {
    const tool = byId.get(id);
    const name = tool?.name ?? id;
    if (tool && !tool.installed && tool.path) unverifiable.push(name);
    else missing.push(name);
  }
  const parts: string[] = [];
  if (missing.length > 0) parts.push(skipReasonForUnmet(missing));
  if (unverifiable.length > 0) {
    parts.push(`선행 도구(${unverifiable.join(', ')})는 이 PC에서 파일이 발견됐지만 버전을 확인하지 못해(응답 지연 등) 설치 여부를 판정할 수 없어 건너뜀`);
  }
  return parts.join(' / ');
}

// 이 도구를 선행 도구로 삼는데 아직 설치되지 않은 도구들(화면 안내 문구용).
export function pendingDependentsOf<T extends ChainTool>(toolId: string, items: readonly T[]): T[] {
  return items.filter((t) => !t.installed && (t.installDependsOn ?? []).includes(toolId));
}

export interface InstallChainPlan<T extends ChainTool> {
  // 이번 재조회에서 새로 설치된 것으로 확인된 선행 도구(이어 설치 대상의 원인이 되는 것만).
  readonly causes: T[];
  // 지금 `run`으로 설치할 수 있고 선행 도구가 모두 설치 확인된 의존 도구(의존 순서로 정렬).
  readonly candidates: T[];
  // 새 선행 도구에 의존하지만 아직 `run`이 아니거나 다른 선행 도구가 미설치라 이어 설치할 수 없는 도구.
  readonly stuck: T[];
}

// 재조회 전(prev)/후(next) 목록을 비교해 "이어서 설치" 제안 대상을 계산한다.
//  - 새로 설치된 선행 도구 = prev에서는 installed=false로 존재했고 next에서 installed=true.
//    (prev가 비었거나 그 도구가 prev에 없으면 비교 기준이 없으므로 제안하지 않는다 — 앱
//    시작 직후 첫 조회가 제안을 띄우지 않게 하는 장치.)
//  - handled(이번 흐름에서 이미 동의·실행·보류 처리한 도구)는 다시 제안하지 않는다 — 같은
//    도구가 두 번 제안·실행되지 않게 하는 상한.
//  - 후보는 `actionKind==='run'` + 선행 도구가 전부 next에서 installed=true인 미설치 도구만.
// 제안할 것도, 막힌 것도 없으면 null.
export function planInstallChain<T extends ChainTool>(prev: readonly ChainTool[], next: readonly T[], handled: ReadonlySet<string>): InstallChainPlan<T> | null {
  if (prev.length === 0) return null;
  const prevById = new Map(prev.map((t) => [t.id, t]));
  const nowInstalled = new Set(next.filter((t) => t.installed).map((t) => t.id));
  const newly = new Set(next.filter((t) => t.installed && prevById.get(t.id) && !prevById.get(t.id)!.installed).map((t) => t.id));
  if (newly.size === 0) return null;
  const dependents = next.filter((t) => !t.installed && !handled.has(t.id) && (t.installDependsOn ?? []).some((dep) => newly.has(dep)));
  if (dependents.length === 0) return null;
  const candidates = orderByInstallDependency(dependents.filter((t) => t.actionKind === 'run' && (t.installDependsOn ?? []).every((dep) => nowInstalled.has(dep))));
  const stuck = dependents.filter((t) => !candidates.includes(t));
  const causeIds = new Set(dependents.flatMap((t) => (t.installDependsOn ?? []).filter((dep) => newly.has(dep))));
  return { causes: next.filter((t) => causeIds.has(t.id)), candidates, stuck };
}
