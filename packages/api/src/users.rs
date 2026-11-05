//! 회원가입·로그인·로그아웃 서버 함수. 요청을 앱 서비스 호출로 바꾸고 세션 쿠키를 싣거나 지운다.
//!
//! 현재 사용자는 세션 쿠키로 정한다([`crate::server::current_user`], ADR 0005).

use dioxus::prelude::*;

use crate::dto::User;
#[cfg(feature = "server")]
use crate::server::{current_user, Service};

/// 회원가입: 닉네임과 비밀번호로 사용자와 채널을 만들고 곧바로 로그인한다(세션 쿠키).
/// 닉네임·비밀번호 규칙을 어기거나 닉네임이 겹치면 400과 거절 사유를 돌려준다.
#[post("/api/auth/sign-up", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn sign_up(nickname: String, password: String) -> Result<User, ServerFnError> {
    let created = service.sign_up(&nickname, &password).await?;
    start_session(&service, &headers, created.user.id).await?;
    Ok(created.user)
}

/// 로그인: 닉네임과 비밀번호가 맞으면 세션 쿠키를 싣고 그 사용자를 돌려준다. 틀리면 401과
/// "닉네임 또는 비밀번호가 맞지 않습니다".
#[post("/api/auth/login", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn log_in(nickname: String, password: String) -> Result<User, ServerFnError> {
    let user = service.log_in(&nickname, &password).await?;
    start_session(&service, &headers, user.id).await?;
    Ok(user)
}

/// 로그아웃: 서버의 세션을 지우고 세션 쿠키도 지운다. 로그인하지 않았어도 성공이다.
#[post("/api/auth/logout", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn log_out() -> Result<(), ServerFnError> {
    if let Some(token) = current_user::session_token(&headers) {
        service.end_session(&token).await?;
    }
    current_user::forget_session()
}

/// 현재 로그인한 사용자. 로그인하지 않았거나 세션이 끝났으면 `None`.
#[get("/api/current-user", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn current_user() -> Result<Option<User>, ServerFnError> {
    Ok(current_user::signed_in(&service, &headers).await?)
}

/// 새 세션을 만들어 쿠키에 싣는다. 이 브라우저에 남아 있던 세션은 지운다(로그인할 때마다 새 토큰).
#[cfg(feature = "server")]
async fn start_session(
    service: &crate::service::AppService,
    headers: &dioxus::fullstack::HeaderMap,
    user_id: i64,
) -> Result<(), ServerFnError> {
    if let Some(old) = current_user::session_token(headers) {
        service.end_session(&old).await?;
    }
    let token = service.start_session(user_id).await?;
    current_user::remember_session(&token)
}
