// ---------------- 워크스페이스 프로젝트 스캔 + 파일트리/미리보기 ----------------
// electron 브랜치의 src/host/window/{projectScanner,statusParser}.ts 알고리즘을
// 그대로 포팅한다(판별 기준·이모지 섹션 파싱·보관 키워드까지 동일). IPC 대신 Tauri
// 커맨드로 옮긴 것만 다르다. 렌더러가 임의 경로를 넘기는 두 번째 커맨드(원본의
// getProjectDetail(path))는 두지 않는다 — 이 커맨드가 STATUS.md 내용까지 한 번에
// 전부 반환하므로 프론트엔드는 이미 받은 목록에서 path로 찾기만 하면 되고, 그래서
// 원본의 pathGuard.ts(렌더러발 경로 검증)에 해당하는 별도 가드가 애초에 필요 없다
// — 사용자 입력을 받는 fs 커맨드 자체가 없다.
//
// `lib.rs`에 있던 코드를 그대로 옮긴 것이다 — 로직은 한 글자도 바꾸지 않았다.
// `autonomy`/`dev_tools`와 동일한 패턴: `#[tauri::command]` 진입점만 이 파일에
// 두고, 무거운 로직은 서브모듈(`status`/`tree`)로 옮겼다.

mod status;
mod tree;

use serde::Serialize;
use status::{classify_archive_status, parse_status_markdown, ProjectStatusSections};
use std::path::{Path, PathBuf};

// `pub(crate)`: `session_chat::validate_project_path`가 `crate::scan_workspace_projects()`
// 결과 타입을 직접 다룬다(TOCTOU 방지 재검증) — 다른 모듈에서의 사용을 허용해야 한다.
#[derive(Serialize)]
pub(crate) struct WorkspaceProject {
    // `pub(crate)`: `session_chat`의 테스트(`validate_project_path_accepts_a_real_scanned_project_path`)와
    // 커맨드(`validate_project_path`)가 `crate::scan_workspace_projects()` 결과의
    // `name`/`path` 필드를 직접 읽는다(TOCTOU 방지 재검증) — 다른 모듈에서의
    // 필드 접근을 허용해야 한다.
    pub(crate) name: String,
    pub(crate) path: String,
    #[serde(rename = "hasStatus")]
    has_status: bool,
    #[serde(rename = "archiveStatus")]
    archive_status: String,
    sections: Option<ProjectStatusSections>,
    #[serde(rename = "updatedAt")]
    updated_at: i64,
}

/// 후보 workspace 루트 목록 — 전역 설정 파일(`~/.claude/malgn-agent.json`)의
/// `workspaces` 항목을 정본으로 위임한다(설계
/// `docs/design/autonomy-runtime-and-config.md` §5). 시그니처는 그대로
/// 유지하되(호출부 4곳이 바뀌지 않도록) 손상된 설정은 **빈 목록**으로 흡수한다
/// (fail-closed — `~/workspace` 하드코딩으로 되돌아가지 않는다. 빈 목록이면
/// 스캔 결과가 0건이고 스케줄러도 아무것도 실행하지 않는다). 손상 상태를
/// 사용자에게 보이는 오류로 드러내는 것은 `config::malgn_agent_config_get()`·
/// `autonomy::autonomy_list()`·스케줄러 tick 3곳의 몫이다.
pub(crate) fn workspace_roots() -> Vec<PathBuf> {
    crate::config::workspace_roots_checked().unwrap_or_default()
}

/// 주어진 경로의 수정시각(mtime)을 UNIX epoch 밀리초로 반환한다. 메타데이터
/// 조회 실패, `modified()` 미지원, 또는 `SystemTime`이 UNIX_EPOCH보다 이전인
/// 클럭 이상 등 어떤 이유로든 값을 구할 수 없으면 패닉 대신 `None`을 반환한다
/// (호출자가 조용히 폴백한다).
fn mtime_millis(path: &Path) -> Option<i64> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

