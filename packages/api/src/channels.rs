//! 내 채널 설정 서버 함수(다음 라이브 제목, 진행 중인 라이브 제목 포함).
//! 쿠키의 현재 사용자를 앱 서비스 호출로 넘기기만 한다.

use dioxus::prelude::*;

use crate::dto::MyChannelSettings;
#[cfg(feature = "server")]
use crate::server::{current_user, Service};
#[cfg(feature = "server")]
use crate::service::ServiceError;

/// 현재 사용자의 채널 설정. 사용자를 고르지 않았거나 기억된 사용자가 DB에 없으면 `None`.
#[get("/api/my-channel", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn my_channel_settings() -> Result<Option<MyChannelSettings>, ServerFnError> {
    let Some(user_id) = current_user::remembered_id(&headers) else {
        return Ok(None);
    };
    Ok(service.my_channel_settings(user_id).await?)
}

/// 현재 사용자 채널의 다음 라이브 제목 바꾸기. 저장한 제목(앞뒤 공백을 지운 것,
/// 비웠으면 `None`)을 돌려준다. 100자를 넘거나 사용자를 고르지 않았으면 400과 거절 사유.
#[post("/api/my-channel/next-live-title", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn change_next_live_title(title: String) -> Result<Option<String>, ServerFnError> {
    let user_id = current_user::remembered_id(&headers).ok_or_else(current_user::not_selected)?;
    match service.change_next_live_title(user_id, &title).await {
        // 기억된 사용자가 DB에 없으면 고르지 않은 것으로 본다.
        Err(ServiceError::UnknownUser) => Err(current_user::not_selected()),
        result => Ok(result?),
    }
}

/// 현재 사용자 채널의 진행 중인 라이브 제목 바꾸기. 저장한 제목(앞뒤 공백을 지운 것,
/// 비웠으면 "`<채널 이름>`님의 라이브")을 돌려준다. 라이브 중이 아니거나, 100자를 넘거나,
/// 사용자를 고르지 않았으면 400과 거절 사유.
#[post("/api/my-channel/live-title", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn change_live_title(title: String) -> Result<String, ServerFnError> {
    let user_id = current_user::remembered_id(&headers).ok_or_else(current_user::not_selected)?;
    match service.change_live_title(user_id, &title).await {
        // 기억된 사용자가 DB에 없으면 고르지 않은 것으로 본다.
        Err(ServiceError::UnknownUser) => Err(current_user::not_selected()),
        result => Ok(result?),
    }
}
