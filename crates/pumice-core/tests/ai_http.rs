//! The explain call against a local fake OpenAI-compatible server.
use pumice_core::{ai, settings::Settings};
use std::io::{Read, Write};
use std::net::TcpListener;

fn serve_once(reply: &'static str, status: &'static str) -> (u16, std::thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut buf = vec![0u8; 16384];
        let mut got = String::new();
        // Read until the JSON body is complete.
        loop {
            let n = s.read(&mut buf).unwrap();
            got.push_str(&String::from_utf8_lossy(&buf[..n]));
            if let Some(i) = got.find("\r\n\r\n") {
                let len: usize = got[..i].lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())).unwrap_or(0);
                if got.len() >= i + 4 + len {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        let resp = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len());
        s.write_all(resp.as_bytes()).unwrap();
        got
    });
    (port, h)
}

#[test]
fn explains_through_an_openai_compatible_server() {
    let (port, h) = serve_once(r#"{"choices":[{"message":{"content":"It is a cache. Safe to remove."}}]}"#, "200 OK");
    let s = Settings { ai_provider: "openai".into(), ai_base_url: format!("http://127.0.0.1:{port}/v1"), ai_model: "test-model".into(), ..Settings::default() };
    let text = ai::explain(&s, "cleanup rule", "Name: Application caches\nPaths: ~/.cache/x").unwrap();
    assert_eq!(text, "It is a cache. Safe to remove.");
    let request = h.join().unwrap();
    assert!(request.starts_with("POST /v1/chat/completions"), "{request}");
    assert!(request.contains("test-model") && request.contains("Application caches"));
}

#[test]
fn reports_server_errors_and_refuses_bad_addresses() {
    let (port, h) = serve_once(r#"{"error":"bad key"}"#, "401 Unauthorized");
    let mut s = Settings { ai_provider: "openai".into(), ai_base_url: format!("http://127.0.0.1:{port}"), ai_model: "m".into(), ..Settings::default() };
    let e = ai::explain(&s, "x", "y").unwrap_err().to_string();
    h.join().unwrap();
    assert!(e.contains("401"), "{e}");
    s.ai_base_url = "http://example.com".into();
    assert!(ai::explain(&s, "x", "y").is_err());
    s.ai_provider = "off".into();
    assert!(ai::explain(&s, "x", "y").unwrap_err().to_string().contains("not set up"));
}
