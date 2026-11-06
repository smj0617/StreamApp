use api::dto::{
    BroadcastTarget, DonationBoard, DonationOrder, DonationResult, DonationStatus, PaymentApproval,
};
use api::rules::{
    won_text, DONATION_PRESETS_WON, MAX_DONATION_MESSAGE_CHARS, MAX_DONATION_WON, MIN_DONATION_WON,
};
use api::{DONATION_FAIL_PATH, DONATION_SUCCESS_PATH};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::recordings::ago_text;
use super::shell::{rejection_reason, CurrentUser, LoginRequired};
use crate::Route;

/// 토스페이먼츠 JavaScript SDK v2(결제창). 토스페이먼츠가 버전 경로 하나로만 제공한다.
const TOSS_SDK: &str = "https://js.tosspayments.com/v2/standard";

/// 결제창 열기. 서버가 만든 주문([`DonationOrder`])을 `dioxus.send`로 받아 그대로 결제를 요청한다
/// (금액·주문번호를 JS 문자열에 직접 넣지 않는다). 결제창은 성공·실패 주소로 페이지를 옮기므로, 이 스크립트가
/// 값을 돌려주는 것은 결제창을 열지 못했거나 PC에서 결제창을 닫았을 때(`USER_CANCEL`)뿐이다.
const REQUEST_PAYMENT_JS: &str = r#"
const order = await dioxus.recv();
const paths = await dioxus.recv();
// <script>가 아직 다 받아지지 않았을 수 있으므로 window.TossPayments를 기다린다(최대 15초).
for (let waited = 0; !window.TossPayments; waited += 100) {
  if (waited >= 15000) return { code: "SDK_NOT_LOADED", message: "결제 모듈을 불러오지 못했습니다" };
  await new Promise((resolve) => setTimeout(resolve, 100));
}
try {
  const payment = TossPayments(order.client_key).payment({ customerKey: order.customer_key });
  const fail = new URL(paths.fail, location.origin);
  // 결제 중 취소(PAY_PROCESS_CANCELED)는 실패 주소에 주문번호가 붙지 않으므로 직접 붙인다.
  fail.searchParams.set("donation", order.order_id);
  await payment.requestPayment({
    method: "CARD",
    amount: { currency: "KRW", value: order.amount },
    orderId: order.order_id,
    orderName: order.order_name,
    successUrl: new URL(paths.success, location.origin).href,
    failUrl: fail.href,
  });
  return null;
} catch (error) {
  return { code: String(error?.code ?? "UNKNOWN"), message: String(error?.message ?? error) };
}
"#;

/// 결제창 스크립트에 넘기는 성공·실패 경로.
#[derive(Serialize)]
struct RedirectPaths {
    success: &'static str,
    fail: &'static str,
}

/// 결제창 스크립트가 돌려준 오류(결제창을 열지 못함, 닫음).
#[derive(Deserialize)]
struct PaymentWindowError {
    code: String,
    message: String,
}

/// 방송 하나(녹화방송·라이브)의 후원: 확정된 후원 내역과 합계는 누구나 보고, 로그인하면 방송 주인에게 후원한다.
/// 내역에는 서버가 결제 승인을 확인해 확정한 후원만 나온다.
#[component]
pub fn Donations(target: BroadcastTarget) -> Element {
    // 서버 렌더링 때 불러온 내역을 HTML에 넣고, 클라이언트는 그 결과를 이어받는다.
    let loaded = use_server_future(move || async move {
        match target {
            BroadcastTarget::Recording(id) => api::recording_donations(id).await,
            BroadcastTarget::Live(id) => api::live_donations(id).await,
        }
    })?;
    let state = (*loaded.read()).clone();

    rsx! {
        section { class: "donations", aria_label: "후원",
            match state {
                Some(Ok(board)) => rsx! {
                    DonationPanel { board, target }
                },
                Some(Err(error)) => {
                    let reason = rejection_reason(error);
                    rsx! {
                        h2 { class: "comments-heading", "후원" }
                        p { class: "form-error", role: "alert", "후원 내역을 불러오지 못했습니다: {reason}" }
                    }
                }
                None => rsx! {},
            }
        }
    }
}

