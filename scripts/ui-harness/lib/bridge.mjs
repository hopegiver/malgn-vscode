// Tauri IPC 브리지 스텁 + 리스너 누수 탐지 프로브.
//
// 이 파일이 내보내는 함수들은 Playwright의 page.addInitScript()로 문자열
// 직렬화되어 실제 앱 JS보다 먼저 실행된다 — 그래서 클로저로 바깥 변수를
// 참조할 수 없고, 인자(arg)로 넘긴 값만 쓸 수 있다(Playwright 제약).
//
// 검증 범위: TS/DOM/CSS 층만. window.__TAURI_INTERNALS__.invoke를 스텁으로
// 갈아끼우므로 Rust 커맨드 자체의 실제 동작(파일 읽기/쓰기, 프로세스 실행 등)은
// 이 하네스로 검증되지 않는다.

/**
 * addInitScript로 주입되는 리스너 누수 탐지 프로브. window.addEventListener/
 * removeEventListener를 감싸 타입별 활성 리스너 개수를 window.__listenerCounts에
 * 기록한다. 화면 전환 시 ESC 리스너 등이 실제로 detach되는지(leave*View 정리
 * 로직이 도는지) 테스트에서 읽어 검증하는 데 쓴다.
 *
 * n6 수정(리뷰 2026-09-24 2차) — 이전에는 type별 정수 카운터를 add에서 +1,
 * remove에서 -1 했다. 이러면 "등록된 적 없는 핸들러를 remove"해도 카운트가
 * 줄어(예: -1), 실제로는 누수가 있어도 다른 정상적인 add/remove 쌍이 그 -1을
 * 상쇄해 카운트가 기준선으로 돌아온 것처럼 보일 수 있었다(M1 사이드바 계정
 * 메뉴의 0ms 지연 등록 구조에서 실측: "열기→0ms 안에 닫기"가 등록 없이
 * removeEventListener를 부른다). 실제로 등록된 함수 참조 집합(Set)을
 * type별로 추적해, 등록되지 않은 핸들러의 remove는 집합 크기를 바꾸지 않게
 * 한다 — __listenerCounts[type]은 이 집합의 size다.
 */
export function installListenerProbe() {
  const registry = new Map(); // type -> Set<핸들러 함수 참조>
  const counts = {};
  window.__listenerCounts = counts;
  const orig = { add: window.addEventListener.bind(window), remove: window.removeEventListener.bind(window) };
  const syncCount = (type) => {
    counts[type] = registry.get(type)?.size ?? 0;
  };
  window.addEventListener = function (type, fn, ...rest) {
    let set = registry.get(type);
    if (!set) {
      set = new Set();
      registry.set(type, set);
    }
    set.add(fn);
    syncCount(type);
    return orig.add(type, fn, ...rest);
  };
  window.removeEventListener = function (type, fn, ...rest) {
    const set = registry.get(type);
    if (set && set.delete(fn)) syncCount(type);
    return orig.remove(type, fn, ...rest);
  };
}

/**
 * fixtures: { [command: string]: value | { __throw } | { __delay, value } | { __delay, __throw } }
 * 함수 값은 지원하지 않는다(Playwright 인자 직렬화가 함수를 못 넘긴다) — 동적
 * 응답이 필요하면 run.mjs에서 page.exposeFunction으로 별도 브리지를 만들거나,
 * 시나리오별로 고정 fixtures를 다시 주입(re-addInitScript는 다음 navigation부터
 * 적용되므로 page.evaluate로 즉석 갱신)한다.
 */
export function installTauriBridge(fixtures) {
  let eventIdSeq = 1;
  window.__invokeLog = [];
  window.__unstubbedCommands = [];
  // 이벤트 이름 → transformCallback이 부여한 콜백 id 목록. session-chat-done
  // 같은 백엔드 스트리밍 이벤트를 시나리오가 직접 발화(emitTauriEvent)할 수
  // 있게, 이 이름으로 구독 중인 콜백만 정확히 골라 부르기 위한 인덱스다
  // (이름 필터 없이 아무 콜백이나 부르면 delta 리스너가 done payload를 받는 등
  // 엉뚱한 이벤트로 잘못 호출된다).
  window.__eventCallbackIdsByName = {};

  function resolveFixture(cmd, args) {
    // plugin:event|listen/unlisten은 세션 내내 여러 화면(세션 채팅 스트리밍,
    // 자율업무 런타임 갱신 등)이 반복 구독하는 프레임워크 커맨드라 픽스처
    // 선언 없이 항상 지원한다 — 매 호출마다 고유 id가 필요하다.
    if (cmd === 'plugin:event|listen') {
      const list = (window.__eventCallbackIdsByName[args.event] ??= []);
      list.push(args.handler);
      return Promise.resolve(eventIdSeq++);
    }
    if (cmd === 'plugin:event|unlisten') {
      return Promise.resolve(undefined);
    }
    if (!Object.prototype.hasOwnProperty.call(fixtures, cmd)) {
      window.__unstubbedCommands.push({ cmd, args });
      return Promise.reject(new Error('UNSTUBBED_COMMAND:' + cmd));
    }
    const spec = fixtures[cmd];
    if (spec && typeof spec === 'object' && ('__throw' in spec || '__delay' in spec)) {
      const run = () => {
        if ('__throw' in spec) throw new Error(spec.__throw);
        return spec.value;
      };
      if (typeof spec.__delay === 'number') {
        return new Promise((resolve, reject) => {
          setTimeout(() => {
            try {
              resolve(run());
            } catch (e) {
              reject(e);
            }
          }, spec.__delay);
        });
      }
      return new Promise((resolve, reject) => {
        try {
          resolve(run());
        } catch (e) {
          reject(e);
        }
      });
    }
    return Promise.resolve(spec);
  }

  // @tauri-apps/api/event의 unlisten()은 invoke('plugin:event|unlisten', ...)
  // 전에 window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(event, id)를
  // 동기 호출한다(__TAURI_INTERNALS__와 별개 전역 객체) — 실측: 이 스텁 없이
  // 세션 채팅 화면을 이탈하면 "Cannot read properties of undefined (reading
  // 'unregisterListener')" pageerror가 실제로 발생한다.
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };

  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      window.__invokeLog.push({ cmd, args, at: Date.now() });
      return resolveFixture(cmd, args);
    },
    transformCallback: (cb) => {
      const id = Math.floor(Math.random() * 1e9);
      window['_cb' + id] = cb;
      return id;
    },
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
  };
}

/**
 * 시나리오가 백엔드 스트리밍 이벤트(session-chat-delta/session-chat-done 등)를
 * 흉내 낼 때 쓴다 — installTauriBridge가 채운 __eventCallbackIdsByName에서 그
 * 이벤트 이름을 구독 중인 콜백만 정확히 호출한다. `page.evaluate(emitTauriEvent,
 * { name, payload })` 형태로 호출한다(addInitScript로 주입되는 다른 함수들과
 * 같은 Playwright 직렬화 제약 — 인자로 받은 값만 참조할 수 있다).
 */
export function emitTauriEvent({ name, payload }) {
  const ids = window.__eventCallbackIdsByName?.[name] ?? [];
  for (const id of ids) {
    const cb = window['_cb' + id];
    if (typeof cb === 'function') cb({ event: name, id: 0, payload });
  }
}
