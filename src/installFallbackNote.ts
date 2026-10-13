// 플러그인 설치 결과 메시지에서 "구버전 claude CLI 폴백 노트"를 가려내는 순수 함수.
//
// 백엔드(src-tauri/src/plugins/mod.rs `with_fallback_note`)는 설치가 `-y`/`-s user`를 빼고
// 재시도해 끝났을 때 — 성공·실패 모두 — result.message 끝에 아래 형식의 블록을 덧붙인다:
//
//   "{원래 메시지}\n\n(구버전 claude CLI 호환: `-y`, `-s user` 옵션을 거부해 해당 옵션을
//    빼고 다시 시도했습니다. `claude update`로 업데이트를 권장합니다.{범위 경고})"
//
// 범위 경고(" 설치 범위가 user가 아닐 수 있어 …")는 `-s user`를 뺀 경우에만 붙는다.
// 폴백이 없었으면(`-y`/`-s user` 모두 유지) 메시지는 그대로라 이 함수는 null을 돌려준다.
// 이 파일은 DOM/Tauri/다른 모듈을 import하지 않는다(Node 경계값 검사가 그대로 로드한다 —
// scripts/ui-harness/lib/fallbackNoteBoundary.mjs).

/** 백엔드가 폴백 노트 앞에 붙이는 구분자 + 여는 문구(mod.rs with_fallback_note의 format!과 동일). */
export const FALLBACK_NOTE_MARK = '\n\n(구버전 claude CLI 호환:';

/**
 * 메시지 끝의 폴백 노트 블록을 바깥 괄호째 돌려준다(없으면 null).
 * 노트는 항상 메시지 맨 끝에 붙으므로 마지막 표식 위치에서 자르고, 닫는 `)`로 끝나는지도
 * 확인한다 — 다른 CLI 출력이 우연히 비슷한 문구를 담아도 형식이 어긋나면 노트로 보지 않는다.
 */
export function extractFallbackNote(message: string | null | undefined): string | null {
  if (!message) return null;
  const idx = message.lastIndexOf(FALLBACK_NOTE_MARK);
  if (idx < 0) return null;
  const note = message.slice(idx + 2).trim();
  return note.endsWith(')') ? note : null;
}

/**
 * 성공 토스트용 — 폴백 노트가 있으면 기본 문구 뒤에 이어 붙이고, 없으면 기본 문구를
 * 그대로 돌려준다(폴백 없는 일반 성공의 표시는 변하지 않는다). 실패 결과에는 쓰지 않는다
 * (실패 토스트는 이미 message 전체를 보여준다).
 *
 * `message`는 반드시 백엔드 원문을 넘긴다 — 길이 제한(sanitizeBackendText)을 먼저 거치면 긴 원래
 * 메시지 때문에 맨 끝의 노트가 잘릴 수 있다. 마스킹·길이 정리는 노트만 따로 `clean`으로 한다.
 */
export function successToastWithFallbackNote(
  baseText: string,
  message: string | null | undefined,
  clean: (text: string) => string = (text) => text
): string {
  const note = extractFallbackNote(message);
  return note === null ? baseText : `${baseText} ${clean(note)}`;
}
