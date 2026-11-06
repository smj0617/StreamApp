//! 댓글 서버 함수. 요청을 앱 서비스 호출로 바꾸기만 한다.
//!
//! 목록은 로그인하지 않아도 볼 수 있다. 쓰기·고치기·지우기는 세션 쿠키의 로그인 사용자로 하고, 로그인하지
//! 않았으면 401로 거절한다. 고치기·지우기의 작성자 확인은 앱 서비스가 한다(403).

use dioxus::prelude::*;

use crate::dto::{BroadcastTarget, Comment, CommentList};
#[cfg(feature = "server")]
use crate::server::{current_user, Service};

/// 녹화방송 하나의 댓글 목록(오래된 순). 없는 녹화방송이면 404.
#[get("/api/recordings/{id}/comments", service: Service)]
pub async fn recording_comments(id: i64) -> Result<CommentList, ServerFnError> {
    Ok(service.comments(BroadcastTarget::Recording(id)).await?)
}

/// 라이브 한 회차의 댓글 목록(오래된 순). 없는 라이브면 404.
#[get("/api/lives/{id}/comments", service: Service)]
pub async fn live_comments(id: i64) -> Result<CommentList, ServerFnError> {
    Ok(service.comments(BroadcastTarget::Live(id)).await?)
}

/// 댓글 쓰기: 로그인한 사용자를 작성자로 방송에 댓글을 단다. 만든 댓글을 돌려준다.
/// 로그인하지 않았으면 401, 없는 방송이면 404, 내용 규칙 위반이면 400.
#[post("/api/comments", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn write_comment(
    target: BroadcastTarget,
    content: String,
) -> Result<Comment, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.write_comment(user_id, target, &content).await?)
}

/// 댓글 고치기: 쓴 사람만. 고친 댓글을 돌려준다.
/// 로그인하지 않았으면 401, 쓴 사람이 아니면 403, 없는 댓글이면 404, 내용 규칙 위반이면 400.
#[post("/api/comments/edit", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn edit_comment(id: i64, content: String) -> Result<Comment, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.edit_comment(user_id, id, &content).await?)
}

/// 댓글 지우기: 쓴 사람만. 로그인하지 않았으면 401, 쓴 사람이 아니면 403, 없는 댓글이면 404.
#[post("/api/comments/delete", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn delete_comment(id: i64) -> Result<(), ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.delete_comment(user_id, id).await?)
}
