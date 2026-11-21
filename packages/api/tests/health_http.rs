//! 경계 B: 상태 확인 경로 `GET /healthz`. 앱이 합치는 axum Router에 요청을 직접 넣고 응답만 확인한다.
//! MediaMTX Control API는 가짜 서버를 띄우거나 아무도 듣지 않는 주소를 준다.

mod common;

use api::health;
use api::service::{db, AppService};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Json, Router,
};
use common::service;
use tower::ServiceExt; // oneshot

/// 아무도 듣지 않는 주소(포트를 잡았다가 바로 놓는다).
async fn closed_address() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    address
}

/// 송출 경로 목록에 빈 목록으로 답하는 가짜 MediaMTX Control API.
async fn fake_mediamtx_api() -> String {
    let app = Router::new().route(
        "/v3/paths/list",
        get(|| async { Json(serde_json::json!({ "itemCount": 0, "pageCount": 0, "items": [] })) }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    address
}

/// `GET /healthz`를 보내고 상태 코드와 JSON 본문을 돌려준다.
async fn get_health(app: Router) -> (StatusCode, serde_json::Value) {
    let res = app
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let content_type = res.headers()["content-type"].to_str().unwrap().to_owned();
    assert!(
        content_type.starts_with("application/json"),
        "{content_type}"
    );
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn healthy_database_and_media_server_is_ok() {
    let app = health::router(service().await, &fake_mediamtx_api().await);

    let (status, body) = get_health(app).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
    assert_eq!(body["mediamtx"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn media_server_down_is_reported_but_still_ok() {
    let app = health::router(service().await, &closed_address().await);

    let (status, body) = get_health(app).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
    assert_eq!(body["mediamtx"], "unreachable");
}

#[tokio::test]
async fn database_that_does_not_answer_is_service_unavailable() {
    let pool = db::connect_memory().await.expect("메모리 DB 연결");
    let svc = AppService::new(pool.clone());
    pool.close().await;
    let app = health::router(svc, &fake_mediamtx_api().await);

    let (status, body) = get_health(app).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "error");
    assert_eq!(body["database"], "error");
    assert_eq!(body["mediamtx"], "ok");
}
