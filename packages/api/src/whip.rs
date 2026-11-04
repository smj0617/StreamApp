//! 휴대폰 방송 WHIP 중계: 브라우저의 송출 시작·종료 요청을 MediaMTX WebRTC 서버로 넘기는 얇은 어댑터.
//!
//! 쿠키의 현재 사용자로 채널 ID와 송출 키를 찾아 `<WebRTC 주소>/<채널ID>/whip?key=<송출 키>`로 넘긴다.
//! 그래서 송출 키는 휴대폰 브라우저에 보내지 않는다. 송출 허용 판단은 OBS 송출과 같이 MediaMTX가
//! 앱의 인증 엔드포인트([`crate::mediamtx`])에 묻는다(ADR 0004).
//!
//! Dioxus에 의존하지 않는 axum [`Router`]라서 앱은 `.merge(..)`하고, 테스트는 가짜 MediaMTX를 띄워 요청을
//! 직접 넣는다(경계 B). [`crate::mediamtx`]처럼 fallback을 두지 않는다.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, post},
    Router,
};

use crate::server::{current_user, json_message};
use crate::service::AppService;
use crate::BROADCAST_WHIP_PATH;

/// MediaMTX가 답하기를 기다리는 시간. WHIP 응답은 MediaMTX가 ICE 후보를 모은 뒤에 온다.
const MEDIAMTX_TIMEOUT: Duration = Duration::from_secs(15);

/// MediaMTX에 연결하지 못했을 때 보여 줄 사유.
const MEDIAMTX_UNREACHABLE: &str =
    "미디어 서버(MediaMTX)에 연결하지 못했습니다. PC에서 dev.ps1로 앱과 MediaMTX를 함께 띄웠는지 확인해 주세요";

/// 이미 라이브 중인 채널로 송출을 시작하려 할 때 보여 줄 사유.
const ALREADY_LIVE: &str =
    "이 채널은 이미 방송 중입니다. 다른 휴대폰이나 OBS에서 하는 방송을 먼저 끝내 주세요";

#[derive(Clone)]
struct WhipState {
    service: AppService,
    /// MediaMTX WebRTC 서버 주소(끝에 `/` 없음), 예: `http://127.0.0.1:8889`
    webrtc_base_url: Arc<str>,
    client: reqwest::Client,
}

/// 휴대폰 방송 WHIP 중계 라우터. `webrtc_base_url`은 MediaMTX WebRTC 서버 주소다
/// (앱은 [`crate::mediamtx::WEBRTC_BASE_URL`], 테스트는 가짜 서버 주소).
pub fn router(service: AppService, webrtc_base_url: &str) -> Router {
    let state = WhipState {
        service,
        webrtc_base_url: webrtc_base_url.trim_end_matches('/').into(),
        client: reqwest::Client::builder()
            .timeout(MEDIAMTX_TIMEOUT)
            .build()
            .expect("reqwest 클라이언트 만들기"),
    };
    Router::new()
        .route(BROADCAST_WHIP_PATH, post(start))
        .route(&format!("{BROADCAST_WHIP_PATH}/{{session}}"), delete(stop))
        .with_state(state)
}

/// 현재 사용자의 채널을 찾지 못한 이유.
enum NoChannel {
    /// 사용자를 고르지 않았거나 기억된 사용자가 DB에 없다(400).
    NotSelected,
    /// DB 오류(500).
    Internal,
}

impl IntoResponse for NoChannel {
    fn into_response(self) -> Response {
        match self {
            NoChannel::NotSelected => {
                json_message(StatusCode::BAD_REQUEST, current_user::NOT_SELECTED)
            }
            NoChannel::Internal => json_message(
                StatusCode::INTERNAL_SERVER_ERROR,
                "서버 내부 오류가 발생했습니다",
            ),
        }
    }
}

/// 쿠키의 현재 사용자 채널 (채널 ID, 송출 키).
async fn my_channel(state: &WhipState, headers: &HeaderMap) -> Result<(String, String), NoChannel> {
    let user_id = current_user::remembered_id(headers).ok_or(NoChannel::NotSelected)?;
    match state.service.my_channel_settings(user_id).await {
        Ok(Some(settings)) => Ok((settings.channel_id, settings.stream_key)),
        Ok(None) => Err(NoChannel::NotSelected),
        Err(error) => {
            tracing::error!("휴대폰 방송: 채널을 찾지 못했습니다: {error}");
            Err(NoChannel::Internal)
        }
    }
}

