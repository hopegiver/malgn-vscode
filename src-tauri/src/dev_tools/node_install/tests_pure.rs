// 순수 로직 테스트(모든 OS): 버전 결정 규칙·엄격 버전 검증·체크섬 파싱·tar 목록 검사·
// 허용 출처 정책. 네트워크·외부 도구·파일시스템을 쓰지 않는다.

use super::archive::validate_listing;
use super::fetch::FetchPolicy;
use super::release::{expected_sha256, select_latest_lts, tarball_name, tarball_top_dir, tarball_url};
use super::types::{InstallError, NodeArch, NodeVersion};

const ARM: NodeArch = NodeArch::Arm64;

fn entry(version: &str, lts: &str, files: &[&str]) -> String {
    let lts_json = if lts.is_empty() { "false".to_string() } else { format!("\"{lts}\"") };
    let files_json: Vec<String> = files.iter().map(|f| format!("\"{f}\"")).collect();
    format!(
        "{{\"version\":{},\"lts\":{lts_json},\"files\":[{}]}}",
        serde_json::to_string(version).unwrap(),
        files_json.join(",")
    )
}

fn index(entries: &[String]) -> Vec<u8> {
    format!("[{}]", entries.join(",")).into_bytes()
}

// ── 엄격 버전 파서 ──

#[test]
fn invalid_version_string_forms_are_rejected_by_strict_parser() {
    for bad in [
        "", "v", "24.22.0", "v24.22", "v24.22.0.1", "v24.22.x", "V24.22.0", " v24.22.0", "v24.22.0 ",
        "v24.22.0\n", "v024.22.0", "v24.022.0", "v24.22.00", "v24.22.0-rc.1", "v24.22.0+meta",
        "v24.22.0/../../x", "v../../1.1", "v24..0", "v-1.2.3", "v+1.2.3", "v1_000.2.3",
        "v99999999999.0.0", "v1000000.0.0", "v24.22.0;rm -rf", "v24.22.0\0",
    ] {
        assert_eq!(NodeVersion::parse_tag(bad), None, "거부돼야 하는 입력: {bad:?}");
    }
}

#[test]
fn strict_version_parser_accepts_canonical_forms_and_roundtrips() {
    for ok in ["v0.0.0", "v22.0.0", "v24.22.0", "v26.12.0", "v999999.999999.999999"] {
        let parsed = NodeVersion::parse_tag(ok).unwrap_or_else(|| panic!("{ok}"));
        assert_eq!(parsed.to_tag(), ok, "재조립 결과가 원문과 같아야 합니다");
    }
}

// ── 버전 결정 규칙(hub 결정 01m4zfesw2qvhnth3jjg89w04k) ──

#[test]
fn release_selects_highest_lts_with_osx_tar_and_ignores_current_line() {
    let body = index(&[
        entry("v26.12.0", "", &["osx-arm64-tar"]),
        entry("v24.22.0", "Krypton", &["osx-arm64-tar", "osx-x64-tar"]),
        entry("v24.21.0", "Krypton", &["osx-arm64-tar"]),
        entry("v22.23.0", "Jod", &["osx-arm64-tar"]),
    ]);
    assert_eq!(select_latest_lts(&body, ARM).unwrap().to_tag(), "v24.22.0");
}

#[test]
fn release_does_not_pin_a_major_a_newer_lts_line_wins() {
    let body = index(&[
        entry("v24.22.0", "Krypton", &["osx-arm64-tar"]),
        entry("v26.2.0", "Mercury", &["osx-arm64-tar"]),
        entry("v22.23.0", "Jod", &["osx-arm64-tar"]),
    ]);
    assert_eq!(select_latest_lts(&body, ARM).unwrap().to_tag(), "v26.2.0");
}

#[test]
fn release_comparison_is_numeric_not_lexicographic() {
    let body = index(&[
        entry("v24.9.0", "Krypton", &["osx-arm64-tar"]),
        entry("v24.10.0", "Krypton", &["osx-arm64-tar"]),
        entry("v24.10.1", "Krypton", &["osx-arm64-tar"]),
    ]);
    assert_eq!(select_latest_lts(&body, ARM).unwrap().to_tag(), "v24.10.1");
}

#[test]
fn release_skips_lts_without_osx_tar_for_this_arch() {
    let body = index(&[
        entry("v24.23.0", "Krypton", &["osx-x64-tar"]),
        entry("v24.22.0", "Krypton", &["osx-arm64-tar"]),
    ]);
    assert_eq!(select_latest_lts(&body, ARM).unwrap().to_tag(), "v24.22.0");
    assert_eq!(select_latest_lts(&body, NodeArch::X64).unwrap().to_tag(), "v24.23.0");
}

