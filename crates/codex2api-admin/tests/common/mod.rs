#![allow(dead_code)]
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use codex2api_storage::Storage;
use serde_json::{Value, json};
use tower::ServiceExt;
pub struct Fixture {
    pub dir: codex2api_storage::test_support::TestDirectory,
    pub storage: Storage,
    pub state: codex2api_admin::AdminState,
    pub app: Router,
    pub cookie: String,
    pub csrf: String,
}
impl Fixture {
    pub async fn new() -> Self {
        let dir = codex2api_storage::test_support::tempdir().unwrap();
        let storage = codex2api_storage::test_support::open(dir.path().join("test-pg"))
            .await
            .unwrap();
        let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
        let app = codex2api_admin::api_router(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/admin/api/login")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"username":"admin","password":"admin"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let csrf = body(response).await["csrf_token"]
            .as_str()
            .unwrap()
            .to_owned();
        Self {
            dir,
            storage,
            state,
            app,
            cookie,
            csrf,
        }
    }
    pub async fn request(&self, method: &str, path: &str, value: Value) -> Response {
        self.with_auth(method, path, value, Some(&self.cookie), Some(&self.csrf))
            .await
    }
    pub async fn with_auth(
        &self,
        method: &str,
        path: &str,
        value: Value,
        cookie: Option<&str>,
        csrf: Option<&str>,
    ) -> Response {
        let mut r = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json");
        if let Some(v) = cookie {
            r = r.header("cookie", v);
        }
        if let Some(v) = csrf {
            r = r.header("x-csrf-token", v);
        }
        self.app
            .clone()
            .oneshot(r.body(Body::from(value.to_string())).unwrap())
            .await
            .unwrap()
    }
    pub async fn get(&self, path: &str) -> Value {
        let r = self.request("GET", path, Value::Null).await;
        let status = r.status();
        let value = body(r).await;
        assert_eq!(status, StatusCode::OK, "{path}: {value}");
        value
    }
    pub async fn consumer(&self, id: &str) -> Value {
        let user = body(self.request("POST","/admin/api/users",json!({"username":id,"password":"secret-fixture","name":id,"email":format!("{id}@example.test"),"enabled":true,"revision":null})).await).await;
        let response = self.request("POST","/admin/api/subscriptions",json!({"user_id":user["id"],"plan_id":"plus","expires_at":null,"enabled":true,"revision":null,"reissue":false})).await;
        assert_eq!(response.status(), StatusCode::OK);
        body(response).await
    }
}
pub async fn body(response: Response) -> Value {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("non-JSON {status}: {}", String::from_utf8_lossy(&bytes)))
}
pub fn model_input(model: &str) -> Value {
    json!({"platform":"chatgpt","model":model,"kind":"text","enabled":true,"revision":null,"token_prices":[{"tier":"standard","min_input_tokens":0,"input_rate":"2.5","cached_rate":"0.5","cache_write_rate":"1","output_rate":"10"}],"image_prices":[]})
}
pub fn plan_input() -> Value {
    json!({"rpm_limit":40,"concurrency_limit":10,"sale_price_usd":null,"duration_days":30,"tag_ids":[],"plan_type":"plus","provider_id":"chatgpt","name":"Integration Plan","models":[{"model":"gpt-6-astra","multiplier":null,"concurrency_limit":0,"rpm_limit":0}],"spending_windows":[{"duration_seconds":604800,"cost_limit_usd":"10"},{"duration_seconds":18000,"cost_limit_usd":"2"}],"allow_purchase":true,"revision":null})
}
