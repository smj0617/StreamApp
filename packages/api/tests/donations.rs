//! 경계 A: 앱 서비스의 후원(과제 ④). 테스트마다 새 메모리 SQLite를 쓰고, 결제 서비스는 가짜를 끼운다.
//! 가짜 결제 서비스는 받은 승인 요청을 세고, 정해 둔 대로 승인·거절·다른 금액을 돌려준다.

mod common;

use std::sync::{Arc, Mutex};

use api::dto::{BroadcastTarget, DonationOrder, DonationStatus, PaymentApproval};
use api::service::{
    AppService, GatewayError, GatewayPayment, NoticeOutcome, PaymentGateway, Payments, ServiceError,
};
use bytes::Bytes;
use common::{service_with_recordings, user, FakeClock, TempDir};
use futures_util::future::BoxFuture;
use futures_util::stream;

/// 가짜 결제 서비스가 승인 요청에 어떻게 답할지.
#[derive(Clone)]
enum Answer {
    /// 요청 그대로 승인한다(같은 결제 키를 두 번 받아도 같은 결제를 돌려준다)
    Approve,
    /// 금액을 바꿔 승인한다(결제 서비스와 주문이 어긋난 상황)
    ApproveWithAmount(i64),
    /// 거절한다
    Reject(&'static str, &'static str),
    /// 연결하지 못한다
    Down,
}

#[derive(Default)]
struct GatewayState {
    answer: Option<Answer>,
    /// 받은 승인 요청(결제 키, 주문번호, 금액)
    confirms: Vec<(String, String, i64)>,
    /// 승인한 결제(조회에 쓴다)
    payments: Vec<GatewayPayment>,
}

#[derive(Clone, Default)]
struct FakeGateway(Arc<Mutex<GatewayState>>);

impl FakeGateway {
    fn answer(&self, answer: Answer) {
        self.0.lock().unwrap().answer = Some(answer);
    }

    fn confirm_count(&self) -> usize {
        self.0.lock().unwrap().confirms.len()
    }

    /// 결제 서비스에 결제 하나가 있다고 정한다(웹훅 조회용).
    fn put(&self, payment: GatewayPayment) {
        let mut state = self.0.lock().unwrap();
        state
            .payments
            .retain(|p| p.payment_key != payment.payment_key);
        state.payments.push(payment);
    }
}

impl PaymentGateway for FakeGateway {
    fn confirm(
        &self,
        payment_key: String,
        order_id: String,
        amount: i64,
    ) -> BoxFuture<'_, Result<GatewayPayment, GatewayError>> {
        let mut state = self.0.lock().unwrap();
        state
            .confirms
            .push((payment_key.clone(), order_id.clone(), amount));
        let result = match state.answer.clone().unwrap_or(Answer::Approve) {
            Answer::Approve => Ok(GatewayPayment {
                payment_key,
                order_id,
                status: "DONE".to_string(),
                total_amount: amount,
            }),
            Answer::ApproveWithAmount(total_amount) => Ok(GatewayPayment {
                payment_key,
                order_id,
                status: "DONE".to_string(),
                total_amount,
            }),
            Answer::Reject(code, message) => Err(GatewayError::Rejected {
                code: code.to_string(),
                message: message.to_string(),
            }),
            Answer::Down => Err(GatewayError::Unavailable("연결 거부".to_string())),
        };
        if let Ok(payment) = &result {
            state
                .payments
                .retain(|p| p.payment_key != payment.payment_key);
            state.payments.push(payment.clone());
        }
        Box::pin(async move { result })
    }

    fn lookup(&self, payment_key: String) -> BoxFuture<'_, Result<GatewayPayment, GatewayError>> {
        let state = self.0.lock().unwrap();
        let result = match state.answer {
            Some(Answer::Down) => Err(GatewayError::Unavailable("연결 거부".to_string())),
            _ => state
                .payments
                .iter()
                .find(|p| p.payment_key == payment_key)
                .cloned()
                .ok_or(GatewayError::Rejected {
                    code: "NOT_FOUND_PAYMENT".to_string(),
                    message: "존재하지 않는 결제 정보 입니다.".to_string(),
                }),
        };
        Box::pin(async move { result })
    }
}

