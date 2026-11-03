//! 경계 A: 앱 서비스. 테스트마다 새 메모리 SQLite를 쓰고, 서비스의 공개 동작으로만 확인한다.

use api::dto::MyChannelSettings;
use api::service::{db, AppService};

async fn service() -> AppService {
    AppService::new(db::connect_memory().await.expect("메모리 DB 연결"))
}

fn nicknames(users: &[api::dto::User]) -> Vec<&str> {
    users.iter().map(|u| u.nickname.as_str()).collect()
}

#[tokio::test]
async fn empty_db_has_no_users() {
    let svc = service().await;

    let users = svc.list_users().await.unwrap();

    assert!(users.is_empty());
}

#[tokio::test]
async fn seeding_empty_db_creates_three_default_users() {
    let svc = service().await;

    svc.seed_default_users().await.unwrap();

    let users = svc.list_users().await.unwrap();
    assert_eq!(nicknames(&users), ["철수", "영희", "민수"]);
}

#[tokio::test]
async fn seeding_does_nothing_when_users_already_exist() {
    let svc = service().await;
    assert!(svc.seed_default_users().await.unwrap());

    let seeded_again = svc.seed_default_users().await.unwrap();

    assert!(!seeded_again);
    let users = svc.list_users().await.unwrap();
    assert_eq!(nicknames(&users), ["철수", "영희", "민수"]);
}

/// 기본 사용자를 넣고, 그 사용자들의 채널 설정(채널 ID, 송출 키)을 만든 순서대로 돌려준다.
async fn seeded_channels(svc: &AppService) -> Vec<MyChannelSettings> {
    svc.seed_default_users().await.unwrap();
    let mut channels = Vec::new();
    for user in svc.list_users().await.unwrap() {
        let settings = svc.my_channel_settings(user.id).await.unwrap();
        channels.push(settings.expect("사용자마다 채널이 있다"));
    }
    channels
}

#[tokio::test]
async fn right_stream_key_can_publish() {
    let svc = service().await;
    let channel = &seeded_channels(&svc).await[0];

    let allowed = svc
        .can_publish(&channel.channel_id, &channel.stream_key)
        .await
        .unwrap();

    assert!(allowed);
}

#[tokio::test]
async fn wrong_stream_key_cannot_publish() {
    let svc = service().await;
    let channel = &seeded_channels(&svc).await[0];

    let allowed = svc
        .can_publish(&channel.channel_id, "wrongStreamKey000000000000000000")
        .await
        .unwrap();

    assert!(!allowed);
}

#[tokio::test]
async fn another_channels_stream_key_cannot_publish() {
    let svc = service().await;
    let channels = seeded_channels(&svc).await;
    let (mine, other) = (&channels[0], &channels[1]);

    let allowed = svc
        .can_publish(&mine.channel_id, &other.stream_key)
        .await
        .unwrap();

    assert!(!allowed);
}

#[tokio::test]
async fn empty_stream_key_cannot_publish() {
    let svc = service().await;
    let channel = &seeded_channels(&svc).await[0];

    let allowed = svc.can_publish(&channel.channel_id, "").await.unwrap();

    assert!(!allowed);
}

#[tokio::test]
async fn unknown_channel_cannot_publish() {
    let svc = service().await;
    let channel = &seeded_channels(&svc).await[0];

    let allowed = svc
        .can_publish("nosuchchanne", &channel.stream_key)
        .await
        .unwrap();

    assert!(!allowed);
}

#[tokio::test]
async fn unknown_user_has_no_channel_settings() {
    let svc = service().await;
    seeded_channels(&svc).await;

    let settings = svc.my_channel_settings(9999).await.unwrap();

    assert_eq!(settings, None);
}