#[component]
fn DonationPanel(board: DonationBoard, target: BroadcastTarget) -> Element {
    let CurrentUser(current) = use_context::<CurrentUser>();
    let me = current.read().as_ref().map(|user| user.id);
    let total = won_text(board.total_amount);

    rsx! {
        h2 { class: "comments-heading",
            "후원 {board.count}건 · 합계 "
            strong { class: "donation-total", "{total}" }
        }
        if me.is_none() {
            LoginRequired { what: "{board.recipient_name}님에게 후원할 수 있습니다." }
        } else if me == Some(board.recipient_user_id) {
            p { class: "muted small", "내 방송에는 후원할 수 없습니다." }
        } else if !board.payments_enabled {
            p { class: "notice",
                "후원 결제가 설정되지 않았습니다. 서버에 토스페이먼츠 테스트 키(STREAMAPP_TOSS_CLIENT_KEY, STREAMAPP_TOSS_SECRET_KEY)를 넣고 다시 시작하세요."
            }
        } else {
            DonateForm { target, recipient_name: board.recipient_name.clone() }
        }
        if board.donations.is_empty() {
            p { class: "muted", "아직 후원이 없습니다." }
        }
        ul { class: "donation-list",
            for donation in board.donations.iter().cloned() {
                li { key: "{donation.id}", class: "donation",
                    div { class: "comment-meta",
                        span { class: "comment-author", "{donation.donor_nickname}" }
                        span { class: "donation-amount", " {won_text(donation.amount)}" }
                        span { class: "muted small", " · {ago_text(board.server_now_ms - donation.confirmed_at_ms)}" }
                    }
                    if !donation.message.is_empty() {
                        p { class: "comment-content", "{donation.message}" }
                    }
                }
            }
        }
    }
}

/// 후원 금액·메시지를 고르고 결제창을 연다. 받는 사람은 이 방송의 채널 주인이고, 서버가 방송에서 다시 정한다.
#[component]
fn DonateForm(target: BroadcastTarget, recipient_name: String) -> Element {
    let mut amount = use_signal(|| DONATION_PRESETS_WON[1]);
    let mut message = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);
    let mut error = use_signal(|| None::<String>);
    let message_length = message.read().trim().chars().count();
    let amount_ok = (MIN_DONATION_WON..=MAX_DONATION_WON).contains(&amount());
    let min = won_text(MIN_DONATION_WON);
    let max = won_text(MAX_DONATION_WON);

    let donate = move || async move {
        busy.set(true);
        notice.set(None);
        error.set(None);
        match api::prepare_donation(target, amount(), message()).await {
            Ok(order) => {
                if let Some(failure) = open_payment_window(&order).await {
                    // 결제창을 열지 못했거나 닫았다: 서버의 주문을 취소·실패로 닫는다.
                    let _ = api::abandon_donation(
                        order.order_id.clone(),
                        failure.code.clone(),
                        failure.message.clone(),
                    )
                    .await;
                    if failure.code == "USER_CANCEL" {
                        notice.set(Some(
                            "결제를 취소했습니다. 후원하지 않았습니다.".to_string(),
                        ));
                    } else {
                        error.set(Some(format!(
                            "결제창을 열지 못했습니다: {}",
                            failure.message
                        )));
                    }
                }
            }
            Err(e) => error.set(Some(rejection_reason(e))),
        }
        busy.set(false);
    };

    rsx! {
        document::Script { src: TOSS_SDK }
        div { class: "donate-form",
            p { class: "donate-recipient",
                "받는 사람 "
                strong { "{recipient_name}" }
                span { class: "muted small", " (이 방송의 채널 주인)" }
            }
            div { class: "donate-presets", role: "group", aria_label: "후원 금액 고르기",
                for preset in DONATION_PRESETS_WON {
                    button {
                        key: "{preset}",
                        class: if amount() == preset { "preset selected" } else { "preset secondary" },
                        aria_pressed: amount() == preset,
                        onclick: move |_| amount.set(preset),
                        "{won_text(preset)}"
                    }
                }
            }
            label { class: "donate-amount",
                "금액(원) "
                input {
                    r#type: "number",
                    min: "{MIN_DONATION_WON}",
                    max: "{MAX_DONATION_WON}",
                    step: "1000",
                    value: "{amount}",
                    oninput: move |event| amount.set(event.value().trim().parse().unwrap_or(0)),
                }
            }
            textarea {
                aria_label: "후원 메시지",
                placeholder: "후원 메시지(선택)",
                rows: "2",
                value: "{message}",
                oninput: move |event| message.set(event.value()),
            }
            div { class: "comment-writer-row",
                span { class: if message_length > MAX_DONATION_MESSAGE_CHARS { "chat-count over" } else { "chat-count" },
                    "{message_length}/{MAX_DONATION_MESSAGE_CHARS}"
                }
                button {
                    disabled: busy() || !amount_ok || message_length > MAX_DONATION_MESSAGE_CHARS,
                    onclick: move |_| async move { donate().await },
                    if busy() { "결제창 여는 중…" } else { "{won_text(amount())} 후원하기" }
                }
            }
            if !amount_ok {
                p { class: "form-error", role: "alert", "후원 금액은 {min}~{max}입니다." }
            }
            p { class: "muted small",
                "토스페이먼츠 테스트 결제입니다. 실제로 돈이 나가지 않습니다."
            }
            if let Some(text) = notice() {
                p { class: "muted small", role: "status", "{text}" }
            }
            if let Some(reason) = error() {
                p { class: "form-error", role: "alert", "{reason}" }
            }
        }
    }
}

