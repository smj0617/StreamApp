//! 사용자 서버 함수. 요청을 앱 서비스 호출로 바꾸기만 한다.

use dioxus::prelude::*;

use crate::dto::User;
#[cfg(feature = "server")]
use crate::server::Service;

/// 사용자 목록 (상단 바 드롭다운).
#[get("/api/users", service: Service)]
pub async fn list_users() -> Result<Vec<User>, ServerFnError> {
    Ok(service.list_users().await?)
}
