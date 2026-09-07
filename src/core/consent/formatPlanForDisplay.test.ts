import { describe, expect, it } from 'vitest';
import { formatPlanForDisplay } from './formatPlanForDisplay.js';
import type { Plan } from '../../providers/types.js';

describe('formatPlanForDisplay', () => {
  it('변경 하나를 사람이 읽을 수 있는 문구로 만든다(레벨·대상·되돌릴 수 있는지·이유 포함)', () => {
    const plan: Plan = {
      providerId: 'agent',
      changes: [
        { id: 'x', target: 'malgn-agent@malgnsoft-plugins', kind: 'install', level: 'L2', after: '{"a":1}', reversible: true, rationale: '플러그인 설치' },
      ],
      diffHash: 'x',
    };
    const text = formatPlanForDisplay(plan);
    expect(text).toContain('L2');
    expect(text).toContain('malgn-agent@malgnsoft-plugins');
    expect(text).toContain('되돌릴 수 있음');
    expect(text).toContain('플러그인 설치');
  });

  it('before가 있으면 "before → after" 형태로 보여준다', () => {
    const plan: Plan = {
      providerId: 'agent',
      changes: [{ id: 'x', target: 't', kind: 'update', level: 'L2', before: '1.8.20', after: '{}', reversible: true, rationale: 'r' }],
      diffHash: 'x',
    };
    expect(formatPlanForDisplay(plan)).toContain('1.8.20 → {}');
  });

  it('되돌릴 수 없는 변경은 명시한다', () => {
    const plan: Plan = {
      providerId: 'agent',
      changes: [{ id: 'x', target: 't', kind: 'install', level: 'L2', after: '{}', reversible: false, rationale: 'r' }],
      diffHash: 'x',
    };
    expect(formatPlanForDisplay(plan)).toContain('되돌릴 수 없음');
  });

  it('변경이 여러 개면 각각 표시한다', () => {
    const plan: Plan = {
      providerId: 'agent',
      changes: [
        { id: 'a', target: 't1', kind: 'install', level: 'L2', after: '{}', reversible: true, rationale: 'r1' },
        { id: 'b', target: 't2', kind: 'exec', level: 'L2', after: '{}', reversible: true, rationale: 'r2' },
      ],
      diffHash: 'x',
    };
    const text = formatPlanForDisplay(plan);
    expect(text).toContain('t1');
    expect(text).toContain('t2');
  });
});
