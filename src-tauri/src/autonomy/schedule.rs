// 고정시각(FixedTime) 스케줄 계산 — 로컬 벽시계 "HH:MM"(+요일) 기준으로 다음
// 실행 인스턴트를 구한다(설계 §2~§5). `scheduler::select_due()`는 이 모듈의
// 결과값(`next_run_at`)만 소비할 뿐 모드를 전혀 모른다 — 모드 차이는 오로지
// "next_run_at을 누가 어떻게 계산하느냐"에만 나타난다.
//
// 핵심 함수를 `Tz: TimeZone` 제네릭으로 둔다 — 테스트에서 `FixedOffset`을
// 주입해 CI 러너 타임존(보통 UTC)과 무관하게 결정적으로 검증하기 위함이다.
// `chrono::Local`을 직접 부르면 러너마다 결과가 달라져 테스트가 비결정적이
// 된다. DST 분기는 `FixedOffset`으로 재현할 수 없으므로 `pick()`을 별도
// 순수 함수로 분리해 `LocalResult::{Single,Ambiguous,None}` 값을 직접 넣어
// 검증한다.
//
// ⚠️ 불변식(나중에 "하루 여러 시각"이나 "시간 단위 고정 스케줄"을 추가할 때
// 반드시 재검토): 지금 설계는 회차 간 최소 간격이 24시간이라는 전제 위에서,
// `MISSED_RUN_GRACE_MINUTES`(config.rs)가 그 간격보다 짧다는 사실만으로
// "따라잡기는 최대 1회"를 별도 로직 없이 보장한다
// (`missed_run_grace_is_shorter_than_minimum_occurrence_gap` 테스트로 고정).
// 이 전제가 깨지면 이 보장도 깨진다.

use super::config::{AutonomyTaskConfig, ScheduleMode, MISSED_RUN_GRACE_MINUTES, STARTUP_GRACE_MINUTES};
use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, TimeZone, Utc};

/// FixedTime 모드의 실행 시각+요일 사양. `AutonomyTaskConfig::fixed_time_spec()`이
/// 정규화(`normalize_task`)를 이미 거친 값으로만 만들어 돌려준다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FixedTimeSpec {
    pub hour: u32,   // 0..=23
    pub minute: u32, // 0..=59
    pub days: Vec<u8>, // 0=일 … 6=토. 비었으면 매일.
}

/// runner.rs `TaskSnapshot`가 들고 다니는 최소 스케줄 정보. "running으로
/// 마킹한 시점"의 값을 그대로 스냅숏해, 실행 도중 사용자가 시각을 바꿔도
/// 이번 회차의 다음 계산은 마킹 시점 spec을 쓰게 한다(기존 스냅숏 철학 유지).
#[derive(Clone, Debug)]
pub(crate) struct ScheduleSnapshot {
    pub mode: ScheduleMode,
    pub interval_minutes: u32,
    pub fixed: Option<FixedTimeSpec>,
}

impl From<&AutonomyTaskConfig> for ScheduleSnapshot {
    fn from(task: &AutonomyTaskConfig) -> Self {
        Self {
            mode: task.schedule_mode,
            interval_minutes: task.interval,
            fixed: task.fixed_time_spec(),
        }
    }
}

/// `"09:00"`/`"9:00"`(24시간, 콜론 구분)만 허용한다. 시는 1~2자리, 분은
/// 반드시 2자리. 범위 이탈(`25:00`, `09:60`)·형식 불일치(`0900`, `""`,
/// `"09:00:00"`)는 전부 거부한다.
pub(crate) fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return None;
    }
    if !h.bytes().all(|b| b.is_ascii_digit()) || !m.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hour: u32 = h.parse().ok()?;
    let minute: u32 = m.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some((hour, minute))
}

