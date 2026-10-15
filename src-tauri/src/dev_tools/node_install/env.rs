// ==================== 주입 경계: 네트워크 · 명령 실행 · 파일시스템 ====================
// 설치 파이프라인(`mod.rs`)은 부수효과를 전부 `InstallEnv`를 통해서만 낸다. 프로덕션은
// `RealEnv`, 테스트는 실패를 주입하는 가짜 구현을 쓴다(해시 조작·Team ID 불일치·다운로드
// 중단·디스크 부족·취소·rename 실패 등). 파일시스템의 "읽기/디렉터리 조작"은 실제
// 임시 디렉터리를 쓰고, 실패를 주입해야 하는 지점(다운로드 파일 쓰기, rename, 외부 도구
// 실행, 여유 공간)만 이 트레이트로 뺐다.

use super::fetch::{FetchError, FetchPolicy, ReqwestFetcher};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

/// 외부 도구 한 번 실행의 결과.
#[derive(Debug, Default, Clone)]
pub(crate) struct ToolRun {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
    pub(crate) cancelled: bool,
    pub(crate) spawn_error: Option<String>,
}

impl ToolRun {
    pub(crate) fn succeeded(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out && !self.cancelled && self.spawn_error.is_none()
    }

    /// 사용자 메시지에 덧붙일 짧은 실패 설명.
    pub(crate) fn describe_failure(&self) -> String {
        if let Some(e) = &self.spawn_error {
            return e.clone();
        }
        if self.timed_out {
            return "시간 초과".to_string();
        }
        let detail = if self.stderr.trim().is_empty() { &self.stdout } else { &self.stderr };
        let detail: String = detail.trim().chars().take(400).collect();
        format!("종료 코드 {:?}: {detail}", self.exit_code)
    }
}

pub(crate) trait InstallEnv {
    /// `url`을 GET해 본문을 `sink`에 흘려 넣는다. 허용 출처 검사·리다이렉트 제한·크기
    /// 상한은 구현의 책임이다.
    fn http_get(
        &self,
        url: &str,
        max_bytes: u64,
        sink: &mut dyn Write,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError>;

    /// 다운로드 파일을 새로 만든다(이미 있으면 실패 — 스테이징 안의 새 이름).
    fn create_file(&self, path: &Path) -> std::io::Result<Box<dyn Write>>;

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;

    /// 시스템 도구를 절대경로로 실행한다(셸 없음). `cancel`이 참이 되면 프로세스 그룹을 정리한다.
    fn run_tool(
        &self,
        program: &str,
        args: &[String],
        timeout: Duration,
        cancel: &dyn Fn() -> bool,
    ) -> ToolRun;

    /// `path`가 속한 볼륨의 사용 가능 바이트. 알 수 없으면 `None`(검사 생략).
    fn free_space(&self, path: &Path) -> Option<u64>;
}

pub(crate) struct RealEnv {
    fetcher: ReqwestFetcher,
}

impl RealEnv {
    pub(crate) fn new() -> Self {
        RealEnv {
            fetcher: ReqwestFetcher::new(FetchPolicy::nodejs_org()),
        }
    }
}

impl InstallEnv for RealEnv {
    fn http_get(
        &self,
        url: &str,
        max_bytes: u64,
        sink: &mut dyn Write,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError> {
        self.fetcher.get(url, max_bytes, sink, cancel)
    }

    fn create_file(&self, path: &Path) -> std::io::Result<Box<dyn Write>> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        Ok(Box::new(opts.open(path)?))
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }

    fn run_tool(
        &self,
        program: &str,
        args: &[String],
        timeout: Duration,
        cancel: &dyn Fn() -> bool,
    ) -> ToolRun {
        // 기존 실행 엔진 재사용: stdin=null, 프로세스 그룹 kill, 파이프 리더 스레드,
        // 100ms 폴링 취소. PATH는 시스템 도구 디렉터리만 준다.
        let out = super::super::process::run_process_with_timeout_cancellable(
            program,
            args,
            "/usr/bin:/bin",
            &[],
            timeout,
            None,
            Some(cancel),
            None,
        );
        ToolRun {
            exit_code: out.exit_code,
            stdout: out.stdout,
            stderr: out.stderr,
            timed_out: out.timed_out,
            cancelled: out.aborted,
            spawn_error: out.spawn_error,
        }
    }

    #[cfg(unix)]
    fn free_space(&self, path: &Path) -> Option<u64> {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: c_path는 NUL 종료 문자열이고 stat은 초기화된 출력 버퍼다. 읽기 전용 syscall.
        let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
        if rc != 0 {
            return None;
        }
        Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
    }

    #[cfg(not(unix))]
    fn free_space(&self, _path: &Path) -> Option<u64> {
        None
    }
}
