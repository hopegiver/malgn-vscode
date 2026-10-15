// ==================== macOS 사용자 공간 Node 설치 파이프라인(P2-3a) ====================
// 관리자 권한 0회로 nodejs.org 공식 darwin tarball을 `~/.local/share/malgn-agent/node/<버전>`
// 에 설치하고 `current` 링크를 원자적으로 전환한다. 셸 rc·`~/.npmrc`는 건드리지 않는다.
//
// 검증·실행 순서(보안 불변식 — 이 순서를 바꾸면 안 된다):
//   1. index.json(https://nodejs.org/dist/, 리다이렉트는 같은 출처만)에서 LTS 버전 결정
//   2. SHASUMS256.txt에서 대상 파일명 행의 SHA256 확보
//   3. tarball 다운로드(상한·정체·취소 방어) → 디스크 파일을 다시 읽어 SHA256 대조
//   4. /usr/bin/tar -tf 목록 검사(경로 탈출) → /usr/bin/tar -xf (빈 전용 디렉터리) →
//      해제 트리 검사(심볼릭 링크 탈출·하드링크·특수 파일·setuid)
//   5. /usr/bin/xattr -dr com.apple.quarantine
//   6. /usr/bin/codesign --verify --strict + Team ID(HX7739G8FX) 대조 + Apple 신뢰 체인 요구사항
//   7. (여기까지 통과한 뒤에만) bin/node --version으로 버전 대조 — 3~6 어느 단계도 통과 전에는
//      다운로드물·해제물을 실행하지 않는다.
//   8. 같은 파일시스템의 스테이징에서 rename으로 버전 디렉터리 배치 → current 링크 원자 전환
//
// 원자성: 모든 작업은 `.staging-*`에서 하고 최종 반영은 rename 둘(버전 디렉터리, current 링크)
// 뿐이다. 어떤 단계가 실패·취소돼도 스테이징은 RAII로 지워지고, 이전 설치(이전 버전 디렉터리와
// current)는 그대로다. 부수효과는 전부 `InstallEnv`를 거치므로 실패를 주입해 테스트한다.

mod archive;
mod env;
mod fetch;
mod layout;
mod release;
mod types;

// 순수 로직·실제 HTTP 클라이언트 테스트는 모든 OS에서, tar/xattr/심볼릭 링크를 실제로 쓰는
// 파이프라인 테스트는 macOS에서만 돈다(시스템 bsdtar 전제 — CI의 macos 러너).
#[cfg(test)]
mod tests_pure;
#[cfg(test)]
mod tests_fetch;
#[cfg(all(test, target_os = "macos"))]
mod testkit;
#[cfg(all(test, target_os = "macos"))]
mod tests;
#[cfg(all(test, target_os = "macos"))]
mod tests_codesign;
#[cfg(all(test, target_os = "macos"))]
mod tests_sandbox;

pub(crate) use env::{InstallEnv, RealEnv};
// 다른 모듈(app_node)의 테스트가 가짜 `InstallEnv`를 만들 때 쓰는 타입 별칭.
#[cfg(test)]
pub(crate) use env::ToolRun as ToolRunForTests;
#[cfg(test)]
pub(crate) use fetch::FetchError as FetchErrorForTests;
pub(crate) use layout::{Layout, BASE_TILDE};
pub(crate) use types::{InstallError, NodeArch, NodeVersion};

use archive::{validate_listing, validate_tree};
use layout::{flip_current, prune_versions, read_current_tag, sweep_stale_staging, InstallLock, StagingDir};
use release::{expected_sha256, index_url, select_latest_lts, shasums_url, tarball_name, tarball_top_dir, tarball_url};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use types::{io_error, EXPECTED_TEAM_ID};

const TAR: &str = "/usr/bin/tar";
const XATTR: &str = "/usr/bin/xattr";
const CODESIGN: &str = "/usr/bin/codesign";