/// 0패딩 정규형(`"09:05"`) 문자열로 되돌린다.
pub(crate) fn format_hhmm(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

/// `days`가 비었으면(매일) 항상 참, 아니면 `date`의 요일(0=일…6=토)이 목록에
/// 있는지 본다.
pub(crate) fn day_allowed(date: NaiveDate, days: &[u8]) -> bool {
    days.is_empty() || days.contains(&(date.weekday().num_days_from_sunday() as u8))
}

/// DST 3분기 규칙만 담는 순수 함수. `primary`가 봄 DST로 존재하지 않으면
/// (`None`) 1시간 뒤로 민 `fallback`을 시도한다. 가을 DST 중복(`Ambiguous`)은
/// 항상 이른 쪽을 택해 그날 1회만 실행되게 한다.
pub(crate) fn pick<Tz: TimeZone>(
    primary: LocalResult<DateTime<Tz>>,
    fallback: LocalResult<DateTime<Tz>>,
) -> Option<DateTime<Utc>> {
    match primary {
        LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(earlier, _) => Some(earlier.with_timezone(&Utc)),
        LocalResult::None => match fallback {
            LocalResult::Single(dt) | LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
            LocalResult::None => None,
        },
    }
}

/// `date`의 `hour:minute`(tz 로컬 벽시계)을 UTC 인스턴트로 해석한다. 봄 DST로
/// 그 벽시계가 존재하지 않으면 `naive + 1시간`으로 한 번 더 시도한다.
pub(crate) fn resolve_local_instant<Tz: TimeZone>(
    tz: &Tz,
    date: NaiveDate,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    let naive = date.and_hms_opt(hour, minute, 0)?;
    let primary = tz.from_local_datetime(&naive);
    let fallback = tz.from_local_datetime(&(naive + Duration::hours(1)));
    pick(primary, fallback)
}

/// `after`보다 "엄격히 큰(>)" 첫 회차의 UTC 인스턴트를 반환한다. 오늘부터
/// 하루씩 최대 8일(오늘+7일+DST 스킵 여유 1) 훑는다 — 단일 요일 지정이면
/// 다음 주 같은 요일까지 가야 하므로 상한이 최소 8이어야 한다. 허용 요일이
/// 0개(병리 입력)면 루프가 끝까지 매치를 못 찾고 `None`을 반환한다.
pub(crate) fn next_occurrence_in<Tz: TimeZone>(
    tz: &Tz,
    spec: &FixedTimeSpec,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let after_local = after.with_timezone(tz);
    let mut date = after_local.date_naive();
    for _ in 0..=8 {
        if day_allowed(date, &spec.days) {
            if let Some(instant) = resolve_local_instant(tz, date, spec.hour, spec.minute) {
                if instant > after {
                    return Some(instant);
                }
            }
        }
        date = date.succ_opt()?;
    }
    None
}

/// 기기 로컬 타임존(`chrono::Local`)으로 해석하는 얇은 래퍼. 프로덕션 코드는
/// 전부 이 함수를 쓰고, 테스트는 결정성을 위해 `next_occurrence_in`에
/// `FixedOffset`을 직접 주입한다.
pub(crate) fn next_occurrence(spec: &FixedTimeSpec, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    next_occurrence_in(&Local, spec, after)
}

/// (A) 최초 등록 — 앱 시작 직후 / 외부에서 파일이 추가된 것을 tick이 처음
/// 발견했을 때. `now - MISSED_RUN_GRACE`를 계산 앵커로 써서 "30분 이내에
/// 지나간 회차"가 과거 인스턴트로 반환되게 하고, 그 결과를
/// `now + STARTUP_GRACE`와 비교해 더 늦은 쪽을 택한다(재시작 몰림 방지).
pub(crate) fn initial_next_run_at(
    task: &AutonomyTaskConfig,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let startup_floor = now + Duration::minutes(STARTUP_GRACE_MINUTES as i64);
    match task.schedule_mode {
        ScheduleMode::Interval => Some(startup_floor),
        ScheduleMode::FixedTime => {
            let spec = task.fixed_time_spec()?;
            let grace = Duration::minutes(MISSED_RUN_GRACE_MINUTES as i64);
            let candidate = next_occurrence(&spec, now - grace)?;
            Some(candidate.max(startup_floor))
        }
    }
}

/// (C)+(D) 편집 후 재스케줄(M3) / 중지→재개(M2) 공용 재계산 — 따라잡기 창
/// (`MISSED_RUN_GRACE`)도 `STARTUP_GRACE` 하한도 적용하지 않는다. "지금 이
/// 순간을 기준으로 다음 회차가 언제인가"만 답한다.
///
/// R1(리뷰 구조 제언 — 재계산 계기를 enum `RescheduleCause`로 명시하고
/// 계기별 표로 앵커를 결정하자는 제안)은 **채택하지 않는다.** 이번 라운드가
/// 실제로 처리하는 계기는 (A)앱시작(`initial_next_run_at`, 기존 유지)
/// / (B)완료(`next_run_after_finish`, 기존 유지) / (C)편집·(D)재개(이 함수
/// 하나로 충분히 겹친다) 셋뿐이라, 계기가 3갈래로 정리되는 지금은 함수
/// 하나로 "따라잡기 앵커 없음"이라는 공통 규칙을 표현하는 것으로 충분하고
/// enum·표 도입은 이번 스코프가 필요로 하는 것보다 넓은 리팩터다. 다만
/// M2·M3·m1이 "앵커를 호출부마다 따로 골랐다가 어긋난" 동일 뿌리라는 진단은
/// 그대로 받아들여 이 함수 하나로 세 결함을 함께 해소한다(개별 앵커 재구현
/// 금지) — 계기가 4개 이상으로 늘어나는 다음 라운드에서 R1을 다시 검토할
/// 것을 권한다.
///
/// Interval 모드는 `now + interval`(사용자가 방금 값을 바꿨거나 task를
/// 재개했으니, 그 값 그대로 지금부터 카운트다운을 다시 시작하는 것이
/// 자연스럽다 — FixedTime에서 Interval로 전환할 때 옛 고정 시각이 그대로
/// 남는 문제(m1)도 이 경로로 없어진다). FixedTime 모드는
/// `next_occurrence(spec, now)` — 재개 시 "밀린 회차를 지금 돌리지 않고
/// 다음 예정 시각까지 기다린다"는 PM 결정(M2)과 "오늘 이미 돈 회차를
/// 편집 저장이 다시 잡으면 안 된다"는 요구(M3)를 앵커 없이 그대로 만족한다.
pub(crate) fn reschedule_next_run_at(
    task: &AutonomyTaskConfig,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    match task.schedule_mode {
        ScheduleMode::Interval => Some(now + Duration::minutes(task.interval as i64)),
        ScheduleMode::FixedTime => {
            let spec = task.fixed_time_spec()?;
            next_occurrence(&spec, now)
        }
    }
}

/// (B) 실행 완료 후 — `runner::run_task()` 말미에서 호출한다. 따라잡기 창도
/// STARTUP_GRACE도 적용하지 않는다(방금 돌았고, 재시작 몰림 상황이 아니다).
pub(crate) fn next_run_after_finish(
    schedule: &ScheduleSnapshot,
    finished_at: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    match schedule.mode {
        ScheduleMode::Interval => {
            Some(finished_at + Duration::minutes(schedule.interval_minutes as i64))
        }
        ScheduleMode::FixedTime => {
            let spec = schedule.fixed.as_ref()?;
            next_occurrence(spec, finished_at)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Timelike};

    fn kst() -> FixedOffset {
        FixedOffset::east_opt(9 * 3600).unwrap()
    }

    fn spec(hour: u32, minute: u32, days: &[u8]) -> FixedTimeSpec {
        FixedTimeSpec {
            hour,
            minute,
            days: days.to_vec(),
        }
    }

    /// KST 로컬 날짜/시각을 UTC 인스턴트로 만드는 테스트 헬퍼.
    fn kst_instant(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        kst()
            .with_ymd_and_hms(y, m, d, h, mi, 0)
            .unwrap()
            .with_timezone(&Utc)
    }

    // ---------------- parse/format ----------------

    #[test]
    fn parse_hhmm_accepts_padded_and_unpadded_hours() {
        assert_eq!(parse_hhmm("09:00"), Some((9, 0)));
        assert_eq!(parse_hhmm("9:00"), Some((9, 0)));
    }

    #[test]
    fn parse_hhmm_rejects_out_of_range_and_malformed() {
        assert_eq!(parse_hhmm("25:00"), None);
        assert_eq!(parse_hhmm("09:60"), None);
        assert_eq!(parse_hhmm("0900"), None);
        assert_eq!(parse_hhmm(""), None);
        assert_eq!(parse_hhmm("09:00:00"), None);
    }

    #[test]
    fn format_hhmm_zero_pads_both_fields() {
        assert_eq!(format_hhmm(9, 5), "09:05");
    }

    // ---------------- next_occurrence (경계 케이스 표 §4.5) ----------------

    // 표 #1 — 오늘이 허용 요일, 시각 아직 안 지남 → 오늘.
    #[test]
    fn next_occurrence_returns_today_when_time_not_yet_passed() {
        // 화 2026-09-15 08:00 KST, spec 09:00 매일.
        let after = kst_instant(2026, 9, 15, 8, 0);
        let s = spec(9, 0, &[]);
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 15, 9, 0)));
    }

    // 표 #3 — 엄격 `>` 회귀. 완료 직후 재계산이 같은 회차를 다시 잡아
    // 무한 재실행하는 것을 막는 핵심 테스트.
    #[test]
    fn next_occurrence_excludes_exact_same_instant() {
        let after = kst_instant(2026, 9, 15, 9, 0); // 화 09:00:00 정각
        let s = spec(9, 0, &[2, 4]);
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 17, 9, 0)), "목요일로 넘어가야 한다");
    }

    // 표 #4 — days=[](매일), 방금 지남 → 다음날.
    #[test]
    fn next_occurrence_moves_to_tomorrow_for_daily_when_just_passed() {
        let after = kst_instant(2026, 9, 15, 9, 0) + Duration::seconds(30);
        let s = spec(9, 0, &[]);
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 16, 9, 0)));
    }

    // 표 #2 — 명세 예시(§4.4) 고정: 화 09:00:30, days=[2,4] → 목 09:00.
    #[test]
    fn next_occurrence_tue_thu_after_tuesday_returns_thursday() {
        let after = kst_instant(2026, 9, 15, 9, 0) + Duration::seconds(30);
        let s = spec(9, 0, &[2, 4]);
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 17, 9, 0)));
    }

    // 표 #5 — 단일 요일 주간 반복, 방금 지남 → 다음 주 같은 요일(루프 상한
    // ≥8 회귀).
    #[test]
    fn next_occurrence_single_weekday_wraps_to_next_week() {
        let after = kst_instant(2026, 9, 15, 9, 0) + Duration::seconds(30); // 화
        let s = spec(9, 0, &[2]);
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 22, 9, 0)), "다음 주 화요일이어야 한다");
    }

    #[test]
    fn next_occurrence_empty_days_means_every_day() {
        let after = kst_instant(2026, 9, 15, 9, 0) + Duration::seconds(30);
        let s = spec(9, 0, &[]);
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 16, 9, 0)));
    }

    // 표 #8 — 주말 걸침: days=[0,6], 금 10:00 → 토 09:00.
    #[test]
    fn next_occurrence_crosses_weekend_boundary() {
        // 2026-09-18은 금요일.
        let after = kst_instant(2026, 9, 18, 10, 0);
        let s = spec(9, 0, &[0, 6]);
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 19, 9, 0)), "토요일 09:00이어야 한다");
    }

    #[test]
    fn next_occurrence_returns_none_when_no_day_ever_allowed() {
        let after = kst_instant(2026, 9, 15, 8, 0);
        let s = spec(9, 0, &[9]); // 정규화를 거치지 않은 병리 입력을 직접 넣는다
        let result = next_occurrence_in(&kst(), &s, after);
        assert_eq!(result, None);
    }

    // ---------------- pick (DST 순수 함수) ----------------

    #[test]
    fn pick_prefers_earlier_instant_on_ambiguous_local_time() {
        let earlier = kst_instant(2026, 9, 15, 1, 30);
        let later = kst_instant(2026, 9, 15, 2, 30);
        let primary: LocalResult<DateTime<FixedOffset>> = LocalResult::Ambiguous(
            earlier.with_timezone(&kst()),
            later.with_timezone(&kst()),
        );
        let fallback: LocalResult<DateTime<FixedOffset>> = LocalResult::None;
        assert_eq!(pick(primary, fallback), Some(earlier));
    }

    #[test]
    fn pick_shifts_forward_one_hour_on_nonexistent_local_time() {
        let shifted = kst_instant(2026, 9, 15, 3, 30);
        let primary: LocalResult<DateTime<FixedOffset>> = LocalResult::None;
        let fallback: LocalResult<DateTime<FixedOffset>> =
            LocalResult::Single(shifted.with_timezone(&kst()));
        assert_eq!(pick(primary, fallback), Some(shifted));
    }

    #[test]
    fn pick_returns_none_when_both_primary_and_fallback_nonexistent() {
        let primary: LocalResult<DateTime<FixedOffset>> = LocalResult::None;
        let fallback: LocalResult<DateTime<FixedOffset>> = LocalResult::None;
        assert_eq!(pick(primary, fallback), None);
    }

    // ---------------- initial_next_run_at / next_run_after_finish ----------------

    fn fixed_task(at_time: &str, days: &[u8]) -> AutonomyTaskConfig {
        AutonomyTaskConfig {
            id: "t".to_string(),
            name: "n".to_string(),
            prompt: "p".to_string(),
            subagent: None,
            interval: 30,
            schedule_mode: ScheduleMode::FixedTime,
            at_time: Some(at_time.to_string()),
            days: days.to_vec(),
            enabled: true,
            timeout: None,
        }
    }

    // 표 #9 — 놓친 회차 따라잡기: grace 창 안이면 오늘 회차를 따라잡는다.
    // `initial_next_run_at`은 프로덕션 경로에서 `chrono::Local`로 해석하는
    // `next_occurrence`를 내부에서 그대로 쓰므로(설계 §4.3 시그니처가
    // tz 파라미터를 받지 않는다), CI 러너의 타임존이 무엇이든 항상 맞도록
    // "5분 전"의 실제 로컬 벽시계를 `chrono::Local::now()`에서 뽑아
    // atTime으로 쓴다 — 하드코딩된 KST 리터럴과 실제 시스템 tz가 어긋나서
    // 생기는 비결정성을 피한다.
    #[test]
    fn initial_next_run_at_catches_up_run_missed_within_grace() {
        let now = Local::now();
        let missed = now - Duration::minutes(5); // grace(30분) 안
        let task = fixed_task(&format_hhmm(missed.hour(), missed.minute()), &[]);
        let now_utc = now.with_timezone(&Utc);
        let result = initial_next_run_at(&task, now_utc);
        let floor = now_utc + Duration::minutes(STARTUP_GRACE_MINUTES as i64);
        assert_eq!(
            result,
            Some(floor),
            "grace 창 안의 회차는 STARTUP_GRACE 시점에 맞춰 즉시 따라잡아야 한다"
        );
    }

    // 표 #10 — 놓친 지 오래(grace 밖) → 오늘 회차는 건너뛰고 다음날로 밀린다.
    // 위와 같은 이유로 `Local::now()` 기준 동적 값을 쓴다.
    #[test]
    fn initial_next_run_at_skips_run_missed_beyond_grace() {
        let now = Local::now();
        let missed = now - Duration::minutes(40); // grace(30분) 밖
        let task = fixed_task(&format_hhmm(missed.hour(), missed.minute()), &[]);
        let now_utc = now.with_timezone(&Utc);
        let result = initial_next_run_at(&task, now_utc).unwrap();
        assert!(
            result >= now_utc + Duration::hours(20),
            "grace 밖이면 오늘 회차를 건너뛰고 다음날로 밀려야 한다: {result:?}"
        );
    }

    // 표 #11 — 재시작 몰림 방지 철학 회귀: STARTUP_GRACE보다 앞당겨지지 않는다.
    #[test]
    fn initial_next_run_at_never_earlier_than_startup_grace() {
        let now = Utc::now();
        let task = fixed_task("00:00", &[]); // 항상 이미 지났을 시각(매일 자정)
        let result = initial_next_run_at(&task, now).unwrap();
        let floor = now + Duration::minutes(STARTUP_GRACE_MINUTES as i64);
        assert!(result >= floor, "STARTUP_GRACE 하한보다 이르면 안 된다");
    }

    // 현행 동작 100% 보존 회귀 — Interval 모드는 now + STARTUP_GRACE 그대로.
    #[test]
    fn initial_next_run_at_for_interval_mode_is_now_plus_startup_grace() {
        let now = Utc::now();
        let mut task = fixed_task("09:00", &[]);
        task.schedule_mode = ScheduleMode::Interval;
        let result = initial_next_run_at(&task, now);
        assert_eq!(result, Some(now + Duration::minutes(STARTUP_GRACE_MINUTES as i64)));
    }

    // 현행 동작 100% 보존 회귀 — Interval 모드는 finished_at + interval분.
    #[test]
    fn next_run_after_finish_for_interval_mode_equals_finished_plus_interval() {
        let finished_at = Utc::now();
        let schedule = ScheduleSnapshot {
            mode: ScheduleMode::Interval,
            interval_minutes: 45,
            fixed: None,
        };
        let result = next_run_after_finish(&schedule, finished_at);
        assert_eq!(result, Some(finished_at + Duration::minutes(45)));
    }

    // m2 회귀(PM이 "유일한 치명 경로"로 지목) — next_run_after_finish의
    // FixedTime 분기 직접 커버리지. 기존 `next_occurrence_excludes_exact_same_instant`는
    // `next_occurrence_in`에 직접 `FixedOffset`을 주입해 검증하므로 실제
    // 프로덕션 호출 사슬(next_run_after_finish → next_occurrence → Local)을
    // 지나지 않는다 — 이 테스트는 그 사슬 전체를 통과시킨다. 완료 시각
    // 기준 로컬 벽시계로 오늘 회차를 만들어 "방금 이 회차가 끝났다"를
    // 재현하고, 반환값이 완료 시각보다 엄격히 미래이며(무한 재실행 방지)
    // 24시간 이내(다음 날 같은 시각)임을 단언한다.
    #[test]
    fn next_run_after_finish_for_fixed_time_mode_returns_next_days_occurrence() {
        let finished_at = Local::now().with_timezone(&Utc);
        let schedule = ScheduleSnapshot {
            mode: ScheduleMode::FixedTime,
            interval_minutes: 30,
            fixed: Some(FixedTimeSpec {
                hour: finished_at.with_timezone(&Local).hour(),
                minute: finished_at.with_timezone(&Local).minute(),
                days: Vec::new(),
            }),
        };
        let result = next_run_after_finish(&schedule, finished_at).unwrap();
        assert!(result > finished_at, "완료 시각과 정확히 같은 인스턴트를 다시 고르면 안 된다(무한 재실행 방지)");
        // DST 전환일의 짧은/긴 하루를 오탐하지 않도록 하한만 확인한다(기존
        // `initial_next_run_at_skips_run_missed_beyond_grace`와 같은 관용구).
        assert!(
            result >= finished_at + Duration::hours(20),
            "매일 같은 시각 반복이면 다음 회차는 내일이어야 한다: {result:?}"
        );
    }

    // M2/M3 회귀 — 편집 재스케줄/재개는 따라잡기 앵커를 쓰지 않으므로
    // "오늘 이미 지난 회차"가 있어도 절대 과거·즉시 인스턴트를 돌려주지
    // 않는다(방금 지난 회차가 있어도 다음 회차를 반환한다).
    #[test]
    fn reschedule_next_run_at_for_fixed_time_never_catches_up_a_recently_passed_occurrence() {
        let now = Local::now();
        let missed = now - Duration::minutes(5); // initial_next_run_at이라면 grace 안에서 따라잡을 창
        let task = fixed_task(&format_hhmm(missed.hour(), missed.minute()), &[]);
        let now_utc = now.with_timezone(&Utc);

        let result = reschedule_next_run_at(&task, now_utc).unwrap();

        assert!(
            result > now_utc,
            "편집/재개 재스케줄은 현재 시각보다 엄격히 미래여야 한다(밀린 회차를 즉시 돌리지 않는다): {result:?}"
        );
        assert!(
            result >= now_utc + Duration::hours(20),
            "오늘 회차는 이미 지났으므로 다음날로 밀려야 한다: {result:?}"
        );
    }

    // M3 회귀 — initial_next_run_at과 달리 STARTUP_GRACE 하한도 적용하지
    // 않는다(재시작 몰림 방지 로직은 앱 시작 전용).
    #[test]
    fn reschedule_next_run_at_for_interval_mode_restarts_countdown_from_now() {
        let now = Utc::now();
        let mut task = fixed_task("09:00", &[]);
        task.schedule_mode = ScheduleMode::Interval;
        task.interval = 20;
        let result = reschedule_next_run_at(&task, now);
        assert_eq!(
            result,
            Some(now + Duration::minutes(20)),
            "Interval 모드는 now + interval로 카운트다운을 다시 시작해야 한다"
        );
    }

    // 상수 불변식 회귀 — §3의 "따라잡기는 최대 1회" 경고를 코드로 고정한다.
    #[test]
    fn missed_run_grace_is_shorter_than_minimum_occurrence_gap() {
        const MINIMUM_OCCURRENCE_GAP_MINUTES: u32 = 24 * 60;
        assert!(MISSED_RUN_GRACE_MINUTES < MINIMUM_OCCURRENCE_GAP_MINUTES);
    }

    // `FixedOffset` 주입으로 CI 러너 타임존과 무관하게 결정적임을 확인한다.
    #[test]
    fn next_occurrence_is_deterministic_under_fixed_offset_timezone() {
        let after = kst_instant(2026, 9, 15, 8, 0);
        let s = spec(9, 0, &[]);
        let a = next_occurrence_in(&kst(), &s, after);
        let b = next_occurrence_in(&kst(), &s, after);
        assert_eq!(a, b);
        assert_eq!(a, Some(kst_instant(2026, 9, 15, 9, 0)));
    }
}
