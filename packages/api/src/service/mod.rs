//! 앱 서비스: Dioxus·HTTP·쿠키를 모르는 순수 Rust 계층이자 주 테스트 경계.
//!
//! 서버 함수(그리고 나중에 생길 채팅 WebSocket, MediaMTX 핸들러)는 요청을 이 서비스의
//! 동작 호출로 바꾸기만 한다. "현재 사용자"는 사용자 ID 인자로만 받는다.
//! 동작은 도메인별 파일(`users.rs` 등)에 `impl AppService` 블록으로 나눠 둔다.
//!
//! 테스트는 [`db::connect_memory`]로 만든 새 메모리 DB 위에서 공개된 동작만 확인한다.

mod channels;
pub mod db;
mod ids;
mod publish;
mod users;

use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppService {
    pool: SqlitePool,
}

impl AppService {
    /// 마이그레이션이 적용된 풀로 서비스를 만든다([`db::connect_file`], [`db::connect_memory`]).
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// 서비스 동작이 실패한 이유.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("데이터베이스 오류: {0}")]
    Database(#[from] sqlx::Error),
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
