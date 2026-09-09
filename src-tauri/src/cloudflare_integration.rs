// ---------------- Cloudflare 연동 (CLI 위임, wrangler) ----------------
// GitHub 연동과 같은 구조: 자격증명은 wrangler 자신이 들고 있고, 이 앱은 상태만
// 읽는다. 이 개발 머신에는 wrangler가 아예 설치되어 있지 않다(PATH·node_modules·
// package.json 전부 0건, 실측 확인됨) — 그래서 "미설치"를 예외로 던지지 않고
// installed:false 정상 값으로 돌려준다. 프론트엔드가 이 값으로 "개발 환경 설치
// 안내" 화면을 보여줄 수 있다.
//
// `wrangler whoami`의 정확한 출력 포맷은 이 머신에서 wrangler를 실행해볼 수 없어
// 실측하지 못했다 — wrangler 공개 문서에 나온 "You are logged in with an OAuth
// Token, associated with the email <email>." 문구를 기준으로 느슨하게 파싱한다.
// 파싱에 실패해도 에러를 내지 않고 connected 여부(종료코드)만 신뢰값으로 남긴다.
//
// connected는 오직 종료코드로만 판단하고, 이메일 파싱 성공 여부와 분리한다 —
// wrangler가 API 토큰(`CLOUDFLARE_API_TOKEN` 등)으로 인증된 경우 whoami가
// 이메일을 출력하지 않을 수 있고(OAuth 세션에서만 이메일이 나온다는 것이 문서상
// 문구다), 그 경우까지 "이메일을 못 뽑았으니 연결 안 됨"으로 보고하면 실제로는
// 연결돼 있는데 거짓으로 미연결 처리하게 된다. 이메일은 어디까지나 있으면
// 붙이는 부가 표시값(`email: Option<String>`)일 뿐, 판정 신호가 아니다.
// `github_status`(종료코드 기반)와 판정 방식을 맞춰 두 탭의 신뢰 기준을
// 일관되게 유지한다.
//
// 다만 이 신호도 완전하지는 않다: wrangler는 미인증 상태에서도 whoami가
// 종료코드 0을 낼 수 있다는 보고가 있어(이 머신에 wrangler가 없어 실측 불가),
// 종료코드만으로 오탐(false positive: 실제로는 미인증인데 connected:true)이
// 날 가능성을 배제하지 못한다. 이 불확실성을 코드가 단정하지 않도록 남겨둔다 —
// wrangler를 설치해 실측할 수 있게 되면 이 가정을 재검증해야 한다.

use crate::cli_launcher::{open_terminal_command, resolve_binary, TerminalLaunchResult};
use serde::Serialize;
use std::process::Command;

const WRANGLER_CANDIDATES: [&str; 2] = ["/opt/homebrew/bin/wrangler", "/usr/local/bin/wrangler"];

fn resolve_wrangler() -> Option<String> {
    resolve_binary(&WRANGLER_CANDIDATES, "wrangler")
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct CloudflareStatus {
    pub installed: bool,
    pub connected: bool,
    pub email: Option<String>,
}

/// stdout에서 "email <값>." 패턴만 뽑는다. 이메일 자체에 '.'이 들어가므로(도메인)
/// 공백/줄바꿈까지만 잘라낸 뒤, 문장을 맺는 마지막 마침표 하나만 별도로 제거한다.
/// 실패해도 None일 뿐 에러가 아니다.
fn extract_email_from_whoami(stdout: &str) -> Option<String> {
    let marker = "email ";
    let idx = stdout.find(marker)?;
    let rest = &stdout[idx + marker.len()..];
    let end = rest.find(['\n', ' ']).unwrap_or(rest.len());
    let candidate = rest[..end].trim().trim_matches('"');
    let candidate = candidate.strip_suffix('.').unwrap_or(candidate);
    if candidate.contains('@') {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// ① 현재 연동 상태 조회. `wrangler whoami`의 종료코드**로만** 로그인 여부를
/// 판단한다(이메일 파싱 성공 여부는 판정에 관여하지 않는다 — API 토큰 인증처럼
/// whoami가 이메일을 출력하지 않는 경우에도 종료코드가 성공이면 connected는
/// true다). 가능하면 stdout에서 이메일만 부가 표시값으로 덧붙인다. wrangler
/// 미설치는 정상 경로다.
#[tauri::command]
pub fn cloudflare_status() -> CloudflareStatus {
    let Some(wrangler) = resolve_wrangler() else {
        return CloudflareStatus {
            installed: false,
            ..Default::default()
        };
    };

    let output = Command::new(&wrangler).arg("whoami").output();
    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let email = extract_email_from_whoami(&stdout);
            CloudflareStatus {
                installed: true,
                connected: true,
                email,
            }
        }
        _ => CloudflareStatus {
            installed: true,
            connected: false,
            email: None,
        },
    }
}

/// ② 연결 시작. `wrangler login`을 앱이 대신 실행하지 않고 터미널 창을 열어
/// 사용자가 직접 브라우저 인증을 진행하게 한다. 미설치면 로그인을 시도하지 않고
/// 설치 안내 메시지를 돌려준다.
#[tauri::command]
pub fn cloudflare_connect() -> Result<TerminalLaunchResult, String> {
    let Some(wrangler) = resolve_wrangler() else {
        return Ok(TerminalLaunchResult {
            opened: false,
            message: "Wrangler CLI가 설치되어 있지 않습니다. 프로젝트에 devDependency로 추가하거나 `npm install -g wrangler`로 설치한 뒤 다시 시도해주세요.".to_string(),
        });
    };
    open_terminal_command(&format!("{wrangler} login"))?;
    Ok(TerminalLaunchResult {
        opened: true,
        message: "터미널 창에서 Cloudflare 로그인 절차를 진행해주세요.".to_string(),
    })
}

/// ③ 연결 해제. 같은 이유로 `wrangler logout`도 터미널 창을 열어 사용자가 직접
/// 확인하며 로그아웃하게 한다.
#[tauri::command]
pub fn cloudflare_disconnect() -> Result<TerminalLaunchResult, String> {
    let Some(wrangler) = resolve_wrangler() else {
        return Ok(TerminalLaunchResult {
            opened: false,
            message: "Wrangler CLI가 설치되어 있지 않습니다.".to_string(),
        });
    };
    open_terminal_command(&format!("{wrangler} logout"))?;
    Ok(TerminalLaunchResult {
        opened: true,
        message: "터미널 창에서 Cloudflare 로그아웃 절차를 진행해주세요.".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::extract_email_from_whoami;

    #[test]
    fn extracts_email_from_typical_whoami_output() {
        let sample = "You are logged in with an OAuth Token, associated with the email test@malgnsoft.com.\n";
        assert_eq!(
            extract_email_from_whoami(sample),
            Some("test@malgnsoft.com".to_string())
        );
    }

    #[test]
    fn returns_none_when_no_email_present() {
        let sample = "You are not authenticated. Please run `wrangler login`.\n";
        assert_eq!(extract_email_from_whoami(sample), None);
    }
}
