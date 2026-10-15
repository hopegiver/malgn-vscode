// 수집키 자동 연동 엔진 — 허브 계약 D-1~D-15를 구현한다
// (malgnai-public `docs/design/device-ingest-key-provisioning.md` 요약 D, 개정 3.1).
//
// 역할 분담
//   otel_hub_client.rs  HTTP 왕복·응답 해석(`IngestApi`)
//   otel_link.rs        연동 상태 모델·발급 전 선검사(`mint_gate`)·Tauri 커맨드·스케줄러
//   이 파일             "언제 무엇을 부르고 결과를 어떻게 다루는가" — 점검 분기, 메모리 상태, 백오프
//
// 시간은 호출자가 넘기는 `now`(밀리초, 단조 증가)로만 센다 — 엔진 안에서 sleep하지 않고 시계를 읽지
// 않는다. 그래서 백오프·폴링·점검 간격을 가짜 시계로 시험할 수 있다. 스케줄러(otel_link.rs)가 10초마다
// `tick`을 부른다.
//
// 메모리 상태(`Memory`)는 영속하지 않는다(계약 D 머리말). 앱이 재기동되면 모두 초기값이고, 서버가 같은
// 판단을 다시 돌려주므로 잃어도 무해하다. 디스크·키체인에 수집키 판단용 상태를 쓰지 않는다.
//   - confirm 거절 여부(D-1)            declined
//   - "기능 없음" 재점검 시각(D-7a)     phase=FeatureUnavailable + 점검 간격
//   - 회전 예산 retry_at(D-14)          budget_hold
//   - 일시 오류 백오프 간격(D-15)       backoff_ms / next_retry_at
//   - 연속 유실 점검 수(D-5)            lost_checks
//   - 설정 쓰기 실패 플래그+원문(D-5)   pending_write
//
// R-4(수집키 원문의 키체인 사본)는 채택하지 않았다. 근거는 `otel_link.rs` 머리말.
//
// mhd_/mhk_ 비노출: 기기 토큰은 `DeviceTokens::latest()`가 `SecretString`으로 주고 `IngestApi` 호출에만
// 쓴다 — `Phase`·`RuntimeView`·로그 어디에도 실리지 않는다. 발급 원문(mhk_)은 `accept_key`에서
// `apply_ingest_key`로만 가고, 쓰기 실패 시에만 `pending_write`(메모리, Debug 가림)에 머문다.

use crate::hub_auth::api::HubApi;
use crate::hub_auth::error::HubAuthError;
use crate::otel_hub_client::{
    ApiError, BlockedReason, CredentialMeta, CredentialState, HttpError, IngestApi, IssuePolicy,
};
use crate::otel_ingest_key::{normalize_ingest_key, SecretString};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// 점검 간격(D 머리말: "1시간 이상 간격").
pub(crate) const CHECK_INTERVAL_MS: u64 = 3_600_000;
/// 첫 데이터 폴링 간격(D-12: "10초 이상").
pub(crate) const POLL_INTERVAL_MS: u64 = 15_000;
/// 첫 데이터 폴링 한도(D-12: 15분).
pub(crate) const WATCH_LIMIT_MS: u64 = 15 * 60_000;
/// 일시 오류 백오프(D-15): 1분에서 시작해 2배씩, 최대 1시간.
pub(crate) const BACKOFF_START_MS: u64 = 60_000;
pub(crate) const BACKOFF_MAX_MS: u64 = 3_600_000;
/// 연속으로 유실된 점검이 이 수에 닿으면 자동 회전을 멈춘다(D-5).
pub(crate) const MAX_LOST_CHECKS: u8 = 2;
/// `RATE_LIMITED`인데 `Retry-After`가 없을 때의 대기(계약 F: 60초).
const DEFAULT_RATE_LIMIT_SECS: u64 = 60;
/// `ROTATE_BUDGET_EXCEEDED`인데 `Retry-After`가 없을 때의 보수적 대기.
const DEFAULT_BUDGET_SECS: u64 = 3_600;

// ---------------- 외부 의존 ----------------

/// 최신 기기 토큰(`mhd_`)을 돌려준다. 호출마다 키체인을 다시 읽는다 — 갱신 스케줄러가 그사이 토큰을
/// 바꿨을 수 있다(D-6). 연결이 없으면 None.
pub(crate) trait DeviceTokens: Send + Sync {
    fn latest(&self) -> Option<SecretString>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProbeResult {
    /// `GET /api/devices/self`가 200 — 연결은 살아 있다.
    Alive,
    /// 401 또는 409 `ALREADY_RENEWED` — 연결이 끊겼다.
    Dead,
    /// 5xx·네트워크 오류·그 밖 — 판정 못 한다.
    Unknown,
}

/// 401의 원인을 가르는 확인 호출(D-7a). 기존 경로 `GET /api/devices/self`를 쓴다.
pub(crate) trait SelfProbe: Send + Sync {
    fn probe(&self, device_token: &str) -> ProbeResult;
}

pub(crate) struct HubSelfProbe<'a>(pub &'a dyn HubApi);

impl SelfProbe for HubSelfProbe<'_> {
    fn probe(&self, device_token: &str) -> ProbeResult {
        match self.0.self_info(device_token) {
            Ok(_) => ProbeResult::Alive,
            Err(HubAuthError::Http { status: 401, .. }) => ProbeResult::Dead,
            Err(HubAuthError::Http { status: 409, ref code }) if code == "ALREADY_RENEWED" => {
                ProbeResult::Dead
            }
            Err(_) => ProbeResult::Unknown,
        }
    }
}

