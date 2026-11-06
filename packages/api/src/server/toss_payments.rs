//! 토스페이먼츠 결제 서비스 클라이언트(테스트 모드): 결제 승인(`POST /v1/payments/confirm`)과 결제 조회
//! (`GET /v1/payments/{paymentKey}`). 앱 서비스의 [`PaymentGateway`] 구현이다.
//!
//! 키는 환경변수로만 받고, `test_`로 시작하는 테스트 키가 아니면 결제를 켜지 않는다(실제 결제를 하지 않는다).
//! 시크릿 키는 서버에만 있고 브라우저에는 클라이언트 키만 간다.

use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::service::{GatewayError, GatewayPayment, PaymentGateway, Payments};

/// 브라우저 결제창에 쓰는 클라이언트 키(API 개별 연동 키, `test_ck_…`)를 정하는 환경변수.
pub const CLIENT_KEY_ENV: &str = "STREAMAPP_TOSS_CLIENT_KEY";
/// 서버가 승인·조회에 쓰는 시크릿 키(`test_sk_…`)를 정하는 환경변수. 저장소에 적지 않는다.
pub const SECRET_KEY_ENV: &str = "STREAMAPP_TOSS_SECRET_KEY";
/// 결제 서비스 API 주소를 바꾸는 환경변수(테스트에서 가짜 서버로 바꿀 때만 쓴다).
pub const API_BASE_ENV: &str = "STREAMAPP_TOSS_API";
/// 토스페이먼츠 API 주소.
pub const DEFAULT_API_BASE: &str = "https://api.tosspayments.com";
/// 테스트 키의 앞부분. 라이브 키(`live_…`)는 받지 않는다.
const TEST_KEY_PREFIX: &str = "test_";
/// 결제 승인 응답을 기다리는 시간. 결제 서비스는 승인에 수 초가 걸릴 수 있다.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// 이미 승인된 결제를 다시 승인 요청했을 때 결제 서비스가 주는 오류 코드.
const ALREADY_PROCESSED: &str = "ALREADY_PROCESSED_PAYMENT";
/// 4xx지만 결제 거절이 아니라 잠시 뒤 다시 시도하면 되는 오류 코드: 같은 멱등키의 요청이 처리 중
/// (`IDEMPOTENT_REQUEST_PROCESSING`, 409), 결제 서비스의 일시적 오류(`PROVIDER_ERROR`).
const RETRY_CODES: [&str; 2] = ["IDEMPOTENT_REQUEST_PROCESSING", "PROVIDER_ERROR"];

/// 환경변수의 테스트 키로 결제 설정을 만든다. 키가 없거나 테스트 키가 아니면 `None`(후원을 받지 않는다).
pub fn payments_from_env() -> Option<Payments> {
    let read = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let (Some(client_key), Some(secret_key)) = (read(CLIENT_KEY_ENV), read(SECRET_KEY_ENV)) else {
        tracing::warn!(
            "{CLIENT_KEY_ENV}·{SECRET_KEY_ENV}가 없어 후원 결제를 끕니다(후원 내역은 그대로 보입니다)"
        );
        return None;
    };
    if !client_key.starts_with(TEST_KEY_PREFIX) || !secret_key.starts_with(TEST_KEY_PREFIX) {
        tracing::error!("토스페이먼츠 테스트 키(test_…)만 쓸 수 있습니다. 후원 결제를 끕니다");
        return None;
    }
    let api_base = read(API_BASE_ENV)
        .map(|base| base.trim_end_matches('/').to_owned())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_owned());
    tracing::info!("후원 결제: 토스페이먼츠 테스트 모드({api_base})");
    Some(Payments {
        client_key,
        gateway: Arc::new(TossPayments::new(api_base, secret_key)),
    })
}

/// 토스페이먼츠 API 클라이언트.
pub struct TossPayments {
    client: reqwest::Client,
    api_base: String,
    secret_key: String,
}

impl TossPayments {
    /// `api_base`는 끝에 `/`가 없는 API 주소(예: [`DEFAULT_API_BASE`]).
    pub fn new(api_base: impl Into<String>, secret_key: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self {
            client,
            api_base: api_base.into(),
            secret_key: secret_key.into(),
        }
    }

