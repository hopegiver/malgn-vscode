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

/// 워크스페이스 루트 바로 아래 항목이 프로젝트 후보에서 왜 빠졌는지 — 사용자가
/// "폴더가 분명히 있는데 왜 안 보이지"를 판단할 수 있게 하는 값이다(2026-09
/// Windows 실사용자 보고: hub 이슈 01m2wm499xmh3046rnvx4cyn8n). 스캔 조건 자체는
/// 바꾸지 않는다 — 이 열거형은 기존 판정 결과에 이름을 붙일 뿐이다.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SkipReason {
    /// 디렉터리가 아님(파일 등).
    NotDirectory,
    /// 이름이 `.`으로 시작(숨김 폴더) — CLAUDE.md가 있어도 제외된다.
    Hidden,
    /// `CLAUDE.md`가 없음 — 가장 흔한 제외 사유(이번 이슈의 유력 원인).
    NoClaudeMd,
    /// 폴더명이 유효한 UTF-8이 아니어서 표시용 이름을 만들 수 없음(극단적 케이스).
    InvalidName,
    /// workspace 루트 자체를 `read_dir`하지 못함(권한 변경·이동식 드라이브
    /// 분리 등) — 이 루트 아래는 전부 조용히 0건으로 보였었다.
    RootUnreadable,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub(crate) struct SkippedEntry {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) reason: SkipReason,
}

/// `list_workspace_projects` 응답 전체 — 인식된 프로젝트와 제외된 항목을
/// 함께 담는다. 기존 `scan_workspace_projects()`(세션 검증 등 내부 재사용
/// 지점)는 `Vec<WorkspaceProject>` 시그니처를 그대로 유지하므로 이 타입에
/// 영향받지 않는다.
#[derive(Serialize, Default)]
pub(crate) struct WorkspaceScanResult {
    pub(crate) projects: Vec<WorkspaceProject>,
    pub(crate) skipped: Vec<SkippedEntry>,
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
/// 워크스페이스 루트 바로 아래 항목 하나가 프로젝트 후보로 인정되는지 판정하는
/// 순수 함수 — 실제 디렉터리/숨김/CLAUDE.md 유무만 본다. `workspace_roots()`
/// (전역 설정)와 무관해 단위 테스트가 임시 디렉터리로 직접 두드릴 수 있다.
/// 통과하면 폴더 이름을 `Ok`로, 제외되면 사유를 `Err(SkipReason)`으로 돌려준다
/// — **스캔 조건 자체(CLAUDE.md 요구)는 바꾸지 않는다**, 그 판정 결과를 침묵하지
/// 않고 이름 붙여 돌려줄 뿐이다.
fn classify_workspace_entry(path: &Path) -> Result<String, SkipReason> {
    if !path.is_dir() {
        return Err(SkipReason::NotDirectory);
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Err(SkipReason::InvalidName);
    };
    if name.starts_with('.') {
        return Err(SkipReason::Hidden);
    }
    if !path.join("CLAUDE.md").is_file() {
        return Err(SkipReason::NoClaudeMd);
    }
    Ok(name.to_string())
}

/// 표시용 이름 — 정상 케이스는 파일명, `InvalidName`처럼 `&str`로 못 읽는
/// 극단적 케이스는 lossy 변환한 전체 경로로 폴백한다(그래도 침묵하지 않는다).
fn skip_entry_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

/// workspace 루트 하나를 스캔해 `results`/`skipped`에 누적한다. root 자체를
/// `read_dir`하지 못하면(권한 변경·이동식 드라이브 분리 등) 그 루트 전체를
/// `RootUnreadable` 한 건으로 건너뛴다 — 예전엔 이 경로가 프로젝트 0건으로
/// 조용히 보였다(`continue`만 하고 기록이 없었다).
fn scan_root_into(workspace_root: &Path, results: &mut Vec<WorkspaceProject>, skipped: &mut Vec<SkippedEntry>) {
    let Ok(entries) = std::fs::read_dir(workspace_root) else {
        skipped.push(SkippedEntry {
            name: skip_entry_name(workspace_root),
            path: workspace_root.to_string_lossy().to_string(),
            reason: SkipReason::RootUnreadable,
        });
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match classify_workspace_entry(&path) {
            Ok(name) => name,
            Err(reason) => {
                skipped.push(SkippedEntry {
                    name: skip_entry_name(&path),
                    path: path.to_string_lossy().to_string(),
                    reason,
                });
                continue;
            }
        };

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
            name,
            path: path.to_string_lossy().to_string(),
            has_status,
            archive_status,
            sections,
            updated_at,
        });
    }
}

