// 실제 reqwest 기반 다운로더(`ReqwestFetcher`)를 로컬 TCP 서버에 붙여 검증한다 — 프로덕션과
// 같은 리다이렉트 정책 콜백·크기 상한·정체/취소 로직이 실제로 동작하는지 본다. 허용 출처는
// 테스트가 로컬 서버로 주입한다(프로덕션은 https://nodejs.org/dist/ 고정).

use super::fetch::{FetchError, FetchPolicy, ReqwestFetcher};
use super::types::InstallError;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

type Handler = Arc<dyn Fn(&mut TcpStream, &str) + Send + Sync>;

struct TestServer {
    port: u16,
    hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // accept 루프를 깨운다.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn read_request_path(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    while !buf.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        buf.push(byte[0]);
        if buf.len() > 16 * 1024 {
            return None;
        }
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let first = text.lines().next()?;
    first.split_whitespace().nth(1).map(str::to_string)
}

fn spawn_server(handler: Handler) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (hits2, stop2) = (hits.clone(), stop.clone());
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            if stop2.load(Ordering::SeqCst) {
                return;
            }
            let Ok(mut stream) = conn else { continue };
            let handler = handler.clone();
            let hits3 = hits2.clone();
            std::thread::spawn(move || {
                if let Some(path) = read_request_path(&mut stream) {
                    hits3.fetch_add(1, Ordering::SeqCst);
                    handler(&mut stream, &path);
                }
            });
        }
    });
    TestServer { port, hits, stop }
}

fn respond(stream: &mut TcpStream, status: &str, headers: &[(&str, String)], body: &[u8]) {
    let mut head = format!("HTTP/1.1 {status}\r\n");
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn fetcher(port: u16) -> ReqwestFetcher {
    ReqwestFetcher::new(FetchPolicy::for_test_origin("http", "127.0.0.1", port))
}

fn get(f: &ReqwestFetcher, url: &str, max: u64) -> Result<Vec<u8>, FetchError> {
    let mut sink: Vec<u8> = Vec::new();
    f.get(url, max, &mut sink, &|| false).map(|_| sink)
}

#[test]
fn real_http_client_fetches_body_from_allowed_origin() {
    let server = spawn_server(Arc::new(|s, _| {
        respond(s, "200 OK", &[("Content-Length", "5".to_string())], b"hello");
    }));
    let body = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1024).unwrap();
    assert_eq!(body, b"hello");
}

