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
// ⚠️ 불변식(예고했던 재검토가 이번 라운드에서 발동한다 — Hourly/Cron 도입):
// 지금까지의 설계는 회차 간 최소 간격이 24시간이라는 전제 위에서,
// `MISSED_RUN_GRACE_MINUTES`(config.rs)가 그 간격보다 짧다는 사실만으로
// "따라잡기는 최대 1회"를 별도 로직 없이 보장해 왔다
// (`missed_run_grace_is_shorter_than_minimum_occurrence_gap` 테스트로 고정).
// 재검토 결론 — 보장은 유지되지만 근거가 두 겹이 됐다:
//
// | 모드     | 회차 간 최소 간격 | grace(30분) 여유 근거가 유효한가                         |
// |----------|-------------------|-----------------------------------------------------------|
// | FixedTime| 24시간            | ✅ 여유 48배                                               |
// | Hourly   | 60분              | ✅ 유효하지만 여유가 2배로 줄었다(60 이상으로 올리면 깨짐) |
// | Cron     | 하한 없음(`*/5`)  | ❌ 무효 — 대신 `initial_next_run_at`이 회차 "목록"이 아닌  |
// |          |                   |    단일 인스턴트만 반환하고 그 값을 `startup_floor`가 한   |
// |          |                   |    점으로 누른다는 별도 근거로 "최대 1회"가 성립한다.      |
//
// 즉 가장 촘촘한 벽시계 모드는 이제 Hourly다 — 이 상수를 60 이상으로 올리는
// 사람은 반드시 위 표를 다시 볼 것.
//
// `pick()` DST 3분기 규칙의 모드 간 비대칭(설계 §4.3): FixedTime·Hourly는
// 봄 부재를 "naive + 1시간"으로 밀어서 실행하고, Cron은 그날 회차를
// 건너뛴다. Hourly의 판정 기준은 "밀린 인스턴트가 그 표현이 약속한 시각
// 집합 안에 있는가"다 — Hourly의 계약은 "매시 M분"이라 시(hour) 라벨이
// 밀려도 그 표현이 약속한 시각 그대로지만, Cron은 시(hour) 자체를
// 고정할 수 있어 미는 것이 표현식이 지정하지 않은 시각을 만들어낸다.
// ⚠️ FixedTime은 이 기준의 예외다 — FixedTime도 "그 시(hour) 그 분"을
// 고정하는 계약이라 이 기준을 그대로 적용하면 Cron처럼 '건너뜀'이 나와야
// 하지만, FixedTime은 레거시 호환으로 "밀어서 실행"을 유지한다. 다음 모드를
// 추가할 때 이 기준으로 판정하려면 FixedTime이 아니라 Hourly를 참조할 것.

use super::config::{
    AutonomyTaskConfig, ScheduleMode, MIN_INTERVAL_MINUTES, MISSED_RUN_GRACE_MINUTES,
    STARTUP_GRACE_MINUTES,
};
use chrono::{
    DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone,
    Timelike, Utc,
};

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
    /// Hourly 모드 스냅숏(§6.1 X3). 이 필드를 빠뜨리면
    /// `next_run_after_finish`가 조용히 `None`으로 떨어져 task가 1회 실행
    /// 후 영영 멈춘다.
    pub hourly_minute: Option<u8>,
    /// Cron 모드 스냅숏(§6.1 X3). 위와 같은 이유로 필수.
    pub cron: Option<String>,
}

