//! 경계 A: 앱 서비스. 테스트마다 새 메모리 SQLite를 쓰고, 서비스의 공개 동작으로만 확인한다.

use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};

use api::dto::{LiveChannel, MyChannelSettings};
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

// ---- 내 채널 설정 ----

#[tokio::test]
async fn channel_settings_give_obs_server_and_stream_key_values() {
    let svc = service().await;
    let created = svc.create_user("철수").await.unwrap();

    let settings = svc
        .my_channel_settings(created.user.id)
        .await
        .unwrap()
        .expect("사용자마다 채널이 있다");

    assert_eq!(settings.channel_id, created.channel_id);
    assert_eq!(settings.obs_server, "rtmp://localhost/");
    assert_eq!(
        settings.obs_stream_key,
        format!("{}?key={}", created.channel_id, settings.stream_key)
    );
}

#[tokio::test]
async fn creating_user_issues_hard_to_guess_stream_key_for_each_channel() {
    let svc = service().await;
    let a = svc.create_user("철수").await.unwrap().user;
    let b = svc.create_user("영희").await.unwrap().user;

    let key_a = svc
        .my_channel_settings(a.id)
        .await
        .unwrap()
        .unwrap()
        .stream_key;
    let key_b = svc
        .my_channel_settings(b.id)
        .await
        .unwrap()
        .unwrap()
        .stream_key;

    assert_ne!(key_a, key_b);
    for key in [&key_a, &key_b] {
        assert!(key.len() >= 32, "{key:?}");
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric()), "{key:?}");
    }
}

#[tokio::test]
async fn unknown_user_has_no_channel_settings() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;

    let settings = svc.my_channel_settings(chulsoo.id + 1).await.unwrap();

    assert_eq!(settings, None);
}

/// 그 사용자의 채널 설정에 보이는 다음 라이브 제목.
async fn next_live_title(svc: &AppService, user_id: i64) -> Option<String> {
    let settings = svc.my_channel_settings(user_id).await.unwrap();
    settings.expect("사용자마다 채널이 있다").next_live_title
}

#[tokio::test]
async fn next_live_title_is_empty_until_written() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;

    assert_eq!(next_live_title(&svc, chulsoo.id).await, None);
}

#[tokio::test]
async fn changed_next_live_title_shows_in_own_channel_settings_only() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;
    let younghee = svc.create_user("영희").await.unwrap().user;

    svc.change_next_live_title(chulsoo.id, "오늘은 마인크래프트")
        .await
        .unwrap();

    assert_eq!(
        next_live_title(&svc, chulsoo.id).await.as_deref(),
        Some("오늘은 마인크래프트")
    );
    assert_eq!(next_live_title(&svc, younghee.id).await, None);
}

#[tokio::test]
async fn next_live_title_of_100_characters_is_accepted() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;
    // 글자 수로 센다: 한글 100자는 300바이트지만 허용된다.
    let title = "가".repeat(100);

    svc.change_next_live_title(chulsoo.id, &title)
        .await
        .unwrap();

    assert_eq!(next_live_title(&svc, chulsoo.id).await, Some(title));
}

#[tokio::test]
async fn next_live_title_longer_than_100_characters_is_rejected() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;
    svc.change_next_live_title(chulsoo.id, "원래 제목")
        .await
        .unwrap();

    for title in ["가".repeat(101), "a".repeat(101)] {
        let result = svc.change_next_live_title(chulsoo.id, &title).await;
        assert!(
            matches!(result, Err(ServiceError::TitleTooLong)),
            "{} -> {result:?}",
            title.chars().count()
        );
    }
    assert_eq!(
        next_live_title(&svc, chulsoo.id).await.as_deref(),
        Some("원래 제목")
    );
}

#[tokio::test]
async fn next_live_title_is_trimmed_before_saving() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;

    let saved = svc
        .change_next_live_title(chulsoo.id, "  저녁 수다 \t")
        .await
        .unwrap();

    assert_eq!(saved.as_deref(), Some("저녁 수다"));
    assert_eq!(
        next_live_title(&svc, chulsoo.id).await.as_deref(),
        Some("저녁 수다")
    );
}

