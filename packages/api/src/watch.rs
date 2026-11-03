//! 시청 서버 함수. 요청을 앱 서비스 호출로 바꾸기만 한다. 사용자를 고르지 않아도 부를 수 있다.

use dioxus::prelude::*;

use crate::dto::WatchPage;
#[cfg(feature = "server")]
use crate::server::Service;

/// 시청 페이지 정보: 채널 이름, 재생 주소, 지금 열린 라이브(오프라인이면 없음). 없는 채널이면 `None`.
#[get("/api/watch/{channel_id}", service: Service)]
pub async fn watch_page(channel_id: String) -> Result<Option<WatchPage>, ServerFnError> {
    Ok(service.watch_page(&channel_id).await?)
}
