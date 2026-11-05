//! 경계 A: 앱 서비스의 회원가입·로그인·로그인 세션(과제 ③). 테스트마다 새 메모리 SQLite를 쓰고,
//! 서비스의 공개 동작으로만 확인한다.

mod common;

use api::service::{ServiceError, SESSION_LIFETIME_MS};
use common::{service, service_with_clock, PASSWORD};

#[tokio::test]
async fn signed_up_user_can_log_in_with_password() {
    let svc = service().await;
    let created = svc.sign_up("Alice", PASSWORD).await.unwrap();

    let user = svc.log_in("Alice", PASSWORD).await.unwrap();

    assert_eq!(user, created.user);
}

#[tokio::test]
async fn login_ignores_ascii_case_and_spaces_of_nickname_like_sign_up() {
    let svc = service().await;
    let created = svc.sign_up("Alice", PASSWORD).await.unwrap();

    let user = svc.log_in("  aLICE ", PASSWORD).await.unwrap();

    assert_eq!(user, created.user);
}

#[tokio::test]
async fn wrong_password_or_unknown_nickname_fails_with_same_reason() {
    let svc = service().await;
    svc.sign_up("철수", PASSWORD).await.unwrap();

    let wrong_password = svc.log_in("철수", "test12345").await;
    let unknown = svc.log_in("영희", PASSWORD).await;

    for result in [wrong_password, unknown] {
        let error = result.unwrap_err();
        assert!(matches!(error, ServiceError::LoginFailed), "{error:?}");
        assert_eq!(error.to_string(), "닉네임 또는 비밀번호가 맞지 않습니다");
        assert_eq!(error.status_code(), 401);
    }
}

#[tokio::test]
async fn password_must_be_8_to_64_chars() {
    let svc = service().await;

    for password in ["1234567", &"a".repeat(65)] {
        let result = svc.sign_up("철수", password).await;
        assert!(
            matches!(result, Err(ServiceError::PasswordLength)),
            "{password}: {result:?}"
        );
    }
    assert!(svc.list_users().await.unwrap().is_empty());

    // 경계 값과 한글·공백이 섞인 비밀번호는 받는다.
    svc.sign_up("철수", "12345678").await.unwrap();
    svc.sign_up("영희", &"가".repeat(64)).await.unwrap();
    svc.sign_up("민수", " 앞뒤 공백 포함 ").await.unwrap();
    assert!(svc.log_in("민수", " 앞뒤 공백 포함 ").await.is_ok());
    assert!(svc.log_in("민수", "앞뒤 공백 포함").await.is_err());
}

#[tokio::test]
async fn sign_up_still_follows_nickname_rules_before_hashing() {
    let svc = service().await;
    svc.sign_up("철수", PASSWORD).await.unwrap();

    let duplicate = svc.sign_up("철수", PASSWORD).await;
    let short = svc.sign_up("철", PASSWORD).await;

    assert!(
        matches!(duplicate, Err(ServiceError::DuplicateNickname)),
        "{duplicate:?}"
    );
    assert!(
        matches!(short, Err(ServiceError::NicknameLength)),
        "{short:?}"
    );
}

#[tokio::test]
async fn session_token_finds_its_user_until_logout() {
    let svc = service().await;
    let created = svc.sign_up("철수", PASSWORD).await.unwrap();

    let token = svc.start_session(created.user.id).await.unwrap();
    assert_eq!(
        svc.session_user(&token).await.unwrap(),
        Some(created.user.clone())
    );

    svc.end_session(&token).await.unwrap();
    assert_eq!(svc.session_user(&token).await.unwrap(), None);
    // 이미 끝난 세션을 다시 끝내도 된다.
    svc.end_session(&token).await.unwrap();
}

#[tokio::test]
async fn each_login_gets_its_own_session() {
    let svc = service().await;
    let chulsoo = svc.sign_up("철수", PASSWORD).await.unwrap().user;
    let younghee = svc.sign_up("영희", PASSWORD).await.unwrap().user;

    let phone = svc.start_session(chulsoo.id).await.unwrap();
    let pc = svc.start_session(chulsoo.id).await.unwrap();
    let other = svc.start_session(younghee.id).await.unwrap();
    svc.end_session(&phone).await.unwrap();

    assert_ne!(phone, pc);
    assert_eq!(svc.session_user(&pc).await.unwrap(), Some(chulsoo));
    assert_eq!(svc.session_user(&other).await.unwrap(), Some(younghee));
}

#[tokio::test]
async fn unknown_token_has_no_user() {
    let svc = service().await;
    svc.sign_up("철수", PASSWORD).await.unwrap();

    assert_eq!(svc.session_user("made-up-token").await.unwrap(), None);
    assert_eq!(svc.session_user("").await.unwrap(), None);
}

#[tokio::test]
async fn session_expires_after_its_lifetime() {
    let (svc, clock) = service_with_clock().await;
    let user = svc.sign_up("철수", PASSWORD).await.unwrap().user;
    clock.set(1_000);
    let token = svc.start_session(user.id).await.unwrap();

    clock.set(1_000 + SESSION_LIFETIME_MS - 1);
    assert_eq!(svc.session_user(&token).await.unwrap(), Some(user));

    clock.set(1_000 + SESSION_LIFETIME_MS);
    assert_eq!(svc.session_user(&token).await.unwrap(), None);
}

#[tokio::test]
async fn session_for_unknown_user_is_rejected() {
    let svc = service().await;

    let result = svc.start_session(999).await;

    assert!(
        matches!(result, Err(ServiceError::UnknownUser)),
        "{result:?}"
    );
}

#[tokio::test]
async fn default_users_log_in_with_given_password() {
    let svc = service().await;
    svc.seed_default_users("seed-pass").await.unwrap();

    for nickname in ["철수", "영희", "민수"] {
        let user = svc.log_in(nickname, "seed-pass").await.unwrap();
        assert_eq!(user.nickname, nickname);
    }
    assert!(svc.log_in("철수", PASSWORD).await.is_err());
}

#[tokio::test]
async fn filling_missing_passwords_leaves_existing_passwords_alone() {
    let svc = service().await;
    svc.sign_up("철수", PASSWORD).await.unwrap();

    let filled = svc.fill_missing_passwords("other-pass").await.unwrap();

    assert_eq!(filled, 0);
    assert!(svc.log_in("철수", PASSWORD).await.is_ok());
    assert!(svc.log_in("철수", "other-pass").await.is_err());
}

#[test]
fn errors_map_to_http_status_codes() {
    assert_eq!(ServiceError::NotSignedIn.status_code(), 401);
    assert_eq!(ServiceError::NotSignedIn.to_string(), "로그인이 필요합니다");
    assert_eq!(ServiceError::NotBroadcastOwner.status_code(), 403);
    assert_eq!(ServiceError::NotCommentAuthor.status_code(), 403);
    assert_eq!(ServiceError::UnknownRecording.status_code(), 404);
    assert_eq!(ServiceError::UnknownComment.status_code(), 404);
    assert_eq!(ServiceError::CommentEmpty.status_code(), 400);
    assert_eq!(ServiceError::PasswordLength.status_code(), 400);
}
