// ==================== HTTP 다운로드(출처 고정·리다이렉트 제한·방어) ====================
// 설치 파이프라인이 네트워크에 닿는 유일한 자리. 방어:
//   - 요청 URL 자체와 모든 리다이렉트 목적지가 `FetchPolicy`(https + nodejs.org +
//     /dist/ 접두, 기본 포트, 자격증명·프래그먼트 없음)를 통과해야 한다. 어긋나면
//     요청을 보내지 않거나(첫 URL) 이동을 거부한다(리다이렉트, 최대 3회).
//   - 응답 본문은 상한(`max_bytes`)을 넘으면 중단한다. Content-Length가 있으면 받기
//     전에 상한과 비교하고, 끝난 뒤 실제 수신량과도 대조한다(중간에 끊긴 다운로드).
//   - 읽기는 작업 스레드가 하고 호출 스레드는 100ms마다 취소·정체·전체 제한시간을
//     확인한다 — 소켓이 멈춰도 취소가 즉시 먹고, 정체(stall)는 별도 상한으로 끊는다.
// 테스트 가능성: `FetchPolicy`는 허용 출처를 값으로 받는다(프로덕션은 `nodejs_org()`
// 고정, 테스트는 로컬 서버 출처를 주입해 실제 reqwest 리다이렉트 거부를 검증한다).

use super::types::InstallError;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use url::Url;

#[derive(Debug)]
pub(crate) enum FetchError {
    /// 첫 URL 또는 리다이렉트 목적지가 허용 출처 밖.
    Rejected(String),
    Status(u16),
    TooLarge { limit: u64 },
    Cancelled,
    Stalled,
    Network(String),
    /// 받은 본문을 sink(파일/버퍼)에 쓰다 실패 — 디스크 부족 판별을 위해 원본 io 오류를 보존한다.
    Sink(std::io::Error),
}

