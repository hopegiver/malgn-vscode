// ==================== 릴리스 결정: index.json · SHASUMS256.txt ====================
// 순수함수만 둔다(네트워크·파일 접근 없음). 버전 결정 규칙은 hub 결정
// 01m4zfesw2qvhnth3jjg89w04k를 그대로 코드로 옮긴 것이다:
//   - lts != false (문자열 코드명이 있는 항목만)
//   - files에 osx-<arch>-tar 포함
//   - 메이저 하한 MIN_MAJOR
//   - 버전 문자열은 ^v\d+\.\d+\.\d+$ 엄격 검증(통과 못 한 항목은 후보에서 제외)
//   - 후보 중 숫자상 최고 버전. 하드코딩된 버전·메이저 고정은 없다.

use super::types::{InstallError, NodeArch, NodeVersion, DIST_BASE, MIN_MAJOR};

pub(crate) fn index_url() -> String {
    format!("{DIST_BASE}/index.json")
}

pub(crate) fn shasums_url(v: NodeVersion) -> String {
    format!("{DIST_BASE}/{}/SHASUMS256.txt", v.to_tag())
}

pub(crate) fn tarball_name(v: NodeVersion, arch: NodeArch) -> String {
    format!("node-{}-darwin-{}.tar.gz", v.to_tag(), arch.dist_token())
}

pub(crate) fn tarball_url(v: NodeVersion, arch: NodeArch) -> String {
    format!("{DIST_BASE}/{}/{}", v.to_tag(), tarball_name(v, arch))
}

/// tar 안의 최상위 디렉터리 이름(`node-v24.22.0-darwin-arm64`).
pub(crate) fn tarball_top_dir(v: NodeVersion, arch: NodeArch) -> String {
    format!("node-{}-darwin-{}", v.to_tag(), arch.dist_token())
}

/// index.json에서 설치할 버전을 고른다. 항목 하나가 깨져 있어도(버전 문자열 형식
/// 위반 등) 그 항목만 후보에서 빠지고 나머지로 계속한다 — 선택된 값은 항상
/// 엄격 검증을 통과한 `NodeVersion`이다.
pub(crate) fn select_latest_lts(index_json: &[u8], arch: NodeArch) -> Result<NodeVersion, InstallError> {
    let parsed: serde_json::Value = serde_json::from_slice(index_json)
        .map_err(|e| InstallError::VersionResolve(format!("index.json을 해석하지 못했습니다: {e}")))?;
    let entries = parsed
        .as_array()
        .ok_or_else(|| InstallError::VersionResolve("index.json이 배열이 아닙니다".to_string()))?;
    let files_key = arch.index_files_key();

    let mut best: Option<NodeVersion> = None;
    for entry in entries {
        let Some(obj) = entry.as_object() else { continue };
        let Some(version) = obj
            .get("version")
            .and_then(|v| v.as_str())
            .and_then(NodeVersion::parse_tag)
        else {
            continue;
        };
        // lts는 LTS 라인이면 코드명 문자열, 아니면 false다. 문자열이 아닌 값(true·null·
        // 누락)은 전부 비-LTS로 본다.
        let is_lts = obj
            .get("lts")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        if !is_lts || version.major < MIN_MAJOR {
            continue;
        }
        let has_tar = obj
            .get("files")
            .and_then(|v| v.as_array())
            .map(|files| files.iter().any(|f| f.as_str() == Some(files_key.as_str())))
            .unwrap_or(false);
        if !has_tar {
            continue;
        }
        if best.map(|b| version > b).unwrap_or(true) {
            best = Some(version);
        }
    }
    best.ok_or_else(|| {
        InstallError::VersionResolve(format!(
            "조건(LTS, {files_key} 제공, 메이저 {MIN_MAJOR} 이상)에 맞는 버전이 index.json에 없습니다"
        ))
    })
}

/// SHASUMS256.txt에서 `file_name`과 정확히 일치하는 행의 SHA256(소문자 hex 64자)을
/// 돌려준다. 행 형식은 `<hex64>  <파일명>`(공백 1개 이상, 파일명 앞 `*`는 이진 모드
/// 표시라 허용). 부분 일치·접두 일치는 인정하지 않으며, 같은 파일명에 서로 다른
/// 해시가 둘 이상이면 모호하므로 거부한다.
pub(crate) fn expected_sha256(shasums: &str, file_name: &str) -> Result<String, InstallError> {
    let mut found: Option<String> = None;
    for line in shasums.lines() {
        let mut tokens = line.split_whitespace();
        let (Some(hash), Some(name), None) = (tokens.next(), tokens.next(), tokens.next()) else {
            continue;
        };
        let name = name.strip_prefix('*').unwrap_or(name);
        if name != file_name {
            continue;
        }
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(InstallError::ChecksumMissing(format!(
                "{file_name} 행의 해시 형식이 올바르지 않습니다"
            )));
        }
        let hash = hash.to_ascii_lowercase();
        match &found {
            Some(existing) if *existing != hash => {
                return Err(InstallError::ChecksumMissing(format!(
                    "{file_name}에 서로 다른 해시가 둘 이상 있습니다"
                )));
            }
            _ => found = Some(hash),
        }
    }
    found.ok_or_else(|| InstallError::ChecksumMissing(format!("{file_name} 행이 없습니다")))
}