/// 각 workspace 루트 바로 아래 1단계 디렉터리만 스캔한다. `CLAUDE.md`가 있으면
/// malgn-agent 프로젝트로 인식한다(STATUS.md 유무는 판별 기준이 아니다 — 없으면
/// `archiveStatus:"unknown"` + `hasStatus:false`로 접는다, 추측 분류 금지).
///
/// `pub(crate)`: `session_chat::validate_project_path`가 이 함수를 그대로
/// 재사용해 요청 시점에 다시 스캔한다(TOCTOU 방지 — 캐시된 프론트 상태를
/// 신뢰하지 않는다). 복붙하지 않고 이 하나의 스캔 로직만 공유한다. `lib.rs`가
/// `pub(crate) use workspace::scan_workspace_projects;`로 재노출한다.
pub(crate) fn scan_workspace_projects() -> Vec<WorkspaceProject> {
    let mut results: Vec<WorkspaceProject> = Vec::new();
    for workspace_root in workspace_roots() {
        let Ok(entries) = std::fs::read_dir(&workspace_root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }

            if !path.join("CLAUDE.md").is_file() {
                continue;
            }

            let status_path = path.join("STATUS.md");
            let has_status = status_path.is_file();

            let (archive_status, sections) = if has_status {
                match std::fs::read_to_string(&status_path) {
                    Ok(content) => {
                        let parsed_sections = parse_status_markdown(&content);
                        let status =
                            classify_archive_status(true, &parsed_sections.current).to_string();
                        (status, Some(parsed_sections))
                    }
                    Err(_) => ("unknown".to_string(), None),
                }
            } else {
                ("unknown".to_string(), None)
            };

            let claude_md_mtime = mtime_millis(&path.join("CLAUDE.md"));
            let status_mtime = if has_status {
                mtime_millis(&status_path)
            } else {
                None
            };
            let updated_at = claude_md_mtime
                .into_iter()
                .chain(status_mtime)
                .max()
                .unwrap_or(0);

            results.push(WorkspaceProject {
                name: name.to_string(),
                path: path.to_string_lossy().to_string(),
                has_status,
                archive_status,
                sections,
                updated_at,
            });
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

/// `check_dev_tools`(dev_tools.rs)와 동일한 이유·관용구 — sync 커맨드가
/// 메인 스레드를 막는 P0 버그 계열이라 async + `spawn_blocking`으로 옮긴다.
#[tauri::command]
pub async fn list_workspace_projects() -> Vec<WorkspaceProject> {
    tauri::async_runtime::spawn_blocking(scan_workspace_projects)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_project_tree(project_path: String) -> Vec<tree::TreeNode> {
    tree::list_tree(&project_path)
}

#[tauri::command]
pub fn read_project_file(project_path: String, relative_path: String) -> tree::FilePreview {
    tree::read_file_preview(&project_path, &relative_path)
}

// `autonomy`/`session_chat`이 `crate::resolve_validated_project_root`로 재사용하는
// 경로를 유지한다 — 함수 정의는 `tree.rs`로 옮겼지만 이 재노출로 두 모듈의 호출부는
// 손대지 않아도 된다(`lib.rs`가 `pub(crate) use workspace::resolve_validated_project_root;`로
// 한 단계 더 재노출한다).
pub(crate) use tree::resolve_validated_project_root;

#[cfg(test)]
mod tests {
    use super::*;

    // 이 머신의 실제 ~/workspace를 스캔해 이 프로젝트(malgn-vscode) 자신이 목록에
    // 있는지 확인한다 — CLAUDE.md가 있으니 반드시 인식되어야 한다.
    #[test]
    fn finds_this_project_in_workspace() {
        let projects = scan_workspace_projects();
        assert!(
            !projects.is_empty(),
            "~/workspace 아래에서 프로젝트를 하나도 찾지 못했습니다"
        );
        assert!(
            projects.iter().any(|p| p.name == "malgn-vscode"),
            "malgn-vscode 자기 자신이 목록에 없습니다"
        );
    }

    #[test]
    fn rejects_project_path_outside_workspace_root() {
        let tree = list_project_tree("/etc".to_string());
        assert!(
            tree.is_empty(),
            "워크스페이스 바깥 경로가 거부되지 않았습니다"
        );
    }

    // 경로 트래버설 차단 — 보안 관련이라 회귀 방지용으로 고정해둔다.
    #[test]
    fn blocks_path_traversal_in_file_preview() {
        let projects = scan_workspace_projects();
        let project = projects
            .iter()
            .find(|p| p.name == "malgn-vscode")
            .expect("malgn-vscode 프로젝트가 없습니다");
        let result = read_project_file(
            project.path.clone(),
            "../../../../../../../etc/passwd".to_string(),
        );
        assert!(
            matches!(
                result,
                tree::FilePreview::Denied | tree::FilePreview::NotFound
            ),
            "경로 트래버설이 차단되지 않았습니다: {:?}",
            result
        );
    }

    #[test]
    fn reads_a_real_text_file_from_this_project() {
        let projects = scan_workspace_projects();
        let project = projects
            .iter()
            .find(|p| p.name == "malgn-vscode")
            .expect("malgn-vscode 프로젝트가 없습니다");
        let result = read_project_file(project.path.clone(), "package.json".to_string());
        match result {
            tree::FilePreview::Text { content } => assert!(content.contains("\"name\"")),
            other => panic!("package.json을 텍스트로 읽지 못했습니다: {:?}", other),
        }
    }

    #[test]
    fn builds_tree_excluding_node_modules() {
        let projects = scan_workspace_projects();
        let project = projects
            .iter()
            .find(|p| p.name == "malgn-vscode")
            .expect("malgn-vscode 프로젝트가 없습니다");
        let tree = list_project_tree(project.path.clone());
        assert!(!tree.is_empty(), "파일 트리가 비어 있습니다");
        if let Some(nm) = tree.iter().find(|n| n.name == "node_modules") {
            assert!(nm.truncated, "node_modules가 제외 표시되지 않았습니다");
        }
    }
}