struct World {
    svc: AppService,
    clock: FakeClock,
    gateway: FakeGateway,
    chulsoo: i64,
    chulsoo_channel: String,
    younghee: i64,
    minsu: i64,
    recording: BroadcastTarget,
    _dir: TempDir,
}

/// 철수가 녹화방송 하나를 올렸고, 영희·민수도 가입해 있다. 결제 서비스는 가짜다.
async fn world() -> World {
    let (svc, clock, dir) = service_with_recordings().await;
    let gateway = FakeGateway::default();
    let svc = svc.with_payments(Payments {
        client_key: "test_ck_fake".to_string(),
        gateway: Arc::new(gateway.clone()),
    });
    let (chulsoo, chulsoo_channel) = user(&svc, "철수").await;
    let (younghee, _) = user(&svc, "영희").await;
    let (minsu, _) = user(&svc, "민수").await;
    let id = svc
        .register_recording(
            chulsoo,
            "녹화",
            "clip.mp4",
            stream::iter(vec![Ok::<_, String>(Bytes::from_static(b"video"))]),
        )
        .await
        .unwrap();
    World {
        svc,
        clock,
        gateway,
        chulsoo,
        chulsoo_channel,
        younghee,
        minsu,
        recording: BroadcastTarget::Recording(id),
        _dir: dir,
    }
}

/// 결제창이 성공 주소로 돌려줄 값(결제 키는 결제 서비스가 만든다고 본다).
fn approval(order: &DonationOrder, payment_key: &str) -> PaymentApproval {
    PaymentApproval {
        payment_key: payment_key.to_string(),
        order_id: order.order_id.clone(),
        amount: order.amount,
    }
}

async fn total(w: &World) -> (i64, i64) {
    let board = w.svc.donations(w.recording).await.unwrap();
    (board.count, board.total_amount)
}

#[tokio::test]
async fn order_is_made_by_server_with_recipient_from_broadcast() {
    let w = world().await;

    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "  응원해요 ")
        .await
        .unwrap();

    assert_eq!(order.amount, 5_000);
    assert_eq!(order.recipient_name, "철수");
    assert_eq!(order.order_name, "철수님 후원");
    assert_eq!(order.client_key, "test_ck_fake");
    assert!(order.order_id.len() >= 6 && order.order_id.len() <= 64);
    assert!(order.customer_key.len() >= 22 && order.customer_key.len() <= 50);
    // 결제를 확인하기 전에는 내역에도 합계에도 없다.
    assert_eq!(total(&w).await, (0, 0));
    let board = w.svc.donations(w.recording).await.unwrap();
    assert_eq!(board.recipient_channel_id, w.chulsoo_channel);
    assert_eq!(board.recipient_user_id, w.chulsoo);
    assert!(board.payments_enabled);
}

#[tokio::test]
async fn order_rules_are_checked_on_server() {
    let w = world().await;

    let below = w
        .svc
        .prepare_donation(w.younghee, w.recording, 999, "")
        .await;
    let above = w
        .svc
        .prepare_donation(w.younghee, w.recording, 1_000_001, "")
        .await;
    let negative = w
        .svc
        .prepare_donation(w.younghee, w.recording, -5_000, "")
        .await;
    let long = w
        .svc
        .prepare_donation(w.younghee, w.recording, 1_000, &"가".repeat(101))
        .await;
    let to_self = w
        .svc
        .prepare_donation(w.chulsoo, w.recording, 1_000, "")
        .await;
    let missing = w
        .svc
        .prepare_donation(w.younghee, BroadcastTarget::Recording(999), 1_000, "")
        .await;

    assert!(matches!(below, Err(ServiceError::DonationAmount)));
    assert!(matches!(above, Err(ServiceError::DonationAmount)));
    assert!(matches!(negative, Err(ServiceError::DonationAmount)));
    assert!(matches!(long, Err(ServiceError::DonationMessageTooLong)));
    assert!(matches!(to_self, Err(ServiceError::DonationToSelf)));
    assert!(matches!(missing, Err(ServiceError::UnknownRecording)));
}

