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
use common::{cookie, service, user};
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

/// 사용자를 회원가입·로그인시키고 (세션 토큰, 채널 ID, 송출 키)를 돌려준다.
async fn streamer(svc: &AppService, nickname: &str) -> (String, String, String) {
    let (user_id, channel_id) = user(svc, nickname).await;
    let settings = svc.my_channel_settings(user_id).await.unwrap().unwrap();
    let session = svc.start_session(user_id).await.unwrap();
    (session, channel_id, settings.stream_key)
}

/// 로그인한 "철수"의 (세션 토큰, 채널 ID, 송출 키).
async fn chulsoo(svc: &AppService) -> (String, String, String) {
    streamer(svc, "철수").await
}

fn offer_request(session: Option<&str>, body: &str) -> Request<Body> {
    let mut request =
        Request::post("/api/broadcast/whip").header(header::CONTENT_TYPE, "application/sdp");
    if let Some(session) = session {
        request = request.header(header::COOKIE, cookie(session));
    }
    request.body(Body::from(body.to_string())).unwrap()
}

/// 가짜 MediaMTX가 주는 세션의 앱 쪽 종료 주소.
const SESSION_PATH: &str = "/api/broadcast/whip/2f6e1c3a-0000-4b9e-9c1d-1234567890ab";

fn stop_request(session: Option<&str>, path: &str) -> Request<Body> {
    let mut request = Request::delete(path);
    if let Some(session) = session {
        request = request.header(header::COOKIE, cookie(session));
    }
    request.body(Body::empty()).unwrap()
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
    let (session, channel_id, stream_key) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(session.as_str()), OFFER)).await;

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
    let (session, channel_id, stream_key) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let started = send(&app, offer_request(Some(session.as_str()), OFFER)).await;
    assert_eq!(started.status(), StatusCode::CREATED);

    let res = send(&app, stop_request(Some(session.as_str()), SESSION_PATH)).await;

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        seen.lock().unwrap()[1..],
        [(
            Method::DELETE,
            format!("/{channel_id}/whip/2f6e1c3a-0000-4b9e-9c1d-1234567890ab?key={stream_key}"),
            String::new()
        )]
    );
}

#[tokio::test]
async fn offer_without_login_is_rejected_before_reaching_mediamtx() {
    let svc = service().await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    for session in [None, Some("made-up-token")] {
        let res = send(&app, offer_request(session, OFFER)).await;

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(message(res).await, "로그인이 필요합니다");
    }
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn stop_by_another_user_is_forbidden_and_broadcast_keeps_going() {
    let svc = service().await;
    let (owner, _, _) = chulsoo(&svc).await;
    let (other, _, _) = streamer(&svc, "영희").await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);
    send(&app, offer_request(Some(owner.as_str()), OFFER)).await;

    let res = send(&app, stop_request(Some(other.as_str()), SESSION_PATH)).await;

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(message(res).await, "방송 주인만 할 수 있습니다");
    // MediaMTX에는 송출 시작만 갔다.
    assert_eq!(seen.lock().unwrap().len(), 1);

    // 주인은 그대로 끝낼 수 있다.
    let res = send(&app, stop_request(Some(owner.as_str()), SESSION_PATH)).await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn stop_without_login_is_rejected_with_401() {
    let svc = service().await;
    let (owner, _, _) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);
    send(&app, offer_request(Some(owner.as_str()), OFFER)).await;

    let res = send(&app, stop_request(None, SESSION_PATH)).await;

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn stop_of_unknown_or_already_stopped_session_is_not_found() {
    let svc = service().await;
    let (session, _, _) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, stop_request(Some(session.as_str()), SESSION_PATH)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(seen.lock().unwrap().is_empty());

    send(&app, offer_request(Some(session.as_str()), OFFER)).await;
    let first = send(&app, stop_request(Some(session.as_str()), SESSION_PATH)).await;
    let second = send(&app, stop_request(Some(session.as_str()), SESSION_PATH)).await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn body_that_is_not_sdp_is_rejected() {
    let svc = service().await;
    let (session, _, _) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(session.as_str()), "hello")).await;

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn mediamtx_refusal_becomes_conflict_with_reason() {
    let svc = service().await;
    let (session, _, _) = chulsoo(&svc).await;
    let (mediamtx, _) = fake_mediamtx(Reply::Reject).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(session.as_str()), OFFER)).await;

    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert!(message(res).await.contains("HTTP 400"));
}

#[tokio::test]
async fn unreachable_mediamtx_becomes_bad_gateway() {
    let svc = service().await;
    let (session, _, _) = chulsoo(&svc).await;
    // 띄웠다가 바로 닫은 포트: 연결이 거절된다.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let app = whip::router(svc, &address);

    let res = send(&app, offer_request(Some(session.as_str()), OFFER)).await;

    assert_eq!(res.status(), StatusCode::BAD_GATEWAY);
    assert!(message(res).await.contains("MediaMTX"));
}

#[tokio::test]
async fn session_with_path_characters_is_rejected() {
    let svc = service().await;
    let (session, _, _) = chulsoo(&svc).await;
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(
        &app,
        stop_request(Some(session.as_str()), "/api/broadcast/whip/..%2Fother"),
    )
    .await;

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn offer_for_channel_already_live_is_rejected_before_reaching_mediamtx() {
    let svc = service().await;
    let (session, channel_id, _) = chulsoo(&svc).await;
    // 다른 곳(OBS, 다른 휴대폰)에서 이미 송출 중이다.
    svc.publish_started(&channel_id).await.unwrap();
    let (mediamtx, seen) = fake_mediamtx(Reply::Accept).await;
    let app = whip::router(svc, &mediamtx);

    let res = send(&app, offer_request(Some(session.as_str()), OFFER)).await;

    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert!(message(res).await.contains("이미 방송 중"));
    assert!(seen.lock().unwrap().is_empty());
}
