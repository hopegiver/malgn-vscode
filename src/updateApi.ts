// 자동 업데이트 오케스트레이션 — VSCode처럼 평소엔 조용하다가 업데이트가 감지되면
// 사이드바 하단에 배지/버튼만 조용히 노출한다(강제 팝업/모달 없음). 실제 적용은
// ①버튼 클릭 또는 ②사용자가 버튼을 누르지 않고 그냥 껐다 켠 경우 다음 실행에서의
// 자동 재적용, 두 경로뿐이다.
//
// ⚠️ 검증된 플러그인 API 동작(과제 지시 근거 — 크레이트 소스 대조 완료, 추측 아님):
//   - download()는 받은 바이트를 프로세스 메모리에만 담는다 — 디스크에 남지 않고
//     앱이 종료되면 완전히 소실된다. "다운로드해 둔 업데이트를 재시작 후 이어서
//     설치"하는 경로는 플러그인에 없다 — 다음 실행에서 check()+download()를 처음부터
//     다시 해야 한다.
//   - Windows: install()은 설치 프로그램을 별도 프로세스로 띄운 뒤 즉시 현재
//     프로세스를 std::process::exit(0)로 종료한다(restart_after_install 기본값
//     true라 설치 프로그램이 새 버전을 자동 재실행한다) — 이 함수 호출 뒤의
//     relaunch()는 이 플랫폼에서 정상적으로 도달하지 않는다.
//   - macOS/Linux: install()은 프로세스를 죽이지 않고 디스크상 .app만 교체한다 —
//     새 버전을 실제로 띄우려면 relaunch()가 반드시 필요하다.
//   - check()는 현재 상태(릴리스 미게시)에서 resolve(null)이 아니라 reject한다.
//     릴리스가 게시된 뒤에도 네트워크 문제로 언제든 reject할 수 있다 — 모든 호출을
//     try/catch로 감싸고 실패는 조용히 무시한다(사용자가 버튼을 직접 눌러 실패한
//     경우에만 최소 안내를 허용한다 — 정상 사용 중 백그라운드 실패로 방해하지 않는다).
//
// 설계 긴장 해소(요구사항 4 "버튼을 안 누르고 껐다 켜도 결과가 같아야") —
// localStorage의 pendingVersion 플래그로 세션을 넘나드는 "이전에 이미 보여줬던
// 버전" 여부를 판단한다:
//   1) 업데이트를 감지·다운로드해 버튼을 "보여주는 시점"에 즉시 그 버전을
//      pendingVersion에 기록한다(beforeunload를 기다리지 않는다 — 강제 종료·OS
//      종료로 그 이벤트가 아예 안 뜨는 경우에도 안전하게 남아야 하기 때문이다).
//   2) 다음 실행의 부팅 체크가 같은 버전을 다시 발견하면, pendingVersion과 일치할
//      때만 자동 적용 "후보"로 본다 — "지난 실행에서 이미 한 번 보여줬는데
//      사용자가 그냥 껐다"로 간주하기 때문이다. 처음 보는 버전이면 평소처럼
//      버튼만 띄우고 자동 설치하지 않는다(요구사항 2 준수).
//   3) 설치 직전에 플래그를 지운다. 실패하면(오프라인 중 재시도 등) 플래그와 버튼
//      상태를 원복해 다음 기회에 다시 시도한다 — 최악의 경우에도 "버튼만 다시
//      보이는" 안전한 상태로 돌아간다.
//
// ⚠️ 리뷰 M2 대응(2) — download()에는 타임아웃도 진행 표시도 없어 "부팅 직후
// 수 초 안에 끝난다"는 전제가 코드로 보장되지 않는다(Windows install()은
// exit(0)으로 즉시 프로세스를 죽인다). 그래서 자동 적용 후보([2])라도 곧장
// 설치하지 않고 아래 게이트를 모두 통과해야만 실제로 적용한다(isEligibleForAutoApply,
// isAnySavingInProgress, showAutoApplyCountdown) — 부팅 후 그레이스 타임 초과나
// 사용자 입력 감지, 저장 중, 또는 카운트다운 중 "나중에" 클릭 중 하나라도
// 걸리면 버튼 경로로 강등해 평소와 동일하게 사용자가 직접 트리거하게 한다.
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { state, notifyChange } from './state';
import { showToast, el } from './dom';

const PENDING_VERSION_KEY = 'malgn-agent.update.pendingVersion';
const LAST_CHECKED_AT_KEY = 'malgn-agent.update.lastCheckedAt';
// "켜져 있는 동안은 24시간에 1번만 추가 체크" — 부팅 시 1회 체크는 이 임계값과
// 무관하게 항상 실행하고, 그 이후 앱이 계속 떠 있는 동안의 "추가" 재확인만 이
// 임계값으로 제한한다.
const RECHECK_THRESHOLD_MS = 24 * 60 * 60 * 1000;
const PERIODIC_POLL_INTERVAL_MS = 60 * 60 * 1000; // 임계값 경과 여부를 1시간마다 확인
const CHECK_TIMEOUT_MS = 10_000; // 오프라인/느린 네트워크에서 무한 대기 방지

