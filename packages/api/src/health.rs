//! 상태 확인 경로 `GET /healthz`. 배포에서 Caddy 뒤의 앱이 살아 있는지, DB에 닿는지 확인한다
//! (`curl -I https://<도메인>/healthz`, 모니터링).
//!
//! Dioxus에 의존하지 않는 axum [`Router`]라서 앱은 `.merge(..)`하고, 테스트는 요청을 직접 넣는다(경계 B).
//! [`crate::mediamtx`]처럼 fallback을 두지 않는다.
//!
//! - DB에 `SELECT 1`이 성공하면 200, 실패하면 503.
//! - MediaMTX Control API 연결 여부는 정보로만 싣는다(꺼져 있어도 503이 되지 않는다). 녹화방송·로그인·
//!   댓글은 MediaMTX 없이도 동작하기 때문이다.
//!
//! 응답 본문: `{"status":"ok"|"error","database":"ok"|"error","mediamtx":"ok"|"unreachable"|"error","version":"<앱 버전>"}`

use std::sync::Arc;
use std::time::Duration;

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;

use crate::service::AppService;

/// 상태 확인 경로.
pub const HEALTH_PATH: &str = "/healthz";
/// MediaMTX Control API가 답하기를 기다리는 시간. 상태 확인이 오래 걸리지 않게 짧게 둔다.
const MEDIAMTX_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct HealthState {
    service: AppService,
    /// MediaMTX Control API 주소(끝에 `/` 없음), 예: `http://127.0.0.1:9997`
    mediamtx_api: Arc<str>,
    client: reqwest::Client,
}

/// 상태 확인 응답 본문.
#[derive(Serialize)]
struct Health {
    /// DB가 응답하면 `ok`, 아니면 `error`(상태 코드 503)
    status: &'static str,
    database: &'static str,
    /// `ok`(Control API가 2xx로 답함), `unreachable`(연결할 수 없음), `error`(오류 상태 등)
    mediamtx: &'static str,
    /// 앱(api 크레이트) 버전
    version: &'static str,
}

/// 상태 확인 라우터. `mediamtx_api`는 MediaMTX Control API 주소다
/// (앱은 [`crate::server::mediamtx_api::address_from_env`], 테스트는 가짜 서버나 닫힌 주소).
pub fn router(service: AppService, mediamtx_api: &str) -> Router {
    let state = HealthState {
        service,
        mediamtx_api: mediamtx_api.trim_end_matches('/').into(),
        client: reqwest::Client::builder()
            .timeout(MEDIAMTX_TIMEOUT)
            .no_proxy()
            .build()
            .expect("reqwest 클라이언트 만들기"),
    };
    Router::new()
        .route(HEALTH_PATH, get(health))
        .with_state(state)
}

async fn health(State(state): State<HealthState>) -> (StatusCode, Json<Health>) {
    let (database, mediamtx) = tokio::join!(state.service.ping_database(), mediamtx_status(&state));
    let database_ok = match database {
        Ok(()) => true,
        Err(e) => {
            tracing::error!("상태 확인: DB가 응답하지 않습니다: {e}");
            false
        }
    };
    let status = if database_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = Health {
        status: if database_ok { "ok" } else { "error" },
        database: if database_ok { "ok" } else { "error" },
        mediamtx,
        version: env!("CARGO_PKG_VERSION"),
    };
    (status, Json(body))
}

/// MediaMTX Control API에 송출 경로 목록(한 개만)을 물어 연결 상태를 돌려준다.
async fn mediamtx_status(state: &HealthState) -> &'static str {
    let url = format!("{}/v3/paths/list?itemsPerPage=1", state.mediamtx_api);
    match state.client.get(url).send().await {
        Ok(response) if response.status().is_success() => "ok",
        Ok(_) => "error",
        Err(e) if e.is_connect() || e.is_timeout() => "unreachable",
        Err(_) => "error",
    }
}
