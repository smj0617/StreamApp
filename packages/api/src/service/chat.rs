//! 채팅(Chat) 동작: 채팅 참여와 채팅 보내기.
//!
//! 채팅은 라이브 하나에 딸려 라이브 중에만 열린다. 메시지는 저장하지 않고, 채팅마다(채널 ID로 찾는다)
//! 메모리에 둔 tokio broadcast로 그 채팅의 참여자에게 보내기만 한다.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use super::{AppService, ServiceError};
use crate::dto::ChatMessage;
use crate::rules::MAX_CHAT_CHARS;

/// 채팅 하나의 broadcast 버퍼가 담아 두는 이벤트 수. 이보다 뒤처진 참여자는 밀린 메시지를 건너뛴다.
const CHAT_BUFFER_CAPACITY: usize = 256;

/// 채팅 참여자가 받는 이벤트.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatEvent {
    /// 그 채널에 누군가 보낸 메시지
    Message(ChatMessage),
    /// 참여한 라이브의 제목이 바뀌었다(스트리머가 라이브 중에 바꿨다). `title`은 저장된 제목이다.
    LiveTitleChanged { title: String },
    /// 참여한 라이브가 끝났다. 이 뒤로는 이벤트가 없다.
    LiveEnded,
}

/// 채팅 참여 결과: 참여한 라이브의 이벤트를 보낸 순서대로 받는 구독.
pub struct ChatSubscription {
    receiver: broadcast::Receiver<ChatEvent>,
}

impl ChatSubscription {
    /// 다음 이벤트를 기다린다. 라이브 종료 알림 뒤에는 `None`(구독 끝).
    ///
    /// 너무 뒤처져 밀린 메시지는 건너뛰고 받을 수 있는 다음 이벤트를 돌려준다.
    pub async fn next_event(&mut self) -> Option<ChatEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!("채팅 참여자가 뒤처져 메시지 {skipped}개를 건너뜁니다");
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

/// 채팅마다 broadcast 송신자 하나. 잠금 구간에서 `.await`하지 않으므로 `std::sync::Mutex`로 충분하다.
#[derive(Clone, Default)]
pub(super) struct ChatHub {
    /// 채널 ID → 그 채널에서 열린 채팅의 broadcast 송신자
    chats: Arc<Mutex<HashMap<String, broadcast::Sender<ChatEvent>>>>,
}

impl ChatHub {
    fn subscribe(&self, channel_id: &str) -> ChatSubscription {
        let mut chats = self.chats.lock().expect("채팅 허브 잠금");
        let sender = chats
            .entry(channel_id.to_owned())
            .or_insert_with(|| broadcast::channel(CHAT_BUFFER_CAPACITY).0);
        ChatSubscription {
            receiver: sender.subscribe(),
        }
    }

    /// 그 채널의 채팅 참여자 모두에게 이벤트를 보낸다. 참여자가 없으면 아무것도 하지 않는다.
    pub(super) fn send(&self, channel_id: &str, event: ChatEvent) {
        let mut chats = self.chats.lock().expect("채팅 허브 잠금");
        if let Some(sender) = chats.get(channel_id) {
            // 참여자가 모두 나갔으면 보낼 곳이 없을 뿐이다. 남은 broadcast는 치운다.
            if sender.send(event).is_err() {
                chats.remove(channel_id);
            }
        }
    }

    /// 그 채널의 참여자 모두에게 라이브 종료를 알리고 broadcast를 치운다. 참여자의 구독은 알림을
    /// 받은 뒤 끝나고, 같은 채널의 다음 라이브는 새 broadcast를 쓴다.
    pub(super) fn end_live(&self, channel_id: &str) {
        let removed = self
            .chats
            .lock()
            .expect("채팅 허브 잠금")
            .remove(channel_id);
        if let Some(sender) = removed {
            let _ = sender.send(ChatEvent::LiveEnded);
        }
    }

    /// 참여자가 하나도 없는 채팅의 broadcast를 치운다.
    fn forget_if_unused(&self, channel_id: &str) {
        let mut chats = self.chats.lock().expect("채팅 허브 잠금");
        if chats
            .get(channel_id)
            .is_some_and(|sender| sender.receiver_count() == 0)
        {
            chats.remove(channel_id);
        }
    }
}

impl AppService {
    /// 채팅 참여: 그 채널의 메시지를 받는 구독을 돌려준다.
    /// 라이브 중이 아니면(없는 채널 포함) [`ServiceError::NotLive`].
    pub async fn join_chat(&self, channel_id: &str) -> Result<ChatSubscription, ServiceError> {
        // 구독을 먼저 만들고 라이브인지 본다. 거꾸로 하면 확인과 구독 사이에 라이브가 끝났을 때
        // 종료 알림을 받지 못한 채 구독이 남는다.
        let subscription = self.chat.subscribe(channel_id);
        if !self.is_live(channel_id).await? {
            drop(subscription);
            self.chat.forget_if_unused(channel_id);
            return Err(ServiceError::NotLive);
        }
        Ok(subscription)
    }

    /// 채팅 보내기: 그 채널의 참여자 모두에게 보낸 사람의 닉네임, 내용(앞뒤 공백을 지운 것), 시각을
    /// 보내고, 보낸 메시지를 돌려준다.
    ///
    /// 내용이 규칙(앞뒤 공백을 지운 뒤 1~[`MAX_CHAT_CHARS`]자)을 어기면 [`ServiceError::ChatMessageEmpty`]나
    /// [`ServiceError::ChatMessageTooLong`], 없는 사용자면 [`ServiceError::UnknownUser`],
    /// 그 채널이 라이브 중이 아니면 [`ServiceError::NotLive`].
    pub async fn send_chat(
        &self,
        user_id: i64,
        channel_id: &str,
        content: &str,
    ) -> Result<ChatMessage, ServiceError> {
        let content = valid_chat_content(content)?;
        let user = self
            .find_user(user_id)
            .await?
            .ok_or(ServiceError::UnknownUser)?;
        if !self.is_live(channel_id).await? {
            return Err(ServiceError::NotLive);
        }
        let message = ChatMessage {
            nickname: user.nickname,
            content: content.to_owned(),
            sent_at_ms: self.now_ms(),
        };
        self.chat
            .send(channel_id, ChatEvent::Message(message.clone()));
        Ok(message)
    }
}

/// 채팅 메시지 내용 규칙: 앞뒤 공백을 지운 뒤 1~[`MAX_CHAT_CHARS`]자. 통과하면 공백을 지운 내용을 돌려준다.
fn valid_chat_content(input: &str) -> Result<&str, ServiceError> {
    let content = input.trim();
    if content.is_empty() {
        return Err(ServiceError::ChatMessageEmpty);
    }
    // 바이트가 아니라 글자 수로 센다(한글 한 글자 = 3바이트).
    if content.chars().count() > MAX_CHAT_CHARS {
        return Err(ServiceError::ChatMessageTooLong);
    }
    Ok(content)
}
