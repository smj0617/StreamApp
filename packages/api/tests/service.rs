//! 경계 A: 앱 서비스. 테스트마다 새 메모리 SQLite를 쓰고, 서비스의 공개 동작으로만 확인한다.

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
