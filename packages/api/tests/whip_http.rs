//! 경계 B: 휴대폰 방송 WHIP 중계. 가짜 MediaMTX WebRTC 서버를 띄우고, 앱이 합치는 axum Router에 요청을
//! 직접 넣어 응답과 MediaMTX가 받은 요청을 확인한다.

mod common;

use std::sync::{Arc, Mutex};

use api::service::AppService;
use api::whip;
use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{header, Method, Request, Response, StatusCode},
    response::IntoResponse,
    Router,
};
use common::{service, user};
use tower::ServiceExt; // oneshot

const OFFER: &str = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\n";
const ANSWER: &str = "v=0\r\no=- 3 4 IN IP4 127.0.0.1\r\ns=answer\r\n";

/// 가짜 MediaMTX가 받은 요청: (메서드, 경로와 쿼리, 본문).
type Seen = Arc<Mutex<Vec<(Method, String, String)>>>;

/// 가짜 MediaMTX가 돌려줄 답.
#[derive(Clone, Copy)]
enum Reply {
    /// 201, SDP 응답, `Location: /<경로>/whip/<세션>`
    Accept,
    /// 400과 MediaMTX 오류 본문(예: 이미 송출 중)
    Reject,
}

/// 가짜 MediaMTX WebRTC 서버를 띄우고 (주소, 받은 요청)을 돌려준다.
async fn fake_mediamtx(reply: Reply) -> (String, Seen) {
    let seen: Seen = Arc::default();
    let app = Router::new()
        .fallback(
            |State((seen, reply)): State<(Seen, Reply)>,
             method: Method,
             OriginalUri(uri): OriginalUri,
             body: String| async move {
                let path = uri.to_string();
                seen.lock()
                    .unwrap()
                    .push((method.clone(), path.clone(), body));
                match (method, reply) {
                    (Method::POST, Reply::Accept) => {
                        let channel = path.trim_start_matches('/').split('/').next().unwrap();
                        (
                            StatusCode::CREATED,
                            [
                                (header::CONTENT_TYPE, "application/sdp".to_string()),
                                (
                                    header::LOCATION,
                                    format!("/{channel}/whip/2f6e1c3a-0000-4b9e-9c1d-1234567890ab?key=secret"),
                                ),
                            ],
                            ANSWER,
                        )
                            .into_response()
                    }
                    (Method::POST, Reply::Reject) => (
                        StatusCode::BAD_REQUEST,
                        r#"{"status":"error","error":"someone is already publishing"}"#,
                    )
                        .into_response(),
                    _ => StatusCode::OK.into_response(),
                }
            },
        )
        .with_state((seen.clone(), reply));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (address, seen)
}

/// 사용자 "철수"를 만들고 (사용자 ID, 채널 ID, 송출 키)를 돌려준다.
async fn chulsoo(svc: &AppService) -> (i64, String, String) {
    let (user_id, channel_id) = user(svc, "철수").await;
    let settings = svc.my_channel_settings(user_id).await.unwrap().unwrap();
    (user_id, channel_id, settings.stream_key)
}

fn offer_request(user_id: Option<i64>, body: &str) -> Request<Body> {
    let mut request =
        Request::post("/api/broadcast/whip").header(header::CONTENT_TYPE, "application/sdp");
    if let Some(user_id) = user_id {
        request = request.header(header::COOKIE, format!("streamapp_user={user_id}"));
    }
    request.body(Body::from(body.to_string())).unwrap()
}

async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    app.clone().oneshot(request).await.unwrap()
}

async fn text(res: Response<Body>) -> String {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn message(res: Response<Body>) -> String {
    let body: serde_json::Value = serde_json::from_str(&text(res).await).unwrap();
    body["message"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn offer_is_forwarded_with_channel_and_stream_key_and_answer_comes_back() {
    let svc = service().await;
    let (user_id, channel_id, stream_key) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(user_id), OFFER)).await;

    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "application/sdp");
    assert_eq!(
        res.headers()[header::LOCATION],
        "/api/broadcast/whip/2f6e1c3a-0000-4b9e-9c1d-1234567890ab"
    );
    assert_eq!(text(res).await, ANSWER);
    assert_eq!(
        *seen.lock().unwrap(),
        [(
            Method::POST,
            format!("/{channel_id}/whip?key={stream_key}"),
            OFFER.to_string()
        )]
    );
}

#[tokio::test]
async fn stop_deletes_the_mediamtx_session_of_my_channel() {
    let svc = service().await;
    let (user_id, channel_id, stream_key) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(
        &app,
        Request::delete("/api/broadcast/whip/2f6e1c3a-0000-4b9e-9c1d-1234567890ab")
            .header(header::COOKIE, format!("streamapp_user={user_id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        *seen.lock().unwrap(),
        [(
            Method::DELETE,
            format!("/{channel_id}/whip/2f6e1c3a-0000-4b9e-9c1d-1234567890ab?key={stream_key}"),
            String::new()
        )]
    );
}

#[tokio::test]
async fn offer_without_selected_user_is_rejected_before_reaching_mediamtx() {
    let svc = service().await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    for user_id in [None, Some(999)] {
        let res = send(&app, offer_request(user_id, OFFER)).await;

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(message(res).await, "사용자를 먼저 골라 주세요");
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn body_that_is_not_sdp_is_rejected() {
    let svc = service().await;
    let (user_id, _, _) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(user_id), "hello")).await;

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mediamtx_refusal_becomes_conflict_with_reason() {
    let svc = service().await;
    let (user_id, _, _) = chulsoo(&svc).await;
    let (mediamtx, _) = fake_mediamtx(Reply::Reject).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(user_id), OFFER)).await;

    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert!(message(res).await.contains("HTTP 400"));
}

#[tokio::test]
async fn unreachable_mediamtx_becomes_bad_gateway() {
    let svc = service().await;
    let (user_id, _, _) = chulsoo(&svc).await;
    // 띄웠다가 바로 닫은 포트: 연결이 거절된다.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let app = whip::router(svc, &address);

    let res = send(&app, offer_request(Some(user_id), OFFER)).await;

    assert_eq!(res.status(), StatusCode::BAD_GATEWAY);
    assert!(message(res).await.contains("MediaMTX"));
}

#[tokio::test]
async fn session_with_path_characters_is_rejected() {
    let svc = service().await;
    let (user_id, _, _) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(
        &app,
        Request::delete("/api/broadcast/whip/..%2Fother")
            .header(header::COOKIE, format!("streamapp_user={user_id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn offer_for_channel_already_live_is_rejected_before_reaching_mediamtx() {
    let svc = service().await;
    let (user_id, channel_id, _) = chulsoo(&svc).await;
    // 다른 곳(OBS, 다른 휴대폰)에서 이미 송출 중이다.
    svc.publish_started(&channel_id).await.unwrap();
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(user_id), OFFER)).await;

    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert!(message(res).await.contains("이미 방송 중"));
    assert!(seen.lock().unwrap().is_empty());
}
