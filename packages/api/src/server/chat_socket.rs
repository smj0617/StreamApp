//! 채팅 WebSocket 연결 하나를 처리한다([`crate::connect_chat`] 서버 함수가 업그레이드한 뒤).
//!
//! 앱 서비스의 채팅 참여로 받은 이벤트를 클라이언트에 보내고, 클라이언트가 보낸 메시지를 채팅
//! 보내기로 넘긴다. 규칙은 모두 서비스가 정하고, 여기서는 거절 사유를 메시지로 바꾸기만 한다.

use dioxus::fullstack::{TypedWebsocket, WebsocketError};

use super::{current_user, displayed_reason};
use crate::chat::{ChatClientEvent, ChatServerEvent};
use crate::service::{AppService, ChatEvent, ServiceError};

/// 연결이 끊기거나 라이브가 끝날 때까지 돈다.
///
/// `sender`는 연결할 때 쿠키에 있던 사용자 ID다(고르지 않았으면 `None`). 연결 도중 사용자를 바꾸면
/// 클라이언트가 새로 연결하므로, 이 연결의 보낸 사람은 바뀌지 않는다.
pub async fn serve(
    service: AppService,
    channel_id: String,
    sender: Option<i64>,
    mut socket: TypedWebsocket<ChatClientEvent, ChatServerEvent>,
) {
    let mut subscription = match service.join_chat(&channel_id).await {
        Ok(subscription) => subscription,
        // 시청 페이지는 라이브 중일 때만 채팅을 연다. 연결하는 사이 라이브가 끝난 것이다.
        Err(ServiceError::NotLive) => {
            let _ = socket.send(ChatServerEvent::LiveEnded).await;
            return;
        }
        Err(e) => {
            let _ = socket
                .send(ChatServerEvent::Rejected {
                    reason: displayed_reason(e),
                })
                .await;
            return;
        }
    };
    if socket.send(ChatServerEvent::Joined).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            event = subscription.next_event() => match event {
                Some(ChatEvent::Message(message)) => {
                    if socket.send(ChatServerEvent::Message(message)).await.is_err() {
                        break;
                    }
                }
                Some(ChatEvent::LiveTitleChanged { title }) => {
                    if socket.send(ChatServerEvent::LiveTitleChanged { title }).await.is_err() {
                        break;
                    }
                }
                Some(ChatEvent::LiveEnded) | None => {
                    let _ = socket.send(ChatServerEvent::LiveEnded).await;
                    break;
                }
            },
            incoming = socket.recv() => match incoming {
                Ok(ChatClientEvent::Send { content }) => {
                    let sent = match sender {
                        Some(user_id) => service
                            .send_chat(user_id, &channel_id, &content)
                            .await
                            .map(drop)
                            .map_err(displayed_reason),
                        None => Err(current_user::NOT_SELECTED.to_owned()),
                    };
                    if let Err(reason) = sent {
                        if socket.send(ChatServerEvent::Rejected { reason }).await.is_err() {
                            break;
                        }
                    }
                }
                // 읽을 수 없는 메시지는 버리고 연결은 유지한다.
                Err(WebsocketError::Deserialization(_)) => {}
                // 클라이언트가 연결을 닫았다(페이지를 떠났거나 사용자를 바꿔 다시 연결).
                Err(_) => break,
            },
        }
    }
}
