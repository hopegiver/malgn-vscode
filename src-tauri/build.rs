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

    tauri_build::build()
}
