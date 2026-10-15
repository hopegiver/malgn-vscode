// 설치 파이프라인 실패 주입 테스트(macOS 전용 — 시스템 bsdtar·xattr와 심볼릭 링크를 실제로
// 사용한다). 네트워크·코드서명·rename·디스크 쓰기는 `FakeEnv`가 주입한 실패로 대체한다.
// 모든 실패 테스트는 같은 불변식으로 끝난다:
//   "기존 설치 보존(이전 버전 디렉터리·current·npm-global 불변) + 스테이징 잔여물 0 +
//    부분 설치(새 버전 디렉터리·임시 링크) 없음".

use super::archive::validate_tree;
use super::layout::{read_current_tag, InstallLock};
use super::testkit::*;
use super::types::{InstallError, NodeArch};
use super::resolve_latest;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const PRIOR: super::NodeVersion = super::NodeVersion { major: 24, minor: 21, patch: 0 };

fn assert_err<T: std::fmt::Debug>(r: Result<T, InstallError>, check: impl Fn(&InstallError) -> bool, what: &str) -> InstallError {
    match r {
        Err(e) if check(&e) => e,
        other => panic!("{what}: 기대한 오류가 아닙니다: {other:?}"),
    }
}

// ── 성공 경로 ──

#[test]
fn install_places_version_flips_current_and_follows_the_verification_order() {
    let fx = Fixture::new("ok");
    let report = fx.install_ok();
    assert_eq!(report.version, fx.version);
    assert!(!report.already_installed);
    assert_eq!(report.previous, None);

    // 배치 결과.
    let dir = fx.layout.version_dir(fx.version);
    assert!(dir.join("bin").join("node").is_file());
    assert_eq!(std::fs::read_link(fx.layout.current_link()).unwrap().to_string_lossy(), "v24.22.0");
    assert!(fx.layout.current_link().join("bin").join("npm").exists(), "current/bin/npm 링크가 풀려야 합니다");
    assert!(fx.layout.npm_global().is_dir(), "npm 전역 prefix 폴더");
    assert_eq!(fx.staging_leftovers(), Vec::<String>::new());

    // 검증 순서: 체크섬 → 다운로드 → (해시) → 목록 → 해제 → xattr → codesign 3종 → node --version → rename.
    let calls = fx.env.calls();
    let pos = |needle: &str| calls.iter().position(|c| c.contains(needle)).unwrap_or_else(|| panic!("{needle} 없음: {calls:?}"));
    assert!(pos("SHASUMS256.txt") < pos("darwin-arm64.tar.gz"));
    assert!(pos("darwin-arm64.tar.gz") < pos("RUN tar -tf"));
    assert!(pos("RUN tar -tf") < pos("RUN tar -xf"));
    assert!(pos("RUN tar -xf") < pos("RUN xattr"));
    assert!(pos("RUN xattr") < pos("RUN codesign --verify"));
    assert!(pos("RUN codesign --verify") < pos("RUN codesign -dvv"));
    assert!(pos("RUN codesign -dvv") < pos("RUN node --version"));
    // 마지막 codesign(-R 체인)은 node 실행 전이어야 한다.
    let last_codesign = calls.iter().rposition(|c| c.starts_with("RUN codesign")).unwrap();
    assert!(last_codesign < pos("RUN node --version"));
    assert!(pos("RUN node --version") < pos("RENAME v24.22.0"));
    assert!(pos("RENAME v24.22.0") < calls.iter().rposition(|c| c == "RENAME current").unwrap());
}

#[test]
fn install_from_previous_version_keeps_previous_one_step_and_flips_current() {
    let fx = Fixture::new("upgrade");
    fx.seed_prior_install(PRIOR);
    let report = fx.install_ok();
    assert_eq!(report.previous, Some(PRIOR));
    assert_eq!(read_current_tag(&fx.layout), Some(fx.version));
    // 이전 버전은 한 단계 보관되고(되돌리는 기능은 없다), 그보다 오래된 버전은 정리된다.
    assert!(fx.layout.version_dir(PRIOR).join("MARKER").is_file());
    assert_eq!(
        std::fs::read_to_string(fx.layout.npm_global().join("lib").join("KEEP")).unwrap(),
        "global-pkg",
        "npm 전역 폴더는 Node 업그레이드 후에도 그대로여야 합니다"
    );
}

