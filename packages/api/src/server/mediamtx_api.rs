//! MediaMTX Control API 클라이언트: 앱이 시작할 때 지금 송출 중인 경로(채널 ID)를 한 번 조회한다.
//!
//! 요청 하나를 보내고 결과를 [`PublishingStatus`]로 바꾸기만 하는 얇은 어댑터라 자동 테스트에서 뺀다.
//! 그 결과로 라이브를 어떻게 맞출지(연결할 수 없으면 모두 닫고, 답을 쓸 수 없으면 건너뛴다)는 앱
//! 서비스의 [`reconcile_publishing`]이 정한다.
//!
//! [`reconcile_publishing`]: crate::service::AppService::reconcile_publishing

use std::time::Duration;

use serde::Deserialize;

use crate::service::PublishingStatus;

/// Control API 주소를 바꾸는 환경변수.
pub const ADDRESS_ENV: &str = "STREAMAPP_MEDIAMTX_API";
/// 기본 Control API 주소. `tools/mediamtx/mediamtx.yml`의 `apiAddress`(`:9997`)와 맞춘다.
pub const DEFAULT_ADDRESS: &str = "http://127.0.0.1:9997";

/// 환경변수 [`ADDRESS_ENV`]에서 읽은 Control API 주소. 없거나 비어 있으면 [`DEFAULT_ADDRESS`].
pub fn address_from_env() -> String {
    std::env::var(ADDRESS_ENV)
        .ok()
        .map(|address| address.trim().trim_end_matches('/').to_string())
        .filter(|address| !address.is_empty())
        .unwrap_or_else(|| DEFAULT_ADDRESS.to_string())
}

/// `GET /v3/paths/list` 응답(MediaMTX v1.21.3) 중 쓰는 필드.
#[derive(Deserialize)]
struct PathList {
    items: Vec<PathItem>,
}

#[derive(Deserialize)]
struct PathItem {
    name: String,
    /// 경로가 준비됐는지(= 송출 중인지). 시청 요청으로만 생긴 경로는 `false`다.
    /// v1.21.3에서 폐지된 `ready` 대신 쓴다.
    #[serde(default)]
    available: bool,
}

/// Control API에 물은 송출 상태. 송출 중인 경로 이름(= 채널 ID) 목록을 받으면 [`PublishingStatus::Known`],
/// 연결할 수 없으면(연결 거부·타임아웃) [`PublishingStatus::Unreachable`], 답을 쓸 수 없으면(HTTP 오류
/// 상태, 응답 해석 실패 등) [`PublishingStatus::Unknown`]. 실패 이유는 경고로 남긴다.
pub async fn publishing_status(address: &str) -> PublishingStatus {
    match publishing_paths(address).await {
        Ok(paths) => PublishingStatus::Known(paths),
        Err(e) if e.is_connect() || e.is_timeout() => {
            tracing::warn!("MediaMTX Control API({address})에 연결할 수 없습니다: {e}");
            PublishingStatus::Unreachable
        }
        Err(e) => {
            tracing::warn!("MediaMTX Control API({address})의 답을 쓸 수 없습니다: {e}");
            PublishingStatus::Unknown
        }
    }
}

/// 지금 송출 중인(준비된) 경로 이름 목록. 경로 이름은 채널 ID다.
///
/// 로컬 개발용이라 한 페이지(최대 1000개)만 읽는다. 시스템 프록시 설정은 쓰지 않는다.
async fn publishing_paths(address: &str) -> Result<Vec<String>, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .no_proxy()
        .build()?;
    let list: PathList = client
        .get(format!("{address}/v3/paths/list?itemsPerPage=1000"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(list
        .items
        .into_iter()
        .filter(|path| path.available)
        .map(|path| path.name)
        .collect())
}
