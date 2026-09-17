# 자율업무 스케줄 UI 2단 계층화 — `Hourly` + `Cron` 설계 (개정판)

대상 워크트리: `/Users/hopegiver/workspace/malgn-vscode-cron` (브랜치 `feat/autonomy-schedule-tabs`, 기점 `434b6e7`)
현행 모드: `Interval`(완료 후 N분) / `FixedTime`(HH:MM + 요일)
추가 모드: **`Hourly`(매시 M분)** / **`Cron`(표준 5필드 `분 시 일 월 요일`)** — 정확히 2개
작업 등급: **Sensitive** — 사용자 승인 없이 `claude -p` 자식 프로세스를 반복 spawn하는 로컬 자동실행 스케줄러(토큰 비용·중복 부작용)
이 문서의 성격: **설계 확정서**. 구현 코드는 포함하지 않는다(시그니처·라인 지정까지만).

> **개정 사유**: 초판은 평평한 모드 확장(`Manual` + `Cron`)을 전제했다. 확정 스펙은 **2단 계층 UI**(최상위 라디오 2개 × 고정시간 하위 탭 4개)이고 `Manual`은 도입하지 않는다(기존 `enabled` 토글이 이미 그 역할). 초판의 `Manual` 관련 설계(§4 전체, `scheduler::tick` 가드, 3경로 `None` 표)는 **전부 폐기**했다. 유지한 것: cron crate 선정 근거(§3), DST 단조성 가드(§5.4), serde 하위호환 리스크(§8.3 — 흡수 기본값만 재결정).

---

## 0. 결정 요약 (한 줄씩)

| # | 결정 | 값 |
|---|---|---|
| A | UI↔백엔드 매핑 | 1단 라디오 2개(상대간격/고정시간) × 고정시간 하위 탭 4개 → 백엔드 모드는 **4개**(`Interval`/`Hourly`/`FixedTime`/`Cron`). 매일·매주 두 탭이 `FixedTime` 하나를 공유한다(`days` 빈 배열 여부로 구분) — §2 |
| B | `Hourly` 계산 | `schedule.rs`에 `next_hourly_in<Tz>(tz, minute, after)` 신설. `pick`/`resolve_local_instant`를 **그대로 재사용**하고 루프만 "날짜 단위"에서 "로컬 시(hour) 라벨 단위"로 축소. 신규 의존성 0 — §4 |
| B-2 | `Hourly` DST | 봄 부재 → `pick`의 `+1시간` 규칙 그대로. **Hourly에서는 이 규칙이 '수용'이 아니라 정확히 맞다**(밀린 인스턴트가 바로 다음 시 슬롯과 일치하므로 사용자가 지정하지 않은 분에 실행되지 않는다). 가을 모호 → 이른 쪽 1회. 전환일 실행 횟수: 봄 23회 / 가을 24회 — §4.3 |
| B-3 | `Hourly` 단조성 | 별도 가드 불필요. `next_occurrence_in`과 **동일한 `instant > after` 필터**(schedule.rs:127)가 정방향 라벨 루프 안에서 되감기 구간을 흡수한다(증명 §4.4) |
| C | cron crate | **`cron-parser = "0.11"`**(초판 유지). 전이 의존성 0개, 5필드 전용, `parse<TZ: TimeZone>` 제네릭. `saffron` 탈락: 숫자 DOW가 Quartz 1=일이라 `0 9 * * 1-5`가 일~목으로 **조용히** 오해석 — §3 |
| C-2 | `Cron` DST | 봄 부재 → crate가 그날 회차를 건너뛴다(FixedTime/Hourly의 `+1시간`과 **다름, 의도된 비대칭**). 가을 모호 → crate도 이른 쪽. 되감기 구간에서 crate가 `after` 이전 인스턴트를 반환하는 경로가 실재 → `advance_until_strictly_after` 가드 필수 — §5.3·§5.4 |
| D | 신규 필드 | `hourly_minute: Option<u8>` / `cron: Option<String>` — 둘 다 `#[serde(default, skip_serializing_if="Option::is_none")]`. 기존 `interval`/`at_time`/`days` **재사용하지 않음** — §8.2 |
| E | 화이트리스트 3지점 | `mod.rs:63`·`mod.rs:195`는 **버그(조용히 재스케줄 누락)** → 공용 술어 `ScheduleMode::is_wall_clock()`(exhaustive `match` 1곳)로 교체. `config.rs:81`은 **부등호라 신규 모드에 이미 올바름**(고칠 것 없음, 주석만) — §6.2 |
| F | exhaustive match 3곳 | `schedule.rs:153`(앱시작 floor) / `:191`(편집·재개) / `:206`(완료 후, **`ScheduleSnapshot` 기준**) — 세 곳의 앵커가 서로 다르다 — §6.1 |
| G | 검증 위치 | `autonomy_save_task`에서 **저장 전 거부**(`Err(한국어)` → 토스트) + `normalize_task`에서 fail-closed 폐기 + 계산 시점 `None`. **신규 Tauri 커맨드 0개, `capabilities/default.json` 무변경** — §7 |
| H | serde 하위호환 | 미지 `scheduleMode` 문자열을 **`Cron`(표현식 없음)으로 흡수**하는 수동 `Deserialize`. `Interval` 흡수는 절대 금지(최대 5분마다 무인 실행). 흡수 대상을 `Cron`으로 고른 결정적 이유는 §8.3-3 |
| I | 무변경 확정 | `Interval`/`FixedTime`의 스키마·계산 로직, `scheduler.rs`, `runner.rs`, `runtime.rs`, `capabilities/default.json` — **한 줄도 바뀌지 않는다** |

### 지시 항목 ↔ 섹션 대응

| 요구 | 섹션 |
|---|---|
| ① `Hourly` 다음 실행 계산 + DST를 FixedTime 비대칭 원칙과 일관되게 | §4 전체 (특히 §4.3·§4.4) |
| ② 화이트리스트 3지점(`mod.rs:63`·`:195`, `config.rs:81`) 지점별 결론 + match 전환 권고 | §6.2 |
| ③ exhaustive match 3곳(`schedule.rs:153`·`:191`·`:207`) 지점별 반환값 | §6.1 |
| ④ cron 표현식 유효성 검증 위치 + 프론트 에러 형태 | §7 |
| ⑤ `Hourly.minute`·`Cron` 표현식의 필드 결정 + serde 기본값·하위호환 | §8 |
| 프론트 데이터 계약(탭↔모드 매핑, 검증 에러 형태) | §2.2 · §7.4 |

---

## 1. 실측 근거

초판이 이 머신에서 실제로 실행해 채집한 값을 유지한다(추정치 없음). 이번 개정에서 **`cargo add cron-parser@0.11 --dry-run`을 재실행해 크레이트·버전이 여전히 해석되는 것을 재확인**했다(`Adding cron-parser v0.11 to dependencies`, 실제 추가는 하지 않음 — 지시서 금지 범위 준수).

```
[cargo add — 신규 잠금 패키지 수]
cron 0.17.0        : 10개 (cron, memchr, phf, phf_generator, phf_macros, phf_shared, rand, rand_core, siphasher, winnow)
saffron 0.1.0      :  4개 (saffron, nom 5.1.3, memchr, version_check)
croner 4.0.0       : 13개 (croner, darling×3, derive_builder×3, fnv, heck, ident_case, strsim, strum, strum_macros)
cron-parser 0.11.2 :  1개 (cron-parser 자신뿐 — 전이 의존성 0)

[cron-parser 0.11.2 동작 실측]
"0 9 * * *"    @2026-09-15 08:00 KST -> 2026-09-15 09:00 KST      (5필드 OK)
"0 0 9 * * *"  -> ERR(InvalidCron)                                 (6필드 거부)
"0 9 * * *"    @2026-09-15 09:00:00 정각 -> 2026-09-16 09:00       (엄격 > 확인)
"30 2 * * *"   @2026-03-08 00:30 PST(봄 DST) -> 2026-03-09 02:30 PDT (그날 건너뜀)
"30 1 * * *"   @2026-11-01 00:30 PDT(가을 DST) -> 2026-11-01 01:30 PDT (이른 쪽)
"0 * * * *"    @2026-03-08 00:30 PST(봄 DST) -> 2026-03-08 01:00 PST (매시간식은 정상 진행)
"0 9 * * 7"    -> ERR(InvalidValue) / "0 9 * * MON" -> OK / "0 9 * JAN *" -> ERR
"@daily"       -> ERR(InvalidCron)
비용: 일반식 1.46µs/호출, 최악 관측 64µs/호출 (release, 10,000회 평균)

[saffron 0.1.0 — 탈락 결정타]
"0 9 * * 1-5" -> 화,수,목,일,월  (숫자 DOW가 Quartz 1=SUN..7=SAT)
"0 9 * * 0"   -> 파싱 실패
c.next_after(DateTime<FixedOffset>) -> E0308 expected `DateTime<Utc>`  (UTC 전용 확정)

[cron 0.17.0 — 탈락 결정타]
"0 9 * * *" -> parse ERR (초 필드 필수: 6~7필드)

[serde — 다운그레이드 위험 실측]
현행 enum에 없는 "hourly" 값이 tasks 중 1건에만 있어도
  serde_json::from_str::<AutonomyFile> -> Err("unknown variant `hourly`")
  → read_autonomy_file(config.rs:173)의 unwrap_or_default() → tasks = 0
  → 그 상태에서 저장하면 write_autonomy_file이 **나머지 task를 전부 지운 파일**을 쓴다
```

**코드 근거 grep(이번 개정에서 직접 재실행)**

```
grep -rn "ScheduleMode"  src-tauri/src --include='*.rs'   # 비-테스트 참조는 config/schedule/mod 3개 파일뿐
grep -rn "schedule_mode" src-tauri/src --include='*.rs'   # ==/!= 비교는 비-테스트 코드에 정확히 3곳
```
→ `scheduler.rs`·`runner.rs`·`runtime.rs`는 `ScheduleMode`라는 이름을 **전혀 모른다**. 이 절약(리뷰 보고서 §잘된점 1)이 이번 개정에서도 **한 글자도 훼손되지 않는다** — `Manual`을 도입하지 않게 되면서 초판이 유일하게 뚫으려 했던 `scheduler::tick` 가드가 통째로 사라졌기 때문이다.

---

## 2. 결정 A — UI 2단 계층 ↔ 백엔드 모드 매핑 (정본)

### 2.1 구조

```
[실행 방식]  ( ) 상대 간격            ← 기존 라디오, 지금 구현 그대로 무변경
             (•) 고정 시간
                 ┌──────┬──────┬──────┬────────────┐
                 │ 매시간 │ 매일  │ 매주  │ 사용자 지정 │   ← 2단 하위 탭(신규)
                 └──────┴──────┴──────┴────────────┘
```

### 2.2 매핑표 (백엔드가 프론트에 보장하는 데이터 계약)

| 1단 | 2단 탭 | 사용자 입력 | `scheduleMode` | 실어 보내는 필드 | 백엔드 변경 |
|---|---|---|---|---|---|
| 상대간격 | — | `INTERVAL_OPTIONS` 드롭다운 | `"interval"` | `interval: N` | **없음** |
| 고정시간 | 매시간 | 분 0~59 | `"hourly"` | `hourlyMinute: M` | **신규 variant + 필드 1개** |
| 고정시간 | 매일 | 시+분 | `"fixedTime"` | `atTime: "HH:MM"`, `days: []` | **없음**(빈 배열=매일, `day_allowed` schedule.rs:78-80이 이미 그렇게 정의) |
| 고정시간 | 매주 | 시+분+요일 다중 | `"fixedTime"` | `atTime: "HH:MM"`, `days: [선택]` | **없음** |
| 고정시간 | 사용자 지정 | cron 표현식 원문 | `"cron"` | `cron: "0 9 * * 1-5"` | **신규 variant + 필드 1개 + crate** |

