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
 */
export function installListenerProbe() {
  const counts = {};
  const orig = { add: window.addEventListener.bind(window), remove: window.removeEventListener.bind(window) };
  window.__listenerCounts = counts;
  window.addEventListener = function (type, ...rest) {
    counts[type] = (counts[type] ?? 0) + 1;
    return orig.add(type, ...rest);
  };
  window.removeEventListener = function (type, ...rest) {
    if (counts[type]) counts[type] -= 1;
    return orig.remove(type, ...rest);
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

  function resolveFixture(cmd, args) {
    // plugin:event|listen/unlisten은 세션 내내 여러 화면(세션 채팅 스트리밍,
    // 자율업무 런타임 갱신 등)이 반복 구독하는 프레임워크 커맨드라 픽스처
    // 선언 없이 항상 지원한다 — 매 호출마다 고유 id가 필요하다.
    if (cmd === 'plugin:event|listen') {
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
