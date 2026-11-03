//! 서버 빌드 전용 연결 코드: 앱 시작 준비와, 서버 함수 어댑터가 함께 쓰는 것.
//!
//! 서버 함수는 이렇게 쓴다(`users.rs` 참고).
//!
//! ```ignore
//! use dioxus::prelude::*;
//! #[cfg(feature = "server")]
//! use crate::server::Service;
//!
//! #[get("/api/users", service: Service)]
//! pub async fn list_users() -> Result<Vec<User>, ServerFnError> {
//!     Ok(service.list_users().await?)   // ServiceError -> ServerFnError 는 아래 From 구현
//! }
//! ```

pub mod current_user;

use std::path::PathBuf;

use dioxus::prelude::ServerFnError;

use crate::service::{db, AppService, ServiceError};

/// DB 파일 경로를 바꾸는 환경변수.
pub const DB_PATH_ENV: &str = "STREAMAPP_DB";
/// 기본 DB 파일 경로. 서버를 실행한 폴더(개발 스크립트는 레포 루트) 기준이며, git에서 제외한다.
pub const DEFAULT_DB_PATH: &str = "data/streamapp.db";

/// 서버 함수가 앱 서비스를 꺼내는 추출자. `main`에서 `.layer(Extension(service))`로 붙인다.
pub type Service = dioxus::server::axum::Extension<AppService>;

/// 앱 시작 준비: DB 연결, 마이그레이션 적용, (사용자가 없으면) 기본 데이터 생성.
pub async fn init_from_env() -> anyhow::Result<AppService> {
    let path = std::env::var_os(DB_PATH_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DB_PATH));
    let pool = db::connect_file(&path).await?;
    tracing::info!("DB 준비 완료: {}", path.display());

    let service = AppService::new(pool);
    if service.seed_default_users().await? {
        tracing::info!("DB가 비어 있어 기본 사용자 3명과 채널을 만들었습니다");
    }
    Ok(service)
}

impl From<ServiceError> for ServerFnError {
    fn from(error: ServiceError) -> Self {
        match error {
            ServiceError::Database(e) => {
                tracing::error!("데이터베이스 오류: {e}");
                ServerFnError::new("서버 내부 오류가 발생했습니다")
            }
            // 나머지는 규칙 위반: 400과 함께 사유를 그대로 보내 화면에 보여 준다.
            rejected => ServerFnError::ServerError {
                message: rejected.to_string(),
                code: 400,
                details: None,
            },
        }
    }
}
