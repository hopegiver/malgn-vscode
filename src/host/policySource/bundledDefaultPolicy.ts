// 3순위(번들 내장) 정책 — architecture.md §3.7.3 "2순위도 실패 → 3순위(번들 내장)".
// "체크아웃·설치본 셋 다 실패는 구조상 불가"(N-1)라고 적혀 있지만 그 불가능성은
// **이 3순위가 항상 성공하기 때문**이다 — 이 파일이 그 마지막 보루다.
//
// 내용은 `schemaVersion`만 채운 최소 유효 정책이다: `loader.ts`의 다른 모든 필드는
// `undefined`일 때 안전한 기본값(agent는 `blocked:true`, killSwitch는 전부 null/빈
// 배열, install은 `assisted`)으로 떨어진다 — "정책을 전혀 못 읽어도 앱은 죽지 않고
// 최소 안전 상태로 뜬다"는 성질을 코드가 스스로 증명하는 자리다. `compat/
// compatibility.json.requires.manifestSchema.min`을 그대로 쓴다 — 값을 이 파일에
// 하드코딩하면 PR-7 "단일 정본" 위반이 되기 때문이다.

import compatibilityRaw from '../../../compat/compatibility.json';

export function buildBundledDefaultPolicyText(): string {
  return JSON.stringify({ schemaVersion: compatibilityRaw.requires.manifestSchema.min });
}