### 2.3 역매핑(편집 모달을 열 때 어느 탭을 선택할 것인가)

모드→탭이 1:1이 아닌 곳은 `fixedTime` 하나뿐이다. 판정 규칙을 값으로 확정한다:

| 저장된 값 | 선택할 탭 |
|---|---|
| `mode==='interval'` | 1단 = 상대간격 |
| `mode==='hourly'` | 고정시간 → 매시간 |
| `mode==='fixedTime'` && (`days.length===0` \|\| `days.length===7`) | 고정시간 → **매일** |
| `mode==='fixedTime'` && `0 < days.length < 7` | 고정시간 → **매주** |
| `mode==='cron'` | 고정시간 → 사용자 지정 |

- `days.length===7`을 매일로 접는 이유: **현행 FE가 "매일"을 `[0..6]` 7개로 저장한다**(`autonomousTasks.ts:801` `WEEKDAY_PRESET_DAILY`, `:934`가 `days.length===0`을 거부). 이번 스펙은 매일을 `[]`로 저장하므로, 기존 파일이 매주 탭으로 잘못 열리는 것을 막아야 한다. 백엔드 의미는 **완전히 동일**하고(`day_allowed`: `days.is_empty() || contains`), 기존 라벨 함수도 이미 둘을 같게 접는다(`computeFixedTimeScheduleLabel`, `autonomousTasks.ts:145-148` — `days.length === 0 || days.length >= 7`). 즉 기존 데이터가 다음 저장 때 `[0..6]`→`[]`로 조용히 바뀌지만 **관측 가능한 동작 변화는 0**이다.
- 백엔드에서 `days.len()==7`을 `[]`로 정규화하지 **않는다**: 정규화로 접으면 이득(파일 정본화)보다 "기존 파일이 읽는 것만으로 내용이 바뀐다"는 놀라움이 크고, 두 표현이 동치라 실익이 없다. FE 판정만으로 충분하다.
- FE 제출 검증 이동(필수): 현행 `autonomousTasks.ts:934`의 `days.length === 0` 거부는 **매주 탭 전용**이 된다. 매일 탭은 `days: []`가 정상값이므로 그 검증을 그대로 두면 매일 탭이 저장 불가가 된다.

### 2.4 `Hourly`를 별도 variant로 두는 결정 (①트레이드오프)

- **선택**: `ScheduleMode::Hourly` + `hourly_minute: Option<u8>` (신규 의존성 0)
- **검토한 대안**: 매시간 탭을 UI 설탕으로만 두고 내부적으로 `cron = "M * * * *"`를 저장해 **신규 variant를 `Cron` 하나로 줄이기**
- **선택 이유**
  1. **의존성 격리**: 매시간은 이 앱에서 가장 흔한 요구인데, 대안은 그 기능을 서드파티 파서에 묶는다. cron-parser를 훗날 vendoring/교체하더라도 매시간은 `chrono`만으로 계속 돈다.
  2. **역매핑이 무손실**: 대안에서는 저장된 `cron` 문자열을 정규식으로 되파싱(`^(\d{1,2}) \* \* \* \*$`)해야 매시간 탭을 복원할 수 있고, `0 */1 * * *`처럼 의미는 같고 형태가 다른 표현식에서 탭 복원이 어긋난다. §2.3 표가 단순한 이유가 이것이다.
  3. **DST 규칙을 우리 것으로 유지**: cron 경로는 crate의 "봄 부재 = 건너뜀" 규칙을 상속받지만(§5.3), 매시간은 `pick()`의 `+1시간` 규칙이 **더 맞다**(§4.3). 대안을 택하면 매시간이 crate 규칙으로 끌려간다.
  4. **검증 표면이 없다**: 0~59 정수는 "형식 오류"라는 개념이 없어 사용자에게 보여줄 파싱 에러 문구가 필요 없다. 대안은 숫자 하나 입력에도 cron 검증 경로를 태운다.
- **포기한 것**: variant가 3개→4개가 된다. `schedule.rs`의 match 3곳에 팔이 1개씩 더 생기고, `next_hourly_in` 함수 1개(~20줄)와 테스트가 늘어난다.
- **감당 방안**: 늘어난 팔은 전부 **컴파일러가 강제**하는 자리다(§6.1). 새 함수는 기존 `pick`/`resolve_local_instant`를 재사용해 DST 규칙 코드를 복제하지 않는다 — 실제 신규 로직은 루프 한 개다.

---

## 3. 결정 C — cron crate 선정 (초판 유지, ①트레이드오프)

이 프로젝트의 Cargo 의존성 추가는 **이번이 첫 건**이다. 그래서 "가볍고 감사 가능"에 가중치를 둔다.

### 3.1 비교표 (전부 실측)

| 축 | **cron-parser 0.11.2** ✅ | cron 0.17.0 | saffron 0.1.0 | croner 4.0.0 |
|---|---|---|---|---|
| 5필드 `분 시 일 월 요일` | **정확히 5필드만 수용**(4/6필드는 `InvalidCron`) | ❌ 초 필수(6~7필드) | 5필드 ✅ | 5~7필드(옵션) |
| 숫자 DOW 의미 | 0=일…6=토 (표준) | 표준 계열 | ❌ **Quartz 1=일…7=토**, `0`은 파싱 실패 | 설정 가능 |
| chrono `DateTime<Tz>` 제네릭 | ✅ `parse<TZ: TimeZone>(&str, &DateTime<TZ>) -> Result<DateTime<TZ>,_>` | ✅ | ❌ **UTC 전용**(컴파일 에러로 확인) | 자체 트레이트 경유 |
| 신규 전이 의존성 | **0개** | 10개(phf proc-macro·rand·winnow) | 4개(nom 5.1.3 — 2019년 계열) | 13개(darling·derive_builder·strum) |
| chrono 피처 확장 | 없음(`default-features=false`, 피처 0) | `clock` 요구 | `alloc` | optional chrono |
| 최신 릴리스 | 2025-12-17 | 2026-06-18 | **2021-02-01 (5년 무릴리스)** | 2026-08-31 |
| 코드 규모(감사 가능성) | **lib.rs 1파일 520줄** | 2,702줄 | 4,576줄 | 7,066줄 |
| `unsafe` / build.rs / proc-macro | 0 / 없음 / 없음 | 0 / 없음 / proc-macro | 0 / 없음 / 없음 | 0 / 없음 / proc-macro 2종 |
| edition / MSRV 영향 | edition **2024**(≥1.85) | 2021 | 2018 | 2021 |
| 라이선스 | BSD-3-Clause | MIT OR Apache-2.0 | BSD-3(Cloudflare) | MIT |

### 3.2 선정: `cron-parser`

- **선택**: `cron-parser = "0.11"`
- **검토한 대안**: `cron`(최다 다운로드), `saffron`(Cloudflare Workers Cron Triggers 실전 검증), `croner`(최다 기능)
- **선택 이유**
  1. **요구사항이 정확히 "표준 5필드"다.** cron-parser는 5필드 외를 전부 거부하므로 사용자 입력 형태와 파서 계약이 1:1이다. `cron`을 쓰면 `"0 " + expr` 어댑터가 필요하고, 사용자가 6필드를 붙여넣으면 첫 칸이 조용히 "초"로 해석되는 함정이 생긴다.
  2. **saffron은 겉모습이 같고 의미가 다르다** — `0 9 * * 1-5`가 월~금이 아니라 일~목이 되고 `0`은 파싱 실패다. "표준 cron 표현식"이라고 UI에 적어 놓고 다른 스케줄을 도는 것은 무인 실행 기능에서 가장 피해야 할 오류다. 게다가 API가 UTC 전용이라 로컬 벽시계 책임이 통째로 우리에게 남는다(§5.2의 경계 결정이 불가능해진다).
  3. **의존성 0.** 첫 의존성 추가로 proc-macro 사슬 10~13개를 끌고 오는 것과 파일 하나(520줄)를 끌고 오는 것의 차이다. 520줄은 이번 설계 중 **전문을 읽었다**(파싱·루프·DST 해석 전부 §5에 인용).
  4. **chrono 제네릭이 기존 `schedule.rs` 관례와 그대로 맞물린다**(§5.2).
- **포기한 것**
  - `@daily`·`@hourly` 매크로(미지원), 월 이름 `JAN`(미지원 — 요일 이름 `MON`은 지원), `7`=일요일 별칭(미지원, `0`만).
  - dom·dow 동시 지정 시 **AND**(Vixie cron은 OR). 실측: `0 9 1 * 1` → 2027-02-01(1일이면서 월요일인 첫날).
  - 다운로드 수 1위가 주는 "군중 안심"(cron 2,338만 vs cron-parser 158만).
- **감당 방안**: 위 미지원 문법은 전부 **파싱 실패(명시적 에러)**로 나오지 조용한 오해석이 아니다 → §7의 저장 시점 거부가 그대로 사용자에게 보인다. AND 의미와 5칸 규칙은 도움말 문구로 고지한다(§7.4). 크레이트가 방치되면 520줄 단일 파일 + 전이 의존성 0이라 vendoring 비용이 가장 낮은 후보이기도 하다.

### 3.3 Cargo.toml에 들어갈 정확한 라인

`src-tauri/Cargo.toml`의 `[dependencies]` 마지막(`dunce = "1"`, 45행) 다음:

```toml
# 자율업무 Cron 모드 — 표준 5필드(분 시 일 월 요일) 전용 파서.
# 이 저장소의 첫 Cargo 의존성 추가라 "가볍고 감사 가능"을 선정 기준으로 삼았다:
# 전이 의존성 0개(cargo add 실측 — 잠금에 추가되는 패키지가 자기 자신 1개뿐),
# lib.rs 단일 파일 520줄, unsafe·build.rs·proc-macro 없음, chrono 피처도 넓히지
# 않는다(default-features=false, 피처 0 요구).
# `parse::<TZ: TimeZone>(&str, &DateTime<TZ>) -> Result<DateTime<TZ>, ParseError>`
# 라서 schedule.rs의 `Tz: TimeZone` 제네릭 관례를 그대로 잇는다(설계 §5.2).
# 매시간(Hourly) 모드는 이 크레이트를 쓰지 않는다 — chrono만으로 계산한다(설계 §4).
cron-parser = "0.11"
```

같은 파일 11행의 MSRV:

```toml
# (기존 BatBadBut 주석은 그대로 두고 값만 올린다 — 1.85는 1.77.2/1.81 완화를
# 모두 포함하므로 위 주석의 전제는 깨지지 않는다.)
# 추가 사유: cron-parser 0.11이 edition 2024라 컴파일러 1.85 이상을 요구한다.
rust-version = "1.85"
```

