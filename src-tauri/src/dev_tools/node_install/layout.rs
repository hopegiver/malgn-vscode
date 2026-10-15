// ==================== 기준 폴더 배치 · 잠금 · 스테이징 · 링크 전환 ====================
// 앱이 관리하는 Node의 디스크 배치(정본):
//
//   <home>/.local/share/malgn-agent/            기준 폴더(BASE)
//     node/<vX.Y.Z>/                            버전별 설치(tarball 내용, 최상위 디렉터리 제거)
//     node/current  ->  <vX.Y.Z>                현재 버전 링크(상대 링크, rename으로 원자 전환)
//     npm-global/                               npm 전역 prefix(node/ 밖 형제 — Node 버전이
//                                               바뀌어도 전역 패키지가 사라지지 않는다)
//     .staging-*                                설치 작업용 임시 디렉터리(같은 파일시스템 → rename)
//     .node-install.lock                        동시 설치 방지 잠금 파일
//
// 셸 rc 파일·`~/.npmrc`는 건드리지 않는다(npm prefix는 자식 프로세스 env로만 주입).

use super::env::InstallEnv;
use super::types::{io_error, InstallError, NodeVersion};
use std::path::{Path, PathBuf};

/// 후보 경로 문자열(`~/` 표기)의 정본 접두. DEV_TOOLS 후보·NPM_CANDIDATES·기본 PATH가
/// 이 문자열(또는 아래 `Layout`)에서 어긋나지 않도록 드리프트 가드 테스트가 대조한다.
pub(crate) const BASE_TILDE: &str = "~/.local/share/malgn-agent";

#[derive(Clone, Debug)]
pub(crate) struct Layout {
    pub(crate) base: PathBuf,
}

impl Layout {
    pub(crate) fn for_home(home: &Path) -> Self {
        Layout {
            base: home.join(".local").join("share").join("malgn-agent"),
        }
    }

    pub(crate) fn node_root(&self) -> PathBuf {
        self.base.join("node")
    }

    pub(crate) fn version_dir(&self, v: NodeVersion) -> PathBuf {
        self.node_root().join(v.to_tag())
    }

    pub(crate) fn current_link(&self) -> PathBuf {
        self.node_root().join("current")
    }

    /// 앱 관리 Node의 bin 디렉터리(`node`, `npm`, `npx`) — `current` 링크를 거친 경로.
    pub(crate) fn node_bin_dir(&self) -> PathBuf {
        self.current_link().join("bin")
    }

    pub(crate) fn npm_global(&self) -> PathBuf {
        self.base.join("npm-global")
    }

    /// npm 전역 설치 실행 파일(shim)이 놓이는 디렉터리(`claude`, `pnpm`, `wrangler` …).
    pub(crate) fn npm_global_bin(&self) -> PathBuf {
        self.npm_global().join("bin")
    }

    pub(crate) fn lock_path(&self) -> PathBuf {
        self.base.join(".node-install.lock")
    }

    pub(crate) fn staging_prefix() -> &'static str {
        ".staging-"
    }

    /// `path`가 이 앱의 Node 폴더(`node/`) 아래인가. 심볼릭 링크를 풀기 전/후 경로 모두
    /// 다룰 수 있게 기준 폴더의 원문과 canonical 두 가지로 비교한다.
    pub(crate) fn contains_node_path(&self, path: &Path) -> bool {
        path_is_under(path, &self.node_root())
    }

    /// `path`가 npm 전역 prefix(`npm-global/`) 아래인가(원문·canonical 모두 비교).
    pub(crate) fn contains_npm_global_path(&self, path: &Path) -> bool {
        path_is_under(path, &self.npm_global())
    }
}

/// `dir` 아래인가: 원문 비교 + `dir`을 canonicalize한 값과의 비교(홈 경로 자체가 심볼릭 링크를
/// 거치는 환경에서 canonical 경로와 원문 경로가 어긋나는 경우를 흡수한다). 어휘적
/// `starts_with`라 `node-evil/` 같은 형제 접두는 걸리지 않는다(성분 단위 비교).
pub(crate) fn path_is_under(path: &Path, dir: &Path) -> bool {
    if path.starts_with(dir) {
        return true;
    }
    match std::fs::canonicalize(dir) {
        Ok(canon) => path.starts_with(canon),
        Err(_) => false,
    }
}

// ---------------- 스테이징 ----------------

/// 설치 작업용 임시 디렉터리. 스코프를 벗어나면(성공·실패·취소·패닉 모두) 통째로 지운다.
/// 최종 위치로 `rename`된 항목은 이미 이 디렉터리 밖에 있으므로 영향받지 않는다.
pub(crate) struct StagingDir {
    path: PathBuf,
}

