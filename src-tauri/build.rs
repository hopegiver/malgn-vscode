fn main() {
    // 로컬 개발 편의용: src-tauri/.env(gitignored, 커밋 대상 아님)에 있는 값을
    // 빌드 프로세스의 환경변수로 로드한다. 파일이 없어도(CI 등) 에러 없이 조용히
    // 넘어간다 — CI(GitHub Actions)는 repo secret을 이미 job env로 주입하므로 이
    // 파일이 필요 없다. CARGO_MANIFEST_DIR을 쓰는 이유는 cargo가 build script를
    // 어떤 작업 디렉터리에서 실행하든 항상 이 크레이트 루트(src-tauri/)를 가리키기
    // 때문이다.
    let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    println!("cargo:rerun-if-changed={}", env_path.display());
    let _ = dotenvy::from_path(&env_path);

    // 이 값이 존재하면(.env 또는 CI 환경변수) 컴파일 타임 상수로 굳혀서
    // src/lib.rs의 option_env!("GOOGLE_OAUTH_CLIENT_SECRET")가 읽을 수 있게 한다.
    // 값이 없으면 아무것도 내보내지 않고, lib.rs 쪽이 None을 받아 런타임에 명확한
    // 에러로 처리한다(패닉 금지).
    if let Ok(secret) = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET") {
        println!("cargo:rustc-env=GOOGLE_OAUTH_CLIENT_SECRET={secret}");
    }

    // MALGN_OTEL_COLLECTOR_BASE(otel_settings.rs의 사내 collector 기본 주소)도
    // 같은 방식으로 주입한다. 값이 없으면(포크·secret 없는 CI) None으로 남아
    // endpoint_defaults_injected: false가 되고, UI가 빈 값 + placeholder로 렌더한다.
    if let Ok(otel_base) = std::env::var("MALGN_OTEL_COLLECTOR_BASE") {
        println!("cargo:rustc-env=MALGN_OTEL_COLLECTOR_BASE={otel_base}");
    }

    // GOOGLE_MCP_OAUTH_CLIENT_ID/SECRET(mcp_manager/catalog.rs의 gmail/
    // google-drive/google-calendar 카탈로그 3개 항목이 공유)도 같은 방식으로
    // 주입한다. Google Cloud OAuth 클라이언트는 API별이 아니라 애플리케이션
    // 단위라 이 세 MCP 엔드포인트가 클라이언트 하나를 그대로 재사용한다
    // (해당 GCP 프로젝트에서 Gmail/Drive/Calendar API 활성화 및 동의화면
    // 스코프 등록은 사용자가 GCP 콘솔에서 처리하는 부분 — 이 코드는 관여하지
    // 않는다). 이름은 앱 자체 Google 로그인용 `GOOGLE_OAUTH_CLIENT_SECRET`과
    // 헷갈리지 않도록 `GOOGLE_MCP_OAUTH_*`로 구분했다.
    //
    // 이 세 엔드포인트(gmailmcp/drivemcp/calendarmcp.googleapis.com)는 RFC
    // 7591 동적 클라이언트 등록(DCR)을 지원하지 않아서, `claude mcp add`가
    // --client-id/--client-secret 없이 자동 DCR을 시도하면 "Incompatible
    // auth server: does not support dynamic client registration"으로
    // 실패한다. 값이 없으면(기본 상태) 카탈로그 원클릭 설치는 기존과 동일하게
    // 동작하되(회귀 없음), 이 세 항목 인증만 이 DCR 미지원 문제로 계속
    // 실패한다 — catalog.rs/mod.rs가 None을 받아 안내 메시지를 붙인다.
    if let Ok(client_id) = std::env::var("GOOGLE_MCP_OAUTH_CLIENT_ID") {
        println!("cargo:rustc-env=GOOGLE_MCP_OAUTH_CLIENT_ID={client_id}");
    }
    if let Ok(client_secret) = std::env::var("GOOGLE_MCP_OAUTH_CLIENT_SECRET") {
        println!("cargo:rustc-env=GOOGLE_MCP_OAUTH_CLIENT_SECRET={client_secret}");
    }

    // DEV_AUTO_LOGIN_EMAIL(로컬 개발 전용 자동 로그인 우회, dev_auto_login.rs)도
    // 같은 방식으로 주입한다. 값이 없으면(대부분의 환경) 위와 동일하게 조용히
    // 넘어간다. `.env` 값이 바뀌면 이 스크립트가 재실행되도록
    // rerun-if-env-changed도 함께 등록한다 — 커맨드라인/CI 환경변수로 값을 줄
    // 수도 있으므로 rerun-if-changed(.env 파일 경로)만으로는 부족하다.
    //
    // 릴리스 바이너리에 이 값이 남지 않는 실효 게이트는 단 하나다 —
    // dev_auto_login.rs의 `option_env!("DEV_AUTO_LOGIN_EMAIL")` 호출 자체가
    // `#[cfg(debug_assertions)]`로 감싸여 있어, 릴리스 컴파일에서는 그 줄이
    // 파싱조차 되지 않는다(rustc-env로 값을 넘겨도 읽어가는 코드가 없다).
    // **이 게이트는 절대 제거하지 말 것** — 아래 build.rs 쪽 PROFILE 분기는
    // 그것을 대신할 두 번째 독립 방어선이 아니다.
    //
    // 아래 `if profile != "release"` 분기는 "이중 방어"가 아니라 `.env` 경로에
    // 한정된 보조적 억제일 뿐이다 — 두 검증자가 각각 독립 크레이트로
    // 재현했듯이, `option_env!`는 `cargo:rustc-env`로 명시 주입된 값만이 아니라
    // **rustc가 상속한 환경변수 전체**를 읽는다. 따라서 셸에서
    // `export DEV_AUTO_LOGIN_EMAIL=...`을 해 둔 채로 `cargo build --release`를
    // 실행하면, 이 build.rs가 아래 조건 때문에 rustc-env를 내보내지 않아도
    // rustc는 상속받은 환경변수를 그대로 읽어 컴파일타임 상수에 반영한다 —
    // 즉 이 PROFILE 게이트는 완전히 우회된다. 릴리스 빌드가 그런 상황에서도
    // 여전히 안전한 유일한 이유는 위 `#[cfg(debug_assertions)]` 한 겹이다.
    println!("cargo:rerun-if-env-changed=DEV_AUTO_LOGIN_EMAIL");
    // PROFILE 값은 "dev|release"가 아니라 "debug|release"다(Cargo가 빌드
    // 스크립트에 넘기는 실제 값, 직접 확인함) — 아래 비교식은 release만
    // 가리므로 이 사실 정정과 무관하게 그대로 둔다.
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile != "release" {
        if let Ok(dev_email) = std::env::var("DEV_AUTO_LOGIN_EMAIL") {
            println!("cargo:rustc-env=DEV_AUTO_LOGIN_EMAIL={dev_email}");
        }
    }

    tauri_build::build()
}