#[tokio::test]
async fn blank_next_live_title_clears_it() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;
    svc.change_next_live_title(chulsoo.id, "원래 제목")
        .await
        .unwrap();

    // 비워 두면 라이브가 시작될 때 기본 제목이 붙는다(제목 없는 라이브가 생기지 않게).
    for blank in ["", "   "] {
        let saved = svc.change_next_live_title(chulsoo.id, blank).await.unwrap();
        assert_eq!(saved, None, "{blank:?}");
        assert_eq!(next_live_title(&svc, chulsoo.id).await, None, "{blank:?}");
    }
}

#[tokio::test]
async fn next_live_title_of_unknown_user_is_rejected() {
    let svc = service().await;
    let chulsoo = svc.create_user("철수").await.unwrap().user;

    let result = svc.change_next_live_title(chulsoo.id + 1, "제목").await;

    assert!(
        matches!(result, Err(ServiceError::UnknownUser)),
        "{result:?}"
    );
}

// ---- 라이브: 송출 시작·종료 반영과 라이브 중인 채널 목록 ----

/// 테스트가 움직이는 시계. 처음에는 0(유닉스 밀리초)이다.
#[derive(Clone, Default)]
struct FakeClock(Arc<AtomicI64>);

impl FakeClock {
    fn set(&self, now_ms: i64) {
        self.0.store(now_ms, Ordering::SeqCst);
    }
}

/// 가짜 시계를 쓰는 서비스와 기본 사용자(철수·영희·민수)의 채널 ID.
async fn service_with_clock() -> (AppService, FakeClock, [String; 3]) {
    let clock = FakeClock::default();
    let time = clock.0.clone();
    let svc = AppService::with_clock(
        db::connect_memory().await.expect("메모리 DB 연결"),
        Arc::new(move || time.load(Ordering::SeqCst)),
    );
    let ids: Vec<String> = seeded_channels(&svc)
        .await
        .into_iter()
        .map(|c| c.channel_id)
        .collect();
    (svc, clock, ids.try_into().expect("기본 사용자는 3명"))
}

fn live(channel_id: &str, channel_name: &str, title: &str, started_at_ms: i64) -> LiveChannel {
    LiveChannel {
        channel_id: channel_id.to_string(),
        channel_name: channel_name.to_string(),
        title: title.to_string(),
        started_at_ms,
    }
}

#[tokio::test]
async fn publish_started_opens_live_titled_after_channel_name() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    clock.set(1_000);

    svc.publish_started(&chulsoo).await.unwrap();

    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&chulsoo, "철수", "철수님의 라이브", 1_000)]);
}

#[tokio::test]
async fn publish_ended_closes_live() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    svc.publish_started(&chulsoo).await.unwrap();
    clock.set(5_000);

    svc.publish_ended(&chulsoo).await.unwrap();

    assert_eq!(svc.live_channels().await.unwrap(), []);
}

#[tokio::test]
async fn live_channels_are_listed_most_recently_started_first() {
    let (svc, clock, [chulsoo, younghee, minsu]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&younghee).await.unwrap();
    clock.set(2_000);
    svc.publish_started(&minsu).await.unwrap();
    clock.set(3_000);
    svc.publish_started(&chulsoo).await.unwrap();

    let lives = svc.live_channels().await.unwrap();

    assert_eq!(
        lives,
        [
            live(&chulsoo, "철수", "철수님의 라이브", 3_000),
            live(&minsu, "민수", "민수님의 라이브", 2_000),
            live(&younghee, "영희", "영희님의 라이브", 1_000),
        ]
    );
}

#[tokio::test]
async fn publish_ended_closes_only_that_channels_live() {
    let (svc, clock, [chulsoo, younghee, _]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();
    clock.set(2_000);
    svc.publish_started(&younghee).await.unwrap();

    svc.publish_ended(&chulsoo).await.unwrap();

    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&younghee, "영희", "영희님의 라이브", 2_000)]);
}

#[tokio::test]
async fn publish_started_while_live_replaces_open_live_with_new_one() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();
    clock.set(2_000);

    svc.publish_started(&chulsoo).await.unwrap();

    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&chulsoo, "철수", "철수님의 라이브", 2_000)]);
}

