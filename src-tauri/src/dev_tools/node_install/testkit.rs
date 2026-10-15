// 테스트 전용 도구상자: 가짜 `InstallEnv`(실패 주입), 임시 디렉터리, 최소 ustar 작성기,
// 설치 픽스처. 이 파일은 `#[cfg(test)]`로만 컴파일된다.

use super::env::{InstallEnv, ToolRun};
use super::fetch::FetchError;
use super::layout::Layout;
use super::release::{tarball_name, tarball_top_dir};
use super::types::{NodeArch, NodeVersion};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

// ---------------- 임시 디렉터리 ----------------

pub(super) struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub(super) fn new(label: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "r22-node-test-{}-{}-{}-{}",
            label,
            std::process::id(),
            n,
            nanos
        ));
        std::fs::create_dir_all(&path).unwrap();
        // macOS의 /var → /private/var 같은 심볼릭 링크 때문에 canonical 경로로 고정한다.
        let path = std::fs::canonicalize(&path).unwrap();
        TestDir { path }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        // 읽기 전용으로 바뀐 디렉터리가 있어도 지울 수 있게 권한을 되돌린다.
        let _ = std::process::Command::new("/bin/chmod")
            .args(["-R", "u+rwx"])
            .arg(&self.path)
            .output();
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------- 최소 ustar 작성기 ----------------

pub(super) enum TarKind {
    Dir,
    File { mode: u32, data: Vec<u8> },
    Symlink(String),
    Hardlink(String),
}

pub(super) struct TarEntry {
    pub(super) name: String,
    pub(super) kind: TarKind,
}

fn octal(buf: &mut [u8], value: u64) {
    let width = buf.len() - 1;
    let s = format!("{value:0width$o}");
    buf[..width].copy_from_slice(s.as_bytes());
    buf[width] = 0;
}

fn header(name: &str, mode: u32, size: u64, typeflag: u8, linkname: &str) -> [u8; 512] {
    let mut h = [0u8; 512];
    assert!(name.len() <= 100 && linkname.len() <= 100, "테스트 tar 이름은 100자 이하");
    h[..name.len()].copy_from_slice(name.as_bytes());
    octal(&mut h[100..108], mode as u64);
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size);
    octal(&mut h[136..148], 0);
    h[148..156].copy_from_slice(b"        ");
    h[156] = typeflag;
    h[157..157 + linkname.len()].copy_from_slice(linkname.as_bytes());
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    let sum: u32 = h.iter().map(|b| *b as u32).sum();
    let s = format!("{sum:06o}");
    h[148..154].copy_from_slice(s.as_bytes());
    h[154] = 0;
    h[155] = b' ';
    h
}

