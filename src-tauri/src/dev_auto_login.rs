// ---------------- 로그인: 로컬 개발 전용 자동 로그인 우회 ----------------
// 개발자 본인 PC에서 `pnpm tauri dev`를 켤 때마다 Google OAuth 브라우저 플로우를
// 처음부터 다시 타지 않도록, 디버그 빌드 + 로컬 설정값이 있을 때만 로그인 화면을
// 건너뛰는 우회 경로다. `google_oauth/`(mod.rs·verify.rs)의 검증 로직은 이 기능
// 때문에 전혀 건드리지 않았다 — 이 파일은 그 모듈과 완전히 독립적이고, 프런트
// 로그인 화면(views/login.ts)이 기대하는 결과 모양(email/name)만 흉내 낸다.
//
// 값 주입 방식은 이 저장소의 기존 관례(GOOGLE_OAUTH_CLIENT_SECRET, build.rs가
// `src-tauri/.env`를 dotenvy로 읽어 `cargo:rustc-env`로 넘기고 `option_env!()`가
// 받는 패턴)를 그대로 따른다 — 새 키 이름은 `DEV_AUTO_LOGIN_EMAIL`.
// `src-tauri/.env`에 아래 한 줄을 추가하면 된다(실제 값은 각자 채운다):
//   DEV_AUTO_LOGIN_EMAIL=you@malgnsoft.com
// ⚠️ 이 값은 절대 CI secret / GitHub Actions에 넣지 말 것 — 로컬 전용 개발
// 편의 기능이고, CI(`ci.yml`)와 릴리스 빌드(`tauri-portable-build.yml`)는 이
// 값이 없는 채로 도는 것이 정상이며 그래야 한다.
//
// 릴리스 빌드에서 이 우회가 절대 실행될 수 없는 이유(컴파일 메커니즘, 런타임
// 분기가 아니다):
//   `DEV_AUTO_LOGIN_EMAIL` 상수와 그 값을 실제로 읽어 결과를 만드는 분기 전체가
//   `#[cfg(debug_assertions)]`로 감싸여 있다. `#[cfg(...)]`는 컴파일 타임에
//   조건이 거짓인 코드 블록을 애초에 파싱·컴파일하지 않는다(런타임 if로 숨겨진
//   죽은 코드가 아니라, 그 자리에 아무 명령어도 생성되지 않는다). cargo의 기본
//   release 프로파일은 `debug-assertions = false`이고, `src-tauri/Cargo.toml`의
//   `[profile.release]`(codegen-units/lto/opt-level/panic/strip)는 이 필드를
//   오버라이드하지 않는다(직접 확인함 — 이 파일 작성 시점의 Cargo.toml 참고).
//   따라서 `cargo build --release`/`pnpm tauri build` 산출물에는
//   `DEV_AUTO_LOGIN_EMAIL`을 읽는 코드도, 그 문자열이 컴파일타임에 박힐 자리도
//   없다 — 어떤 런타임 환경변수·커맨드라인 플래그로도 켤 수 있는 문이 아예
//   존재하지 않는다. `dev_auto_login()` 커맨드 자체는 릴리스 빌드에도 등록되어
//   있지만(프런트가 매번 호출해도 IPC 에러 없이 동작하도록), 그 본문은
//   `#[cfg(not(debug_assertions))]` 분기만 컴파일되어 무조건 `None`을 반환한다.

#[cfg(debug_assertions)]
const DEV_AUTO_LOGIN_EMAIL: Option<&str> = option_env!("DEV_AUTO_LOGIN_EMAIL");

/// 로그인 화면(views/login.ts)이 실제 Google 로그인 성공 시 받는 모양(email/name)만
/// 맞춘다 — `google_oauth::GoogleLoginResult`는 재사용하지 않는다(필드가 비공개라
/// 이 모듈에서 만들 수 없고, `google_oauth/mod.rs`는 애초에 손대지 않기로 했다).
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
pub struct DevAutoLoginResult {
    email: String,
    name: String,
}

