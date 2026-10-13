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
