// ==================== 공용 타입: 버전·아키텍처·오류 ====================
// macOS 사용자 공간 Node 설치(P2-3a)의 어휘를 한 곳에 둔다. 이 모듈의 값은 전부
// "검증을 통과한 것만 담는" 타입이다 — 특히 `NodeVersion`은 엄격 정규식 검증을
// 통과해야만 만들어지므로, URL·디렉터리 이름·argv에 들어가는 버전 문자열은 항상
// 이 타입을 거쳐 재조립된 값이다(경로 주입 차단).

use std::fmt;

/// 번들된 Node 실행파일의 서명 Team ID(OpenJS Foundation, 이전 Node.js Foundation).
/// 공식 darwin 배포본의 `bin/node`가 이 Team ID의 Developer ID로 서명돼 있음을
/// 실측으로 확인했다(`codesign -dvv` → `TeamIdentifier=HX7739G8FX`). 다운로드한
/// 바이너리가 이 값과 다르면 설치를 중단한다 — SHA256(SHASUMS256.txt)과는 독립인
/// 두 번째 신뢰 근거다.
pub(crate) const EXPECTED_TEAM_ID: &str = "HX7739G8FX";

/// 출처는 이 접두 하나로 고정한다. 모든 URL은 이 접두 + 검증된 `NodeVersion` +
/// 코드가 만든 파일명으로만 조립한다(사용자·네트워크 입력이 URL에 들어가지 않는다).
pub(crate) const DIST_BASE: &str = "https://nodejs.org/dist";

/// 이 앱이 설치하는 Node의 메이저 하한. (결정 01m4zfesw2qvhnth3jjg89w04k: 메이저
/// 고정은 하지 않되, 오래된 LTS 라인은 제외한다.)
pub(crate) const MIN_MAJOR: u32 = 22;

/// 버전 각 자리의 상한 — 정규식은 `\d+`지만 비정상적으로 긴 숫자를 받아들일 이유가 없다.
const MAX_COMPONENT: u32 = 999_999;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NodeArch {
    Arm64,
    X64,
}

impl NodeArch {
    /// 이 앱 바이너리의 아키텍처. 지원하지 않는 값이면 `None`.
    pub(crate) fn current() -> Option<Self> {
        match std::env::consts::ARCH {
            "aarch64" => Some(NodeArch::Arm64),
            "x86_64" => Some(NodeArch::X64),
            _ => None,
        }
    }