impl FetchError {
    pub(crate) fn into_install_error(self, context: &str) -> InstallError {
        match self {
            FetchError::Rejected(s) => InstallError::RedirectRejected(s),
            FetchError::Status(code) => InstallError::Network(format!("{context}: HTTP {code}")),
            FetchError::TooLarge { limit } => InstallError::TooLarge { limit },
            FetchError::Cancelled => InstallError::Cancelled,
            FetchError::Stalled => InstallError::Network(format!("{context}: 응답이 오래 멈춰 중단했습니다")),
            FetchError::Network(s) => InstallError::Network(format!("{context}: {s}")),
            FetchError::Sink(e) => super::types::io_error(context, e),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FetchPolicy {
    scheme: String,
    host: String,
    port: Option<u16>,
    path_prefix: String,
}

impl FetchPolicy {
    /// 프로덕션 정책: `https://nodejs.org/dist/…` 하나뿐.
    pub(crate) fn nodejs_org() -> Self {
        FetchPolicy {
            scheme: "https".to_string(),
            host: "nodejs.org".to_string(),
            port: None,
            path_prefix: "/dist/".to_string(),
        }
    }

    /// 테스트 전용: 로컬 서버를 허용 출처로 쓴다. 프로덕션 코드 경로는 호출하지 않는다.
    #[cfg(test)]
    pub(crate) fn for_test_origin(scheme: &str, host: &str, port: u16) -> Self {
        FetchPolicy {
            scheme: scheme.to_string(),
            host: host.to_string(),
            port: Some(port),
            path_prefix: "/dist/".to_string(),
        }
    }

    pub(crate) fn allows(&self, url: &Url) -> bool {
        url.scheme() == self.scheme
            && url.host_str() == Some(self.host.as_str())
            && url.port() == self.port
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.path().starts_with(&self.path_prefix)
    }
}

/// 리다이렉트 최대 횟수.
const MAX_REDIRECTS: usize = 3;

pub(crate) struct ReqwestFetcher {
    policy: FetchPolicy,
    stall_timeout: Duration,
    total_timeout: Duration,
    connect_timeout: Duration,
}

enum Msg {
    Head { content_length: Option<u64> },
    Chunk(Vec<u8>),
    Done,
    Failed(FetchError),
}

impl ReqwestFetcher {
    pub(crate) fn new(policy: FetchPolicy) -> Self {
        ReqwestFetcher {
            policy,
            stall_timeout: Duration::from_secs(30),
            total_timeout: Duration::from_secs(20 * 60),
            connect_timeout: Duration::from_secs(15),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_timeouts(mut self, stall: Duration, total: Duration) -> Self {
        self.stall_timeout = stall;
        self.total_timeout = total;
        self
    }

    pub(crate) fn get(
        &self,
        url: &str,
        max_bytes: u64,
        sink: &mut dyn Write,
        cancel: &dyn Fn() -> bool,
    ) -> Result<(), FetchError> {
        let parsed = Url::parse(url).map_err(|e| FetchError::Rejected(format!("{url} ({e})")))?;
        if !self.policy.allows(&parsed) {
            return Err(FetchError::Rejected(url.to_string()));
        }
        if cancel() {
            return Err(FetchError::Cancelled);
        }

        let (tx, rx) = mpsc::sync_channel::<Msg>(4);
        let policy = self.policy.clone();
        let connect_timeout = self.connect_timeout;
        let total_timeout = self.total_timeout;
        let request_url = parsed.to_string();
        std::thread::spawn(move || {
            worker(request_url, policy, connect_timeout, total_timeout, max_bytes, tx);
        });

        let started = Instant::now();
        let mut last_progress = Instant::now();
        let mut received: u64 = 0;
        let mut expected_len: Option<u64> = None;
        loop {
            if cancel() {
                return Err(FetchError::Cancelled);
            }
            if started.elapsed() >= self.total_timeout {
                return Err(FetchError::Network("전체 제한시간을 넘었습니다".to_string()));
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Msg::Head { content_length }) => {
                    expected_len = content_length;
                    last_progress = Instant::now();
                }
                Ok(Msg::Chunk(bytes)) => {
                    received += bytes.len() as u64;
                    if received > max_bytes {
                        return Err(FetchError::TooLarge { limit: max_bytes });
                    }
                    sink.write_all(&bytes).map_err(FetchError::Sink)?;
                    last_progress = Instant::now();
                }
                Ok(Msg::Done) => {
                    if let Some(expected) = expected_len {
                        if received != expected {
                            return Err(FetchError::Network(format!(
                                "다운로드가 중간에 끊겼습니다({received}/{expected} 바이트)"
                            )));
                        }
                    }
                    sink.flush().map_err(FetchError::Sink)?;
                    return Ok(());
                }
                Ok(Msg::Failed(e)) => return Err(e),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if last_progress.elapsed() >= self.stall_timeout {
                        return Err(FetchError::Stalled);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(FetchError::Network("다운로드 작업이 예기치 않게 끝났습니다".to_string()));
                }
            }
        }
    }
}

fn worker(
    url: String,
    policy: FetchPolicy,
    connect_timeout: Duration,
    total_timeout: Duration,
    max_bytes: u64,
    tx: mpsc::SyncSender<Msg>,
) {
    let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error("리다이렉트 횟수 상한을 넘었습니다");
        }
        if policy.allows(attempt.url()) {
            attempt.follow()
        } else {
            let target = attempt.url().to_string();
            attempt.error(format!("허용되지 않은 리다이렉트 목적지: {target}"))
        }
    });
    let client = match reqwest::blocking::Client::builder()
        .redirect(redirect_policy)
        .connect_timeout(connect_timeout)
        .timeout(total_timeout)
        .user_agent(concat!("malgn-agent/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(Msg::Failed(FetchError::Network(format!("HTTP 클라이언트 생성 실패: {e}"))));
            return;
        }
    };
    let mut response = match client.get(&url).send() {
        Ok(r) => r,
        Err(e) => {
            let err = if e.is_redirect() {
                FetchError::Rejected(e.to_string())
            } else {
                FetchError::Network(e.to_string())
            };
            let _ = tx.send(Msg::Failed(err));
            return;
        }
    };
    let status = response.status();
    if !status.is_success() {
        let _ = tx.send(Msg::Failed(FetchError::Status(status.as_u16())));
        return;
    }
    let content_length = response.content_length();
    if let Some(len) = content_length {
        if len > max_bytes {
            let _ = tx.send(Msg::Failed(FetchError::TooLarge { limit: max_bytes }));
            return;
        }
    }
    if tx.send(Msg::Head { content_length }).is_err() {
        return;
    }
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match response.read(&mut buf) {
            Ok(0) => {
                let _ = tx.send(Msg::Done);
                return;
            }
            Ok(n) => {
                if tx.send(Msg::Chunk(buf[..n].to_vec())).is_err() {
                    return; // 호출 쪽이 취소·실패로 떠났다.
                }
            }
            Err(e) => {
                let _ = tx.send(Msg::Failed(FetchError::Network(e.to_string())));
                return;
            }
        }
    }
}
