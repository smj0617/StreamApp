//! 경계 B: 녹화방송 영상 파일 HTTP. 앱이 합치는 axum Router에 요청을 직접 넣고 응답만 확인한다.

mod common;

use api::recording_files;
use api::service::AppService;
use axum::{
    body::Body,
    http::{header, Request, Response, StatusCode},
    Router,
};
use common::{cookie, service_with_recordings, signed_in};
use tower::ServiceExt; // oneshot

const BOUNDARY: &str = "streamapp-test-boundary";

/// 멀티파트 칸 하나: (칸 이름, 파일 이름(파일 칸이면), 내용).
type Part<'a> = (&'a str, Option<&'a str>, &'a [u8]);

/// 브라우저 `FormData`가 보내는 것과 같은 모양의 올리기 요청. `session`이 있으면 세션 쿠키를 싣는다.
fn upload_request(session: Option<&str>, parts: &[Part]) -> Request<Body> {
    let mut body = Vec::new();
    for (name, file_name, content) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match file_name {
            Some(file_name) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{file_name}\"\r\n\
                     Content-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());

    let mut request = Request::post("/api/recordings/upload").header(
        header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={BOUNDARY}"),
    );
    if let Some(session) = session {
        request = request.header(header::COOKIE, cookie(session));
    }
    request.body(Body::from(body)).unwrap()
}

/// 제목과 영상 파일을 차례로 보내는 올리기 요청.
fn upload(session: Option<&str>, title: &str, file_name: &str, content: &[u8]) -> Request<Body> {
    upload_request(
        session,
        &[
            ("title", None, title.as_bytes()),
            ("file", Some(file_name), content),
        ],
    )
}

async fn send(app: &Router, request: Request<Body>) -> Response<Body> {
    app.clone().oneshot(request).await.unwrap()
}

async fn json_body(res: Response<Body>) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or_else(|_| panic!("JSON이 아니다: {bytes:?}"))
}

async fn assert_rejected(app: &Router, svc: &AppService, request: Request<Body>, reason: &str) {
    assert_rejected_with(app, svc, request, StatusCode::BAD_REQUEST, reason).await;
}

async fn assert_rejected_with(
    app: &Router,
    svc: &AppService,
    request: Request<Body>,
    status: StatusCode,
    reason: &str,
) {
    let res = send(app, request).await;

    assert_eq!(res.status(), status);
    assert_eq!(json_body(res).await["message"], reason);
    assert!(svc.recordings().await.unwrap().is_empty());
}

#[tokio::test]
async fn upload_by_signed_in_user_registers_recording_and_serves_its_file() {
    let (svc, _, _dir) = service_with_recordings().await;
    let session = signed_in(&svc, "철수").await;
    let app = recording_files::router(svc.clone());

    let res = send(
        &app,
        upload(Some(session.as_str()), "첫 녹화", "clip.mp4", b"0123456789"),
    )
    .await;

    assert_eq!(res.status(), StatusCode::CREATED);
    let id = json_body(res).await["id"].as_i64().expect("id는 숫자다");
    let page = svc.recording_page(id).await.unwrap().expect("등록됐다");
    assert_eq!(page.title, "첫 녹화");
    assert_eq!(page.channel_name, "철수");

    let res = send(
        &app,
        Request::get(&page.video_url).body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "video/mp4");
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"0123456789");
}

#[tokio::test]
async fn video_file_answers_range_requests_for_seeking() {
    let (svc, _, _dir) = service_with_recordings().await;
    let session = signed_in(&svc, "철수").await;
    let app = recording_files::router(svc.clone());
    let res = send(
        &app,
        upload(Some(session.as_str()), "녹화", "clip.webm", b"0123456789"),
    )
    .await;
    let id = json_body(res).await["id"].as_i64().unwrap();
    let video_url = svc.recording_page(id).await.unwrap().unwrap().video_url;

    let res = send(
        &app,
        Request::get(&video_url)
            .header(header::RANGE, "bytes=2-5")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "video/webm");
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&bytes[..], b"2345");
}