pub(crate) struct Env<'a> {
    pub path: &'a Path,
    pub tokens: &'a dyn DeviceTokens,
    pub api: &'a dyn IngestApi,
    pub probe: &'a dyn SelfProbe,
    /// 발급 요청의 label(계약 E 규칙으로 만든 값).
    pub label: &'a str,
}

// ---------------- 상태 ----------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    /// 앱 기동 후 첫 점검.
    Startup,
    /// 허브 (재)연결 직후.
    Connected,
    /// 1시간 이상 간격의 정기 점검.
    Periodic,
    /// 백오프·`Retry-After`가 끝나 이어 하는 점검.
    Resume,
    /// 사용자의 [다시 시도] — 연속 유실 수·백오프·거절 기록을 비우고 1번부터 점검한다.
    User,
    /// 설정 변경으로 수집 준비 상태가 바뀌었을 수 있어 하는 점검(자동 한계는 그대로).
    Settings,
}

/// 마지막 점검의 결과(= UI가 보여 줄 연동 단계). 데이터를 품지 않는다 — 기기 토큰·키·URL이 들어갈 수 없다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
    /// 별도 단계 없음(상태는 settings.json 실물에서 도출한다).
    #[default]
    Idle,
    /// 점검·발급 중.
    Checking,
    /// 허브 기기 토큰이 없다(허브 미연결·만료).
    NoDeviceToken,
    /// 웹에서 폐기된 키 — 사람의 확인을 기다린다(D-1).
    ConfirmRequired,
    /// 사용자가 확인을 거절했다 — 이번 앱 실행 동안 다시 묻지 않는다.
    ConfirmDeclined,
    /// 이 연결로는 발급·회전이 불가능하다(D-1·D-10).
    Blocked(Option<BlockedReason>),
    /// 사용자 유효 키 상한(D-8).
    CredentialLimit,
    /// 이 기기의 회전 예산 소진(D-14).
    RotateBudget { retry_at: Option<String> },
    /// 응답 유실로 끝난 점검이 연속 2회 — 자동 회전 중지(D-5).
    RotationLost,
    /// 연결이 끊겼다 — 재연결 필요(D-7).
    Reconnect,
    /// 서버에 이 기능이 아직 없다(D-7a 200) — 1시간 뒤 다시 점검.
    FeatureUnavailable,
    /// 발급은 됐으나 settings.json에 쓰지 못했다(D-5). 원문은 메모리에 있다.
    WriteFailed,
    /// 데스크톱 앱 발급 토큰이 아니다(D-11) — 재시도하지 않는다.
    NotDesktopToken,
    /// 서버가 요청을 거부했다(D-11 외 4xx) — 자동 재시도하지 않는다.
    HubRejected,
    /// 500·네트워크 오류(D-15) — 백오프 후 자동 재시도.
    Transient,
    /// 요청 빈도 초과(429 `RATE_LIMITED`, D-14) — `Retry-After` 뒤 같은 점검을 이어 한다.
    RateLimited,
    /// 발급 전 선검사: 수집 설정이 켜져 있지 않다.
    NotReady,
    /// 발급 전 선검사: 엔드포인트가 허브 밖이거나 평문이다.
    EndpointNotHub,
    /// 허브가 돌려준 값이 수집키 형식이 아니다.
    InvalidKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Watch {
    prefix: String,
    started: u64,
    last_poll: Option<u64>,
}

#[derive(Default, Debug)]
struct Memory {
    phase: Phase,
    last_check: Option<u64>,
    forced: Option<Trigger>,
    next_retry_at: Option<u64>,
    backoff_ms: u64,
    declined: bool,
    budget_hold: Option<(u64, Option<String>)>,
    lost_checks: u8,
    pending_write: Option<SecretString>,
    watch: Option<Watch>,
    confirmed_prefix: Option<String>,
    first_data_timed_out: bool,
}

/// UI가 쓰는 엔진 상태 복사본(원문 없음).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RuntimeView {
    pub phase: Phase,
    /// 첫 데이터가 확인된 키의 앞 8자.
    pub confirmed_prefix: Option<String>,
    /// 첫 데이터를 기다리며 폴링 중이다(D-12).
    pub waiting_first_data: bool,
    /// 15분 안에 첫 데이터가 오지 않아 폴링을 멈췄다.
    pub first_data_timed_out: bool,
}

#[derive(Default)]
struct RunState {
    /// 409 `TOKEN_SUPERSEDED` 재시도를 이미 썼다(점검당 1회, D-6).
    token_retried: bool,
    /// 409 `ALREADY_ROTATED`/404 `NO_DEVICE_CREDENTIAL` 뒤 GET부터 다시 판정했다(점검당 1회, D-9).
    reeval_used: bool,
    /// 이번 점검에서 서버에 보낸 rotate 요청 수. 응답 유실 재시도를 포함해 최대 `MAX_ROTATES_PER_CHECK`회(D 머리말).
    rotate_calls: u8,
    /// 점검을 시작할 때의 연결 세대(`LinkEngine::epoch`). 연결 해제·재연결로 세대가 바뀌면 이 점검의 결과는
    /// 설정 파일에도 엔진 상태에도 반영하지 않는다(RV-05).
    epoch: u64,
    /// 사용자의 [다시 시도]로 시작한 점검이다 — 15분 타임아웃 뒤에도 첫 데이터 확인을 다시 시작한다(RV-03).
    restart_watch: bool,
}

