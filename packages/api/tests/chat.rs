//! 경계 A: 앱 서비스의 채팅(채팅 참여, 채팅 보내기, 라이브 종료 알림). 테스트마다 새 메모리 SQLite를
//! 쓰고, 서비스의 공개 동작(사용자 만들기, 송출 시작·종료 반영, 채팅 참여·보내기)으로만 확인한다.

mod common;

use std::time::Duration;

use api::dto::ChatMessage;
use api::service::{ChatEvent, ChatSubscription, PublishingStatus, ServiceError};
use common::{service_with_clock, user};

/// 구독의 다음 이벤트. 오지 않으면 테스트가 멈추지 않게 1초 뒤 실패한다.
async fn next_event(subscription: &mut ChatSubscription) -> Option<ChatEvent> {
    tokio::time::timeout(Duration::from_secs(1), subscription.next_event())
        .await
        .expect("1초 안에 이벤트가 와야 한다")
}

fn message(nickname: &str, content: &str, sent_at_ms: i64) -> ChatEvent {
    ChatEvent::Message(ChatMessage {
        nickname: nickname.to_string(),
        content: content.to_string(),
        sent_at_ms,
    })
}

#[tokio::test]
async fn both_subscriptions_on_same_channel_receive_message_with_nickname_and_time() {
    let (svc, clock) = service_with_clock().await;
    let (_, chulsoo_channel) = user(&svc, "철수").await;
    let (younghee, _) = user(&svc, "영희").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut first = svc.join_chat(&chulsoo_channel).await.unwrap();
    let mut second = svc.join_chat(&chulsoo_channel).await.unwrap();
    clock.set(7_000);

    svc.send_chat(younghee, &chulsoo_channel, "안녕하세요")
        .await
        .unwrap();

    for subscription in [&mut first, &mut second] {
        assert_eq!(
            next_event(subscription).await,
            Some(message("영희", "안녕하세요", 7_000))
        );
    }
}

#[tokio::test]
async fn subscription_on_other_channel_does_not_receive_message() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    let (younghee, younghee_channel) = user(&svc, "영희").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    svc.publish_started(&younghee_channel).await.unwrap();
    let mut younghee_viewer = svc.join_chat(&younghee_channel).await.unwrap();

    svc.send_chat(younghee, &chulsoo_channel, "철수 채널에 보냄")
        .await
        .unwrap();
    // 채널 안에서는 보낸 순서대로 받으므로, 영희 채널의 다음 이벤트가 이 메시지면 앞의 메시지는 오지 않은 것이다.
    svc.send_chat(chulsoo, &younghee_channel, "영희 채널에 보냄")
        .await
        .unwrap();

    assert_eq!(
        next_event(&mut younghee_viewer).await,
        Some(message("철수", "영희 채널에 보냄", 0))
    );
}

// ---- 채팅 참여: 라이브 중이 아니면 거절 ----

#[tokio::test]
async fn joining_chat_of_channel_that_never_went_live_is_rejected() {
    let (svc, _) = service_with_clock().await;
    let (_, chulsoo_channel) = user(&svc, "철수").await;
    let (_, younghee_channel) = user(&svc, "영희").await;
    svc.publish_started(&younghee_channel).await.unwrap();

    let result = svc.join_chat(&chulsoo_channel).await;

    assert!(matches!(result, Err(ServiceError::NotLive)));
}

#[tokio::test]
async fn joining_chat_after_live_ended_is_rejected() {
    let (svc, _) = service_with_clock().await;
    let (_, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    svc.publish_ended(&chulsoo_channel).await.unwrap();

    let result = svc.join_chat(&chulsoo_channel).await;

    assert!(matches!(result, Err(ServiceError::NotLive)));
}

#[tokio::test]
async fn joining_chat_of_unknown_channel_is_rejected_as_not_live() {
    let (svc, _) = service_with_clock().await;
    user(&svc, "철수").await;

    let result = svc.join_chat("nosuchchanne").await;

    assert!(matches!(result, Err(ServiceError::NotLive)));
}

// ---- 채팅 보내기: 거절 규칙 ----

#[tokio::test]
async fn sending_to_channel_that_is_not_live_is_rejected() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    let (_, younghee_channel) = user(&svc, "영희").await;
    svc.publish_started(&younghee_channel).await.unwrap();
    svc.publish_ended(&younghee_channel).await.unwrap();

    for channel in [&chulsoo_channel, &younghee_channel, "nosuchchanne"] {
        let result = svc.send_chat(chulsoo, channel, "안녕하세요").await;
        assert!(
            matches!(result, Err(ServiceError::NotLive)),
            "{channel} -> {result:?}"
        );
    }
}