const INDEX_MAX_BYTES: u64 = 8 * 1024 * 1024;
const SHASUMS_MAX_BYTES: u64 = 256 * 1024;
/// 공식 darwin tarball은 ~50MB다. 3배 넘게 여유를 두되 비정상적으로 큰 응답은 막는다.
const TARBALL_MAX_BYTES: u64 = 160 * 1024 * 1024;
/// tarball(50MB) + 해제물(230MB) + 교체 중 이전 사본 여유.
const REQUIRED_FREE_BYTES: u64 = 700 * 1024 * 1024;

const TOOL_TIMEOUT_QUICK: Duration = Duration::from_secs(60);
const TOOL_TIMEOUT_LIST: Duration = Duration::from_secs(120);
const TOOL_TIMEOUT_EXTRACT: Duration = Duration::from_secs(600);

#[derive(Debug)]
pub(crate) struct InstallReport {
    pub(crate) version: NodeVersion,
    /// 설치 직전에 `current`가 가리키던 버전(없었으면 `None`).
    pub(crate) previous: Option<NodeVersion>,
    /// 같은 버전이 이미 검증된 상태로 있어 다운로드 없이 끝났는가(멱등 경로).
    pub(crate) already_installed: bool,
    pub(crate) log: Vec<String>,
}

fn check_cancel(cancel: &dyn Fn() -> bool) -> Result<(), InstallError> {
    if cancel() {
        Err(InstallError::Cancelled)
    } else {
        Ok(())
    }
}

fn path_arg(p: &Path) -> Result<String, InstallError> {
    p.to_str()
        .map(str::to_string)
        .ok_or_else(|| InstallError::Io(format!("UTF-8이 아닌 경로입니다: {}", p.display())))
}

fn fetch_to_vec(
    env: &dyn InstallEnv,
    url: &str,
    max_bytes: u64,
    cancel: &dyn Fn() -> bool,
    context: &str,
) -> Result<Vec<u8>, InstallError> {
    let mut buf: Vec<u8> = Vec::new();
    env.http_get(url, max_bytes, &mut buf, cancel)
        .map_err(|e| e.into_install_error(context))?;
    Ok(buf)
}

/// 설치할 LTS 버전을 정한다(네트워크 1회: index.json). 설치 파이프라인의 첫 단계이자,
/// 미리보기·plan_id가 "어떤 버전을 설치할 것인가"를 동의 화면에 보여주는 데도 쓴다.
pub(crate) fn resolve_latest(
    env: &dyn InstallEnv,
    arch: NodeArch,
    cancel: &dyn Fn() -> bool,
) -> Result<NodeVersion, InstallError> {
    let body = fetch_to_vec(env, &index_url(), INDEX_MAX_BYTES, cancel, "index.json")?;
    select_latest_lts(&body, arch)
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    Ok(hex)
}

/// `codesign -R`에 주는 코드 요구식(앞의 `=`는 "리터럴 요구식" 표기). 실제 codesign에서 의도대로
/// 평가되는지는 `tests_codesign`(macOS 한정, 네트워크 불필요)이 Apple 플랫폼 바이너리와 ad-hoc
/// 서명 사본으로 확인한다 — 이 문자열을 고치면 그 테스트가 함께 갱신돼야 한다.
fn developer_id_requirement() -> String {
    format!("=anchor apple generic and certificate leaf[subject.OU] = \"{EXPECTED_TEAM_ID}\"")
}