pub(super) fn build_tar(entries: &[TarEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        match &e.kind {
            TarKind::Dir => out.extend_from_slice(&header(&e.name, 0o755, 0, b'5', "")),
            TarKind::File { mode, data } => {
                out.extend_from_slice(&header(&e.name, *mode, data.len() as u64, b'0', ""));
                out.extend_from_slice(data);
                let pad = (512 - data.len() % 512) % 512;
                out.extend(std::iter::repeat_n(0u8, pad));
            }
            TarKind::Symlink(target) => out.extend_from_slice(&header(&e.name, 0o755, 0, b'2', target)),
            TarKind::Hardlink(target) => out.extend_from_slice(&header(&e.name, 0o644, 0, b'1', target)),
        }
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    out
}

pub(super) fn dir(name: &str) -> TarEntry {
    TarEntry { name: format!("{name}/"), kind: TarKind::Dir }
}

pub(super) fn file(name: &str, mode: u32, data: &str) -> TarEntry {
    TarEntry { name: name.to_string(), kind: TarKind::File { mode, data: data.as_bytes().to_vec() } }
}

pub(super) fn symlink(name: &str, target: &str) -> TarEntry {
    TarEntry { name: name.to_string(), kind: TarKind::Symlink(target.to_string()) }
}

pub(super) fn hardlink(name: &str, target: &str) -> TarEntry {
    TarEntry { name: name.to_string(), kind: TarKind::Hardlink(target.to_string()) }
}

/// 공식 tarball과 같은 모양의 정상 항목들.
pub(super) fn good_entries(top: &str) -> Vec<TarEntry> {
    vec![
        dir(top),
        dir(&format!("{top}/bin")),
        file(&format!("{top}/bin/node"), 0o755, "#!/bin/sh\necho fake-node\n"),
        symlink(&format!("{top}/bin/npm"), "../lib/node_modules/npm/bin/npm-cli.js"),
        symlink(&format!("{top}/bin/npx"), "../lib/node_modules/npm/bin/npx-cli.js"),
        dir(&format!("{top}/lib")),
        dir(&format!("{top}/lib/node_modules")),
        dir(&format!("{top}/lib/node_modules/npm")),
        dir(&format!("{top}/lib/node_modules/npm/bin")),
        file(&format!("{top}/lib/node_modules/npm/bin/npm-cli.js"), 0o644, "// npm\n"),
        file(&format!("{top}/lib/node_modules/npm/bin/npx-cli.js"), 0o644, "// npx\n"),
        file(&format!("{top}/lib/node_modules/npm/package.json"), 0o644, "{}\n"),
    ]
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------- 가짜 환경 ----------------

#[derive(Clone)]
pub(super) enum Route {
    Body(Vec<u8>),
    /// 앞부분만 쓰고 네트워크 오류로 끊긴다(다운로드 중단).
    Truncated { prefix: Vec<u8> },
    Rejected,
    Status(u16),
}

type Hook = Box<dyn Fn() + Send>;

pub(super) struct FakeEnv {
    pub(super) routes: Mutex<HashMap<String, Route>>,
    pub(super) calls: Mutex<Vec<String>>,
    /// 다운로드 파일에 이 바이트 수 이후 쓰면 StorageFull(디스크 부족).
    pub(super) disk_full_after: Mutex<Option<u64>>,
    pub(super) free_space: Mutex<Option<u64>>,
    /// `to` 경로의 마지막 성분이 이 중 하나이면 그 rename을 한 번 실패시킨다(소모됨).
    pub(super) fail_rename_to: Mutex<Vec<String>>,
    pub(super) tar_extract_override: Mutex<Option<ToolRun>>,
    pub(super) codesign_verify_ok: Mutex<bool>,
    pub(super) team_id: Mutex<String>,
    pub(super) chain_ok: Mutex<bool>,
    pub(super) xattr_ok: Mutex<bool>,
    pub(super) node_version_output: Mutex<String>,
    /// http_get이 청크 n개를 쓴 뒤 호출할 훅(취소 시점 제어용).
    pub(super) after_chunks: Mutex<Option<(usize, Hook)>>,
    /// 지정한 프로그램 호출 직전에 훅 실행(예: tar 직전에 취소 플래그 올리기).
    pub(super) before_run: Mutex<Option<(String, Hook)>>,
}

impl FakeEnv {
    pub(super) fn new() -> Self {
        FakeEnv {
            routes: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            disk_full_after: Mutex::new(None),
            free_space: Mutex::new(Some(100 * 1024 * 1024 * 1024)),
            fail_rename_to: Mutex::new(Vec::new()),
            tar_extract_override: Mutex::new(None),
            codesign_verify_ok: Mutex::new(true),
            team_id: Mutex::new(super::types::EXPECTED_TEAM_ID.to_string()),
            chain_ok: Mutex::new(true),
            xattr_ok: Mutex::new(true),
            node_version_output: Mutex::new(String::new()),
            after_chunks: Mutex::new(None),
            before_run: Mutex::new(None),
        }
    }

    pub(super) fn route(&self, url: &str, route: Route) {
        self.routes.lock().unwrap().insert(url.to_string(), route);
    }

    pub(super) fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    pub(super) fn count_prefix(&self, prefix: &str) -> usize {
        self.calls().iter().filter(|c| c.starts_with(prefix)).count()
    }

    /// 어떤 외부 도구도(tar·xattr·codesign·node) 실행되지 않았다.
    pub(super) fn ran_no_tools(&self) -> bool {
        self.count_prefix("RUN ") == 0
    }

    pub(super) fn ran_program(&self, needle: &str) -> bool {
        self.calls().iter().any(|c| c.starts_with("RUN ") && c.contains(needle))
    }

    fn really_run(program: &str, args: &[String]) -> ToolRun {
        // fork~exec 창에서 다른 테스트의 fd(잠금 파일 등)가 상속되는 경합을 줄이기 위해 spawn을 직렬화한다.
        let _guard = crate::process_util::test_support::SPAWN_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let out = std::process::Command::new(program).args(args).output();
        match out {
            Ok(o) => ToolRun {
                exit_code: o.status.code(),
                stdout: String::from_utf8_lossy(&o.stdout).to_string(),
                stderr: String::from_utf8_lossy(&o.stderr).to_string(),
                ..Default::default()
            },
            Err(e) => ToolRun { spawn_error: Some(e.to_string()), ..Default::default() },
        }
    }
}

struct LimitedWriter {
    inner: std::fs::File,
    remaining: u64,
}

impl Write for LimitedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if (buf.len() as u64) > self.remaining {
            // 일부는 쓰고 나머지에서 ENOSPC — 실제 디스크 가득 참과 같은 모양.
            let n = self.remaining as usize;
            if n > 0 {
                self.inner.write_all(&buf[..n])?;
                self.remaining = 0;
                return Ok(n);
            }
            return Err(std::io::Error::from_raw_os_error(libc::ENOSPC));
        }
        self.remaining -= buf.len() as u64;
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl InstallEnv for FakeEnv {
    fn http_get(
        &self,
        url: &str,
        _max_bytes: u64,
        sink: &mut dyn Write,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError> {
        self.calls.lock().unwrap().push(format!("GET {url}"));
        let route = self.routes.lock().unwrap().get(url).cloned();
        let write_chunks = |bytes: &[u8], sink: &mut dyn Write| -> Result<(), FetchError> {
            for (i, chunk) in bytes.chunks(16 * 1024).enumerate() {
                if cancel() {
                    return Err(FetchError::Cancelled);
                }
                sink.write_all(chunk).map_err(FetchError::Sink)?;
                let hook = self.after_chunks.lock().unwrap();
                if let Some((n, f)) = hook.as_ref() {
                    if i + 1 == *n {
                        f();
                    }
                }
            }
            Ok(())
        };
        match route {
            None => Err(FetchError::Network(format!("라우트 없음: {url}"))),
            Some(Route::Body(bytes)) => {
                write_chunks(&bytes, sink)?;
                if cancel() {
                    return Err(FetchError::Cancelled);
                }
                Ok(())
            }
            Some(Route::Truncated { prefix }) => {
                write_chunks(&prefix, sink)?;
                Err(FetchError::Network("connection reset by peer".to_string()))
            }
            Some(Route::Rejected) => Err(FetchError::Rejected(format!("{url} → https://evil.example/x"))),
            Some(Route::Status(code)) => Err(FetchError::Status(code)),
        }
    }

    fn create_file(&self, path: &Path) -> std::io::Result<Box<dyn Write>> {
        self.calls.lock().unwrap().push(format!("CREATE {}", path.file_name().unwrap().to_string_lossy()));
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
        match *self.disk_full_after.lock().unwrap() {
            Some(limit) => Ok(Box::new(LimitedWriter { inner: file, remaining: limit })),
            None => Ok(Box::new(file)),
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        let to_name = to.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.calls.lock().unwrap().push(format!("RENAME {to_name}"));
        {
            // 한 번만 실패시킨다(롤백 rename이 같은 이름을 쓰는 경우에도 복원은 성공해야 한다).
            let mut rules = self.fail_rename_to.lock().unwrap();
            if let Some(i) = rules.iter().position(|n| *n == to_name) {
                rules.remove(i);
                return Err(std::io::Error::other("주입된 rename 실패"));
            }
        }
        std::fs::rename(from, to)
    }

    fn run_tool(
        &self,
        program: &str,
        args: &[String],
        _timeout: Duration,
        cancel: &dyn Fn() -> bool,
    ) -> ToolRun {
        let label = program.rsplit('/').next().unwrap_or(program);
        // 훅은 호출 기록 뒤, 취소 확인 앞에서 실행한다(훅이 취소 플래그를 올리는 시나리오).
        let first_arg = args.first().cloned().unwrap_or_default();
        self.calls.lock().unwrap().push(format!("RUN {label} {first_arg}"));
        {
            let hook = self.before_run.lock().unwrap();
            if let Some((needle, f)) = hook.as_ref() {
                if label.contains(needle.as_str()) || first_arg.contains(needle.as_str()) {
                    f();
                }
            }
        }
        if cancel() {
            return ToolRun { cancelled: true, ..Default::default() };
        }
        match program {
            "/usr/bin/tar" => {
                if first_arg == "-xf" {
                    if let Some(over) = self.tar_extract_override.lock().unwrap().clone() {
                        return over;
                    }
                }
                Self::really_run(program, args)
            }
            "/usr/bin/xattr" => {
                if *self.xattr_ok.lock().unwrap() {
                    Self::really_run(program, args)
                } else {
                    ToolRun { exit_code: Some(1), stderr: "xattr: 주입된 실패".to_string(), ..Default::default() }
                }
            }
            "/usr/bin/codesign" => {
                if args.iter().any(|a| a == "-dvv") {
                    let team = self.team_id.lock().unwrap().clone();
                    ToolRun {
                        exit_code: Some(0),
                        stderr: format!("Identifier=node\nAuthority=Developer ID Application: Test ({team})\nTeamIdentifier={team}\n"),
                        ..Default::default()
                    }
                } else if args.iter().any(|a| a == "-R") {
                    if *self.chain_ok.lock().unwrap() {
                        ToolRun { exit_code: Some(0), ..Default::default() }
                    } else {
                        ToolRun { exit_code: Some(3), stderr: "test-requirement: code failed to satisfy specified code requirement(s)".to_string(), ..Default::default() }
                    }
                } else if *self.codesign_verify_ok.lock().unwrap() {
                    ToolRun { exit_code: Some(0), stderr: "valid on disk".to_string(), ..Default::default() }
                } else {
                    ToolRun { exit_code: Some(1), stderr: "a sealed resource is missing or invalid".to_string(), ..Default::default() }
                }
            }
            // 앱이 서명 확인을 마친 node로 npm 본체를 불러 보는 호출(`node …/npm-cli.js --version`):
            // 실제 파일 내용으로 판정한다(픽스처의 정상 npm-cli.js는 "// npm"으로 시작).
            p if p.ends_with("/bin/node") && args.first().is_some_and(|a| a.ends_with("npm-cli.js")) => {
                let ok = std::fs::read_to_string(&args[0]).map(|c| c.starts_with("// npm")).unwrap_or(false);
                if ok {
                    ToolRun { exit_code: Some(0), stdout: "10.9.0\n".to_string(), ..Default::default() }
                } else {
                    ToolRun { exit_code: Some(1), stderr: "Error: Cannot find module 'npm'".to_string(), ..Default::default() }
                }
            }
            p if p.ends_with("/bin/node") => {
                let out = self.node_version_output.lock().unwrap().clone();
                ToolRun { exit_code: Some(0), stdout: out, ..Default::default() }
            }
            other => ToolRun { spawn_error: Some(format!("테스트가 모르는 프로그램: {other}")), ..Default::default() },
        }
    }

    fn free_space(&self, _path: &Path) -> Option<u64> {
        *self.free_space.lock().unwrap()
    }
}

// ---------------- 설치 픽스처 ----------------

pub(super) const ARCH: NodeArch = NodeArch::Arm64;

pub(super) fn v(major: u32, minor: u32, patch: u32) -> NodeVersion {
    NodeVersion { major, minor, patch }
}

pub(super) struct Fixture {
    pub(super) _home: TestDir,
    pub(super) layout: Layout,
    pub(super) env: FakeEnv,
    pub(super) version: NodeVersion,
    pub(super) tar_bytes: Vec<u8>,
}

pub(super) fn index_json_for(entries: &[(&str, &str, bool)]) -> Vec<u8> {
    let items: Vec<String> = entries
        .iter()
        .map(|(ver, lts, has_tar)| {
            let lts_json = if lts.is_empty() { "false".to_string() } else { format!("\"{lts}\"") };
            let files = if *has_tar { "[\"osx-arm64-tar\",\"osx-x64-tar\",\"src\"]" } else { "[\"src\"]" };
            format!("{{\"version\":\"{ver}\",\"lts\":{lts_json},\"files\":{files}}}")
        })
        .collect();
    format!("[{}]", items.join(",")).into_bytes()
}

impl Fixture {
    /// 정상 tarball을 서빙하는 환경. `version`은 설치 대상.
    pub(super) fn new(label: &str) -> Self {
        Self::with_entries(label, good_entries)
    }

    pub(super) fn with_entries(label: &str, make: impl Fn(&str) -> Vec<TarEntry>) -> Self {
        let home = TestDir::new(label);
        let layout = Layout::for_home(home.path());
        let version = v(24, 22, 0);
        let top = tarball_top_dir(version, ARCH);
        let tar_bytes = build_tar(&make(&top));
        let env = FakeEnv::new();
        *env.node_version_output.lock().unwrap() = format!("{}\n", version.to_tag());
        let fx = Fixture { _home: home, layout, env, version, tar_bytes };
        fx.serve_defaults();
        fx
    }

    pub(super) fn serve_defaults(&self) {
        let name = tarball_name(self.version, ARCH);
        let hash = sha256_hex(&self.tar_bytes);
        self.env.route(
            &super::release::index_url(),
            Route::Body(index_json_for(&[
                ("v26.12.0", "", true),
                ("v24.22.0", "Krypton", true),
                ("v22.23.0", "Jod", true),
            ])),
        );
        self.env.route(
            &super::release::shasums_url(self.version),
            Route::Body(format!("{hash}  {name}\n{}  other-file.tar.gz\n", "0".repeat(64)).into_bytes()),
        );
        self.env.route(&super::release::tarball_url(self.version, ARCH), Route::Body(self.tar_bytes.clone()));
    }

    pub(super) fn install(&self, cancel: &dyn Fn() -> bool) -> Result<super::InstallReport, super::InstallError> {
        super::install_version(&self.env, &self.layout, ARCH, self.version, cancel)
    }

    pub(super) fn install_ok(&self) -> super::InstallReport {
        self.install(&|| false).expect("설치가 성공해야 합니다")
    }

    /// 이전에 설치돼 있던 버전을 흉내 낸다(표식 파일 + current 링크). 이 상태가 실패 후에도
    /// 그대로여야 한다.
    pub(super) fn seed_prior_install(&self, prior: NodeVersion) {
        let dir = self.layout.version_dir(prior);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join("node"), "PRIOR-NODE").unwrap();
        std::fs::write(dir.join("MARKER"), "prior-install-intact").unwrap();
        std::os::unix::fs::symlink(prior.to_tag(), self.layout.current_link()).unwrap();
        std::fs::create_dir_all(self.layout.npm_global().join("lib")).unwrap();
        std::fs::write(self.layout.npm_global().join("lib").join("KEEP"), "global-pkg").unwrap();
    }

    pub(super) fn staging_leftovers(&self) -> Vec<String> {
        match std::fs::read_dir(&self.layout.base) {
            Ok(read) => read
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.starts_with(".staging-"))
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// 실패 후 불변식: 이전 설치 보존 + 스테이징 잔여물 0 + 새 버전 디렉터리 없음.
    pub(super) fn assert_prior_preserved_and_clean(&self, prior: NodeVersion) {
        assert_eq!(self.staging_leftovers(), Vec::<String>::new(), "스테이징 잔여물이 남았습니다");
        let marker = self.layout.version_dir(prior).join("MARKER");
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "prior-install-intact", "이전 설치가 변했습니다");
        let target = std::fs::read_link(self.layout.current_link()).expect("current 링크가 있어야 합니다");
        assert_eq!(target.to_string_lossy(), prior.to_tag(), "current가 이전 버전을 가리켜야 합니다");
        assert!(
            std::fs::symlink_metadata(self.layout.version_dir(self.version)).is_err(),
            "부분 설치된 새 버전 디렉터리가 남았습니다"
        );
        assert_eq!(
            std::fs::read_to_string(self.layout.npm_global().join("lib").join("KEEP")).unwrap(),
            "global-pkg",
            "npm 전역 폴더가 변했습니다"
        );
        // .current-new-* 임시 링크도 남으면 안 된다.
        let stray: Vec<String> = std::fs::read_dir(self.layout.node_root())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with(".current-new-"))
            .collect();
        assert!(stray.is_empty(), "임시 current 링크가 남았습니다: {stray:?}");
    }

    /// 아무것도 없던 상태에서 실패한 경우: 기준 폴더에 설치 흔적이 없어야 한다.
    pub(super) fn assert_clean_first_install_failure(&self) {
        assert_eq!(self.staging_leftovers(), Vec::<String>::new(), "스테이징 잔여물이 남았습니다");
        assert!(
            std::fs::symlink_metadata(self.layout.version_dir(self.version)).is_err(),
            "부분 설치된 버전 디렉터리가 남았습니다"
        );
        assert!(
            std::fs::symlink_metadata(self.layout.current_link()).is_err(),
            "current 링크가 생겼습니다"
        );
    }
}
