//! Redis 채팅 중계(과제 ⑤): 여러 앱 인스턴스가 같은 채팅의 이벤트를 나눠 받게 한다.
//!
//! - 이벤트를 보내는 인스턴스는 자기 참여자에게 바로 보내고([`super::chat::LocalChats`]), 같은 이벤트를 Redis
//!   채널 `streamapp:chat:<채널ID>`에 올린다(PUBLISH). 올리는 일은 작업 하나가 순서대로 하므로 한 인스턴스가
//!   보낸 이벤트의 순서가 유지된다.
//! - 모든 인스턴스는 `streamapp:chat:*`를 구독하고(PSUBSCRIBE), 다른 인스턴스가 올린 이벤트를 자기 참여자에게
//!   보낸다. 자기가 올린 이벤트는 이미 보냈으므로 인스턴스 ID로 걸러 낸다.
//!
//! Redis에 연결하지 못해도 앱은 멈추지 않는다: 이 인스턴스의 참여자는 그대로 채팅하고, 다른 인스턴스로의
//! 전달만 끊긴다(경고 로그). 구독은 끊기면 잠시 뒤 다시 연결하고, 올리기는 다음 이벤트 때 다시 연결한다.
//! 끊긴 동안의 이벤트는 다시 보내지 않는다(채팅은 저장하지 않는다).

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::chat::{ChatEvent, LocalChats};
use crate::dto::ChatMessage;

/// 채팅 이벤트를 올리는 Redis 채널 이름의 앞부분. 뒤에 채널 ID가 붙는다.
pub const CHANNEL_PREFIX: &str = "streamapp:chat:";
/// 올리기를 기다리는 이벤트 수. 이보다 밀리면(Redis가 느리거나 끊김) 다른 인스턴스로의 전달을 건너뛴다.
const OUTBOX_CAPACITY: usize = 1024;
/// Redis 연결을 기다리는 시간.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// 올리기 연결에 실패한 뒤 다시 연결해 보기까지 이벤트를 건너뛰는 시간.
const PUBLISH_RETRY_AFTER: Duration = Duration::from_secs(1);
/// 구독이 끊긴 뒤 다시 연결해 보기까지 기다리는 시간(처음 값과 가장 긴 값).
const SUBSCRIBE_RETRY_MIN: Duration = Duration::from_millis(500);
const SUBSCRIBE_RETRY_MAX: Duration = Duration::from_secs(10);