/// 결제창을 연다. 결제창이 페이지를 옮기면 돌아오지 않는다. 열지 못했거나 닫았으면 그 오류를 돌려준다.
async fn open_payment_window(order: &DonationOrder) -> Option<PaymentWindowError> {
    let eval = document::eval(REQUEST_PAYMENT_JS);
    let paths = RedirectPaths {
        success: DONATION_SUCCESS_PATH,
        fail: DONATION_FAIL_PATH,
    };
    if eval.send(order.clone()).is_err() || eval.send(paths).is_err() {
        return Some(PaymentWindowError {
            code: "SCRIPT_FAILED".to_string(),
            message: "결제 스크립트를 실행하지 못했습니다".to_string(),
        });
    }
    eval.join::<Option<PaymentWindowError>>()
        .await
        .unwrap_or_else(|e| {
            Some(PaymentWindowError {
                code: "SCRIPT_FAILED".to_string(),
                message: e.to_string(),
            })
        })
}

/// 쿼리 문자열(라우터가 이미 퍼센트 디코딩한 값)에서 이름이 `key`인 값.
fn query_value(query: &str, key: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value.to_string())
}

/// 후원한 방송으로 돌아가는 주소. 녹화방송이 지워졌거나 라이브면 받는 채널의 시청 페이지.
fn back_route(result: &DonationResult) -> Route {
    match result.target {
        Some(BroadcastTarget::Recording(id)) => Route::RecordingDetail { id },
        _ => Route::Watch {
            channel_id: result.recipient_channel_id.clone(),
        },
    }
}