// 리뷰 M2 대응 — 캐리오버 자동설치는 더 이상 "버튼 없이 곧장" 진행하지 않는다.
// 부팅 후 이 시간 안에, 그리고 사용자 입력이 전혀 없었던 상태에서만 자동 적용
// 후보로 본다(그 기준을 넘겼거나 이미 뭔가 조작 중이면 버튼으로 강등). download()에
// 걸리는 시간이 예측 불가능하다는 리뷰 지적을 "부팅 직후일 것"이라는 가정 대신
// 실측(경과시간+입력이벤트)으로 검증하기 위함이다.
const BOOT_GRACE_MS = 60_000;
// 적용 직전 비모달 카운트다운 — 이 몇 초 동안 "나중에"를 누르면 자동 적용을
// 포기하고 버튼 경로로 강등한다(요구사항: 강제 팝업/모달 금지 — role="status"의
// 배너일 뿐 다른 조작을 막지 않는다).
const AUTO_APPLY_COUNTDOWN_SEC = 5;
// 모듈이 로드되는 시점(≈ 앱 부팅 직후, main.ts가 이 모듈을 정적 import하는 시점)을
// 기준선으로 삼는다. 이후 사용자 입력이 한 번이라도 감지되면 "이미 작업 중"으로
// 간주해 이번 실행에서는 캐리오버 자동 적용을 시도하지 않는다.
const bootAt = Date.now();
let userInteractedSinceBoot = false;
window.addEventListener('pointerdown', () => { userInteractedSinceBoot = true; }, { capture: true, passive: true });
window.addEventListener('keydown', () => { userInteractedSinceBoot = true; }, { capture: true, passive: true });

let isChecking = false;
let periodicTimer: number | null = null;
// 이번 세션에서 감지·다운로드해 둔 Update — 버튼 클릭 시 재사용한다. 직렬화
// 불가능한 리소스 핸들이라 state(AppState)에는 존재 여부/버전만 반영한다.
let currentUpdate: Update | null = null;

// 요구사항 3 "저장 중이면 연기" — state 전역의 saving류 플래그를 모아 확인한다.
// 새 저장 플래그가 추가되면 이 목록도 함께 늘려야 한다.
function isAnySavingInProgress(): boolean {
  return state.otel.saving || state.malgnAgentConfig.saving || state.appLinks.saving;
}

// 요구사항 2 "사용자가 이미 작업 중이면 자동 적용하지 않는다" — 부팅 후 경과
// 시간과 그 사이 입력 이벤트 유무만으로 판정한다(과설계 금지, 최소 구현).
function isEligibleForAutoApply(): boolean {
  if (userInteractedSinceBoot) return false;
  if (Date.now() - bootAt > BOOT_GRACE_MS) return false;
  return true;
}

// 요구사항 1 "적용 직전 비모달 카운트다운 고지 + 취소 수단" — 배경을 막지 않는
// 하단 배너(role="status")로 몇 초 뒤 자동 적용됨을 알리고, "나중에"를 누르면
// false(취소)를 즉시 resolve한다. 카운트다운이 끝까지 흐르면 true(진행)를 resolve.
function showAutoApplyCountdown(version: string): Promise<boolean> {
  return new Promise((resolve) => {
    let secondsLeft = AUTO_APPLY_COUNTDOWN_SEC;
    let settled = false;
    let timer: number;

    const textNode = document.createTextNode('');
    const updateText = (): void => {
      textNode.textContent = `업데이트(v${version})를 ${secondsLeft}초 후 적용하고 재시작합니다.`;
    };
    updateText();

    const finish = (proceed: boolean): void => {
      if (settled) return;
      settled = true;
      window.clearInterval(timer);
      banner.remove();
      resolve(proceed);
    };

    const laterBtn = el('button', { className: 'btn update-countdown-later-btn', onClick: () => finish(false) }, ['나중에']);
    laterBtn.type = 'button';

    const banner = el('div', { className: 'update-countdown-banner' }, [el('span', {}, [textNode]), laterBtn]);
    banner.setAttribute('role', 'status');
    banner.setAttribute('aria-live', 'polite');
    document.body.appendChild(banner);

    timer = window.setInterval(() => {
      secondsLeft -= 1;
      if (secondsLeft <= 0) {
        finish(true);
        return;
      }
      updateText();
    }, 1000);
  });
}

