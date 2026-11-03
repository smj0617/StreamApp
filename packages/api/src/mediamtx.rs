//! MediaMTX 연동 HTTP 엔드포인트. 요청을 [`AppService`] 호출로 바꾸기만 하는 얇은 어댑터다.
//!
//! Dioxus에 의존하지 않는 axum [`Router`]라서, 앱은 Dioxus 라우터에 `.merge(..)`하고
//! 테스트는 요청을 직접 넣는다(경계 B). 로컬 전용이라는 전제로 인증은 없다(ADR 0001).
//!
//! 경로는 서버 함수(`/api/..`)와 겹치지 않게 `/internal/mediamtx/..` 아래에 둔다.
//! 이 라우터에는 fallback을 두지 않는다(Dioxus 라우터의 SSR fallback과 합칠 수 없다).

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;

use crate::service::AppService;

/// `authMethod: http` 인증 요청을 받는 경로. `tools/mediamtx/mediamtx.yml`의 `authHTTPAddress`와 같아야 한다.
pub const AUTH_PATH: &str = "/internal/mediamtx/auth";

/// MediaMTX 연동 라우터.
pub fn router(service: AppService) -> Router {
    Router::new()
        .route(AUTH_PATH, post(auth))
        .with_state(service)
}

/// MediaMTX v1.21.3이 동작(송출·재생 등)마다 POST하는 JSON 중 판단에 쓰는 필드.
/// 나머지(`ip`, `user`, `password`, `token`, `protocol`, `id`, `userAgent`)는 무시한다.
#[derive(Deserialize)]
struct AuthRequest {
    /// `publish`, `read`, `playback`, `api`, `metrics`, `pprof`
    action: String,
    /// 경로 이름. 송출이면 채널 ID다(`rtmp://localhost/<채널ID>?key=<송출키>`).
    #[serde(default)]
    path: String,
    /// URL의 `?` 뒤 원문(디코딩 전). 송출이면 `key=<송출키>`.
    #[serde(default)]
    query: String,
}

/// 송출이면 송출 허용 판단 결과로 2xx 또는 401, 그 밖의 동작은 모두 2xx.
///
/// 2xx 응답 본문은 비워 둔다. MediaMTX는 비어 있지 않은 본문을 `{"expireAt": ..}`로 읽으려 하고,
/// 읽지 못하면 인증 실패로 본다.
async fn auth(State(service): State<AppService>, Json(request): Json<AuthRequest>) -> StatusCode {
    if request.action != "publish" {
        return StatusCode::OK;
    }
    let stream_key = query_value(&request.query, "key").unwrap_or_default();
    match service.can_publish(&request.path, &stream_key).await {
        Ok(true) => StatusCode::OK,
        Ok(false) => {
            tracing::info!(
                "송출 거절: 채널 '{}'이 없거나 송출 키가 맞지 않습니다",
                request.path
            );
            StatusCode::UNAUTHORIZED
        }
        Err(e) => {
            tracing::error!("송출 허용 판단 실패: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// `a=1&key=abc` 같은 쿼리 문자열에서 `name`의 값을 디코딩해 꺼낸다.
fn query_value(query: &str, name: &str) -> Option<String> {
    form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}
