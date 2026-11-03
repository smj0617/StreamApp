//! MediaMTX 연동 HTTP 엔드포인트. 요청을 [`AppService`] 호출로 바꾸기만 하는 얇은 어댑터다.
//!
//! Dioxus에 의존하지 않는 axum [`Router`]라서, 앱은 Dioxus 라우터에 `.merge(..)`하고
//! 테스트는 요청을 직접 넣는다(경계 B). 로컬 전용이라는 전제로 인증은 없다(ADR 0001).
//!
//! 경로는 서버 함수(`/api/..`)와 겹치지 않게 `/internal/mediamtx/..` 아래에 둔다.
//! 이 라우터에는 fallback을 두지 않는다(Dioxus 라우터의 SSR fallback과 합칠 수 없다).

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Deserialize;

use crate::service::{AppService, ServiceError};

/// `authMethod: http` 인증 요청을 받는 경로. `tools/mediamtx/mediamtx.yml`의 `authHTTPAddress`와 같아야 한다.
pub const AUTH_PATH: &str = "/internal/mediamtx/auth";
/// 경로가 준비되면(송출 시작) `runOnAvailable` 훅이 `?path=<채널ID>`를 붙여 POST하는 경로.
pub const AVAILABLE_HOOK_PATH: &str = "/internal/mediamtx/hooks/available";
/// 경로의 준비가 끝나면(송출 종료) `runOnUnavailable` 훅이 `?path=<채널ID>`를 붙여 POST하는 경로.
pub const UNAVAILABLE_HOOK_PATH: &str = "/internal/mediamtx/hooks/unavailable";

/// MediaMTX 연동 라우터.
pub fn router(service: AppService) -> Router {
    Router::new()
        .route(AUTH_PATH, post(auth))
        .route(AVAILABLE_HOOK_PATH, post(available))
        .route(UNAVAILABLE_HOOK_PATH, post(unavailable))
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

/// 훅 요청의 쿼리. 훅은 `curl.exe`로 본문 없이 POST하므로 값은 쿼리로만 온다.
#[derive(Deserialize)]
struct HookQuery {
    /// MediaMTX가 `$MTX_PATH`를 바꿔 넣은 경로 이름 = 채널 ID
    path: String,
}

/// 송출 시작 훅: 송출 시작 반영. 없는 채널이면 404.
async fn available(State(service): State<AppService>, Query(hook): Query<HookQuery>) -> StatusCode {
    match service.publish_started(&hook.path).await {
        Ok(()) => {
            tracing::info!("라이브 시작: 채널 '{}'", hook.path);
            StatusCode::OK
        }
        Err(ServiceError::UnknownChannel) => {
            tracing::warn!("송출 시작 알림을 무시합니다: 없는 채널 '{}'", hook.path);
            StatusCode::NOT_FOUND
        }
        Err(e) => {
            tracing::error!("송출 시작 반영 실패(채널 '{}'): {e}", hook.path);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// 송출 종료 훅: 송출 종료 반영.
async fn unavailable(
    State(service): State<AppService>,
    Query(hook): Query<HookQuery>,
) -> StatusCode {
    match service.publish_ended(&hook.path).await {
        Ok(()) => {
            tracing::info!("라이브 종료: 채널 '{}'", hook.path);
            StatusCode::OK
        }
        Err(e) => {
            tracing::error!("송출 종료 반영 실패(채널 '{}'): {e}", hook.path);
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