/// 실제 OAuth 경로(`google_oauth/mod.rs`)와 같은 도메인 문자열이다. 상수를
/// 공유하지 않는 이유: 그 모듈은 이 기능 때문에 건드리지 않기로 했고, 이 값은
/// `&str` 리터럴 하나라 드리프트 위험이 사실상 없다(바뀌면 두 파일 모두 고쳐야
/// 하는데, 도메인 자체가 바뀌는 일은 거의 없다).
///
/// `build_dev_auto_login_result`와 동일하게 `cfg(any(debug_assertions, test))`로
/// 감싼다 — 릴리스(비테스트) 빌드에서는 그 함수 자체가 컴파일되지 않아 이 상수를
/// 참조하는 코드가 없고, 감싸지 않으면 "constant is never used" 경고가 새로
/// 생긴다(`cargo check --release`로 실측).
#[cfg(any(debug_assertions, test))]
const DEV_AUTO_LOGIN_ALLOWED_DOMAIN_SUFFIX: &str = "@malgnsoft.com";

/// 순수 변환 함수 — cfg 플래그와 무관하게 항상 컴파일되므로 디버그/릴리스
/// 어느 프로필의 `cargo test`로도 검증할 수 있다. 이메일 로컬파트를 표시 이름으로
/// 쓴다(실제 이름 조회 API를 부르지 않는다 — 로컬 개발 편의 기능에 그 정도
/// 정확도까지는 필요 없다).
///
/// `@malgnsoft.com`으로 끝나는 값만 통과시킨다 — 실제 Google OAuth 경로가
/// `hd=malgnsoft.com`으로 강제하는 것과 같은 불변식이다. 이 값은 OTel 자동
/// 설정(`ensureOtelAutoConfigured` → `views/settings.ts`의 identity 조립 →
/// `~/.claude/settings.json`의 `OTEL_RESOURCE_ATTRIBUTES` employee.* 필드)에
/// 그대로 흘러 들어가 사내 텔레메트리 귀속으로 쓰이므로, 임의 문자열이 아니라
/// 실제 사내 계정이어야 한다. 도메인이 아니면 `None`을 반환해 프런트가 기존
/// 로그인 화면으로 자연스럽게 떨어지게 한다(우회 경로를 쓰지 않는 것과 동일한
/// 취급).
///
/// 릴리스(비테스트) 빌드에서는 `dev_auto_login()`의 `#[cfg(not(debug_assertions))]`
/// 분기만 컴파일되어 이 함수를 호출하지 않는다 — 감싸지 않으면 "function is never
/// used" 경고가 새로 생긴다(`cargo check --release`로 실측, F4). `cargo test
/// --release`에서는 `cfg(test)`가 참이라 아래 테스트들이 여전히 이 함수를 직접
/// 호출해 검증한다.
#[cfg(any(debug_assertions, test))]
fn build_dev_auto_login_result(raw_email: &str) -> Option<DevAutoLoginResult> {
    let email = raw_email.trim();
    if email.is_empty() {
        return None;
    }
    if !email
        .to_ascii_lowercase()
        .ends_with(DEV_AUTO_LOGIN_ALLOWED_DOMAIN_SUFFIX)
    {
        return None;
    }
    let name = email.split('@').next().unwrap_or(email).to_string();
    Some(DevAutoLoginResult {
        email: email.to_string(),
        name,
    })
}

