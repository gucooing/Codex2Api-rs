mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn tag_updates_replace_explicit_selection_and_require_authenticated_writes() {
    let f = Fixture::new().await;
    let account = f.state.accounts.create_pending().await.unwrap().account;
    for tag in ["old", "new"] {
        f.storage
            .save_supplier_tag(tag, "chatgpt", tag)
            .await
            .unwrap();
    }
    f.storage
        .edit_supplier_tags(&[account.id.clone()], &["old".into()], false)
        .await
        .unwrap();
    let input = json!({"account_ids":[account.id],"tag_ids":["new"]});
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/suppliers/tags",
            input.clone(),
            None,
            None
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.with_auth(
            "POST",
            "/admin/api/suppliers/tags",
            input.clone(),
            Some(&f.cookie),
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.request("POST", "/admin/api/suppliers/tags", input)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        f.storage.supplier_tag_ids(&account.id).await.unwrap(),
        vec!["new"]
    );
    assert_eq!(
        f.request(
            "POST",
            "/admin/api/suppliers/tags",
            json!({"account_ids":[account.id]})
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        f.storage.supplier_tag_ids(&account.id).await.unwrap(),
        vec!["new"]
    );
    assert_eq!(
        f.request(
            "POST",
            "/admin/api/suppliers/tags",
            json!({"account_ids":[account.id],"tag_ids":[]})
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert!(
        f.storage
            .supplier_tag_ids(&account.id)
            .await
            .unwrap()
            .is_empty()
    );
}

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
