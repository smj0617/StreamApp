//! 녹화방송 영상 파일 HTTP: 올리기(멀티파트)와 영상 파일 내주기. 요청을 [`AppService`] 호출로 바꾸기만
//! 하는 얇은 어댑터다.
//!
//! Dioxus에 의존하지 않는 axum [`Router`]라서, 앱은 Dioxus 라우터에 `.merge(..)`하고 테스트는 요청을 직접
//! 넣는다(경계 B). [`crate::mediamtx`]처럼 fallback을 두지 않는다.

use axum::{
    extract::{multipart::Field, DefaultBodyLimit, Multipart, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Serialize;
use tower_http::services::ServeDir;

use crate::server::{current_user, displayed_reason, json_message};
use crate::service::{AppService, ServiceError};
use crate::{RECORDING_MEDIA_PATH, RECORDING_UPLOAD_PATH, UPLOAD_FILE_FIELD, UPLOAD_TITLE_FIELD};

/// 올리기 요청 본문 최대 크기: 영상 파일 최대 크기에 제목과 멀티파트 경계 몫을 더한다.
/// 영상 파일 크기는 앱 서비스가 받으면서 따로 센다.
const UPLOAD_BODY_LIMIT: usize = crate::rules::MAX_RECORDING_BYTES as usize + 1024 * 1024;

/// 녹화방송 영상 파일 라우터: 올리기와, 녹화방송 폴더의 파일 내주기(Range 요청 포함).
pub fn router(service: AppService) -> Router {
    let media = ServeDir::new(service.recordings_dir());
    Router::new()
        .route(
            RECORDING_UPLOAD_PATH,
            post(upload).layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT)),
        )
        .nest_service(RECORDING_MEDIA_PATH, media)
        .with_state(service)
}

/// 녹화방송 올리기: 쿠키의 현재 사용자 채널에 녹화방송을 등록한다. 사용자를 고르지 않았거나 기억된
/// 사용자가 DB에 없으면 영상 파일을 받기 전에 거절한다.
async fn upload(
    State(service): State<AppService>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let Some(user_id) = current_user::remembered_id(&headers) else {
        return message(StatusCode::BAD_REQUEST, current_user::NOT_SELECTED);
    };
    let mut title = None;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => return rejected(ServiceError::RecordingFileMissing),
            Err(error) => {
                tracing::info!("녹화방송 올리기 요청을 읽지 못했습니다: {error}");
                return rejected(ServiceError::UploadInterrupted);
            }
        };
        match field.name() {
            Some(UPLOAD_TITLE_FIELD) => match field.text().await {
                Ok(text) => title = Some(text),
                Err(_) => return rejected(ServiceError::UploadInterrupted),
            },
            Some(UPLOAD_FILE_FIELD) => return register(&service, user_id, title, field).await,
            // 모르는 칸은 건너뛴다.
            _ => {}
        }
    }
}

/// 영상 파일 칸을 받으며 녹화방송을 등록한다. 제목 칸이 없었으면 빈 제목으로 보고 거절한다.
async fn register(
    service: &AppService,
    user_id: i64,
    title: Option<String>,
    field: Field<'_>,
) -> Response {
    let file_name = field.file_name().unwrap_or_default().to_owned();
    let title = title.unwrap_or_default();
    match service
        .register_recording(user_id, &title, &file_name, field)
        .await
    {
        Ok(id) => (StatusCode::CREATED, Json(Uploaded { id })).into_response(),
        Err(ServiceError::UnknownUser) => {
            message(StatusCode::BAD_REQUEST, current_user::NOT_SELECTED)
        }
        Err(error) => rejected(error),
    }
}

/// 서비스 오류를 상태 코드와 `{"message": ..}`로 바꾼다. 서버 잘못이면 500과 감춘 사유, 아니면 400.
fn rejected(error: ServiceError) -> Response {
    let status = if error.is_internal() {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    message(status, &displayed_reason(error))
}

fn message(status: StatusCode, message: &str) -> Response {
    json_message(status, message)
}

/// 올리기 성공 응답 본문.
#[derive(Serialize)]
struct Uploaded {
    id: i64,
}