function readPendingVersion(): string | null {
  try {
    return window.localStorage.getItem(PENDING_VERSION_KEY);
  } catch {
    return null; // localStorage 접근 불가 환경 — 캐리오버 자동설치 없이 항상 "처음 보는 버전"으로 취급
  }
}

function writePendingVersion(version: string | null): void {
  try {
    if (version) window.localStorage.setItem(PENDING_VERSION_KEY, version);
    else window.localStorage.removeItem(PENDING_VERSION_KEY);
  } catch {
    /* 조용히 무시 — 이 세션에서는 버튼 클릭 경로만 정상 동작한다 */
  }
}

function readLastCheckedAt(): number {
  try {
    const raw = window.localStorage.getItem(LAST_CHECKED_AT_KEY);
    const n = raw ? Number(raw) : 0;
    return Number.isFinite(n) ? n : 0;
  } catch {
    return 0;
  }
}

function writeLastCheckedAt(ts: number): void {
  try {
    window.localStorage.setItem(LAST_CHECKED_AT_KEY, String(ts));
  } catch {
    /* 조용히 무시 */
  }
}

// 설치 저수준 절차 — 클릭 경로/캐리오버 자동 경로 공용. UI 상태(state.update)는
// 건드리지 않는다 — 두 호출부의 "실패 시 어떻게 보여줄지"가 다르기 때문에
// 그건 각 호출부가 직접 처리한다.
async function applyUpdate(update: Update): Promise<void> {
  writePendingVersion(null);
  try {
    // Windows: 이 호출이 설치 프로그램을 띄운 뒤 프로세스를 즉시 종료하므로
    // 아래 relaunch()는 이 플랫폼에서 정상적으로 도달하지 않는다(검증된 사실).
    // macOS/Linux: 프로세스가 살아있으므로 relaunch()가 반드시 실행돼야 새
    // 버전이 뜬다.
    await update.install();
    await relaunch();
  } catch (err) {
    // 실패 — 다음 기회를 위해 플래그를 되살린다(무한 재시도 크래시 없이, 다음
    // 부팅 체크가 다시 같은 버전을 pending으로 인식하게 한다).
    writePendingVersion(update.version);
    throw err;
  }
}

// 사이드바 버튼 클릭 — 사용자가 직접 트리거한 유일한 경로라 실패 시 최소한의
// 안내(토스트)를 허용한다. 정상 흐름이면 Windows는 프로세스가 이미 죽어 이 함수
// 끝까지 도달하지 않고, macOS는 relaunch()가 성공해 곧 새 프로세스가 뜬다.
export async function applyUpdateFromButton(): Promise<void> {
  if (!currentUpdate || state.update.installing) return;
  state.update.installing = true;
  notifyChange();
  try {
    await applyUpdate(currentUpdate);
  } catch {
    state.update.installing = false;
    notifyChange();
    showToast('업데이트 적용에 실패했습니다. 잠시 후 다시 시도해주세요.');
  }
}

// 사이드바 "업데이트 확인" 버튼 — 유일한 사용자 트리거 수동 체크 경로.
// tryCheckAndOfferUpdate(manual: true)를 그대로 재사용한다(24시간 임계값은
// 애초에 이 함수에 없으므로 강제 체크가 된다 — 새 파이프라인을 만들지 않는다).
// state.update.checking만 이 함수가 소유한다(자동 배경 체크는 절대 건드리지
// 않음 — 그래야 배경 체크 중에 사이드바가 "확인 중…"으로 깜빡이지 않는다).
export async function checkForUpdateFromButton(): Promise<void> {
  if (isChecking) {
    // 부팅 직후 1회 자동 체크 또는 1시간 주기 배경 체크와 정확히 겹친 극히
    // 드문 경우 — 재진입 가드(isChecking) 때문에 tryCheckAndOfferUpdate가
    // 아무 일도 안 하고 조용히 return해버리면 버튼을 눌렀는데 반응이 없는
    // 것처럼 보인다. 최소한의 안내만 준다.
    showToast('이미 업데이트를 확인하고 있습니다. 잠시 후 다시 시도해주세요.');
    return;
  }
  state.update.checking = true;
  notifyChange();
  try {
    await tryCheckAndOfferUpdate(true);
  } finally {
    state.update.checking = false;
    notifyChange();
  }
}

