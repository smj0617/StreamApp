//! 녹화방송 서버 함수와, 웹과 서버가 함께 쓰는 녹화방송 주소. 서버 함수는 요청을 앱 서비스 호출로
//! 바꾸기만 한다. 목록·상세는 로그인하지 않아도 부를 수 있고, 제목 수정·삭제는 로그인한 주인만 할 수 있다
//! (주인 확인은 앱 서비스가 한다).
//!
//! 영상 파일을 받는 올리기와 영상 파일 내주기는 서버 함수가 아니라 axum 라우터
//! ([`crate::recording_files`])가 맡는다. 큰 파일을 조각으로 받아 바로 디스크에 쓰고, `<video>`가 보내는
//! Range 요청에 답해야 하기 때문이다.

use dioxus::prelude::*;

use crate::dto::{RecordingList, RecordingPage};
#[cfg(feature = "server")]
use crate::server::{current_user, Service};

/// 녹화방송 올리기(멀티파트 POST) 주소. 로그인해야 한다. 제목 칸([`UPLOAD_TITLE_FIELD`])을 영상 파일 칸
/// ([`UPLOAD_FILE_FIELD`])보다 먼저 보내야 한다. 성공하면 201과 `{"id": <녹화방송 ID>}`, 거절하면 401(로그인
/// 필요)·400(규칙 위반)·500(서버 잘못)과 `{"message": "<사유>"}`.
pub const RECORDING_UPLOAD_PATH: &str = "/api/recordings/upload";
/// 올리기 요청의 제목 칸 이름.
pub const UPLOAD_TITLE_FIELD: &str = "title";
/// 올리기 요청의 영상 파일 칸 이름.
pub const UPLOAD_FILE_FIELD: &str = "file";
/// 녹화방송 영상 파일을 내주는 주소의 앞부분. 영상 주소는 `<RECORDING_MEDIA_PATH>/<파일 이름>`이다.
pub const RECORDING_MEDIA_PATH: &str = "/media/recordings";

/// 녹화방송 목록(홈). 최근에 등록한 순서이고, 등록 후 지난 시간을 계산할 서버의 현재 시각을 함께 준다.
#[get("/api/recordings", service: Service)]
pub async fn list_recordings() -> Result<RecordingList, ServerFnError> {
    Ok(RecordingList {
        recordings: service.recordings().await?,
        server_now_ms: service.now_ms(),
    })
}

/// 녹화방송 상세 페이지 정보. 없는 녹화방송이면 `None`.
#[get("/api/recordings/{id}", service: Service)]
pub async fn recording_page(id: i64) -> Result<Option<RecordingPage>, ServerFnError> {
    Ok(service.recording_page(id).await?)
}

/// 녹화방송 제목 바꾸기. 로그인한 주인만 할 수 있다. 저장한 제목을 돌려준다.
/// 로그인하지 않았으면 401, 주인이 아니면 403, 없는 녹화방송이면 404, 제목 규칙 위반이면 400.
#[post("/api/recordings/rename", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn rename_recording(id: i64, title: String) -> Result<String, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.rename_recording(user_id, id, &title).await?)
}

/// 녹화방송 삭제(영상 파일과 댓글 포함). 로그인한 주인만 할 수 있다.
/// 로그인하지 않았으면 401, 주인이 아니면 403, 없는 녹화방송이면 404.
#[post("/api/recordings/delete", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn delete_recording(id: i64) -> Result<(), ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.delete_recording(user_id, id).await?)
}