#[test]
fn install_prunes_versions_older_than_the_previous_one() {
    let fx = Fixture::new("prune");
    fx.seed_prior_install(PRIOR);
    let ancient = fx.layout.version_dir(super::NodeVersion { major: 22, minor: 1, patch: 0 });
    std::fs::create_dir_all(&ancient).unwrap();
    let stray_file = fx.layout.node_root().join("not-a-version");
    std::fs::write(&stray_file, "x").unwrap();
    fx.install_ok();
    assert!(!ancient.exists(), "오래된 버전은 정리돼야 합니다");
    assert!(stray_file.exists(), "형식이 다른 항목은 건드리지 않습니다");
    assert!(fx.layout.version_dir(PRIOR).exists());
}

#[test]
fn rerun_of_same_version_is_idempotent_and_skips_download() {
    let fx = Fixture::new("idem");
    fx.install_ok();
    let gets_before = fx.env.count_prefix("GET ");
    let second = fx.install_ok();
    assert!(second.already_installed);
    assert_eq!(fx.env.count_prefix("GET "), gets_before, "이미 온전한 같은 버전은 다시 받지 않아야 합니다");
    assert_eq!(read_current_tag(&fx.layout), Some(fx.version));
    assert_eq!(fx.staging_leftovers(), Vec::<String>::new());
}

#[test]
fn rerun_repairs_current_link_when_healthy_version_dir_exists_but_current_is_missing() {
    let fx = Fixture::new("idem-link");
    fx.install_ok();
    std::fs::remove_file(fx.layout.current_link()).unwrap();
    let again = fx.install_ok();
    assert!(again.already_installed);
    assert_eq!(read_current_tag(&fx.layout), Some(fx.version));
}

#[test]
fn damaged_same_version_dir_is_replaced_atomically() {
    let fx = Fixture::new("repair");
    fx.install_ok();
    // 손상: node가 사라진 같은 버전 디렉터리.
    std::fs::remove_file(fx.layout.version_dir(fx.version).join("bin").join("node")).unwrap();
    let again = fx.install_ok();
    assert!(!again.already_installed, "손상본은 다시 설치해야 합니다");
    assert!(fx.layout.version_dir(fx.version).join("bin").join("node").is_file());
    assert_eq!(fx.staging_leftovers(), Vec::<String>::new());
}

#[test]
fn resolve_latest_uses_index_json_not_a_hardcoded_version() {
    let fx = Fixture::new("resolve");
    let v = resolve_latest(&fx.env, NodeArch::Arm64, &|| false).unwrap();
    assert_eq!(v.to_tag(), "v24.22.0");
    fx.env.route(
        &super::release::index_url(),
        Route::Body(index_json_for(&[("v24.30.1", "Krypton", true), ("v24.22.0", "Krypton", true)])),
    );
    assert_eq!(resolve_latest(&fx.env, NodeArch::Arm64, &|| false).unwrap().to_tag(), "v24.30.1");
}

// ── 해시 조작 ──

#[test]
fn hash_mismatch_tampered_tarball_aborts_before_any_extraction_or_execution() {
    let fx = Fixture::new("hash");
    fx.seed_prior_install(PRIOR);
    // 공식 체크섬은 그대로 두고 tarball 바이트를 조작한다.
    let mut tampered = fx.tar_bytes.clone();
    tampered[600] ^= 0xFF;
    fx.env.route(&super::release::tarball_url(fx.version, ARCH), Route::Body(tampered));

    let err = assert_err(fx.install(&|| false), |e| matches!(e, InstallError::HashMismatch { .. }), "hash_mismatch");
    assert!(err.to_string().contains("SHA256"));
    assert!(fx.env.ran_no_tools(), "해시 불일치 시 tar·xattr·codesign·node 어느 것도 실행되면 안 됩니다: {:?}", fx.env.calls());
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn hash_mismatch_manipulated_checksum_row_is_rejected_too() {
    let fx = Fixture::new("hash-sums");
    let name = super::release::tarball_name(fx.version, ARCH);
    fx.env.route(
        &super::release::shasums_url(fx.version),
        Route::Body(format!("{}  {name}\n", "ab".repeat(32)).into_bytes()),
    );
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::HashMismatch { .. }), "hash_mismatch(체크섬 조작)");
    assert!(fx.env.ran_no_tools());
    fx.assert_clean_first_install_failure();
}

