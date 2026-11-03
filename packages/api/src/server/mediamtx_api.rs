//! MediaMTX Control API 클라이언트: 앱이 시작할 때 지금 송출 중인 경로(채널 ID)를 한 번 조회한다.
//!
//! 요청 하나를 보내고 응답을 채널 ID 목록으로 바꾸기만 하는 얇은 어댑터라 자동 테스트에서 뺀다.
//! 목록으로 라이브를 맞추는 규칙은 앱 서비스의 [`reconcile_publishing`]에 있다.
//!
//! [`reconcile_publishing`]: crate::service::AppService::reconcile_publishing

use std::time::Duration;

use serde::Deserialize;

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

/// 지금 송출 중인(준비된) 경로 이름 목록. 경로 이름은 채널 ID다.
///
/// 로컬 개발용이라 한 페이지(최대 1000개)만 읽는다. 시스템 프록시 설정은 쓰지 않는다.
pub async fn publishing_paths(address: &str) -> Result<Vec<String>, reqwest::Error> {
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