/// 프런트가 앱 시작 시 한 번 호출한다(Google 로그인 버튼을 누르기 전에). `Some`이면
/// 로그인 화면을 건너뛰고 바로 이 값으로 인증 상태를 채운다 — 브라우저를 전혀
/// 열지 않는다. `None`이면 "이 경로를 쓰지 않는다"는 뜻이고(릴리스 빌드이거나,
/// 디버그 빌드라도 `.env`에 값이 없거나 값이 `@malgnsoft.com` 도메인이 아니면)
/// 프런트는 기존 Google 로그인 화면을 그대로 보여준다.
///
/// 이 함수 자체(브라우저를 열지 않고 네트워크 호출도 하지 않는다)는 부작용이
/// 없지만, **`Some`을 반환해 인증 상태가 세워진 뒤의 기능 전체는 그렇지 않다** —
/// `state.authenticated = true`가 되는 순간 `main.ts`의 `ensureOtelAutoConfigured()`
/// 게이트를 실제 로그인과 동일하게 통과하고, `views/settings.ts`가 이 이메일로
/// `~/.claude/settings.json`의 `OTEL_RESOURCE_ATTRIBUTES`(employee.* 등 사내
/// 텔레메트리 귀속)를 실제로 파일에 기록한다. 위 도메인 검증이 바로 이 부작용
/// 때문에 필요하다 — 우회 경로로 들어온 이메일이 실제 사내 계정이어야 한다.
#[tauri::command]
pub fn dev_auto_login() -> Option<DevAutoLoginResult> {
    #[cfg(debug_assertions)]
    {
        return build_dev_auto_login_result(DEV_AUTO_LOGIN_EMAIL?);
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_result_from_valid_email() {
        let result = build_dev_auto_login_result("dev@malgnsoft.com")
            .expect("유효한 이메일은 Some이어야 합니다");
        assert_eq!(result.email, "dev@malgnsoft.com");
        assert_eq!(result.name, "dev");
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let result = build_dev_auto_login_result("  dev@malgnsoft.com  ")
            .expect("공백은 trim된 뒤 유효해야 합니다");
        assert_eq!(result.email, "dev@malgnsoft.com");
    }

    #[test]
    fn rejects_empty_or_whitespace_only_email() {
        assert!(build_dev_auto_login_result("").is_none());
        assert!(build_dev_auto_login_result("   ").is_none());
    }

    #[test]
    fn rejects_value_without_malgnsoft_domain() {
        // F1: 이 결과가 OTel 사내 텔레메트리 귀속(employee.*)에 그대로 흘러
        // 들어가므로, 실제 OAuth 경로의 hd=malgnsoft.com 불변식과 동일한 검증을
        // 우회 경로에도 강제한다. '@' 자체가 없는 값도, 다른 도메인도 모두 거부.
        assert!(build_dev_auto_login_result("not-an-email").is_none());
        assert!(build_dev_auto_login_result("dev@gmail.com").is_none());
        assert!(build_dev_auto_login_result("dev@malgnsoft.com.evil.com").is_none());
    }

    #[test]
    fn accepts_malgnsoft_domain_case_insensitively() {
        // 실제 이메일 도메인은 대소문자를 구분하지 않는다 — 우회 경로도 같은
        // 관용도를 둔다.
        let result = build_dev_auto_login_result("Dev@MalgnSoft.com")
            .expect("대소문자만 다른 malgnsoft.com 도메인은 허용되어야 합니다");
        assert_eq!(result.email, "Dev@MalgnSoft.com");
    }

    // F3: 이 테스트의 두 분기는 가치가 다르다.
    //   - 디버그 분기는 `dev_auto_login()`의 본문을 그대로 재작성한
    //     동어반복이다 — `expected`를 유도하는 식이 실제 함수 본문
    //     (`DEV_AUTO_LOGIN_EMAIL?`를 `build_dev_auto_login_result`에 넘기는 것)과
    //     같으므로, 함수 구현이 스스로와 다를 수 없어 이 분기는 사실상 항상
    //     통과한다. 리팩터링 중 두 곳을 동시에 안 바꾸면 걸러지지 않는다는
    //     한계를 인정하고 남겨둔다(회귀 방지 가치는 낮지만 컴파일 유지 비용도
    //     낮다).
    //   - 릴리스 분기(`cargo test --release`)가 이 테스트의 실제 가치다 —
    //     `DEV_AUTO_LOGIN_EMAIL` 설정 여부와 무관하게 `dev_auto_login()`이
    //     항상 `None`인지, 즉 릴리스 배제 불변식 자체를 검증한다. `ci.yml`의
    //     rust-test 잡이 `cargo test --release`도 돌리게 되어(F3) 이 분기가
    //     비로소 CI에서 실행된다 — 이전에는 debug 프로필로만 돌아 이 분기가
    //     한 번도 컴파일조차 되지 않았다.
    #[test]
    fn command_matches_compile_time_configuration() {
        #[cfg(debug_assertions)]
        {
            let expected = DEV_AUTO_LOGIN_EMAIL.and_then(build_dev_auto_login_result);
            assert_eq!(
                dev_auto_login(),
                expected,
                "디버그 빌드에서는 DEV_AUTO_LOGIN_EMAIL 컴파일타임 상수와 결과가 항상 일치해야 합니다(동어반복 — 위 주석 참고)"
            );
        }
        #[cfg(not(debug_assertions))]
        {
            assert_eq!(
                dev_auto_login(),
                None,
                "릴리스 빌드에서는 DEV_AUTO_LOGIN_EMAIL 설정 여부와 무관하게 항상 None이어야 합니다"
            );
        }
    }
}