#[test]
fn checksum_row_missing_for_the_tarball_aborts_before_download() {
    let fx = Fixture::new("sums-missing");
    fx.env.route(
        &super::release::shasums_url(fx.version),
        Route::Body(format!("{}  some-other-file.tar.gz\n", "ab".repeat(32)).into_bytes()),
    );
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::ChecksumMissing(_)), "체크섬 행 없음");
    assert_eq!(fx.env.count_prefix("GET ").saturating_sub(1), 0, "체크섬을 확인하지 못하면 tarball을 받지 않아야 합니다");
    assert!(!fx.env.calls().iter().any(|c| c.contains(".tar.gz")));
    fx.assert_clean_first_install_failure();
}

// ── 서명 / Team ID ──

#[test]
fn team_id_mismatch_aborts_before_running_node_and_preserves_prior_install() {
    let fx = Fixture::new("team");
    fx.seed_prior_install(PRIOR);
    *fx.env.team_id.lock().unwrap() = "ABCDE12345".to_string();

    let err = assert_err(fx.install(&|| false), |e| matches!(e, InstallError::TeamIdMismatch { .. }), "team_id_mismatch");
    assert!(err.to_string().contains("ABCDE12345"));
    assert!(!fx.env.ran_program("node --version"), "서명자 불일치 바이너리를 실행하면 안 됩니다");
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn team_id_missing_from_codesign_output_is_a_mismatch() {
    let fx = Fixture::new("team-none");
    *fx.env.team_id.lock().unwrap() = "not set".to_string();
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::TeamIdMismatch { .. }), "team_id 미설정");
    assert!(!fx.env.ran_program("node --version"));
    fx.assert_clean_first_install_failure();
}

#[test]
fn team_id_matching_but_apple_chain_requirement_failing_is_rejected() {
    // 자체 서명 인증서가 같은 OU를 달고 있는 경우 — TeamIdentifier 문자열만 보면 속는다.
    let fx = Fixture::new("team-chain");
    *fx.env.chain_ok.lock().unwrap() = false;
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::CodeSign(_)), "신뢰 체인 불일치");
    assert!(!fx.env.ran_program("node --version"));
    fx.assert_clean_first_install_failure();
}

#[test]
fn codesign_verify_failure_aborts_before_running_node() {
    let fx = Fixture::new("codesign");
    fx.seed_prior_install(PRIOR);
    *fx.env.codesign_verify_ok.lock().unwrap() = false;
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::CodeSign(_)), "codesign verify 실패");
    assert!(!fx.env.ran_program("node --version"));
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn quarantine_removal_failure_aborts_before_signature_check_or_execution() {
    let fx = Fixture::new("xattr");
    *fx.env.xattr_ok.lock().unwrap() = false;
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Quarantine(_)), "xattr 실패");
    assert!(!fx.env.ran_program("codesign"));
    assert!(!fx.env.ran_program("node --version"));
    fx.assert_clean_first_install_failure();
}

#[test]
fn installed_node_reporting_a_different_version_is_rejected() {
    let fx = Fixture::new("version-mismatch");
    fx.seed_prior_install(PRIOR);
    *fx.env.node_version_output.lock().unwrap() = "v24.99.0\n".to_string();
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::VersionMismatch { .. }), "버전 불일치");
    fx.assert_prior_preserved_and_clean(PRIOR);
}

// ── 다운로드 중단 / 리다이렉트 / 상태 코드 ──