/// 송출 시작: SDP 제안을 MediaMTX로 넘기고 SDP 응답을 돌려준다.
async fn start(State(state): State<WhipState>, headers: HeaderMap, offer: String) -> Response {
    let (channel_id, stream_key) = match my_channel(&state, &headers).await {
        Ok(channel) => channel,
        Err(no_channel) => return no_channel.into_response(),
    };
    if !offer.starts_with("v=") {
        return json_message(StatusCode::BAD_REQUEST, "SDP 제안이 아닙니다");
    }
    // 이미 라이브 중이면(다른 휴대폰, OBS 등) 넘기지 않고 거절한다. MediaMTX는 WebRTC 시작 요청에 먼저
    // 201로 답하고 영상 연결이 맺어진 뒤에야 송출을 거절하므로(`overridePublisher: false`), 휴대폰은
    // 거절 사유를 알 수 없다.
    match state.service.watch_page(&channel_id).await {
        Ok(Some(page)) if page.live.is_some() => {
            return json_message(StatusCode::CONFLICT, ALREADY_LIVE);
        }
        Ok(_) => {}
        Err(error) => {
            tracing::error!("휴대폰 방송: 라이브 상태를 확인하지 못했습니다: {error}");
            return NoChannel::Internal.into_response();
        }
    }
    let key: String = form_urlencoded::byte_serialize(stream_key.as_bytes()).collect();
    let url = format!("{}/{channel_id}/whip?key={key}", state.webrtc_base_url);
    let response = match state
        .client
        .post(&url)
        .header(header::CONTENT_TYPE, "application/sdp")
        .body(offer)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!("휴대폰 방송: MediaMTX WHIP 요청 실패(채널 '{channel_id}'): {error}");
            return json_message(StatusCode::BAD_GATEWAY, MEDIAMTX_UNREACHABLE);
        }
    };

    let status = response.status();
    let session = response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(session_from_location);
    let body = response.text().await.unwrap_or_default();
    match (status.as_u16(), session) {
        (201, Some(session)) => {
            tracing::info!("휴대폰 방송 연결: 채널 '{channel_id}'");
            (
                StatusCode::CREATED,
                [
                    (header::CONTENT_TYPE, "application/sdp".to_string()),
                    (header::LOCATION, format!("{BROADCAST_WHIP_PATH}/{session}")),
                ],
                body,
            )
                .into_response()
        }
        (201, None) => {
            tracing::error!(
                "휴대폰 방송: MediaMTX 응답에 세션 주소가 없습니다(채널 '{channel_id}')"
            );
            json_message(
                StatusCode::BAD_GATEWAY,
                "미디어 서버의 응답을 이해하지 못했습니다",
            )
        }
        (code, _) => {
            tracing::warn!(
                "휴대폰 방송: MediaMTX가 거절했습니다(채널 '{channel_id}', HTTP {code}): {body}"
            );
            json_message(
                StatusCode::CONFLICT,
                &format!(
                    "미디어 서버가 송출을 거절했습니다(HTTP {code}). OBS 등 다른 곳에서 이미 이 채널로 송출 중인지 확인해 주세요"
                ),
            )
        }
    }
}

/// 송출 종료: MediaMTX의 WHIP 세션을 지운다. 세션이 이미 없으면(연결이 먼저 끊겼다) 끝난 것으로 본다.
async fn stop(
    State(state): State<WhipState>,
    headers: HeaderMap,
    Path(session): Path<String>,
) -> Response {
    if !valid_session(&session) {
        return json_message(StatusCode::BAD_REQUEST, "잘못된 송출 세션입니다");
    }
    let (channel_id, stream_key) = match my_channel(&state, &headers).await {
        Ok(channel) => channel,
        Err(no_channel) => return no_channel.into_response(),
    };
    // 세션 주소에도 송출 시작 때처럼 송출 키를 붙인다(MediaMTX가 준 세션 주소의 모양 그대로).
    let key: String = form_urlencoded::byte_serialize(stream_key.as_bytes()).collect();
    let url = format!(
        "{}/{channel_id}/whip/{session}?key={key}",
        state.webrtc_base_url
    );
    match state.client.delete(&url).send().await {
        Ok(response) if response.status().is_success() || response.status() == 404 => {
            tracing::info!("휴대폰 방송 종료: 채널 '{channel_id}'");
            StatusCode::OK.into_response()
        }
        Ok(response) => {
            let code = response.status().as_u16();
            tracing::warn!(
                "휴대폰 방송: 세션을 지우지 못했습니다(채널 '{channel_id}', HTTP {code})"
            );
            json_message(
                StatusCode::BAD_GATEWAY,
                &format!("미디어 서버가 송출 종료를 거절했습니다(HTTP {code})"),
            )
        }
        Err(error) => {
            tracing::warn!("휴대폰 방송: 세션 종료 요청 실패(채널 '{channel_id}'): {error}");
            json_message(StatusCode::BAD_GATEWAY, MEDIAMTX_UNREACHABLE)
        }
    }
}

/// MediaMTX가 준 세션 주소 `/<채널ID>/whip/<세션>?key=<송출 키>`에서 세션 ID만 꺼낸다.
/// 쿼리(송출 키)는 휴대폰에 돌려주지 않는다.
fn session_from_location(location: &str) -> Option<String> {
    let path = location.split(['?', '#']).next()?;
    let session = path.trim_end_matches('/').rsplit('/').next()?;
    valid_session(session).then(|| session.to_owned())
}

/// MediaMTX 세션 ID(UUID) 모양인지: 영문·숫자·`-`만, 1~64자. 경로에 그대로 넣으므로 다른 문자를 막는다.
fn valid_session(session: &str) -> bool {
    !session.is_empty()
        && session.len() <= 64
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}
