//! 경계 A: 앱 서비스의 댓글(과제 ③). 테스트마다 새 메모리 SQLite를 쓰고, 서비스의 공개 동작으로만
//! 상태를 만든다. 로그인 확인은 서버 함수가 세션으로 하고, 여기서는 로그인한 사용자 ID로 부른다.

mod common;

use api::dto::{BroadcastTarget, Comment};
use api::service::{AppService, ServiceError};
use bytes::Bytes;
use common::{service_with_recordings, user, FakeClock, TempDir};
use futures_util::stream;

struct World {
    svc: AppService,
    clock: FakeClock,
    chulsoo: i64,
    younghee: i64,
    recording: BroadcastTarget,
    _dir: TempDir,
}

/// 철수가 녹화방송 하나를 올렸고, 영희도 가입해 있다.
async fn world() -> World {
    let (svc, clock, dir) = service_with_recordings().await;
    let (chulsoo, _) = user(&svc, "철수").await;
    let (younghee, _) = user(&svc, "영희").await;
    let id = svc
        .register_recording(
            chulsoo,
            "녹화",
            "clip.mp4",
            stream::iter(vec![Ok::<_, String>(Bytes::from_static(b"video"))]),
        )
        .await
        .unwrap();
    World {
        svc,
        clock,
        chulsoo,
        younghee,
        recording: BroadcastTarget::Recording(id),
        _dir: dir,
    }
}

#[tokio::test]
async fn written_comments_are_listed_oldest_first_with_author() {
    let w = world().await;
    w.clock.set(1_000);
    let first = w
        .svc
        .write_comment(w.younghee, w.recording, "  재밌어요 ")
        .await
        .unwrap();
    w.clock.set(2_000);
    let second = w
        .svc
        .write_comment(w.chulsoo, w.recording, "고마워요")
        .await
        .unwrap();
    w.clock.set(3_000);

    let list = w.svc.comments(w.recording).await.unwrap();

    assert_eq!(
        first,
        Comment {
            id: first.id,
            author_id: w.younghee,
            author_nickname: "영희".to_string(),
            content: "재밌어요".to_string(),
            created_at_ms: 1_000,
            edited: false,
        }
    );
    assert_eq!(list.comments, [first, second]);
    assert_eq!(list.server_now_ms, 3_000);
}

#[tokio::test]
async fn comments_belong_to_one_broadcast() {
    let w = world().await;
    let (_, channel_id) = user(&w.svc, "민수").await;
    w.svc.publish_started(&channel_id).await.unwrap();
    let live_id = w
        .svc
        .watch_page(&channel_id)
        .await
        .unwrap()
        .unwrap()
        .live
        .unwrap()
        .id;
    let live = BroadcastTarget::Live(live_id);

    w.svc
        .write_comment(w.younghee, w.recording, "녹화 댓글")
        .await
        .unwrap();
    w.svc
        .write_comment(w.younghee, live, "라이브 댓글")
        .await
        .unwrap();

    let recording: Vec<String> = w
        .svc
        .comments(w.recording)
        .await
        .unwrap()
        .comments
        .into_iter()
        .map(|c| c.content)
        .collect();
    let live: Vec<String> = w
        .svc
        .comments(live)
        .await
        .unwrap()
        .comments
        .into_iter()
        .map(|c| c.content)
        .collect();
    assert_eq!(recording, ["녹화 댓글"]);
    assert_eq!(live, ["라이브 댓글"]);
}

#[tokio::test]
async fn live_comments_stay_after_live_ends() {
    let w = world().await;
    let (_, channel_id) = user(&w.svc, "민수").await;
    w.svc.publish_started(&channel_id).await.unwrap();
    let live_id = w
        .svc
        .watch_page(&channel_id)
        .await
        .unwrap()
        .unwrap()
        .live
        .unwrap()
        .id;
    w.svc
        .write_comment(w.younghee, BroadcastTarget::Live(live_id), "수고했어요")
        .await
        .unwrap();

    w.svc.publish_ended(&channel_id).await.unwrap();

    let list = w
        .svc
        .comments(BroadcastTarget::Live(live_id))
        .await
        .unwrap();
    assert_eq!(list.comments.len(), 1);
}

