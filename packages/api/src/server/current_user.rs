//! 현재 사용자: 고른 사용자 ID를 쿠키에 기억하고 읽는다.
//!
//! 인증이 없으므로(ADR 0001) 쿠키에는 사용자 ID만 넣는다. 일반 창과 시크릿 창은 쿠키
//! 저장소가 달라 서로 다른 사용자로 동시에 있을 수 있다. 서버 함수는 여기서 읽은 ID를
//! 앱 서비스에 넘기기만 한다.

use dioxus::fullstack::headers::{Cookie, HeaderMapExt};
use dioxus::fullstack::{http, FullstackContext, HeaderMap};
use dioxus::prelude::ServerFnError;

/// 고른 사용자 ID를 담는 쿠키 이름.
pub const COOKIE_NAME: &str = "streamapp_user";

/// 요청 쿠키에 기억된 사용자 ID. 쿠키가 없거나 숫자가 아니면 `None`.
///
/// `HeaderMap` 추출자로 받은 헤더를 넘긴다. `TypedHeader<Cookie>` 추출자는 쿠키가 없는
/// 첫 방문 요청을 거절하므로 쓰지 않는다.
pub fn remembered_id(headers: &HeaderMap) -> Option<i64> {
    headers
        .typed_get::<Cookie>()?
        .get(COOKIE_NAME)?
        .parse()
        .ok()
}

/// 지금 처리 중인 서버 함수 응답에 사용자 ID 쿠키를 싣는다.
pub fn remember(user_id: i64) -> Result<(), ServerFnError> {
    // Path=/ 가 없으면 쿠키가 /api 아래 요청에만 실려 페이지(SSR) 요청에서 읽지 못한다.
    let cookie =
        format!("{COOKIE_NAME}={user_id}; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly");
    let value =
        http::HeaderValue::from_str(&cookie).map_err(|e| ServerFnError::new(e.to_string()))?;
    let context = FullstackContext::current()
        .ok_or_else(|| ServerFnError::new("요청 정보 없이 쿠키를 쓸 수 없습니다"))?;
    context.add_response_header(http::header::SET_COOKIE, value);
    Ok(())
}