- **버전 지정 방식**: `"0.11"`(= `^0.11`). 0.x 캐럿이라 breaking change는 자동으로 들어오지 않고 패치는 받는다. `=0.11.2` 완전 고정은 하지 않는다 — 기존 19개 의존성이 전부 느슨한 캐럿이고 `Cargo.lock`이 커밋돼 있어 실제 빌드는 이미 고정돼 있다.
- **MSRV를 건드리기 싫다면**: `cron-parser = "0.10"`(edition 2021 → 1.81에서 컴파일). 0.10.0 ↔ 0.11.2 소스를 `diff -u`로 대조한 결과 **의미 변경 0**(필드 개수 검사를 배열 패턴 let-else로 바꾸고 `with_ymd_and_hms` match를 헬퍼로 뽑아낸 순수 리팩터)이다. 다만 0.10 라인은 후속 패치가 없으므로 **권고는 0.11 + MSRV 1.85**다. CI는 `toolchain: stable`(ci.yml:83-85, tauri-portable-build.yml:80-82)이고 1.81을 강제하는 잡이 없어 실제 파손 위험은 없다.
- BSD-3-Clause는 바이너리 배포 시 저작권 고지 재현을 요구한다. 이 저장소는 public이고 GitHub Actions로 바이너리를 낸다 — 배포물 라이선스 고지 절차가 현재 없다면 **별도 이슈로 기록**한다(이번 스코프 밖, 신규 위험은 아니지만 첫 서드파티 라이선스 편입이라 한 번은 정해야 한다).

---

## 4. 결정 B — `Hourly`의 다음 실행 계산 (지시 ①)

### 4.1 기존 daily 계산을 먼저 읽는다

`next_occurrence_in`(schedule.rs:117-135)의 실제 구조는 다음 4요소다:

1. `after.with_timezone(tz)`로 **로컬 달력 공간**으로 넘어간다(:122-123).
2. **날짜를 하나씩 전진**시키며(`date.succ_opt()`, :132) 허용 요일만 본다(:125).
3. 각 후보 날짜의 벽시계를 `resolve_local_instant` → `pick`으로 **인스턴트로 확정**한다(:126). 여기에만 DST 규칙이 있다(봄 부재=+1시간, 가을 모호=이른 쪽).
4. **`instant > after` 엄격 비교**(:127)를 통과한 첫 값만 반환한다. 이것이 "완료 직후 재계산이 같은 회차를 다시 잡아 무한 재실행하는 것"을 막는 핵심이다(테스트 주석 :276-278).

`Hourly`는 이 중 **2번만 "로컬 시(hour) 라벨 단위"로 축소**하고 1·3·4를 그대로 쓴다. 즉 DST 규칙을 새로 쓰지 않는다 — 복제하면 두 모드의 규칙이 갈라질 자리가 생긴다.

### 4.2 시그니처

```rust
/// 매시 `minute`분(로컬 벽시계) 회차 중 `after`보다 "엄격히 큰(>)" 첫
/// 인스턴트. `next_occurrence_in`의 날짜 루프를 시(hour) 루프로 축소한
/// 것이고, DST 해석은 같은 `resolve_local_instant`/`pick`을 그대로 쓴다.
pub(crate) fn next_hourly_in<Tz: TimeZone>(
    tz: &Tz,
    minute: u32,          // 0..=59 (호출부에서 이미 정규화된 값)
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>>;

/// 기기 로컬 타임존 얇은 래퍼 — `next_occurrence`(:140)와 같은 관례.
pub(crate) fn next_hourly(minute: u32, after: DateTime<Utc>) -> Option<DateTime<Utc>>;
```

흐름(의사 절차):

```
after_local = after.with_timezone(tz)
naive = after_local.date_naive().and_hms_opt(after_local.hour(), minute, 0)   // 이번 시 라벨의 M분
for _ in 0..4 {
    if let Some(instant) = pick(tz.from_local_datetime(&naive),
                               tz.from_local_datetime(&(naive + 1h))) {       // = resolve_local_instant와 동일 규칙
        if instant > after { return Some(instant) }
    }
    naive = naive + 1h        // ← 로컬 naive 공간에서 "시 라벨"을 전진시킨다
}
None                          // fail-closed (실존 tz에서는 도달하지 않는다, §4.4)
```

> `resolve_local_instant`는 `(date, hour, minute)`를 받는 형태(:101-106)라 그대로는 못 쓴다. **naive를 받는 오버로드를 하나 뽑아내고(`resolve_local_naive(tz, naive)`) 기존 `resolve_local_instant`가 그것을 호출하게 하는 순수 리팩터**를 권한다 — 기존 시그니처·동작·테스트는 그대로 두고 내부만 위임한다. 이렇게 해야 DST 규칙 구현이 파일 안에 **하나만** 존재한다.

### 4.3 DST — `Hourly`가 FixedTime 규칙을 그대로 쓰는 것이 왜 '수용'이 아니라 '정답'인가

| 상황 | FixedTime(`pick`) | **Hourly** | Cron(cron-parser) |
|---|---|---|---|
| 정상(Single) | 그 인스턴트 | 그 인스턴트 | 그 인스턴트 |
| 가을 중복(Ambiguous) | 이른 쪽 1회 | **이른 쪽 1회** | 이른 쪽 1회 |
| 봄 부재(None) | `naive + 1시간`으로 밀어 그날 실행 | **`naive + 1시간` — 동일** | **그날 회차를 건너뜀(다름, §5.3)** |

초판은 봄 부재 규칙의 FixedTime↔Cron 비대칭을 "의도된 수용"으로 정리했다. 그 판단 근거는 *"`30 2 * * *`는 03:30을 뜻하지 않는다 — 사용자가 적어 준 계약을 우리가 깨는 쪽이 건너뛰는 쪽보다 나쁘다"*였다. **이 근거를 Hourly에 그대로 적용하면 결론이 반대로 나온다:**

- `Hourly(minute=30)`의 계약은 "**매시** 30분"이다. 봄 전환으로 02:30이 존재하지 않을 때 `+1시간` fallback이 만드는 값은 03:30인데, **03:30은 이 표현이 어차피 실행하기로 약속한 시각이다.** 분(分)은 보존되고 시(hour) 라벨만 밀린다.
- 즉 Hourly에서 `+1시간` 규칙은 "사용자가 지정하지 않은 시각에 실행"을 만들지 않는다. Cron에서 그 규칙을 거부한 이유가 Hourly에는 존재하지 않는다.
- 반대로 Hourly에서 Cron식 "건너뜀"을 택하면 **전환일에 02시대와 03시대가 둘 다 비는** 결과가 될 수 있어 "매시간"이라는 계약을 더 크게 깬다.

**따라서 세 모드의 규칙은 "FixedTime·Hourly = 밀어서 실행 / Cron = 건너뜀"으로 정리된다. 이 비대칭의 판정 기준 — "밀린 인스턴트가 그 표현이 약속한 시각 집합 안에 있는가" — 은 Hourly에는 그대로 성립하지만, FixedTime은 이 기준의 예외다(레거시 호환으로 밀기를 유지한다).** 다음 모드를 추가할 때는 FixedTime이 아니라 이 기준(및 Hourly의 적용 사례)으로 판정한다. 이 문장을 `pick()` 주석 옆에 그대로 남긴다(다음 모드를 추가하는 사람이 같은 기준으로 판정할 수 있도록).

**전환일 실행 횟수(관측 가능한 결과)**

- 봄(1시간 소실, 예: US/Pacific 02:00→03:00): 라벨 00,01,**02**,03,…,23 중 02 라벨이 03:M 인스턴트로 흡수된다. 03 라벨도 같은 인스턴트라 §4.4의 `> after` 필터가 두 번째를 버린다 → **그날 23회**(23시간 = 23회, 정확).
- 가을(1시간 중복, 예: 01:00 두 번): 01 라벨은 이른 쪽(PDT) 1회만. 두 번째 통과의 01:M은 실행하지 않는다 → **그날 24회**(25시간 동안 24회 — 01:M PDT와 02:M PST 사이만 2시간 간격).
- 한국(KST)은 DST를 쓰지 않으므로 1차 사용자에게는 관측되지 않는다. 그래도 **결정으로 적어 둔다**(개발자가 DST 지역 tz로 머신을 맞추면 바로 보인다).

### 4.4 단조성 — `Hourly`에 별도 가드가 필요 없는 이유 (③)

Cron 경로에는 `advance_until_strictly_after`라는 별도 가드가 필요하다(§5.4). **Hourly에는 필요 없다.** 근거는 추측이 아니라 다음 두 보조정리다.

**보조정리 1 (후보 인스턴트는 라벨 순서로 비감소):** 후보는 로컬 naive 라벨을 +1시간씩 전진시키며 만들고, 각 라벨은 `pick`으로 확정된다. 가을 중복 라벨은 이른 쪽(그 라벨의 최소 인스턴트)을, 봄 부재 라벨은 +1시간(= 다음 라벨의 인스턴트)을 준다. 두 경우 모두 결과가 **다음 라벨의 인스턴트를 넘어서지 않는다.** 따라서 후보 인스턴트 수열은 비감소다.

**보조정리 2 (거부는 접두부에만 발생, 최대 1회):** 필터는 `instant > after`뿐이고 후보가 비감소이므로 거부 구간은 수열의 접두부다. 첫 후보는 `after`의 로컬 시 라벨에서 시작하므로 그 인스턴트는 `after`보다 최대 1시간 이르다(가을 되감기 구간이면 최대 2시간). 두 번째 후보는 첫 후보보다 최소 1시간, 되감기 구간에서는 2시간 뒤다 — 계산하면 **두 번째 후보는 항상 `after`보다 크다.** 실측 대응 시나리오:

> `after` = 11-01 01:45 **PST**(두 번째 통과, 09:45Z), `minute=30`
> → 라벨 01 → `Ambiguous` → 이른 쪽 01:30 **PDT**(08:30Z) → `08:30Z > 09:45Z` 거짓 → **거부**
> → 라벨 02 → `Single` 02:30 PST(10:30Z) → 통과 ✅

즉 초판이 Cron에서 발견한 "예정에 없던 즉시 실행 1회" 경로가 Hourly에도 **똑같이 존재하지만**, 정방향 라벨 루프 + `> after` 필터 조합이 그것을 구조적으로 흡수한다. Cron이 별도 가드를 필요로 하는 이유는 **우리가 루프를 돌리지 않고 crate에 한 번 묻고 끝내기 때문**이다(§5.4).

**루프 상한 `4`의 근거:** 거부 최대 1회(보조정리 2) + 해석 실패(`pick`이 `None`) 최대 1회 + 성공 1회 = 3회면 충분하다. 해석 실패는 2시간 점프 tz에서만 발생한다(실존: `Antarctica/Troll`, 3월에 UTC+0→UTC+2). 여유 1을 더해 4로 둔다. 이 상한에 도달하면 `None`(fail-closed — 실행하지 않는다). 참고로 기존 `next_occurrence_in`도 같은 방식으로 "주기(7일) + DST 여유 1"을 상한 8로 잡았다(:113-116).

### 4.5 `MISSED_RUN_GRACE`와의 상호작용 — `schedule.rs:13-18` 불변식의 이행

파일 13-18행이 스스로 예고해 둔 조건이 **이번에 발동한다**:

> *"⚠️ 불변식(나중에 '하루 여러 시각'이나 **'시간 단위 고정 스케줄'**을 추가할 때 반드시 재검토): 지금 설계는 회차 간 최소 간격이 24시간이라는 전제 위에서, `MISSED_RUN_GRACE_MINUTES`가 그 간격보다 짧다는 사실만으로 '따라잡기는 최대 1회'를 별도 로직 없이 보장한다."*

