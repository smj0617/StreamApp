//! 경계 A: 앱 서비스의 시청 페이지 정보. 테스트마다 새 메모리 SQLite를 쓰고,
//! 서비스의 공개 동작(사용자 만들기, 송출 시작·종료 반영)으로만 상태를 만든다.

use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};

use api::dto::{OpenLive, WatchPage};
use api::service::{db, AppService};

/// 테스트가 움직이는 시계(유닉스 밀리초)를 쓰는 서비스.
async fn service_with_clock() -> (AppService, Arc<AtomicI64>) {
    let now = Arc::new(AtomicI64::new(0));
    let time = now.clone();
    let svc = AppService::with_clock(
        db::connect_memory().await.expect("메모리 DB 연결"),
        Arc::new(move || time.load(Ordering::SeqCst)),
    );
    (svc, now)
}

/// 사용자 "철수"를 만들고 그 채널 ID를 돌려준다.
async fn chulsoo_channel(svc: &AppService) -> String {
    svc.create_user("철수").await.unwrap().channel_id
}

#[tokio::test]
async fn live_channel_watch_page_shows_open_live_and_playback_url() {
    let (svc, now) = service_with_clock().await;
    let channel_id = chulsoo_channel(&svc).await;
    now.store(1_000, Ordering::SeqCst);
    svc.publish_started(&channel_id).await.unwrap();
    now.store(9_000, Ordering::SeqCst);

    let page = svc.watch_page(&channel_id).await.unwrap();

    assert_eq!(
        page,
        Some(WatchPage {
            channel_id: channel_id.clone(),
            channel_name: "철수".to_string(),
            playback_url: format!("http://localhost:8888/{channel_id}/index.m3u8"),
            live: Some(OpenLive {
                title: "철수님의 라이브".to_string(),
                started_at_ms: 1_000,
            }),
        })
    );
}

#[tokio::test]
async fn channel_that_never_went_live_is_offline() {
    let (svc, _) = service_with_clock().await;
    let channel_id = chulsoo_channel(&svc).await;

    let page = svc
        .watch_page(&channel_id)
        .await
        .unwrap()
        .expect("채널이 있다");

    assert_eq!(page.channel_name, "철수");
    assert_eq!(page.live, None);
}

#[tokio::test]
async fn channel_whose_live_ended_is_offline() {
    let (svc, now) = service_with_clock().await;
    let channel_id = chulsoo_channel(&svc).await;
    let younghee = svc.create_user("영희").await.unwrap().channel_id;
    now.store(1_000, Ordering::SeqCst);
    svc.publish_started(&channel_id).await.unwrap();
    svc.publish_started(&younghee).await.unwrap();
    now.store(5_000, Ordering::SeqCst);

    svc.publish_ended(&channel_id).await.unwrap();

    let page = svc
        .watch_page(&channel_id)
        .await
        .unwrap()
        .expect("채널이 있다");
    assert_eq!(page.channel_name, "철수");
    assert_eq!(page.live, None);
}

#[tokio::test]
async fn unknown_channel_has_no_watch_page() {
    let (svc, _) = service_with_clock().await;
    chulsoo_channel(&svc).await;

    let page = svc.watch_page("nosuchchanne").await.unwrap();

    assert_eq!(page, None);
}
