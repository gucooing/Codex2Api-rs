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
    pub dir: tempfile::TempDir,
    pub storage: Storage,
    pub state: codex2api_admin::AdminState,
    pub app: Router,
    pub cookie: String,
    pub csrf: String,
}
impl Fixture {
    pub async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().join("test.sqlite")).await.unwrap();
        let state = codex2api_admin::AdminState::new(storage.clone()).unwrap();
        let app = codex2api_admin::router(state.clone());
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
        assert_eq!(r.status(), StatusCode::OK, "{path}");
        body(r).await
    }
    pub async fn consumer(&self, id: &str) -> Value {
        let r=self.request("POST","/admin/api/consumers",json!({"username":id,"password":"secret-fixture","provider_id":"chatgpt","name":id,"email":format!("{id}@example.test"),"plan_id":"plus","subscription_expires_at":null,"enabled":true})).await;
        assert_eq!(r.status(), StatusCode::OK);
        body(r).await
    }
}
pub async fn body(response: Response) -> Value {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("non-JSON {status}: {}", String::from_utf8_lossy(&bytes)))
}
pub fn model_input(model: &str) -> Value {
    json!({"provider_id":"chatgpt","model":model,"kind":"text","enabled":true,"revision":null,"token_prices":[{"tier":"standard","min_input_tokens":0,"input_rate":"2.5","cached_rate":"0.5","cache_write_rate":"1","output_rate":"10"}],"image_prices":[]})
}
pub fn plan_input() -> Value {
    json!({"provider_id":"chatgpt","name":"Integration Plan","model_access":"all","models":[],"free_model_access":"none","free_models":[],"free_access_enabled":false,"primary_cost_limit_usd":"2","weekly_cost_limit_usd":"10","free_primary_cost_limit_usd":"0","free_weekly_cost_limit_usd":"0","enabled":true,"revision":null})
}