/// 결제창의 성공 주소 `/donations/success?paymentKey=…&orderId=…&amount=…`. 화면에 결제 성공을 표시하지 않고,
/// 받은 값을 서버에 보내 서버가 결제 승인과 금액·받는 채널을 확인한 결과만 보여 준다.
/// 새로고침하면 같은 값이 다시 가지만, 서버가 중복으로 처리해 다시 적립하지 않는다.
#[component]
pub fn DonationSuccess(query: String) -> Element {
    let approval = PaymentApproval {
        payment_key: query_value(&query, "paymentKey").unwrap_or_default(),
        order_id: query_value(&query, "orderId").unwrap_or_default(),
        amount: query_value(&query, "amount")
            .and_then(|amount| amount.parse().ok())
            .unwrap_or(0),
    };
    let mut outcome = use_signal(|| None::<Result<DonationResult, String>>);

    // 브라우저에서 화면을 그린 뒤 한 번만 서버에 확인을 요청한다(서버 렌더링 중에는 부르지 않는다).
    use_effect(move || {
        let approval = approval.clone();
        spawn(async move {
            let result = api::confirm_donation(approval)
                .await
                .map_err(rejection_reason);
            outcome.set(Some(result));
        });
    });

    rsx! {
        section { class: "donation-result",
            h1 { "후원 결제 확인" }
            match outcome() {
                None => rsx! {
                    p { role: "status", "서버가 결제 승인을 확인하는 중입니다…" }
                },
                Some(Ok(result)) => rsx! {
                    if result.already_processed {
                        p { class: "notice", role: "status",
                            "이미 확정된 후원입니다. 같은 결제를 다시 받아도 한 번만 적립합니다."
                        }
                    } else {
                        p { class: "donation-done", role: "status",
                            "{result.recipient_name}님에게 {won_text(result.amount)} 후원을 확정했습니다."
                        }
                    }
                    p { class: "muted small", "주문번호 {result.order_id}" }
                    Link { to: back_route(&result), "방송으로 돌아가기" }
                },
                Some(Err(reason)) => rsx! {
                    p { class: "form-error", role: "alert", "후원을 확정하지 못했습니다: {reason}" }
                    Link { to: Route::Home {}, "홈으로" }
                },
            }
        }
    }
}

/// 결제창의 실패 주소 `/donations/fail?code=…&message=…&donation=<주문번호>`. 서버의 주문을 취소·실패로 닫고
/// 결과를 보여 준다. 후원은 적립되지 않는다.
#[component]
pub fn DonationFail(query: String) -> Element {
    let code = query_value(&query, "code").unwrap_or_default();
    let message = query_value(&query, "message").unwrap_or_default();
    let order_id = query_value(&query, "donation").or_else(|| query_value(&query, "orderId"));
    let mut outcome = use_signal(|| None::<Result<DonationResult, String>>);

    let (code_for_effect, message_for_effect) = (code.clone(), message.clone());
    use_effect(move || {
        let Some(order_id) = order_id.clone() else {
            return;
        };
        let (code, message) = (code_for_effect.clone(), message_for_effect.clone());
        spawn(async move {
            let result = api::abandon_donation(order_id, code, message)
                .await
                .map_err(rejection_reason);
            outcome.set(Some(result));
        });
    });

    let canceled = code == "PAY_PROCESS_CANCELED" || code == "USER_CANCEL";
    rsx! {
        section { class: "donation-result",
            h1 { if canceled { "후원 결제 취소" } else { "후원 결제 실패" } }
            if canceled {
                p { class: "notice", role: "status", "결제를 취소했습니다. 후원하지 않았습니다." }
            } else {
                p { class: "form-error", role: "alert", "결제에 실패했습니다: {message} ({code})" }
            }
            match outcome() {
                Some(Ok(result)) if result.status == DonationStatus::Confirmed => rsx! {
                    // 실패 표시가 와도 서버가 이미 결제 승인을 확인한 주문이면 그대로 둔다.
                    p { class: "muted small", "이 주문은 이미 확정된 후원입니다." }
                    Link { to: back_route(&result), "방송으로 돌아가기" }
                },
                Some(Ok(result)) => rsx! {
                    Link { to: back_route(&result), "방송으로 돌아가기" }
                },
                Some(Err(reason)) => rsx! {
                    p { class: "muted small", "주문 상태를 바꾸지 못했습니다: {reason}" }
                    Link { to: Route::Home {}, "홈으로" }
                },
                None => rsx! {
                    Link { to: Route::Home {}, "홈으로" }
                },
            }
        }
    }
}