impl StagingDir {
    pub(crate) fn create(base: &Path) -> Result<Self, InstallError> {
        std::fs::create_dir_all(base).map_err(|e| io_error("기준 폴더 생성", e))?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = base.join(format!("{}{}-{}", Layout::staging_prefix(), std::process::id(), nanos));
        std::fs::create_dir(&path).map_err(|e| io_error("스테이징 디렉터리 생성", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
        }
        Ok(StagingDir { path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 잠금을 잡은 상태에서만 호출한다: 이전 실행이 남긴 `.staging-*`는 (잠금이 있으니) 반드시
/// 죽은 실행의 잔여물이다.
pub(crate) fn sweep_stale_staging(base: &Path) {
    let Ok(read) = std::fs::read_dir(base) else { return };
    for entry in read.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(Layout::staging_prefix()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

// ---------------- 디스크 잠금 ----------------

/// 프로세스 간 동시 설치 방지. `flock`은 프로세스가 죽으면 자동으로 풀리므로 stale 잠금
/// 파일 문제가 없다(앱 내부 동시성은 `EXECUTION_LOCK`이 이미 직렬화한다 — 이 잠금은
/// 앱 인스턴스가 둘 뜬 경우를 위한 두 번째 방어선이다).
pub(crate) struct InstallLock {
    _file: std::fs::File,
}

/// 잠금 충돌을 곧바로 `Locked`로 단정하기 전에 기다리는 시간. 다른 스레드가 `fork`한 자식이
/// `exec`하기 전 아주 짧은 순간 이 fd를 물고 있을 수 있어(fork~exec 창) 일시적으로 충돌처럼
/// 보인다 — 실제로 다른 설치가 진행 중이면 이 시간보다 훨씬 오래 잠겨 있다.
const LOCK_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

impl InstallLock {
    pub(crate) fn acquire(layout: &Layout) -> Result<Self, InstallError> {
        Self::acquire_with_grace(layout, LOCK_GRACE)
    }

    fn acquire_with_grace(layout: &Layout, grace: std::time::Duration) -> Result<Self, InstallError> {
        std::fs::create_dir_all(&layout.base).map_err(|e| io_error("기준 폴더 생성", e))?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(layout.lock_path())
            .map_err(|e| io_error("잠금 파일 열기", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let deadline = std::time::Instant::now() + grace;
            loop {
                // SAFETY: 유효한 fd에 대한 advisory 잠금 호출. 비블로킹.
                let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
                if rc == 0 {
                    break;
                }
                let err = std::io::Error::last_os_error();
                if err.kind() != std::io::ErrorKind::WouldBlock {
                    return Err(io_error("설치 잠금", err));
                }
                if std::time::Instant::now() >= deadline {
                    return Err(InstallError::Locked);
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
        #[cfg(not(unix))]
        let _ = grace;
        Ok(InstallLock { _file: file })
    }
}

// ---------------- current 링크 ----------------

/// `current`가 가리키는 버전 디렉터리 이름(`v24.22.0`). 링크가 없거나 형식이 다르면 `None`.
pub(crate) fn read_current_tag(layout: &Layout) -> Option<NodeVersion> {
    let target = std::fs::read_link(layout.current_link()).ok()?;
    let name = target.file_name()?.to_str()?;
    NodeVersion::parse_tag(name)
}

#[cfg(unix)]
fn make_symlink(target: &str, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn make_symlink(_target: &str, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "심볼릭 링크는 이 플랫폼에서 지원하지 않습니다",
    ))
}

/// `current`를 `tag` 디렉터리로 원자 전환한다: 임시 이름으로 새 상대 링크를 만든 뒤
/// `rename`으로 덮어쓴다(POSIX rename은 링크 교체가 원자적이다). 실패하면 임시 링크를
/// 지우고 기존 `current`는 그대로다.
pub(crate) fn flip_current(env: &dyn InstallEnv, layout: &Layout, tag: &str) -> std::io::Result<()> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp = layout
        .node_root()
        .join(format!(".current-new-{}-{}", std::process::id(), nanos));
    make_symlink(tag, &tmp)?;
    if let Err(e) = env.rename(&tmp, &layout.current_link()) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// 정리: `keep`에 없는 `vX.Y.Z` 형식 디렉터리만 지운다(형식이 다른 이름·`current`·점 파일은
/// 건드리지 않는다). 실패는 무시한다 — 정리는 설치 성공 여부와 무관한 후처리다.
pub(crate) fn prune_versions(layout: &Layout, keep: &[NodeVersion]) {
    let Ok(read) = std::fs::read_dir(layout.node_root()) else { return };
    for entry in read.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        let Some(v) = NodeVersion::parse_tag(&name) else { continue };
        if keep.contains(&v) {
            continue;
        }
        let path = entry.path();
        if std::fs::symlink_metadata(&path).map(|m| m.is_dir()).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}
