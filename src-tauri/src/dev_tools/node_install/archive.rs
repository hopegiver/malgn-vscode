// ==================== tar 안전성 검사(해제 전 목록 · 해제 후 트리) ====================
// 해시가 맞더라도(= nodejs.org가 준 바이트와 같더라도) 해제 단계는 별도 방어선을 둔다:
//   1) 해제 전 `tar -tf` 목록의 모든 이름을 검사한다(절대경로·`..`·최상위 디렉터리 이탈 금지).
//   2) 해제는 비어 있는 전용 디렉터리에만 하고, 해제 후 트리를 `symlink_metadata`(링크를
//      따라가지 않음)로 훑어 심볼릭 링크 탈출·하드링크·특수 파일·setuid를 검사한다.
// 둘 다 파일시스템을 읽기만 한다 — 해제물을 실행하지 않는다.

use super::types::InstallError;
use std::path::{Component, Path, PathBuf};

/// 해제 후 트리 전체 크기 상한. 공식 darwin tarball은 ~230MB로 풀린다 — 4배 넘게 여유를
/// 두되 디스크를 가득 채우는 비정상 압축은 막는다.
pub(crate) const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const MAX_DEPTH: usize = 64;

fn unsafe_archive(msg: impl Into<String>) -> InstallError {
    InstallError::UnsafeArchive(msg.into())
}

/// `tar -tf` 출력(줄당 항목 하나)을 검사한다. 모든 항목은 `<top>` 또는 `<top>/…`여야 하고,
/// 절대경로·`..`·`.` 성분·빈 성분(`//`)·빈 줄이 있으면 거부한다. 이름에 개행이 들어 있어
/// 목록이 어긋나는 경우에도 어긋난 줄이 위 규칙(최상위 접두)을 어기므로 거부된다.
pub(crate) fn validate_listing(listing: &str, top: &str) -> Result<usize, InstallError> {
    let mut count = 0usize;
    for line in listing.lines() {
        count += 1;
        if count > MAX_ENTRIES {
            return Err(unsafe_archive("항목 수가 비정상적으로 많습니다"));
        }
        if line.is_empty() {
            return Err(unsafe_archive("빈 항목 이름이 있습니다"));
        }
        if line.starts_with('/') {
            return Err(unsafe_archive(format!("절대경로 항목: {line}")));
        }
        let trimmed = line.strip_suffix('/').unwrap_or(line);
        let mut components = trimmed.split('/');
        let first = components.next().unwrap_or("");
        if first != top {
            return Err(unsafe_archive(format!("최상위 디렉터리({top}) 밖의 항목: {line}")));
        }
        for c in components {
            if c.is_empty() || c == "." || c == ".." {
                return Err(unsafe_archive(format!("허용되지 않는 경로 성분({c:?}): {line}")));
            }
        }
    }
    if count == 0 {
        return Err(unsafe_archive("압축 파일이 비어 있습니다"));
    }
    Ok(count)
}

/// 해제된 트리 요약.
#[derive(Debug, Default)]
pub(crate) struct TreeSummary {
    pub(crate) files: usize,
    pub(crate) dirs: usize,
    pub(crate) symlinks: usize,
    pub(crate) total_bytes: u64,
}