impl From<&AutonomyTaskConfig> for ScheduleSnapshot {
    fn from(task: &AutonomyTaskConfig) -> Self {
        Self {
            mode: task.schedule_mode,
            interval_minutes: task.interval,
            fixed: task.fixed_time_spec(),
            hourly_minute: task.hourly_minute_spec(),
            cron: task.cron_expr().map(|s| s.to_string()),
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

/// `naive`(tz 로컬 벽시계)를 UTC 인스턴트로 해석한다. 봄 DST로 그 벽시계가
/// 존재하지 않으면 `naive + 1시간`으로 한 번 더 시도한다. `resolve_local_instant`
/// 와 `next_hourly_in`이 공유하는 DST 해석 정본이다 — 규칙을 두 곳에
/// 복제하면 두 모드의 DST 처리가 갈라질 자리가 생긴다.
pub(crate) fn resolve_local_naive<Tz: TimeZone>(
    tz: &Tz,
    naive: NaiveDateTime,
) -> Option<DateTime<Utc>> {
    let primary = tz.from_local_datetime(&naive);
    let fallback = tz.from_local_datetime(&(naive + Duration::hours(1)));
    pick(primary, fallback)
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
    resolve_local_naive(tz, naive)
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

/// 매시 `minute`분(로컬 벽시계) 회차 중 `after`보다 "엄격히 큰(>)" 첫
/// 인스턴트. `next_occurrence_in`의 날짜 루프를 시(hour) 루프로 축소한
/// 것이고, DST 해석은 같은 `resolve_local_naive`(=`pick`)를 그대로 쓴다 —
/// 봄 부재는 "naive + 1시간"으로 밀어 실행한다(파일 상단 불변식 참조: 이
/// fallback이 Hourly에서는 '수용'이 아니라 정답이다. 매시 M분이라는 계약이
/// 지정하지 않은 시각을 만들지 않는다).
///
/// 별도 단조성 가드가 필요 없다(설계 §4.4) — 후보는 로컬 naive 라벨을
/// +1시간씩 전진시키며 만들고 각 라벨의 확정 인스턴트는 다음 라벨의
/// 인스턴트를 넘지 않으므로(가을 중복=이른 쪽, 봄 부재=다음 라벨과 동일)
/// 후보 수열이 비감소다. 거부는 그 수열의 접두부에만 발생하고 실측상
/// 최대 1회(되감기 구간에서도 두 번째 후보는 항상 `after`보다 크다).
///
/// 루프 상한 `4`: 거부 최대 1회 + 해석 실패(2시간 점프 tz, 예:
/// `Antarctica/Troll`) 최대 1회 + 성공 1회 + 여유 1.
pub(crate) fn next_hourly_in<Tz: TimeZone>(
    tz: &Tz,
    minute: u32,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let after_local = after.with_timezone(tz);
    let mut naive = after_local
        .date_naive()
        .and_hms_opt(after_local.hour(), minute, 0)?;
    for _ in 0..4 {
        if let Some(instant) = resolve_local_naive(tz, naive) {
            if instant > after {
                return Some(instant);
            }
        }
        naive += Duration::hours(1);
    }
    None // fail-closed — 실존 tz에서는 도달하지 않는다.
}

/// 기기 로컬 타임존(`chrono::Local`)으로 해석하는 얇은 래퍼(`next_occurrence`
/// 와 같은 관례).
pub(crate) fn next_hourly(minute: u32, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    next_hourly_in(&Local, minute, after)
}

/// `next_from(anchor)`가 `after`보다 큰 값을 줄 때까지 앵커를 1시간씩
/// 밀어가며 재시도한다. `pick()`과 같은 이유로 분리한 순수 함수다 — DST는
/// `FixedOffset`으로 재현할 수 없으므로 "계산기"를 클로저로 주입해 값으로
/// 직접 검증한다. Cron 경로(`next_cron_in`) 전용 가드다 — FixedTime/Hourly는
/// 정방향 라벨 루프 + `> after` 필터 조합이 이 문제를 구조적으로 흡수하지만
/// (§4.4), Cron은 crate에 "한 번 묻고 끝내는" 구조라 가을 되감기 구간에서
/// `after`보다 이른 인스턴트가 돌아오는 경로가 실재한다(§5.4).
///
/// 되감기 폭은 실존 tz에서 1시간을 넘지 않으므로 1시간씩 최대 3회만 민다
/// (최초 시도 포함 총 4회). 전부 실패하면 `None`(fail-closed — 실행하지
/// 않는다. 무한루프 없음).
pub(crate) fn advance_until_strictly_after(
    mut next_from: impl FnMut(DateTime<Utc>) -> Option<DateTime<Utc>>,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let mut anchor = after;
    for _ in 0..4 {
        let candidate = next_from(anchor)?;
        if candidate > after {
            return Some(candidate);
        }
        anchor += Duration::hours(1);
    }
    None
}

/// cron-parser 0.11.2의 실측 CPU 소모 경로를 `parse()` 호출 **전에** 차단하는
/// 형태 가드(보안 조건 1). 어느 필드든 `split(',')` 후 전부 빈 문자열이면
/// (`", * * * *"` 류) crate 내부에서 빈 `BTreeSet`이 `Ok`로 반환돼 4년치
/// (약 210만 회)를 매 반복 재파싱하며 순회한다(최악 관측 1.7초, 입력 1자당
/// 약 14.5ms 선형 증가 — 콤마 2,000개면 29초, 100KB면 약 24분). 실측상 이
/// 가드가 통과시키는 정상 표현식의 파싱 비용은 최악 0.045ms다.
///
/// 두 진입점 모두에서 이 가드를 거친다(보안 조건 2): ① 저장 커맨드
/// 경계(`validate_cron`, `mod.rs`) ② 계산 경로(`next_cron_in` — 손으로
/// 고친 `autonomy.json`이 `scan_all_tasks`를 거쳐 이리로 들어올 수 있으므로
/// `normalize_task`를 믿지 않고 여기서도 다시 막는다).
fn cron_shape_is_safe(expr: &str) -> bool {
    const MAX_CRON_LEN: usize = 128;
    if expr.is_empty() || expr.len() > MAX_CRON_LEN {
        return false;
    }
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return false;
    }
    fields
        .iter()
        .all(|field| field.split(',').any(|part| !part.is_empty()))
}

/// naive는 우리가 만들지 않는다(설계 §5.2) — `after`를 tz 로컬로 넘겨
/// crate에 그대로 맡긴다. crate가 `LocalResult::Ambiguous(earlier, _) =>
/// break earlier` 규칙을 이미 쓰므로(lib.rs) "가을 모호는 이른 쪽" 원칙이
/// 세 모드에서 저절로 일치한다. 봄 부재는 crate가 그날 회차를 건너뛴다
/// (FixedTime/Hourly의 "+1시간"과 다름 — 파일 상단 불변식 표 참조, 의도된
/// 비대칭).
///
/// `advance_until_strictly_after`로 감싸는 이유(§5.4): 가을 되감기 구간에서
/// crate가 `after`보다 이른 인스턴트를 반환하는 경로가 실재한다 — crate에
/// "한 번 묻고 끝내는" 구조라 FixedTime/Hourly가 가진 정방향 라벨 루프의
/// 구조적 방어가 없다.
pub(crate) fn next_cron_in<Tz: TimeZone>(
    tz: &Tz,
    expr: &str,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    if !cron_shape_is_safe(expr) {
        return None;
    }
    advance_until_strictly_after(
        |anchor| {
            let local = anchor.with_timezone(tz);
            cron_parser::parse(expr, &local)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        },
        after,
    )
}

/// 기기 로컬 타임존으로 해석하는 얇은 래퍼(`next_occurrence`/`next_hourly`와
/// 같은 관례).
pub(crate) fn next_cron(expr: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    next_cron_in(&Local, expr, after)
}

/// `normalize_task`(config.rs)의 L2 정규화 전용 — 공백을 단일화하고, 보안
/// 가드(`cron_shape_is_safe`)와 파싱 가능성만 확인한다. 과잉 실행 브레이크
/// (`MIN_INTERVAL_MINUTES` 미만 거부)는 `validate_cron`(L1, 저장 시점 UX
/// 검증) 전용이라 여기서는 적용하지 않는다 — L2는 "손으로 고친 파일도
/// 안전한가"만 보장하면 되고, 브레이크 상수가 나중에 바뀌어도 이미 저장된
/// 정상 파일이 다음 로드 때 재해석되어서는 안 된다.
pub(crate) fn normalize_cron_expr(expr: &str) -> Option<String> {
    let normalized = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    if !cron_shape_is_safe(&normalized) {
        return None;
    }
    cron_parser::parse(&normalized, &Local::now()).ok()?;
    Some(normalized)
}

/// 저장 시점 거부(설계 §7.3, L1). crate의 영어 에러를 그대로 노출하지 않고
/// 한국어 고정 문구로 감싼다 — crate가 어느 칸이 틀렸는지 알려주지 않으므로
/// 필드 위치는 지어내지 않는다.
///
/// 검사 순서: ①길이/형태 가드(`cron_shape_is_safe`, parse() 호출 전) ②파싱
/// 가능성 ③과잉 실행 브레이크(다음 9회차를 더 뽑아 인접 간격 최솟값이
/// `MIN_INTERVAL_MINUTES` 이상인지 — 샘플링이라 상한 증명은 아니다. 최종
/// 브레이크는 `select_due`의 러닝 체크·concurrency 상한·task별 타임아웃).
///
/// 4년 하드캡 전용 문구(:361 부근)는 두 번째 이후 `parse` 호출이 실패할
/// 때만 반환되고, 최초 `parse`(:356 부근)가 실패하면 GENERIC_ERR로 묶인다.
/// 다만 이 순서가 "4년 캡으로 인한 실패는 항상 최초 파싱 성공 이후에만
/// 일어난다"는 것을 보장하지는 않는다 — 실측: `validate_cron("0 0 29 2 1")`은
/// 구조적으로 유효해 보이는 표현식이지만 최초 `parse`부터 실패해
/// GENERIC_ERR("5칸으로 적어 주세요")을 반환한다. 즉 두 실패가 항상 호출
/// 순서만으로 구분되는 것은 아니다(문구 분기 개선은 후속 과제).
pub(crate) fn validate_cron(expr: &str) -> Result<(), String> {
    const GENERIC_ERR: &str = "cron 표현식이 올바르지 않습니다. 5칸(분 시 일 월 요일)으로 적어 주세요. 요일은 0(일)~6(토)만 지원합니다(7은 미지원). 예: 0 9 * * 1-5";

    let normalized = expr.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err("cron 표현식을 입력해야 합니다.".to_string());
    }
    if !cron_shape_is_safe(&normalized) {
        return Err(GENERIC_ERR.to_string());
    }

    let now = Local::now();
    let first =
        cron_parser::parse(&normalized, &now).map_err(|_| GENERIC_ERR.to_string())?;

    let mut prev = first;
    for _ in 0..9 {
        let next = cron_parser::parse(&normalized, &prev).map_err(|_| {
            "4년 내 실행 시각을 찾을 수 없습니다. 표현식을 다시 확인해 주세요.".to_string()
        })?;
        if (next - prev).num_minutes() < MIN_INTERVAL_MINUTES as i64 {
            return Err(format!(
                "실행 간격이 너무 짧습니다. 최소 {MIN_INTERVAL_MINUTES}분 이상 간격이 되도록 표현식을 조정해 주세요."
            ));
        }
        prev = next;
    }

    Ok(())
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
    let grace = Duration::minutes(MISSED_RUN_GRACE_MINUTES as i64);
    match task.schedule_mode {
        ScheduleMode::Interval => Some(startup_floor),
        ScheduleMode::FixedTime => {
            let spec = task.fixed_time_spec()?;
            let candidate = next_occurrence(&spec, now - grace)?;
            Some(candidate.max(startup_floor))
        }
        ScheduleMode::Hourly => {
            let minute = task.hourly_minute_spec()?;
            let candidate = next_hourly(minute as u32, now - grace)?;
            Some(candidate.max(startup_floor))
        }
        ScheduleMode::Cron => {
            let expr = task.cron_expr()?;
            let candidate = next_cron(expr, now - grace)?;
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
        ScheduleMode::Hourly => {
            let minute = task.hourly_minute_spec()?;
            next_hourly(minute as u32, now)
        }
        ScheduleMode::Cron => {
            let expr = task.cron_expr()?;
            next_cron(expr, now)
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
        ScheduleMode::Hourly => {
            let minute = schedule.hourly_minute?;
            next_hourly(minute as u32, finished_at)
        }
        ScheduleMode::Cron => {
            let expr = schedule.cron.as_deref()?;
            next_cron(expr, finished_at)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

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
            hourly_minute: None,
            cron: None,
            enabled: true,
            timeout: None,
        }
    }

    fn hourly_task(minute: u8) -> AutonomyTaskConfig {
        AutonomyTaskConfig {
            id: "t".to_string(),
            name: "n".to_string(),
            prompt: "p".to_string(),
            subagent: None,
            interval: 30,
            schedule_mode: ScheduleMode::Hourly,
            at_time: None,
            days: Vec::new(),
            hourly_minute: Some(minute),
            cron: None,
            enabled: true,
            timeout: None,
        }
    }

    fn cron_task(expr: &str) -> AutonomyTaskConfig {
        AutonomyTaskConfig {
            id: "t".to_string(),
            name: "n".to_string(),
            prompt: "p".to_string(),
            subagent: None,
            interval: 30,
            schedule_mode: ScheduleMode::Cron,
            at_time: None,
            days: Vec::new(),
            hourly_minute: None,
            cron: Some(expr.to_string()),
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
            hourly_minute: None,
            cron: None,
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
            hourly_minute: None,
            cron: None,
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

    // 상수 불변식 회귀 — "따라잡기는 최대 1회" 경고를 코드로 고정한다.
    // Hourly 도입으로 가장 촘촘한 벽시계 모드의 최소 간격이 24시간(FixedTime)
    // 에서 60분(Hourly)으로 좁혀졌다(파일 상단 불변식 표 참조) — 이 테스트가
    // MISSED_RUN_GRACE_MINUTES를 60 이상으로 올리는 변경을 막는 방어선이다.
    #[test]
    fn missed_run_grace_is_shorter_than_minimum_occurrence_gap() {
        const MINIMUM_OCCURRENCE_GAP_MINUTES: u32 = 60;
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

    // =====================================================================
    // Hourly/Cron 모드(이번 라운드) — 설계 §9 T1~T15
    // =====================================================================

    // ---------------- next_hourly_in (T1~T4) ----------------

    // T1 — 정상 계산: 아직 이번 시 라벨의 분이 지나지 않았으면 오늘(이번 시).
    #[test]
    fn next_hourly_in_returns_this_hour_when_minute_not_yet_passed() {
        let after = kst_instant(2026, 9, 15, 8, 0);
        let result = next_hourly_in(&kst(), 30, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 15, 8, 30)));
    }

    // T2 — 이번 시 라벨이 이미 지났으면 다음 시로.
    #[test]
    fn next_hourly_in_moves_to_next_hour_when_minute_already_passed() {
        let after = kst_instant(2026, 9, 15, 8, 45);
        let result = next_hourly_in(&kst(), 30, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 15, 9, 30)));
    }

    // T3(최우선) — 엄격 `>` 회귀. 완료 직후 재계산이 같은 회차를 다시 잡아
    // 무한 재실행하는 것을 막는다(`next_occurrence_excludes_exact_same_instant`
    // 와 동일 취지).
    #[test]
    fn next_hourly_in_excludes_exact_same_instant() {
        let after = kst_instant(2026, 9, 15, 8, 30); // 정각
        let result = next_hourly_in(&kst(), 30, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 15, 9, 30)), "다음 시로 넘어가야 한다");
    }

    // T4 — 자정 넘김.
    #[test]
    fn next_hourly_in_crosses_midnight() {
        let after = kst_instant(2026, 9, 15, 23, 45);
        let result = next_hourly_in(&kst(), 30, after);
        assert_eq!(result, Some(kst_instant(2026, 9, 16, 0, 30)));
    }

    #[test]
    fn next_hourly_in_is_deterministic_under_fixed_offset_timezone() {
        let after = kst_instant(2026, 9, 15, 8, 0);
        let a = next_hourly_in(&kst(), 30, after);
        let b = next_hourly_in(&kst(), 30, after);
        assert_eq!(a, b);
    }

    // ---------------- Hourly DST(②) — chrono-tz 없이 결정적으로 재현 ----------------
    //
    // `FixedOffset`은 전환이 없어 `LocalResult::None`/`Ambiguous`를 자연
    // 발생시킬 수 없다(설계 §9의 정직한 한계). 아래 `SimulatedDstTz`는 지정된
    // 한 시간 구간에서만 봄 부재/가을 중복을 흉내 내는 최소 `TimeZone`
    // 구현이다 — `chrono-tz`를 dev-dependency로 추가하지 않고도
    // `next_hourly_in`을 실제 DST 분기(`resolve_local_naive`/`pick`)로
    // 통과시켜 값으로 검증한다. 오프셋 자체(+9)는 임의값이고 실제 KST가
    // DST를 쓴다는 뜻이 아니다 — 전환 유무만 흉내 낸다.
    #[derive(Clone, Copy)]
    struct SimulatedDstTz {
        /// 이 naive 시각부터 1시간(`start..start+1h`)은 봄 부재 — 존재하지
        /// 않는 벽시계로 취급한다.
        spring_gap_start: Option<NaiveDateTime>,
        /// 이 naive 시각부터 1시간은 가을 중복 — 이른/늦은 두 오프셋으로
        /// 해석 가능한 벽시계로 취급한다.
        fall_ambiguous_start: Option<NaiveDateTime>,
    }

    const SIM_TZ_OFFSET_SECS: i32 = 9 * 3600;

    impl TimeZone for SimulatedDstTz {
        type Offset = FixedOffset;

        fn from_offset(_offset: &FixedOffset) -> Self {
            // 이 경로는 실행된다 — `cron_parser::parse`는 내부에서
            // `dt.timezone()`으로 tz를 재구성하며, 그 결과가 시뮬레이션
            // 필드를 잃은 이 기본값이 된다(실측 확인). 즉 이 픽스처를
            // `next_cron_in`에 넘기면 DST 시뮬레이션이 조용히 사라진다 —
            // 이 픽스처로 Cron 경로의 DST를 검증할 수 없다는 뜻이다.
            // `tz`를 참조로 직접 쓰는 `next_hourly_in` 경로에서만 유효하다.
            SimulatedDstTz { spring_gap_start: None, fall_ambiguous_start: None }
        }

        fn offset_from_local_date(&self, _local: &NaiveDate) -> LocalResult<FixedOffset> {
            LocalResult::Single(FixedOffset::east_opt(SIM_TZ_OFFSET_SECS).unwrap())
        }

        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<FixedOffset> {
            let normal = FixedOffset::east_opt(SIM_TZ_OFFSET_SECS).unwrap();
            if let Some(gap) = self.spring_gap_start {
                if *local >= gap && *local < gap + Duration::hours(1) {
                    return LocalResult::None;
                }
            }
            if let Some(amb) = self.fall_ambiguous_start {
                if *local >= amb && *local < amb + Duration::hours(1) {
                    let earlier = normal;
                    let later = FixedOffset::east_opt(SIM_TZ_OFFSET_SECS - 3600).unwrap();
                    return LocalResult::Ambiguous(earlier, later);
                }
            }
            LocalResult::Single(normal)
        }

        fn offset_from_utc_date(&self, _utc: &NaiveDate) -> FixedOffset {
            FixedOffset::east_opt(SIM_TZ_OFFSET_SECS).unwrap()
        }

        fn offset_from_utc_datetime(&self, _utc: &NaiveDateTime) -> FixedOffset {
            FixedOffset::east_opt(SIM_TZ_OFFSET_SECS).unwrap()
        }
    }

    // ② Hourly 봄 DST — `minute=30`인데 그 시각(예: 02:30)이 통째로 존재하지
    // 않으면(`spring_gap_start`), `pick()`의 "+1시간" fallback이 다음 라벨의
    // 인스턴트를 그대로 돌려준다(파일 상단 불변식 표 — Hourly는 "밀어서
    // 실행"이 정답이다: 03:30은 이 표현이 어차피 실행하기로 약속한 시각).
    #[test]
    fn next_hourly_in_shifts_forward_one_hour_on_spring_dst_gap() {
        let tz = SimulatedDstTz {
            spring_gap_start: Some(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap().and_hms_opt(2, 0, 0).unwrap()),
            fall_ambiguous_start: None,
        };
        let after = kst_instant(2026, 3, 8, 1, 45); // 부재 구간(02:00~03:00) 이전
        let result = next_hourly_in(&tz, 30, after);
        assert_eq!(
            result,
            Some(kst_instant(2026, 3, 8, 3, 30)),
            "02:30 라벨이 통째로 없으므로 03:30(다음 라벨의 인스턴트)으로 밀려야 한다"
        );
    }

    // ② Hourly 가을 DST — `minute=30`인데 그 시각이 중복되면(`fall_ambiguous_start`),
    // `pick()`이 이른 쪽 오프셋 1회만 택해 그 라벨이 두 번 실행되지 않는다.
    #[test]
    fn next_hourly_in_picks_earlier_instant_on_fall_dst_ambiguity() {
        let tz = SimulatedDstTz {
            spring_gap_start: None,
            fall_ambiguous_start: Some(
                NaiveDate::from_ymd_opt(2026, 11, 1).unwrap().and_hms_opt(1, 0, 0).unwrap(),
            ),
        };
        let after = kst_instant(2026, 11, 1, 0, 45); // 중복 구간(01:00~02:00) 이전
        let result = next_hourly_in(&tz, 30, after);
        assert_eq!(
            result,
            Some(kst_instant(2026, 11, 1, 1, 30)),
            "01:30 라벨은 이른 쪽 오프셋 1회만 실행돼야 한다"
        );
    }

    // ---------------- initial_next_run_at / next_run_after_finish(Hourly, T5·T6) ----------------

    // T5 — 놓친 회차 따라잡기: grace(30분) 창 안이면 오늘(이번 시) 회차를
    // 따라잡는다. `chrono::Local::now()` 기준 동적 값을 써서 CI 러너 tz와
    // 무관하게 항상 맞도록 한다(기존 FixedTime 테스트와 동일 관용구).
    #[test]
    fn initial_next_run_at_for_hourly_catches_up_run_missed_within_grace() {
        let now = Local::now();
        let missed = now - Duration::minutes(5); // grace(30분) 안
        let task = hourly_task(missed.minute() as u8);
        let now_utc = now.with_timezone(&Utc);
        let result = initial_next_run_at(&task, now_utc);
        let floor = now_utc + Duration::minutes(STARTUP_GRACE_MINUTES as i64);
        assert_eq!(
            result,
            Some(floor),
            "grace 창 안의 회차는 STARTUP_GRACE 시점에 맞춰 즉시 따라잡아야 한다"
        );
    }

    // T6 — next_run_after_finish의 Hourly 분기 **프로덕션 사슬**
    // (`ScheduleSnapshot → next_hourly → Local`) 직접 커버리지. 이 필드를
    // 스냅숏에 빠뜨리면 이 테스트가 `unwrap()`에서 패닉한다.
    #[test]
    fn next_run_after_finish_for_hourly_mode_returns_next_hour_occurrence() {
        let finished_at = Local::now().with_timezone(&Utc);
        let minute = finished_at.with_timezone(&Local).minute() as u8;
        let schedule = ScheduleSnapshot {
            mode: ScheduleMode::Hourly,
            interval_minutes: 30,
            fixed: None,
            hourly_minute: Some(minute),
            cron: None,
        };
        let result = next_run_after_finish(&schedule, finished_at)
            .expect("Hourly 스냅숏이면 반드시 다음 회차를 반환해야 한다(T7 취지)");
        assert!(result > finished_at, "완료 시각과 같은 인스턴트를 다시 고르면 안 된다(무한 재실행 방지)");
        assert!(result <= finished_at + Duration::hours(1), "매시 반복이면 다음 회차는 1시간 이내여야 한다");
    }

    // ---------------- next_cron_in (T8) ----------------

    // T8 — 정상 해석.
    #[test]
    fn next_cron_in_computes_normal_next_occurrence_with_fixed_offset() {
        let after = kst_instant(2026, 9, 15, 8, 0);
        let result = next_cron_in(&kst(), "0 9 * * *", after);
        assert_eq!(result, Some(kst_instant(2026, 9, 15, 9, 0)));
    }

    // T8 — 엄격 `>`(정각 입력 → 다음 회차).
    #[test]
    fn next_cron_in_excludes_exact_same_instant() {
        let after = kst_instant(2026, 9, 15, 9, 0); // 정각
        let result = next_cron_in(&kst(), "0 9 * * *", after);
        assert_eq!(result, Some(kst_instant(2026, 9, 16, 9, 0)));
    }

    // T8 — 무효 표현식 → `None`(패닉 없음).
    #[test]
    fn next_cron_in_returns_none_for_invalid_expression_without_panicking() {
        let after = kst_instant(2026, 9, 15, 8, 0);
        assert_eq!(next_cron_in(&kst(), "not a cron", after), None);
        assert_eq!(
            next_cron_in(&kst(), "0 9 * * 7", after),
            None,
            "요일 7(일요일 별칭)은 이 crate가 지원하지 않는다"
        );
    }

    // ---------------- initial_next_run_at / next_run_after_finish(Cron, T7·T9) ----------------

    // T7 — next_run_after_finish의 Cron 분기 프로덕션 사슬 커버리지.
    #[test]
    fn next_run_after_finish_for_cron_mode_returns_next_occurrence_after_finish() {
        let finished_at = Utc::now();
        let schedule = ScheduleSnapshot {
            mode: ScheduleMode::Cron,
            interval_minutes: 30,
            fixed: None,
            hourly_minute: None,
            cron: Some("*/5 * * * *".to_string()),
        };
        let result = next_run_after_finish(&schedule, finished_at)
            .expect("Cron 스냅숏이면 반드시 다음 회차를 반환해야 한다(T7 취지)");
        assert!(result > finished_at);
        assert!(result <= finished_at + Duration::minutes(5));
    }

    // T9 — §4.5 불변식: `*/5 * * * *`가 grace 창을 여러 번 지나쳤어도 등록
    // 시 반환값은 "회차 목록"이 아니라 `startup_floor` 한 점이다.
    #[test]
    fn initial_next_run_at_for_frequent_cron_still_collapses_to_single_startup_floor() {
        let now = Utc::now();
        let task = cron_task("*/5 * * * *");
        let result = initial_next_run_at(&task, now).unwrap();
        let floor = now + Duration::minutes(STARTUP_GRACE_MINUTES as i64);
        assert_eq!(
            result, floor,
            "여러 회차를 지나쳤어도 등록 시 반환값은 startup_floor 한 점이어야 한다"
        );
    }

    // ---------------- advance_until_strictly_after (T10, ④ Cron DST 되감기 단조성) ----------------

    // T10(a) — 첫 후보가 `after` 이하(가을 되감기에서 실재하는 경로, §5.4)면
    // 앵커를 밀어 두 번째 값을 반환한다.
    #[test]
    fn advance_until_strictly_after_pushes_anchor_forward_when_first_candidate_is_stale() {
        let after = kst_instant(2026, 11, 1, 1, 0);
        let stale = kst_instant(2026, 11, 1, 0, 30); // after 이하(되감기 경로 재현)
        let fresh = kst_instant(2026, 11, 1, 2, 30); // after보다 이후
        let mut calls = 0;
        let result = advance_until_strictly_after(
            |_anchor| {
                calls += 1;
                Some(if calls == 1 { stale } else { fresh })
            },
            after,
        );
        assert_eq!(result, Some(fresh));
        assert_eq!(calls, 2, "첫 후보가 거부되면 앵커를 밀어 두 번째를 시도해야 한다");
    }

    // T10(b) — 계속 과거만 주는 스텁 → 3회 재시도 후 `None`(fail-closed,
    // 무한루프 없음).
    #[test]
    fn advance_until_strictly_after_gives_up_after_four_tries_without_looping_forever() {
        let after = Utc::now();
        let always_stale = after - Duration::minutes(1);
        let mut calls = 0;
        let result = advance_until_strictly_after(
            |_anchor| {
                calls += 1;
                Some(always_stale)
            },
            after,
        );
        assert_eq!(result, None, "계속 과거만 준다면 fail-closed로 None을 반환해야 한다");
        assert_eq!(calls, 4, "무한루프 없이 정확히 4회(최초+3회) 시도 후 포기해야 한다");
    }

    // ---------------- validate_cron (T15) ----------------

    #[test]
    fn validate_cron_accepts_well_formed_expression() {
        assert!(validate_cron("0 9 * * 1-5").is_ok());
    }

    #[test]
    fn validate_cron_rejects_unparseable_expression_with_korean_message() {
        let err = validate_cron("not a cron").unwrap_err();
        assert!(err.contains("cron 표현식이 올바르지 않습니다"), "실제 문구: {err}");
    }

    #[test]
    fn validate_cron_rejects_six_field_expression() {
        let err = validate_cron("0 0 9 * * *").unwrap_err();
        assert!(err.contains("5칸"), "실제 문구: {err}");
    }

    #[test]
    fn validate_cron_rejects_dow_seven_with_sunday_hint() {
        // 이 crate는 요일을 0~6만 받는다(7=일요일 별칭 미지원) — 가장 흔한
        // 실수이므로 에러 문구에 "일요일은 0"이라는 안내가 있어야 한다.
        let err = validate_cron("0 0 * * 7").unwrap_err();
        assert!(err.contains("0(일)"), "실제 문구: {err}");
    }

    // T15 — 과잉 실행 브레이크: `* * * * *`(매분)는 거부되고, 문구에
    // `MIN_INTERVAL_MINUTES` 값이 상수에서 그대로 나와야 한다(하드코딩
    // 금지 — 값을 바꿀 때 고칠 파일이 1개가 되도록).
    #[test]
    fn validate_cron_rejects_too_frequent_expression_mentioning_min_interval_minutes() {
        let err = validate_cron("* * * * *").unwrap_err();
        assert!(
            err.contains(&MIN_INTERVAL_MINUTES.to_string()),
            "에러 문구에 MIN_INTERVAL_MINUTES 값이 박혀 나와야 한다: {err}"
        );
    }

    // ---------------- 보안 가드(⑤) — 느린 경로를 parse() 호출 전에 차단 ----------------
    //
    // `", * * * *"`류(필드가 빈 집합이 되는 입력)는 cron-parser 0.11.2에서
    // 실측 최악 1.7초가 걸리는 경로다(빈 `BTreeSet`이 `Ok`로 반환돼 4년치를
    // 매 반복 재파싱). 아래 두 테스트가 몇 초씩 걸리면 가드가 parse() 호출을
    // 막지 못하고 있다는 신호다 — 두 진입점(L1 `validate_cron`, 계산 경로
    // `next_cron_in`) 모두에서 빠르게 거부돼야 한다(보안 조건 1·2).
    #[test]
    fn validate_cron_rejects_empty_field_set_input_quickly() {
        let start = std::time::Instant::now();
        let result = validate_cron(", * * * *");
        let elapsed = start.elapsed();
        assert!(result.is_err());
        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "형태 가드가 parse() 호출을 막지 못하면 이 입력은 초 단위로 느려진다: {elapsed:?}"
        );
    }