    /// 배포 파일명에 쓰는 토큰(`node-vX-darwin-<token>.tar.gz`).
    pub(crate) fn dist_token(self) -> &'static str {
        match self {
            NodeArch::Arm64 => "arm64",
            NodeArch::X64 => "x64",
        }
    }

    /// `index.json`의 `files` 배열에 들어 있는 항목 이름.
    pub(crate) fn index_files_key(self) -> String {
        format!("osx-{}-tar", self.dist_token())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NodeVersion {
    pub(crate) major: u32,
    pub(crate) minor: u32,
    pub(crate) patch: u32,
}

impl NodeVersion {
    /// 엄격 파서: `^v\d+\.\d+\.\d+$`만 받는다. 선행 0(`v024.1.1`)·공백·접미(`-rc.1`)·
    /// 경로 문자(`/`, `..`)는 전부 `None`이다. 재조립한 `to_tag()`가 입력과 정확히
    /// 같아야 통과시키므로, 이 타입에서 나온 문자열만 URL·경로에 쓰인다.
    pub(crate) fn parse_tag(s: &str) -> Option<Self> {
        let rest = s.strip_prefix('v')?;
        Self::parse_numbers(rest)
    }

    /// `node --version`을 `normalize_version`으로 정규화한 값("24.22.0")처럼 `v`가
    /// 없는 형태도 같은 엄격함으로 파싱한다.
    pub(crate) fn parse_plain(s: &str) -> Option<Self> {
        Self::parse_numbers(s)
    }

    fn parse_numbers(s: &str) -> Option<Self> {
        let mut parts = s.split('.');
        let major = parse_component(parts.next()?)?;
        let minor = parse_component(parts.next()?)?;
        let patch = parse_component(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(NodeVersion { major, minor, patch })
    }

    /// `v24.22.0` — 디렉터리 이름·배포 URL의 버전 토큰.
    pub(crate) fn to_tag(self) -> String {
        format!("v{}.{}.{}", self.major, self.minor, self.patch)
    }

}

fn parse_component(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // 선행 0 금지("0"은 허용) — 재조립 문자열이 원문과 달라지는 입력을 막는다.
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    if n > MAX_COMPONENT {
        return None;
    }
    Some(n)
}

/// 설치 파이프라인의 모든 실패. 사용자에게 보이는 문구는 `Display`다 — 각 변형은
/// "기존 설치가 보존됐다"는 사실을 전제로 하므로(원자성 불변식) 문구도 그 전제를
/// 따른다.
#[derive(Debug)]
pub(crate) enum InstallError {
    UnsupportedPlatform(String),
    /// 다른 설치가 이미 진행 중(디스크 잠금).
    Locked,
    DiskFull(String),
    Io(String),
    Network(String),
    /// 허용 범위 밖 URL 또는 리다이렉트(출처가 nodejs.org/dist/ https가 아님).
    RedirectRejected(String),
    TooLarge { limit: u64 },
    /// index.json 파싱 실패 또는 조건에 맞는 버전 없음.
    VersionResolve(String),
    /// 고정(pinned) 버전 문자열이 엄격 검증을 통과하지 못했다.
    BadVersion(String),
    /// SHASUMS256.txt에 해당 파일명 행이 없거나 모호하다.
    ChecksumMissing(String),
    HashMismatch { expected: String, actual: String },
    /// tar 항목 이름·링크·특수 파일 등이 안전 규칙을 어겼다.
    UnsafeArchive(String),
    Extract(String),
    Quarantine(String),
    /// 코드서명 검증 실패(서명 무효 또는 신뢰 체인 불일치).
    CodeSign(String),
    TeamIdMismatch { found: String },
    VersionMismatch { expected: String, actual: String },
    Cancelled,
}

impl InstallError {
    /// 취소는 실패 메시지가 아니라 별도 결과로 다뤄야 하는 호출부(프런트 문구)를 위해.
    pub(crate) fn is_cancelled(&self) -> bool {
        matches!(self, InstallError::Cancelled)
    }
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::UnsupportedPlatform(s) => write!(f, "이 컴퓨터에서는 앱 내장 Node 설치를 지원하지 않습니다: {s}"),
            InstallError::Locked => write!(f, "다른 Node 설치가 이미 진행 중입니다. 완료 후 다시 시도해주세요."),
            InstallError::DiskFull(s) => write!(f, "디스크 공간이 부족합니다. 공간을 확보한 뒤 다시 시도해주세요. ({s})"),
            InstallError::Io(s) => write!(f, "파일 작업에 실패했습니다: {s}"),
            InstallError::Network(s) => write!(f, "nodejs.org에서 받아오지 못했습니다: {s}"),
            InstallError::RedirectRejected(s) => write!(f, "허용되지 않은 주소로의 이동을 거부했습니다(https://nodejs.org/dist/ 만 허용): {s}"),
            InstallError::TooLarge { limit } => write!(f, "다운로드 크기가 상한({limit} 바이트)을 넘어 중단했습니다."),
            InstallError::VersionResolve(s) => write!(f, "설치할 Node.js LTS 버전을 정하지 못했습니다: {s}"),
            InstallError::BadVersion(s) => write!(f, "버전 문자열이 올바르지 않습니다: {s}"),
            InstallError::ChecksumMissing(s) => write!(f, "공식 체크섬(SHASUMS256.txt)에서 해당 파일을 확인하지 못했습니다: {s}"),
            InstallError::HashMismatch { expected, actual } => write!(f, "다운로드 파일의 SHA256이 공식 체크섬과 다릅니다(기대 {expected}, 실제 {actual}). 설치하지 않았습니다."),
            InstallError::UnsafeArchive(s) => write!(f, "압축 파일의 구조가 안전하지 않아 중단했습니다: {s}"),
            InstallError::Extract(s) => write!(f, "압축 해제에 실패했습니다: {s}"),
            InstallError::Quarantine(s) => write!(f, "격리 속성 제거에 실패했습니다: {s}"),
            InstallError::CodeSign(s) => write!(f, "Node 실행파일의 코드서명 검증에 실패했습니다: {s}"),
            InstallError::TeamIdMismatch { found } => write!(f, "Node 실행파일의 서명자(Team ID {found})가 OpenJS Foundation({EXPECTED_TEAM_ID})이 아닙니다. 설치하지 않았습니다."),
            InstallError::VersionMismatch { expected, actual } => write!(f, "설치한 Node의 버전({actual})이 기대한 버전({expected})과 다릅니다."),
            InstallError::Cancelled => write!(f, "설치가 취소되었습니다. 기존 설치는 그대로입니다."),
        }
    }
}

/// 쓰기 실패가 디스크 부족인지 판별한다(ENOSPC / EDQUOT / StorageFull).
pub(crate) fn is_disk_full(e: &std::io::Error) -> bool {
    if matches!(
        e.kind(),
        std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded
    ) {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(e.raw_os_error(), Some(code) if code == libc::ENOSPC || code == libc::EDQUOT)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub(crate) fn io_error(context: &str, e: std::io::Error) -> InstallError {
    if is_disk_full(&e) {
        InstallError::DiskFull(format!("{context}: {e}"))
    } else {
        InstallError::Io(format!("{context}: {e}"))
    }
}