/// `pub(crate)`: `session_chat::validate_project_path`가 이 함수를 그대로
/// 재사용해 요청 시점에 다시 스캔한다(TOCTOU 방지 — 캐시된 프론트 상태를
/// 신뢰하지 않는다). 복붙하지 않고 이 하나의 스캔 로직만 공유한다. `lib.rs`가
/// `pub(crate) use workspace::scan_workspace_projects;`로 재노출한다.
///
/// 제외 사유(`SkippedEntry`)까지 필요한 호출부는 `scan_workspace_projects_detailed()`를
/// 쓴다 — 이 함수는 기존 호출부(session_chat/session_list) 시그니처를 바꾸지
/// 않으려고 남겨 둔 얇은 래퍼다.
pub(crate) fn scan_workspace_projects() -> Vec<WorkspaceProject> {
    scan_workspace_projects_detailed().projects
}

/// `list_workspace_projects` 전용 — 인식된 프로젝트와 함께, 후보였지만 제외된
/// 항목을 사유와 함께 반환한다.
pub(crate) fn scan_workspace_projects_detailed() -> WorkspaceScanResult {
    let mut results: Vec<WorkspaceProject> = Vec::new();
    let mut skipped: Vec<SkippedEntry> = Vec::new();
    for workspace_root in workspace_roots() {
        scan_root_into(&workspace_root, &mut results, &mut skipped);
    }
    results.sort_by(|a, b| a.name.cmp(&b.name));
    WorkspaceScanResult { projects: results, skipped }
}

