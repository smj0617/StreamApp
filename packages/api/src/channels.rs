//! 내 채널 설정 서버 함수(다음 라이브 제목, 진행 중인 라이브 제목 포함).
//! 세션 쿠키의 로그인 사용자를 앱 서비스 호출로 넘기기만 한다. 언제나 그 사용자 자신의 채널만 다루므로
//! 다른 사람의 채널 설정·송출 키·라이브 제목에는 닿을 수 없다.

use dioxus::prelude::*;

use crate::dto::MyChannelSettings;
#[cfg(feature = "server")]
use crate::server::{current_user, Service};

/// 로그인한 사용자의 채널 설정(송출 키 포함). 로그인하지 않았으면 401.
#[get("/api/my-channel", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn my_channel_settings() -> Result<Option<MyChannelSettings>, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.my_channel_settings(user_id).await?)
}

/// 로그인한 사용자 채널의 다음 라이브 제목 바꾸기. 저장한 제목(앞뒤 공백을 지운 것,
/// 비웠으면 `None`)을 돌려준다. 100자를 넘으면 400, 로그인하지 않았으면 401과 거절 사유.
#[post("/api/my-channel/next-live-title", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn change_next_live_title(title: String) -> Result<Option<String>, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.change_next_live_title(user_id, &title).await?)
}

/// 로그인한 사용자 채널의 진행 중인 라이브 제목 바꾸기. 저장한 제목(앞뒤 공백을 지운 것,
/// 비웠으면 자동 제목 "`<채널 이름>`님의 라이브")을 돌려준다. 라이브 중이 아니거나 100자를 넘으면 400,
/// 로그인하지 않았으면 401과 거절 사유.
#[post("/api/my-channel/live-title", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn change_live_title(title: String) -> Result<String, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.change_live_title(user_id, &title).await?)
}