#[tokio::test]
async fn missing_video_file_is_not_found() {
    let (svc, _, _dir) = service_with_recordings().await;
    let app = recording_files::router(svc);

    let res = send(
        &app,
        Request::get("/media/recordings/nothing.mp4")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn upload_without_login_is_rejected_with_401() {
    let (svc, _, _dir) = service_with_recordings().await;
    let app = recording_files::router(svc.clone());

    assert_rejected_with(
        &app,
        &svc,
        upload(None, "제목", "clip.mp4", b"x"),
        StatusCode::UNAUTHORIZED,
        "로그인이 필요합니다",
    )
    .await;
}

#[tokio::test]
async fn upload_with_unknown_or_ended_session_is_rejected_with_401() {
    let (svc, _, _dir) = service_with_recordings().await;
    let ended = signed_in(&svc, "철수").await;
    svc.end_session(&ended).await.unwrap();
    let app = recording_files::router(svc.clone());

    for session in ["made-up-token", ended.as_str()] {
        assert_rejected_with(
            &app,
            &svc,
            upload(Some(session), "제목", "clip.mp4", b"x"),
            StatusCode::UNAUTHORIZED,
            "로그인이 필요합니다",
        )
        .await;
    }
}

#[tokio::test]
async fn old_user_id_cookie_is_not_a_login() {
    let (svc, _, _dir) = service_with_recordings().await;
    let (user_id, _) = common::user(&svc, "철수").await;
    let app = recording_files::router(svc.clone());
    let mut request = upload(None, "제목", "clip.mp4", b"x");
    request.headers_mut().insert(
        header::COOKIE,
        format!("streamapp_user={user_id}").parse().unwrap(),
    );

    assert_rejected_with(
        &app,
        &svc,
        request,
        StatusCode::UNAUTHORIZED,
        "로그인이 필요합니다",
    )
    .await;
}

#[tokio::test]
async fn upload_breaking_rules_is_rejected_with_reason() {
    let (svc, _, _dir) = service_with_recordings().await;
    let session = signed_in(&svc, "철수").await;
    let app = recording_files::router(svc.clone());

    assert_rejected(
        &app,
        &svc,
        upload(Some(session.as_str()), " ", "clip.mp4", b"x"),
        "녹화방송 제목을 입력해 주세요",
    )
    .await;
    assert_rejected(
        &app,
        &svc,
        upload(Some(session.as_str()), "제목", "clip.avi", b"x"),
        "mp4나 webm 영상 파일만 등록할 수 있습니다",
    )
    .await;
    assert_rejected(
        &app,
        &svc,
        upload(Some(session.as_str()), "제목", "clip.mp4", b""),
        "빈 영상 파일입니다",
    )
    .await;
}

#[tokio::test]
async fn upload_without_file_field_is_rejected() {
    let (svc, _, _dir) = service_with_recordings().await;
    let session = signed_in(&svc, "철수").await;
    let app = recording_files::router(svc.clone());

    assert_rejected(
        &app,
        &svc,
        upload_request(
            Some(session.as_str()),
            &[("title", None, "제목".as_bytes())],
        ),
        "영상 파일을 골라 주세요",
    )
    .await;
}

#[tokio::test]
async fn upload_with_file_before_title_has_no_title() {
    let (svc, _, _dir) = service_with_recordings().await;
    let session = signed_in(&svc, "철수").await;
    let app = recording_files::router(svc.clone());

    assert_rejected(
        &app,
        &svc,
        upload_request(
            Some(session.as_str()),
            &[
                ("file", Some("clip.mp4"), b"x"),
                ("title", None, "늦은 제목".as_bytes()),
            ],
        ),
        "녹화방송 제목을 입력해 주세요",
    )
    .await;
}