#[tokio::test]
async fn sending_as_unknown_user_is_rejected_and_not_delivered() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut viewer = svc.join_chat(&chulsoo_channel).await.unwrap();

    let result = svc.send_chat(chulsoo + 1, &chulsoo_channel, "누구게").await;

    assert!(
        matches!(result, Err(ServiceError::UnknownUser)),
        "{result:?}"
    );
    svc.send_chat(chulsoo, &chulsoo_channel, "다음 메시지")
        .await
        .unwrap();
    assert_eq!(
        next_event(&mut viewer).await,
        Some(message("철수", "다음 메시지", 0))
    );
}

#[tokio::test]
async fn blank_message_is_rejected_and_not_delivered() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut viewer = svc.join_chat(&chulsoo_channel).await.unwrap();

    for blank in ["", "   ", " \t\n "] {
        let result = svc.send_chat(chulsoo, &chulsoo_channel, blank).await;
        assert!(
            matches!(result, Err(ServiceError::ChatMessageEmpty)),
            "{blank:?} -> {result:?}"
        );
    }

    svc.send_chat(chulsoo, &chulsoo_channel, "다음 메시지")
        .await
        .unwrap();
    assert_eq!(
        next_event(&mut viewer).await,
        Some(message("철수", "다음 메시지", 0))
    );
}

#[tokio::test]
async fn message_longer_than_200_characters_is_rejected_and_not_delivered() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut viewer = svc.join_chat(&chulsoo_channel).await.unwrap();

    for content in ["가".repeat(201), "a".repeat(201)] {
        let result = svc.send_chat(chulsoo, &chulsoo_channel, &content).await;
        assert!(
            matches!(result, Err(ServiceError::ChatMessageTooLong)),
            "{} -> {result:?}",
            content.chars().count()
        );
    }

    svc.send_chat(chulsoo, &chulsoo_channel, "다음 메시지")
        .await
        .unwrap();
    assert_eq!(
        next_event(&mut viewer).await,
        Some(message("철수", "다음 메시지", 0))
    );
}

#[tokio::test]
async fn message_is_trimmed_and_200_characters_after_trimming_is_accepted() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut viewer = svc.join_chat(&chulsoo_channel).await.unwrap();
    // 글자 수로 센다: 한글 200자는 600바이트지만 허용된다. 앞뒤 공백은 세지 않는다.
    let longest = "가".repeat(200);

    let sent = svc
        .send_chat(chulsoo, &chulsoo_channel, &format!("  {longest} \t"))
        .await
        .unwrap();
    svc.send_chat(chulsoo, &chulsoo_channel, "  안녕  ")
        .await
        .unwrap();

    assert_eq!(sent.content, longest);
    assert_eq!(
        next_event(&mut viewer).await,
        Some(message("철수", &longest, 0))
    );
    assert_eq!(
        next_event(&mut viewer).await,
        Some(message("철수", "안녕", 0))
    );
}

// ---- 라이브 제목 바뀜 알림 ----

fn title_changed(title: &str) -> ChatEvent {
    ChatEvent::LiveTitleChanged {
        title: title.to_string(),
    }
}

#[tokio::test]
async fn changing_live_title_tells_every_participant_the_saved_title() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut first = svc.join_chat(&chulsoo_channel).await.unwrap();
    let mut second = svc.join_chat(&chulsoo_channel).await.unwrap();

    svc.change_live_title(chulsoo, "  새 제목  ").await.unwrap();
    svc.change_live_title(chulsoo, "").await.unwrap();

    for subscription in [&mut first, &mut second] {
        // 저장한 제목 그대로: 앞뒤 공백을 지우고, 비우면 자동 제목이다.
        assert_eq!(
            next_event(subscription).await,
            Some(title_changed("새 제목"))
        );
        assert_eq!(
            next_event(subscription).await,
            Some(title_changed("철수님의 라이브"))
        );
    }
}

#[tokio::test]
async fn changing_live_title_does_not_tell_participants_of_other_channel() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    let (younghee, younghee_channel) = user(&svc, "영희").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    svc.publish_started(&younghee_channel).await.unwrap();
    let mut younghee_viewer = svc.join_chat(&younghee_channel).await.unwrap();

    svc.change_live_title(chulsoo, "철수의 새 제목")
        .await
        .unwrap();
    svc.send_chat(younghee, &younghee_channel, "저는 그대로예요")
        .await
        .unwrap();

    assert_eq!(
        next_event(&mut younghee_viewer).await,
        Some(message("영희", "저는 그대로예요", 0))
    );
}

#[tokio::test]
async fn rejected_live_title_change_tells_nobody() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut viewer = svc.join_chat(&chulsoo_channel).await.unwrap();

    let result = svc.change_live_title(chulsoo, &"가".repeat(101)).await;
    svc.send_chat(chulsoo, &chulsoo_channel, "제목은 그대로예요")
        .await
        .unwrap();

    assert!(
        matches!(result, Err(ServiceError::TitleTooLong)),
        "{result:?}"
    );
    assert_eq!(
        next_event(&mut viewer).await,
        Some(message("철수", "제목은 그대로예요", 0))
    );
}

