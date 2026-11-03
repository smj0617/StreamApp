//! 경계 B: MediaMTX 연동 HTTP. 앱이 합치는 axum Router에 요청을 직접 넣고 응답만 확인한다.

use api::dto::MyChannelSettings;
use api::mediamtx;
use api::service::{db, AppService};
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
    Router,
};
use tower::ServiceExt; // oneshot

async fn service() -> AppService {
    AppService::new(db::connect_memory().await.expect("메모리 DB 연결"))
}

/// 기본 사용자를 넣고 첫 사용자의 채널 설정을 돌려준다.
async fn seeded_channel(svc: &AppService) -> MyChannelSettings {
    svc.seed_default_users().await.unwrap();
    let user = svc.list_users().await.unwrap().remove(0);
    svc.my_channel_settings(user.id).await.unwrap().unwrap()
}

/// MediaMTX v1.21.3이 `authMethod: http`로 보내는 인증 요청과 같은 모양의 본문.
fn auth_request(action: &str, path: &str, query: &str) -> Request<Body> {
    let body = serde_json::json!({
        "ip": "127.0.0.1",
        "user": "",
        "password": "",
        "token": "",
        "action": action,
        "path": path,
        "protocol": "rtmp",
        "id": null,
        "query": query,
        "userAgent": "Lavf/62.3.100"
    });
    Request::post("/internal/mediamtx/auth")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    app.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn publish_with_right_stream_key_is_allowed() {
    let svc = service().await;
    let channel = seeded_channel(&svc).await;
    let app = mediamtx::router(svc);

    let res = send(
        &app,
        auth_request(
            "publish",
            &channel.channel_id,
            &format!("key={}", channel.stream_key),
        ),
    )
    .await;

    assert!(res.status().is_success(), "status: {}", res.status());
    // MediaMTX는 비어 있지 않은 2xx 본문을 {"expireAt": ..}로 읽으려 하고, 못 읽으면 거절한다.
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(body.is_empty(), "본문: {body:?}");
}

#[tokio::test]
async fn publish_with_wrong_stream_key_is_unauthorized() {
    let svc = service().await;
    let channel = seeded_channel(&svc).await;
    let app = mediamtx::router(svc);

    let res = send(
        &app,
        auth_request(
            "publish",
            &channel.channel_id,
            "key=wrongStreamKey000000000000000000",
        ),
    )
    .await;

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn publish_without_stream_key_is_unauthorized() {
    let svc = service().await;
    let channel = seeded_channel(&svc).await;
    let app = mediamtx::router(svc);

    let res = send(&app, auth_request("publish", &channel.channel_id, "")).await;

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn publish_to_unknown_channel_is_unauthorized() {
    let svc = service().await;
    let channel = seeded_channel(&svc).await;
    let app = mediamtx::router(svc);

    let res = send(
        &app,
        auth_request(
            "publish",
            "nosuchchanne",
            &format!("key={}", channel.stream_key),
        ),
    )
    .await;

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn stream_key_is_found_among_other_query_parameters() {
    let svc = service().await;
    let channel = seeded_channel(&svc).await;
    let app = mediamtx::router(svc);

    let res = send(
        &app,
        auth_request(
            "publish",
            &channel.channel_id,
            &format!("foo=1&key={}&bar=2", channel.stream_key),
        ),
    )
    .await;

    assert!(res.status().is_success(), "status: {}", res.status());
}

#[tokio::test]
async fn read_is_allowed_without_stream_key() {
    let svc = service().await;
    let channel = seeded_channel(&svc).await;
    let app = mediamtx::router(svc);

    let res = send(&app, auth_request("read", &channel.channel_id, "")).await;

    assert!(res.status().is_success(), "status: {}", res.status());
}

#[tokio::test]
async fn other_actions_are_allowed() {
    let svc = service().await;
    let app = mediamtx::router(svc);

    for action in ["playback", "api", "metrics", "pprof"] {
        let res = send(&app, auth_request(action, "", "")).await;

        assert!(
            res.status().is_success(),
            "{action} status: {}",
            res.status()
        );
    }
}
