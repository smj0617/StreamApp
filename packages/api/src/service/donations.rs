//! 후원(Donation) 동작: 주문 만들기, 결제 승인 확인 후 확정, 승인 통지(웹훅) 처리, 취소·실패 기록, 방송별
//! 후원 내역과 합계.
//!
//! 흐름(ADR 0006):
//!
//! 1. [`AppService::prepare_donation`]: 로그인한 사용자가 방송과 금액을 고르면, 서버가 받는 채널(방송 주인)과
//!    금액을 정해 `pending` 주문을 만든다. 화면은 이 주문 그대로 결제 서비스 결제창을 연다.
//! 2. 결제창이 성공 주소로 돌아오면 [`AppService::confirm_donation`]: 돌아온 금액·주문번호를 주문과 비교하고,
//!    결제 서비스에 승인을 요청해 받은 결제(주문번호·금액·상태)를 다시 비교한 뒤에만 `confirmed`로 바꾼다.
//!    화면의 성공 표시만으로는 확정하지 않는다.
//! 3. 결제 서비스의 승인 통지(웹훅)는 [`AppService::receive_payment_notice`]: 본문을 믿지 않고 결제 키로
//!    결제를 조회해 2와 같은 확정 경로를 탄다.
//!
//! 중복 방지: 확정은 "아직 `confirmed`가 아닌 행"만 바꾸는 조건부 UPDATE 한 번이고, `order_id`·`payment_key`는
//! UNIQUE다. 같은 승인 통지가 몇 번 오든(새로고침, 웹훅 재전송, 동시 요청) 후원은 한 번만 적립된다.

use super::ids::{new_customer_key, new_order_id};
use super::payments::{GatewayError, GatewayPayment, PAYMENT_CLOSED, PAYMENT_DONE};
use super::{AppService, ServiceError};
use crate::dto::{
    BroadcastTarget, Donation, DonationBoard, DonationOrder, DonationResult, DonationStatus,
    PaymentApproval,
};
use crate::rules::{MAX_DONATION_MESSAGE_CHARS, MAX_DONATION_WON, MIN_DONATION_WON};

/// 승인 통지(웹훅)를 처리한 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeOutcome {
    /// 기다리던 주문을 이번 통지로 확정했다
    Confirmed,
    /// 이미 확정된 주문이라 아무것도 바꾸지 않았다(중복 통지)
    AlreadyConfirmed,
    /// 결제가 취소·중단·만료되어 기다리던 주문을 실패로 닫았다
    Closed,
    /// 이 앱의 주문이 아니거나 처리할 상태 변화가 아니라 무시했다
    Ignored,
}

/// 받는 사람: 방송의 채널과 그 주인.
struct Recipient {
    channel_id: String,
    channel_name: String,
    user_id: i64,
}

#[derive(sqlx::FromRow)]
struct DonationRow {
    order_id: String,
    donor_id: i64,
    recipient_channel_id: String,
    recipient_name: String,
    recording_id: Option<i64>,
    live_id: Option<i64>,
    amount: i64,
    status: String,
    payment_key: Option<String>,
    failure_reason: Option<String>,
}

impl DonationRow {
    fn status(&self) -> DonationStatus {
        match self.status.as_str() {
            "confirmed" => DonationStatus::Confirmed,
            "canceled" => DonationStatus::Canceled,
            "failed" => DonationStatus::Failed,
            _ => DonationStatus::Pending,
        }
    }

    fn target(&self) -> Option<BroadcastTarget> {
        match (self.recording_id, self.live_id) {
            (Some(id), _) => Some(BroadcastTarget::Recording(id)),
            (None, Some(id)) => Some(BroadcastTarget::Live(id)),
            (None, None) => None,
        }
    }

    fn result(&self, already_processed: bool) -> DonationResult {
        DonationResult {
            order_id: self.order_id.clone(),
            status: self.status(),
            amount: self.amount,
            recipient_name: self.recipient_name.clone(),
            recipient_channel_id: self.recipient_channel_id.clone(),
            target: self.target(),
            already_processed,
            failure_reason: self.failure_reason.clone(),
        }
    }
}

const SELECT_DONATION: &str = "SELECT donations.order_id, donations.donor_id,
            donations.recipient_channel_id, channels.name AS recipient_name,
            donations.recording_id, donations.live_id, donations.amount, donations.status,
            donations.payment_key, donations.failure_reason
     FROM donations JOIN channels ON channels.id = donations.recipient_channel_id";