/// Redis로 주고받는 이벤트 한 건(JSON).
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct Envelope {
    /// 올린 인스턴스의 ID. 자기가 올린 이벤트를 걸러 낸다.
    origin: String,
    event: WireEvent,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireEvent {
    Message(ChatMessage),
    LiveTitleChanged { title: String },
    LiveEnded,
}

impl From<&ChatEvent> for WireEvent {
    fn from(event: &ChatEvent) -> Self {
        match event {
            ChatEvent::Message(message) => WireEvent::Message(message.clone()),
            ChatEvent::LiveTitleChanged { title } => WireEvent::LiveTitleChanged {
                title: title.clone(),
            },
            ChatEvent::LiveEnded => WireEvent::LiveEnded,
        }
    }
}

/// Redis 채팅 중계. 복제해도 같은 올리기 작업을 함께 쓴다.
#[derive(Clone)]
pub(super) struct ChatRelay {
    client: redis::Client,
    /// 이 인스턴스의 ID(시작할 때 무작위로 정한다)
    instance_id: std::sync::Arc<str>,
    /// 올리기 작업으로 보내는 (Redis 채널 이름, JSON)
    outbox: mpsc::Sender<(String, String)>,
}

impl ChatRelay {
    /// Redis 주소(`redis://host:6379` 등)로 중계를 만들고 올리기 작업을 띄운다. 주소 모양이 잘못됐으면
    /// 오류지만, Redis가 꺼져 있어도 오류가 아니다(연결은 작업이 맡는다). tokio 런타임 안에서 불러야 한다.
    pub(super) fn connect(redis_url: &str) -> redis::RedisResult<Self> {
        let client = redis::Client::open(redis_url)?;
        let (outbox, receiver) = mpsc::channel(OUTBOX_CAPACITY);
        tokio::spawn(publish_loop(client.clone(), receiver));
        Ok(Self {
            client,
            instance_id: format!("{:016x}", rand::random::<u64>()).into(),
            outbox,
        })
    }

    /// 다른 인스턴스가 올린 이벤트를 `local`의 참여자에게 보내는 구독 작업을 띄운다.
    pub(super) fn deliver_remote_events_to(&self, local: LocalChats) {
        tokio::spawn(subscribe_loop(
            self.client.clone(),
            self.instance_id.clone(),
            local,
        ));
    }

    /// 이벤트를 다른 인스턴스로 보내도록 올리기 작업에 넘긴다. 기다리지 않는다.
    pub(super) fn publish(&self, channel_id: &str, event: &ChatEvent) {
        let envelope = Envelope {
            origin: self.instance_id.to_string(),
            event: event.into(),
        };
        let payload = match serde_json::to_string(&envelope) {
            Ok(payload) => payload,
            Err(e) => {
                tracing::error!("채팅 중계: 이벤트를 JSON으로 바꾸지 못했습니다: {e}");
                return;
            }
        };
        let channel = format!("{CHANNEL_PREFIX}{channel_id}");
        if let Err(e) = self.outbox.try_send((channel, payload)) {
            tracing::warn!("채팅 중계: 다른 인스턴스로 보낼 이벤트를 건너뜁니다({e})");
        }
    }
}

/// 올리기 작업: 넘겨받은 이벤트를 순서대로 PUBLISH한다. 연결이 끊기면 다음 이벤트 때 다시 연결한다.
async fn publish_loop(client: redis::Client, mut receiver: mpsc::Receiver<(String, String)>) {
    let mut connection: Option<redis::aio::MultiplexedConnection> = None;
    let mut retry_at: Option<Instant> = None;
    while let Some((channel, payload)) = receiver.recv().await {
        if connection.is_none() {
            if retry_at.is_some_and(|at| Instant::now() < at) {
                continue; // 방금 연결에 실패했다. 이 이벤트는 다른 인스턴스로 보내지 않는다.
            }
            match tokio::time::timeout(CONNECT_TIMEOUT, client.get_multiplexed_async_connection())
                .await
            {
                Ok(Ok(opened)) => {
                    tracing::info!("채팅 중계: Redis 올리기 연결");
                    connection = Some(opened);
                    retry_at = None;
                }
                Ok(Err(e)) => {
                    tracing::warn!("채팅 중계: Redis에 연결하지 못했습니다: {e}");
                    retry_at = Some(Instant::now() + PUBLISH_RETRY_AFTER);
                    continue;
                }
                Err(_) => {
                    tracing::warn!("채팅 중계: Redis 연결 시간이 지났습니다");
                    retry_at = Some(Instant::now() + PUBLISH_RETRY_AFTER);
                    continue;
                }
            }
        }
        let Some(open) = connection.as_mut() else {
            continue;
        };
        let published: redis::RedisResult<i64> = redis::cmd("PUBLISH")
            .arg(&channel)
            .arg(&payload)
            .query_async(open)
            .await;
        if let Err(e) = published {
            tracing::warn!("채팅 중계: Redis에 이벤트를 올리지 못했습니다: {e}");
            connection = None;
        }
    }
}

/// 구독 작업: `streamapp:chat:*`를 구독해 다른 인스턴스의 이벤트를 이 인스턴스의 참여자에게 보낸다.
/// 끊기면 기다렸다가(점점 길게, 최대 [`SUBSCRIBE_RETRY_MAX`]) 다시 연결한다. 앱이 끝날 때까지 돈다.
async fn subscribe_loop(
    client: redis::Client,
    instance_id: std::sync::Arc<str>,
    local: LocalChats,
) {
    let pattern = format!("{CHANNEL_PREFIX}*");
    let mut wait = SUBSCRIBE_RETRY_MIN;
    loop {
        match tokio::time::timeout(CONNECT_TIMEOUT, client.get_async_pubsub()).await {
            Ok(Ok(mut pubsub)) => match pubsub.psubscribe(&pattern).await {
                Ok(()) => {
                    tracing::info!("채팅 중계: Redis 구독 시작({pattern})");
                    wait = SUBSCRIBE_RETRY_MIN;
                    let mut messages = pubsub.on_message();
                    while let Some(message) = messages.next().await {
                        let Some(channel_id) =
                            message.get_channel_name().strip_prefix(CHANNEL_PREFIX)
                        else {
                            continue;
                        };
                        match message.get_payload::<String>() {
                            Ok(payload) => deliver(&local, &instance_id, channel_id, &payload),
                            Err(e) => tracing::warn!("채팅 중계: 읽을 수 없는 메시지: {e}"),
                        }
                    }
                    tracing::warn!("채팅 중계: Redis 구독이 끊겼습니다");
                }
                Err(e) => tracing::warn!("채팅 중계: Redis 구독을 시작하지 못했습니다: {e}"),
            },
            Ok(Err(e)) => tracing::warn!("채팅 중계: Redis에 연결하지 못했습니다: {e}"),
            Err(_) => tracing::warn!("채팅 중계: Redis 연결 시간이 지났습니다"),
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(SUBSCRIBE_RETRY_MAX);
    }
}

/// Redis에서 받은 이벤트 한 건을 이 인스턴스의 참여자에게 보낸다. 자기가 올린 것은 건너뛴다.
fn deliver(local: &LocalChats, instance_id: &str, channel_id: &str, payload: &str) {
    let envelope: Envelope = match serde_json::from_str(payload) {
        Ok(envelope) => envelope,
        Err(e) => {
            tracing::warn!("채팅 중계: 읽을 수 없는 이벤트: {e}");
            return;
        }
    };
    if envelope.origin == instance_id {
        return;
    }
    match envelope.event {
        WireEvent::Message(message) => local.send(channel_id, ChatEvent::Message(message)),
        WireEvent::LiveTitleChanged { title } => {
            local.send(channel_id, ChatEvent::LiveTitleChanged { title })
        }
        WireEvent::LiveEnded => local.end_live(channel_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_round_trips_every_event_kind() {
        let events = [
            ChatEvent::Message(ChatMessage {
                nickname: "영희".into(),
                content: "안녕하세요".into(),
                sent_at_ms: 7_000,
            }),
            ChatEvent::LiveTitleChanged {
                title: "새 제목".into(),
            },
            ChatEvent::LiveEnded,
        ];
        for event in &events {
            let envelope = Envelope {
                origin: "a".into(),
                event: event.into(),
            };
            let json = serde_json::to_string(&envelope).unwrap();
            assert_eq!(serde_json::from_str::<Envelope>(&json).unwrap(), envelope);
        }
    }

    #[tokio::test]
    async fn remote_event_reaches_local_participants_but_own_event_is_skipped() {
        let local = LocalChats::default();
        let mut subscription = local.subscribe("ch");
        let own = serde_json::to_string(&Envelope {
            origin: "me".into(),
            event: WireEvent::LiveTitleChanged {
                title: "내 것".into(),
            },
        })
        .unwrap();
        let remote = serde_json::to_string(&Envelope {
            origin: "other".into(),
            event: WireEvent::LiveTitleChanged {
                title: "남의 것".into(),
            },
        })
        .unwrap();

        deliver(&local, "me", "ch", &own);
        deliver(&local, "me", "ch", &remote);
        deliver(&local, "me", "ch", "깨진 JSON");

        assert_eq!(
            subscription.next_event().await,
            Some(ChatEvent::LiveTitleChanged {
                title: "남의 것".into()
            })
        );
    }
}
