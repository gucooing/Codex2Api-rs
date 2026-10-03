mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn tags_routing_and_rpm_have_authenticated_admin_operations() {
    let f = Fixture::new().await;
    let consumer = f.consumer("pool-admin").await;
    let id = consumer["id"].as_str().unwrap();
    let tag = f
        .request(
            "POST",
            "/admin/api/supplier-tags",
            json!({"name":"Main","provider_id":"chatgpt"}),
        )
        .await;
    assert_eq!(tag.status(), StatusCode::OK);
    let tags = body(tag).await;
    let tag_id = tags["items"][0]["id"].as_str().unwrap();
    assert_eq!(
        f.request(
            "PUT",
            &format!("/admin/api/consumers/{id}/routing"),
            json!({"tag_id":tag_id,"supplier_id":null,"revision":null})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.request(
            "DELETE",
            &format!("/admin/api/supplier-tags/{tag_id}"),
            json!(null)
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let path = format!("/admin/api/consumers/{id}/rate-limit");
    assert_eq!(f.get(&path).await["effective_rpm"], 20);
    assert_eq!(
        body(f.request("PUT", &path, json!({"rpm":0})).await).await["effective_rpm"],
        0
    );
    assert_eq!(
        body(f.request("PUT", &path, json!({"rpm":null})).await).await["effective_rpm"],
        20
    );
    assert_eq!(
        f.with_auth("PUT", &path, json!({"rpm":0}), None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        f.request("PUT", &path, json!({"rpm":-1}))
            .await
            .status()
            .is_client_error()
    );
    assert_eq!(
        f.request(
            "PUT",
            &format!("/admin/api/consumers/{id}/routing"),
            json!({"tag_id":tag_id,"supplier_id":"outside","revision":1})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
}