#[test]
fn download_interrupted_midway_leaves_no_partial_install_and_keeps_prior() {
    let fx = Fixture::new("interrupted");
    fx.seed_prior_install(PRIOR);
    let prefix = fx.tar_bytes[..fx.tar_bytes.len() / 2].to_vec();
    fx.env.route(&super::release::tarball_url(fx.version, ARCH), Route::Truncated { prefix });
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Network(_)), "download_interrupted");
    assert!(fx.env.ran_no_tools());
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn redirect_rejected_during_tarball_download_aborts_cleanly() {
    let fx = Fixture::new("redirect");
    fx.seed_prior_install(PRIOR);
    fx.env.route(&super::release::tarball_url(fx.version, ARCH), Route::Rejected);
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::RedirectRejected(_)), "redirect_rejected");
    assert!(fx.env.ran_no_tools());
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn http_error_status_on_checksum_fetch_aborts_cleanly() {
    let fx = Fixture::new("status");
    fx.env.route(&super::release::shasums_url(fx.version), Route::Status(404));
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Network(_)), "HTTP 404");
    fx.assert_clean_first_install_failure();
}

// ── 디스크 부족 ──

#[test]
fn disk_full_while_writing_the_download_leaves_no_staging_and_keeps_prior() {
    let fx = Fixture::new("diskfull-write");
    fx.seed_prior_install(PRIOR);
    *fx.env.disk_full_after.lock().unwrap() = Some(2_000);
    let err = assert_err(fx.install(&|| false), |e| matches!(e, InstallError::DiskFull(_)), "disk_full(쓰기 실패)");
    assert!(err.to_string().contains("디스크 공간"));
    assert!(fx.env.ran_no_tools());
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn disk_full_during_extraction_is_reported_and_cleaned_up() {
    let fx = Fixture::new("diskfull-extract");
    fx.seed_prior_install(PRIOR);
    *fx.env.tar_extract_override.lock().unwrap() = Some(super::env::ToolRun {
        exit_code: Some(1),
        stderr: "tar: Error writing to file: No space left on device".to_string(),
        ..Default::default()
    });
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::DiskFull(_)), "disk_full(해제 실패)");
    assert!(!fx.env.ran_program("codesign"));
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn disk_full_precheck_refuses_before_downloading_anything_large() {
    let fx = Fixture::new("diskfull-pre");
    fx.seed_prior_install(PRIOR);
    *fx.env.free_space.lock().unwrap() = Some(50 * 1024 * 1024);
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::DiskFull(_)), "disk_full(사전 점검)");
    assert_eq!(fx.env.count_prefix("GET "), 0);
    fx.assert_prior_preserved_and_clean(PRIOR);
}

// ── 취소 ──

