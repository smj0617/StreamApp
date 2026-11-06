//! 경계 B: 결제 서비스 HTTP. 가짜 토스페이먼츠 API 서버를 띄워 결제 클라이언트(`TossPayments`)가 보내는
//! 요청(인증 헤더·본문)과 응답 해석을 확인하고, 웹훅 Router에 통지를 직접 넣어 중복 통지가 한 번만
//! 적립되는지 확인한다. 실제 토스페이먼츠에는 접속하지 않는다.

mod common;

use std::sync::{Arc, Mutex};

use api::dto::{BroadcastTarget, PaymentApproval};
use api::payment_webhook;
use api::server::toss_payments::TossPayments;
use api::service::{AppService, GatewayError, PaymentGateway, Payments};
use axum::{
    body::Body,
    extract::{OriginalUri, State},
    http::{header, HeaderMap, Method, Request, StatusCode},
    response::IntoResponse,
    Router,
};
use bytes::Bytes;
use common::{service_with_recordings, user, TempDir};
use futures_util::stream;
use tower::ServiceExt; // oneshot

const SECRET_KEY: &str = "test_sk_fake";

/// 가짜 결제 서비스가 받은 요청: (메서드, 경로, Authorization, Idempotency-Key, 본문).
type Seen = Arc<Mutex<Vec<(Method, String, String, String, String)>>>;

/// 가짜 결제 서비스가 승인 요청에 줄 답.
#[derive(Clone, Copy)]
enum Reply {
    /// 200, 요청한 주문·금액 그대로 승인된 Payment
    Approve,
    /// 400 `ALREADY_PROCESSED_PAYMENT`(이미 승인됨). 조회하면 승인된 Payment
    AlreadyProcessed,
    /// 400 카드사 거절
    Reject,
    /// 500
    Broken,
    /// 409 `IDEMPOTENT_REQUEST_PROCESSING`(같은 멱등키의 요청이 처리 중)
    Processing,
}

fn payment_json(payment_key: &str, order_id: &str, amount: i64) -> String {
    format!(
        r#"{{"version":"2022-11-16","paymentKey":"{payment_key}","orderId":"{order_id}","orderName":"철수님 후원","status":"DONE","totalAmount":{amount},"balanceAmount":{amount},"method":"카드"}}"#
    )
}

/// 가짜 토스페이먼츠 API 서버를 띄우고 (주소, 받은 요청)을 돌려준다. 조회(`GET /v1/payments/<키>`)에는
/// 주문 `known_order`·금액 `known_amount`로 승인된 결제를 돌려준다.
async fn fake_toss(reply: Reply, known_order: &str, known_amount: i64) -> (String, Seen) {
    let seen: Seen = Arc::default();
    let known_order = known_order.to_string();
    let app = Router::new()
        .fallback(
            move |State((seen, reply)): State<(Seen, Reply)>,
                  method: Method,
                  OriginalUri(uri): OriginalUri,
                  headers: HeaderMap,
                  body: String| {
                let known_order = known_order.clone();
                async move {
                    let path = uri.path().to_string();
                    let header_text = |name: &str| {
                        headers
                            .get(name)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or_default()
                            .to_string()
                    };
                    seen.lock().unwrap().push((
                        method.clone(),
                        path.clone(),
                        header_text("authorization"),
                        header_text("idempotency-key"),
                        body.clone(),
                    ));
                    let json = [(header::CONTENT_TYPE, "application/json")];
                    if method == Method::GET {
                        let key = path.trim_start_matches("/v1/payments/");
                        return (
                            StatusCode::OK,
                            json,
                            payment_json(key, &known_order, known_amount),
                        )
                            .into_response();
                    }
                    let request: serde_json::Value = serde_json::from_str(&body).unwrap();
                    match reply {
                        Reply::Approve => (
                            StatusCode::OK,
                            json,
                            payment_json(
                                request["paymentKey"].as_str().unwrap(),
                                request["orderId"].as_str().unwrap(),
                                request["amount"].as_i64().unwrap(),
                            ),
                        )
                            .into_response(),
                        Reply::AlreadyProcessed => (
                            StatusCode::BAD_REQUEST,
                            json,
                            r#"{"code":"ALREADY_PROCESSED_PAYMENT","message":"이미 처리된 결제 입니다."}"#,
                        )
                            .into_response(),
                        Reply::Reject => (
                            StatusCode::FORBIDDEN,
                            json,
                            r#"{"code":"REJECT_CARD_COMPANY","message":"카드사에서 거절했습니다."}"#,
                        )
                            .into_response(),
                        Reply::Processing => (
                            StatusCode::CONFLICT,
                            json,
                            r#"{"code":"IDEMPOTENT_REQUEST_PROCESSING","message":"이전 멱등 요청이 처리중입니다."}"#,
                        )
                            .into_response(),
                        Reply::Broken => (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            json,
                            r#"{"code":"FAILED_INTERNAL_SYSTEM_PROCESSING","message":"내부 오류"}"#,
                        )
                            .into_response(),
                    }
                }
            },
        )
        .with_state((seen.clone(), reply));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (address, seen)
}

