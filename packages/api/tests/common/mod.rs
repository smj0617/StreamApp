//! 테스트 파일들이 함께 쓰는 도우미. 테스트마다 새 메모리 SQLite를 쓴다.
//!
//! 테스트 파일마다 `mod common;`으로 가져온다. 파일마다 쓰는 도우미가 달라 쓰지 않은 도우미 경고는 끈다.
#![allow(dead_code)]

use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};

use api::service::{db, AppService};

/// 새 메모리 DB를 쓰는 서비스. 시계는 실제 시계다.
pub async fn service() -> AppService {
    AppService::new(db::connect_memory().await.expect("메모리 DB 연결"))
}

/// 테스트가 움직이는 시계. 처음에는 0(유닉스 밀리초)이다.
#[derive(Clone, Default)]
pub struct FakeClock(Arc<AtomicI64>);

impl FakeClock {
    pub fn set(&self, now_ms: i64) {
        self.0.store(now_ms, Ordering::SeqCst);
    }
}

/// 가짜 시계를 쓰는, 새 메모리 DB의 서비스.
pub async fn service_with_clock() -> (AppService, FakeClock) {
    let clock = FakeClock::default();
    let time = clock.0.clone();
    let svc = AppService::with_clock(
        db::connect_memory().await.expect("메모리 DB 연결"),
        Arc::new(move || time.load(Ordering::SeqCst)),
    );
    (svc, clock)
}

/// 사용자를 만들고 (사용자 ID, 채널 ID)를 돌려준다.
pub async fn user(svc: &AppService, nickname: &str) -> (i64, String) {
    let created = svc.create_user(nickname).await.unwrap();
    (created.user.id, created.channel_id)
}