    async fn confirm_payment(
        &self,
        payment_key: String,
        order_id: String,
        amount: i64,
    ) -> Result<GatewayPayment, GatewayError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct ConfirmRequest<'a> {
            payment_key: &'a str,
            order_id: &'a str,
            amount: i64,
        }
        let request = self
            .client
            .post(format!("{}/v1/payments/confirm", self.api_base))
            // 시크릿 키 뒤에 `:`을 붙여 Basic 인증(비밀번호 없음). reqwest가 `<키>:`를 base64로 만든다.
            .basic_auth(&self.secret_key, None::<&str>)
            // 같은 주문의 승인 요청을 다시 보내도 결제 서비스가 한 번만 처리하게 한다.
            .header("Idempotency-Key", &order_id)
            .json(&ConfirmRequest {
                payment_key: &payment_key,
                order_id: &order_id,
                amount,
            });
        match read_payment(request).await {
            // 이미 승인된 결제(동시 요청·다시 시도): 결제를 조회해 그 결과를 돌려준다.
            Err(GatewayError::Rejected { code, .. }) if code == ALREADY_PROCESSED => {
                self.lookup_payment(payment_key).await
            }
            other => other,
        }
    }

    async fn lookup_payment(&self, payment_key: String) -> Result<GatewayPayment, GatewayError> {
        // 결제 키는 결제 서비스가 만든 값이지만 경로에 넣으므로 경로 조각으로 이스케이프한다.
        let mut url = reqwest::Url::parse(&format!("{}/v1/payments/", self.api_base))
            .map_err(|error| GatewayError::Unavailable(error.to_string()))?;
        url.path_segments_mut()
            .map_err(|()| GatewayError::Unavailable("잘못된 API 주소".to_owned()))?
            .pop_if_empty()
            .push(&payment_key);
        let request = self
            .client
            .get(url)
            .basic_auth(&self.secret_key, None::<&str>);
        read_payment(request).await
    }
}

impl PaymentGateway for TossPayments {
    fn confirm(
        &self,
        payment_key: String,
        order_id: String,
        amount: i64,
    ) -> BoxFuture<'_, Result<GatewayPayment, GatewayError>> {
        Box::pin(self.confirm_payment(payment_key, order_id, amount))
    }

    fn lookup(&self, payment_key: String) -> BoxFuture<'_, Result<GatewayPayment, GatewayError>> {
        Box::pin(self.lookup_payment(payment_key))
    }
}

/// Payment 객체 중 쓰는 필드.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TossPayment {
    payment_key: String,
    order_id: String,
    status: String,
    total_amount: i64,
}

/// 오류 응답 본문.
#[derive(Deserialize)]
struct TossError {
    code: String,
    message: String,
}

/// 요청을 보내고 Payment 객체나 오류로 바꾼다. 4xx는 결제 서비스의 거절, 그 밖의 실패는 연결 실패로 본다.
async fn read_payment(request: reqwest::RequestBuilder) -> Result<GatewayPayment, GatewayError> {
    let response = request
        .send()
        .await
        .map_err(|error| GatewayError::Unavailable(error.to_string()))?;
    let status = response.status();
    if status.is_success() {
        let payment: TossPayment = response.json().await.map_err(|error| {
            GatewayError::Unavailable(format!("결제 응답을 읽지 못함: {error}"))
        })?;
        return Ok(GatewayPayment {
            payment_key: payment.payment_key,
            order_id: payment.order_id,
            status: payment.status,
            total_amount: payment.total_amount,
        });
    }
    match response.json::<TossError>().await {
        // 같은 멱등키의 첫 요청이 아직 처리 중이다(동시 승인 요청). 거절이 아니므로 주문을 닫지 않고 다시 시도하게 한다.
        Ok(error) if RETRY_CODES.contains(&error.code.as_str()) => Err(GatewayError::Unavailable(
            format!("{status} {} {}", error.code, error.message),
        )),
        Ok(error) if status.is_client_error() => Err(GatewayError::Rejected {
            code: error.code,
            message: error.message,
        }),
        Ok(error) => Err(GatewayError::Unavailable(format!(
            "{status} {} {}",
            error.code, error.message
        ))),
        Err(_) => Err(GatewayError::Unavailable(format!("{status}"))),
    }
}
