import { describe, expect, it } from 'vitest';
import { parseStatusMarkdown } from './statusParser.js';

describe('parseStatusMarkdown', () => {
  it('이 저장소 STATUS.md와 같은 4섹션 구조를 파싱한다', () => {
    const content = [
      '# STATUS — 예시',
      '',
      '## 🟢 현재 상태',
      '- 상태 줄 1',
      '- 상태 줄 2',
      '',
      '## ✅ 최근 완료',
      '- 완료 A',
      '',
      '## 🚧 진행 중',
      '- 진행 B',
      '',
      '## ⛔ 막힌 것',
      '- 블로커 C',
    ].join('\n');

    const result = parseStatusMarkdown(content);
    expect(result.parsed).toBe(true);
    expect(result.current).toContain('상태 줄 1');
    expect(result.recentDone).toContain('완료 A');
    expect(result.inProgress).toContain('진행 B');
    expect(result.blocked).toContain('블로커 C');
    expect(result.raw).toBe(content);
  });

  it('헤딩 제목 문구가 달라도 이모지만으로 식별한다(제목은 매칭에 안 쓴다)', () => {
    const content = ['## 🟢 완전히 다른 제목', '자유 텍스트'].join('\n');
    const result = parseStatusMarkdown(content);
    expect(result.parsed).toBe(true);
    expect(result.current).toBe('자유 텍스트');
  });

  it('같은 이모지가 여러 번 나오면 첫 번째만 취한다', () => {
    const content = ['## 🚧 A', '첫번째', '## 🚧 B', '두번째'].join('\n');
    const result = parseStatusMarkdown(content);
    expect(result.inProgress).toBe('첫번째');
  });

  it('인식 가능한 이모지 헤딩이 하나도 없으면 parsed:false이고 raw만 채운다', () => {
    const content = '# 그냥 자유 형식 문서\n아무 구조도 없음';
    const result = parseStatusMarkdown(content);
    expect(result.parsed).toBe(false);
    expect(result.current).toBeNull();
    expect(result.recentDone).toBeNull();
    expect(result.inProgress).toBeNull();
    expect(result.blocked).toBeNull();
    expect(result.raw).toBe(content);
  });

  it('빈 문자열 입력에도 죽지 않는다', () => {
    const result = parseStatusMarkdown('');
    expect(result.parsed).toBe(false);
    expect(result.raw).toBe('');
  });

  it('마지막 섹션은 파일 끝까지를 본문으로 취한다', () => {
    const content = ['## ⛔ 막힌 것', '줄1', '줄2', ''].join('\n');
    const result = parseStatusMarkdown(content);
    expect(result.blocked).toBe('줄1\n줄2');
  });
});
