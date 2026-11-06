//! 결제 서비스 연결부(포트). 앱 서비스는 결제 서비스에 "승인해 달라"와 "이 결제가 지금 어떤 상태인지"만
//! 묻는다. 실제 구현은 토스페이먼츠 테스트 모드 HTTP 클라이언트(`crate::server::toss_payments`)이고, 테스트는
//! 가짜 구현을 끼운다.

use std::sync::Arc;

use futures_util::future::BoxFuture;

/// 결제 서비스가 알려 준 결제 하나. 앱 서비스는 이 값을 주문과 비교한 뒤에만 후원을 확정한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatewayPayment {
    pub payment_key: String,
    pub order_id: String,
    /// 결제 서비스의 결제 상태. 승인이 끝난 결제는 [`PAYMENT_DONE`]이다.
    pub status: String,
    /// 결제된 총 금액(원)
    pub total_amount: i64,
}

/// 승인이 끝난 결제의 상태 값.
pub const PAYMENT_DONE: &str = "DONE";

/// 더 이상 승인될 수 없는 결제의 상태 값(취소·중단·만료). 이런 통지를 받으면 기다리던 주문을 실패로 닫는다.
pub const PAYMENT_CLOSED: [&str; 4] = ["CANCELED", "PARTIAL_CANCELED", "ABORTED", "EXPIRED"];

/// 결제 서비스에 물어본 결과가 결제가 아닐 때.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayError {
    /// 결제 서비스가 거절했다(카드 거절, 인증 만료, 없는 결제 등). 메시지는 결제 서비스가 준 그대로다.
    Rejected { code: String, message: String },
    /// 결제 서비스에 연결하지 못했거나 알 수 없는 응답을 받았다. 다시 시도하면 될 수 있다.
    Unavailable(String),
}

/// 결제 서비스. 같은 결제를 두 번 승인 요청해도(동시 요청, 다시 시도) 이미 승인된 결제면 그 결제를 돌려줘야
/// 한다(토스페이먼츠 구현은 `ALREADY_PROCESSED_PAYMENT`를 받으면 조회로 바꾼다).
pub trait PaymentGateway: Send + Sync {
    /// 결제 승인 요청.
    fn confirm(
        &self,
        payment_key: String,
        order_id: String,
        amount: i64,
    ) -> BoxFuture<'_, Result<GatewayPayment, GatewayError>>;

    /// 결제 키로 결제를 조회한다. 웹훅 본문을 믿지 않고 이 조회 결과로 처리한다.
    fn lookup(&self, payment_key: String) -> BoxFuture<'_, Result<GatewayPayment, GatewayError>>;
}

/// 앱 서비스에 끼우는 결제 설정: 브라우저에 줄 클라이언트 키와 서버가 쓰는 결제 서비스.
#[derive(Clone)]
pub struct Payments {
    pub client_key: String,
    pub gateway: Arc<dyn PaymentGateway>,
}
