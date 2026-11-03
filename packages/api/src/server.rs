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

pub mod chat_socket;
pub mod current_user;
pub mod mediamtx_api;

use std::path::PathBuf;

use dioxus::prelude::ServerFnError;

use crate::service::{db, AppService, PublishingStatus, ServiceError};

/// DB 파일 경로를 바꾸는 환경변수.
pub const DB_PATH_ENV: &str = "STREAMAPP_DB";
/// 기본 DB 파일 경로. 서버를 실행한 폴더(개발 스크립트는 레포 루트) 기준이며, git에서 제외한다.
pub const DEFAULT_DB_PATH: &str = "data/streamapp.db";

/// 서버 함수가 앱 서비스를 꺼내는 추출자. `main`에서 `.layer(Extension(service))`로 붙인다.
pub type Service = dioxus::server::axum::Extension<AppService>;

/// 앱 시작 준비: DB 연결, 마이그레이션 적용, (사용자가 없으면) 기본 데이터 생성,
/// MediaMTX의 실제 송출에 라이브 맞추기.
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
    reconcile_with_mediamtx(&service).await?;
    Ok(service)
}

/// 앱이 꺼져 있던 동안 놓친 송출 시작·종료 알림을 보정한다: MediaMTX Control API로 지금 송출 중인
/// 경로를 한 번 조회해 송출 상태 맞추기를 부른다. 연결할 수 없으면 송출 중인 채널이 없다고 보고(열린
/// 라이브를 모두 닫는다), 답을 쓸 수 없으면 라이브를 그대로 둔다.
async fn reconcile_with_mediamtx(service: &AppService) -> Result<(), ServiceError> {
    let address = mediamtx_api::address_from_env();
    let status = mediamtx_api::publishing_status(&address).await;
    match &status {
        PublishingStatus::Known(paths) => {
            tracing::info!(
                "MediaMTX에서 송출 중인 경로 {}개를 확인했습니다",
                paths.len()
            );
        }
        PublishingStatus::Unreachable => {
            tracing::warn!("MediaMTX가 꺼져 있다고 보고 열린 라이브를 모두 닫습니다");
        }
        PublishingStatus::Unknown => {
            tracing::warn!(
                "송출 상태를 알 수 없어 송출 상태 맞추기를 건너뜁니다(라이브는 그대로 둡니다)"
            );
        }
    }
    service.reconcile_publishing(status).await?;
    tracing::info!(
        "라이브 중인 채널 {}개로 시작합니다",
        service.live_channels().await?.len()
    );
    Ok(())
}

/// DB 오류 대신 화면에 보여 줄 문구. 자세한 내용은 로그에만 남긴다.
const INTERNAL_ERROR: &str = "서버 내부 오류가 발생했습니다";

/// 서비스 오류를 화면에 보여 줄 사유로 바꾼다. 규칙 위반은 사유 그대로, DB 오류는 자세한 내용을 로그에
/// 남기고 [`INTERNAL_ERROR`]로 바꾼다. 서버 함수 오류(아래 `From`)와 채팅 거절 메시지가 함께 쓴다.
pub(crate) fn displayed_reason(error: ServiceError) -> String {
    match error {
        ServiceError::Database(e) => {
            tracing::error!("데이터베이스 오류: {e}");
            INTERNAL_ERROR.to_owned()
        }
        rejected => rejected.to_string(),
    }
}

impl From<ServiceError> for ServerFnError {
    fn from(error: ServiceError) -> Self {
        // DB 오류는 서버 잘못(500)이고, 나머지는 규칙 위반(400)이라 사유를 화면에 그대로 보여 준다.
        let code = if matches!(error, ServiceError::Database(_)) {
            500
        } else {
            400
        };
        ServerFnError::ServerError {
            message: displayed_reason(error),
            code,
            details: None,
        }
    }
}
