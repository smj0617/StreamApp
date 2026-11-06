//! 후원 서버 함수. 요청을 앱 서비스 호출로 바꾸기만 한다.
//!
//! 후원 내역·합계는 로그인하지 않아도 볼 수 있다. 주문 만들기·승인 확인·취소 기록은 세션 쿠키의 로그인
//! 사용자로 하고, 로그인하지 않았으면 401로 거절한다. 금액·받는 채널·결제 승인 확인은 앱 서비스가 한다.

use dioxus::prelude::*;

use crate::dto::{BroadcastTarget, DonationBoard, DonationOrder, DonationResult, PaymentApproval};
#[cfg(feature = "server")]
use crate::server::{current_user, Service};

/// 화면이 결제창의 성공 주소로 쓰는 경로. 결제 서비스가 `paymentKey`·`orderId`·`amount`를 붙여 돌려보낸다.
pub const DONATION_SUCCESS_PATH: &str = "/donations/success";
/// 화면이 결제창의 실패 주소로 쓰는 경로. 결제 서비스가 `code`·`message`를 붙여 돌려보낸다.
pub const DONATION_FAIL_PATH: &str = "/donations/fail";
/// 결제 서비스(토스페이먼츠) 개발자센터에 등록하는 웹훅 주소의 경로.
pub const PAYMENT_WEBHOOK_PATH: &str = "/api/payments/toss/webhook";

/// 녹화방송 하나의 확정된 후원 내역과 합계. 없는 녹화방송이면 404.
#[get("/api/recordings/{id}/donations", service: Service)]
pub async fn recording_donations(id: i64) -> Result<DonationBoard, ServerFnError> {
    Ok(service.donations(BroadcastTarget::Recording(id)).await?)
}

/// 라이브 한 회차의 확정된 후원 내역과 합계. 없는 라이브면 404.
#[get("/api/lives/{id}/donations", service: Service)]
pub async fn live_donations(id: i64) -> Result<DonationBoard, ServerFnError> {
    Ok(service.donations(BroadcastTarget::Live(id)).await?)
}

/// 후원 주문 만들기: 로그인한 사용자가 방송과 금액(원)·메시지를 골랐다. 받는 채널은 서버가 방송에서 정한다.
/// 화면은 돌려받은 주문 그대로 결제창을 연다.
/// 로그인하지 않았으면 401, 없는 방송이면 404, 결제 설정이 없으면 503, 금액·메시지 규칙 위반·자기 방송이면 400.
#[post("/api/donations/prepare", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn prepare_donation(
    target: BroadcastTarget,
    amount: i64,
    message: String,
) -> Result<DonationOrder, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service
        .prepare_donation(user_id, target, amount, &message)
        .await?)
}

/// 결제창의 성공 주소로 돌아온 값으로 결제 승인을 확인하고 후원을 확정한다. 같은 값을 다시 보내도(새로고침)
/// 다시 적립하지 않고 `already_processed`로 돌려준다.
/// 로그인하지 않았으면 401, 주문한 사람이 아니면 403, 없는 주문이면 404, 금액·결제 정보가 주문과 다르거나
/// 결제 서비스가 거절하면 400, 결제 서비스에 연결하지 못하면 503.
#[post("/api/donations/confirm", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn confirm_donation(approval: PaymentApproval) -> Result<DonationResult, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service.confirm_donation(user_id, approval).await?)
}

/// 결제창을 닫았거나 실패 주소로 돌아왔을 때 기다리던 주문을 취소·실패로 닫는다. 확정된 주문은 바꾸지 않는다.
/// 로그인하지 않았으면 401, 주문한 사람이 아니면 403, 없는 주문이면 404.
#[post("/api/donations/abandon", service: Service, headers: dioxus::fullstack::HeaderMap)]
pub async fn abandon_donation(
    order_id: String,
    code: String,
    message: String,
) -> Result<DonationResult, ServerFnError> {
    let user_id = current_user::require_user_id(&service, &headers).await?;
    Ok(service
        .abandon_donation(user_id, &order_id, &code, &message)
        .await?)
}
