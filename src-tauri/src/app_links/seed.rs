// 앱링크 시드(초기 기본값) — `~/.claude/malgn-agent-apps.json` 파일이 아직
// 없는 최초 실행 시에만 사용자에게 보여주는 "편집 가능한 기본값"이다.
// 하드코딩된 고정 카탈로그가 아니다: 사용자가 설정 화면에서 자유롭게
// 수정·삭제·추가할 수 있고, 한 번이라도 저장하면(전부 지우고 빈 목록으로
// 저장해도) 그 뒤로는 이 함수가 다시 호출되지 않는다 — 판별 기준은 파일의
// 존재 여부뿐이다(`store::load_from_path`가 파일 없음/있음을 가른다).
//
// 호스트명 리터럴을 이 파일 한 곳에만 두는 이유: 이 저장소는 public이라 사내
// 호스트명이 소스에 남는다. 자격증명이 아니고 대부분 이미 공개된 값이라 그대로
// 커밋하기로 했지만, 방침이 바뀌면 이 파일 내부만 `option_env!` 주입 방식
// (`build.rs`의 `GOOGLE_OAUTH_CLIENT_SECRET`·`otel_settings.rs`의
// `MALGN_OTEL_COLLECTOR_BASE`와 동일 패턴)으로 교체하면 된다 — 접근 지점이
// `default_links()` 함수 하나뿐이라 호출부(`store.rs`)로 교체가 전파되지 않는다.

use super::store::AppLink;

fn seed_link(id: &str, name: &str, url: &str) -> AppLink {
    AppLink {
        // 시드 id는 매 호출마다 새로 발급하지 않는다 — 고정 슬러그라야
        // 프론트 렌더가 안정적이다(같은 항목은 항상 같은 id).
        id: id.to_string(),
        name: name.to_string(),
        url: url.to_string(),
        enabled: true,
    }
}

/// 설정 파일이 존재하지 않을 때만 호출되는 초기 기본 목록.
pub fn default_links() -> Vec<AppLink> {
    vec![
        seed_link("seed-hrai", "성과관리시스템", "https://malgnsoft.hrai.kr"),
        seed_link(
            "seed-malgnai-hub",
            "맑은AI-Hub",
            "https://malgnai-hub.apiserver.kr",
        ),
        seed_link("seed-jira", "지라(Jira)", "https://malgn.atlassian.net/"),
        seed_link(
            "seed-gmail",
            "이메일(Gmail)",
            "https://mail.google.com/mail/u/0/",
        ),
        seed_link("seed-office", "맑은오피스", "https://office.malgnsoft.com"),
    ]
}