#[tokio::test]
async fn publishing_again_after_it_ended_starts_new_live() {
    let (svc, clock, [chulsoo, younghee, _]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();
    clock.set(2_000);
    svc.publish_started(&younghee).await.unwrap();
    clock.set(3_000);
    svc.publish_ended(&chulsoo).await.unwrap();
    // 유예 시간이 없다: 바로 다시 붙어도 새 라이브다.

    svc.publish_started(&chulsoo).await.unwrap();

    let lives = svc.live_channels().await.unwrap();
    assert_eq!(
        lives,
        [
            live(&chulsoo, "철수", "철수님의 라이브", 3_000),
            live(&younghee, "영희", "영희님의 라이브", 2_000),
        ]
    );
}

#[tokio::test]
async fn publish_ended_without_open_live_changes_nothing() {
    let (svc, clock, [chulsoo, younghee, _]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();

    svc.publish_ended(&younghee).await.unwrap();
    svc.publish_ended("nosuchchanne").await.unwrap();

    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&chulsoo, "철수", "철수님의 라이브", 1_000)]);
}

#[tokio::test]
async fn publish_started_on_unknown_channel_is_rejected() {
    let (svc, _clock, _) = service_with_clock().await;

    let result = svc.publish_started("nosuchchanne").await;

    assert!(
        matches!(result, Err(ServiceError::UnknownChannel)),
        "{result:?}"
    );
    assert_eq!(svc.live_channels().await.unwrap(), []);
}

// ---- 라이브 제목: 다음 라이브 제목 복사와 라이브 중 변경 ----

/// 기본 사용자 중 그 닉네임을 가진 사용자의 ID.
async fn user_id(svc: &AppService, nickname: &str) -> i64 {
    let users = svc.list_users().await.unwrap();
    let user = users.into_iter().find(|u| u.nickname == nickname);
    user.expect("기본 사용자").id
}

#[tokio::test]
async fn publish_started_opens_live_titled_with_next_live_title() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    let chulsoo_user = user_id(&svc, "철수").await;
    svc.change_next_live_title(chulsoo_user, "오늘은 마인크래프트")
        .await
        .unwrap();
    clock.set(1_000);

    svc.publish_started(&chulsoo).await.unwrap();

    let lives = svc.live_channels().await.unwrap();
    assert_eq!(
        lives,
        [live(&chulsoo, "철수", "오늘은 마인크래프트", 1_000)]
    );
}

#[tokio::test]
async fn changed_live_title_shows_in_live_channels() {
    let (svc, clock, [chulsoo, younghee, _]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();
    clock.set(2_000);
    svc.publish_started(&younghee).await.unwrap();

    let saved = svc
        .change_live_title(user_id(&svc, "철수").await, "주제를 바꿨어요")
        .await
        .unwrap();

    assert_eq!(saved, "주제를 바꿨어요");
    // 시작 시각은 그대로이고, 다른 채널의 라이브 제목은 바뀌지 않는다.
    let lives = svc.live_channels().await.unwrap();
    assert_eq!(
        lives,
        [
            live(&younghee, "영희", "영희님의 라이브", 2_000),
            live(&chulsoo, "철수", "주제를 바꿨어요", 1_000),
        ]
    );
}

#[tokio::test]
async fn live_title_cannot_be_changed_when_not_live() {
    let (svc, clock, [chulsoo, younghee, _]) = service_with_clock().await;
    let chulsoo_user = user_id(&svc, "철수").await;
    // 한 번도 라이브를 하지 않은 채널(영희는 라이브 중이지만 남의 채널이다)
    clock.set(1_000);
    svc.publish_started(&younghee).await.unwrap();

    let never_live = svc.change_live_title(chulsoo_user, "새 제목").await;

    assert!(
        matches!(never_live, Err(ServiceError::NotLive)),
        "{never_live:?}"
    );

    // 라이브가 끝난 뒤
    svc.publish_started(&chulsoo).await.unwrap();
    svc.publish_ended(&chulsoo).await.unwrap();

    let ended = svc.change_live_title(chulsoo_user, "새 제목").await;

    assert!(matches!(ended, Err(ServiceError::NotLive)), "{ended:?}");
    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&younghee, "영희", "영희님의 라이브", 1_000)]);
}

#[tokio::test]
async fn live_title_of_100_characters_is_accepted() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();
    // 글자 수로 센다: 한글 100자는 300바이트지만 허용된다.
    let title = "가".repeat(100);

    svc.change_live_title(user_id(&svc, "철수").await, &title)
        .await
        .unwrap();

    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&chulsoo, "철수", &title, 1_000)]);
}

