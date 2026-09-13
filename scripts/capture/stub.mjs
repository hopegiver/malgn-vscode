// window.__TAURI_INTERNALS__ 스텁 빌더. capture.mjs와 달리 이 앱은 Tauri
// invoke가 없으면 아무 화면도 데이터를 못 채우므로, 플레인 브라우저에 이
// 객체를 통째로 주입해야 한다(과제 지시사항 "이미 확인된 사실" 참고).
import { READ_FIXTURES, ACTION_DEFAULTS, ERROR_MESSAGES, USER, LOGIN_ERROR_MESSAGE } from './fixtures.mjs';

const NORMAL_DELAY_MS = 80; // 실제 IPC 왕복을 흉내낸 최소 지연
export const LOADING_DELAY_MS = 20000; // "로딩 상태" 캡처 동안 응답을 보류하는 지연(로그인 직후 일괄 로드되는 커맨드들과 공유 기준시각이라 넉넉히 잡는다)
const NEVER_MS = 999_999; // 로그인 자체의 pending 상태 캡처용 — 캡처 시간 내 자연 해소되지 않음

function actionEntries() {
  const out = {};
  for (const [cmd, value] of Object.entries(ACTION_DEFAULTS)) out[cmd] = { value, delayMs: NORMAL_DELAY_MS };
  return out;
}

// kind: 'normal' | 'empty' | 'error' | 'app-loading' | 'login-normal' | 'login-error' | 'login-loading'
export function buildScenarioConfig(kind) {
  switch (kind) {
    case 'normal':
    case 'empty': {
      const commands = actionEntries();
      for (const [cmd, value] of Object.entries(READ_FIXTURES[kind])) commands[cmd] = { value, delayMs: NORMAL_DELAY_MS };
      return { commands, defaultDelayMs: NORMAL_DELAY_MS };
    }
    case 'error': {
      const commands = actionEntries();
      for (const cmd of Object.keys(READ_FIXTURES.normal)) {
        commands[cmd] = { error: ERROR_MESSAGES[cmd] ?? `${cmd} 호출이 실패했습니다 (캡처 하네스 에러 시나리오).`, delayMs: NORMAL_DELAY_MS };
      }
      // 로그인 자체는 성공해야 이후 화면들의 "데이터 조회 실패" 상태를 볼 수 있다.
      commands.google_oauth_login = { value: USER, delayMs: NORMAL_DELAY_MS };
      return { commands, defaultDelayMs: NORMAL_DELAY_MS };
    }
    case 'app-loading': {
      // 로그인만 즉시 통과시키고, 이후 모든 데이터 커맨드는 캡처 구간 내내 응답하지
      // 않아 각 화면이 "불러오는 중…" 상태에 머물게 한다.
      const commands = actionEntries();
      for (const [cmd, value] of Object.entries(READ_FIXTURES.normal)) commands[cmd] = { value, delayMs: LOADING_DELAY_MS };
      commands.google_oauth_login = { value: USER, delayMs: NORMAL_DELAY_MS };
      return { commands, defaultDelayMs: LOADING_DELAY_MS };
    }
    case 'login-normal': {
      const commands = actionEntries();
      for (const [cmd, value] of Object.entries(READ_FIXTURES.normal)) commands[cmd] = { value, delayMs: NORMAL_DELAY_MS };
      return { commands, defaultDelayMs: NORMAL_DELAY_MS };
    }
    case 'login-error': {
      const commands = actionEntries();
      commands.google_oauth_login = { error: LOGIN_ERROR_MESSAGE, delayMs: NORMAL_DELAY_MS };
      return { commands, defaultDelayMs: NORMAL_DELAY_MS };
    }
    case 'login-loading': {
      const commands = actionEntries();
      commands.google_oauth_login = { value: USER, delayMs: NEVER_MS };
      return { commands, defaultDelayMs: NEVER_MS };
    }
    default:
      throw new Error(`알 수 없는 시나리오: ${kind}`);
  }
}

// Playwright의 page.addInitScript(fn, arg)에 그대로 전달되는 함수 — 브라우저
// 컨텍스트에서 함수 소스 그대로 재실행되므로 바깥 클로저(import 등)를 참조하면
// 안 된다. 인자로 받은 config만 사용한다.
export function installTauriStub(config) {
  window.__TAURI_STUB_CALLS__ = [];
  window.__TAURI_INTERNALS__ = {
    transformCallback() {
      // 이벤트 채널 id만 흉내낸다 — 실제 이벤트는 쏘지 않는다(이 앱은 구독 실패를
      // 이미 "Tauri IPC 브리지 없는 플레인 브라우저" 정상 폴백 경로로 처리한다).
      return 0;
    },
    convertFileSrc(filePath) {
      return filePath;
    },
    invoke(cmd, _args) {
      window.__TAURI_STUB_CALLS__.push({ cmd, at: Date.now() });
      const entry = config.commands[cmd];
      const delayMs = entry && typeof entry.delayMs === 'number' ? entry.delayMs : config.defaultDelayMs;
      return new Promise((resolve, reject) => {
        setTimeout(() => {
          if (!entry) {
            reject(new Error(`[capture-stub] '${cmd}' 명령이 이 시나리오에 정의되어 있지 않습니다.`));
            return;
          }
          if (entry.error) reject(new Error(entry.error));
          else resolve(entry.value === undefined ? null : entry.value);
        }, delayMs);
      });
    },
  };
}
