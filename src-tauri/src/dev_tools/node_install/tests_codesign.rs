// 코드 요구식(`developer_id_requirement`)과 `verify_node_signature`가 실제 `/usr/bin/codesign`에서
// 의도대로 동작하는지 네트워크 없이 확인한다(macOS 한정 — CI의 macos 러너에서도 돈다).
// 다른 테스트의 가짜 환경은 codesign 인자 내용을 보지 않고 플래그로 성공·실패를 정하므로, 요구식이
// 느슨해지는 회귀(예: OU 조건 삭제)는 여기서만 잡힌다.
//
// 재료는 전부 호스트에 있는 것뿐이다: Apple 플랫폼 바이너리(`/bin/ls`, TeamIdentifier=not set,
// 서명자 `Software Signing`/OU=`Apple Software`)와 그 사본을 ad-hoc 서명한 파일(테스트 임시 폴더).
// 음성 경로(OpenJS가 아닌 서명은 거부)가 핵심이고, 요구식 구조가 "파싱되고 평가된다"는 것은
// 같은 템플릿에 Apple의 OU를 넣은 양성 대조로 확인한다 — 구문 오류(종료 코드 1)와 "요구식 불만족"
// (종료 코드 3)을 구분해 단언하므로, 문자열이 깨져서 우연히 실패하는 경우를 통과시키지 않는다.

use super::env::ToolRun;
use super::types::InstallError;
use super::{developer_id_requirement, verify_node_signature, InstallEnv, RealEnv, CODESIGN, EXPECTED_TEAM_ID};
use std::path::{Path, PathBuf};
use std::time::Duration;

const APPLE_PLATFORM_BINARY: &str = "/bin/ls";
/// 양성 대조용 후보: leaf 인증서(OU)를 가진 Apple 서명 바이너리를 앞에서부터 찾는다. leaf의 OU
/// 값("Apple Software" 등)은 macOS 버전·이미지마다 달라질 수 있어 리터럴로 박지 않고 호스트에서 읽는다
/// (macOS 26.5 로컬은 `/bin/ls`의 OU가 "Apple Software"지만 GitHub macos-26 러너 이미지 26.6.2에서는
/// 같은 값으로 통과하지 않았다).
const APPLE_CONTROL_CANDIDATES: &[&str] = &[
    "/bin/ls",
    "/bin/cat",
    "/usr/bin/codesign",
    "/usr/bin/openssl",
    "/usr/bin/ssh",
    "/usr/bin/tar",
];
const OPENSSL: &str = "/usr/bin/openssl";

/// 테스트 전용 임시 폴더(테스트 temp 디렉터리 아래). 드롭하면 지운다.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("r22-codesign-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `/bin/ls`를 임시 폴더로 복사해 ad-hoc 서명한다(서명은 유효하지만 Team ID·Apple 체인이 없다).
fn adhoc_signed_copy(dir: &TempDir) -> PathBuf {
    let copy = dir.0.join("node-lookalike");
    std::fs::copy(APPLE_PLATFORM_BINARY, &copy).unwrap();
    let run = codesign(&["-s", "-", "--force", copy.to_str().unwrap()]);
    assert!(run.succeeded(), "ad-hoc 서명 준비 실패: {}", run.describe_failure());
    copy
}

fn codesign(args: &[&str]) -> ToolRun {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    RealEnv::new().run_tool(CODESIGN, &args, Duration::from_secs(60), &|| false)
}