/// `test_sk_fake:`의 base64.
fn expected_basic_auth() -> String {
    "Basic dGVzdF9za19mYWtlOg==".to_string()
}

#[tokio::test]
async fn confirm_sends_secret_key_and_order_amount() {
    let (address, seen) = fake_toss(Reply::Approve, "unused", 0).await;
    let toss = TossPayments::new(address, SECRET_KEY);

    let payment = toss
        .confirm("pay_1".to_string(), "don-abc123".to_string(), 5_000)
        .await
        .unwrap();

    assert_eq!(payment.payment_key, "pay_1");
    assert_eq!(payment.order_id, "don-abc123");
    assert_eq!(payment.status, "DONE");
    assert_eq!(payment.total_amount, 5_000);
    let seen = seen.lock().unwrap();
    let (method, path, auth, idempotency, body) = &seen[0];
    assert_eq!(method, Method::POST);
    assert_eq!(path, "/v1/payments/confirm");
    assert_eq!(auth, &expected_basic_auth());
    assert_eq!(idempotency, "don-abc123");
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"paymentKey": "pay_1", "orderId": "don-abc123", "amount": 5000})
    );
}

#[tokio::test]
async fn already_processed_payment_is_looked_up() {
    let (address, seen) = fake_toss(Reply::AlreadyProcessed, "don-abc123", 5_000).await;
    let toss = TossPayments::new(address, SECRET_KEY);

    let payment = toss
        .confirm("pay_1".to_string(), "don-abc123".to_string(), 5_000)
        .await
        .unwrap();

    assert_eq!(payment.order_id, "don-abc123");
    assert_eq!(payment.total_amount, 5_000);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].0, Method::GET);
    assert_eq!(seen[1].1, "/v1/payments/pay_1");
    assert_eq!(seen[1].2, expected_basic_auth());
}

#[tokio::test]
async fn client_errors_are_rejections_and_server_errors_are_unavailable() {
    let (reject_address, _) = fake_toss(Reply::Reject, "unused", 0).await;
    let (broken_address, _) = fake_toss(Reply::Broken, "unused", 0).await;

    let rejected = TossPayments::new(reject_address, SECRET_KEY)
        .confirm("pay_1".to_string(), "don-abc123".to_string(), 5_000)
        .await;
    let broken = TossPayments::new(broken_address, SECRET_KEY)
        .confirm("pay_1".to_string(), "don-abc123".to_string(), 5_000)
        .await;
    let (processing_address, _) = fake_toss(Reply::Processing, "unused", 0).await;
    // 같은 주문의 승인 요청이 처리 중이면 거절이 아니다(주문을 닫지 않고 다시 시도한다).
    let processing = TossPayments::new(processing_address, SECRET_KEY)
        .confirm("pay_1".to_string(), "don-abc123".to_string(), 5_000)
        .await;
    assert!(matches!(processing, Err(GatewayError::Unavailable(_))));
    let unreachable = TossPayments::new("http://127.0.0.1:9", SECRET_KEY)
        .confirm("pay_1".to_string(), "don-abc123".to_string(), 5_000)
        .await;

    assert_eq!(
        rejected,
        Err(GatewayError::Rejected {
            code: "REJECT_CARD_COMPANY".to_string(),
            message: "카드사에서 거절했습니다.".to_string(),
        })
    );
    assert!(matches!(broken, Err(GatewayError::Unavailable(_))));
    assert!(matches!(unreachable, Err(GatewayError::Unavailable(_))));
}