/// 한 번의 점검에서 rotate는 응답 유실 재시도를 포함해 최대 2회다(계약 D 머리말).
const MAX_ROTATES_PER_CHECK: u8 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mint {
    Create { confirm: bool },
    Rotate,
}

pub(crate) struct LinkEngine {
    mem: Mutex<Memory>,
    run: Mutex<()>,
    notifier: OnceLock<Box<dyn Fn() + Send + Sync>>,
    /// 허브 연결 세대. 연결 해제·재연결(`on_disconnected`/`on_connected`)마다 올라간다. 서버 응답을 기다리는 동안
    /// (최대 35초) 연결이 바뀌면 그 점검은 낡은 점검이다 — 낡은 점검이 해제로 지운 수집키를 설정 파일에 다시 쓰거나
    /// 새 연결의 상태를 덮어쓰지 않게 한다(RV-05).
    epoch: AtomicU64,
    /// 설정 파일 쓰기(`apply_guarded`)와 연결 해제의 키 삭제(`disconnect_with`)를 서로 직렬화한다. `run`과 달리
    /// 서버 응답을 기다리는 동안 잡지 않으므로(파일 입출력 동안만) 해제가 막히지 않는다.
    gate: Mutex<()>,
}

/// 키 앞 8자(`mhk_` + 4자) — 허브 `token_prefix`와 비교하는 값.
fn prefix8(key: &str) -> String {
    key.chars().take(8).collect()
}

impl LinkEngine {
    pub(crate) fn new() -> Self {
        Self {
            mem: Mutex::new(Memory::default()),
            run: Mutex::new(()),
            notifier: OnceLock::new(),
            epoch: AtomicU64::new(0),
            gate: Mutex::new(()),
        }
    }

    fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    fn is_stale(&self, st: &RunState) -> bool {
        self.epoch() != st.epoch
    }

    fn lock_gate(&self) -> std::sync::MutexGuard<'_, ()> {
        self.gate.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 상태가 바뀔 때 부르는 훅(프론트에 변경 이벤트를 보낸다). 한 번만 설정된다.
    pub(crate) fn set_notifier(&self, f: Box<dyn Fn() + Send + Sync>) {
        let _ = self.notifier.set(f);
    }