/// `target`의 leaf 인증서 subject OU를 읽는다. 인증서가 없거나(인증서 없는 플랫폼 서명) 읽지 못하면 `None`.
fn leaf_ou_of(target: &str) -> Option<String> {
    let dir = TempDir::new(&format!("leaf-ou-{}", target.replace('/', "_")));
    let prefix = dir.0.join("cert");
    let run = codesign(&["-d", &format!("--extract-certificates={}", prefix.display()), target]);
    if !run.succeeded() {
        return None;
    }
    let leaf = dir.0.join("cert0"); // 0번이 leaf, 이후가 중간·루트 CA
    let args: Vec<String> = ["x509", "-inform", "der", "-noout", "-subject", "-nameopt", "RFC2253", "-in"]
        .iter()
        .map(|a| a.to_string())
        .chain(std::iter::once(leaf.to_string_lossy().into_owned()))
        .collect();
    let out = RealEnv::new().run_tool(OPENSSL, &args, Duration::from_secs(30), &|| false);
    if !out.succeeded() {
        return None;
    }
    // RFC2253: "subject=CN=...,OU=Apple Software,O=Apple Inc.,C=US" — OU 값은 쉼표로 끊기지 않는 경우만 신뢰한다.
    out.stdout
        .trim()
        .trim_start_matches("subject=")
        .split(',')
        .find_map(|part| part.trim().strip_prefix("OU=").map(str::to_string))
        .filter(|ou| !ou.is_empty() && !ou.contains(['"', '\\']))
}

fn check_requirement(requirement: &str, target: &Path) -> ToolRun {
    codesign(&["--verify", "-R", requirement, target.to_str().unwrap()])
}

/// codesign이 요구식을 정상 파싱했고 그 대상이 요구식을 만족하지 않는다 — 종료 코드 3.
/// (구문 오류는 종료 코드 1 + "invalid or corrupted code requirement")
fn assert_requirement_not_satisfied(run: &ToolRun, what: &str) {
    assert_eq!(run.exit_code, Some(3), "{what}: 요구식 불만족(3)이어야 합니다 — stderr: {}", run.stderr);
    assert!(run.stderr.contains("failed to satisfy"), "{what}: {}", run.stderr);
    assert!(!run.stderr.contains("invalid or corrupted"), "{what}: 요구식 구문 오류입니다 — {}", run.stderr);
}

#[test]
fn requirement_text_is_pinned_to_apple_chain_plus_openjs_team_id() {
    assert_eq!(
        developer_id_requirement(),
        "=anchor apple generic and certificate leaf[subject.OU] = \"HX7739G8FX\""
    );
    assert_eq!(EXPECTED_TEAM_ID, "HX7739G8FX");
}

#[test]
fn real_codesign_rejects_an_apple_platform_binary_for_the_openjs_requirement() {
    let run = check_requirement(&developer_id_requirement(), Path::new(APPLE_PLATFORM_BINARY));
    assert_requirement_not_satisfied(&run, "/bin/ls");
}

#[test]
fn real_codesign_rejects_an_adhoc_signed_copy_for_the_openjs_requirement() {
    let dir = TempDir::new("req-adhoc");
    let copy = adhoc_signed_copy(&dir);
    // 서명 자체는 유효하다 — 그래도 Apple 체인·OU가 없어 요구식은 만족하지 못한다.
    assert!(codesign(&["--verify", "--strict", copy.to_str().unwrap()]).succeeded());
    let run = check_requirement(&developer_id_requirement(), &copy);
    assert_requirement_not_satisfied(&run, "ad-hoc 사본");
}

