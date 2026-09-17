use codex2api_storage::{OutboundProxy, ProxyConnectionCheck, ProxyQualityCheck};
use serde::Deserialize;
use std::time::{Duration, Instant};

const GEO_URL: &str = "https://ipwho.is/?fields=success,message,ip,country,country_code,region,city,timezone.id&lang=zh-CN";
const MAX_BODY: usize = 256 * 1024;

fn client(proxy: &OutboundProxy) -> Result<reqwest::Client, String> {
    let url = codex2api_storage::parse_proxy_url(&proxy.url).map_err(|_| "代理地址无效")?;
    codex2api_auth::transport::http_builder()
        .map_err(|_| "无法初始化检测客户端")?
        .proxy(reqwest::Proxy::all(url).map_err(|_| "代理地址无效")?)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化检测客户端".to_string())
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "请求超时"
    } else if error.is_connect() {
        "代理连接或 TLS 握手失败"
    } else {
        "请求失败或连接中断"
    }
    .to_string()
}

async fn read_body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if body.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err("响应数据过大".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn elapsed(start: Instant) -> i64 {
    start.elapsed().as_millis().min(i64::MAX as u128) as i64
}

#[derive(Deserialize)]
struct GeoResponse {
    success: bool,
    ip: Option<String>,
    country_code: Option<String>,
    country: Option<String>,
    region: Option<String>,
    city: Option<String>,
    timezone: Option<GeoTimezone>,
}

#[derive(Deserialize)]
struct GeoTimezone {
    id: Option<String>,
}

pub(crate) async fn connection(proxy: &OutboundProxy) -> ProxyConnectionCheck {
    connection_to(proxy, GEO_URL).await
}

async fn connection_to(proxy: &OutboundProxy, endpoint: &str) -> ProxyConnectionCheck {
    let client = match client(proxy) {
        Ok(client) => client,
        Err(error) => {
            return ProxyConnectionCheck {
                error: Some(error),
                ..Default::default()
            };
        }
    };
    let start = Instant::now();
    let result = async {
        let response = client.get(endpoint).send().await.map_err(network_error)?;
        if !response.status().is_success() {
            return Err(format!(
                "地区查询服务返回 HTTP {}",
                response.status().as_u16()
            ));
        }
        let body = read_body(response).await?;
        let geo: GeoResponse = serde_json::from_slice(&body).map_err(|_| "地区查询响应无效")?;
        if !geo.success {
            return Err("地区查询失败".into());
        }
        if geo
            .ip
            .as_deref()
            .and_then(|ip| ip.parse::<std::net::IpAddr>().ok())
            .is_none()
            || geo
                .country
                .as_deref()
                .is_none_or(|country| country.trim().is_empty())
        {
            return Err("地区查询未返回有效的出口 IP 和国家".into());
        }
        Ok(geo)
    }
    .await;
    let latency_ms = elapsed(start);
    match result {
        Ok(geo) => ProxyConnectionCheck {
            ok: true,
            latency_ms,
            error: None,
            exit_ip: geo.ip,
            country_code: geo.country_code,
            country: geo.country,
            region: geo.region,
            city: geo.city,
            timezone: geo
                .timezone
                .and_then(|zone| zone.id)
                .filter(|zone| zone.parse::<chrono_tz::Tz>().is_ok()),
        },
        Err(error) => ProxyConnectionCheck {
            latency_ms,
            error: Some(error),
            ..Default::default()
        },
    }
}

pub(crate) async fn quality(proxy: &OutboundProxy) -> ProxyQualityCheck {
    quality_to(proxy, &codex2api_upstream::Endpoint::Models.url()).await
}

async fn quality_to(proxy: &OutboundProxy, endpoint: &str) -> ProxyQualityCheck {
    let client = match client(proxy) {
        Ok(client) => client,
        Err(error) => {
            return ProxyQualityCheck {
                error: Some(error),
                ..Default::default()
            };
        }
    };
    let start = Instant::now();
    let mut http_status = None;
    let result = async {
        let response = client.get(endpoint).send().await.map_err(network_error)?;
        let status = response.status();
        http_status = Some(i64::from(status.as_u16()));
        if response
            .headers()
            .get("cf-mitigated")
            .is_some_and(|value| value == "challenge")
        {
            return Err("ChatGPT 验证挑战拦截".into());
        }
        let body = read_body(response).await?;
        let json = serde_json::from_slice::<serde_json::Value>(&body).ok();
        if json.as_ref().is_some_and(|value| {
            let text = value.to_string().to_ascii_lowercase();
            text.contains("unsupported_country_region_territory")
                || text.contains("country, region, or territory not supported")
        }) {
            return Err("ChatGPT 不支持该代理的国家或地区".into());
        }
        if status.is_success()
            && json
                .as_ref()
                .is_some_and(|v| v.get("models").is_some_and(|models| models.is_array()))
        {
            return Ok(());
        }
        // Official unauthenticated JSON is expected for this proxy availability check.
        if status == reqwest::StatusCode::UNAUTHORIZED
            && json.as_ref().is_some_and(serde_json::Value::is_object)
        {
            return Ok(());
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err("ChatGPT 拒绝访问（HTTP 403）".into());
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err("ChatGPT 限流（HTTP 429）".into());
        }
        Err(format!("ChatGPT 响应异常（HTTP {}）", status.as_u16()))
    }
    .await;
    ProxyQualityCheck {
        ok: result.is_ok(),
        latency_ms: elapsed(start),
        http_status,
        error: result.err(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, Uri},
        response::Response,
    };
    use codex2api_storage::Storage;

    #[tokio::test]
    async fn probes_use_selected_proxy_measure_requests_and_persist_independent_results() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().fallback(|uri: Uri, headers: HeaderMap| async move {
            assert_eq!(uri.host(), Some("probe.invalid"));
            assert!(!headers.contains_key("authorization"));
            assert!(!headers.contains_key("cookie"));
            tokio::time::sleep(Duration::from_millis(30)).await;
            let (status, body, challenge) = match uri.path() {
                "/geo" => (200, r#"{"success":true,"ip":"203.0.113.1","country_code":"US","country":"美国","region":"加利福尼亚州","city":"洛杉矶","timezone":{"id":"America/Los_Angeles"}}"#, false),
                "/invalid-zone" => (200, r#"{"success":true,"ip":"203.0.113.1","country":"美国","timezone":{"id":"Invalid/Zone"}}"#, false),
                "/unauthorized" => (401, r#"{"detail":"Unauthorized"}"#, false),
                "/login-required" => (401, r#"{"error":{"message":"Authentication required"}}"#, false),
                "/blocked-401" => (401, r#"{"error":{"code":"unsupported_country_region_territory"}}"#, false),
                "/models" => (200, r#"{"models":[]}"#, false),
                "/blocked" => (403, r#"{"error":{"code":"unsupported_country_region_territory"}}"#, false),
                "/limited" => (429, "rate limited", false),
                "/challenge" => (200, "<html>challenge</html>", true),
                "/html" => (200, "<html>not the API</html>", false),
                "/bad-401" => (401, "<html>proxy error</html>", false),
                _ => (200, r#"{"success":false}"#, false),
            };
            let mut response = Response::builder().status(status).header("connection", "close");
            if challenge { response = response.header("cf-mitigated", "challenge"); }
            response.body(Body::from(body)).unwrap()
        });
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checks.sqlite");
        let storage = Storage::open(&path).await.unwrap();
        let proxy = storage
            .create_outbound_proxy("test", &format!("http://{address}"))
            .await
            .unwrap();
        let connected = connection_to(&proxy, "http://probe.invalid/geo").await;
        assert!(connected.ok, "{:?}", connected.error);
        assert!(connected.latency_ms >= 30);
        let saved = storage
            .save_proxy_connection_check(&proxy, &connected)
            .await
            .unwrap();
        assert_eq!(saved.exit_ip.as_deref(), Some("203.0.113.1"));
        assert_eq!(saved.country.as_deref(), Some("美国"));
        assert_eq!(saved.city.as_deref(), Some("洛杉矶"));
        assert_eq!(saved.timezone.as_deref(), Some("America/Los_Angeles"));
        for (path, ok, status) in [
            ("unauthorized", true, 401),
            ("login-required", true, 401),
            ("blocked-401", false, 401),
            ("models", true, 200),
            ("blocked", false, 403),
            ("limited", false, 429),
            ("challenge", false, 200),
            ("html", false, 200),
            ("bad-401", false, 401),
        ] {
            let checked = quality_to(&proxy, &format!("http://probe.invalid/{path}")).await;
            assert_eq!(checked.ok, ok, "{path}: {:?}", checked.error);
            assert_eq!(checked.http_status, Some(status));
            if path.starts_with("blocked") {
                assert_eq!(
                    checked.error.as_deref(),
                    Some("ChatGPT 不支持该代理的国家或地区")
                );
            }
            assert!(checked.latency_ms >= 30);
            storage
                .save_proxy_quality_check(&proxy, &checked)
                .await
                .unwrap();
        }
        storage.close().await;
        let storage = Storage::open(&path).await.unwrap();
        let saved = storage.require_outbound_proxy(&proxy.id).await.unwrap();
        assert_eq!(saved.country.as_deref(), Some("美国"));
        assert_eq!(saved.connection_ok, Some(true));
        assert_eq!(saved.timezone.as_deref(), Some("America/Los_Angeles"));
        assert_eq!(saved.quality_ok, Some(false));
        assert!(saved.quality_latency_ms.unwrap() >= 30);
        assert!(saved.connection_checked_at.is_some() && saved.quality_checked_at.is_some());
        let bad_geo = connection_to(&proxy, "http://probe.invalid/bad-geo").await;
        assert!(!bad_geo.ok);
        assert!(bad_geo.exit_ip.is_none());
        let invalid_zone = connection_to(&proxy, "http://probe.invalid/invalid-zone").await;
        assert!(invalid_zone.ok);
        assert!(invalid_zone.timezone.is_none());
        server.abort();
        server.await.unwrap_err();
        let failed = connection_to(&proxy, "http://probe.invalid/geo").await;
        assert!(!failed.ok);
        assert!(failed.error.is_some());
        storage.close().await;
    }

    #[tokio::test]
    #[ignore = "requires CODEX2API_TEST_PROXY_URL and external network"]
    async fn live_proxy_checks() {
        let url = std::env::var("CODEX2API_TEST_PROXY_URL").expect("set explicit test proxy");
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("live-check.sqlite"))
            .await
            .unwrap();
        let proxy = storage
            .create_outbound_proxy("live test", &url)
            .await
            .unwrap();
        let connected = connection(&proxy).await;
        assert!(connected.ok, "{:?}", connected.error);
        assert!(
            connected
                .timezone
                .as_deref()
                .is_some_and(|zone| zone.parse::<chrono_tz::Tz>().is_ok())
        );
        let checked = quality(&proxy).await;
        assert!(checked.ok, "{:?}", checked.error);
        println!(
            "connection={}ms country={} region={} city={} timezone={}; ChatGPT={}ms HTTP={}",
            connected.latency_ms,
            connected.country.as_deref().unwrap_or(""),
            connected.region.as_deref().unwrap_or(""),
            connected.city.as_deref().unwrap_or(""),
            connected.timezone.as_deref().unwrap_or(""),
            checked.latency_ms,
            checked.http_status.unwrap_or(0)
        );
        storage
            .save_proxy_connection_check(&proxy, &connected)
            .await
            .unwrap();
        storage
            .save_proxy_quality_check(&proxy, &checked)
            .await
            .unwrap();
        storage.close().await;
    }
}
