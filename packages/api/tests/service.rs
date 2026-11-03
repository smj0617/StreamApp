//! 경계 A: 앱 서비스. 테스트마다 새 메모리 SQLite를 쓰고, 서비스의 공개 동작으로만 확인한다.

use api::dto::MyChannelSettings;
use api::service::{db, AppService, ServiceError};

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

// ---- 사용자 만들기 ----

#[tokio::test]
async fn creating_user_also_creates_channel_named_after_nickname() {
    let svc = service().await;

    let created = svc.create_user("철수").await.unwrap();

    assert_eq!(created.user.nickname, "철수");
    assert_eq!(created.channel_name, "철수");
    let users = svc.list_users().await.unwrap();
    assert_eq!(users, [created.user]);
}

#[tokio::test]
async fn each_created_user_gets_own_url_safe_channel_id() {
    let svc = service().await;

    let a = svc.create_user("철수").await.unwrap();
    let b = svc.create_user("영희").await.unwrap();

    assert_ne!(a.channel_id, b.channel_id);
    for id in [&a.channel_id, &b.channel_id] {
        assert!(!id.is_empty());
        assert!(id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}

#[tokio::test]
async fn nickname_is_trimmed_before_saving() {
    let svc = service().await;

    let created = svc.create_user("  Alice \t").await.unwrap();

    assert_eq!(created.user.nickname, "Alice");
    assert_eq!(created.channel_name, "Alice");
}

#[tokio::test]
async fn nickname_of_2_to_10_characters_is_accepted() {
    let svc = service().await;

    // 글자 수로 센다: 한글 10자는 30바이트지만 허용된다.
    for nickname in ["ab", "가나", "가나다라마바사아자차", "abcdefghij"] {
        let created = svc.create_user(nickname).await.unwrap();
        assert_eq!(created.user.nickname, nickname);
    }
}

#[tokio::test]
async fn nickname_shorter_than_2_or_longer_than_10_characters_is_rejected() {
    let svc = service().await;

    // 길이는 앞뒤 공백을 지운 뒤에 센다.
    for nickname in [
        "",
        "   ",
        "a",
        "  가  ",
        "abcdefghijk",
        "가나다라마바사아자차카",
    ] {
        let result = svc.create_user(nickname).await;
        assert!(
            matches!(result, Err(ServiceError::NicknameLength)),
            "{nickname:?} -> {result:?}"
        );
    }
    assert!(svc.list_users().await.unwrap().is_empty());
}

#[tokio::test]
async fn nickname_of_hangul_latin_letters_and_digits_is_accepted() {
    let svc = service().await;

    let created = svc.create_user("Kim철수99").await.unwrap();

    assert_eq!(created.user.nickname, "Kim철수99");
}

#[tokio::test]
async fn nickname_with_other_characters_is_rejected() {
    let svc = service().await;

    for nickname in [
        "철 수",   // 가운데 공백
        "chul_su", // 기호
        "a-b",
        "철수!",
        "ㄱㄴㄷ", // 한글 자모만
        "café",   // 영문 외 라틴 문자
        "あいう", // 다른 문자 체계
        "１２３", // 전각 숫자
        "철수😀",
    ] {
        let result = svc.create_user(nickname).await;
        assert!(
            matches!(result, Err(ServiceError::NicknameCharacters)),
            "{nickname:?} -> {result:?}"
        );
    }
    assert!(svc.list_users().await.unwrap().is_empty());
}

#[tokio::test]
async fn nickname_differing_only_in_ascii_case_is_rejected_as_duplicate() {
    let svc = service().await;
    let alice = svc.create_user("Alice").await.unwrap();

    for nickname in ["Alice", "alice", "ALICE", "  aLiCe "] {
        let result = svc.create_user(nickname).await;
        assert!(
            matches!(result, Err(ServiceError::DuplicateNickname)),
            "{nickname:?} -> {result:?}"
        );
    }
    assert_eq!(svc.list_users().await.unwrap(), [alice.user]);
}

#[tokio::test]
async fn nickname_of_default_user_is_rejected_as_duplicate() {
    let svc = service().await;
    svc.seed_default_users().await.unwrap();

    let result = svc.create_user("철수").await;

    assert!(matches!(result, Err(ServiceError::DuplicateNickname)));
}

// ---- 사용자 고르기 ----

#[tokio::test]
async fn existing_user_can_be_found_by_id() {
    let svc = service().await;
    svc.create_user("철수").await.unwrap();
    let younghee = svc.create_user("영희").await.unwrap().user;

    let found = svc.find_user(younghee.id).await.unwrap();

    assert_eq!(found, Some(younghee));
}

#[tokio::test]
async fn unknown_user_id_finds_nobody() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;

    let found = svc.find_user(chulsoo.id + 1).await.unwrap();

    assert_eq!(found, None);
}

// ---- 송출 허용 판단 ----

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