#[test]
fn cancelled_during_download_leaves_nothing_behind() {
    // 청크가 둘 이상이 되도록 tarball을 키운 픽스처.
    let fx = Fixture::with_entries("cancel-dl", |top| {
        let mut e = good_entries(top);
        e.push(file(&format!("{top}/big.bin"), 0o644, &"x".repeat(200_000)));
        e
    });
    fx.seed_prior_install(PRIOR);
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    // 첫 16KB 청크를 받은 직후 사용자가 취소한다.
    *fx.env.after_chunks.lock().unwrap() = Some((1, Box::new(move || f2.store(true, Ordering::SeqCst))));
    let err = assert_err(fx.install(&|| flag.load(Ordering::SeqCst)), |e| e.is_cancelled(), "cancelled(다운로드 중)");
    assert!(err.to_string().contains("기존 설치는 그대로"));
    assert!(fx.env.ran_no_tools());
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn cancelled_before_start_does_nothing_at_all() {
    let fx = Fixture::new("cancel-early");
    assert_err(fx.install(&|| true), |e| e.is_cancelled(), "cancelled(시작 전)");
    assert_eq!(fx.env.calls(), Vec::<String>::new());
    assert!(!fx.layout.base.exists(), "취소했는데 기준 폴더가 만들어졌습니다");
}

#[test]
fn cancelled_just_before_extraction_leaves_nothing_behind() {
    let fx = Fixture::new("cancel-extract");
    fx.seed_prior_install(PRIOR);
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    *fx.env.before_run.lock().unwrap() = Some(("-xf".to_string(), Box::new(move || f2.store(true, Ordering::SeqCst))));
    assert_err(fx.install(&|| flag.load(Ordering::SeqCst)), |e| e.is_cancelled(), "cancelled(해제 직전)");
    assert!(!fx.env.ran_program("xattr"));
    assert!(!fx.env.ran_program("codesign"));
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn cancelled_after_verification_but_before_placement_leaves_nothing_behind() {
    let fx = Fixture::new("cancel-late");
    fx.seed_prior_install(PRIOR);
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    // node --version이 호출되는 순간 취소 → 반영(rename) 전에 멈춰야 한다.
    *fx.env.before_run.lock().unwrap() = Some(("node".to_string(), Box::new(move || f2.store(true, Ordering::SeqCst))));
    assert_err(fx.install(&|| flag.load(Ordering::SeqCst)), |e| e.is_cancelled(), "cancelled(반영 직전)");
    assert_eq!(fx.env.count_prefix("RENAME"), 0);
    fx.assert_prior_preserved_and_clean(PRIOR);
}

// ── tar 경로 탈출 / 링크 탈출 ──

#[test]
fn tar_path_escape_entry_is_rejected_before_extraction_and_nothing_is_written_outside() {
    let fx = Fixture::with_entries("tar-escape", |top| {
        let mut e = good_entries(top);
        e.push(file("../evil.txt", 0o644, "pwned"));
        e
    });
    fx.seed_prior_install(PRIOR);
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::UnsafeArchive(_)), "tar_path_escape");
    assert!(!fx.env.ran_program("tar -xf"), "탈출 항목이 있으면 해제 자체를 시작하면 안 됩니다");
    assert!(!fx.env.ran_program("codesign"));
    assert!(!fx.env.ran_program("node --version"));
    assert!(!fx.layout.base.join("evil.txt").exists());
    assert!(!fx.layout.base.parent().unwrap().join("evil.txt").exists());
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn tar_absolute_path_entry_is_rejected_before_extraction() {
    let fx = Fixture::with_entries("tar-abs", |top| {
        let mut e = good_entries(top);
        e.push(file("/tmp/r22-abs-evil.txt", 0o644, "pwned"));
        e
    });
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::UnsafeArchive(_)), "tar_absolute_path");
    assert!(!fx.env.ran_program("tar -xf"));
    assert!(!std::path::Path::new("/tmp/r22-abs-evil.txt").exists());
    fx.assert_clean_first_install_failure();
}

#[test]
fn tar_symlink_escape_is_rejected_after_extraction_before_any_execution() {
    let fx = Fixture::with_entries("tar-symlink", |top| {
        let mut e = good_entries(top);
        // 최상위 밖(홈 쪽)을 가리키는 상대 링크.
        e.push(symlink(&format!("{top}/lib/escape"), "../../../../../../outside"));
        e
    });
    fx.seed_prior_install(PRIOR);
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::UnsafeArchive(_)), "symlink 탈출");
    assert!(fx.env.ran_program("tar -xf"), "링크 탈출은 해제 후 트리 검사에서 잡힌다");
    assert!(!fx.env.ran_program("xattr"));
    assert!(!fx.env.ran_program("codesign"));
    assert!(!fx.env.ran_program("node --version"));
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn tar_absolute_symlink_target_is_rejected() {
    let fx = Fixture::with_entries("tar-abs-link", |top| {
        let mut e = good_entries(top);
        e.push(symlink(&format!("{top}/lib/passwd"), "/etc/passwd"));
        e
    });
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::UnsafeArchive(_)), "절대경로 링크");
    assert!(!fx.env.ran_program("codesign"));
    fx.assert_clean_first_install_failure();
}

#[test]
fn tar_hardlink_entry_is_rejected_after_extraction() {
    let fx = Fixture::with_entries("tar-hardlink", |top| {
        let mut e = good_entries(top);
        e.push(hardlink(&format!("{top}/bin/node-copy"), &format!("{top}/bin/node")));
        e
    });
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::UnsafeArchive(_)), "하드링크");
    assert!(!fx.env.ran_program("codesign"));
    fx.assert_clean_first_install_failure();
}