재검토 결론 — **보장은 유지되지만 근거가 두 겹이 된다.**

| 모드 | 회차 간 최소 간격 | 기존 근거(간격 > grace 30분)가 유효한가 |
|---|---|---|
| FixedTime | 24시간 | ✅ 여유 48배 |
| **Hourly** | **60분** | ✅ 유효하지만 **여유가 2배로 줄었다** — `MISSED_RUN_GRACE_MINUTES`를 60 이상으로 올리면 이 근거가 깨진다 |
| **Cron** | **하한 없음**(`*/5`) | ❌ **무효** |

- Cron에도 보장이 서는 **새 근거**: `initial_next_run_at`은 회차 "목록"이 아니라 **단일 인스턴트**를 반환하고, 그 값이 과거면 `max(startup_floor)`가 `now + STARTUP_GRACE` 한 점으로 눌러 버린다(:152, :159). 지나간 회차가 6개든 1개든 실행은 **정확히 1회**다. 이 근거는 모드와 무관하게 성립하므로 Hourly에도 이중으로 걸린다.
- 조치 3건:
  1. 13-18행 주석을 위 표 + 새 근거로 갱신한다(예고가 발동했음을 명시).
  2. `missed_run_grace_is_shorter_than_minimum_occurrence_gap`(schedule.rs:527)의 상수를 `24*60` → **`60`(Hourly)** 으로 **강화**하고 테스트명·주석에 "가장 촘촘한 벽시계 모드는 이제 Hourly다"를 적는다. 삭제하지 않는다 — 이 테스트가 바로 "grace를 60 이상으로 올리는 변경"을 막는 장치가 된다.
  3. 신규 회귀 1건: `*/5 * * * *`가 grace 창을 여러 번 지나쳤어도 등록 시 반환값은 `startup_floor` **한 점**(§9 T9).

---

## 5. 결정 C-2 — `Cron`의 다음 실행 계산 (초판 유지)

### 5.1 기존 원칙 (인용)

- (schedule.rs:6-11) 핵심 함수를 `Tz: TimeZone` 제네릭으로 둔다 — 테스트에서 `FixedOffset`을 주입해 CI 러너 타임존과 무관하게 결정적으로 검증하기 위함. DST 분기는 `FixedOffset`으로 재현할 수 없으므로 `pick()`을 순수 함수로 분리해 `LocalResult` 값을 직접 넣어 검증한다.
- (schedule.rs:137-142) 프로덕션은 `Local` 얇은 래퍼, 테스트는 `FixedOffset` 주입.
- (schedule.rs:1-4) `scheduler::select_due()`는 이 모듈의 결과값(`next_run_at`)만 소비할 뿐 모드를 전혀 모른다.

### 5.2 경계 결정: **naive는 우리가 만들지 않는다**

```rust
pub(crate) fn next_cron_in<Tz: TimeZone>(tz: &Tz, expr: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>>;
pub(crate) fn next_cron(expr: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>>;
```

내부 흐름은 3줄이다: `after.with_timezone(tz)` → `cron_parser::parse(expr, &local)` → `.with_timezone(&Utc)`.

**왜 이 경계인가** — crate 구현(lib.rs:125-219)을 읽고 내린 결론:

1. crate는 `dt.naive_local()`로 **로컬 naive 공간에서** 필드 매칭을 반복하고, 매치를 찾은 마지막 순간에만 `tz.from_local_datetime(...)`으로 인스턴트를 확정한다(lib.rs:214-219).
2. 그 확정 로직이 **우리 `pick()`과 같은 규칙**을 이미 쓴다 — `LocalResult::Ambiguous(earlier, _later) => break earlier`(lib.rs:216). "가을 모호는 이른 쪽" 원칙이 세 모드에서 저절로 일치한다.
3. 우리가 naive를 직접 만들어 넘기면 이 확정 단계가 UTC로 고정돼 **tz 해석이 통째로 사라진다**(saffron 탈락 사유와 같은 문제).
4. `Tz: TimeZone` 제네릭이 유지되므로 테스트의 `FixedOffset` 주입 관례가 cron 경로에도 100% 이어진다.

### 5.3 봄 부재를 FixedTime/Hourly 규칙으로 맞추지 않는 이유

- `+1시간`은 "그 표현이 약속한 시각 집합 안에 밀린 값이 들어갈 때만" 옳다(§4.3에서 세운 기준). cron 표현식은 시(hour)를 고정할 수 있어 `30 2 * * *`를 03:30으로 밀면 **그 표현식이 지정하지 않은 시각에 실행**된다.
- 실제 노출 범위: 봄 DST 전환일의 1시간 창에 **매치가 전부 들어가는 표현식**만 해당한다. 시간 단위 이상으로 자주 도는 표현식은 실측에서 정상 진행했다(`0 * * * *` @2026-03-08 00:30 PST → 01:00 PST).
- 이 비대칭은 결정이므로 §4.3 표를 `pick()` 주석 옆에 명시하고 회귀로 고정한다.

### 5.4 단조성 가드 — `after` 이하가 나오는 실재 경로 (③)

가을 되감기 구간에서 crate가 `after`보다 **이른** 인스턴트를 돌려주는 경로가 있다.

> `after` = 11-01 01:00 **PST**(두 번째 통과, 09:00Z), 표현식 `30 1 * * *`
> → crate는 naive `01:01`에서 시작해 naive `01:30` 매치 → `Ambiguous` → **이른 쪽 = 01:30 PDT(08:30Z)** 반환
> → 반환값(08:30Z) **<** after(09:00Z) → `select_due`의 `next <= now`가 즉시 성립 → **예정에 없던 즉시 실행 1회**

FixedTime·Hourly는 이 경로가 구조적으로 막혀 있다(`instant > after` 필터, §4.4). 따라서 이 가드는 신규 안전장치가 아니라 **이미 합의된 불변식을 cron 경로에도 동등하게 적용**하는 것이다.

```rust
/// pick()과 같은 이유로 분리한 순수 함수 — DST는 FixedOffset으로 재현할 수
/// 없으므로, "계산기"를 클로저로 주입해 값으로 직접 검증한다.
/// 되감기 폭은 실존 tz에서 1시간을 넘지 않으므로 1시간씩 최대 3회만 민다.
/// 전부 실패하면 None(fail-closed — 실행하지 않는다).
pub(crate) fn advance_until_strictly_after(
    next_from: impl FnMut(DateTime<Utc>) -> Option<DateTime<Utc>>,
    after: DateTime<Utc>,
) -> Option<DateTime<Utc>>;
```

`next_cron_in`은 이 함수에 `|anchor| parse(expr, &anchor.with_timezone(tz)).ok().map(|d| d.with_timezone(&Utc))`를 넘긴다. 앵커를 **인스턴트 기준 +1시간**씩 미는 것이 중요하다 — 되감기 "두 번째 통과"에서 인스턴트 +1h는 로컬 naive도 +1h 밀어 모호 구간을 빠져나온다.

### 5.5 성능

`ensure_registered`(scheduler.rs:143)의 인자는 `or_insert_with` 밖에서 **매 tick(10초) × 매 task마다 무조건 평가**된다(리뷰 n1). Cron이 들어오면 그 평가에 파서가 끼는데 실측 비용은 일반식 **1.46µs**, 최악 **64µs**다. task 50개여도 tick당 최악 3.2ms — 무시 가능하다. Hourly는 파싱이 없어 더 싸다. **n1 수정(클로저화)은 이번 스코프에 넣지 않는다**(수치를 근거로 남겨 두는 것으로 충분).

---

## 6. 결정 E·F — enum 확장의 영향 지점 전수 (④ 완결성)

조사 방법: `grep -rn "ScheduleMode|schedule_mode" src-tauri/src --include='*.rs'` 전수 + 해당 3개 파일 전문 Read(§1 하단). `ScheduleMode`를 참조하는 비-테스트 코드는 **`autonomy/config.rs`·`schedule.rs`·`mod.rs` 3개 파일뿐**이다.

### 6.1 컴파일 에러가 나는 match — **정확히 3곳**, 의미가 서로 다르다 (지시 ③)

세 곳은 "다음 실행 시각을 누가·어떤 앵커로 묻는가"가 각각 다르다. 앵커를 호출부마다 따로 고르다 어긋난 것이 리뷰 M2·M3·m1의 공통 뿌리였으므로(`schedule.rs:164-186` 주석), 신규 팔도 **기존 모드가 쓰는 앵커를 그대로 따른다**.

| # | 파일:라인 | 함수 / 계기 | 앵커 규칙 | `Hourly` 팔 | `Cron` 팔 |
|---|---|---|---|---|---|
| X1 | `schedule.rs:153` (팔 :154·:155) | `initial_next_run_at` — **(A) 앱 시작 / tick이 외부 추가를 처음 발견**. 따라잡기 창(`MISSED_RUN_GRACE`) 적용 + `startup_floor`(재시작 몰림 방지) 하한 | `max(f(now - grace), startup_floor)` | `let m = task.hourly_minute_spec()?;`<br>`Some(next_hourly(m, now - grace)?.max(startup_floor))` | `let e = task.cron_expr()?;`<br>`Some(next_cron(e, now - grace)?.max(startup_floor))` |
| X2 | `schedule.rs:191` (팔 :192·:193) | `reschedule_next_run_at` — **(C) 편집 저장 / (D) 중지→재개**. 따라잡기 창도 floor도 **적용하지 않는다**. "지금 이 순간 기준 다음 회차"만 답한다 | `f(now)` | `next_hourly(task.hourly_minute_spec()?, now)` | `next_cron(task.cron_expr()?, now)` |
| X3 | `schedule.rs:206` (팔 :207·:210) | `next_run_after_finish` — **(B) 실행 완료 직후**. 인자가 `&AutonomyTaskConfig`가 아니라 **`&ScheduleSnapshot`**(마킹 시점 동결값, runner.rs:12-17 스냅숏 철학) | `f(finished_at)` | `next_hourly(schedule.hourly_minute?, finished_at)` | `next_cron(schedule.cron.as_deref()?, finished_at)` |

**X3이 다른 점(중요):** X1·X2는 `task.<accessor>()`로 현재 설정을 읽지만 X3은 스냅숏 필드를 읽는다. 그래서 `ScheduleSnapshot`(schedule.rs:36-40)에 **필드 2개를 추가**하고 `From<&AutonomyTaskConfig>`(:42-50)에서 접근자로 채워야 한다(§8.2). 이 한 단계를 빼먹으면 X3가 "완료 후 다음 회차 없음"으로 조용히 떨어져 **task가 1회 실행 후 영영 멈춘다** — 리뷰 m2가 FixedTime에서 지적한 "분기 직접 커버리지 0"과 같은 자리라 T6·T7 테스트를 프로덕션 사슬 전체로 태운다(§9).

세 곳 모두 **와일드카드 팔(`_ =>`)이 없다** — 컴파일러가 반드시 잡아 준다. 새 팔에도 `_ =>`를 넣지 않는다(다음 모드 추가 때도 같은 보호를 받기 위해).

### 6.2 조용히 컴파일되는 화이트리스트 3지점 — 지점별 결론 (지시 ②)