/// 코드서명 3중 검증. 하나라도 어긋나면 `Err`:
///   ① `codesign --verify --strict` — 서명이 유효하고 파일이 변조되지 않았다.
///   ② `codesign -dvv` 출력의 `TeamIdentifier=`가 정확히 `HX7739G8FX`다.
///   ③ `-R` 요구사항 — Apple 루트로 이어지는 Developer ID 체인(`anchor apple generic`)이면서
///      leaf 인증서 OU가 같은 Team ID다. ②만으로는 자체 서명 인증서가 같은 OU를 달고 있는
///      경우를 구분하지 못하므로 ③이 신뢰 체인을 닫는다.
fn verify_node_signature(
    env: &dyn InstallEnv,
    node_bin: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<(), InstallError> {
    let node = path_arg(node_bin)?;

    let verify = env.run_tool(
        CODESIGN,
        &["--verify".into(), "--strict".into(), "--verbose=2".into(), node.clone()],
        TOOL_TIMEOUT_QUICK,
        cancel,
    );
    if verify.cancelled {
        return Err(InstallError::Cancelled);
    }
    if !verify.succeeded() {
        return Err(InstallError::CodeSign(verify.describe_failure()));
    }

    let display = env.run_tool(
        CODESIGN,
        &["-dvv".into(), node.clone()],
        TOOL_TIMEOUT_QUICK,
        cancel,
    );
    if display.cancelled {
        return Err(InstallError::Cancelled);
    }
    if !display.succeeded() {
        return Err(InstallError::CodeSign(display.describe_failure()));
    }
    // codesign은 -d 출력을 stderr로 낸다. 어느 쪽에 나오든 읽는다.
    let mut team_ids: Vec<String> = Vec::new();
    for line in display.stderr.lines().chain(display.stdout.lines()) {
        if let Some(value) = line.trim().strip_prefix("TeamIdentifier=") {
            team_ids.push(value.trim().to_string());
        }
    }
    if team_ids.is_empty() {
        return Err(InstallError::TeamIdMismatch { found: "(없음)".to_string() });
    }
    if let Some(bad) = team_ids.iter().find(|t| t.as_str() != EXPECTED_TEAM_ID) {
        return Err(InstallError::TeamIdMismatch { found: bad.clone() });
    }

    let chain = env.run_tool(
        CODESIGN,
        &["--verify".into(), "-R".into(), developer_id_requirement(), node],
        TOOL_TIMEOUT_QUICK,
        cancel,
    );
    if chain.cancelled {
        return Err(InstallError::Cancelled);
    }
    if !chain.succeeded() {
        return Err(InstallError::CodeSign(format!(
            "Apple Developer ID 신뢰 체인 요구사항을 만족하지 않습니다: {}",
            chain.describe_failure()
        )));
    }
    Ok(())
}

/// 서명 검증을 통과한 `bin/node`만 실행해 버전을 대조한다.
fn check_node_version(
    env: &dyn InstallEnv,
    node_bin: &Path,
    expected: NodeVersion,
    cancel: &dyn Fn() -> bool,
) -> Result<(), InstallError> {
    let run = env.run_tool(&path_arg(node_bin)?, &["--version".into()], Duration::from_secs(20), cancel);
    if run.cancelled {
        return Err(InstallError::Cancelled);
    }
    let actual = run.stdout.trim().to_string();
    if !run.succeeded() || actual != expected.to_tag() {
        return Err(InstallError::VersionMismatch {
            expected: expected.to_tag(),
            actual: if actual.is_empty() { run.describe_failure() } else { actual },
        });
    }
    Ok(())
}

/// 서명·버전을 확인한 `node`로 npm 본체를 실제로 불러 본다(`node npm-cli.js --version`). 파일이
/// 있는지만 보는 `check_required_files`가 못 잡는 "npm JS가 깨졌다(디스크 오류·사용자 실수)"를
/// 잡아, 멱등 경로가 깨진 설치를 "정상"으로 넘기지 않고 다시 설치(교체)하게 한다. 읽기만 하는
/// 호출이다(설정·캐시 쓰기 없음). 한계: npm이 로드되는 수준까지만 본다 — 로드되지 않는 하위 파일의
/// 부분 손상은 이 검사로 알 수 없다.
fn check_npm_loads(
    env: &dyn InstallEnv,
    version_dir: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<(), InstallError> {
    let node_bin = version_dir.join("bin").join("node");
    let npm_cli = version_dir.join("lib").join("node_modules").join("npm").join("bin").join("npm-cli.js");
    let run = env.run_tool(
        &path_arg(&node_bin)?,
        &[path_arg(&npm_cli)?, "--version".into()],
        TOOL_TIMEOUT_QUICK,
        cancel,
    );
    if run.cancelled {
        return Err(InstallError::Cancelled);
    }
    if !run.succeeded() || run.stdout.trim().is_empty() {
        return Err(InstallError::Extract(format!("npm이 실행되지 않습니다: {}", run.describe_failure())));
    }
    Ok(())
}

/// `<root>/bin/{node,npm,npx}`와 npm 본체가 실제로 쓸 수 있는 상태인지(링크가 풀리는지) 본다.
fn check_required_files(root: &Path) -> Result<(), InstallError> {
    let missing = |what: &str| InstallError::Extract(format!("필수 파일이 없습니다: {what}"));
    let node = root.join("bin").join("node");
    if !std::fs::metadata(&node).map(|m| m.is_file()).unwrap_or(false) {
        return Err(missing("bin/node"));
    }
    for (link, target_file) in [("npm", "npm-cli.js"), ("npx", "npx-cli.js")] {
        let p = root.join("bin").join(link);
        let resolved = std::fs::canonicalize(&p).map_err(|_| missing(&format!("bin/{link}")))?;
        let ok = resolved.file_name().map(|n| n == target_file).unwrap_or(false)
            && std::fs::metadata(&resolved).map(|m| m.is_file()).unwrap_or(false);
        if !ok {
            return Err(missing(&format!("bin/{link} (→ {target_file})")));
        }
    }
    let pkg = root.join("lib").join("node_modules").join("npm").join("package.json");
    if !std::fs::metadata(&pkg).map(|m| m.is_file()).unwrap_or(false) {
        return Err(missing("lib/node_modules/npm/package.json"));
    }
    Ok(())
}

/// 이미 배치된 같은 버전이 쓸 수 있는 상태인가(필수 파일 + 서명 + 실행 버전 + npm 로드). 아니면 `Ok(false)`로
/// 교체 경로를 타 다시 설치한다(= 깨진 설치의 복구 경로). 취소만 오류로 전파한다.
fn existing_install_is_healthy(
    env: &dyn InstallEnv,
    layout: &Layout,
    version: NodeVersion,
    cancel: &dyn Fn() -> bool,
) -> Result<bool, InstallError> {
    let dir = layout.version_dir(version);
    if std::fs::symlink_metadata(&dir).is_err() {
        return Ok(false);
    }
    if check_required_files(&dir).is_err() {
        return Ok(false);
    }
    let node_bin = dir.join("bin").join("node");
    match verify_node_signature(env, &node_bin, cancel) {
        Ok(()) => {}
        Err(InstallError::Cancelled) => return Err(InstallError::Cancelled),
        Err(_) => return Ok(false),
    }
    match check_node_version(env, &node_bin, version, cancel) {
        Ok(()) => {}
        Err(InstallError::Cancelled) => return Err(InstallError::Cancelled),
        Err(_) => return Ok(false),
    }
    match check_npm_loads(env, &dir, cancel) {
        Ok(()) => Ok(true),
        Err(InstallError::Cancelled) => Err(InstallError::Cancelled),
        Err(_) => Ok(false),
    }
}

/// 버전을 고정(pinned)해 설치한다. 호출자가 동의 화면에 보여준 버전과 실제 설치 버전이
/// 갈라지지 않도록 "최신 결정"(`resolve_latest`)과 "설치"를 분리했다.
pub(crate) fn install_version(
    env: &dyn InstallEnv,
    layout: &Layout,
    arch: NodeArch,
    version: NodeVersion,
    cancel: &dyn Fn() -> bool,
) -> Result<InstallReport, InstallError> {
    let mut log: Vec<String> = Vec::new();
    check_cancel(cancel)?;

    // 같은 앱 인스턴스끼리는 EXECUTION_LOCK이 직렬화한다. 이 flock은 앱이 둘 뜬 경우의 방어선.
    let _lock = InstallLock::acquire(layout)?;
    sweep_stale_staging(layout);
    std::fs::create_dir_all(layout.node_root()).map_err(|e| io_error("node 폴더 생성", e))?;
    let previous = read_current_tag(layout);
    let tag = version.to_tag();

    // ── 멱등 경로: 같은 버전이 이미 쓸 수 있는 상태이면 다운로드 없이 current만 맞춘다.
    if existing_install_is_healthy(env, layout, version, cancel)? {
        log.push(format!("{tag}이(가) 이미 설치되어 있어 다운로드를 건너뜁니다(서명·버전·npm 실행 재확인 통과)."));
        let mut switched = false;
        if previous != Some(version) {
            check_cancel(cancel)?;
            flip_current(env, layout, &tag).map_err(|e| io_error("current 링크 전환", e))?;
            switched = true;
            log.push(format!("current → {tag}"));
        }
        ensure_npm_global(layout, &mut log);
        let mut keep = vec![version];
        keep.extend(previous);
        if switched {
            prune_versions(layout, &keep);
        }
        return Ok(InstallReport { version, previous, already_installed: true, log });
    }

    // ── 사전 점검: 여유 공간.
    if let Some(free) = env.free_space(&layout.base) {
        if free < REQUIRED_FREE_BYTES {
            return Err(InstallError::DiskFull(format!(
                "여유 {}MB, 필요 약 {}MB",
                free / (1024 * 1024),
                REQUIRED_FREE_BYTES / (1024 * 1024)
            )));
        }
    }

    let staging = StagingDir::create(&layout.base)?;
    let file_name = tarball_name(version, arch);
    let top = tarball_top_dir(version, arch);

    // ── 2. 체크섬.
    check_cancel(cancel)?;
    log.push(format!("[1/6] {tag} 공식 체크섬(SHASUMS256.txt) 확인"));
    let shasums = fetch_to_vec(env, &shasums_url(version), SHASUMS_MAX_BYTES, cancel, "SHASUMS256.txt")?;
    let shasums = String::from_utf8(shasums)
        .map_err(|_| InstallError::ChecksumMissing("SHASUMS256.txt가 UTF-8이 아닙니다".to_string()))?;
    let expected_hash = expected_sha256(&shasums, &file_name)?;

    // ── 3. 다운로드 + SHA256 대조.
    check_cancel(cancel)?;
    log.push(format!("[2/6] {file_name} 다운로드"));
    let tarball = staging.path().join(&file_name);
    {
        let mut sink = env
            .create_file(&tarball)
            .map_err(|e| io_error("다운로드 파일 생성", e))?;
        env.http_get(&tarball_url(version, arch), TARBALL_MAX_BYTES, &mut *sink, cancel)
            .map_err(|e| e.into_install_error("tarball 다운로드"))?;
    }
    let actual_hash = sha256_file(&tarball).map_err(|e| io_error("다운로드 파일 해시 계산", e))?;
    if actual_hash != expected_hash {
        return Err(InstallError::HashMismatch { expected: expected_hash, actual: actual_hash });
    }
    log.push(format!("[3/6] SHA256 일치({expected_hash})"));

    // ── 4. 해제(목록 검사 → 전용 디렉터리 해제 → 트리 검사).
    check_cancel(cancel)?;
    let tarball_arg = path_arg(&tarball)?;
    let listing = env.run_tool(TAR, &["-tf".into(), tarball_arg.clone()], TOOL_TIMEOUT_LIST, cancel);
    if listing.cancelled {
        return Err(InstallError::Cancelled);
    }
    if !listing.succeeded() {
        return Err(InstallError::Extract(format!("목록 읽기 실패: {}", listing.describe_failure())));
    }
    let entries = validate_listing(&listing.stdout, &top)?;
    let extract_dir = staging.path().join("extract");
    std::fs::create_dir(&extract_dir).map_err(|e| io_error("해제 디렉터리 생성", e))?;
    let extract = env.run_tool(
        TAR,
        &["-xf".into(), tarball_arg, "-C".into(), path_arg(&extract_dir)?, "--no-same-owner".into()],
        TOOL_TIMEOUT_EXTRACT,
        cancel,
    );
    if extract.cancelled {
        return Err(InstallError::Cancelled);
    }
    if !extract.succeeded() {
        let detail = extract.describe_failure();
        if detail.contains("No space left on device") || detail.contains("Disk quota exceeded") {
            return Err(InstallError::DiskFull(detail));
        }
        return Err(InstallError::Extract(detail));
    }
    let summary = validate_tree(&extract_dir, &top)?;
    log.push(format!(
        "[4/6] 해제·구조 검사 통과(목록 {entries}개, 파일 {} / 디렉터리 {} / 링크 {})",
        summary.files, summary.dirs, summary.symlinks
    ));
    let root = extract_dir.join(&top);

    // ── 5. 격리 속성 제거 → 코드서명 검증. 이 둘을 통과하기 전에는 해제물을 실행하지 않는다.
    check_cancel(cancel)?;
    let quarantine = env.run_tool(
        XATTR,
        &["-dr".into(), "com.apple.quarantine".into(), path_arg(&root)?],
        TOOL_TIMEOUT_EXTRACT,
        cancel,
    );
    if quarantine.cancelled {
        return Err(InstallError::Cancelled);
    }
    if !quarantine.succeeded() {
        return Err(InstallError::Quarantine(quarantine.describe_failure()));
    }
    let node_bin = root.join("bin").join("node");
    verify_node_signature(env, &node_bin, cancel)?;
    log.push(format!("[5/6] 코드서명 검증 통과(Team ID {EXPECTED_TEAM_ID})"));

    // ── 6. 검증 후 실행: 버전 대조 + 필수 파일.
    check_node_version(env, &node_bin, version, cancel)?;
    check_required_files(&root)?;

    // ── 7. 반영(되돌릴 수 있는 rename 둘). 이 지점 이후로는 취소하지 않는다.
    check_cancel(cancel)?;
    let target = layout.version_dir(version);
    let mut replaced: Option<std::path::PathBuf> = None;
    if std::fs::symlink_metadata(&target).is_ok() {
        // 같은 버전의 손상본 — 스테이징으로 치워 두었다가 성공하면 스테이징과 함께 삭제된다.
        let aside = staging.path().join("replaced");
        env.rename(&target, &aside).map_err(|e| io_error("기존 손상본 치우기", e))?;
        replaced = Some(aside);
    }
    if let Err(e) = env.rename(&root, &target) {
        if let Some(aside) = &replaced {
            let _ = env.rename(aside, &target);
        }
        return Err(io_error("버전 디렉터리 배치", e));
    }
    if previous != Some(version) {
        if let Err(e) = flip_current(env, layout, &tag) {
            // 되돌림: 새로 배치한 디렉터리를 스테이징으로 치우고 손상본이 있었다면 복원한다.
            let _ = env.rename(&target, &staging.path().join("failed-new"));
            if let Some(aside) = &replaced {
                let _ = env.rename(aside, &target);
            }
            return Err(io_error("current 링크 전환", e));
        }
    }
    log.push(format!("[6/6] {tag} 설치 완료 → {}", target.display()));

    ensure_npm_global(layout, &mut log);
    let mut keep = vec![version];
    keep.extend(previous);
    prune_versions(layout, &keep);

    Ok(InstallReport { version, previous, already_installed: false, log })
}

/// npm 전역 prefix 폴더. 없어도 `npm i -g`가 만들지만(실측), PATH 후보·쓰기권한 검사가
/// 안정적으로 동작하도록 만들어 둔다. 실패는 설치 성공을 뒤집지 않는다.
fn ensure_npm_global(layout: &Layout, log: &mut Vec<String>) {
    if let Err(e) = std::fs::create_dir_all(layout.npm_global()) {
        log.push(format!("npm 전역 폴더를 만들지 못했습니다(npm이 필요할 때 만듭니다): {e}"));
    }
}
