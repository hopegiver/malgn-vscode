// ==================== 10. 프론트 계약(부록 C) — 반환 타입 ====================

use super::DevTool;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DevToolStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub install_method: Option<String>,
    /// "run" | "manual" | "none"
    pub action_kind: String,
    pub manual_hint: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DevToolPreview {
    pub id: String,
    pub plan_id: String,
    pub will_run: bool,
    pub command_display: String,
    pub affected: Vec<String>,
    pub notes: String,
    /// 과제 4(fail-open 수정): dry-run 미리보기가 spawn 실패하거나 타임아웃하면
    /// `affected`는 안전한 기본값(`vec![def.label]`, 길이 1)으로 채워지는데, 이
    /// 길이만으로는 "실제로 영향 범위가 1개로 확인됨"과 구분이 안 된다. 이 필드가
    /// false면 `affected`/`notes`를 신뢰할 수 없다는 뜻이며, 프론트는 이 경우
    /// "전체 업데이트" 배치 자동실행에서 반드시 제외하고 개별 확인 대기로 남겨야
    /// 한다(개별 실행 경로는 화면에 그대로 노출해 사용자가 판단하게 한다).
    pub preview_reliable: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Updated,
    AlreadyLatest,
    UnknownAfter,
    Failed,
    TimedOut,
    NotSupported,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DevToolActionResult {
    pub id: String,
    pub outcome: Outcome,
    pub verified: bool,
    pub version_before: Option<String>,
    pub version_after: Option<String>,
    pub normalized_before: Option<String>,
    pub normalized_after: Option<String>,
    pub install_method: String,
    pub ran_command: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub message: String,
    pub log_tail: String,
    // 신규 요구사항: 사용자 로그인 셸 PATH 노출 확인.
    pub path_visible: bool,
    pub path_hint: Option<String>,
    pub path_hint_target: Option<String>,
}

pub(crate) fn not_supported_result(def: &DevTool, message: String) -> DevToolActionResult {
    DevToolActionResult {
        id: def.key.to_string(),
        outcome: Outcome::NotSupported,
        verified: false,
        version_before: None,
        version_after: None,
        normalized_before: None,
        normalized_after: None,
        install_method: String::new(),
        ran_command: None,
        exit_code: None,
        duration_ms: 0,
        message,
        log_tail: String::new(),
        path_visible: false,
        path_hint: None,
        path_hint_target: None,
    }
}