> **선행 정정.** 위임 프롬프트는 `config.rs:81`을 "`normalize_task()`의 clamp 규칙"으로 적었으나, 실물 :81은 **`fixed_time_spec()` 안의 모드 게이트**(`if self.schedule_mode != ScheduleMode::FixedTime { return None }`)다. `normalize_task()`는 **:121-144**에 따로 있다(clamp 대상: `interval`·`timeout`·`days`·`at_time`). 두 곳의 성격이 정반대라(전자는 읽기 게이트, 후자는 쓰기 정규화) 아래에서 나눠 다룬다.

| # | 지점 | 이 코드가 실제로 판정하는 것 | 신규 모드에서 나와야 하는 값 | 조치 |
|---|---|---|---|---|
| **S1** | `mod.rs:59-65`, 핵심 `:63`<br>`should_reschedule_on_save` | "저장 직후 `next_run_at`을 **즉시 재계산**할 것인가." `Interval`만 예외로 두는 코드다 — Interval은 카운트다운 성격이라 "편집은 다음 회차부터 반영"이 기존 합의(:57-58 주석), 벽시계 모드는 값 자체가 인스턴트를 정의하므로 즉시 반영해야 한다(M3) | **`Hourly` = true, `Cron` = true.** 현행 `new_mode == FixedTime`이면 `Hourly`→`Hourly`(분만 변경)·`Cron`→`Cron`(표현식만 변경) 편집이 **재스케줄되지 않는다** → 리뷰 M3/m1과 **같은 뿌리의 결함이 신규 모드에서 재발**한다(최대 1시간/무기한 늦게 반영) | **고친다**(아래 S-공통) |
| **S2** | `mod.rs:195`<br>`autonomy_set_enabled` | "중지→재개 시 `next_run_at`을 **다시 계산**할 것인가." 중지 중에는 `next_run_at`이 지워지지 않고 그대로 남으므로(`select_due`가 `enabled`로만 걸러낸다, scheduler.rs:74), 재개 시 재계산하지 않으면 **과거 값이 그대로 due가 되어 즉시 1회 실행**된다(M2) | **`Hourly` = true, `Cron` = true.** 현행대로면 매시 30분 task를 3시간 중지 후 재개하는 순간 **즉시 실행**된다. Interval은 기존 예외 유지(:201-206 주석이 명시한 의도된 동작) | **고친다**(아래 S-공통) |
| **S3** | `config.rs:76-91`, 핵심 `:81`<br>`fixed_time_spec()` | "이 task에서 **FixedTime 사양을 꺼낼 수 있는가**." `!=`(블랙리스트 아닌 **부등호**) 형태라 신규 variant는 자동으로 `None`으로 떨어진다 | **`Hourly`/`Cron` = `None`이 정답.** `Hourly` task가 `at_time` 잔여값을 갖고 있어도 FixedTime 사양으로 오독되지 않는다 | **코드 변경 없음.** 대신 주석 1줄로 "이 부등호는 의도된 형태 — 새 모드가 추가되면 여기서 `None`이 정답이다"를 못 박는다. 그리고 **짝이 되는 접근자 2개를 같은 자리에 신설**한다(§8.2) |

**S-공통 조치 — `match`로 바꿔 컴파일러가 잡게 할 것인가: 예. 단, 조건식 2개를 각각 match로 만들지 않고 술어 1개로 합친다.**

S1과 S2는 표현이 다르지만 **같은 질문**("이 모드의 다음 실행 시각이 벽시계 약속에서 나오는가, 아니면 직전 완료로부터의 카운트다운에서 나오는가")을 한다. 같은 질문이 두 파일에 두 형태로 박혀 있었던 것이 애초에 이번 함정의 원인이므로, **정본 하나**로 모은다:

```rust
// config.rs — ScheduleMode 옆
impl ScheduleMode {
    /// 다음 실행 시각이 "벽시계 약속"에서 나오는 모드인가(= 직전 완료 시각
    /// 으로부터의 카운트다운이 아닌가). 저장 직후 재계산(M3)과 중지→재개
    /// 재계산(M2)이 필요한 모드가 정확히 이 집합이다.
    ///
    /// `_ =>` 와일드카드를 쓰지 않는다 — 모드를 추가하면 **여기서 컴파일
    /// 에러가 나서** 추가자가 "이 모드는 저장·재개 때 재계산해야 하는가"를
    /// 반드시 한 번 판단하게 만드는 것이 이 match의 목적이다.
    pub(crate) fn is_wall_clock(self) -> bool {
        match self {
            ScheduleMode::Interval => false,
            ScheduleMode::FixedTime | ScheduleMode::Hourly | ScheduleMode::Cron => true,
        }
    }
}
```

- `mod.rs:63` → `new_mode.is_wall_clock() || previous_mode.map(|m| m != new_mode).unwrap_or(false)`
- `mod.rs:195` → `enabled && task.schedule_mode.is_wall_clock()`

**트레이드오프(정직하게)**: 두 호출부를 각각 `match`로 인라인하면 지점별로 다른 값을 줄 자유가 남지만, 지금 두 지점의 답이 완전히 같고 앞으로 갈릴 근거도 없다 — 자유를 남기는 대신 **두 곳이 어긋날 가능성**을 남기는 쪽이 이 코드베이스가 실제로 겪은 실패(M2/M3/m1)다. 갈릴 필요가 생기면 그때 술어를 2개로 쪼개면 되고, 그 시점에도 컴파일러 보호는 유지된다. `config.rs:81`을 match로 바꾸지 않는 이유도 같은 잣대다 — 그 지점은 **이미 올바르고**, match로 바꿔도 새로 잡히는 실수가 없으며 단순함만 잃는다.

**기존 테스트 영향**: `mod.rs:377-407`의 `should_reschedule_on_save_*` 3건은 그대로 통과한다(동작 동일). 신규 T11이 `Hourly`/`Cron` 케이스를 덮는다.

### 6.3 구조체 리터럴 — 필드 추가로 인한 컴파일 에러 4곳

| 파일:라인 | 리터럴 | 채울 값 |
|---|---|---|
| `config.rs:206-218` | `sample_task()` | `hourly_minute: None, cron: None` |
| `schedule.rs:373-384` | `fixed_task()` 헬퍼 | 동일 |
| `schedule.rs:448-452` | `ScheduleSnapshot{ Interval }` | `hourly_minute: None, cron: None` |
| `schedule.rs:468-476` | `ScheduleSnapshot{ FixedTime }` | 동일 |

> 즉 **"컴파일 에러 지점" = match 3곳 + 구조체 리터럴 4곳 = 7곳**이다. 지시가 물은 *exhaustive match* 지점만 세면 **3곳**이다.

### 6.4 프론트엔드 계약 (구현은 frontend-dev 범위, 계약만 확정)

| 파일:라인 | 현재 | 확장 |
|---|---|---|
| `src/autonomyApi.ts:15` | `type AutonomyScheduleMode = 'interval' \| 'fixedTime'` | `\| 'hourly' \| 'cron'` 추가 |
| `src/autonomyApi.ts` `AutonomyTaskConfig` | — | `hourlyMinute?: number \| null` · `cron?: string \| null` 추가. 백엔드가 `skip_serializing_if=None`이라 **키 자체가 없을 수 있다**(:24-30 주석이 이미 경고한 옵셔널 함정) |
| `src/state.ts:36-38` | `scheduleMode`/`atTime`/`days` | `hourlyMinute: number \| null` · `cron: string \| null`(비-옵셔널 정규화) 추가 |
| `src/views/autonomousTasks.ts:151` `computeScheduleLabel` | 2분기 | 4분기 — `hourly` = `매시 ${M}분`, `cron` = 표현식 원문(빈 값이면 `스케줄을 다시 지정해 주세요`, §8.3-3) |
| `src/views/autonomousTasks.ts:185-196` `computeNextRunLabel` | `nextRunAt==null`이면 fixedTime만 오류 문구 | `hourly`/`cron`도 fixedTime과 **같은 취급**(`'실행 시각이 올바르지 않습니다'`). **네 모드 중 `null`이 정상인 모드는 없다** — 초판의 `Manual`이 사라지면서 리뷰 M4(거짓 오류 문구) 재발 위험도 함께 사라졌다 |
| `src/views/autonomousTasks.ts:934` | `days.length===0` 거부 | **매주 탭 전용**으로 이동(§2.3) |
| `src/views/autonomousTasks.ts:851-866` `updateStartupHint` | 2분기(고정시각/상대간격) | 1단 라디오 기준 그대로 유지 — 고정시간 4개 탭은 모두 같은 따라잡기 문구를 쓴다(`missedRunGraceMinutes`는 계속 `limits`에서 읽고 **하드코딩 금지**) |

---

## 7. 결정 G — 검증 위치와 프론트 에러 계약 (지시 ④)

### 7.1 신규 Tauri 커맨드를 만들지 않는다

`capabilities/default.json`(CLAUDE.md의 "권한 표면을 의도적으로 좁게 유지" 원칙)을 건드리지 않는다. 검증은 **기존 `autonomy_save_task`**(mod.rs:128-165) 안에서 한다. 실시간 미리보기 커맨드(`autonomy_preview_cron` 등)는 **이번 범위에 넣지 않는다** — 저장 시점 거부만으로 잘못된 표현식이 파일에 들어가는 경로가 닫히고, 미리보기는 "있으면 좋은 것"이다.

### 7.2 3계층 배치

| 계층 | 위치 | `Hourly` | `Cron` |
|---|---|---|---|
| **L1 경계**(즉시 피드백) | `mod.rs:137` 락 획득 직후, `config::upsert_task`(:144) **이전** | `hourly_minute`이 없거나 `>59`면 **파일을 쓰지 않고** `Err(한국어)` | `validate_cron(expr)?` 실패면 **파일을 쓰지 않고** `Err(한국어)` |
| **L2 도메인**(fail-closed 보존) | `config.rs:121` `normalize_task` | `filter(|m| *m <= 59)` → 위반 시 `None` | 공백 정규화 후 파싱 불가/빈 값이면 `None` |
| **L3 최종 게이트** | 접근자 → `None` → `next_*` → `None` → `scheduler.rs:81`이 후보에서 제외 | ✅ | ✅ |

- **L2에서 모드를 절대 다운그레이드하지 않는다.** `at_time`에 대해 이미 확립된 규칙(config.rs:132-136: *"Interval로 '다운그레이드'하지 않는다 — 그러면 하루 1회 의도가 interval(최소 5분) 실행으로 바뀌어 최대 288배 과잉 실행이 된다"*)을 신규 2모드에 그대로 적용한다.
- L1이 없어도 L2+L3이 안전하지만(실행 안 됨), L1이 없으면 사용자는 "저장은 됐는데 영영 안 돈다"를 겪는다 — 리뷰 M4가 지적한 UX 실패 계열. **L1의 존재 이유는 안전이 아니라 즉시 피드백이다.**
- `Hourly`에도 L1을 두는 이유: 그것이 없으면 잘못된 분(分)이 L2에서 조용히 `None`이 되어 위 UX 실패로 직행한다. FE의 `<input type="number" min="0" max="59">`는 계약이지 방어가 아니다(손편집 파일·FE 버그).

### 7.3 `validate_cron`이 검사하는 것

```rust
// schedule.rs
pub(crate) fn validate_cron(expr: &str) -> Result<(), String>;
```