#[tokio::test]
async fn without_payment_settings_no_order_is_made() {
    let (svc, _clock, _dir) = service_with_recordings().await;
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

    let result = svc
        .prepare_donation(younghee, BroadcastTarget::Recording(id), 1_000, "")
        .await;

    assert!(matches!(result, Err(ServiceError::PaymentsNotConfigured)));
    let board = svc.donations(BroadcastTarget::Recording(id)).await.unwrap();
    assert!(!board.payments_enabled);
}

#[tokio::test]
async fn confirmed_payment_is_listed_with_total() {
    let w = world().await;
    w.clock.set(1_000);
    let first = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "응원해요")
        .await
        .unwrap();
    let second = w
        .svc
        .prepare_donation(w.minsu, w.recording, 10_000, "")
        .await
        .unwrap();

    w.clock.set(2_000);
    let result = w
        .svc
        .confirm_donation(w.younghee, approval(&first, "pay_1"))
        .await
        .unwrap();
    w.clock.set(3_000);
    w.svc
        .confirm_donation(w.minsu, approval(&second, "pay_2"))
        .await
        .unwrap();

    assert_eq!(result.status, DonationStatus::Confirmed);
    assert!(!result.already_processed);
    assert_eq!(result.amount, 5_000);
    assert_eq!(result.target, Some(w.recording));
    // 결제 서비스에는 화면이 보낸 값이 아니라 서버의 주문 금액으로 승인을 요청했다.
    assert_eq!(
        w.gateway.0.lock().unwrap().confirms[0],
        ("pay_1".to_string(), first.order_id.clone(), 5_000)
    );
    let board = w.svc.donations(w.recording).await.unwrap();
    assert_eq!((board.count, board.total_amount), (2, 15_000));
    let rows: Vec<(String, i64, String, i64)> = board
        .donations
        .into_iter()
        .map(|d| (d.donor_nickname, d.amount, d.message, d.confirmed_at_ms))
        .collect();
    assert_eq!(
        rows,
        [
            ("민수".to_string(), 10_000, String::new(), 3_000),
            ("영희".to_string(), 5_000, "응원해요".to_string(), 2_000),
        ]
    );
}

#[tokio::test]
async fn same_approval_twice_is_counted_once() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();

    let first = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();
    // 성공 페이지 새로고침: 같은 값이 다시 온다.
    let again = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();

    assert!(!first.already_processed);
    assert!(again.already_processed);
    assert_eq!(again.status, DonationStatus::Confirmed);
    // 이미 확정된 주문이라 결제 서비스를 다시 부르지 않았다.
    assert_eq!(w.gateway.confirm_count(), 1);
    assert_eq!(total(&w).await, (1, 5_000));
}

#[tokio::test]
async fn approval_with_another_payment_key_for_confirmed_order_is_rejected() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();

    let other = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_other"))
        .await;

    assert!(matches!(other, Err(ServiceError::PaymentMismatch)));
    assert_eq!(total(&w).await, (1, 5_000));
}

#[tokio::test]
async fn concurrent_approvals_are_counted_once() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();

    let (a, b) = tokio::join!(
        w.svc
            .confirm_donation(w.younghee, approval(&order, "pay_1")),
        w.svc
            .confirm_donation(w.younghee, approval(&order, "pay_1")),
    );

    let a = a.unwrap();
    let b = b.unwrap();
    // 하나만 새로 확정했고 다른 하나는 중복으로 처리했다.
    assert_ne!(a.already_processed, b.already_processed);
    assert_eq!(total(&w).await, (1, 5_000));
}

#[tokio::test]
async fn tampered_amount_is_not_approved() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 50_000, "")
        .await
        .unwrap();
    let mut tampered = approval(&order, "pay_1");
    tampered.amount = 1_000;

    let result = w.svc.confirm_donation(w.younghee, tampered).await;

    assert!(matches!(result, Err(ServiceError::DonationAmountMismatch)));
    // 결제 서비스에 승인을 요청하지 않았고, 주문은 실패로 닫혔다.
    assert_eq!(w.gateway.confirm_count(), 0);
    let closed = w
        .svc
        .abandon_donation(w.younghee, &order.order_id, "", "")
        .await
        .unwrap();
    assert_eq!(closed.status, DonationStatus::Failed);
    assert_eq!(total(&w).await, (0, 0));
}