/// `extract_root` 안에는 정확히 `top` 디렉터리 하나만 있어야 하고, 그 아래 모든 항목이
/// 일반 파일·디렉터리·루트 안으로만 향하는 심볼릭 링크여야 한다.
pub(crate) fn validate_tree(extract_root: &Path, top: &str) -> Result<TreeSummary, InstallError> {
    let mut top_entries = std::fs::read_dir(extract_root)
        .map_err(|e| InstallError::Io(format!("해제 결과를 읽지 못했습니다: {e}")))?;
    let first = top_entries
        .next()
        .ok_or_else(|| unsafe_archive("해제 결과가 비어 있습니다"))?
        .map_err(|e| InstallError::Io(e.to_string()))?;
    if top_entries.next().is_some() || first.file_name() != top {
        return Err(unsafe_archive(format!("해제 결과가 최상위 디렉터리 {top} 하나가 아닙니다")));
    }
    let top_path = first.path();
    let meta = std::fs::symlink_metadata(&top_path).map_err(|e| InstallError::Io(e.to_string()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(unsafe_archive("최상위 항목이 일반 디렉터리가 아닙니다"));
    }
    let canonical_top = std::fs::canonicalize(&top_path)
        .map_err(|e| InstallError::Io(format!("경로 확인 실패: {e}")))?;

    let mut summary = TreeSummary::default();
    walk(&top_path, &top_path, &canonical_top, 0, &mut summary)?;
    Ok(summary)
}

fn walk(
    dir: &Path,
    top_path: &Path,
    canonical_top: &Path,
    depth: usize,
    summary: &mut TreeSummary,
) -> Result<(), InstallError> {
    if depth > MAX_DEPTH {
        return Err(unsafe_archive("디렉터리 깊이가 비정상적으로 깊습니다"));
    }
    let read = std::fs::read_dir(dir).map_err(|e| InstallError::Io(format!("{}: {e}", dir.display())))?;
    for entry in read {
        let entry = entry.map_err(|e| InstallError::Io(e.to_string()))?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path).map_err(|e| InstallError::Io(e.to_string()))?;
        let ft = meta.file_type();
        if summary.files + summary.dirs + summary.symlinks > MAX_ENTRIES {
            return Err(unsafe_archive("항목 수가 비정상적으로 많습니다"));
        }
        if ft.is_symlink() {
            summary.symlinks += 1;
            check_symlink(&path, top_path, canonical_top)?;
        } else if ft.is_dir() {
            summary.dirs += 1;
            walk(&path, top_path, canonical_top, depth + 1, summary)?;
        } else if ft.is_file() {
            summary.files += 1;
            summary.total_bytes += meta.len();
            if summary.total_bytes > MAX_EXTRACTED_BYTES {
                return Err(unsafe_archive("해제 크기가 상한을 넘었습니다"));
            }
            check_file_mode(&path, &meta)?;
        } else {
            return Err(unsafe_archive(format!(
                "일반 파일·디렉터리·링크가 아닌 항목(장치/소켓/FIFO): {}",
                path.display()
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn check_file_mode(path: &Path, meta: &std::fs::Metadata) -> Result<(), InstallError> {
    use std::os::unix::fs::MetadataExt;
    if meta.nlink() > 1 {
        return Err(unsafe_archive(format!("하드링크 항목: {}", path.display())));
    }
    if meta.mode() & 0o6000 != 0 {
        return Err(unsafe_archive(format!("setuid/setgid 비트가 있는 파일: {}", path.display())));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_file_mode(_path: &Path, _meta: &std::fs::Metadata) -> Result<(), InstallError> {
    Ok(())
}

/// 링크 대상은 상대경로여야 하고, 어휘적(lexical)으로 `top` 밖으로 나가면 안 된다. 실제로
/// 해석되는 경우에는 `canonicalize` 결과도 `top` 안이어야 한다(중간 링크를 거친 탈출 방어).
/// 아직 가리키는 파일이 없는 링크(dangling)는 어휘 검사만 통과하면 허용한다 — 필요한
/// 링크(bin/npm 등)의 실재 여부는 설치 단계의 필수 파일 검사가 따로 본다.
fn check_symlink(link: &Path, top_path: &Path, canonical_top: &Path) -> Result<(), InstallError> {
    let target = std::fs::read_link(link).map_err(|e| InstallError::Io(e.to_string()))?;
    if target.is_absolute() {
        return Err(unsafe_archive(format!(
            "절대경로를 가리키는 심볼릭 링크: {} -> {}",
            link.display(),
            target.display()
        )));
    }
    let parent = link.parent().unwrap_or(top_path);
    let rel_parent = parent
        .strip_prefix(top_path)
        .map_err(|_| unsafe_archive("링크 위치가 최상위 밖입니다"))?;
    let mut stack: Vec<PathBuf> = rel_parent
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(PathBuf::from(n)),
            _ => None,
        })
        .collect();
    for comp in target.components() {
        match comp {
            Component::Normal(n) => stack.push(PathBuf::from(n)),
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_none() {
                    return Err(unsafe_archive(format!(
                        "최상위 밖으로 나가는 심볼릭 링크: {} -> {}",
                        link.display(),
                        target.display()
                    )));
                }
            }
            _ => {
                return Err(unsafe_archive(format!(
                    "해석할 수 없는 링크 대상: {} -> {}",
                    link.display(),
                    target.display()
                )))
            }
        }
    }
    if let Ok(resolved) = std::fs::canonicalize(link) {
        if !resolved.starts_with(canonical_top) {
            return Err(unsafe_archive(format!(
                "해석 결과가 최상위 밖인 심볼릭 링크: {} -> {}",
                link.display(),
                resolved.display()
            )));
        }
    }
    Ok(())
}
