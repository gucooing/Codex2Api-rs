mod common;
use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn search_price_is_admin_only_revisioned_and_never_defaulted() {
    let f = Fixture::new().await;
    let path = "/admin/api/billing/chatgpt/search";
    assert_eq!(f.get(path).await, json!({"price":null,"revision":null}));
    assert_eq!(
        f.with_auth("GET", path, json!(null), None, None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.with_auth(
            "PUT",
            path,
            json!({"price":"0.005","revision":null}),
            Some(&f.cookie),
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let saved = f
        .request("PUT", path, json!({"price":"0.005","revision":null}))
        .await;
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(body(saved).await, json!({"price":"0.005","revision":1}));
    assert_eq!(
        f.request("PUT", path, json!({"price":"0.007","revision":null}))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    for price in ["-1", "NaN", "0.0000000001"] {
        assert_eq!(
            f.request("PUT", path, json!({"price":price,"revision":1}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(f.get(path).await["price"], "0.005");
    assert_eq!(
        f.request("PUT", path, json!({"price":null,"revision":1}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(f.get(path).await, json!({"price":null,"revision":2}));
    assert!(
        f.storage
            .model_configs("chatgpt")
            .await
            .unwrap()
            .iter()
            .all(|m| m.model != "search")
    );
}
