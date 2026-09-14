// 앱링크 커맨드 3개 + 상태 구조체. 설계 정본: `docs/design-app-links.md` §3.
// 스키마·순수 검증·파일 IO는 `store.rs`, 초기 시드 값은 `seed.rs`에 있다.

mod seed;
mod store;

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use store::{AppLink, AppLinksFile};

/// save/open의 파일 접근 구간을 직렬화한다(`autonomy::mod`의
/// `AUTONOMY_FILE_LOCK` 선례와 동일 패턴). 백그라운드 스레드는 이 파일을
/// 건드리지 않지만, 설정 화면에서 토글을 연타하면 save가 겹칠 수 있다.
static APP_LINKS_FILE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Clone, Debug)]
pub struct AppLinksLimits {
    #[serde(rename = "maxLinks")]
    pub max_links: usize,
    #[serde(rename = "maxNameLength")]
    pub max_name_length: usize,
    #[serde(rename = "maxUrlLength")]
    pub max_url_length: usize,
    #[serde(rename = "allowedSchemes")]
    pub allowed_schemes: Vec<String>,
}

fn limits() -> AppLinksLimits {
    AppLinksLimits {
        max_links: store::MAX_LINKS,
        max_name_length: store::MAX_NAME_LENGTH,
        max_url_length: store::MAX_URL_LENGTH,
        allowed_schemes: store::ALLOWED_SCHEMES.iter().map(|s| s.to_string()).collect(),
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct AppLinksStatus {
    pub ok: bool,
    pub error: Option<String>,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "fileExists")]
    pub file_exists: bool,
    pub links: Vec<AppLink>,
    pub warnings: Vec<String>,
    pub limits: AppLinksLimits,
}

/// 손상된 파일이어도 `ok:false`를 담은 정상 응답으로 내려간다
/// (`malgn_agent_config_get()`과 동일한 "상태 조회는 Err를 던지지 않는다" 규약).
fn build_status(path: Option<&Path>) -> AppLinksStatus {
    let Some(path) = path else {
        return AppLinksStatus {
            ok: false,
            error: Some("홈 디렉터리를 확인할 수 없습니다.".to_string()),
            file_path: String::new(),
            file_exists: false,
            links: Vec::new(),
            warnings: Vec::new(),
            limits: limits(),
        };
    };

    let file_path = path.to_string_lossy().to_string();
    let file_exists = path.is_file();

    match store::load_from_path(path) {
        Ok(file) => {
            let (valid, warnings) = store::sanitize_loaded_links(file.links);
            AppLinksStatus {
                ok: true,
                error: None,
                file_path,
                file_exists,
                links: valid,
                warnings,
                limits: limits(),
            }
        }
        // fail-closed: 손상된 설정으로는 링크를 내려주지 않는다(설계 §2-2).
        Err(e) => AppLinksStatus {
            ok: false,
            error: Some(e),
            file_path,
            file_exists,
            links: Vec::new(),
            warnings: Vec::new(),
            limits: limits(),
        },
    }
}

/// 항상 성공한다(손상 파일도 ok:false를 담은 정상 응답으로 내려간다).
#[tauri::command]
pub fn app_links_get() -> AppLinksStatus {
    let _guard = APP_LINKS_FILE_LOCK.lock().unwrap();
    build_status(store::app_links_file_path().as_deref())
}

/// 전체 목록 치환 저장. 추가·수정·삭제·토글·순서변경이 전부 이 하나를 쓴다.
/// 검증 실패는 전량 거부(부분 저장 없음). 성공 시 저장 직후 상태를 그대로
/// 반환해 프론트가 재조회 없이 최신 값을 받는다.
#[tauri::command]
pub fn app_links_save(links: Vec<AppLink>) -> Result<AppLinksStatus, String> {
    let _guard = APP_LINKS_FILE_LOCK.lock().unwrap();

    let path = store::app_links_file_path()
        .ok_or_else(|| "홈 디렉터리를 확인할 수 없습니다.".to_string())?;

    let validated = store::validate_links(&links)?;
    let file = AppLinksFile {
        version: 1,
        links: validated,
    };
    store::save_to_path(&path, &file)?;

    Ok(build_status(Some(&path)))
}

/// 저장된 링크를 OS 기본 브라우저로 연다. 프론트는 URL이 아니라 id만 넘긴다
/// (§6 보안 — 열리는 URL은 반드시 저장·검증을 통과한 목록 안의 값이어야 한다).
///
/// 내부 순서(고정, §3-3):
/// 1. 파일 로드 실패 → `Err`(fail-closed, 손상된 설정으로는 아무것도 열지 않는다)
/// 2. id로 항목 검색 → 없으면 `Err`
/// 3. 저장 때와 동일한 `validate_url`로 재검증(손으로 편집된 파일 방어)
/// 4. `app.opener().open_url()` — 셸을 거치지 않는다
#[tauri::command]
pub fn app_links_open(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let _guard = APP_LINKS_FILE_LOCK.lock().unwrap();

    let path = store::app_links_file_path()
        .ok_or_else(|| "홈 디렉터리를 확인할 수 없습니다.".to_string())?;
    let file = store::load_from_path(&path)?;
    let link = store::find_link_for_open(&file.links, &id)?;

    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&link.url, None::<&str>)
        .map_err(|e| format!("브라우저를 열지 못했습니다: {e}"))?;

    Ok(())
}
