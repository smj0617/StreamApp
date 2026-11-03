//! 현재 사용자: 고른 사용자 ID를 쿠키에 기억하고 읽는다.
//!
//! 인증이 없으므로(ADR 0001) 쿠키에는 사용자 ID만 넣는다. 일반 창과 시크릿 창은 쿠키
//! 저장소가 달라 서로 다른 사용자로 동시에 있을 수 있다. 서버 함수는 여기서 읽은 ID를
//! 앱 서비스에 넘기기만 한다.

use dioxus::fullstack::headers::{Cookie, HeaderMapExt};
use dioxus::fullstack::{http, FullstackContext, HeaderMap};
use dioxus::prelude::ServerFnError;

use crate::service::ServiceError;

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

/// 사용자를 고르지 않아 거절할 때 보여 줄 사유.
pub const NOT_SELECTED: &str = "사용자를 먼저 골라 주세요";

/// 현재 사용자가 있어야 하는 요청을, 사용자를 고르지 않았을 때 거절하는 오류(400과 사유).
pub fn not_selected() -> ServerFnError {
    ServerFnError::ServerError {
        message: NOT_SELECTED.to_owned(),
        code: 400,
        details: None,
    }
}

/// 현재 사용자 ID로 한 서비스 동작의 결과를 서버 함수 결과로 바꾼다. 기억된 사용자가 DB에 없으면
/// ([`ServiceError::UnknownUser`], 예: DB 파일을 지운 뒤) 고르지 않은 것으로 보고 [`not_selected`]로 거절한다.
pub fn unknown_as_not_selected<T>(result: Result<T, ServiceError>) -> Result<T, ServerFnError> {
    result.map_err(|error| match error {
        ServiceError::UnknownUser => not_selected(),
        other => other.into(),
    })
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
