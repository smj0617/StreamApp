//! 라이브 서버 함수. 요청을 앱 서비스 호출로 바꾸기만 한다.
//!
//! 라이브를 열고 닫는 것은 MediaMTX 훅([`crate::mediamtx`])이 하고, 앱에는 "방송 시작" 버튼이 없다.

use dioxus::prelude::*;

use crate::dto::LiveChannelList;
#[cfg(feature = "server")]
use crate::server::Service;

/// 라이브 중인 채널 목록(홈). 최근에 시작한 순서이고, 경과 시간을 계산할 서버의 현재 시각을 함께 준다.
#[get("/api/live-channels", service: Service)]
pub async fn list_live_channels() -> Result<LiveChannelList, ServerFnError> {
    Ok(LiveChannelList {
        channels: service.live_channels().await?,
        server_now_ms: service.now_ms(),
    })
}