#[test]
fn release_enforces_major_floor_of_22() {
    let body = index(&[
        entry("v20.19.0", "Iron", &["osx-arm64-tar"]),
        entry("v18.20.8", "Hydrogen", &["osx-arm64-tar"]),
    ]);
    assert!(matches!(select_latest_lts(&body, ARM), Err(InstallError::VersionResolve(_))));
    let body = index(&[
        entry("v20.19.0", "Iron", &["osx-arm64-tar"]),
        entry("v22.0.0", "Jod", &["osx-arm64-tar"]),
    ]);
    assert_eq!(select_latest_lts(&body, ARM).unwrap().to_tag(), "v22.0.0");
}

#[test]
fn release_treats_non_string_lts_values_as_not_lts() {
    let body = br#"[
        {"version":"v26.0.0","lts":true,"files":["osx-arm64-tar"]},
        {"version":"v25.0.0","lts":null,"files":["osx-arm64-tar"]},
        {"version":"v25.1.0","files":["osx-arm64-tar"]},
        {"version":"v25.2.0","lts":"","files":["osx-arm64-tar"]},
        {"version":"v24.22.0","lts":"Krypton","files":["osx-arm64-tar"]}
    ]"#;
    assert_eq!(select_latest_lts(body, ARM).unwrap().to_tag(), "v24.22.0");
}

#[test]
fn release_invalid_version_strings_in_index_are_never_selected() {
    // 형식 위반 항목은 (더 높은 숫자처럼 보여도) 후보에서 빠지고, 남은 정상 항목이 선택된다.
    let body = index(&[
        entry("v99.0.0/../../etc", "Evil", &["osx-arm64-tar"]),
        entry("v99.0", "Evil", &["osx-arm64-tar"]),
        entry("99.0.0", "Evil", &["osx-arm64-tar"]),
        entry("v099.0.0", "Evil", &["osx-arm64-tar"]),
        entry("v99.0.0-rc.1", "Evil", &["osx-arm64-tar"]),
        entry("v24.22.0", "Krypton", &["osx-arm64-tar"]),
    ]);
    assert_eq!(select_latest_lts(&body, ARM).unwrap().to_tag(), "v24.22.0");

    let only_bad = index(&[entry("v99.0.0/../x", "Evil", &["osx-arm64-tar"])]);
    assert!(matches!(select_latest_lts(&only_bad, ARM), Err(InstallError::VersionResolve(_))));
}

#[test]
fn release_malformed_index_json_is_an_error_not_a_panic() {
    for bad in [&b""[..], b"not json", b"{}", b"[1,2,3]", b"[null]", b"\xff\xfe"] {
        assert!(select_latest_lts(bad, ARM).is_err());
    }
}

#[test]
fn urls_and_names_are_built_only_from_validated_versions() {
    let v = NodeVersion::parse_tag("v24.22.0").unwrap();
    assert_eq!(tarball_name(v, ARM), "node-v24.22.0-darwin-arm64.tar.gz");
    assert_eq!(tarball_top_dir(v, NodeArch::X64), "node-v24.22.0-darwin-x64");
    assert_eq!(
        tarball_url(v, ARM),
        "https://nodejs.org/dist/v24.22.0/node-v24.22.0-darwin-arm64.tar.gz"
    );
    // 모든 생성 URL은 프로덕션 허용 출처 정책을 통과한다.
    let policy = FetchPolicy::nodejs_org();
    for url in [
        super::release::index_url(),
        super::release::shasums_url(v),
        tarball_url(v, ARM),
        tarball_url(v, NodeArch::X64),
    ] {
        assert!(policy.allows(&url::Url::parse(&url).unwrap()), "{url}");
    }
}

// ── SHASUMS256.txt ──

const HASH_A: &str = "6a9724d9b50a5f22b4816cded441b5a8d9ddae546191a5e5b7efa5a5e2ee26d5";
const HASH_B: &str = "1111111111111111111111111111111111111111111111111111111111111111";

#[test]
fn shasums_requires_exact_file_name_match() {
    let text = format!(
        "{HASH_B}  node-v24.22.0-darwin-arm64.tar.gz.sig\n\
         {HASH_B}  xnode-v24.22.0-darwin-arm64.tar.gz\n\
         {HASH_B}  node-v24.22.0-darwin-arm64.tar.xz\n\
         {HASH_A}  node-v24.22.0-darwin-arm64.tar.gz\n"
    );
    assert_eq!(expected_sha256(&text, "node-v24.22.0-darwin-arm64.tar.gz").unwrap(), HASH_A);
}

#[test]
fn shasums_missing_file_name_row_is_rejected() {
    let text = format!("{HASH_A}  node-v24.22.0-darwin-x64.tar.gz\n");
    assert!(matches!(
        expected_sha256(&text, "node-v24.22.0-darwin-arm64.tar.gz"),
        Err(InstallError::ChecksumMissing(_))
    ));
    assert!(matches!(expected_sha256("", "a.tar.gz"), Err(InstallError::ChecksumMissing(_))));
}

