// 렌더러가 IPC로 넘기는 프로젝트 경로 문자열을 검증한다 — "렌더러가 임의 경로를 읽게
// 하지 않는다. 스캔 대상은 `~/workspace` 아래로 한정"(작업 지시 · §2.8과 같은 신뢰
// 경계 정신). `registerProjectIpc.ts`가 STATUS.md를 읽기 **전에** 항상 이 함수를
// 거친다 — `..` 상위 이동, 절대경로 치환, 다단계 하위 경로(`workspaceRoot/a/b`)를
// 전부 거부하고 **`workspaceRoot` 바로 아래 1단계 자식 디렉터리만** 허용한다.

import { isAbsolute, relative, resolve, sep } from 'node:path';

export function isDirectChildOfWorkspaceRoot(workspaceRoot: string, candidatePath: string): boolean {
  if (typeof candidatePath !== 'string' || candidatePath.length === 0) return false;

  const resolvedRoot = resolve(workspaceRoot);
  const resolvedCandidate = resolve(candidatePath);
  const rel = relative(resolvedRoot, resolvedCandidate);

  if (rel === '' || rel.startsWith('..') || isAbsolute(rel)) return false;
  return !rel.includes(sep); // 1단계 자식만 허용 — 하위 트래버설 차단
}