#[test]
fn the_ou_clause_is_what_rejects_apple_signed_code() {
    // 대조 2(OU 값과 무관): OU 조건을 지운 요구식(= OU 조건이 느슨해진 회귀)은 Apple 서명 코드를 통과시킨다.
    // 즉 현재 요구식의 거부는 OU 조건 덕분이다. (`anchor apple generic` 통과는 다른 테스트도 단언한다.)
    let without_ou = check_requirement("=anchor apple generic", Path::new(APPLE_PLATFORM_BINARY));
    assert!(without_ou.succeeded(), "OU 조건이 없으면 /bin/ls가 통과해야 합니다: {}", without_ou.stderr);
    // 현재 요구식은 같은 대상을 거부한다.
    assert!(!check_requirement(&developer_id_requirement(), Path::new(APPLE_PLATFORM_BINARY)).succeeded());

    // 양성 대조 1: 같은 템플릿에 그 바이너리의 실제 leaf OU를 넣으면 통과한다 — 요구식이 파싱되고
    // `certificate leaf[subject.OU]` 비교가 실제로 평가된다는 증거. OU는 호스트에서 읽는다(리터럴 금지).
    let Some((binary, apple_ou)) = APPLE_CONTROL_CANDIDATES
        .iter()
        .find_map(|b| leaf_ou_of(b).map(|ou| (*b, ou)))
    else {
        // 이 호스트의 Apple 서명 바이너리 중 leaf 인증서를 가진 것이 없다(인증서 없는 플랫폼 서명뿐) —
        // OU 비교를 평가시킬 재료가 없으므로 양성 대조 1만 건너뛴다. 보안 단언(OpenJS 요구식이 Apple
        // 바이너리·ad-hoc 사본을 거부)은 위 단언과 다른 테스트가 그대로 지킨다.
        eprintln!("leaf 인증서를 가진 Apple 서명 바이너리가 없어 OU 양성 대조 1을 건너뜁니다");
        return;
    };
    let apple_req = format!("=anchor apple generic and certificate leaf[subject.OU] = \"{apple_ou}\"");
    let ok = check_requirement(&apple_req, Path::new(binary));
    assert!(ok.succeeded(), "대조 요구식({apple_req})이 {binary}에 통과해야 합니다: {} {}", ok.stderr, ok.stdout);
    // 같은 템플릿에서 OU 값만 다르면 거부된다(종료 코드 3) — OU 문자열 비교가 실제로 일어난다는 증거.
    let other = format!("=anchor apple generic and certificate leaf[subject.OU] = \"{apple_ou}-not\"");
    assert_requirement_not_satisfied(&check_requirement(&other, Path::new(binary)), "다른 OU 값");
    // 그리고 OpenJS 요구식은 이 바이너리도 거부한다.
    assert!(!check_requirement(&developer_id_requirement(), Path::new(binary)).succeeded());
}

#[test]
fn the_anchor_clause_rejects_code_without_an_apple_chain() {
    let dir = TempDir::new("anchor");
    let copy = adhoc_signed_copy(&dir);
    // /bin/ls는 `anchor apple generic`을 만족하지만 ad-hoc 사본은 만족하지 못한다.
    assert!(check_requirement("=anchor apple generic", Path::new(APPLE_PLATFORM_BINARY)).succeeded());
    let run = check_requirement("=anchor apple generic", &copy);
    assert_requirement_not_satisfied(&run, "ad-hoc 사본 anchor");
}

#[test]
fn verify_node_signature_rejects_an_apple_platform_binary_with_a_team_id_mismatch() {
    let err = verify_node_signature(&RealEnv::new(), Path::new(APPLE_PLATFORM_BINARY), &|| false).unwrap_err();
    assert!(matches!(err, InstallError::TeamIdMismatch { .. }), "{err:?}");
}

#[test]
fn verify_node_signature_rejects_an_adhoc_signed_copy_with_a_team_id_mismatch() {
    let dir = TempDir::new("verify-adhoc");
    let copy = adhoc_signed_copy(&dir);
    let err = verify_node_signature(&RealEnv::new(), &copy, &|| false).unwrap_err();
    assert!(matches!(err, InstallError::TeamIdMismatch { .. }), "{err:?}");
}

#[test]
fn verify_node_signature_rejects_a_tampered_signed_copy_before_looking_at_the_team_id() {
    let dir = TempDir::new("verify-tampered");
    let copy = adhoc_signed_copy(&dir);
    // 서명 후 바이트를 바꾸면 `--verify --strict`(1단계)가 먼저 거부한다.
    let mut bytes = std::fs::read(&copy).unwrap();
    let last = bytes.len() / 2;
    bytes[last] ^= 0xff;
    std::fs::write(&copy, bytes).unwrap();
    let err = verify_node_signature(&RealEnv::new(), &copy, &|| false).unwrap_err();
    assert!(matches!(err, InstallError::CodeSign(_)), "{err:?}");
}
