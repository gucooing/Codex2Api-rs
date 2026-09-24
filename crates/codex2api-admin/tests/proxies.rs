mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;
#[tokio::test]
async fn proxy_crud_hides_credentials_and_preserves_password_on_edit() {
    let f = Fixture::new().await;
    let mut value = json!({"name":"Outbound","protocol":"socks5h","host":"::1","port":1080,"username":"user@name","password":"secret-proxy"});
    let r = f.request("POST", "/admin/api/proxies", value.clone()).await;
    assert_eq!(r.status(), StatusCode::OK);
    let p = body(r).await;
    let id = p["id"].as_str().unwrap();
    let path = format!("/admin/api/proxies/{id}");
    assert_eq!(p["has_password"], true);
    assert!(!p.to_string().contains("secret-proxy"));
    assert!(p.get("url").is_none());
    let list = f.get("/admin/api/proxies").await;
    assert!(!list.to_string().contains("secret-proxy"));
    value["name"] = "Edited".into();
    value["password"] = serde_json::Value::Null;
    assert_eq!(
        f.request("PUT", &path, value.clone()).await.status(),
        StatusCode::OK
    );
    let stored = f.storage.require_outbound_proxy(id).await.unwrap();
    assert!(stored.url.contains("secret-proxy"));
    value["host"] = "host:8080".into();
    assert_eq!(
        f.request("PUT", &path, value.clone()).await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request("POST", &format!("{path}/check/unknown"), json!({}))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.request("DELETE", &path, json!({"confirm_unbind":false}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.request("DELETE", &path, json!({"confirm_unbind":false}))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
#[tokio::test]
async fn editing_proxy_refreshes_only_bound_supplier_transport() {
    let f = Fixture::new().await;
    let p = f
        .storage
        .create_outbound_proxy("bound", "http://127.0.0.1:1080")
        .await
        .unwrap();
    let a = f.state.accounts.create_pending().await.unwrap().account;
    let b = f.state.accounts.create_pending().await.unwrap().account;
    f.storage
        .set_account_proxy(&a.id, Some(&p.id))
        .await
        .unwrap();
    let a_http = f.state.auth.account_http(&a.id).await.unwrap();
    let b_http = f.state.auth.account_http(&b.id).await.unwrap();
    assert_eq!(f.request("PUT",&format!("/admin/api/proxies/{}",p.id),json!({"name":"changed","protocol":"http","host":"127.0.0.1","port":1081,"username":"","password":null})).await.status(),StatusCode::OK);
    let refreshed = f.state.auth.account_http(&a.id).await.unwrap();
    assert!(!std::sync::Arc::ptr_eq(&a_http, &refreshed));
    assert!(refreshed.proxy_url().unwrap().contains("1081"));
    assert!(std::sync::Arc::ptr_eq(
        &b_http,
        &f.state.auth.account_http(&b.id).await.unwrap()
    ));
}