    fn notify(&self) {
        if let Some(f) = self.notifier.get() {
            f();
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut Memory) -> R) -> R {
        let mut guard = self.mem.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut guard)
    }

    pub(crate) fn view(&self) -> RuntimeView {
        self.with(|m| RuntimeView {
            phase: m.phase.clone(),
            confirmed_prefix: m.confirmed_prefix.clone(),
            waiting_first_data: m.watch.is_some(),
            first_data_timed_out: m.first_data_timed_out,
        })
    }

    /// 다음 점검이 필요한가(스케줄러가 부른다).
    pub(crate) fn due(&self, now: u64) -> Option<Trigger> {
        self.with(|m| {
            if m.phase == Phase::Checking {
                return None;
            }
            if let Some(t) = m.forced {
                return Some(t);
            }
            let Some(last) = m.last_check else {
                return Some(Trigger::Startup);
            };
            if let Some(at) = m.next_retry_at {
                return (now >= at).then_some(Trigger::Resume);
            }
            (now >= last.saturating_add(CHECK_INTERVAL_MS)).then_some(Trigger::Periodic)
        })
    }

    /// 허브를 (재)연결했다 — 새 연결로 취급한다(D-7: 재연결하면 GET은 `auto`가 된다). 메모리 상태를
    /// 모두 비우고 곧바로 점검하도록 예약한다. 이전 연결에서 진행 중이던 점검은 낡은 점검이 된다(세대 증가).
    pub(crate) fn on_connected(&self) {
        {
            let _gate = self.lock_gate();
            self.epoch.fetch_add(1, Ordering::SeqCst);
            self.with(|m| {
                *m = Memory::default();
                m.forced = Some(Trigger::Connected);
            });
        }
        self.notify();
    }

    /// 허브 연결 해제 — 메모리 상태(보관 중인 미기록 원문 포함)를 비운다. 점검은 예약하지 않는다.
    /// (실제 경로는 `disconnect_with`로 키 삭제까지 한 임계구역에서 한다 — 이 형태는 테스트용이다.)
    #[cfg(test)]
    pub(crate) fn on_disconnected(&self, now: u64) {
        self.disconnect_with(now, || ());
    }

    /// 연결 해제 + 그 직후의 후속 작업 `after`(수집키 삭제)를 **같은 임계구역**에서 한다(D-13, RV-05). 먼저 세대를
    /// 올리고 메모리를 비운 뒤 `after`를 실행하므로, 서버 응답을 기다리던 점검이 뒤늦게 201을 받아도
    /// `apply_guarded`가 세대 불일치를 보고 키를 쓰지 않는다 — "해제 후 지운 키가 다시 써지는" 순서가 없다.
    pub(crate) fn disconnect_with<R>(&self, now: u64, after: impl FnOnce() -> R) -> R {
        let out = {
            let _gate = self.lock_gate();
            self.epoch.fetch_add(1, Ordering::SeqCst);
            self.with(|m| {
                *m = Memory::default();
                // 연결이 없으니 기동 점검을 다시 하지 않는다. 다음 연결 때 `on_connected`가 예약한다.
                m.last_check = Some(now);
                m.phase = Phase::NoDeviceToken;
            });
            after()
        };
        self.notify();
        out
    }

    /// 낡지 않은 점검만 설정 파일에 키를 쓴다. 세대 확인과 쓰기를 `gate` 안에서 함께 해서 연결 해제의 키 삭제와
    /// 엇갈리지 않는다.
    fn apply_guarded(&self, env: &Env, key: SecretString, st: &RunState) -> Applied {
        let _gate = self.lock_gate();
        if self.is_stale(st) {
            return Applied::Stale;
        }
        match crate::otel_settings::apply_ingest_key(env.path, key) {
            Ok(()) => Applied::Written,
            Err(_) => Applied::Failed,
        }
    }

    /// 스케줄러의 한 박자: 점검이 필요하면 점검하고, 아니면 첫 데이터 폴링을 한다.
    pub(crate) fn tick(&self, env: &Env, now: u64) {
        if let Some(trigger) = self.due(now) {
            self.run_check(env, trigger, now);
        } else {
            self.poll_first_data(env, now);
        }
    }

    // ---------------- 점검 ----------------

    pub(crate) fn run_check(&self, env: &Env, trigger: Trigger, now: u64) {
        // 점검은 한 번에 하나만 돈다. 이미 돌고 있으면 그 결과를 기다리지 않고 돌아간다.
        let Ok(_run) = self.run.try_lock() else {
            return;
        };
        self.with(|m| {
            m.forced = None;
            m.phase = Phase::Checking;
            m.last_check = Some(now);
            m.next_retry_at = None;
            if trigger == Trigger::User {
                // 사용자의 [다시 시도]는 연속 유실 수·백오프·거절을 비우고 1번부터 점검한다(D-5).
                // 회전 예산 보류(D-14)는 서버가 정한 시각까지 유지한다 — 일찍 부르면 같은 429를 받을 뿐이다.
                m.lost_checks = 0;
                m.backoff_ms = 0;
                m.declined = false;
            }
        });
        self.notify();
        let mut st = RunState {
            epoch: self.epoch(),
            restart_watch: trigger == Trigger::User,
            ..RunState::default()
        };
        let phase = self.evaluate(env, now, &mut st);
        self.finish(phase, &st);
    }

    fn finish(&self, phase: Phase, st: &RunState) {
        if self.is_stale(st) {
            // 점검 도중 연결이 해제·재연결됐다 — 이 결과는 이전 연결의 것이라 현재 상태를 덮지 않는다.
            return;
        }
        self.with(|m| {
            // 백오프(D-15)는 연속된 일시 오류에서만 자란다. 점검이 일시 오류가 아닌 결과(정상 완료나 서버의
            // 명확한 답)로 끝나면 간격을 처음(1분)으로 되돌린다.
            if !matches!(phase, Phase::Transient | Phase::RateLimited) {
                m.backoff_ms = 0;
            }
            m.phase = phase;
        });
        self.notify();
    }

    fn evaluate(&self, env: &Env, now: u64, st: &mut RunState) -> Phase {
        let Some(mut token) = env.tokens.latest() else {
            return Phase::NoDeviceToken;
        };
        // 파일 상태가 이미 발급을 막고 있으면(해석 불가·형식이 다른 Authorization) 서버를 부르지 않는다.
        if let Err(stop) = crate::otel_link::file_blocks_linking(env.path) {
            return stop;
        }
        let state = match self.get(env, &mut token, now, st) {
            Ok(s) => s,
            Err(p) => return p,
        };
        self.dispatch(env, &mut token, state, now, st)
    }

    /// 1번: `issue_policy`에 따라 분기한다.
    fn dispatch(
        &self,
        env: &Env,
        token: &mut SecretString,
        state: CredentialState,
        now: u64,
        st: &mut RunState,
    ) -> Phase {
        // 보관 중인 미기록 원문은 현행 키가 있을 때만 의미가 있다(D-5: 없으면 버리고 1번부터 다시 판정).
        if state.issue_policy != IssuePolicy::HasCurrent {
            self.with(|m| m.pending_write = None);
        }
        match state.issue_policy {
            IssuePolicy::Auto => self.mint(env, token, Mint::Create { confirm: false }, now, st),
            IssuePolicy::ConfirmRequired => self.need_confirm(),
            IssuePolicy::Blocked => Phase::Blocked(state.blocked_reason),
            IssuePolicy::HasCurrent => match state.current {
                Some(cur) => self.has_current(env, token, cur, now, st),
                // has_current인데 current가 없는 응답은 계약 위반이다 — 판정하지 않고 일시 오류로 다룬다.
                None => self.transient(now),
            },
        }
    }

    /// D-1: 사람에게 묻는다. 이번 실행에서 이미 거절했다면 묻지 않는다.
    fn need_confirm(&self) -> Phase {
        // 웹에서 폐기된 키다 — 파일에 남은 그 키를 같은 실행에서 이전에 확인한 수신(`confirmed_prefix`)·진행 중이던
        // 첫 데이터 확인(`watch`)·타임아웃 안내가 "정상 수집 중"처럼 가리지 않게 비운다(RV-01).
        self.with(|m| {
            m.confirmed_prefix = None;
            m.watch = None;
            m.first_data_timed_out = false;
        });
        if self.with(|m| m.declined) {
            Phase::ConfirmDeclined
        } else {
            Phase::ConfirmRequired
        }
    }

    /// 4번: 현행 키가 있을 때.
    fn has_current(
        &self,
        env: &Env,
        token: &mut SecretString,
        cur: CredentialMeta,
        now: u64,
        st: &mut RunState,
    ) -> Phase {
        // 5번: 설정 쓰기 실패 플래그가 켜져 있으면 rotate하지 않고, 서버의 현행 키와 같은 원문으로만 다시 쓴다.
        let pending = self.with(|m| m.pending_write.clone());
        if let Some(p) = pending {
            if prefix8(p.expose()) == cur.token_prefix {
                if let Err(stop) = crate::otel_link::mint_gate(env.path) {
                    return stop;
                }
                return match self.apply_guarded(env, p.clone(), st) {
                    Applied::Written => {
                        self.with(|m| {
                            m.pending_write = None;
                            m.lost_checks = 0;
                        });
                        self.after_applied(&cur.token_prefix, now);
                        Phase::Idle
                    }
                    Applied::Failed => Phase::WriteFailed,
                    Applied::Stale => Phase::NoDeviceToken,
                };
            }
            // 그 사이 웹 폐기·회전·cron 폐기 — 원문을 버리고 1번부터 다시 판정한다(아래 일반 경로).
            self.with(|m| m.pending_write = None);
        }

        let file_prefix = crate::otel_link::file_key_prefix(env.path);
        if file_prefix.as_deref() == Some(cur.token_prefix.as_str()) {
            // 이미 적용된 상태다 — 끝낸다. 누가 회전했는지는 판단하지 않는다.
            self.with(|m| m.lost_checks = 0);
            if cur.has_signal() {
                self.with(|m| {
                    m.confirmed_prefix = Some(cur.token_prefix.clone());
                    m.watch = None;
                    m.first_data_timed_out = false;
                });
            } else {
                // D-12: 15분 안에 신호가 없어 확인을 멈춘 키(`first_data_timed_out`)는 정기 점검의 이 GET 1회로만
                // 다시 본다 — 매시간 15분 폴링을 되풀이하지 않는다(키당 하루 약 1,460회 GET). 폴링을 다시 여는 것은
                // 메모리가 비어 있는 앱 기동·연결 직후(타임아웃 표시가 없다)와 사용자의 [다시 시도]뿐이다.
                let keep_stopped = self.with(|m| {
                    if st.restart_watch {
                        m.first_data_timed_out = false;
                    }
                    m.first_data_timed_out
                });
                if !keep_stopped {
                    self.start_watch_if_absent(&cur.token_prefix, now);
                }
            }
            return Phase::Idle;
        }
        // 앞 8자가 다르거나 설정에 키가 없다 → 회전한다.
        self.mint(env, token, Mint::Rotate, now, st)
    }

    fn start_watch_if_absent(&self, prefix: &str, now: u64) {
        self.with(|m| {
            let same = m.watch.as_ref().map(|w| w.prefix == prefix).unwrap_or(false);
            if !same {
                m.watch = Some(Watch {
                    prefix: prefix.to_string(),
                    started: now,
                    last_poll: None,
                });
            }
        });
    }

    /// 새 키가 설정에 적용됐다 — 첫 데이터 폴링을 시작한다.
    fn after_applied(&self, prefix: &str, now: u64) {
        self.with(|m| {
            m.confirmed_prefix = None;
            m.first_data_timed_out = false;
            m.watch = Some(Watch {
                prefix: prefix.to_string(),
                started: now,
                last_poll: None,
            });
        });
    }

    // ---------------- 발급·회전 ----------------

    fn mint(
        &self,
        env: &Env,
        token: &mut SecretString,
        kind: Mint,
        now: u64,
        st: &mut RunState,
    ) -> Phase {
        // 선검사(RV-004/RV-005): 설정 파일 실물이 이 키를 안전하게 받을 수 있는지. 서버를 부르기 전에 멈춘다.
        if let Err(stop) = crate::otel_link::mint_gate(env.path) {
            return stop;
        }
        if kind == Mint::Rotate {
            // 회전 예산(D-14): retry_at까지 rotate하지 않는다. settings.json은 그대로 둔다.
            let hold = self.with(|m| match m.budget_hold.clone() {
                Some((until, at)) if now < until => Some(at),
                Some(_) => {
                    m.budget_hold = None;
                    None
                }
                None => None,
            });
            if let Some(retry_at) = hold {
                return Phase::RotateBudget { retry_at };
            }
            // 연속 유실 상한(D-5): 유실로 끝난 점검이 2회 연속이면 자동 회전을 멈춘다.
            if self.with(|m| m.lost_checks) >= MAX_LOST_CHECKS {
                return Phase::RotationLost;
            }
        }

        let mut lost_in_this_check: u8 = 0;
        loop {
            if self.is_stale(st) {
                // 요청을 기다리는 동안 연결이 해제·재연결됐다 — 이전 연결의 기기 토큰으로 더 보내지 않는다.
                return Phase::NoDeviceToken;
            }
            if kind == Mint::Rotate {
                // 이 점검에서 rotate를 이미 상한만큼 보냈다 — 점검을 끝낸다(다음 점검의 4번에 맡긴다).
                if st.rotate_calls >= MAX_ROTATES_PER_CHECK {
                    return Phase::Idle;
                }
                st.rotate_calls += 1;
            }
            let result = match kind {
                Mint::Create { confirm } => env.api.create(token.expose(), env.label, confirm),
                Mint::Rotate => env.api.rotate(token.expose()),
            };
            let err = match result {
                Ok(created) => return self.accept_key(env, created.token, now, st),
                Err(e) => e,
            };
            match err {
                ApiError::Lost => {
                    lost_in_this_check += 1;
                    match kind {
                        Mint::Rotate => {
                            // D-5: 응답 유실이면 즉시 1회 다시 보낸다. 다만 직전 점검이 유실로 끝났다면
                            // 재시도 없이 1회만 보낸다 — 그래야 자동으로 만들어지는 키가 최대 3개(2+1)다.
                            let previous_lost = self.with(|m| m.lost_checks) > 0;
                            if lost_in_this_check == 1 && !previous_lost && st.rotate_calls < MAX_ROTATES_PER_CHECK {
                                continue;
                            }
                            let n = self.with(|m| {
                                m.lost_checks = m.lost_checks.saturating_add(1);
                                m.lost_checks
                            });
                            return if n >= MAX_LOST_CHECKS {
                                Phase::RotationLost
                            } else {
                                self.transient(now)
                            };
                        }
                        Mint::Create { .. } => {
                            // 발급(POST) 응답 유실: 다시 보내면 이미 만들어졌을 때 409 DEVICE_CREDENTIAL_EXISTS가
                            // 나서 4번으로 처리된다(예산을 소모하지 않는다).
                            if lost_in_this_check == 1 {
                                continue;
                            }
                            return self.transient(now);
                        }
                    }
                }
                ApiError::Http(h) => {
                    match self.mint_http_error(env, token, kind, &h, now, st) {
                        MintNext::Done(p) => return p,
                        MintNext::RetryWithFreshToken => {
                            match env.tokens.latest() {
                                Some(t) => *token = t,
                                None => return Phase::NoDeviceToken,
                            }
                            continue;
                        }
                        MintNext::Reevaluate => return self.reevaluate(env, token, now, st),
                    }
                }
                other => return self.common_error(env, token, &other, now),
            }
        }
    }

    fn mint_http_error(
        &self,
        env: &Env,
        token: &SecretString,
        kind: Mint,
        h: &HttpError,
        now: u64,
        st: &mut RunState,
    ) -> MintNext {
        match (h.status, h.code.as_str()) {
            // D-6: 키체인에서 최신 mhd_를 다시 읽고 1회 재시도.
            (409, "TOKEN_SUPERSEDED") if !st.token_retried => {
                st.token_retried = true;
                MintNext::RetryWithFreshToken
            }
            (409, "TOKEN_SUPERSEDED") => MintNext::Done(self.transient(now)),
            // 발급 응답 유실 뒤 재시도, 또는 GET과 POST 사이에 다른 경로가 만든 키 → 4번.
            (409, "DEVICE_CREDENTIAL_EXISTS") => match (&h.current, kind) {
                (Some(cur), Mint::Create { .. }) => {
                    MintNext::Done(self.has_current_from_conflict(env, token, cur.clone(), now, st))
                }
                _ => MintNext::Reevaluate,
            },
            // D-10: 직전 키가 수동 폐기됨 → confirm_required와 같이 처리.
            (409, "REISSUE_CONFIRMATION_REQUIRED") => match kind {
                Mint::Create { confirm: false } => MintNext::Done(self.need_confirm()),
                // 이미 확인을 받아 보냈는데 또 요구하면 서버 계약과 어긋난다 — 자동 재시도하지 않는다.
                _ => MintNext::Done(Phase::HubRejected),
            },
            // D-10: GET이 has_current여도 rotate가 이 409를 줄 수 있다(유예 키만 quota로 폐기).
            (409, "DEVICE_REISSUE_BLOCKED") => MintNext::Done(Phase::Blocked(h.blocked_reason)),
            // D-8: 자동 처리하지 않는다.
            (409, "CREDENTIAL_LIMIT") => MintNext::Done(Phase::CredentialLimit),
            // D-9: GET을 다시 호출해 1번부터. 같은 점검에서 또 나면 끝낸다.
            (409, "ALREADY_ROTATED") | (404, "NO_DEVICE_CREDENTIAL") if kind == Mint::Rotate => {
                MintNext::Reevaluate
            }
            // D-14: 회전 예산. Retry-After(=retry_at)까지 rotate하지 않는다.
            (429, "ROTATE_BUDGET_EXCEEDED") => {
                let secs = h.retry_after_secs.unwrap_or(DEFAULT_BUDGET_SECS);
                let until = now.saturating_add(secs.saturating_mul(1000));
                let retry_at = h.retry_at.clone();
                self.with(|m| m.budget_hold = Some((until, retry_at.clone())));
                MintNext::Done(Phase::RotateBudget { retry_at })
            }
            _ => MintNext::Done(self.common_error(env, token, &ApiError::Http(h.clone()), now)),
        }
    }

    /// 409 `DEVICE_CREDENTIAL_EXISTS` 본문의 `current`로 4번을 처리한다. 4번이 다시 발급 경로(rotate)로
    /// 이어질 수 있으므로 `Create`의 재귀가 아니라 회전 경로로만 간다.
    fn has_current_from_conflict(
        &self,
        env: &Env,
        token: &SecretString,
        cur: CredentialMeta,
        now: u64,
        st: &mut RunState,
    ) -> Phase {
        let mut token = token.clone();
        self.has_current(env, &mut token, cur, now, st)
    }

    /// D-9: GET을 1회 다시 호출해 1번부터 분기한다. 같은 점검에서 두 번째면 끝낸다.
    fn reevaluate(
        &self,
        env: &Env,
        token: &mut SecretString,
        now: u64,
        st: &mut RunState,
    ) -> Phase {
        if st.reeval_used {
            return Phase::Idle;
        }
        st.reeval_used = true;
        match self.get(env, token, now, st) {
            Ok(state) => self.dispatch(env, token, state, now, st),
            Err(p) => p,
        }
    }

    /// 201을 받았다 — 형식을 검증하고 설정에 적용한다(D-2·D-5).
    fn accept_key(&self, env: &Env, key: SecretString, now: u64, st: &RunState) -> Phase {
        // 응답을 받았으니 "유실로 끝난 점검"이 아니다(rotate 201이든 발급 201이든 수는 0이 된다).
        if !self.is_stale(st) {
            self.with(|m| {
                m.lost_checks = 0;
                m.budget_hold = None;
            });
        }
        if normalize_ingest_key(key.expose()).is_err() {
            return Phase::InvalidKey;
        }
        let prefix = prefix8(key.expose());
        match self.apply_guarded(env, key.clone(), st) {
            // 응답을 기다리는 동안 연결이 해제·재연결됐다 — 받은 키는 쓰지 않고 버린다(RV-05). 서버는 해제 때
            // 이 기기의 수집키를 이미 폐기했으므로 살아 있는 키가 남지 않는다.
            Applied::Stale => Phase::NoDeviceToken,
            Applied::Written => {
                self.with(|m| m.pending_write = None);
                self.after_applied(&prefix, now);
                Phase::Idle
            }
            Applied::Failed => {
                // 쓰기 실패를 회전으로 되풀이하면 회전 예산만 소진된다. 원문을 메모리에 보관하고 플래그를
                // 켠다(Phase::WriteFailed). 이후 점검은 GET만 한다.
                self.with(|m| m.pending_write = Some(key));
                Phase::WriteFailed
            }
        }
    }

    // ---------------- GET ----------------

    fn get(
        &self,
        env: &Env,
        token: &mut SecretString,
        now: u64,
        st: &mut RunState,
    ) -> Result<CredentialState, Phase> {
        loop {
            match env.api.get(token.expose()) {
                Ok(state) => return Ok(state),
                Err(ApiError::Http(h)) if h.status == 409 && h.code == "TOKEN_SUPERSEDED" && !st.token_retried => {
                    st.token_retried = true;
                    match env.tokens.latest() {
                        Some(t) => *token = t,
                        None => return Err(Phase::NoDeviceToken),
                    }
                }
                Err(e) => return Err(self.common_error(env, token, &e, now)),
            }
        }
    }

    // ---------------- 공통 오류 ----------------

    /// 경로에 상관없는 오류의 처리: 401(D-7), ALREADY_RENEWED(D-7b), 403(D-11), 429 RATE_LIMITED(D-14),
    /// 5xx·네트워크(D-15), 그 밖의 4xx. 응답 유실(`Lost`)은 호출자가 경로에 맞게 다루고, 여기 오면
    /// 일시 오류로 본다.
    fn common_error(
        &self,
        env: &Env,
        token: &SecretString,
        err: &ApiError,
        now: u64,
    ) -> Phase {
        match err {
            ApiError::NotSent | ApiError::Lost => self.transient(now),
            ApiError::Http(h) => match (h.status, h.code.as_str()) {
                (401, _) => self.on_unauthorized(env, token, now),
                (409, "ALREADY_RENEWED") => Phase::Reconnect,
                (403, _) => Phase::NotDesktopToken,
                (429, _) => {
                    // RATE_LIMITED: Retry-After초 뒤에 같은 점검을 이어 한다. 백오프를 키우지 않는다.
                    let secs = h.retry_after_secs.unwrap_or(DEFAULT_RATE_LIMIT_SECS);
                    self.with(|m| m.next_retry_at = Some(now.saturating_add(secs.saturating_mul(1000))));
                    Phase::RateLimited
                }
                (s, _) if s >= 500 => self.transient(now),
                _ => Phase::HubRejected,
            },
        }
    }

    /// D-7a: 같은 mhd_로 `GET /api/devices/self`를 1회 호출해 원인을 가른다. 401 본문의 message로는
    /// 판정하지 않는다.
    fn on_unauthorized(&self, env: &Env, token: &SecretString, now: u64) -> Phase {
        match env.probe.probe(token.expose()) {
            // 연결은 살아 있다 — 서버에 이 기능이 없는 상태다. 재로그인하지 않고 1시간 뒤 다시 점검한다.
            ProbeResult::Alive => Phase::FeatureUnavailable,
            ProbeResult::Dead => Phase::Reconnect,
            ProbeResult::Unknown => self.transient(now),
        }
    }

    /// D-15: 1분부터 2배씩 늘려 최대 1시간 간격으로 다시 시도한다. 재로그인하지 않고 settings.json도
    /// 바꾸지 않는다.
    fn transient(&self, now: u64) -> Phase {
        self.with(|m| {
            m.backoff_ms = if m.backoff_ms == 0 {
                BACKOFF_START_MS
            } else {
                (m.backoff_ms.saturating_mul(2)).min(BACKOFF_MAX_MS)
            };
            m.next_retry_at = Some(now.saturating_add(m.backoff_ms));
        });
        Phase::Transient
    }

    // ---------------- 확인(confirm_required) ----------------

    /// 사용자가 확인창에서 거절했다(D-1) — 이번 앱 실행 동안 다시 묻지 않는다.
    pub(crate) fn decline_confirm(&self) {
        self.with(|m| {
            m.declined = true;
            if m.phase == Phase::ConfirmRequired {
                m.phase = Phase::ConfirmDeclined;
            }
        });
        self.notify();
    }

    /// 사용자가 확인창에서 동의했다(D-1) — `POST {label, confirm_reissue:true}`를 보낸다.
    pub(crate) fn accept_confirm(&self, env: &Env, now: u64) {
        let Ok(_run) = self.run.try_lock() else {
            return;
        };
        if self.with(|m| m.phase != Phase::ConfirmRequired) {
            return; // 확인을 요구하는 상태가 아니다(이미 처리됐거나 상태가 바뀌었다).
        }
        self.with(|m| {
            m.phase = Phase::Checking;
            m.last_check = Some(now);
        });
        self.notify();
        let mut st = RunState {
            epoch: self.epoch(),
            ..RunState::default()
        };
        let phase = match env.tokens.latest() {
            Some(mut token) => self.mint(env, &mut token, Mint::Create { confirm: true }, now, &mut st),
            None => Phase::NoDeviceToken,
        };
        self.finish(phase, &st);
    }

    // ---------------- 첫 데이터 확인(D-12) ----------------

    /// 10초 이상 간격으로 GET을 폴링한다. 신호가 도착하거나 15분이 지나면 멈춘다.
    pub(crate) fn poll_first_data(&self, env: &Env, now: u64) {
        let due = self.with(|m| match &m.watch {
            Some(w) if m.phase != Phase::Checking => {
                if now.saturating_sub(w.started) >= WATCH_LIMIT_MS {
                    Some(true)
                } else if w.last_poll.map_or(true, |t| now.saturating_sub(t) >= POLL_INTERVAL_MS) {
                    Some(false)
                } else {
                    None
                }
            }
            _ => None,
        });
        let Some(expired) = due else {
            return;
        };
        if expired {
            // 15분 — 폴링을 멈추고 "새 Claude Code 세션을 열면 수집이 시작됩니다"를 안내한다. 이후에는 다음
            // 점검(1시간 간격·앱 기동)의 GET 1회로만 다시 확인한다(has_current의 D-12 주석 참고).
            self.with(|m| {
                m.watch = None;
                m.first_data_timed_out = true;
            });
            self.notify();
            return;
        }
        let Ok(_run) = self.run.try_lock() else {
            return;
        };
        let (prefix, token) = (self.with(|m| m.watch.as_ref().map(|w| w.prefix.clone())), env.tokens.latest());
        let (Some(prefix), Some(token)) = (prefix, token) else {
            self.with(|m| m.watch = None);
            return;
        };
        self.with(|m| {
            if let Some(w) = m.watch.as_mut() {
                w.last_poll = Some(now);
            }
        });
        let epoch = self.epoch();
        let reply = env.api.get(token.expose());
        if self.epoch() != epoch {
            // 응답을 기다리는 동안 연결이 해제·재연결됐다 — 이전 연결의 확인 결과는 반영하지 않는다.
            return;
        }
        match reply {
            Ok(state) => {
                let cur_has_signal = state
                    .current
                    .as_ref()
                    .filter(|c| c.token_prefix == prefix)
                    .map(|c| c.has_signal());
                match cur_has_signal {
                    Some(true) => {
                        self.with(|m| {
                            m.confirmed_prefix = Some(prefix);
                            m.watch = None;
                        });
                        self.notify();
                    }
                    Some(false) => {}
                    // 현행 키가 바뀌었다(웹 폐기·다른 곳의 회전) — 이 키를 더 기다리지 않는다. 판정은 다음 점검이 한다.
                    None => {
                        self.with(|m| m.watch = None);
                        self.notify();
                    }
                }
            }
            // 응답 유실·연결 실패는 다음 폴링에서 다시 본다. 그 밖의 오류(401·429·5xx…)는 폴링을 멈추고
            // 일반 경로(다음 점검)에 맡긴다.
            Err(ApiError::Lost) | Err(ApiError::NotSent) => {}
            Err(_) => {
                self.with(|m| m.watch = None);
                self.notify();
            }
        }
    }
}

/// `apply_guarded`의 결과.
enum Applied {
    Written,
    Failed,
    /// 점검 도중 연결이 바뀌어 쓰지 않았다.
    Stale,
}

enum MintNext {
    Done(Phase),
    RetryWithFreshToken,
    Reevaluate,
}

#[cfg(test)]
pub(crate) mod testkit;
#[cfg(test)]
mod tests;