#[derive(sqlx::FromRow)]
struct DonationListRow {
    id: i64,
    donor_nickname: String,
    amount: i64,
    message: String,
    confirmed_at_ms: i64,
}

/// 취소로 보는 결제창 오류 코드: 결제창을 닫음(`USER_CANCEL`), 결제 중 취소(`PAY_PROCESS_CANCELED`).
const CANCEL_CODES: [&str; 2] = ["USER_CANCEL", "PAY_PROCESS_CANCELED"];

impl AppService {
    /// 후원 주문 만들기: 로그인한 사용자가 방송 하나와 금액을 골랐다. 받는 채널은 방송에서 서버가 정한다.
    ///
    /// 거절: 결제 설정이 없으면 [`ServiceError::PaymentsNotConfigured`], 금액이 범위 밖이면
    /// [`ServiceError::DonationAmount`], 메시지가 길면 [`ServiceError::DonationMessageTooLong`], 없는 방송이면
    /// [`ServiceError::UnknownRecording`]·[`ServiceError::UnknownLive`], 자기 방송이면
    /// [`ServiceError::DonationToSelf`].
    pub async fn prepare_donation(
        &self,
        donor_id: i64,
        target: BroadcastTarget,
        amount: i64,
        message: &str,
    ) -> Result<DonationOrder, ServiceError> {
        let payments = self
            .payments
            .clone()
            .ok_or(ServiceError::PaymentsNotConfigured)?;
        if !(MIN_DONATION_WON..=MAX_DONATION_WON).contains(&amount) {
            return Err(ServiceError::DonationAmount);
        }
        let message = message.trim();
        if message.chars().count() > MAX_DONATION_MESSAGE_CHARS {
            return Err(ServiceError::DonationMessageTooLong);
        }
        let recipient = self.recipient(target).await?;
        if recipient.user_id == donor_id {
            return Err(ServiceError::DonationToSelf);
        }
        let (recording_id, live_id) = match target {
            BroadcastTarget::Recording(id) => (Some(id), None),
            BroadcastTarget::Live(id) => (None, Some(id)),
        };
        let order_id = new_order_id();
        sqlx::query(
            "INSERT INTO donations
                (order_id, donor_id, recipient_channel_id, recording_id, live_id, amount, message, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&order_id)
        .bind(donor_id)
        .bind(&recipient.channel_id)
        .bind(recording_id)
        .bind(live_id)
        .bind(amount)
        .bind(message)
        .bind(self.now_ms())
        .execute(&self.pool)
        .await
        .map_err(|error| match error {
            // 방송은 위에서 확인했으므로 남은 외래 키는 후원한 사용자다.
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => ServiceError::UnknownUser,
            other => other.into(),
        })?;
        Ok(DonationOrder {
            order_id,
            amount,
            order_name: format!("{}님 후원", recipient.channel_name),
            recipient_name: recipient.channel_name,
            client_key: payments.client_key,
            customer_key: new_customer_key(),
        })
    }

    /// 결제창의 성공 주소로 돌아온 승인 요청을 확인하고, 결제 서비스 승인까지 끝나면 후원을 확정한다.
    ///
    /// 같은 승인 요청이 다시 와도(새로고침) 이미 확정된 주문이면 결제 서비스를 다시 부르지 않고
    /// `already_processed`로 돌려준다(다시 적립하지 않는다).
    ///
    /// 거절: 없는 주문이면 [`ServiceError::UnknownDonation`], 주문한 사람이 아니면
    /// [`ServiceError::NotDonationOwner`], 돌아온 금액이 주문과 다르면 [`ServiceError::DonationAmountMismatch`]
    /// (결제 서비스에 승인을 요청하지 않고 주문을 실패로 닫는다), 결제 서비스가 거절하면
    /// [`ServiceError::PaymentRejected`], 결제 서비스가 알려 준 결제가 주문과 다르면
    /// [`ServiceError::PaymentMismatch`], 결제 서비스에 연결하지 못하면 [`ServiceError::PaymentServiceUnavailable`]
    /// (주문은 그대로 두어 다시 시도할 수 있다).
    pub async fn confirm_donation(
        &self,
        donor_id: i64,
        approval: PaymentApproval,
    ) -> Result<DonationResult, ServiceError> {
        let payments = self
            .payments
            .clone()
            .ok_or(ServiceError::PaymentsNotConfigured)?;
        let row = self
            .donation_by_order(&approval.order_id)
            .await?
            .ok_or(ServiceError::UnknownDonation)?;
        if row.donor_id != donor_id {
            return Err(ServiceError::NotDonationOwner);
        }
        if row.status() == DonationStatus::Confirmed {
            return if row.payment_key.as_deref() == Some(approval.payment_key.as_str()) {
                Ok(row.result(true))
            } else {
                Err(ServiceError::PaymentMismatch)
            };
        }
        // 결제창이 돌려준 금액을 먼저 주문과 비교한다. 다르면 화면에서 금액을 고친 것이므로 승인하지 않는다.
        if approval.amount != row.amount {
            self.close_donation(&row.order_id, "failed", "결제 금액이 주문과 다릅니다")
                .await?;
            return Err(ServiceError::DonationAmountMismatch);
        }
        // 금액은 화면이 보낸 값이 아니라 서버의 주문 금액으로 승인을 요청한다.
        let confirmed = payments
            .gateway
            .confirm(approval.payment_key, row.order_id.clone(), row.amount)
            .await;
        match confirmed {
            Ok(payment) => self.settle(row, payment).await,
            Err(GatewayError::Rejected { code, message }) => {
                tracing::info!("결제 승인 거절({}): {code} {message}", row.order_id);
                let status = if CANCEL_CODES.contains(&code.as_str()) {
                    "canceled"
                } else {
                    "failed"
                };
                self.close_donation(&row.order_id, status, &message).await?;
                Err(ServiceError::PaymentRejected(message))
            }
            Err(GatewayError::Unavailable(detail)) => {
                tracing::warn!(
                    "결제 서비스에 연결하지 못했습니다({}): {detail}",
                    row.order_id
                );
                Err(ServiceError::PaymentServiceUnavailable)
            }
        }
    }

    /// 결제 서비스의 결제 상태 변경 통지(웹훅)를 처리한다. 통지 본문은 믿지 않고, 결제 키로 결제 서비스에
    /// 결제를 조회해 그 결과로만 처리한다. 같은 통지가 여러 번 와도 확정은 한 번뿐이다.
    ///
    /// 결제 서비스에 연결하지 못하면 [`ServiceError::PaymentServiceUnavailable`]: 웹훅 어댑터는 200이 아닌
    /// 응답을 돌려 결제 서비스가 다시 보내게 한다.
    pub async fn receive_payment_notice(
        &self,
        payment_key: &str,
    ) -> Result<NoticeOutcome, ServiceError> {
        let Some(payments) = self.payments.clone() else {
            return Ok(NoticeOutcome::Ignored);
        };
        let payment = match payments.gateway.lookup(payment_key.to_owned()).await {
            Ok(payment) => payment,
            Err(GatewayError::Rejected { code, message }) => {
                tracing::info!("통지받은 결제를 조회할 수 없어 무시합니다: {code} {message}");
                return Ok(NoticeOutcome::Ignored);
            }
            Err(GatewayError::Unavailable(detail)) => {
                tracing::warn!("통지받은 결제를 조회하지 못했습니다: {detail}");
                return Err(ServiceError::PaymentServiceUnavailable);
            }
        };
        let Some(row) = self.donation_by_order(&payment.order_id).await? else {
            return Ok(NoticeOutcome::Ignored);
        };
        if payment.status == PAYMENT_DONE {
            return match self.settle(row, payment).await {
                Ok(result) if result.already_processed => Ok(NoticeOutcome::AlreadyConfirmed),
                Ok(_) => Ok(NoticeOutcome::Confirmed),
                // 결제가 주문과 다르면 확정하지 않는다. 통지 자체는 처리했다.
                Err(ServiceError::PaymentMismatch) => Ok(NoticeOutcome::Ignored),
                Err(other) => Err(other),
            };
        }
        if PAYMENT_CLOSED.contains(&payment.status.as_str())
            && row.status() == DonationStatus::Pending
        {
            let reason = format!("결제가 끝나지 않았습니다({})", payment.status);
            self.close_donation(&row.order_id, "failed", &reason)
                .await?;
            return Ok(NoticeOutcome::Closed);
        }
        Ok(NoticeOutcome::Ignored)
    }

    /// 결제창이 실패 주소로 돌아왔거나 결제창을 닫았을 때: 기다리던 주문을 취소·실패로 닫는다. 이미 확정된
    /// 주문은 바꾸지 않는다(화면의 실패 표시로 확정을 되돌리지 않는다).
    ///
    /// 거절: 없는 주문이면 [`ServiceError::UnknownDonation`], 주문한 사람이 아니면
    /// [`ServiceError::NotDonationOwner`].
    pub async fn abandon_donation(
        &self,
        donor_id: i64,
        order_id: &str,
        code: &str,
        message: &str,
    ) -> Result<DonationResult, ServiceError> {
        let row = self
            .donation_by_order(order_id)
            .await?
            .ok_or(ServiceError::UnknownDonation)?;
        if row.donor_id != donor_id {
            return Err(ServiceError::NotDonationOwner);
        }
        if row.status() == DonationStatus::Pending {
            let (status, reason) = if CANCEL_CODES.contains(&code) {
                ("canceled", "결제를 취소했습니다".to_owned())
            } else if message.trim().is_empty() {
                ("failed", format!("결제에 실패했습니다({code})"))
            } else {
                ("failed", message.trim().to_owned())
            };
            self.close_donation(order_id, status, &reason).await?;
        }
        let row = self
            .donation_by_order(order_id)
            .await?
            .ok_or(ServiceError::UnknownDonation)?;
        Ok(row.result(false))
    }

    /// 방송 하나의 확정된 후원 내역(최근 확정순)과 합계. 결제를 기다리거나 실패·취소된 주문은 넣지 않는다.
    /// 없는 방송이면 [`ServiceError::UnknownRecording`]·[`ServiceError::UnknownLive`].
    pub async fn donations(&self, target: BroadcastTarget) -> Result<DonationBoard, ServiceError> {
        let recipient = self.recipient(target).await?;
        let (column, id) = match target {
            BroadcastTarget::Recording(id) => ("recording_id", id),
            BroadcastTarget::Live(id) => ("live_id", id),
        };
        // 열 이름은 위의 두 값뿐이라 SQL에 그대로 넣어도 된다.
        let rows = sqlx::query_as::<_, DonationListRow>(&format!(
            "SELECT donations.id, users.nickname AS donor_nickname, donations.amount,
                    donations.message, donations.confirmed_at_ms
             FROM donations JOIN users ON users.id = donations.donor_id
             WHERE donations.{column} = ? AND donations.status = 'confirmed'
             ORDER BY donations.confirmed_at_ms DESC, donations.id DESC"
        ))
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let (count, total_amount) = sqlx::query_as::<_, (i64, i64)>(&format!(
            "SELECT COUNT(*), COALESCE(SUM(amount), 0) FROM donations
             WHERE {column} = ? AND status = 'confirmed'"
        ))
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        Ok(DonationBoard {
            donations: rows
                .into_iter()
                .map(|row| Donation {
                    id: row.id,
                    donor_nickname: row.donor_nickname,
                    amount: row.amount,
                    message: row.message,
                    confirmed_at_ms: row.confirmed_at_ms,
                })
                .collect(),
            total_amount,
            count,
            recipient_channel_id: recipient.channel_id,
            recipient_name: recipient.channel_name,
            recipient_user_id: recipient.user_id,
            payments_enabled: self.payments.is_some(),
            server_now_ms: self.now_ms(),
        })
    }

    /// 결제 서비스가 알려 준 결제를 주문과 비교하고, 맞으면 주문을 확정한다. 확정 성공 표시·웹훅이 함께 쓰는
    /// 유일한 확정 경로다.
    ///
    /// 확인하는 것: 결제의 주문번호와 금액이 서버의 주문과 같은지, 결제 상태가 승인 완료인지, 받는 채널이
    /// 주문을 만들 때 서버가 방송에서 정한 채널 그대로인지.
    async fn settle(
        &self,
        row: DonationRow,
        payment: GatewayPayment,
    ) -> Result<DonationResult, ServiceError> {
        if payment.order_id != row.order_id || payment.total_amount != row.amount {
            tracing::error!(
                "결제 정보가 주문과 다릅니다: 주문 {} {}원, 결제 {} {}원",
                row.order_id,
                row.amount,
                payment.order_id,
                payment.total_amount
            );
            if row.status() != DonationStatus::Confirmed {
                self.close_donation(&row.order_id, "failed", "결제 정보가 주문과 다릅니다")
                    .await?;
            }
            return Err(ServiceError::PaymentMismatch);
        }
        if payment.status != PAYMENT_DONE {
            return Err(ServiceError::PaymentRejected(format!(
                "결제가 승인되지 않았습니다({})",
                payment.status
            )));
        }
        // 받는 채널: 주문을 만들 때 서버가 방송에서 정한 채널이 지금도 그 방송의 채널이고, 후원한 사람 자신이
        // 아닌지 다시 본다. 녹화방송이 지워졌으면(방송 없음) 주문에 남은 채널 그대로 받는다.
        if let Some(target) = row.target() {
            match self.recipient(target).await {
                Ok(now)
                    if now.channel_id != row.recipient_channel_id
                        || now.user_id == row.donor_id =>
                {
                    if row.status() != DonationStatus::Confirmed {
                        self.close_donation(
                            &row.order_id,
                            "failed",
                            "후원 받는 채널이 주문과 다릅니다",
                        )
                        .await?;
                    }
                    return Err(ServiceError::DonationRecipientMismatch);
                }
                Ok(_) | Err(ServiceError::UnknownRecording | ServiceError::UnknownLive) => {}
                Err(other) => return Err(other),
            }
        }

        // 아직 확정되지 않은 주문만 바꾼다. 결제 서비스가 승인했다고 확인했으므로, 화면이 먼저 취소·실패로
        // 닫았던 주문이라도 확정한다(실제로 돈이 나갔다).
        let changed = sqlx::query(
            "UPDATE donations
             SET status = 'confirmed', payment_key = ?, confirmed_at_ms = ?, failure_reason = NULL
             WHERE order_id = ? AND status <> 'confirmed'",
        )
        .bind(&payment.payment_key)
        .bind(self.now_ms())
        .bind(&row.order_id)
        .execute(&self.pool)
        .await
        .map_err(|error| match error {
            // 다른 주문이 이미 같은 결제 키로 확정되었다.
            sqlx::Error::Database(db) if db.is_unique_violation() => ServiceError::PaymentMismatch,
            other => other.into(),
        })?
        .rows_affected();

        let saved = self
            .donation_by_order(&row.order_id)
            .await?
            .ok_or(ServiceError::UnknownDonation)?;
        if changed == 1 {
            tracing::info!("후원 확정: {} {}원", saved.order_id, saved.amount);
            return Ok(saved.result(false));
        }
        // 그 사이 다른 요청(새로고침·웹훅)이 먼저 확정했다. 같은 결제면 중복 통지다.
        if saved.payment_key.as_deref() == Some(payment.payment_key.as_str()) {
            Ok(saved.result(true))
        } else {
            Err(ServiceError::PaymentMismatch)
        }
    }

    /// 기다리던(`pending`) 주문만 취소·실패로 닫는다.
    async fn close_donation(
        &self,
        order_id: &str,
        status: &str,
        reason: &str,
    ) -> Result<(), ServiceError> {
        sqlx::query(
            "UPDATE donations SET status = ?, failure_reason = ?
             WHERE order_id = ? AND status = 'pending'",
        )
        .bind(status)
        .bind(reason)
        .bind(order_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn donation_by_order(&self, order_id: &str) -> Result<Option<DonationRow>, ServiceError> {
        Ok(sqlx::query_as::<_, DonationRow>(&format!(
            "{SELECT_DONATION} WHERE donations.order_id = ?"
        ))
        .bind(order_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// 방송의 받는 사람(채널과 채널 주인). 없는 방송이면 [`ServiceError::UnknownRecording`]·
    /// [`ServiceError::UnknownLive`].
    async fn recipient(&self, target: BroadcastTarget) -> Result<Recipient, ServiceError> {
        let (table, id, missing) = match target {
            BroadcastTarget::Recording(id) => ("recordings", id, ServiceError::UnknownRecording),
            BroadcastTarget::Live(id) => ("lives", id, ServiceError::UnknownLive),
        };
        // 표 이름은 위의 두 값뿐이라 SQL에 그대로 넣어도 된다.
        let found = sqlx::query_as::<_, (String, String, i64)>(&format!(
            "SELECT channels.id, channels.name, channels.user_id
             FROM {table} JOIN channels ON channels.id = {table}.channel_id
             WHERE {table}.id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let (channel_id, channel_name, user_id) = found.ok_or(missing)?;
        Ok(Recipient {
            channel_id,
            channel_name,
            user_id,
        })
    }
}
