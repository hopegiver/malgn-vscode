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
//   1) 이전 LTS 패치 설치
//   2) (P2-3b) 앱 실행 경로(`actions::spawn_plan_process` + install 후보 해석)로 `npm i -g pnpm` —
//      npm_config_prefix 주입으로 npm-global에 설치되는지, `npm prefix -g` 값, Manual 강등 없음
//   3) (P2-3c) 앱 업데이트 경로로 최신 LTS 갱신: 설치방식 분류(appManagedNode) → 업데이트 계획 →
//      새 LTS 미리보기(plan_id에 대상 버전 바인딩) → 실행 — current 전환 + 이전 버전 한 단계 보존,
//      npm-global 패키지 생존
//   4) 같은 버전 재실행은 다운로드 없이 멱등(미리보기도 "이미 최신"으로 will_run=false)
//   5) current/bin/node --version 일치, 셸 rc·~/.npmrc 미생성

use super::types::NodeVersion;
use super::{install_version, resolve_latest, InstallEnv, Layout, NodeArch, RealEnv};
use crate::dev_tools::actions::spawn_plan_process;
use crate::dev_tools::app_node;
use crate::dev_tools::classify::{describe_install_method, InstallMethod};
use crate::dev_tools::install_resolver::{resolve_plan, ResolvedAction};
use crate::dev_tools::install_resolver::{resolve_install_plan, InstallResolution, ResolvedRunners};
use crate::dev_tools::plan_table::{Runner, RunPlan};
use crate::dev_tools::runners::install_prefix_writable_with;
use crate::dev_tools::ToolId;
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

    // 1-b) 앱 실행 경로로 npm 연쇄 설치(P2-3b). 러너는 앱 관리 npm만 있는 상태를 흉내 낸다
    //      (이 PC에는 Homebrew npm이 따로 있어 후보 해석을 그대로 쓰면 그쪽이 잡히므로 주입한다).
    let npm_path = layout.node_bin_dir().join("npm").to_string_lossy().to_string();
    assert!(install_prefix_writable_with(Some(&layout), ToolId::Pnpm, Runner::Npm, &npm_path));
    let runners = ResolvedRunners {
        brew: None,
        npm: Some(npm_path.clone()),
        pnpm: None,
        winget: None,
        app_node: Some(layout.node_root().to_string_lossy().to_string()),
    };
    let prefix_plan = RunPlan { runner: Runner::Npm, args: &[], preview_args: None, env: &[], timeout_secs: 120 };
    let out = spawn_plan_process(&npm_path, &["prefix".to_string(), "-g".to_string()], &prefix_plan);
    println!("=== [1b] npm prefix -g (앱 주입 env) → {:?} (exit {:?})", out.stdout.trim(), out.exit_code);
    // npm은 UUID처럼 보이는 경로 조각(이 샌드박스 경로에는 세션 ID가 들어 있다)을 출력에서 `***`로
    // 가린다 — 값 전체가 아니라 끝부분으로 대조하고, 실제 효과(설치 위치)는 아래에서 파일로 확인한다.
    assert!(out.stdout.trim().ends_with(".local/share/malgn-agent/npm-global"), "{}", out.stdout);
    let (_, plain_prefix, _) = run_clean(
        std::path::Path::new(&npm_path),
        &["prefix", "-g"],
        &home,
        &layout.node_bin_dir().to_string_lossy(),
    );
    println!("=== [1b] (대조) env 주입 없이 npm prefix -g → {plain_prefix:?}");
    assert!(plain_prefix.contains("/node/"), "주입이 없으면 버전 폴더 안이어야 합니다: {plain_prefix}");

    match resolve_install_plan(ToolId::Pnpm, &runners) {
        InstallResolution::Run { runner_path, argv, plan, .. } => {
            assert_eq!(runner_path, npm_path);
            println!("=== [1b] install 계획: {runner_path} {argv:?}");
            let out = spawn_plan_process(&runner_path, &argv, &plan);
            println!("=== [1b] npm install -g pnpm → exit {:?} ({:?})\n{}", out.exit_code, out.duration, out.stdout.trim());
            assert_eq!(out.exit_code, Some(0), "stderr: {}", out.stderr);
        }
        InstallResolution::Manual(mp) => panic!("앱 npm이 있으면 Manual로 강등되면 안 됩니다: {}", mp.message_ko),
    }
    let pnpm_bin = layout.npm_global_bin().join("pnpm");
    assert!(pnpm_bin.exists(), "pnpm이 npm-global/bin에 설치돼야 합니다");
    assert!(!layout.version_dir(older).join("lib").join("node_modules").join("pnpm").exists(), "버전 폴더 안에 설치되면 안 됩니다");
    let path_env = crate::dev_tools::build_child_path_env(Some(&pnpm_bin.to_string_lossy()));
    let (code, out, err) = run_clean(&pnpm_bin, &["--version"], &home, &path_env);
    println!("=== [1b] npm-global/bin/pnpm --version (앱 PATH) → code={code} out={out:?} err={err:?}");
    assert_eq!(code, 0, "앱 PATH에 node가 있어야 shim이 동작합니다");

    // 2) 앱 업데이트 경로(P2-3c)로 최신 LTS 갱신.
    let node_current = layout.node_bin_dir().join("node").to_string_lossy().to_string();
    let resolved = resolve_plan(ToolId::Node, &node_current);
    println!("=== [2] 설치방식 분류 = {:?}", resolved.method);
    assert_eq!(resolved.method, InstallMethod::AppManagedNode);
    assert_eq!(describe_install_method(&resolved.method), "appManagedNode");
    let (runner_path, update_plan) = match resolved.action {
        ResolvedAction::Run { runner_path, plan } => (runner_path, plan),
        ResolvedAction::Manual(mp) => panic!("앱이 설치한 Node는 Manual이면 안 됩니다: {}", mp.message_ko),
    };
    assert_eq!(update_plan.runner, Runner::AppNode);
    let update_args: Vec<String> = update_plan.args.iter().map(|a| match a {
        crate::dev_tools::plan_table::Arg::Lit(s) => s.to_string(),
        other => panic!("리터럴 인자만 허용: {other:?}"),
    }).collect();
    let preview = app_node::build_update_preview(
        crate::dev_tools::tool_definition(ToolId::Node),
        &runner_path,
        &update_args,
        older.to_tag().trim_start_matches('v'),
    )
    .expect("업데이트 미리보기");
    println!("=== [2] 미리보기: will_run={} reliable={} notes={}", preview.will_run, preview.preview_reliable, preview.notes);
    assert!(preview.will_run, "이전 버전이 설치돼 있으면 새 LTS 갱신이 가능해야 합니다");
    assert!(preview.notes.contains(&latest.to_tag()));
    let started = std::time::Instant::now();
    let run = app_node::run_pinned(&latest.to_tag());
    println!("=== [2] 갱신 실행 {:?} exit={:?}\n{}", started.elapsed(), run.exit_code, run.stdout);
    assert_eq!(run.exit_code, Some(0), "stderr: {}", run.stderr);
    assert!(run.stdout.contains("한 단계 보관"), "이전 버전 보존 안내가 있어야 합니다");
    assert_eq!(std::fs::read_link(layout.current_link()).unwrap().to_string_lossy(), latest.to_tag());
    assert!(layout.version_dir(older).join("bin").join("node").exists(), "이전 버전은 한 단계 남아야 합니다");

    // 2-b) Node를 갱신해도 npm-global 패키지가 살아남고 새 node로 실행된다.
    assert!(pnpm_bin.exists());
    let (code, out, err) = run_clean(&pnpm_bin, &["--version"], &home, &path_env);
    println!("=== [2b] Node 갱신 후 pnpm --version → code={code} out={out:?} err={err:?}");
    assert_eq!(code, 0);

    // 3) 멱등 재실행: 다운로드 없이 빠르게 끝나야 한다.
    let preview_again = app_node::build_update_preview(
        crate::dev_tools::tool_definition(ToolId::Node),
        &runner_path,
        &update_args,
        latest.to_tag().trim_start_matches('v'),
    )
    .expect("재미리보기");
    println!("=== [3] 최신 상태 미리보기: will_run={} notes={}", preview_again.will_run, preview_again.notes);
    assert!(!preview_again.will_run);
    let started = std::time::Instant::now();
    let r3 = install_version(&env, &layout, arch, latest, &cancel).expect("재실행");
    let rerun = started.elapsed();
    println!("=== [3] 재실행 {:?} already_installed={}\n{}", rerun, r3.already_installed, r3.log.join("\n"));
    assert!(r3.already_installed);
    assert!(rerun < std::time::Duration::from_secs(20), "재실행이 다운로드를 한 것 같습니다: {rerun:?}");

    // 4) 실제 실행: current/bin/node --version.
    let bin = layout.current_link().join("bin");
    let (code, out, err) = run_clean(&bin.join("node"), &["--version"], &home, &bin.to_string_lossy());
    println!("=== [4] current/bin/node --version → code={code} out={out:?} err={err:?}");
    assert_eq!(code, 0);
    assert_eq!(out, latest.to_tag());

    // 셸 rc·~/.npmrc 불변: 샌드박스 홈에 만들어지지 않았다.
    for rc in [".zshrc", ".zprofile", ".zshenv", ".bashrc", ".bash_profile", ".profile", ".npmrc"] {
        assert!(!home.join(rc).exists(), "{rc}가 생겼습니다");
    }

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
