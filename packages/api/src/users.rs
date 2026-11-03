//! 사용자 서버 함수. 요청을 앱 서비스 호출로 바꾸기만 한다.
//!
//! 현재 사용자는 쿠키로 기억한다([`crate::server::current_user`]).

use dioxus::prelude::*;

use crate::dto::{CreatedUser, User};
#[cfg(feature = "server")]
use crate::server::{current_user, Service};
#[cfg(feature = "server")]
use crate::service::ServiceError;

/// 사용자 목록 (상단 바 드롭다운).
#[get("/api/users", service: Service)]
pub async fn list_users() -> Result<Vec<User>, ServerFnError> {
    Ok(service.list_users().await?)
}

/// 사용자 만들기. 만든 사용자를 곧바로 현재 사용자로 기억한다.
/// 닉네임 규칙을 어기거나 겹치면 400과 거절 사유를 돌려준다.
#[post("/api/users", service: Service)]
pub async fn create_user(nickname: String) -> Result<CreatedUser, ServerFnError> {
    let created = service.create_user(&nickname).await?;
    current_user::remember(created.user.id)?;
    Ok(created)
}

/// 기존 사용자 고르기: 그 사용자를 현재 사용자로 기억한다.
#[post("/api/current-user", service: Service)]
pub async fn select_user(user_id: i64) -> Result<User, ServerFnError> {
    let user = service
        .find_user(user_id)
        .await?
        .ok_or(ServiceError::UnknownUser)?;
    current_user::remember(user.id)?;
    Ok(user)
}

/// 현재 사용자. 고르지 않았거나 기억된 사용자가 DB에 없으면(예: DB 파일을 지운 뒤) `None`.
#[get("/api/current-user", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn current_user() -> Result<Option<User>, ServerFnError> {
    let Some(user_id) = current_user::remembered_id(&headers) else {
        return Ok(None);
    };
    Ok(service.find_user(user_id).await?)
}