1. **파싱 가능성** — `cron_parser::parse(expr, &Local::now())`가 `Ok`인가.
2. **과잉 실행 브레이크** — 다음 10회차를 뽑아 인접 간격의 최소값이 `MIN_INTERVAL_MINUTES`(=5, config.rs:19 **정본**) 이상인가. `* * * * *`(매분)·`*/2` 같은 표현식을 저장 단계에서 거부한다.
   - `Interval` 모드에는 이미 5분 하한 clamp가 있는데(config.rs:123) cron에는 어떤 하한도 없다 — 그 비대칭을 메운다. **새 상수를 만들지 않고 기존 정본 상수를 재사용**하고, 에러 문구의 숫자도 `format!("... 최소 {MIN_INTERVAL_MINUTES}분 ...")`으로 **상수에서 생성**한다(값을 바꿀 때 고칠 파일이 1개가 되도록 — config.rs:14-16이 선언한 규율).
   - 비용: 파싱 10회 ≈ 15µs.
   - **정직한 한계**: 샘플링은 증명이 아니다. `0,1 9 1 1 *`(1년에 두 번, 1분 간격)처럼 짧은 간격이 먼 미래에만 있으면 통과한다. 최종 브레이크는 따로 있다 — 같은 task는 `select_due`가 `running`인 동안 절대 재선택하지 않고(scheduler.rs:76-80), 전역 `concurrency` 상한(기본 3, 최대 8)과 task별 타임아웃이 걸린다. 이 가드는 "실수로 매분 실행"을 막는 실용적 브레이크지 상한 증명이 아니다.
3. 길이/문자 상한 같은 촘촘함은 여기서 정하지 않는다 — **로직 강도** 항목이라 필요해질 때 올린다(§7.5-b).

`Hourly`는 이에 대응하는 브레이크가 필요 없다 — 간격이 항상 60분으로 고정이라 구조적으로 `MIN_INTERVAL_MINUTES`를 넘는다.

### 7.4 프론트 에러 계약

기존 계약을 그대로 쓴다. `saveAutonomyTask`가 reject하면 `autonomousTasks.ts:958`이 `showToast(err.message)`로 **Rust가 돌려준 문자열을 그대로 띄운다**. 따라서 **메시지 자체가 계약**이다.

| 상황 | `autonomy_save_task` 반환 | 사용자가 보는 것 |
|---|---|---|
| 정상 | `Ok(())` | "추가되었습니다" 토스트 + 목록 갱신 |
| 매시간: 분 누락/범위 밖 | `Err("매시간 모드는 0~59 사이의 분을 지정해야 합니다.")` | 그 문장이 토스트로 |
| cron: 파싱 실패 | `Err("cron 표현식이 올바르지 않습니다. 5칸(분 시 일 월 요일)으로 적어 주세요. 예: 0 9 * * 1-5")` | 그 문장이 토스트로 |
| cron: 너무 잦음 | `Err(format!("실행 간격이 너무 짧습니다. 최소 {MIN_INTERVAL_MINUTES}분 이상 간격이 되도록 표현식을 조정해 주세요."))` | 그 문장이 토스트로 |
| 경로 검증 실패 | `Err("프로젝트 경로가 올바르지 않습니다.")` (기존 그대로) | 기존 그대로 |

규칙 3가지:

1. **crate의 영어 에러(`ParseError::InvalidValue` 등)를 그대로 흘리지 않는다.** 한국어 고정 문구로 감싼다 — 내부 타입명·파일 경로·스택이 사용자 화면에 나가지 않게.
2. 필드 위치까지 알려 주려 시도하지 않는다 — crate가 어느 칸이 틀렸는지 주지 않는다(`InvalidValue` 하나로 뭉뚱그림). **없는 정보를 지어내지 않는다.**
3. **저장이 거부되면 모달은 닫히지 않는다**(기존 catch 블록이 `saveBtn.disabled = false`로 되돌리고 모달을 유지한다 — `autonomousTasks.ts:959-962`). 별도 작업 불필요.

프론트가 추가로 알아야 할 계약(구현은 FE 몫):

- `scheduleMode='hourly'`면 `hourlyMinute`를 0~59 정수로 반드시 채워 보낸다(없거나 범위 밖이면 L1 거부).
- `scheduleMode='cron'`이면 `cron` 문자열을 반드시 채워 보낸다(빈 문자열·null이면 L1 거부).
- `scheduleMode='fixedTime'`이면 매일 탭은 `days: []`, 매주 탭은 최소 1개(§2.3).
- 사용자 지정 탭 도움말 고정 문구: **`분 시 일 월 요일` 5칸 / 요일 0=일…6=토 / `@daily` 같은 매크로 미지원 / 일·요일을 동시에 좁히면 둘 다 맞는 날에만 실행(AND)**.
- 매시간 탭 도움말: **"매시 M분에 실행합니다. 상대 간격 60분과 달리 벽시계 정각에 맞춰 실행합니다."** (두 옵션이 UI에서 60분으로 겹쳐 보이므로 차이를 문장으로 고지한다.)
- cron 최소 간격 숫자를 **하드코딩하지 않는다** — 이미 노출돼 있는 `limits.minInterval`(`src-tauri/src/config/mod.rs:42-43,63` → `src/configApi.ts:14`)을 읽는다.

### 7.5 보안 구조적 결정 확정표

Skill `domain-backend-api-security`의 "언제 정하는가 (a) 구조적 결정" 항목을 **열어 하나씩 대조**했다. 이 기능은 HTTP API가 아니라 로컬 Tauri IPC이므로 항목을 그 축으로 옮겨 읽었다.

| (a) 항목 | 이번 설계의 값 |
|---|---|
| 인증 게이트 위치 / 공개 경로 경계 | **변경 없음.** 신규 커맨드 0개, `capabilities/default.json` 무변경. 게이트는 기존 `crate::resolve_validated_project_root`(workspace 밖 경로 거부, mod.rs:133) 하나 그대로 |
| 인가의 축(역할 개념) | **도입하지 않는다.** 단일 로컬 사용자 데스크톱 앱, 역할 개념이 존재하지 않음 |
| 소유권/테넌트 키의 존재 | **변경 없음.** 소유 경계는 `TaskKey = (project_path, task_id)`이고 `project_path`는 검증된 workspace 하위여야 한다. 신규 필드 2개는 이 경계와 무관 |
| 자격증명 전달 방식·CORS | **해당 없음**(로컬 IPC, 네트워크 표면 없음) |
| 민감 데이터의 저장 위치·형태 | **신규 민감 데이터 없음.** `hourlyMinute`/`cron`은 사용자가 입력한 스케줄 값이며 `autonomy.json`에 평문(기존 `prompt`/`atTime`과 동일 취급) |
| 응답으로 내보내는 것의 경계 | **신규 노출 = 스케줄 값 2개(본인 입력) + 검증 에러 문구**뿐 → 경계 이동 없음. 단 §7.4-1(crate 원문 에러 비노출)을 값으로 확정 |

**강도 조절 대상이 아닌 최소 요건**(처음부터 지킨다):

- **fail-closed** — 잘못된/미지의 스케줄 설정은 "실행하지 않음"으로 떨어진다(L2·L3 + §8.3 미지 모드 흡수). 절대 `Interval`로 다운그레이드하지 않는다.
- **명령어 인젝션 없음** — `cron` 문자열은 파서 안에서만 살고 자식 프로세스 argv에 **전혀 나가지 않는다**. `claude -p <prompt>`의 argv 구성(runner.rs)은 무변경이며, mod.rs:9-16의 셸 미경유 불변식도 그대로다.
- 로그·에러에 토큰·자격증명을 싣지 않음 — 해당 없음(둘 다 취급하지 않음).

**(b) 로직 강도로 미루는 것**(과설계 방지): cron 문자열 길이 상한·허용 문자 화이트리스트, `MIN_INTERVAL_MINUTES` 값 자체의 조정, 샘플 개수 10회의 조정, 에러 문구 세분화.

---

## 8. 데이터 모델 · 스키마 마이그레이션 (지시 ⑤)

### 8.1 `ScheduleMode` 확장

```rust
// config.rs:35-43
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Default)]   // ← Deserialize는 derive에서 제외(§8.3)
#[serde(rename_all = "camelCase")]
pub enum ScheduleMode {
    #[default] Interval,   // "interval"
    FixedTime,             // "fixedTime"  — 매일/매주 두 탭이 공유
    Hourly,                // "hourly"     — 매시 M분
    Cron,                  // "cron"       — 표준 5필드 표현식
}
```

### 8.2 `AutonomyTaskConfig` 신규 필드 — 기존 필드를 재사용하지 않는 결정

```rust
/// Hourly 모드의 실행 "분"(0..=59). **매시 이 분에** 실행한다(기기 로컬 벽시계
/// 기준 — 상대간격 60분과 달리 정각 정렬). 범위를 벗어난 값은 정규화에서
/// 버린다(clamp하지 않는다 — 사용자가 지정하지 않은 분에 실행하지 않기 위함,
/// at_time과 같은 fail-closed 규칙).
#[serde(default, rename = "hourlyMinute", skip_serializing_if = "Option::is_none")]
pub hourly_minute: Option<u8>,

/// Cron 모드의 표준 5필드 표현식(`분 시 일 월 요일`). **기기 로컬 벽시계**
/// 기준으로 해석한다(at_time과 동일 원칙 — UTC로 변환해 저장하지 않는다).
/// 정규화를 통과하지 못한 값은 보존하지 않는다(fail-closed).
#[serde(default, skip_serializing_if = "Option::is_none")]
pub cron: Option<String>,
```

- **기존 `interval` 재사용 기각**: `interval`은 모드와 무관하게 **항상 존재하고 항상 clamp된다**는 계약이 이미 문서화돼 있고(config.rs:52-58) 하한이 5다. Hourly의 분은 0이 정상값이라 clamp 계약과 정면 충돌한다.
- **기존 `at_time` 재사용 기각**(`":30"` 같은 형태): `parse_hhmm`의 계약(schedule.rs:52-69)을 깨야 하고, 그 함수는 FixedTime의 방어선이다. 한 파서가 두 의미를 갖는 순간 §6.2의 함정이 파서 레벨에서 재생산된다.
- **단일 `schedule_value: String` 통합 필드 기각**: 모드마다 같은 문자열을 재해석하게 되어 탭을 바꾸면 옛 값이 **조용히 다른 의미로** 읽힌다. 타입이 분리돼 있으면 잔여값이 무해하다(접근자가 모드로 게이트하므로).
- **`u8` vs `u32`**: 0~59라 `u8`로 충분하다. `FixedTimeSpec`이 `u32`(schedule.rs:27-28)인 것은 `chrono` API 시그니처를 그대로 받기 위함이라, 경계에서 `as u32` 한 번만 한다.

`ScheduleSnapshot`(schedule.rs:36-40)에도 두 필드를 추가하고 `From<&AutonomyTaskConfig>`(:42-50)에서 접근자로 채운다 — X3(§6.1)이 이 값을 읽는다.

```rust
pub(crate) struct ScheduleSnapshot {
    pub mode: ScheduleMode,
    pub interval_minutes: u32,
    pub fixed: Option<FixedTimeSpec>,
    pub hourly_minute: Option<u8>,   // 신규
    pub cron: Option<String>,        // 신규
}
```

> 대안으로 `enum ModeSpec { Interval(u32), Fixed(FixedTimeSpec), Hourly(u8), Cron(String) }` 하나로 스냅숏을 접을 수 있고 그쪽이 "불가능한 상태"를 타입으로 제거한다. **이번엔 채택하지 않는다** — `From` 구현과 테스트 리터럴 2곳을 다시 쓰는 리팩터이고, 현재 4모드에서 잘못 조합될 경로가 접근자 게이트로 이미 막혀 있다. 모드가 5개를 넘으면 그때 재검토를 권한다.