/// `check_dev_tools`(dev_tools.rs)와 동일한 이유·관용구 — sync 커맨드가
/// 메인 스레드를 막는 P0 버그 계열이라 async + `spawn_blocking`으로 옮긴다.
#[tauri::command]
pub async fn list_workspace_projects() -> WorkspaceScanResult {
    tauri::async_runtime::spawn_blocking(scan_workspace_projects_detailed)
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
    // 머신 의존(~/workspace/malgn-vscode가 실제로 있어야 함) — CI 러너의 홈에는
    // 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored workspace::tests::finds_this_project_in_workspace
    #[test]
    #[ignore]
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
    // 머신 의존(~/workspace/malgn-vscode가 실제로 있어야 함) — CI 러너의 홈에는
    // 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored workspace::tests::blocks_path_traversal_in_file_preview
    #[test]
    #[ignore]
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

    // 머신 의존(~/workspace/malgn-vscode가 실제로 있어야 함) — CI 러너의 홈에는
    // 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored workspace::tests::reads_a_real_text_file_from_this_project
    #[test]
    #[ignore]
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

    // 머신 의존(~/workspace/malgn-vscode가 실제로 있어야 함) — CI 러너의 홈에는
    // 없어 #[ignore]. 로컬 실행:
    // cargo test -- --ignored workspace::tests::builds_tree_excluding_node_modules
    #[test]
    #[ignore]
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

    // ---------------- 제외 사유 분류(SkipReason) ----------------
    // 실제 ~/workspace/전역 설정과 무관한 임시 디렉터리만 써서, 머신 상태와
    // 상관없이 항상 실행된다(#[ignore] 없음).

    fn temp_subdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "malgn-vscode-workspace-scan-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("임시 디렉터리를 만들지 못했습니다");
        dir
    }

    // (a) CLAUDE.md가 있는 폴더 — 프로젝트 후보로 인정되어야 한다(Ok).
    #[test]
    fn classify_workspace_entry_accepts_a_folder_with_claude_md() {
        let dir = temp_subdir("has-claude-md");
        std::fs::write(dir.join("CLAUDE.md"), "# hi").unwrap();
        let expected_name = dir.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(classify_workspace_entry(&dir), Ok(expected_name));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (b) CLAUDE.md가 없는 폴더 — NoClaudeMd로 제외되어야 한다(이번 이슈의
    // 유력 원인 — 예전엔 이 사유가 프런트까지 전혀 올라가지 않았다).
    #[test]
    fn classify_workspace_entry_rejects_a_folder_without_claude_md() {
        let dir = temp_subdir("no-claude-md");
        assert_eq!(classify_workspace_entry(&dir), Err(SkipReason::NoClaudeMd));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // (c) 숨김 폴더 — CLAUDE.md가 있어도 이름이 `.`으로 시작하면 Hidden으로
    // 제외되어야 한다(스캔 조건 우선순위: 숨김 판정이 CLAUDE.md 유무보다 먼저).
    #[test]
    fn classify_workspace_entry_rejects_a_hidden_folder_even_with_claude_md() {
        let parent = temp_subdir("hidden-parent");
        let dir = parent.join(".hidden-project");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CLAUDE.md"), "# hi").unwrap();
        assert_eq!(classify_workspace_entry(&dir), Err(SkipReason::Hidden));
        let _ = std::fs::remove_dir_all(&parent);
    }

    // (d) 파일(디렉터리가 아님) — NotDirectory로 제외되어야 한다.
    #[test]
    fn classify_workspace_entry_rejects_a_plain_file() {
        let parent = temp_subdir("file-parent");
        let file = parent.join("not-a-folder.txt");
        std::fs::write(&file, "hi").unwrap();
        assert_eq!(classify_workspace_entry(&file), Err(SkipReason::NotDirectory));
        let _ = std::fs::remove_dir_all(&parent);
    }

    // 네 종류(a~d)를 한 루트 아래 함께 두고 scan_root_into()가 인식 결과와
    // 제외 사유를 동시에 정확히 쌓는지 못박는다 — classify_workspace_entry
    // 단위 테스트와 달리 이건 결과 누적(results/skipped)까지 확인한다.
    #[test]
    fn scan_root_into_collects_matches_and_skip_reasons_together() {
        let root = temp_subdir("mixed-root");

        let good = root.join("good-project");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(good.join("CLAUDE.md"), "# hi").unwrap();

        std::fs::create_dir_all(root.join("no-claude-md")).unwrap();

        let hidden = root.join(".hidden");
        std::fs::create_dir_all(&hidden).unwrap();
        std::fs::write(hidden.join("CLAUDE.md"), "# hi").unwrap();

        std::fs::write(root.join("just-a-file.txt"), "hi").unwrap();

        let mut results: Vec<WorkspaceProject> = Vec::new();
        let mut skipped: Vec<SkippedEntry> = Vec::new();
        scan_root_into(&root, &mut results, &mut skipped);

        assert_eq!(results.len(), 1, "CLAUDE.md가 있는 폴더 1개만 인식돼야 합니다");
        assert_eq!(results[0].name, "good-project");

        assert_eq!(skipped.len(), 3, "나머지 3개는 사유와 함께 제외돼야 합니다");
        assert!(skipped
            .iter()
            .any(|s| s.name == "no-claude-md" && s.reason == SkipReason::NoClaudeMd));
        assert!(skipped
            .iter()
            .any(|s| s.name == ".hidden" && s.reason == SkipReason::Hidden));
        assert!(skipped
            .iter()
            .any(|s| s.name == "just-a-file.txt" && s.reason == SkipReason::NotDirectory));

        let _ = std::fs::remove_dir_all(&root);
    }

    // read_dir 자체가 실패하는 루트(권한 없음)는 안쪽 항목별이 아니라 루트
    // 통째로 RootUnreadable 한 건으로 보고돼야 한다 — 예전엔 이 경로가 아무
    // 기록도 없이 프로젝트 0건으로만 보였다. unix 전용(권한 비트 의존,
    // dev_tools/runners.rs의 기존 chmod 테스트 패턴을 따른다).
    #[cfg(unix)]
    #[test]
    fn scan_root_into_reports_root_unreadable_when_read_dir_fails() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_subdir("unreadable-root");
        let original_perms = std::fs::metadata(&root).unwrap().permissions();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o000))
            .expect("chmod 000 실패");

        let mut results: Vec<WorkspaceProject> = Vec::new();
        let mut skipped: Vec<SkippedEntry> = Vec::new();
        scan_root_into(&root, &mut results, &mut skipped);

        // remove_dir_all이 가능하려면 먼저 권한을 복구해야 한다.
        std::fs::set_permissions(&root, original_perms).expect("chmod 복구 실패");

        assert!(results.is_empty());
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].reason, SkipReason::RootUnreadable);

        let _ = std::fs::remove_dir_all(&root);
    }
}