#[tokio::test]
async fn payment_with_different_amount_from_gateway_is_not_confirmed() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.gateway.answer(Answer::ApproveWithAmount(100));

    let result = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await;

    assert!(matches!(result, Err(ServiceError::PaymentMismatch)));
    assert_eq!(total(&w).await, (0, 0));
}

#[tokio::test]
async fn rejected_payment_closes_order_as_failed() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.gateway.answer(Answer::Reject(
        "REJECT_CARD_COMPANY",
        "카드사에서 거절했습니다",
    ));

    let result = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await;

    match result {
        Err(ServiceError::PaymentRejected(message)) => {
            assert_eq!(message, "카드사에서 거절했습니다")
        }
        other => panic!("결제 거절이어야 합니다: {other:?}"),
    }
    let state = w
        .svc
        .abandon_donation(w.younghee, &order.order_id, "", "")
        .await
        .unwrap();
    assert_eq!(state.status, DonationStatus::Failed);
    assert_eq!(
        state.failure_reason.as_deref(),
        Some("카드사에서 거절했습니다")
    );
    assert_eq!(total(&w).await, (0, 0));
}

#[tokio::test]
async fn unreachable_gateway_keeps_order_pending_for_retry() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.gateway.answer(Answer::Down);

    let first = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await;
    w.gateway.answer(Answer::Approve);
    let retry = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();

    assert!(matches!(
        first,
        Err(ServiceError::PaymentServiceUnavailable)
    ));
    assert_eq!(retry.status, DonationStatus::Confirmed);
    assert_eq!(total(&w).await, (1, 5_000));
}

#[tokio::test]
async fn only_the_donor_can_confirm_or_abandon() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();

    let confirm = w
        .svc
        .confirm_donation(w.minsu, approval(&order, "pay_1"))
        .await;
    let abandon = w
        .svc
        .abandon_donation(w.minsu, &order.order_id, "USER_CANCEL", "")
        .await;
    let unknown = w
        .svc
        .confirm_donation(
            w.younghee,
            PaymentApproval {
                payment_key: "pay_1".to_string(),
                order_id: "don-none".to_string(),
                amount: 5_000,
            },
        )
        .await;

    assert!(matches!(confirm, Err(ServiceError::NotDonationOwner)));
    assert!(matches!(abandon, Err(ServiceError::NotDonationOwner)));
    assert!(matches!(unknown, Err(ServiceError::UnknownDonation)));
    assert_eq!(w.gateway.confirm_count(), 0);
}

#[tokio::test]
async fn canceled_payment_is_recorded_and_not_counted() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();

    let canceled = w
        .svc
        .abandon_donation(w.younghee, &order.order_id, "USER_CANCEL", "취소되었습니다")
        .await
        .unwrap();
    let failed_order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    let failed = w
        .svc
        .abandon_donation(
            w.younghee,
            &failed_order.order_id,
            "PAY_PROCESS_ABORTED",
            "결제가 중단되었습니다",
        )
        .await
        .unwrap();

    assert_eq!(canceled.status, DonationStatus::Canceled);
    assert_eq!(failed.status, DonationStatus::Failed);
    assert_eq!(
        failed.failure_reason.as_deref(),
        Some("결제가 중단되었습니다")
    );
    assert_eq!(total(&w).await, (0, 0));
}

#[tokio::test]
async fn failure_report_does_not_undo_confirmed_donation() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();

    let after = w
        .svc
        .abandon_donation(w.younghee, &order.order_id, "USER_CANCEL", "")
        .await
        .unwrap();

    assert_eq!(after.status, DonationStatus::Confirmed);
    assert_eq!(total(&w).await, (1, 5_000));
}

#[tokio::test]
async fn repeated_webhook_notice_confirms_once() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    // 화면이 성공 주소로 돌아오지 못했지만 결제 서비스에서는 승인이 끝났다.
    w.gateway.put(GatewayPayment {
        payment_key: "pay_1".to_string(),
        order_id: order.order_id.clone(),
        status: "DONE".to_string(),
        total_amount: 5_000,
    });

    let first = w.svc.receive_payment_notice("pay_1").await.unwrap();
    let second = w.svc.receive_payment_notice("pay_1").await.unwrap();
    // 늦게 도착한 성공 주소 요청도 중복으로 처리된다.
    let late = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();

    assert_eq!(first, NoticeOutcome::Confirmed);
    assert_eq!(second, NoticeOutcome::AlreadyConfirmed);
    assert!(late.already_processed);
    assert_eq!(total(&w).await, (1, 5_000));
}