/// 철수의 녹화방송에 영희가 5,000원 후원 주문을 만든 서비스. 결제 서비스는 가짜 토스페이먼츠 서버를 쓰는
/// 실제 클라이언트다.
async fn world_with_order() -> (AppService, BroadcastTarget, i64, String, Seen, TempDir) {
    let (svc, _clock, dir) = service_with_recordings().await;
    // 주문번호는 주문을 만들어야 알 수 있으므로, 먼저 주문을 만들 서비스와 같은 DB를 쓰는 서비스를 나중에 만든다.
    let (chulsoo, _) = user(&svc, "철수").await;
    let (younghee, _) = user(&svc, "영희").await;
    let id = svc
        .register_recording(
            chulsoo,
            "녹화",
            "clip.mp4",
            stream::iter(vec![Ok::<_, String>(Bytes::from_static(b"video"))]),
        )
        .await
        .unwrap();
    let target = BroadcastTarget::Recording(id);
    let placeholder = svc.clone().with_payments(Payments {
        client_key: "test_ck_fake".to_string(),
        gateway: Arc::new(TossPayments::new("http://127.0.0.1:9", SECRET_KEY)),
    });
    let order = placeholder
        .prepare_donation(younghee, target, 5_000, "")
        .await
        .unwrap();
    let (address, seen) = fake_toss(Reply::Approve, &order.order_id, 5_000).await;
    let svc = svc.with_payments(Payments {
        client_key: "test_ck_fake".to_string(),
        gateway: Arc::new(TossPayments::new(address, SECRET_KEY)),
    });
    (svc, target, younghee, order.order_id, seen, dir)
}

fn notice(payment_key: &str) -> Request<Body> {
    Request::post("/api/payments/toss/webhook")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"eventType":"PAYMENT_STATUS_CHANGED","createdAt":"2026-11-06T10:00:00.000000","data":{{"paymentKey":"{payment_key}","orderId":"꾸며 낸 값","status":"DONE","totalAmount":999999}}}}"#
        )))
        .unwrap()
}

#[tokio::test]
async fn repeated_webhook_is_counted_once() {
    let (svc, target, _younghee, _order_id, seen, _dir) = world_with_order().await;
    let app = payment_webhook::router(svc.clone());

    let first = app.clone().oneshot(notice("pay_1")).await.unwrap();
    let second = app.clone().oneshot(notice("pay_1")).await.unwrap();

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    let board = svc.donations(target).await.unwrap();
    // 본문의 금액(999999)이 아니라 결제 서비스 조회 결과(5,000원)로 한 번만 적립했다.
    assert_eq!((board.count, board.total_amount), (1, 5_000));
    // 통지마다 결제 서비스에 결제를 조회했다.
    let lookups = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|(method, path, ..)| method == Method::GET && path == "/v1/payments/pay_1")
        .count();
    assert_eq!(lookups, 2);
}

#[tokio::test]
async fn success_redirect_after_webhook_is_duplicate() {
    let (svc, target, younghee, order_id, _seen, _dir) = world_with_order().await;
    let app = payment_webhook::router(svc.clone());
    app.oneshot(notice("pay_1")).await.unwrap();

    let result = svc
        .confirm_donation(
            younghee,
            PaymentApproval {
                payment_key: "pay_1".to_string(),
                order_id,
                amount: 5_000,
            },
        )
        .await
        .unwrap();

    assert!(result.already_processed);
    let board = svc.donations(target).await.unwrap();
    assert_eq!((board.count, board.total_amount), (1, 5_000));
}

#[tokio::test]
async fn unreadable_or_other_events_are_acknowledged() {
    let (svc, target, ..) = world_with_order().await;
    let app = payment_webhook::router(svc.clone());

    let garbage = app
        .clone()
        .oneshot(
            Request::post("/api/payments/toss/webhook")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();
    let other = app
        .clone()
        .oneshot(
            Request::post("/api/payments/toss/webhook")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"eventType":"DEPOSIT_CALLBACK","data":{"paymentKey":"pay_1"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(garbage.status(), StatusCode::OK);
    assert_eq!(other.status(), StatusCode::OK);
    let board = svc.donations(target).await.unwrap();
    assert_eq!(board.count, 0);
}

#[tokio::test]
async fn webhook_asks_for_retry_when_gateway_is_down() {
    let (svc, ..) = world_with_order().await;
    let down = svc.with_payments(Payments {
        client_key: "test_ck_fake".to_string(),
        gateway: Arc::new(TossPayments::new("http://127.0.0.1:9", SECRET_KEY)),
    });

    let response = payment_webhook::router(down)
        .oneshot(notice("pay_1"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
