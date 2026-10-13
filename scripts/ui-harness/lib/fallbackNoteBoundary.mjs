// 설치 폴백 노트 판별(src/installFallbackNote.ts) 경계값 검사 + 백엔드 문자열 대조.
//
// runFallbackNoteChecks(mod, rustSource)
//   mod        — installFallbackNote 모듈(Vite가 서빙하는 변환 결과 또는 node --experimental-strip-types 로드)
//   rustSource — src-tauri/src/plugins/mod.rs 원문. with_fallback_note의 format! 문자열이 이 검사의
//                샘플과 FALLBACK_NOTE_MARK에 그대로 들어 있는지 대조해, 백엔드 문구가 바뀌면 여기서 잡는다.
// node 단독 실행 예:
//   node --experimental-strip-types -e "
//     const m = await import('./src/installFallbackNote.ts');
//     const { runFallbackNoteChecks } = await import('./scripts/ui-harness/lib/fallbackNoteBoundary.mjs');
//     const src = (await import('node:fs')).readFileSync('src-tauri/src/plugins/mod.rs','utf8');
//     const r = runFallbackNoteChecks(m, src); console.log(r.rows.join('\n'));
//     process.exit(r.failures.length ? 1 : 0);"
export function runFallbackNoteChecks(mod, rustSource) {
  const { extractFallbackNote, successToastWithFallbackNote, FALLBACK_NOTE_MARK } = mod;
  const rows = [];
  const failures = [];
  let total = 0;
  const check = (name, cond, detail) => {
    total += 1;
    const line = `${cond ? 'PASS' : 'FAIL'} ${name}${cond ? '' : ` — ${detail}`}`;
    rows.push(line);
    if (!cond) failures.push(line);
  };

  // with_fallback_note와 같은 조립(mod.rs의 format!/scope_hint 상수를 그대로 옮긴 사본).
  const SCOPE_HINT = ' 설치 범위가 user가 아닐 수 있어 설치 목록에 보이지 않으면 claude를 업데이트한 뒤 다시 설치해주세요.';
  const backend = (message, { yes, scopeUser }) => {
    const dropped = [];
    if (!yes) dropped.push('`-y`');
    if (!scopeUser) dropped.push('`-s user`');
    if (dropped.length === 0) return message;
    return `${message}\n\n(구버전 claude CLI 호환: ${dropped.join(', ')} 옵션을 거부해 해당 옵션을 빼고 다시 시도했습니다. \`claude update\`로 업데이트를 권장합니다.${scopeUser ? '' : SCOPE_HINT})`;
  };

  // 0) 백엔드 소스 대조 — 사본이 실제 Rust 문자열과 어긋나면 즉시 실패.
  check('mod.rs에 폴백 노트 format! 문구 존재', rustSource.includes('\\n\\n(구버전 claude CLI 호환: {} 옵션을 거부해 해당 옵션을 빼고 다시 시도했습니다. `claude update`로 업데이트를 권장합니다.{})'), 'with_fallback_note 문구가 바뀜 — src/installFallbackNote.ts FALLBACK_NOTE_MARK와 이 사본을 함께 갱신');
  check('mod.rs에 범위 경고 문구 존재', rustSource.includes(`"${SCOPE_HINT}"`), 'scope_hint 문구가 바뀜');
  check('FALLBACK_NOTE_MARK가 Rust 구분자+여는 문구와 일치', FALLBACK_NOTE_MARK === '\n\n(구버전 claude CLI 호환:' && rustSource.includes('\\n\\n(구버전 claude CLI 호환:'), `MARK=${JSON.stringify(FALLBACK_NOTE_MARK)}`);

  const cases = [
    ['-y만 뺌', { yes: false, scopeUser: true }, false],
    ['-s user만 뺌(범위 경고)', { yes: true, scopeUser: false }, true],
    ['둘 다 뺌(범위 경고)', { yes: false, scopeUser: false }, true],
  ];
  for (const [label, flags, hasScope] of cases) {
    const msg = backend('Successfully installed plugin: malgn-agent@malgnsoft-plugins', flags);
    const note = extractFallbackNote(msg);
    check(`${label}: 노트 추출`, note !== null && note.startsWith('(구버전 claude CLI 호환:') && note.endsWith(')'), JSON.stringify(note));
    check(`${label}: 범위 경고 ${hasScope ? '포함' : '미포함'}`, note !== null && note.includes('설치 범위가 user가 아닐 수 있어') === hasScope, JSON.stringify(note));
    check(`${label}: 원래 메시지는 노트에 안 섞임`, note !== null && !note.includes('Successfully installed'), JSON.stringify(note));
    const toast = successToastWithFallbackNote('설치 완료', msg);
    check(`${label}: 토스트에 노트 포함`, note !== null && toast === `설치 완료 ${note}`, toast);
  }

  // 폴백 없는 일반 성공/실패/빈 값 — 표시 불변.
  const plain = backend('Successfully installed plugin: x', { yes: true, scopeUser: true });
  check('폴백 없음: 메시지 불변', plain === 'Successfully installed plugin: x', plain);
  check('폴백 없음: 노트 null', extractFallbackNote(plain) === null, '');
  check('폴백 없음: 토스트 기본 문구 그대로', successToastWithFallbackNote('설치 완료', plain) === '설치 완료', '');
  check('빈 문자열: null', extractFallbackNote('') === null, '');
  check('null/undefined: null', extractFallbackNote(null) === null && extractFallbackNote(undefined) === null, '');
  check('빈 메시지 + 노트(원래 메시지 없음)도 추출', extractFallbackNote(backend('', { yes: false, scopeUser: true })) !== null, '');

  // 오탐 방지.
  check('표식이 본문 중간에만 있고 닫는 괄호로 안 끝나면 null', extractFallbackNote('x\n\n(구버전 claude CLI 호환: 잘림') === null, '');
  check('줄바꿈 구분자 없는 유사 문구는 null', extractFallbackNote('(구버전 claude CLI 호환: a)') === null, '');
  check('다른 괄호 문구는 null', extractFallbackNote('done\n\n(참고: 재시작 필요)') === null, '');

  // 노트 앞의 긴 본문(길이 제한이 노트를 잘라낼 크기)에서도 노트 전체가 온전히 추출된다.
  const long = backend('A'.repeat(5000), { yes: false, scopeUser: false });
  const longNote = extractFallbackNote(long);
  check('긴 본문 뒤의 노트도 온전히 추출', longNote !== null && longNote.endsWith('다시 설치해주세요.)') && !longNote.includes('AAAA'), String(longNote && longNote.length));

  // CRLF/후행 공백.
  const crlfNote = extractFallbackNote(backend('ok', { yes: false, scopeUser: true }) + '\n');
  check('후행 개행이 있어도 추출', crlfNote !== null && crlfNote.endsWith(')'), JSON.stringify(crlfNote));

  return { total, failures, rows };
}