#[tokio::test]
async fn live_title_longer_than_100_characters_is_rejected() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    let chulsoo_user = user_id(&svc, "철수").await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();

    for title in ["가".repeat(101), "a".repeat(101)] {
        let result = svc.change_live_title(chulsoo_user, &title).await;
        assert!(
            matches!(result, Err(ServiceError::TitleTooLong)),
            "{} -> {result:?}",
            title.chars().count()
        );
    }
    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&chulsoo, "철수", "철수님의 라이브", 1_000)]);
}

#[tokio::test]
async fn live_title_is_trimmed_before_saving() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();

    let saved = svc
        .change_live_title(user_id(&svc, "철수").await, "  저녁 수다 \t")
        .await
        .unwrap();

    assert_eq!(saved, "저녁 수다");
    let lives = svc.live_channels().await.unwrap();
    assert_eq!(lives, [live(&chulsoo, "철수", "저녁 수다", 1_000)]);
}

#[tokio::test]
async fn blank_live_title_falls_back_to_default_title() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    let chulsoo_user = user_id(&svc, "철수").await;
    svc.change_next_live_title(chulsoo_user, "원래 제목")
        .await
        .unwrap();
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();

    // 제목 없는 라이브가 생기지 않게, 비우면 다음 라이브 제목 없이 시작했을 때와 같은 기본 제목이 된다.
    for blank in ["", "   "] {
        let saved = svc.change_live_title(chulsoo_user, blank).await.unwrap();

        assert_eq!(saved, "철수님의 라이브", "{blank:?}");
        let lives = svc.live_channels().await.unwrap();
        assert_eq!(lives, [live(&chulsoo, "철수", "철수님의 라이브", 1_000)]);
    }
}

#[tokio::test]
async fn live_title_of_unknown_user_is_rejected() {
    let (svc, _clock, _) = service_with_clock().await;
    let last_user = svc.list_users().await.unwrap().last().unwrap().id;

    let result = svc.change_live_title(last_user + 1, "제목").await;

    assert!(
        matches!(result, Err(ServiceError::UnknownUser)),
        "{result:?}"
    );
}

/// 그 사용자의 채널 설정에 보이는 진행 중인 라이브 제목.
async fn live_title_in_settings(svc: &AppService, user_id: i64) -> Option<String> {
    let settings = svc.my_channel_settings(user_id).await.unwrap();
    settings.expect("사용자마다 채널이 있다").live_title
}

#[tokio::test]
async fn channel_settings_give_live_title_only_while_live() {
    let (svc, clock, [chulsoo, younghee, _]) = service_with_clock().await;
    let chulsoo_user = user_id(&svc, "철수").await;
    svc.change_next_live_title(chulsoo_user, "오늘은 마인크래프트")
        .await
        .unwrap();
    clock.set(1_000);
    svc.publish_started(&younghee).await.unwrap();

    assert_eq!(live_title_in_settings(&svc, chulsoo_user).await, None);

    svc.publish_started(&chulsoo).await.unwrap();

    assert_eq!(
        live_title_in_settings(&svc, chulsoo_user).await.as_deref(),
        Some("오늘은 마인크래프트")
    );

    svc.publish_ended(&chulsoo).await.unwrap();

    assert_eq!(live_title_in_settings(&svc, chulsoo_user).await, None);
}

#[tokio::test]
async fn changing_live_title_leaves_next_live_title_alone() {
    let (svc, clock, [chulsoo, ..]) = service_with_clock().await;
    let chulsoo_user = user_id(&svc, "철수").await;
    svc.change_next_live_title(chulsoo_user, "오늘은 마인크래프트")
        .await
        .unwrap();
    clock.set(1_000);
    svc.publish_started(&chulsoo).await.unwrap();

    svc.change_live_title(chulsoo_user, "주제를 바꿨어요")
        .await
        .unwrap();

    let settings = svc
        .my_channel_settings(chulsoo_user)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(settings.live_title.as_deref(), Some("주제를 바꿨어요"));
    assert_eq!(
        settings.next_live_title.as_deref(),
        Some("오늘은 마인크래프트")
    );
}
