//! 과제 ⑤: 앱 인스턴스 여럿이 같은 상태를 나눠 쓰는지. 인스턴스마다 따로 연결한 서비스 둘이 임시 폴더의 같은
//! 파일 DB를 쓴다(공유 볼륨의 SQLite와 같다). 휴대폰 방송 송출 세션은 `whip_http.rs`에서 본다.
//!
//! Redis 채팅 중계 테스트는 Redis가 있어야 해서 기본으로 건너뛴다. 이렇게 돌린다.
//!
//! ```text
//! docker run -d --rm -p 6379:6379 --name streamapp-test-redis redis:8.8.4-alpine
//! STREAMAPP_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test -p api --features server --test multi_instance -- --include-ignored
//! ```

mod common;

use std::time::Duration;

use api::dto::ChatMessage;
use api::service::{AppService, ChatEvent, ChatSubscription, PublishingStatus};
use common::{two_instances, user, PASSWORD};

#[tokio::test]
async fn instances_starting_together_migrate_and_seed_default_users_once() {
    let (first, second, _dir) = two_instances().await;

    let (a, b) = tokio::join!(
        first.seed_default_users(PASSWORD),
        second.seed_default_users(PASSWORD)
    );

    // 정확히 한 쪽만 기본 사용자를 만들었다.
    assert_eq!(
        [a.unwrap(), b.unwrap()]
            .into_iter()
            .filter(|seeded| *seeded)
            .count(),
        1
    );
    assert_eq!(first.list_users().await.unwrap().len(), 3);
    assert_eq!(second.list_users().await.unwrap().len(), 3);
}

#[tokio::test]
async fn session_started_on_one_instance_is_signed_in_on_the_other() {
    let (first, second, _dir) = two_instances().await;
    let (user_id, _) = user(&first, "철수").await;

    let token = first.start_session(user_id).await.unwrap();

    let found = second.session_user(&token).await.unwrap();
    assert_eq!(found.map(|user| user.id), Some(user_id));
}

#[tokio::test]
async fn live_started_by_hook_on_one_instance_is_live_on_the_other() {
    let (first, second, _dir) = two_instances().await;
    let (_, channel_id) = user(&first, "철수").await;

    first.publish_started(&channel_id).await.unwrap();
    assert_eq!(second.live_channels().await.unwrap().len(), 1);

    second.publish_ended(&channel_id).await.unwrap();
    assert!(first.live_channels().await.unwrap().is_empty());
}

#[tokio::test]
async fn new_instance_that_cannot_reach_mediamtx_keeps_lives_in_multi_instance_mode() {
    let (first, second, _dir) = two_instances().await;
    let (_, channel_id) = user(&first, "철수").await;
    first.publish_started(&channel_id).await.unwrap();

    // 스케일 아웃으로 뜬 둘째 인스턴스가 시작하며 MediaMTX에 닿지 못했다.
    second
        .reconcile_publishing(PublishingStatus::Unreachable.keeping_lives_if_unreachable())
        .await
        .unwrap();

    assert_eq!(first.live_channels().await.unwrap().len(), 1);
}

#[test]
fn only_unreachable_changes_in_multi_instance_mode() {
    assert_eq!(
        PublishingStatus::Unreachable.keeping_lives_if_unreachable(),
        PublishingStatus::Unknown
    );
    assert_eq!(
        PublishingStatus::Unknown.keeping_lives_if_unreachable(),
        PublishingStatus::Unknown
    );
    let known = PublishingStatus::Known(vec!["ch".into()]);
    assert_eq!(known.clone().keeping_lives_if_unreachable(), known);
}

#[tokio::test]
async fn malformed_redis_url_is_a_configuration_error() {
    let (first, _, _dir) = two_instances().await;
    assert!(first.with_chat_relay("not a url").is_err());
}

