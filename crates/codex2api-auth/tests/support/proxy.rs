//! Local test proxies only. The private key belongs to a disposable fixture certificate.
use std::{io, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    },
};

pub fn server_tls() -> Arc<rustls::ServerConfig> {
    codex2api_auth::transport::ensure_tls_provider();
    let cert =
        CertificateDer::from_pem_slice(include_bytes!("../fixtures/proxy-server.pem")).unwrap();
    let key =
        PrivateKeyDer::from_pem_slice(include_bytes!("../fixtures/proxy-server-key.pem")).unwrap();
    Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
    )
}

pub async fn read_headers<S: AsyncRead + Unpin>(socket: &mut S) -> io::Result<String> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() > 16384 {
            return Err(io::Error::other("headers too large"));
        }
        bytes.push(socket.read_u8().await?);
    }
    String::from_utf8(bytes).map_err(io::Error::other)
}

pub async fn socks_tunnel<S: AsyncRead + AsyncWrite + Unpin>(socket: &mut S) -> String {
    assert_eq!(socket.read_u8().await.unwrap(), 5);
    let count = socket.read_u8().await.unwrap();
    let mut methods = vec![0; count as usize];
    socket.read_exact(&mut methods).await.unwrap();
    assert!(methods.contains(&2));
    socket.write_all(&[5, 2]).await.unwrap();
    assert_eq!(socket.read_u8().await.unwrap(), 1);
    let mut user = vec![0; socket.read_u8().await.unwrap() as usize];
    socket.read_exact(&mut user).await.unwrap();
    let mut password = vec![0; socket.read_u8().await.unwrap() as usize];
    socket.read_exact(&mut password).await.unwrap();
    assert_eq!(user, b"user");
    assert_eq!(password, b"pass");
    socket.write_all(&[1, 0]).await.unwrap();
    assert_eq!(socket.read_u8().await.unwrap(), 5);
    assert_eq!(socket.read_u8().await.unwrap(), 1);
    assert_eq!(socket.read_u8().await.unwrap(), 0);
    let host = match socket.read_u8().await.unwrap() {
        1 => {
            let mut ip = [0; 4];
            socket.read_exact(&mut ip).await.unwrap();
            std::net::Ipv4Addr::from(ip).to_string()
        }
        4 => {
            let mut ip = [0; 16];
            socket.read_exact(&mut ip).await.unwrap();
            std::net::Ipv6Addr::from(ip).to_string()
        }
        3 => {
            let mut host = vec![0; socket.read_u8().await.unwrap() as usize];
            socket.read_exact(&mut host).await.unwrap();
            String::from_utf8(host).unwrap()
        }
        kind => panic!("unexpected address type {kind}"),
    };
    let port = socket.read_u16().await.unwrap();
    socket
        .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
        .await
        .unwrap();
    format!("{host}:{port}")
}

pub async fn http_tunnel<S: AsyncRead + AsyncWrite + Unpin>(socket: &mut S) -> String {
    let headers = read_headers(socket).await.unwrap();
    assert!(headers.starts_with("CONNECT "));
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("proxy-authorization: basic dxnlcjpwyxnz")
    );
    let target = headers.split_whitespace().nth(1).unwrap().to_string();
    socket
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .unwrap();
    target
}

pub async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    (listener, addr)
}

pub fn acceptor() -> TlsAcceptor {
    TlsAcceptor::from(server_tls())
}