#[tokio::test]
async fn webhook_after_confirm_is_duplicate() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();

    let notice = w.svc.receive_payment_notice("pay_1").await.unwrap();

    assert_eq!(notice, NoticeOutcome::AlreadyConfirmed);
    assert_eq!(total(&w).await, (1, 5_000));
}

#[tokio::test]
async fn webhook_trusts_gateway_lookup_not_notice() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();

    // 결제 서비스에 없는 결제 키(꾸며 낸 통지)는 무시한다.
    let forged = w.svc.receive_payment_notice("pay_forged").await.unwrap();
    // 결제 서비스 조회 결과가 다른 금액이면 확정하지 않는다.
    w.gateway.put(GatewayPayment {
        payment_key: "pay_1".to_string(),
        order_id: order.order_id.clone(),
        status: "DONE".to_string(),
        total_amount: 100,
    });
    let mismatch = w.svc.receive_payment_notice("pay_1").await.unwrap();

    assert_eq!(forged, NoticeOutcome::Ignored);
    assert_eq!(mismatch, NoticeOutcome::Ignored);
    assert_eq!(total(&w).await, (0, 0));
}

#[tokio::test]
async fn webhook_for_expired_payment_closes_pending_order() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.gateway.put(GatewayPayment {
        payment_key: "pay_1".to_string(),
        order_id: order.order_id.clone(),
        status: "EXPIRED".to_string(),
        total_amount: 5_000,
    });

    let notice = w.svc.receive_payment_notice("pay_1").await.unwrap();

    assert_eq!(notice, NoticeOutcome::Closed);
    let state = w
        .svc
        .abandon_donation(w.younghee, &order.order_id, "", "")
        .await
        .unwrap();
    assert_eq!(state.status, DonationStatus::Failed);
}

#[tokio::test]
async fn webhook_when_gateway_is_down_asks_for_retry() {
    let w = world().await;
    w.gateway.answer(Answer::Down);

    let result = w.svc.receive_payment_notice("pay_1").await;

    assert!(matches!(
        result,
        Err(ServiceError::PaymentServiceUnavailable)
    ));
}

#[tokio::test]
async fn live_donations_belong_to_that_live() {
    let w = world().await;
    w.svc.publish_started(&w.chulsoo_channel).await.unwrap();
    let live_id = w
        .svc
        .watch_page(&w.chulsoo_channel)
        .await
        .unwrap()
        .unwrap()
        .live
        .unwrap()
        .id;
    let live = BroadcastTarget::Live(live_id);
    let order = w
        .svc
        .prepare_donation(w.younghee, live, 1_000, "라이브 응원")
        .await
        .unwrap();

    w.svc
        .confirm_donation(w.younghee, approval(&order, "pay_live"))
        .await
        .unwrap();

    let live_board = w.svc.donations(live).await.unwrap();
    assert_eq!((live_board.count, live_board.total_amount), (1, 1_000));
    assert_eq!(live_board.recipient_name, "철수");
    // 같은 채널의 녹화방송 합계에는 들어가지 않는다(방송별 합계).
    assert_eq!(total(&w).await, (0, 0));
}

#[tokio::test]
async fn deleting_recording_keeps_donation_records() {
    let w = world().await;
    let order = w
        .svc
        .prepare_donation(w.younghee, w.recording, 5_000, "")
        .await
        .unwrap();
    w.svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();
    let BroadcastTarget::Recording(id) = w.recording else {
        unreachable!()
    };

    w.svc.delete_recording(w.chulsoo, id).await.unwrap();

    // 후원 기록은 남고(받는 채널 그대로), 지워진 녹화방송의 내역은 404다.
    let after = w
        .svc
        .confirm_donation(w.younghee, approval(&order, "pay_1"))
        .await
        .unwrap();
    assert!(after.already_processed);
    assert_eq!(after.target, None);
    assert_eq!(after.recipient_name, "철수");
    assert!(matches!(
        w.svc.donations(w.recording).await,
        Err(ServiceError::UnknownRecording)
    ));
}
