#[path = "support/proxy.rs"]
#[allow(dead_code)]
mod fixture;

use codex2api_accounts::{AccountIdentity, HostRuntime};
use codex2api_auth::transport::AccountHttpClients;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

async fn reply<S: AsyncRead + AsyncWrite + Unpin>(socket: &mut S, socks: bool, remote_dns: bool) {
    if socks {
        let host = fixture::socks_tunnel(socket).await;
        if remote_dns {
            assert_eq!(host, "official.invalid:8080");
        } else {
            assert!(
                host.starts_with("127.0.0.1:")
                    || host.starts_with("::1:")
                    || host.starts_with("[::1]:"),
                "SOCKS5 target: {host}"
            );
        }
    }
    let headers = fixture::read_headers(socket).await.unwrap();
    if !socks {
        assert!(headers.starts_with("GET http://official.invalid:8080/official HTTP/1.1"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("proxy-authorization: basic dxnlcjpwyxnz")
        );
    }
    socket
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nproxied")
        .await
        .unwrap();
    socket.shutdown().await.unwrap();
}

#[tokio::test]
async fn all_account_http_clients_use_explicit_proxy_or_direct_and_never_fall_back() {
    // Isolate environment overrides from other test threads; this also proves system
    // proxies cannot override either an explicit account proxy or direct mode.
    if std::env::var_os("CODEX2API_PROXY_TEST_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "all_account_http_clients_use_explicit_proxy_or_direct_and_never_fall_back",
                "--nocapture",
            ])
            .env("CODEX2API_PROXY_TEST_CHILD", "1")
            .env(
                "CODEX_CA_CERTIFICATE",
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/proxy-ca.pem"),
            )
            .env("HTTP_PROXY", "http://127.0.0.1:9")
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env("ALL_PROXY", "http://127.0.0.1:9")
            .env("NO_PROXY", "")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let identity = AccountIdentity::new(
        "test",
        uuid::Uuid::new_v4().to_string(),
        HostRuntime::generate(),
    );
    for scheme in ["http", "https", "socks5", "socks5h"] {
        let (listener, addr) = fixture::listener().await;
        let tls = fixture::acceptor();
        let server = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().await.unwrap();
                if scheme == "https" {
                    let mut socket = tls.accept(socket).await.unwrap();
                    reply(&mut socket, false, false).await;
                } else {
                    reply(
                        &mut socket,
                        scheme.starts_with("socks"),
                        scheme == "socks5h",
                    )
                    .await;
                }
            }
        });
        let clients = AccountHttpClients::with_proxy(
            &identity,
            Some(&format!("{scheme}://user:pass@{addr}")),
        )
        .unwrap();
        let host = if scheme == "socks5" {
            "localhost"
        } else {
            "official.invalid"
        };
        for client in [&clients.raw, &clients.authenticated, &clients.api] {
            let response = client
                .get(format!("http://{host}:8080/official"))
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), "proxied");
        }
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
        let (direct, direct_addr) = fixture::listener().await;
        let result = clients
            .api
            .get(format!("http://{direct_addr}/official"))
            .timeout(Duration::from_secs(2))
            .send()
            .await;
        assert!(
            result.is_err(),
            "unavailable proxy must not fall back to direct"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), direct.accept())
                .await
                .is_err()
        );
    }
    let (listener, addr) = fixture::listener().await;
    let server = tokio::spawn(async move {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let headers = fixture::read_headers(&mut socket).await.unwrap();
            assert!(headers.starts_with("GET /direct HTTP/1.1"));
            assert!(!headers.to_ascii_lowercase().contains("proxy-authorization"));
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\ndirect",
                )
                .await
                .unwrap();
        }
    });
    let clients = AccountHttpClients::new(&identity).unwrap();
    for client in [&clients.raw, &clients.authenticated, &clients.api] {
        assert_eq!(
            client
                .get(format!("http://{addr}/direct"))
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "direct"
        );
    }
    server.await.unwrap();
}