#[test]
fn shasums_conflicting_duplicate_rows_are_rejected_but_identical_ones_are_fine() {
    let conflicting = format!(
        "{HASH_A}  node-v24.22.0-darwin-arm64.tar.gz\n{HASH_B}  node-v24.22.0-darwin-arm64.tar.gz\n"
    );
    assert!(expected_sha256(&conflicting, "node-v24.22.0-darwin-arm64.tar.gz").is_err());
    let same = format!(
        "{HASH_A}  node-v24.22.0-darwin-arm64.tar.gz\n{HASH_A} *node-v24.22.0-darwin-arm64.tar.gz\n"
    );
    assert_eq!(expected_sha256(&same, "node-v24.22.0-darwin-arm64.tar.gz").unwrap(), HASH_A);
}

#[test]
fn shasums_malformed_hash_is_rejected_and_uppercase_is_normalized() {
    for bad in ["abc", &"g".repeat(64), &"a".repeat(63), &"a".repeat(65)] {
        let text = format!("{bad}  f.tar.gz\n");
        assert!(expected_sha256(&text, "f.tar.gz").is_err(), "{bad}");
    }
    let upper = format!("{}  f.tar.gz\n", HASH_A.to_ascii_uppercase());
    assert_eq!(expected_sha256(&upper, "f.tar.gz").unwrap(), HASH_A);
}

// ── tar 목록 검사(경로 탈출) ──

const TOP: &str = "node-v24.22.0-darwin-arm64";

#[test]
fn tar_listing_accepts_the_official_shape() {
    let listing = format!("{TOP}/\n{TOP}/bin/\n{TOP}/bin/node\n{TOP}/lib/node_modules/npm/package.json\n");
    assert_eq!(validate_listing(&listing, TOP).unwrap(), 4);
}

#[test]
fn tar_path_escape_dotdot_entries_are_rejected() {
    for bad in [
        format!("{TOP}/\n../evil\n"),
        format!("{TOP}/\n{TOP}/../evil\n"),
        format!("{TOP}/\n{TOP}/bin/../../evil\n"),
        format!("{TOP}/\n{TOP}/bin/..\n"),
    ] {
        assert!(
            matches!(validate_listing(&bad, TOP), Err(InstallError::UnsafeArchive(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn tar_path_escape_absolute_and_foreign_top_level_entries_are_rejected() {
    for bad in [
        format!("{TOP}/\n/etc/passwd\n"),
        format!("{TOP}/\nother-dir/file\n"),
        format!("{TOP}/\n{TOP}-evil/file\n"),
        format!("{TOP}/\n./{TOP}/file\n"),
        format!("{TOP}/\n{TOP}//double-slash\n"),
        format!("{TOP}/\n{TOP}/./dot\n"),
    ] {
        assert!(
            matches!(validate_listing(&bad, TOP), Err(InstallError::UnsafeArchive(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn tar_path_escape_newline_smuggling_in_a_name_is_rejected() {
    // 이름에 개행이 들어 있으면 목록이 어긋나 '../../x' 같은 줄이 생긴다 — 최상위 접두 규칙으로 걸린다.
    let listing = format!("{TOP}/\n{TOP}/a\n../../x\n");
    assert!(validate_listing(&listing, TOP).is_err());
    let blank = format!("{TOP}/\n\n{TOP}/b\n");
    assert!(validate_listing(&blank, TOP).is_err());
}

#[test]
fn tar_empty_listing_is_rejected() {
    assert!(validate_listing("", TOP).is_err());
}

// ── 허용 출처 정책 ──

fn allows(policy: &FetchPolicy, s: &str) -> bool {
    policy.allows(&url::Url::parse(s).unwrap())
}

#[test]
fn redirect_policy_accepts_only_https_nodejs_org_dist() {
    let p = FetchPolicy::nodejs_org();
    assert!(allows(&p, "https://nodejs.org/dist/index.json"));
    assert!(allows(&p, "https://nodejs.org/dist/v24.22.0/SHASUMS256.txt"));
    assert!(allows(&p, "https://NODEJS.ORG/dist/index.json"));
}

#[test]
fn redirect_policy_rejects_other_hosts_schemes_ports_and_paths() {
    let p = FetchPolicy::nodejs_org();
    for bad in [
        "http://nodejs.org/dist/index.json",
        "https://evil.example/dist/index.json",
        "https://nodejs.org.evil.example/dist/index.json",
        "https://evil.example/nodejs.org/dist/index.json",
        "https://nodejs.org@evil.example/dist/index.json",
        "https://user:pw@nodejs.org/dist/index.json",
        "https://nodejs.org:8443/dist/index.json",
        "https://nodejs.org./dist/index.json",
        "https://www.nodejs.org/dist/index.json",
        "https://nodejs.org/",
        "https://nodejs.org/download/release/index.json",
        "https://nodejs.org/distx/index.json",
        "https://nodejs.org/dist/../etc/passwd",
        "https://nodejs.org/dist/%2e%2e/etc/passwd",
        "https://nodejs.org/dist/index.json#frag",
        "file:///etc/passwd",
        "ftp://nodejs.org/dist/index.json",
    ] {
        assert!(!allows(&p, bad), "거부돼야 하는 주소: {bad}");
    }
}
