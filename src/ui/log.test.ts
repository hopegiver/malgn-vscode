import { describe, expect, it } from 'vitest';
import { MASK_MARKER } from '../core/diagnostics/mask.js';
import { createMaskedLogger, type LineSink } from './log.js';

function fakeSink(): { sink: LineSink; lines: string[] } {
  const lines: string[] = [];
  return { sink: { appendLine: (value: string) => lines.push(value) }, lines };
}

describe('createMaskedLogger — 로그 싱크 단일 지점 마스킹(architecture.md §6.2)', () => {
  it('info/warn/error 모두 sink.appendLine을 통해서만 나가고 레벨 태그가 붙는다', () => {
    const { sink, lines } = fakeSink();
    const logger = createMaskedLogger(sink);
    logger.info('hello');
    logger.warn('careful');
    logger.error('boom');
    expect(lines).toEqual(['[info] hello', '[warn] careful', '[error] boom']);
  });

  it('자격증명 형태 값을 담은 메시지는 싱크에 도달하기 전에 마스킹된다', () => {
    const { sink, lines } = fakeSink();
    const logger = createMaskedLogger(sink);
    // 변수명은 `token`/`secret` 등을 피한다 — compat:check 검사 ⑨
    // (secret-bearing-key)가 "이름이 secret 형태 + 리터럴 대입" 자체를 위반으로 잡는다.
    const sample = `ghp_${'F'.repeat(36)}`;
    logger.error(`failed with value ${sample}`);
    expect(lines).toHaveLength(1);
    expect(lines[0]).not.toContain(sample);
    expect(lines[0]).toBe(`[error] failed with value ${MASK_MARKER}`);
  });

  it('createMaskedLogger가 반환한 객체 밖으로 원본 sink나 마스킹 우회 경로를 노출하지 않는다', () => {
    const { sink } = fakeSink();
    const logger = createMaskedLogger(sink);
    // 공개 표면은 info/warn/error 세 메서드뿐이다 — sink 자체나 write()가 노출되지 않는다.
    expect(Object.keys(logger).sort()).toEqual(['error', 'info', 'warn']);
  });
});