// manual=true(위 checkForUpdateFromButton)일 때만 결과를 showToast로 알린다.
// manual=false(기본값, 부팅 1회 + 1시간 주기 배경 체크)는 기존 그대로 모든
// 실패/최신 경로에서 침묵한다 — 이 매개변수를 추가한 것 외에 자동 호출부
// (initUpdateCheck 등)는 한 글자도 바뀌지 않았다(기본 인자라 그대로 manual=false).
async function tryCheckAndOfferUpdate(manual = false): Promise<void> {
  if (isChecking) return;
  isChecking = true;
  try {
    let update: Update | null;
    try {
      update = await check({ timeout: CHECK_TIMEOUT_MS });
    } catch {
      // 엔드포인트 미게시(404)·네트워크 오류 등 — 자동 체크는 조용히 무시(에러
      // 배너/토스트/모달 금지, 정상 사용 중 업데이트 서버 문제로 방해받으면
      // 안 된다). 수동 버튼 클릭이면 사용자가 결과를 기다리고 있으므로 최소
      // 안내만 준다 — 원인을 캐묻지 않고 담백하게(사용자가 할 수 있는 조치가
      // 없다).
      if (manual) showToast('업데이트를 확인하지 못했습니다. 잠시 후 다시 시도해주세요.');
      return;
    } finally {
      writeLastCheckedAt(Date.now());
    }
    if (!update) {
      // 이미 최신 버전 — 자동 체크는 화면에 어떤 변화도 없다. 수동 클릭이면
      // "눌렀는데 아무 일도 안 일어났다"로 오인하지 않도록 알려준다.
      if (manual) showToast('현재 최신 버전입니다.');
      return;
    }

    try {
      await update.download();
    } catch {
      // 서명 검증 실패(pubkey 미설정 과도기 등) — 자동 체크는 조용히 무시,
      // 버튼도 띄우지 않는다. 수동 클릭이면 위 check() 실패와 동일하게 담백한
      // 실패 안내만 준다.
      if (manual) showToast('업데이트를 확인하지 못했습니다. 잠시 후 다시 시도해주세요.');
      return;
    }

    currentUpdate = update;
    const pendingVersion = readPendingVersion();
    if (pendingVersion === update.version) {
      // 캐리오버: 지난 실행에서 이미 이 버전을 버튼으로 보여줬는데 클릭 없이
      // 종료됐다 — 요구사항 4에 따라 원칙적으로는 이번 실행에서 자동 적용을
      // 시도하되(리뷰 M2 대응), 그 전에 반드시: ①비모달 카운트다운으로 예고하고
      // 취소 수단을 주며 ②부팅 직후·무입력 상태가 아니면 ③저장 중이면 각각
      // 버튼 경로로 강등한다. 아래 세 조건 중 하나라도 걸리면 자동 적용을 포기한다.
      const demoteToButton = (): void => {
        state.update.available = true;
        state.update.version = update.version;
        state.update.installing = false;
        notifyChange();
      };

      if (!isEligibleForAutoApply() || isAnySavingInProgress()) {
        // 요구사항 2·3: 부팅 후 시간이 지났거나 이미 입력이 있었거나, 저장
        // 진행 중이면 카운트다운조차 띄우지 않고 곧장 버튼만 노출한다.
        demoteToButton();
        return;
      }

      const proceed = await showAutoApplyCountdown(update.version);
      // 카운트다운이 흐르는 동안에도 저장이 시작될 수 있으니 진행 직전 다시 확인한다.
      if (!proceed || isAnySavingInProgress()) {
        demoteToButton();
        return;
      }

      try {
        await applyUpdate(update);
      } catch {
        // 이번에도 실패(예: 여전히 오프라인) — 버튼을 띄워 수동 재시도를 허용한다.
        demoteToButton();
      }
      return;
    }

    // 처음 보는 버전 — 조용히 배지/버튼만 노출한다(강제 적용 없음, 요구사항 2).
    writePendingVersion(update.version);
    state.update.available = true;
    state.update.version = update.version;
    state.update.installing = false;
    notifyChange();
  } finally {
    isChecking = false;
  }
}

// main.ts handleNavigation()이 인증 게이트(state.authenticated) 통과 후 세션당
// 정확히 한 번 호출한다(fire-and-forget — 메인 렌더를 기다리게 하지 않는다).
// 로그인 전에는 이 함수 자체가 호출되지 않는다. 부팅 시 1회 체크는 무조건 실행하고, 이후
// 앱이 켜져 있는 동안은 24시간 경과 여부를 주기적으로 확인해 추가 체크를 최대
// 24시간에 1번으로 제한한다. 이미 버튼이 떠 있으면(state.update.available)
// 보여줄 게 이미 있으니 추가 체크를 건너뛴다.
export function initUpdateCheck(): void {
  void tryCheckAndOfferUpdate();
  if (periodicTimer !== null) return;
  periodicTimer = window.setInterval(() => {
    if (state.update.available) return;
    if (Date.now() - readLastCheckedAt() < RECHECK_THRESHOLD_MS) return;
    void tryCheckAndOfferUpdate();
  }, PERIODIC_POLL_INTERVAL_MS);
}
