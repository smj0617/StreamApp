//! 채팅 WebSocket 서버 함수. 연결 하나를 앱 서비스의 채팅 참여·채팅 보내기 호출로 바꾸기만 한다.
//!
//! 보낸 사람은 WebSocket을 연결할 때 쿠키에 있던 사용자다. 사용자를 바꾸면 클라이언트가 연결을
//! 다시 맺는다. 거절은 HTTP 오류가 아니라 연결된 뒤 메시지로 알린다(브라우저 WebSocket은 업그레이드
//! 응답의 상태 코드를 보여 주지 않는다).

use dioxus::fullstack::{WebSocketOptions, Websocket};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::dto::ChatMessage;
#[cfg(feature = "server")]
use crate::server::{current_user, Service};

/// 클라이언트 → 서버.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ChatClientEvent {
    /// 메시지 보내기. 내용 규칙은 서버가 확인한다.
    Send { content: String },
}

/// 서버 → 클라이언트.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ChatServerEvent {
    /// 채팅에 참여했다. 이 뒤에 보낸 메시지부터 받는다.
    Joined,
    /// 그 채널에 누군가(자기 자신 포함) 보낸 메시지
    Message(ChatMessage),
    /// 스트리머가 라이브 중에 라이브 제목을 바꿨다. 시청 페이지가 보여 주는 제목을 바꾼다.
    LiveTitleChanged { title: String },
    /// 라이브가 끝났다. 서버가 곧 연결을 닫는다.
    LiveEnded,
    /// 이 연결이 보낸 메시지가 거절된 사유. 보낸 연결에만 온다.
    Rejected { reason: String },
}

/// 채널 하나의 채팅 연결. 라이브 중이 아니면 연결 직후 [`ChatServerEvent::LiveEnded`]를 보내고 닫는다.
#[get("/api/chat/{channel_id}", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn connect_chat(
    channel_id: String,
    options: WebSocketOptions,
) -> Result<Websocket<ChatClientEvent, ChatServerEvent>> {
    let sender = current_user::remembered_id(&headers);
    let service = service.0;
    Ok(options.on_upgrade(move |socket| {
        crate::server::chat_socket::serve(service, channel_id, sender, socket)
    }))
}
