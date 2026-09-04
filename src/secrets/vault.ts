// SecretStorage 래퍼 + 마스킹 — architecture.md §1.1(`secrets/vault`) · §6.2 원문:
// "SecretStorage에서 읽은 값은 read 시점에 `Secret<string>`으로 감싸 `toString()`이
// `***`를 반환하게 한다."
//
// 이 파일은 `vscode.SecretStorage`를 직접 감싸지 않는다 — 실제 SecretStorage 접근은
// W7+(각 provider가 자격증명을 읽는 시점)의 책임이고, 이 슬라이스가 제공하는 것은
// "읽은 값을 감싸는 브랜드 타입" 자체다. 호출자는 `SecretStorage.get()`이 반환한
// 문자열을 얻는 즉시 `wrapSecret()`으로 감싸고, 이후에는 원본 문자열을 별도로 들고
// 있지 않는다 — 그래야 "읽은 시점에 감싼다"가 실제로 강제된다.
//
// [핵심 성질] `#value`는 ECMAScript 클래스 **진짜 private 필드**다(클로저 흉내가
// 아니라 언어 수준 캡슐화) — `JSON.stringify`·`Object.keys`·구조분해 등 어떤 구조적
// 접근으로도 저장소 밖에서 값을 꺼낼 수 없고, 오직 `reveal()`을 호출한 코드만 값을
// 얻는다. `toString()`·`toJSON()` 둘 다 `MASK_MARKER`를 반환해 문자열 보간
// (`` `${secret}` ``)과 `JSON.stringify(secret)`(진단 리포트 직렬화 경로) 양쪽에서
// 마스킹이 자동으로 걸린다 — 호출자가 "이 값은 비밀이니 로그에 넣지 마세요"를 기억할
// 필요가 없다.

import { MASK_MARKER } from '../core/diagnostics/mask.js';

export class Secret<T extends string = string> {
  readonly #value: T;

  constructor(value: T) {
    this.#value = value;
  }

  /** 문자열 보간·`console.log`·`appendLine` 등 암묵적 문자열화 경로가 전부 이걸 탄다. */
  toString(): string {
    return MASK_MARKER;
  }

  /** `JSON.stringify({...})`가 이 메서드를 우선 호출한다 — 진단 리포트 직렬화
   * (`core/diagnostics/report.ts`)가 `Secret`을 필드로 들고 있어도 원문이 새지 않는다. */
  toJSON(): string {
    return MASK_MARKER;
  }

  /** 원본 값이 실제로 필요한 유일한 통로(예: OS API에 그대로 전달, exec의 env 주입).
   * 호출부를 늘리지 않는 것이 §6.2 불변량의 실행 조건이다 — 새 호출부를 추가할 때는
   * "이 값이 로그·저널·진단 리포트로 흘러들어갈 경로가 없는가"를 먼저 확인한다. */
  reveal(): T {
    return this.#value;
  }
}

/** `SecretStorage.get()` 등에서 얻은 원문을 읽은 시점 즉시 감싼다. */
export function wrapSecret<T extends string = string>(value: T): Secret<T> {
  return new Secret(value);
}
