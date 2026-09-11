// ---------------- 프로젝트 파일 트리 + 미리보기 (읽기 전용) ----------------
// electron 브랜치 src/host/window/pathGuard.ts의 "워크스페이스 루트 바로 아래
// 1단계 자식만 허용" 검증을 포팅한다 — project_path가 ~/workspace 바깥이나 다단계
// 하위 경로를 가리키지 못하게 막는다. relative_path는 canonicalize() 기반
// starts_with 검사로 추가 방어한다(심볼릭 링크·`..`까지 실제로 해석해서 막는다 —
// 문자열 비교보다 강하다).
//
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
// `#[tauri::command]` 진입점(`list_project_tree`/`read_project_file`)은 매크로
// 제약상 `mod.rs`에 남아 있고, 이 파일은 그 명령이 위임하는 순수 로직만 담는다.

use serde::Serialize;
use std::path::{Path, PathBuf};

fn is_direct_child_of_workspace_root(workspace_root: &Path, candidate: &Path) -> bool {
    let Ok(resolved_root) = workspace_root.canonicalize() else {
        return false;
    };
    let Ok(resolved_candidate) = candidate.canonicalize() else {
        return false;
    };
    let Ok(rel) = resolved_candidate.strip_prefix(&resolved_root) else {
        return false;
    };
    !rel.as_os_str().is_empty() && rel.components().count() == 1
}

/// `session_chat::validate_project_path`(다른 모듈)와 `autonomy`의 커맨드들이
/// `crate::resolve_validated_project_root`로 이 함수를 그대로 재사용한다 — `lib.rs`가
/// `pub(crate) use workspace::resolve_validated_project_root;`로 재노출한다.
pub(crate) fn resolve_validated_project_root(project_path: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(project_path);
    let is_child_of_any_root = super::workspace_roots()
        .iter()
        .any(|root| is_direct_child_of_workspace_root(root, &candidate));
    if !is_child_of_any_root {
        return None;
    }
    candidate.canonicalize().ok()
}

#[derive(Serialize, Debug)]
pub(crate) struct TreeNode {
    // 필드 전부 `pub(crate)`: `mod.rs`의 회귀 테스트(`builds_tree_excluding_node_modules`
    // 등)가 이 구조체를 직접 만들지 않고 `list_project_tree()` 결과의 `name`/`truncated`
    // 필드를 읽어 검증한다 — 부모 모듈에서의 필드 접근을 허용해야 한다.
    pub(crate) name: String,
    #[serde(rename = "relativePath")]
    relative_path: String,
    #[serde(rename = "isDirectory")]
    is_directory: bool,
    children: Option<Vec<TreeNode>>,
    pub(crate) truncated: bool,
}

const EXCLUDED_DIR_NAMES: [&str; 9] = [
    "node_modules",
    ".git",
    "dist",
    "target",
    ".next",
    "build",
    ".venv",
    "__pycache__",
    ".cache",
];
const MAX_TREE_DEPTH: usize = 4;

fn build_tree(dir: &Path, rel_prefix: &str, depth: usize) -> Vec<TreeNode> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut nodes: Vec<TreeNode> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') && name != ".gitignore" {
            continue;
        }
        let relative_path = if rel_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let is_dir = path.is_dir();

        if is_dir {
            if EXCLUDED_DIR_NAMES.contains(&name) {
                nodes.push(TreeNode {
                    name: name.to_string(),
                    relative_path,
                    is_directory: true,
                    children: Some(Vec::new()),
                    truncated: true,
                });
                continue;
            }
            if depth >= MAX_TREE_DEPTH {
                nodes.push(TreeNode {
                    name: name.to_string(),
                    relative_path,
                    is_directory: true,
                    children: None,
                    truncated: true,
                });
                continue;
            }
            let children = build_tree(&path, &relative_path, depth + 1);
            nodes.push(TreeNode {
                name: name.to_string(),
                relative_path,
                is_directory: true,
                children: Some(children),
                truncated: false,
            });
        } else {
            nodes.push(TreeNode {
                name: name.to_string(),
                relative_path,
                is_directory: false,
                children: None,
                truncated: false,
            });
        }
    }
    nodes.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    nodes
}

/// `#[tauri::command] list_project_tree`(mod.rs)가 위임하는 순수 로직.
pub(crate) fn list_tree(project_path: &str) -> Vec<TreeNode> {
    let Some(root) = resolve_validated_project_root(project_path) else {
        return Vec::new();
    };
    build_tree(&root, "", 0)
}

#[derive(Serialize, Debug)]
#[serde(tag = "kind")]
pub(crate) enum FilePreview {
    #[serde(rename = "text")]
    Text { content: String },
    #[serde(rename = "tooLarge")]
    TooLarge { size: u64 },
    #[serde(rename = "binary")]
    Binary,
    #[serde(rename = "notFound")]
    NotFound,
    #[serde(rename = "denied")]
    Denied,
}

const MAX_PREVIEW_BYTES: u64 = 512 * 1024;
const BINARY_EXTENSIONS: [&str; 20] = [
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "pdf", "zip", "gz", "tar", "woff",
    "woff2", "ttf", "eot", "exe", "dll", "so", "dylib",
];

/// `#[tauri::command] read_project_file`(mod.rs)이 위임하는 순수 로직.
pub(crate) fn read_file_preview(project_path: &str, relative_path: &str) -> FilePreview {
    let Some(root) = resolve_validated_project_root(project_path) else {
        return FilePreview::Denied;
    };

    let candidate = root.join(relative_path);
    let Ok(canon_candidate) = candidate.canonicalize() else {
        return FilePreview::NotFound;
    };
    if !canon_candidate.starts_with(&root) {
        return FilePreview::Denied;
    }
    if !canon_candidate.is_file() {
        return FilePreview::NotFound;
    }

    let Ok(metadata) = std::fs::metadata(&canon_candidate) else {
        return FilePreview::NotFound;
    };
    if metadata.len() > MAX_PREVIEW_BYTES {
        return FilePreview::TooLarge {
            size: metadata.len(),
        };
    }

    let looks_binary_ext = canon_candidate
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| BINARY_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false);
    if looks_binary_ext {
        return FilePreview::Binary;
    }

    let Ok(bytes) = std::fs::read(&canon_candidate) else {
        return FilePreview::NotFound;
    };
    let sample_len = bytes.len().min(8000);
    if bytes[..sample_len].contains(&0) {
        return FilePreview::Binary;
    }

    match String::from_utf8(bytes) {
        Ok(content) => FilePreview::Text { content },
        Err(_) => FilePreview::Binary,
    }
}