#[tokio::test]
async fn comment_on_unknown_broadcast_is_not_found() {
    let w = world().await;

    let recording = w
        .svc
        .write_comment(w.younghee, BroadcastTarget::Recording(999), "?")
        .await;
    let live = w
        .svc
        .write_comment(w.younghee, BroadcastTarget::Live(999), "?")
        .await;
    let list = w.svc.comments(BroadcastTarget::Live(999)).await;

    assert!(
        matches!(recording, Err(ServiceError::UnknownRecording)),
        "{recording:?}"
    );
    assert!(matches!(live, Err(ServiceError::UnknownLive)), "{live:?}");
    assert!(matches!(list, Err(ServiceError::UnknownLive)), "{list:?}");
}

#[tokio::test]
async fn comment_content_must_be_1_to_500_chars() {
    let w = world().await;

    let empty = w.svc.write_comment(w.younghee, w.recording, " \n ").await;
    let long = w
        .svc
        .write_comment(w.younghee, w.recording, &"가".repeat(501))
        .await;
    let max = w
        .svc
        .write_comment(w.younghee, w.recording, &"가".repeat(500))
        .await;

    assert!(
        matches!(empty, Err(ServiceError::CommentEmpty)),
        "{empty:?}"
    );
    assert!(
        matches!(long, Err(ServiceError::CommentTooLong)),
        "{long:?}"
    );
    assert!(max.is_ok());
    assert_eq!(w.svc.comments(w.recording).await.unwrap().comments.len(), 1);
}

#[tokio::test]
async fn comment_by_unknown_user_is_rejected() {
    let w = world().await;

    let result = w.svc.write_comment(999, w.recording, "누구?").await;

    assert!(
        matches!(result, Err(ServiceError::UnknownUser)),
        "{result:?}"
    );
}

#[tokio::test]
async fn author_can_edit_comment() {
    let w = world().await;
    let comment = w
        .svc
        .write_comment(w.younghee, w.recording, "오타있음")
        .await
        .unwrap();

    let edited = w
        .svc
        .edit_comment(w.younghee, comment.id, " 오타 고침 ")
        .await
        .unwrap();

    assert_eq!(edited.content, "오타 고침");
    assert!(edited.edited);
    assert_eq!(
        w.svc.comments(w.recording).await.unwrap().comments,
        [edited]
    );
}

#[tokio::test]
async fn another_user_cannot_edit_or_delete_comment() {
    let w = world().await;
    let comment = w
        .svc
        .write_comment(w.younghee, w.recording, "영희의 댓글")
        .await
        .unwrap();

    // 녹화방송 주인(철수)도 남의 댓글은 고치거나 지울 수 없다.
    let edited = w.svc.edit_comment(w.chulsoo, comment.id, "바꿈").await;
    let deleted = w.svc.delete_comment(w.chulsoo, comment.id).await;

    for result in [edited.map(drop), deleted] {
        let error = result.unwrap_err();
        assert!(matches!(error, ServiceError::NotCommentAuthor), "{error:?}");
        assert_eq!(error.status_code(), 403);
    }
    assert_eq!(
        w.svc.comments(w.recording).await.unwrap().comments,
        [comment]
    );
}

#[tokio::test]
async fn edit_follows_content_rules_for_author() {
    let w = world().await;
    let comment = w
        .svc
        .write_comment(w.younghee, w.recording, "원래")
        .await
        .unwrap();

    let result = w.svc.edit_comment(w.younghee, comment.id, "  ").await;

    assert!(
        matches!(result, Err(ServiceError::CommentEmpty)),
        "{result:?}"
    );
    assert_eq!(
        w.svc.comments(w.recording).await.unwrap().comments,
        [comment]
    );
}

#[tokio::test]
async fn author_can_delete_comment() {
    let w = world().await;
    let comment = w
        .svc
        .write_comment(w.younghee, w.recording, "지울 댓글")
        .await
        .unwrap();

    w.svc.delete_comment(w.younghee, comment.id).await.unwrap();

    assert!(w
        .svc
        .comments(w.recording)
        .await
        .unwrap()
        .comments
        .is_empty());
    let again = w.svc.delete_comment(w.younghee, comment.id).await;
    assert!(
        matches!(again, Err(ServiceError::UnknownComment)),
        "{again:?}"
    );
}