#[test]
fn tree_setuid_file_is_rejected() {
    // bsdtar는 일반 사용자로 풀 때 setuid 비트를 버리지만, 트리 검사는 그래도 독립적으로 거부한다.
    use std::os::unix::fs::PermissionsExt;
    let tmp = TestDir::new("tree-setuid");
    let top = tmp.path().join("node-v24.22.0-darwin-arm64");
    std::fs::create_dir_all(top.join("bin")).unwrap();
    let f = top.join("bin").join("su-like");
    std::fs::write(&f, "x").unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o4755)).unwrap();
    assert_err(validate_tree(tmp.path(), "node-v24.22.0-darwin-arm64"), |e| matches!(e, InstallError::UnsafeArchive(_)), "setuid");
}

#[test]
fn tree_special_file_and_wrong_top_are_rejected() {
    let tmp = TestDir::new("tree-special");
    let top = tmp.path().join("node-v24.22.0-darwin-arm64");
    std::fs::create_dir_all(&top).unwrap();
    // 최상위가 기대 이름이 아니면 거부.
    assert_err(validate_tree(tmp.path(), "node-v1.0.0-darwin-arm64"), |e| matches!(e, InstallError::UnsafeArchive(_)), "다른 최상위");
    // FIFO(특수 파일) 거부.
    let status = std::process::Command::new("/usr/bin/mkfifo").arg(top.join("pipe")).status().unwrap();
    assert!(status.success());
    assert_err(validate_tree(tmp.path(), "node-v24.22.0-darwin-arm64"), |e| matches!(e, InstallError::UnsafeArchive(_)), "FIFO");
}

#[test]
fn tree_symlink_chain_escaping_through_an_inner_link_is_rejected() {
    // a -> b, b -> ../../outside : 각각을 어휘적으로 볼 때 a는 안쪽처럼 보여도 해석 결과가 밖이다.
    let tmp = TestDir::new("tree-chain");
    let top = tmp.path().join("node-v24.22.0-darwin-arm64");
    std::fs::create_dir_all(top.join("lib")).unwrap();
    std::fs::create_dir_all(tmp.path().join("outside")).unwrap();
    std::os::unix::fs::symlink("../../outside", top.join("lib").join("b")).unwrap();
    std::os::unix::fs::symlink("b", top.join("lib").join("a")).unwrap();
    assert_err(validate_tree(tmp.path(), "node-v24.22.0-darwin-arm64"), |e| matches!(e, InstallError::UnsafeArchive(_)), "링크 체인 탈출");
}

#[test]
fn tar_with_two_top_level_directories_is_rejected_by_the_listing_check() {
    let fx = Fixture::with_entries("tar-two-top", |top| {
        let mut e = good_entries(top);
        e.push(dir("another-top"));
        e.push(file("another-top/x", 0o644, "x"));
        e
    });
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::UnsafeArchive(_)), "두 번째 최상위");
    assert!(!fx.env.ran_program("tar -xf"));
    fx.assert_clean_first_install_failure();
}

#[test]
fn tarball_missing_npm_is_rejected_before_placement() {
    let fx = Fixture::with_entries("tar-no-npm", |top| {
        good_entries(top)
            .into_iter()
            .filter(|e| !e.name.ends_with("/bin/npm"))
            .collect()
    });
    fx.seed_prior_install(PRIOR);
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Extract(_)), "npm 링크 없음");
    assert_eq!(fx.env.count_prefix("RENAME"), 0);
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn tree_validation_accepts_the_well_formed_fixture_after_real_extraction() {
    let fx = Fixture::new("tree-ok");
    let tmp = TestDir::new("tree-ok-extract");
    let tar_path = tmp.path().join("n.tar");
    std::fs::write(&tar_path, &fx.tar_bytes).unwrap();
    let out = tmp.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let status = std::process::Command::new("/usr/bin/tar")
        .args(["-xf"])
        .arg(&tar_path)
        .arg("-C")
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success());
    let top = super::release::tarball_top_dir(fx.version, ARCH);
    let summary = validate_tree(&out, &top).unwrap();
    assert!(summary.files >= 4 && summary.symlinks == 2);
}

// ── rename 실패(원자 반영 롤백) ──

