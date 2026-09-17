use std::{io, sync::Arc};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, rustls};
use tokio_tungstenite::{
    MaybeTlsStream,
    tungstenite::{
        Error,
        proxy::{ProxyAuth, ProxyConfig, ProxyScheme},
    },
};

fn invalid(message: &str) -> Error {
    Error::Io(io::Error::new(io::ErrorKind::InvalidInput, message))
}

/// Explicit per-account transport. Never consult process proxy variables or fall back to direct.
pub(crate) async fn connect(
    destination: &str,
    proxy_url: Option<&str>,
    tls: Arc<rustls::ClientConfig>,
) -> Result<MaybeTlsStream<TcpStream>, Error> {
    let target = reqwest::Url::parse(destination).map_err(|_| invalid("Invalid WebSocket URL"))?;
    let host = target
        .host_str()
        .ok_or_else(|| invalid("Missing WebSocket host"))?;
    let host = host.trim_matches(['[', ']']);
    let port = target
        .port_or_known_default()
        .ok_or_else(|| invalid("Missing WebSocket port"))?;
    let Some(proxy_url) = proxy_url else {
        return Ok(MaybeTlsStream::Plain(
            TcpStream::connect((host, port)).await?,
        ));
    };
    let proxy = codex2api_storage::parse_proxy_url(proxy_url)
        .map_err(|_| invalid("Invalid account proxy"))?;
    let proxy_host = proxy
        .host_str()
        .ok_or_else(|| invalid("Missing proxy host"))?
        .trim_matches(['[', ']']);
    let proxy_port = proxy.port_or_known_default().unwrap_or(1080);
    let socket = TcpStream::connect((proxy_host, proxy_port)).await?;
    let socket = if proxy.scheme() == "https" {
        let name = rustls::pki_types::ServerName::try_from(proxy_host.to_owned())
            .map_err(|_| invalid("Invalid proxy TLS hostname"))?;
        MaybeTlsStream::Rustls(TlsConnector::from(tls).connect(name, socket).await?)
    } else {
        MaybeTlsStream::Plain(socket)
    };
    let auth = if !proxy.username().is_empty() || proxy.password().is_some() {
        Some(ProxyAuth {
            username: urlencoding::decode(proxy.username())
                .map_err(|_| invalid("Invalid proxy username"))?
                .into_owned(),
            password: urlencoding::decode(proxy.password().unwrap_or(""))
                .map_err(|_| invalid("Invalid proxy password"))?
                .into_owned(),
        })
    } else {
        None
    };
    let scheme = match proxy.scheme() {
        "http" | "https" => ProxyScheme::Http,
        "socks5" => ProxyScheme::Socks5,
        "socks5h" => ProxyScheme::Socks5h,
        _ => return Err(invalid("Unsupported proxy scheme")),
    };
    // The pinned fork sends hostnames remotely for both SOCKS variants. Resolve explicitly
    // for socks5, matching reqwest; socks5h must leave DNS to the proxy.
    let tunnel_host = if proxy.scheme() == "socks5" {
        tokio::net::lookup_host((host, port))
            .await?
            .next()
            .ok_or_else(|| invalid("Cannot resolve WebSocket host"))?
            .ip()
            .to_string()
    } else if matches!(scheme, ProxyScheme::Http) && host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let config = ProxyConfig {
        scheme,
        host: proxy_host.to_owned(),
        port: proxy_port,
        auth,
    };
    tokio_tungstenite::proxy::connect_via_proxy(socket, &config, &tunnel_host, port).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UpstreamClient, proxy_fixture as fixture};
    use codex2api_accounts::{AccountIdentity, HostRuntime};
    use codex2api_auth::transport::AccountHttpClients;
    use futures::{SinkExt, StreamExt};
    use tokio::io::{AsyncRead, AsyncWrite};

    async fn serve<S: AsyncRead + AsyncWrite + Unpin>(mut socket: S, scheme: &str) {
        let target = if scheme.starts_with("socks") {
            fixture::socks_tunnel(&mut socket).await
        } else {
            fixture::http_tunnel(&mut socket).await
        };
        if scheme == "socks5" {
            assert!(target == "127.0.0.1:443" || target == "::1:443");
        } else {
            assert_eq!(target, "localhost:443");
        }
        let socket = fixture::acceptor().accept(socket).await.unwrap();
        let mut ws = tokio_tungstenite::accept_hdr_async_with_config(
            socket,
            |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                assert!(!request.headers().contains_key("proxy-authorization"));
                assert_eq!(request.headers()["authorization"], "Bearer token");
                Ok(response)
            },
            Some(crate::websocket::websocket_config()),
        )
        .await
        .unwrap();
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            "proxied".into(),
        ))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn websockets_use_all_four_proxies_with_tls_dns_auth_and_direct_isolation() {
        if std::env::var_os("CODEX2API_WS_PROXY_CHILD").is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "proxy::tests::websockets_use_all_four_proxies_with_tls_dns_auth_and_direct_isolation", "--nocapture"])
                .env("CODEX2API_WS_PROXY_CHILD", "1")
                .env("CODEX_CA_CERTIFICATE", concat!(env!("CARGO_MANIFEST_DIR"), "/../codex2api-auth/tests/fixtures/proxy-ca.pem"))
                .env("HTTP_PROXY", "http://127.0.0.1:9").env("HTTPS_PROXY", "http://127.0.0.1:9")
                .env("ALL_PROXY", "http://127.0.0.1:9").env("NO_PROXY", "")
                .output().unwrap();
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
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                if scheme == "https" {
                    serve(fixture::acceptor().accept(socket).await.unwrap(), scheme).await;
                } else {
                    serve(socket, scheme).await;
                }
            });
            let mut client =
                UpstreamClient::new(identity.clone(), "token".into(), Some("account".into()))
                    .unwrap();
            client.use_account_http(Arc::new(
                AccountHttpClients::with_proxy(
                    &identity,
                    Some(&format!("{scheme}://user:pass@{addr}")),
                )
                .unwrap(),
            ));
            let (mut socket, _) = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                client.connect_websocket_to("wss://localhost/live", http::HeaderMap::new()),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap().into_text().unwrap(),
                "proxied"
            );
            server.await.unwrap();
            let (direct, direct_addr) = fixture::listener().await;
            assert!(
                client
                    .connect_websocket_to(
                        &format!("ws://{direct_addr}/live"),
                        http::HeaderMap::new()
                    )
                    .await
                    .is_err()
            );
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), direct.accept())
                    .await
                    .is_err()
            );
        }
        let (listener, addr) = fixture::listener().await;
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async_with_config(
                socket,
                Some(crate::websocket::websocket_config()),
            )
            .await
            .unwrap();
            ws.send(tokio_tungstenite::tungstenite::Message::Text(
                "direct".into(),
            ))
            .await
            .unwrap();
        });
        let client = UpstreamClient::new(identity, "token".into(), None).unwrap();
        let (mut socket, _) = client
            .connect_websocket_to(&format!("ws://{addr}/live"), http::HeaderMap::new())
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_text().unwrap(),
            "direct"
        );
        server.await.unwrap();
    }
}