**접근자 2개 신설**(`config.rs:76-91`, `fixed_time_spec()` 바로 옆 — 같은 모양·같은 방어 수준):

```rust
/// Hourly 모드이고 분이 0..=59일 때만 값을 준다. (S3와 같은 부등호 게이트)
pub(crate) fn hourly_minute_spec(&self) -> Option<u8>;
/// Cron 모드이고 표현식이 비어 있지 않을 때만 값을 준다.
pub(crate) fn cron_expr(&self) -> Option<&str>;
```

**`normalize_task`(config.rs:121-144) 추가 규칙 — 어디에 끼우는가**

기존 3블록(interval/timeout clamp → days 정리 → at_time 정규화) **뒤에** 2블록을 잇는다:

```rust
// ── 신규 A: 모드가 소유하지 않는 "옵셔널 스케줄 필드"는 버린다 ──
//   모드별로 필드가 정확히 하나만 남는다 → 손으로 열어 본 파일이 자명해지고,
//   §8.3의 미지 모드 흡수가 "스케줄 없음"임을 데이터로 보장한다.
//   `interval`은 이 규칙에서 제외한다 — 레거시 호환 때문에 항상 존재·항상
//   clamp라는 기존 계약(config.rs:52-58)이 따로 있다.
// ── 신규 B: 소유한 필드만 정규화(fail-closed, 모드 다운그레이드 금지) ──
//   hourly_minute: 0..=59 아니면 None  (clamp 아님 — §8.2 주석 참조)
//   cron:          trim + 내부 공백 단일화 후 파싱 성공할 때만 Some
```

- 신규 A의 부작용(정직하게): `Interval` 모드 task가 손편집·FE 잔여로 `atTime`을 갖고 있으면 **읽는 것만으로 메모리에서 지워지고 다음 저장 때 파일에서 사라진다.** 이는 이 파일이 이미 선언한 철학과 **같다** — *"옛 파일에 이 필드가 남아 있어도 조용히 버려지고, 다음 저장 시점에 파일에서 사라진다"*(config.rs:3-8). 현행 FE는 interval 모드에서 `atTime: null, days: []`를 보내므로(`autonomousTasks.ts:926-927`) **FE 관측 변화는 0**이다.
- **기존 테스트 1건이 픽스처 수정을 요구한다**: `normalize_sorts_dedups_and_drops_out_of_range_days`(config.rs:452-457)는 `sample_task`(=**Interval 모드**)에 `days=[4,2,2,9]`를 넣고 `[2,4]`를 기대한다. 신규 A 아래서는 `[]`가 된다. **조치: 픽스처의 모드를 `FixedTime`으로 바꾼다**(이 테스트의 진짜 의도는 요일 살균이고, `days`는 FixedTime에서만 의미를 갖는다). 동작 회귀가 아니라 **테스트가 검증하려던 상황을 정확히 하는 수정**이다. 이 1건 외에 기존 45건은 그대로 통과한다.
- 신규 A를 **넣지 않는** 선택지도 있다(잔여값 보존). 그 경우 §8.3의 흡수 안전성이 "잔여 `cron` 문자열이 없을 것"이라는 **가정**에 의존하게 된다. Sensitive 등급(무인 실행)에서 안전 속성을 가정에 맡기지 않는다는 판단으로 A를 채택했다.

### 8.3 ⚠️ 다운그레이드 안전성 — 미지 모드 값의 흡수

**현재 구조에는 실재하는 데이터 손실 경로가 있고, enum 확장이 그 경로를 처음으로 도달 가능하게 만든다**(§1 실측).

#### 8.3-1 이 조치가 실제로 보호하는 것 / 보호하지 못하는 것 (초판 정정)

초판은 이 조치를 "구 버전 앱이 신 버전 파일을 읽을 때의 데이터 손실 방지"로 적었다. **정확하지 않다.** 우리가 지금 쓰는 코드는 **이미 배포된 바이너리에는 들어가지 않는다.**

| 시나리오 | 이번 조치의 효과 |
|---|---|
| 현재 배포판(`interval`/`fixedTime`만 앎)이 **이번 버전이 쓴** `"hourly"` 파일을 읽음 | ❌ **보호 못 함.** 그 바이너리는 이미 고정돼 있다 → tasks=0 → 다음 저장에서 나머지 task 유실 |
| **이번 버전**이 미래 버전(`"monthly"` 등)이 쓴 파일을 읽음 | ✅ 보호됨(흡수) |
| 이번 버전 이후의 모든 버전 간 교차 | ✅ 보호됨 |

즉 이 조치는 **"이번이 마지막으로 위험한 전환"으로 만드는 조치**다. 현재 배포판↔이번 버전의 1회성 위험은 코드로 없앨 수 없다 — 운영으로 처리한다:

- 자율업무 설정 파일은 **프로젝트 폴더 안**(`<project>/.claude/autonomy.json`)에 있어 저장소 공유·다중 머신을 타고 돌아다닌다. 사내 워크스테이션이 여러 대이므로 **실재 가능한 시나리오**다.
- 완화: 릴리스 노트에 "이 버전 이후에 만든 자율업무는 구버전 앱에서 열지 말 것"을 명시하고, 앱 내 업데이트 경로를 통해 전원 갱신을 유도한다. **PM 판단 사항**이며 설계로 해결하지 않는다.
- (선택 권고, 작음) 파괴적 덮어쓰기 자체를 끊는 방법도 있다: `read_autonomy_file`(config.rs:168-178)의 `unwrap_or_default()`가 **파싱 실패와 파일 없음을 구분하지 못한다**. 쓰기 경로 3곳(`autonomy_save_task`/`autonomy_delete_task`/`autonomy_set_enabled`)만 `Result`를 받는 사촌 함수(`read_autonomy_file_checked`)를 쓰게 하면, 파싱 실패 시 빈 파일을 쓰는 대신 한국어 에러가 토스트로 뜬다. 읽기 전용 경로(`autonomy_list_blocking`/`scan_all_tasks`)는 관대한 현행 함수 그대로 둔다. **이 역시 미래 보호이지 현재 배포판 보호는 아니다** — 이번 스코프에 넣을지는 PM 판단(구현 규모: 함수 1개 + 호출부 3곳).

#### 8.3-2 채택 조치

```rust
// 알려진 값 4개는 그대로, 그 외 미지 문자열은 Cron(표현식 없음)으로
// 떨어뜨린다(§8.3-3). Interval로 떨어뜨리면 정반대(최대 5분마다 무인 실행)가
// 되므로 절대 그렇게 하지 않는다 — fail-closed.
// Serialize는 derive 유지. 필드의 #[serde(default)]는 그대로 두어야 한다 —
// **키가 아예 없는 레거시 파일은 여전히 Default(=Interval)** 이고, 이 수동
// 구현은 "키는 있는데 값이 미지"일 때만 관여한다. 두 경로를 헷갈리면
// 레거시 파일이 전부 Cron으로 바뀐다.
impl<'de> Deserialize<'de> for ScheduleMode { /* 문자열 매칭 6줄 */ }
```

#### 8.3-3 흡수 대상을 `Cron`으로 정한 근거 (초판의 `Manual`을 대체)

`Manual`이 없어졌으므로 "자동 실행 없음"을 뜻하는 전용 값이 없다. 후보는 페이로드가 비면 실행되지 않는 세 모드다.

| 후보 | 실행되나(페이로드 없음) | 잔여값으로 되살아날 위험 | **저장 버튼을 그냥 눌렀을 때** |
|---|---|---|---|
| `Interval` | ❌ **실행된다**(interval은 항상 존재·clamp≥5) | — | 즉시 무인 실행 시작 |
| `FixedTime` | 안 함(`at_time` 없으면 `None`) | ⚠️ 미래 모드가 `atTime`을 함께 쓸 가능성이 있다(예: "매월 N일 HH:MM") → **그 시각에 매일 실행** | 매일 탭이 기본 09:00을 채워 **조용히 유효한 스케줄이 된다** |
| `Hourly` | 안 함 | 낮음(§8.2 신규 A로 잔여 제거) | 매시간 탭의 빈 입력이 0으로 기본화되면 **매시 정각 실행** |
| **`Cron`** ✅ | 안 함 | 낮음(§8.2 신규 A) | **L1이 빈 표현식을 거부한다** → 사용자가 명시적으로 스케줄을 다시 고를 때까지 저장 자체가 막힌다 |

**결정적 차이는 마지막 열이다.** 다른 후보들은 "읽을 때는 안전하지만 사용자가 편집 모달을 열고 저장만 눌러도 실행되는 스케줄로 조용히 굳어진다". `Cron`만이 **쓰기 게이트에서도 fail-closed**라, 해석 불가능한 스케줄이 사용자 모르게 실행 가능한 스케줄로 바뀌는 경로가 없다. 무인 실행 기능에서는 이 성질이 UI 자연스러움보다 우선한다.

- FE 표시 계약(§6.4): `cron`이 비어 있으면 스케줄 라벨은 `스케줄을 다시 지정해 주세요`, 다음 실행 라벨은 `실행 시각이 올바르지 않습니다`. 사용자는 "사용자 지정" 탭이 열린 빈 입력창을 보게 된다.
- 감수하는 것: 원래 모드 문자열은 보존되지 않는다. 그 1건을 구 버전에서 저장하면 `"cron"`으로 덮인다 — **전체 task 손실보다 낫다**는 교환이다.

### 8.4 마이그레이션 표

| 파일 상태 | 읽기 결과 | 다음 저장 시 |
|---|---|---|
| 레거시(`scheduleMode` 키 없음) | `Interval`, 신규 필드 `None` | `"scheduleMode":"interval"` 명시(기존 동작) |
| `fixedTime` + `days:[0..6]`(현행 FE의 "매일") | 그대로 | FE가 매일 탭으로 열고 `days:[]`로 저장(동치, §2.3) |
| `fixedTime` + `days:[2,4]` | 그대로 | 그대로(매주 탭) |
| `hourly` + `hourlyMinute:30` | `Hourly`, `Some(30)` | `"scheduleMode":"hourly","hourlyMinute":30` |
| `hourly` + `hourlyMinute:75`(손편집) | `Hourly`, `None` → 영구 미실행 | 키가 빠진 채 저장(fail-closed 유지) |
| `cron` + 유효 표현식 | `Cron`, `Some(정규화형)` | `"cron":"0 9 * * 1-5"` |
| `cron` + 깨진 표현식(손편집) | `Cron`, `None` → 영구 미실행 | 키가 빠진 채 저장 |
| 미지 값(미래 버전 파일을 이번 버전이 읽음) | **`Cron` + 표현식 없음**(§8.3) — **다른 task는 전부 보존** | `"cron"`으로 덮여 쓰인다(감수) |
| `manual`(초판 설계가 유출됐을 경우) | 미지 값 취급 → `Cron` + 표현식 없음 | 위와 동일 |

### 8.5 모드별 JSON 예시 (완결성 — 프론트/손편집 계약)