#[test]
fn rename_failure_placing_the_version_directory_keeps_prior_and_cleans_up() {
    let fx = Fixture::new("rename-version");
    fx.seed_prior_install(PRIOR);
    fx.env.fail_rename_to.lock().unwrap().push("v24.22.0".to_string());
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Io(_)), "rename 실패(버전 디렉터리)");
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn rename_failure_flipping_current_rolls_back_the_new_version_directory() {
    let fx = Fixture::new("rename-current");
    fx.seed_prior_install(PRIOR);
    fx.env.fail_rename_to.lock().unwrap().push("current".to_string());
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Io(_)), "rename 실패(current)");
    fx.assert_prior_preserved_and_clean(PRIOR);
}

#[test]
fn rename_failure_on_first_install_leaves_no_trace() {
    let fx = Fixture::new("rename-first");
    fx.env.fail_rename_to.lock().unwrap().push("current".to_string());
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Io(_)), "첫 설치 current 실패");
    fx.assert_clean_first_install_failure();
}

#[test]
fn failure_replacing_a_damaged_same_version_restores_the_damaged_copy() {
    let fx = Fixture::new("rename-restore");
    fx.install_ok();
    std::fs::remove_file(fx.layout.version_dir(fx.version).join("bin").join("node")).unwrap();
    std::fs::write(fx.layout.version_dir(fx.version).join("DAMAGED-MARKER"), "keep-me").unwrap();
    // 새 사본을 배치하는 rename만 실패시킨다(손상본은 이미 치워진 상태 → 복원돼야 한다).
    // 첫 번째 rename(손상본 치우기)은 목적지 이름이 "replaced"라 통과하고, 두 번째(배치)는 실패한다.
    fx.env.fail_rename_to.lock().unwrap().push("v24.22.0".to_string());
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Io(_)), "배치 실패");
    assert_eq!(
        std::fs::read_to_string(fx.layout.version_dir(fx.version).join("DAMAGED-MARKER")).unwrap(),
        "keep-me",
        "교체 실패 시 기존(손상) 디렉터리를 복원해야 합니다"
    );
    assert_eq!(fx.staging_leftovers(), Vec::<String>::new());
}

// ── 잠금 · 스테이징 청소 · 입력 검증 ──

#[test]
fn install_is_rejected_while_another_install_holds_the_lock() {
    let fx = Fixture::new("lock");
    fx.seed_prior_install(PRIOR);
    let held = InstallLock::acquire(&fx.layout).expect("첫 잠금");
    assert_err(fx.install(&|| false), |e| matches!(e, InstallError::Locked), "잠금 충돌");
    assert_eq!(fx.env.calls(), Vec::<String>::new(), "잠금 충돌 시 아무것도 하지 않아야 합니다");
    fx.assert_prior_preserved_and_clean(PRIOR);
    drop(held);
    // 잠금이 풀리면 정상 진행.
    fx.install_ok();
}

#[test]
fn stale_staging_directories_from_a_crashed_run_are_swept_on_next_install() {
    let fx = Fixture::new("sweep");
    let stale = fx.layout.base.join(".staging-99999-1");
    std::fs::create_dir_all(stale.join("extract")).unwrap();
    std::fs::write(stale.join("node.tar.gz"), "partial").unwrap();
    fx.install_ok();
    assert_eq!(fx.staging_leftovers(), Vec::<String>::new());
}

#[test]
fn pinned_version_with_unexpected_shape_cannot_be_constructed() {
    // 버전은 NodeVersion을 통해서만 URL·경로에 들어간다 — 문자열에서 만드는 길은 엄격 파서뿐이다.
    for bad in ["v24.22.0/../..", "v24.22", "24.22.0", "v24.22.0-rc.1", "v024.22.0"] {
        assert!(super::NodeVersion::parse_tag(bad).is_none(), "{bad}");
    }
}

#[test]
fn no_staging_or_partial_files_remain_in_the_base_folder_after_success() {
    let fx = Fixture::new("clean-success");
    fx.install_ok();
    let names: Vec<String> = std::fs::read_dir(&fx.layout.base)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    for n in &names {
        assert!(
            n == "node" || n == "npm-global" || n == ".node-install.lock",
            "예상 밖의 항목이 기준 폴더에 남았습니다: {n} (전체: {names:?})"
        );
    }
}
