// 실제 nodejs.org 왕복 검증(#[ignore] — 평소 `cargo test`에는 돌지 않는다).
//
// 이 PC의 실제 `~/.local/share`·셸 rc·`~/.npmrc`·전역 npm을 건드리지 않도록, 테스트 바이너리를
// `HOME=<scratch>/r22-sandbox/home`으로 직접 실행해야만 동작한다(HOME 경로에 `r22-sandbox`가
// 없으면 시작 즉시 패닉). 실행 방법:
//   cargo test --no-run                      # 테스트 바이너리 경로 확인
//   mkdir -p <scratch>/r22-sandbox/home
//   HOME=<scratch>/r22-sandbox/home <바이너리> sandbox_roundtrip --ignored --nocapture
//
// 검증 내용(실제 네트워크·실제 /usr/bin/tar·xattr·codesign·실제 Node 실행):
//   1) 이전 LTS 패치 설치 → 2) 최신 LTS로 갱신(current 전환 + 이전 버전 한 단계 보존) →
//   3) 같은 버전 재실행은 다운로드 없이 멱등 → 4) current/bin/node --version 일치.

use super::types::NodeVersion;
use super::{install_version, resolve_latest, InstallEnv, Layout, NodeArch, RealEnv};
use std::path::PathBuf;
use std::process::Command;

fn sandbox_home() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME");
    assert!(
        home.contains("r22-sandbox"),
        "이 테스트는 HOME에 'r22-sandbox'가 들어 있는 샌드박스에서만 실행합니다(현재 HOME={home})"
    );
    let from_dirs = dirs::home_dir().expect("home_dir");
    assert_eq!(from_dirs, PathBuf::from(&home), "dirs::home_dir()이 HOME과 달라 샌드박스를 보장할 수 없습니다");
    PathBuf::from(home)
}

fn run_clean(program: &std::path::Path, args: &[&str], home: &std::path::Path, extra_path: &str) -> (i32, String, String) {
    let out = Command::new(program)
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", format!("{extra_path}:/usr/bin:/bin"))
        .output()
        .expect("실행");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

/// index.json에서 최신보다 낮은 가장 높은 적격 버전(같은 규칙: LTS·osx-arm64/x64-tar·메이저 22+).
fn previous_eligible(env: &RealEnv, arch: NodeArch, latest: NodeVersion) -> NodeVersion {
    let mut body: Vec<u8> = Vec::new();
    env.http_get(&super::release::index_url(), 8 * 1024 * 1024, &mut body, &|| false).expect("index.json");
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let key = arch.index_files_key();
    parsed
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| {
            let v = NodeVersion::parse_tag(e.get("version")?.as_str()?)?;
            let lts = e.get("lts")?.as_str().map(|s| !s.is_empty()).unwrap_or(false);
            let has = e.get("files")?.as_array()?.iter().any(|f| f.as_str() == Some(key.as_str()));
            (lts && has && v.major >= 22 && v < latest).then_some(v)
        })
        .max()
        .expect("이전 적격 버전이 없습니다")
}

#[test]
#[ignore]
fn sandbox_roundtrip_real_nodejs_org() {
    let home = sandbox_home();
    let layout = Layout::for_home(&home);
    let env = RealEnv::new();
    let arch = NodeArch::current().expect("지원 CPU");
    let cancel = || false;

    println!("=== sandbox HOME = {}", home.display());
    let latest = resolve_latest(&env, arch, &cancel).expect("최신 LTS 결정");
    let older = previous_eligible(&env, arch, latest);
    println!("=== 최신 LTS = {}, 이전 적격 버전 = {}", latest.to_tag(), older.to_tag());

    // 1) 이전 버전 설치.
    let started = std::time::Instant::now();
    let r1 = install_version(&env, &layout, arch, older, &cancel).expect("이전 버전 설치");
    println!("=== [1] {} 설치 {:?}\n{}", older.to_tag(), started.elapsed(), r1.log.join("\n"));
    assert!(!r1.already_installed);
    assert_eq!(std::fs::read_link(layout.current_link()).unwrap().to_string_lossy(), older.to_tag());

    // 2) 최신으로 갱신.
    let started = std::time::Instant::now();
    let r2 = install_version(&env, &layout, arch, latest, &cancel).expect("최신 버전 설치");
    println!("=== [2] {} 갱신 {:?}\n{}", latest.to_tag(), started.elapsed(), r2.log.join("\n"));
    assert_eq!(r2.previous, Some(older));
    assert_eq!(std::fs::read_link(layout.current_link()).unwrap().to_string_lossy(), latest.to_tag());
    assert!(layout.version_dir(older).join("bin").join("node").exists(), "이전 버전은 한 단계 남아야 합니다");

    // 3) 멱등 재실행: 다운로드 없이 빠르게 끝나야 한다.
    let started = std::time::Instant::now();
    let r3 = install_version(&env, &layout, arch, latest, &cancel).expect("재실행");
    let rerun = started.elapsed();
    println!("=== [3] 재실행 {:?} already_installed={}\n{}", rerun, r3.already_installed, r3.log.join("\n"));
    assert!(r3.already_installed);
    assert!(rerun < std::time::Duration::from_secs(20), "재실행이 다운로드를 한 것 같습니다: {rerun:?}");

    // 4) 실제 실행: current/bin/node --version.
    let bin = layout.current_link().join("bin");
    let (code, out, err) = run_clean(&bin.join("node"), &["--version"], &home, "");
    println!("=== [4] current/bin/node --version → code={code} out={out:?} err={err:?}");
    assert_eq!(code, 0);
    assert_eq!(out, latest.to_tag());

    // 스테이징·임시 파일 잔여물 없음.
    let leftovers: Vec<String> = std::fs::read_dir(&layout.base)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with(".staging-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    println!("=== 기준 폴더 내용:");
    for e in std::fs::read_dir(&layout.base).unwrap().flatten() {
        println!("    {}", e.file_name().to_string_lossy());
    }
    for e in std::fs::read_dir(layout.node_root()).unwrap().flatten() {
        println!("    node/{}", e.file_name().to_string_lossy());
    }
}
