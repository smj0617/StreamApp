//! 현재 사용자: 로그인 세션 토큰을 쿠키에 담고, 요청마다 그 토큰으로 사용자를 찾는다(ADR 0005).
//!
//! 쿠키에는 무작위 세션 토큰만 넣고(HttpOnly, SameSite=Lax), 토큰이 가리키는 사용자는 서버가 DB의
//! 세션으로 정한다. 그래서 쿠키를 고쳐 다른 사용자가 될 수 없다. 서버 함수와 axum 어댑터(녹화방송 올리기,
//! 휴대폰 방송 중계, 채팅)는 여기서 찾은 사용자 ID를 앱 서비스에 넘기기만 하고, 방송 주인·댓글 작성자
//! 확인은 앱 서비스가 한다.

use dioxus::fullstack::headers::{Cookie, HeaderMapExt};
use dioxus::fullstack::{http, FullstackContext, HeaderMap};
use dioxus::prelude::ServerFnError;

use crate::dto::User;
use crate::service::{AppService, ServiceError, SESSION_LIFETIME_MS};

/// 세션 토큰을 담는 쿠키 이름.
pub const COOKIE_NAME: &str = "streamapp_session";

/// 요청 쿠키의 세션 토큰. 쿠키가 없거나 비었으면 `None`.
///
/// `HeaderMap` 추출자로 받은 헤더를 넘긴다. `TypedHeader<Cookie>` 추출자는 쿠키가 없는
/// 첫 방문 요청을 거절하므로 쓰지 않는다.
pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .typed_get::<Cookie>()?
        .get(COOKIE_NAME)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

/// 요청을 보낸 로그인 사용자. 로그인하지 않았거나, 세션이 끝났거나(로그아웃·기한), 모르는 토큰이면 `None`.
pub async fn signed_in(
    service: &AppService,
    headers: &HeaderMap,
) -> Result<Option<User>, ServiceError> {
    match session_token(headers) {
        Some(token) => service.session_user(&token).await,
        None => Ok(None),
    }
}

/// 로그인한 사용자 ID. 로그인하지 않았으면 [`ServiceError::NotSignedIn`](401).
pub async fn require_user_id(
    service: &AppService,
    headers: &HeaderMap,
) -> Result<i64, ServiceError> {
    signed_in(service, headers)
        .await?
        .map(|user| user.id)
        .ok_or(ServiceError::NotSignedIn)
}

/// 지금 처리 중인 서버 함수 응답에 세션 토큰 쿠키를 싣는다(로그인·회원가입).
pub fn remember_session(token: &str) -> Result<(), ServerFnError> {
    let max_age = SESSION_LIFETIME_MS / 1000;
    // Path=/ 가 없으면 쿠키가 /api 아래 요청에만 실려 페이지(SSR) 요청에서 읽지 못한다.
    // HttpOnly: 페이지 스크립트가 토큰을 읽지 못한다. SameSite=Lax: 다른 사이트에서 보낸 POST에는 실리지 않는다.
    set_cookie(&format!(
        "{COOKIE_NAME}={token}; Path=/; Max-Age={max_age}; SameSite=Lax; HttpOnly"
    ))
}

/// 지금 처리 중인 서버 함수 응답으로 세션 토큰 쿠키를 지운다(로그아웃).
pub fn forget_session() -> Result<(), ServerFnError> {
    set_cookie(&format!(
        "{COOKIE_NAME}=; Path=/; Max-Age=0; SameSite=Lax; HttpOnly"
    ))
}

fn set_cookie(cookie: &str) -> Result<(), ServerFnError> {
    let value =
        http::HeaderValue::from_str(cookie).map_err(|e| ServerFnError::new(e.to_string()))?;
    let context = FullstackContext::current()
        .ok_or_else(|| ServerFnError::new("요청 정보 없이 쿠키를 쓸 수 없습니다"))?;
    context.add_response_header(http::header::SET_COOKIE, value);
    Ok(())
}
