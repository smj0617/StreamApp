//! 결제 서비스(토스페이먼츠) 웹훅: 결제 상태가 바뀌었다는 통지(`PAYMENT_STATUS_CHANGED`)를 받는다. 요청을
//! [`AppService::receive_payment_notice`] 호출로 바꾸기만 하는 얇은 어댑터다.
//!
//! 통지 본문은 누구나 보낼 수 있으므로 믿지 않는다. 본문에서 결제 키만 꺼내고, 앱 서비스가 그 키로 결제 서비스에
//! 결제를 조회해 확인한 뒤 처리한다. 같은 통지가 여러 번 와도(결제 서비스의 재전송) 확정은 한 번뿐이다.
//!
//! 결제 서비스는 10초 안에 200을 받지 못하면 다시 보낸다. 처리했거나 무시할 통지는 200, 결제 서비스 조회나
//! DB에 실패하면 500을 돌려 다시 받는다.
//!
//! Dioxus에 의존하지 않는 axum [`Router`]라서, 앱은 Dioxus 라우터에 `.merge(..)`하고 테스트는 요청을 직접
//! 넣는다.

use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Deserialize;

use crate::service::{AppService, NoticeOutcome};
use crate::PAYMENT_WEBHOOK_PATH;

/// 이 앱이 처리하는 웹훅 이벤트 타입.
const PAYMENT_STATUS_CHANGED: &str = "PAYMENT_STATUS_CHANGED";

/// 웹훅 라우터.
pub fn router(service: AppService) -> Router {
    Router::new()
        .route(PAYMENT_WEBHOOK_PATH, post(receive))
        .with_state(service)
}

/// 웹훅 본문 중 쓰는 필드: `{"eventType": "...", "data": {"paymentKey": "...", ...}}`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Notice {
    event_type: String,
    data: NoticeData,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoticeData {
    payment_key: Option<String>,
}

async fn receive(
    State(service): State<AppService>,
    body: Result<Json<Notice>, JsonRejection>,
) -> StatusCode {
    let notice = match body {
        Ok(Json(notice)) => notice,
        Err(error) => {
            tracing::info!("읽을 수 없는 결제 웹훅을 무시합니다: {error}");
            return StatusCode::OK;
        }
    };
    let Some(payment_key) = notice
        .data
        .payment_key
        .filter(|_| notice.event_type == PAYMENT_STATUS_CHANGED)
    else {
        return StatusCode::OK;
    };
    match service.receive_payment_notice(&payment_key).await {
        Ok(outcome) => {
            if outcome == NoticeOutcome::AlreadyConfirmed {
                tracing::info!("이미 확정된 결제의 통지를 다시 받았습니다(다시 적립하지 않음)");
            }
            StatusCode::OK
        }
        Err(error) => {
            tracing::warn!("결제 웹훅 처리 실패, 결제 서비스가 다시 보냅니다: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