    #[test]
    fn next_cron_in_rejects_empty_field_set_input_quickly() {
        let start = std::time::Instant::now();
        let result = next_cron_in(&kst(), ", * * * *", kst_instant(2026, 9, 15, 8, 0));
        let elapsed = start.elapsed();
        assert!(result.is_none());
        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "계산 경로에도 같은 가드가 없으면 손편집 파일이 스케줄러 tick을 지연시킨다: {elapsed:?}"
        );
    }

    #[test]
    fn cron_shape_is_safe_rejects_wrong_field_count_and_overlong_input() {
        assert!(!cron_shape_is_safe("* * * *")); // 4필드
        assert!(!cron_shape_is_safe("* * * * * *")); // 6필드
        assert!(!cron_shape_is_safe(&"*".repeat(200))); // 길이 상한 초과
        assert!(cron_shape_is_safe("0 9 * * 1-5"));
    }

    // ---------------- normalize_cron_expr(L2) ----------------

    #[test]
    fn normalize_cron_expr_collapses_whitespace_and_confirms_parseability() {
        assert_eq!(
            normalize_cron_expr("0   9  * * 1-5"),
            Some("0 9 * * 1-5".to_string())
        );
        assert_eq!(normalize_cron_expr("not a cron"), None);
        assert_eq!(normalize_cron_expr(", * * * *"), None, "형태 가드를 통과하지 못해야 한다");
    }
}