```jsonc
// 상대간격 — 무변경
{ "id":"a","name":"빌드 점검","prompt":"…","interval":30,"scheduleMode":"interval","enabled":true }

// 매시간(신규) — 매시 30분
{ "id":"b","name":"큐 확인","prompt":"…","interval":30,"scheduleMode":"hourly","hourlyMinute":30,"enabled":true }

// 매일 — days 생략(= 빈 배열 = 매일). `skip_serializing_if="Vec::is_empty"`라 키 자체가 없다.
{ "id":"c","name":"야간 요약","prompt":"…","interval":30,"scheduleMode":"fixedTime","atTime":"09:00","enabled":true }

// 매주 — 화·목
{ "id":"d","name":"주간 점검","prompt":"…","interval":30,"scheduleMode":"fixedTime","atTime":"09:00","days":[2,4],"enabled":true }

// 사용자 지정(신규)
{ "id":"e","name":"평일 아침","prompt":"…","interval":30,"scheduleMode":"cron","cron":"0 9 * * 1-5","enabled":true }
```

`interval`이 모든 예시에 남아 있는 것은 의도다(config.rs:52-58의 기존 계약).

---

## 9. 테스트 (회귀 기준)

기존 `schedule.rs` 24건·`config.rs` 21건·`mod.rs` 4건·`scheduler.rs` 7건은 **`normalize_sorts_dedups_and_drops_out_of_range_days` 픽스처 1건(§8.2)과 구조체 리터럴 기계적 수정(§6.3)을 제외하고 한 건도 깨지지 않아야 한다.**

**신규 필수 (15건)**

| # | 대상 | 단언 |
|---|---|---|
| T1 | `next_hourly_in` + `FixedOffset` 주입 | `minute=30` @08:00 KST → 같은 날 08:30. CI 러너 tz와 무관하게 결정적 |
| T2 | `next_hourly_in` 시 경계 | `minute=30` @08:45 → **09:30**(현재 시 라벨이 이미 지났으면 다음 시로) |
| T3 | **엄격 `>` 회귀 (Hourly, 최우선)** | `minute=30` @08:30:00 **정각** → 09:30. 완료 직후 재계산이 같은 회차를 다시 잡아 무한 재실행하는 것을 막는다(FixedTime `next_occurrence_excludes_exact_same_instant`와 동일 취지) |
| T4 | `next_hourly_in` 자정 넘김 | `minute=30` @23:45 → 익일 00:30 |
| T5 | `initial_next_run_at(Hourly)` 따라잡기 | grace(30분) 창 안에 지나간 회차 → `startup_floor`(= `now+3분`) 한 점. grace 밖이면 다음 시 회차 |
| T6 | `next_run_after_finish(Hourly)` | **프로덕션 사슬**(`ScheduleSnapshot → next_hourly → Local`) 통과 + `> finished_at` + `<= finished_at + 1시간`. 리뷰 m2가 지적한 "분기 직접 커버리지 0" 반복 방지 |
| T7 | `next_run_after_finish(Cron)` | 같은 취지로 Cron 사슬 통과 + `> finished_at` |
| T8 | `next_cron_in` 3종 | 정상 해석 / 엄격 `>`(정각 입력 → 다음 회차) / 무효 표현식 → `None`(패닉 없음) |
| T9 | §4.5 불변식 | `*/5 * * * *`가 grace 창을 6회 지나쳤어도 등록 시 반환은 **1개 인스턴트** = `startup_floor` |
| T10 | `advance_until_strictly_after` | (a) 첫 호출이 과거 값을 주는 스텁 → 두 번째 앵커 값 반환, (b) 계속 과거만 주는 스텁 → 3회 후 `None`(fail-closed, 무한루프 없음) |
| T11 | **`is_wall_clock` 경유 S1/S2** | `should_reschedule_on_save`: Hourly→Hourly(분만 변경)·Cron→Cron(표현식만 변경)이 `true`, Interval→Interval은 여전히 `false`. `autonomy_set_enabled` 판정식도 동일 술어를 쓰는지 |
| T12 | `normalize_task` fail-closed | 깨진 cron / `hourlyMinute=75` → 각각 `None`이면서 **모드는 유지**(다운그레이드 금지) |
| T13 | `normalize_task` 신규 A | Hourly task의 `atTime`/`days` 잔여값이 제거되고, FixedTime task의 `days`는 보존되는지 |
| T14 | **미지 모드 흡수**(§8.3) | 미지 `scheduleMode` 1건이 섞인 JSON을 읽어도 `tasks`가 **전부 살아남고**, 그 1건이 `Cron`+`cron:None`이며, **키가 없는 레거시 task는 여전히 `Interval`** |
| T15 | `validate_cron` 3분기 | 정상 `Ok` / 파싱 실패 문구 / `* * * * *` 거부 문구(문구에 `MIN_INTERVAL_MINUTES` 값이 박혀 나오는지) |

**권장 (3건)**: 신규 필드 JSON 라운드트립(`hourlyMinute` 0이 `skip_serializing_if`에 걸리지 않는지 — `Some(0)`은 직렬화돼야 한다), `ScheduleSnapshot::from`이 두 신규 필드를 접근자 경유로 채우는지, `select_due`가 `next_run_at: None`을 후보에서 빼는지(scheduler.rs:81 가정의 명시적 고정).

**DST 테스트의 정직한 한계**: `FixedOffset`으로는 DST를 재현할 수 없다(schedule.rs:9-11이 이미 적어 둔 사실). §4.3 표의 DST 행들은 이번 설계에서 `chrono-tz` 스크래치 바이너리로 실측했지만, 리포지토리 테스트로 상시화하려면 `chrono-tz`를 `[dev-dependencies]`에 추가해야 한다(런타임 바이너리에는 안 들어가지만 tz 데이터베이스만큼 빌드가 무거워진다). **이번 스코프에서는 추가하지 않고**, 대신 (a) `pick()` 단위 테스트 3건(기존)이 DST 3분기를 값으로 고정하고, (b) T10이 cron 단조성 가드를 tz 없이 값으로 검증하며, (c) Hourly는 §4.4의 두 보조정리를 코드 주석으로 남겨 재현 경로를 문서화한다. 이 절충을 §11에 미검증으로 남긴다.

---

## 10. 파일별 변경 요약 · 구현 순서

| 파일 | 변경 |
|---|---|
| `src-tauri/Cargo.toml` | `cron-parser = "0.11"` 추가(45행 뒤), `rust-version` 1.81→1.85(11행) |
| `autonomy/config.rs` | enum variant 2개 추가(:37-43) + 수동 `Deserialize`(§8.3) + `is_wall_clock()`(§6.2) / 필드 2개 추가(:73 뒤) / 접근자 2개 신설(:90 옆) / `normalize_task` 신규 A·B 블록(:143 뒤) / 테스트 픽스처 2건 수정 |
| `autonomy/schedule.rs` | `ScheduleSnapshot` 필드 2개(:36-50) / `resolve_local_naive` 추출 리팩터(:101-111) / `next_hourly_in`·`next_hourly`·`next_cron_in`·`next_cron`·`advance_until_strictly_after`·`validate_cron` 신설 / **match 3곳에 팔 2개씩(:153·:191·:206)** / 상단 불변식 주석 갱신(:13-18) / `pick()` 옆에 §4.3 비대칭 표 주석 / 테스트 리터럴 3곳 |
| `autonomy/mod.rs` | `should_reschedule_on_save` 조건을 `is_wall_clock()`로 교체(:63) / `autonomy_set_enabled` 동일 교체(:195) / `autonomy_save_task`에 L1 검증 삽입(:137~:144 사이) |
| `autonomy/scheduler.rs` · `runner.rs` · `runtime.rs` | **무변경** (초판의 `Manual` 가드가 사라지면서 `select_due`의 "모드를 모른다" 절약이 100% 보존됨) |
| `capabilities/default.json` | **무변경** (신규 커맨드 0개) |
| `src/**`(프론트) | §2.2·§2.3·§6.4·§7.4 계약 — 이번 범위 밖(frontend-dev) |

**구현 순서** (이 순서가 아니면 중간 단계가 컴파일되지 않거나 위험 구간이 생긴다)

1. `Cargo.toml`(crate + MSRV) → `cargo build`로 의존성만 먼저 확인.
2. `config.rs`: enum 확장 + 수동 `Deserialize` + `is_wall_clock` + 필드 2개 + 접근자 2개 + `normalize_task`. → 이 시점에 **컴파일 에러가 §6.1의 match 3곳 + §6.3의 리터럴 4곳으로 정확히 나오는 것이 정상**이다(목록과 다르면 이 설계가 놓친 지점이 있다는 신호).
3. `schedule.rs`: `resolve_local_naive` 추출 → `next_hourly_*`(의존성 없는 쪽 먼저) → `advance_until_strictly_after` → `next_cron_*` → `validate_cron` → match 3곳 + 리터럴. T1~T10, T12·T13 작성.
4. `mod.rs`: S1·S2 교체 + L1 검증. T11·T15 작성.
5. T14 + 나머지 → `cargo test` 전체 통과 확인.
6. (별도) 프론트 §2.2·§2.3·§6.4.

---

## 11. 남은 리스크 · 미검증 (정직 보고)

- **현재 배포판 ↔ 이번 버전의 1회성 파일 손실 위험은 코드로 막을 수 없다**(§8.3-1). 릴리스 노트·전원 갱신이라는 운영 조치가 필요하며, 이는 PM 판단 사항이다. §8.3-1 말미의 `read_autonomy_file_checked` 권고(선택, 소규모)를 채택할지도 함께 정해야 한다.
- **DST 경로는 스크래치 실행으로 실측했으나 리포지토리 테스트로 상시화하지 않는다**(§9 말미). Hourly의 두 보조정리(§4.4)는 **증명이지 실행 검증이 아니다** — `chrono-tz`를 dev-dependency로 넣는 순간 T1~T6을 실제 DST tz로 재현할 수 있으므로, 여유가 생기면 그 추가를 권한다.
- **`validate_cron`의 10회 샘플링은 상한 증명이 아니다**(§7.3-2). 먼 미래에만 짧은 간격이 있는 표현식은 통과한다.
- **미지 모드 → `Cron` 흡수는 원래 값을 보존하지 않는다**(§8.4 마지막 행).
- **cron-parser의 dom·dow AND 의미**(Vixie는 OR)는 도움말 문구로만 고지한다. 사용자가 Vixie 습관대로 쓰면 예상보다 **덜** 실행된다(과잉 실행 방향이 아니라는 점에서 안전한 쪽 오차).
- **`MISSED_RUN_GRACE_MINUTES`의 안전 여유가 48배에서 2배로 줄었다**(§4.5). 값 자체는 그대로지만, 이 상수를 만지는 다음 사람이 60을 넘기면 Hourly의 "따라잡기 최대 1회"가 첫 번째 근거를 잃는다 — 테스트 강화(24*60→60)가 그 방어선이다.
- **BSD-3-Clause 고지 절차**가 이 저장소에 없다(§3.3 마지막). 이번 구현을 막지는 않지만 첫 서드파티 편입이므로 별도 이슈 권고.
- **Windows에서 빌드해 보지 않았다.** cron-parser는 OS 의존 코드가 없고(std + chrono만) `chrono::Local`은 이미 이 저장소가 쓰는 API지만, CI의 Windows 잡 통과 여부는 구현 후 확인 사항이다.
- **UI 2단 탭의 시각 설계는 이 문서 범위 밖**이다. 백엔드가 보장하는 것은 §2.2 매핑표·§2.3 역매핑 규칙·§7.4 에러 문구까지이며, 탭 컴포넌트 구조·상태 보존(탭 전환 시 입력값 유지 여부)은 frontend-dev 판단이다.