// ---- 라이브 종료 알림 ----

#[tokio::test]
async fn publish_ended_tells_every_participant_live_ended_and_ends_subscriptions() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut first = svc.join_chat(&chulsoo_channel).await.unwrap();
    let mut second = svc.join_chat(&chulsoo_channel).await.unwrap();
    svc.send_chat(chulsoo, &chulsoo_channel, "곧 끝낼게요")
        .await
        .unwrap();

    svc.publish_ended(&chulsoo_channel).await.unwrap();

    for subscription in [&mut first, &mut second] {
        assert_eq!(
            next_event(subscription).await,
            Some(message("철수", "곧 끝낼게요", 0))
        );
        assert_eq!(next_event(subscription).await, Some(ChatEvent::LiveEnded));
        assert_eq!(next_event(subscription).await, None);
    }
}

#[tokio::test]
async fn publish_ended_does_not_tell_participants_of_other_channel() {
    let (svc, _) = service_with_clock().await;
    let (_, chulsoo_channel) = user(&svc, "철수").await;
    let (younghee, younghee_channel) = user(&svc, "영희").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    svc.publish_started(&younghee_channel).await.unwrap();
    let mut younghee_viewer = svc.join_chat(&younghee_channel).await.unwrap();

    svc.publish_ended(&chulsoo_channel).await.unwrap();
    svc.send_chat(younghee, &younghee_channel, "저는 계속해요")
        .await
        .unwrap();

    assert_eq!(
        next_event(&mut younghee_viewer).await,
        Some(message("영희", "저는 계속해요", 0))
    );
}

#[tokio::test]
async fn reconciling_that_closes_ghost_live_tells_its_participants_live_ended() {
    let (svc, _) = service_with_clock().await;
    let (_, chulsoo_channel) = user(&svc, "철수").await;
    let (younghee, younghee_channel) = user(&svc, "영희").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    svc.publish_started(&younghee_channel).await.unwrap();
    let mut chulsoo_viewer = svc.join_chat(&chulsoo_channel).await.unwrap();
    let mut younghee_viewer = svc.join_chat(&younghee_channel).await.unwrap();

    // 영희만 계속 송출 중이다: 철수의 라이브는 종료 알림을 놓친 유령 라이브다.
    svc.reconcile_publishing(PublishingStatus::Known(vec![younghee_channel.clone()]))
        .await
        .unwrap();
    svc.send_chat(younghee, &younghee_channel, "저는 계속해요")
        .await
        .unwrap();

    assert_eq!(
        next_event(&mut chulsoo_viewer).await,
        Some(ChatEvent::LiveEnded)
    );
    assert_eq!(
        next_event(&mut younghee_viewer).await,
        Some(message("영희", "저는 계속해요", 0))
    );
}

#[tokio::test]
async fn publish_started_while_live_tells_participants_of_replaced_live_that_it_ended() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut old_viewer = svc.join_chat(&chulsoo_channel).await.unwrap();

    // 송출 종료 알림을 놓친 채 송출 시작 알림이 다시 왔다: 열린 라이브를 닫고 새 라이브를 연다.
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut new_viewer = svc.join_chat(&chulsoo_channel).await.unwrap();
    svc.send_chat(chulsoo, &chulsoo_channel, "새 라이브예요")
        .await
        .unwrap();

    // 닫힌 라이브의 참여자는 송출 종료 때처럼 종료 알림으로 끝나고 새 라이브의 메시지를 받지 않는다.
    assert_eq!(
        next_event(&mut old_viewer).await,
        Some(ChatEvent::LiveEnded)
    );
    assert_eq!(next_event(&mut old_viewer).await, None);
    assert_eq!(
        next_event(&mut new_viewer).await,
        Some(message("철수", "새 라이브예요", 0))
    );
}

#[tokio::test]
async fn chat_of_new_live_on_same_channel_can_be_joined_after_previous_live_ended() {
    let (svc, _) = service_with_clock().await;
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    svc.publish_started(&chulsoo_channel).await.unwrap();
    let mut old_viewer = svc.join_chat(&chulsoo_channel).await.unwrap();
    svc.publish_ended(&chulsoo_channel).await.unwrap();
    svc.publish_started(&chulsoo_channel).await.unwrap();

    let mut new_viewer = svc.join_chat(&chulsoo_channel).await.unwrap();
    svc.send_chat(chulsoo, &chulsoo_channel, "다시 왔어요")
        .await
        .unwrap();

    assert_eq!(
        next_event(&mut new_viewer).await,
        Some(message("철수", "다시 왔어요", 0))
    );
    // 지난 라이브의 참여자는 종료 알림으로 끝나고 새 라이브의 메시지를 받지 않는다.
    assert_eq!(
        next_event(&mut old_viewer).await,
        Some(ChatEvent::LiveEnded)
    );
    assert_eq!(next_event(&mut old_viewer).await, None);
}