#[test]
fn redirect_to_other_host_is_refused_by_real_client() {
    let server = spawn_server(Arc::new(|s, _| {
        respond(
            s,
            "302 Found",
            &[("Location", "https://evil.example/dist/index.json".to_string()), ("Content-Length", "0".to_string())],
            b"",
        );
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1024).unwrap_err();
    assert!(matches!(err, FetchError::Rejected(_)), "{err:?}");
    // 설치 파이프라인에서는 리다이렉트 거부 오류로 올라온다.
    assert!(matches!(err.into_install_error("t"), InstallError::RedirectRejected(_)));
}

#[test]
fn redirect_to_other_port_on_same_host_is_refused_and_target_never_contacted() {
    let other = spawn_server(Arc::new(|s, _| {
        respond(s, "200 OK", &[("Content-Length", "2".to_string())], b"ok");
    }));
    let other_port = other.port;
    let server = spawn_server(Arc::new(move |s, _| {
        respond(
            s,
            "302 Found",
            &[("Location", format!("http://127.0.0.1:{other_port}/dist/x")), ("Content-Length", "0".to_string())],
            b"",
        );
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1024).unwrap_err();
    assert!(matches!(err, FetchError::Rejected(_)), "{err:?}");
    assert_eq!(other.hits.load(Ordering::SeqCst), 0, "허용 밖 목적지에는 연결하지 않아야 합니다");
}

#[test]
fn redirect_to_path_outside_dist_prefix_is_refused() {
    let server = spawn_server(Arc::new(|s, path| {
        if path == "/dist/a" {
            respond(s, "302 Found", &[("Location", "/other/x".to_string()), ("Content-Length", "0".to_string())], b"");
        } else {
            respond(s, "200 OK", &[("Content-Length", "2".to_string())], b"ok");
        }
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1024).unwrap_err();
    assert!(matches!(err, FetchError::Rejected(_)), "{err:?}");
}

#[test]
fn redirect_within_same_origin_is_followed() {
    let server = spawn_server(Arc::new(|s, path| {
        if path == "/dist/a" {
            respond(s, "302 Found", &[("Location", "/dist/b".to_string()), ("Content-Length", "0".to_string())], b"");
        } else {
            respond(s, "200 OK", &[("Content-Length", "4".to_string())], b"done");
        }
    }));
    let body = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1024).unwrap();
    assert_eq!(body, b"done");
}

#[test]
fn redirect_loop_beyond_hop_limit_is_refused() {
    let server = spawn_server(Arc::new(|s, path| {
        let n: usize = path.trim_start_matches("/dist/").parse().unwrap_or(0);
        respond(
            s,
            "302 Found",
            &[("Location", format!("/dist/{}", n + 1)), ("Content-Length", "0".to_string())],
            b"",
        );
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/0", server.port), 1024).unwrap_err();
    assert!(matches!(err, FetchError::Rejected(_)), "{err:?}");
    assert!(server.hits.load(Ordering::SeqCst) <= 5, "리다이렉트 횟수 상한을 넘어 계속 따라갔습니다");
}

#[test]
fn request_url_outside_policy_is_rejected_without_sending() {
    let server = spawn_server(Arc::new(|s, _| {
        respond(s, "200 OK", &[("Content-Length", "2".to_string())], b"ok");
    }));
    let f = fetcher(server.port);
    for url in [
        format!("http://127.0.0.1:{}/other/x", server.port),
        format!("http://localhost:{}/dist/x", server.port),
        "https://nodejs.org/dist/index.json".to_string(),
        "not a url".to_string(),
    ] {
        assert!(matches!(get(&f, &url, 1024), Err(FetchError::Rejected(_))), "{url}");
    }
    assert_eq!(server.hits.load(Ordering::SeqCst), 0, "정책 밖 URL은 요청 자체를 보내면 안 됩니다");
}

#[test]
fn download_size_cap_declared_content_length_is_refused_before_reading() {
    let server = spawn_server(Arc::new(|s, _| {
        respond(s, "200 OK", &[("Content-Length", "100000".to_string())], &[b'x'; 100]);
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1000).unwrap_err();
    assert!(matches!(err, FetchError::TooLarge { limit: 1000 }), "{err:?}");
}

#[test]
fn download_size_cap_applies_to_bodies_without_content_length() {
    let server = spawn_server(Arc::new(|s, _| {
        // Content-Length 없이 연결을 닫을 때까지 본문(HTTP/1.1 close-delimited).
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
        for _ in 0..200 {
            if s.write_all(&[b'y'; 1024]).is_err() {
                return;
            }
        }
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 5000).unwrap_err();
    assert!(matches!(err, FetchError::TooLarge { .. }), "{err:?}");
}

#[test]
fn download_interrupted_midway_is_an_error_not_a_short_success() {
    let server = spawn_server(Arc::new(|s, _| {
        // 100바이트를 약속하고 10바이트만 보낸 뒤 연결을 끊는다.
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n0123456789");
        let _ = s.flush();
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1000).unwrap_err();
    assert!(matches!(err, FetchError::Network(_)), "{err:?}");
}

#[test]
fn stalled_server_is_cut_by_the_stall_timeout() {
    let server = spawn_server(Arc::new(|s, _| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabcde");
        let _ = s.flush();
        std::thread::sleep(Duration::from_secs(5));
    }));
    let f = fetcher(server.port).with_timeouts(Duration::from_millis(400), Duration::from_secs(30));
    let started = Instant::now();
    let err = get(&f, &format!("http://127.0.0.1:{}/dist/a", server.port), 1000).unwrap_err();
    assert!(matches!(err, FetchError::Stalled), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn cancel_interrupts_a_stalled_download_promptly() {
    let server = spawn_server(Arc::new(|s, _| {
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabcde");
        let _ = s.flush();
        std::thread::sleep(Duration::from_secs(5));
    }));
    let f = fetcher(server.port).with_timeouts(Duration::from_secs(60), Duration::from_secs(60));
    let started = Instant::now();
    let cancel_flag = AtomicBool::new(false);
    let mut sink: Vec<u8> = Vec::new();
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(300));
            cancel_flag.store(true, Ordering::SeqCst);
        });
        f.get(
            &format!("http://127.0.0.1:{}/dist/a", server.port),
            1000,
            &mut sink,
            &|| cancel_flag.load(Ordering::SeqCst),
        )
    });
    assert!(matches!(result, Err(FetchError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(3), "취소가 멈춘 소켓 때문에 지연됐습니다");
}

#[test]
fn non_success_status_is_reported() {
    let server = spawn_server(Arc::new(|s, _| {
        respond(s, "404 Not Found", &[("Content-Length", "0".to_string())], b"");
    }));
    let err = get(&fetcher(server.port), &format!("http://127.0.0.1:{}/dist/a", server.port), 1000).unwrap_err();
    assert!(matches!(err, FetchError::Status(404)), "{err:?}");
}

#[test]
fn sink_write_failure_surfaces_as_disk_full_install_error() {
    struct FullDisk;
    impl Write for FullDisk {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::StorageFull))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let server = spawn_server(Arc::new(|s, _| {
        respond(s, "200 OK", &[("Content-Length", "5".to_string())], b"hello");
    }));
    let f = fetcher(server.port);
    let err = f
        .get(&format!("http://127.0.0.1:{}/dist/a", server.port), 1000, &mut FullDisk, &|| false)
        .unwrap_err();
    assert!(matches!(err, FetchError::Sink(_)), "{err:?}");
    assert!(matches!(err.into_install_error("t"), InstallError::DiskFull(_)));
}