#[tokio::test]
async fn unreachable_redis_does_not_stop_chat_within_the_instance() {
    let (svc, _, _dir) = two_instances().await;
    // 띄웠다가 바로 닫은 포트: Redis 연결이 거절된다.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let svc = svc
        .with_chat_relay(&format!("redis://127.0.0.1:{port}"))
        .unwrap();
    let (_, channel_id) = user(&svc, "철수").await;
    let (younghee, _) = user(&svc, "영희").await;
    svc.publish_started(&channel_id).await.unwrap();
    let mut viewer = svc.join_chat(&channel_id).await.unwrap();

    let sent = svc
        .send_chat(younghee, &channel_id, "안녕하세요")
        .await
        .unwrap();

    assert_eq!(
        next_event(&mut viewer).await,
        Some(ChatEvent::Message(sent))
    );
}

/// 구독의 다음 이벤트. 오지 않으면 테스트가 멈추지 않게 3초 뒤 실패한다.
async fn next_event(subscription: &mut ChatSubscription) -> Option<ChatEvent> {
    tokio::time::timeout(Duration::from_secs(3), subscription.next_event())
        .await
        .expect("3초 안에 이벤트가 와야 한다")
}

/// 테스트용 Redis 주소(환경변수 `STREAMAPP_TEST_REDIS_URL`).
fn test_redis_url() -> String {
    std::env::var("STREAMAPP_TEST_REDIS_URL")
        .expect("Redis 통합 테스트에는 STREAMAPP_TEST_REDIS_URL이 필요합니다")
}

/// Redis 중계를 끼운 인스턴스 둘. 구독이 붙을 때까지 잠깐 기다린다.
async fn two_relayed_instances() -> (AppService, AppService, common::TempDir) {
    let (first, second, dir) = two_instances().await;
    let url = test_redis_url();
    let first = first.with_chat_relay(&url).unwrap();
    let second = second.with_chat_relay(&url).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    (first, second, dir)
}

#[tokio::test]
#[ignore = "Redis가 필요하다(STREAMAPP_TEST_REDIS_URL)"]
async fn chat_sent_on_one_instance_reaches_viewer_on_the_other_through_redis() {
    let (first, second, _dir) = two_relayed_instances().await;
    let (_, channel_id) = user(&first, "철수").await;
    let (younghee, _) = user(&first, "영희").await;
    first.publish_started(&channel_id).await.unwrap();
    let mut viewer_on_second = second.join_chat(&channel_id).await.unwrap();
    let mut viewer_on_first = first.join_chat(&channel_id).await.unwrap();

    let sent = first
        .send_chat(younghee, &channel_id, "다른 파드에서 보냄")
        .await
        .unwrap();

    let expected = Some(ChatEvent::Message(ChatMessage {
        nickname: "영희".into(),
        content: "다른 파드에서 보냄".into(),
        sent_at_ms: sent.sent_at_ms,
    }));
    assert_eq!(next_event(&mut viewer_on_second).await, expected);
    // 보낸 인스턴스의 참여자는 한 번만 받는다(자기가 올린 것은 Redis에서 다시 받지 않는다).
    assert_eq!(next_event(&mut viewer_on_first).await, expected);
    assert!(
        tokio::time::timeout(Duration::from_millis(500), viewer_on_first.next_event())
            .await
            .is_err()
    );
}

#[tokio::test]
#[ignore = "Redis가 필요하다(STREAMAPP_TEST_REDIS_URL)"]
async fn live_end_on_one_instance_ends_chat_on_the_other_through_redis() {
    let (first, second, _dir) = two_relayed_instances().await;
    let (_, channel_id) = user(&first, "철수").await;
    first.publish_started(&channel_id).await.unwrap();
    let mut viewer_on_second = second.join_chat(&channel_id).await.unwrap();

    first.publish_ended(&channel_id).await.unwrap();

    assert_eq!(
        next_event(&mut viewer_on_second).await,
        Some(ChatEvent::LiveEnded)
    );
    assert_eq!(next_event(&mut viewer_on_second).await, None);
}

#[tokio::test]
async fn responses_carry_the_instance_name_when_given() {
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt; // oneshot

    let app = || Router::new().route("/", get(|| async { "ok" }));

    let tagged = api::instance::tag_responses(app(), Some("streamapp-abc"))
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        tagged.headers()[api::instance::INSTANCE_HEADER],
        "streamapp-abc"
    );

    let untagged = api::instance::tag_responses(app(), None)
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(!untagged
        .headers()
        .contains_key(api::instance::INSTANCE_HEADER));
}
