import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { isDirectChildOfWorkspaceRoot } from './pathGuard.js';

const ROOT = '/home/runner/workspace';

describe('isDirectChildOfWorkspaceRoot', () => {
  it('루트 바로 아래 1단계 자식은 허용한다', () => {
    expect(isDirectChildOfWorkspaceRoot(ROOT, join(ROOT, 'malgn-vscode'))).toBe(true);
  });

  it('2단계 이상 하위 경로는 거부한다(트래버설 차단)', () => {
    expect(isDirectChildOfWorkspaceRoot(ROOT, join(ROOT, 'malgn-vscode', 'src'))).toBe(false);
  });

  it('`..`로 루트 밖을 가리키면 거부한다', () => {
    expect(isDirectChildOfWorkspaceRoot(ROOT, join(ROOT, '..', 'etc', 'passwd'))).toBe(false);
    expect(isDirectChildOfWorkspaceRoot(ROOT, '/home/runner/workspace/../../etc')).toBe(false);
  });

  it('완전히 무관한 절대경로는 거부한다', () => {
    expect(isDirectChildOfWorkspaceRoot(ROOT, '/etc/passwd')).toBe(false);
  });

  it('루트 자기 자신은 거부한다(1단계 자식이 아니다)', () => {
    expect(isDirectChildOfWorkspaceRoot(ROOT, ROOT)).toBe(false);
  });

  it('문자열이 아니거나 빈 값이면 거부한다', () => {
    expect(isDirectChildOfWorkspaceRoot(ROOT, '')).toBe(false);
    // @ts-expect-error 런타임 방어 확인 — IPC 경계 너머에서 온 값은 타입이 보장되지 않는다
    expect(isDirectChildOfWorkspaceRoot(ROOT, null)).toBe(false);
  });
});
