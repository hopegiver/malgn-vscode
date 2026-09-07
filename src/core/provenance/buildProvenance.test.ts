// NT-R22 단위 테스트 — docs/release-gates.md §7.6.6.
import { describe, expect, it } from 'vitest';
import { InvalidProvenanceInputError, buildProvenanceRecord, sha256Of, verifyArtifactMatchesRecord } from './buildProvenance.js';

const VALID_INPUT = {
  siteProfile: 'example' as const,
  commitSha: 'abc1234',
  artifactPath: 'dist/extension.cjs',
  artifactSha256: 'a'.repeat(64),
  buildTimestamp: '2026-09-07T00:00:00.000Z',
};

describe('buildProvenanceRecord', () => {
  it('유효한 입력이면 레코드를 만들고 signature는 항상 null이다', () => {
    const record = buildProvenanceRecord(VALID_INPUT);
    expect(record.recordVersion).toBe(1);
    expect(record.siteProfile).toBe('example');
    expect(record.commitSha).toBe('abc1234');
    expect(record.artifactSha256).toBe(`sha256:${'a'.repeat(64)}`);
    expect(record.signature).toBeNull();
  });

  it('이미 sha256: 접두가 붙은 해시도 받아들이고 정규화한다', () => {
    const record = buildProvenanceRecord({ ...VALID_INPUT, artifactSha256: `sha256:${'b'.repeat(64)}` });
    expect(record.artifactSha256).toBe(`sha256:${'b'.repeat(64)}`);
  });

  it('siteProfile이 site/example이 아니면 던진다', () => {
    // @ts-expect-error 잘못된 값 주입 테스트
    expect(() => buildProvenanceRecord({ ...VALID_INPUT, siteProfile: 'bogus' })).toThrow(InvalidProvenanceInputError);
  });

  it('commitSha 형식이 아니면 던진다', () => {
    expect(() => buildProvenanceRecord({ ...VALID_INPUT, commitSha: 'not-a-sha!!' })).toThrow(InvalidProvenanceInputError);
  });

  it('artifactPath가 비어 있으면 던진다', () => {
    expect(() => buildProvenanceRecord({ ...VALID_INPUT, artifactPath: '' })).toThrow(InvalidProvenanceInputError);
  });

  it('artifactSha256이 유효한 hex가 아니면 던진다', () => {
    expect(() => buildProvenanceRecord({ ...VALID_INPUT, artifactSha256: 'not-hex' })).toThrow(InvalidProvenanceInputError);
  });

  it('buildTimestamp가 유효한 날짜가 아니면 던진다', () => {
    expect(() => buildProvenanceRecord({ ...VALID_INPUT, buildTimestamp: 'not-a-date' })).toThrow(InvalidProvenanceInputError);
  });
});

describe('sha256Of / verifyArtifactMatchesRecord', () => {
  it('같은 버퍼는 같은 해시를 낸다', () => {
    const buf = Buffer.from('hello world', 'utf8');
    expect(sha256Of(buf)).toBe(sha256Of(Buffer.from('hello world', 'utf8')));
  });

  it('레코드의 해시와 실제 아티팩트가 같으면 true', () => {
    const buf = Buffer.from('artifact content', 'utf8');
    const record = buildProvenanceRecord({ ...VALID_INPUT, artifactSha256: sha256Of(buf) });
    expect(verifyArtifactMatchesRecord(record, buf)).toBe(true);
  });

  it('아티팩트가 재빌드로 바뀌면(내용 다름) false — 레코드가 낡았음을 잡는다', () => {
    const buf = Buffer.from('artifact content v1', 'utf8');
    const record = buildProvenanceRecord({ ...VALID_INPUT, artifactSha256: sha256Of(buf) });
    const rebuilt = Buffer.from('artifact content v2', 'utf8');
    expect(verifyArtifactMatchesRecord(record, rebuilt)).toBe(false);
  });
});
