//! 백엔드 HTTP API 호출. 서버 함수(`api` 크레이트의 `#[get]`)는 평범한 GET에 JSON으로 답한다.
//!
//! 응답 타입은 서버의 `api::dto`에서 데스크톱이 읽는 필드만 옮겨 적었다. 서버가 필드를 더해도 데스크톱은
//! 그대로 읽는다(모르는 필드는 무시). 서버 dto와 맞는지는 아래 테스트가 `api::dto` 값을 직렬화해 확인한다.
//! `api` 크레이트를 일반 의존성으로 두지 않은 까닭: `api`는 dioxus `fullstack` feature를 켜는데, 서버 주소가
//! 실행 중에 바뀌는 데스크톱에는 서버 함수 클라이언트(서버 주소를 한 번만 정한다)가 맞지 않고, 풀스택 앱으로
//! 번들될 여지도 없애기 위해서다.

use std::sync::LazyLock;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::server_url::ServerUrl;

/// 라이브 중인 채널 하나(`api::dto::LiveChannel`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct LiveChannel {
    pub channel_id: String,
    pub channel_name: String,
    pub title: String,
    pub started_at_ms: i64,
    /// HLS 재생 주소. 이 필드가 없던 서버도 읽을 수 있게 비어 있어도 된다(시청 화면은
    /// `GET /api/watch/{channel_id}`의 재생 주소를 쓴다).
    #[serde(default)]
    pub playback_url: String,
}

/// `GET /api/live-channels` 응답(`api::dto::LiveChannelList`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct LiveChannelList {
    pub channels: Vec<LiveChannel>,
    pub server_now_ms: i64,
}

/// 녹화방송 목록의 한 편(`api::dto::RecordingSummary`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct RecordingSummary {
    pub id: i64,
    pub title: String,
    pub channel_name: String,
    pub registered_at_ms: i64,
}

/// `GET /api/recordings` 응답(`api::dto::RecordingList`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct RecordingList {
    pub recordings: Vec<RecordingSummary>,
    pub server_now_ms: i64,
}

/// `GET /api/recordings/{id}` 응답(`api::dto::RecordingPage`). 없는 녹화방송이면 `null`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct RecordingPage {
    pub id: i64,
    pub title: String,
    pub channel_id: String,
    pub channel_name: String,
    /// 영상 주소. 서버는 `/media/recordings/<파일>`처럼 서버 기준 경로를 준다.
    pub video_url: String,
    pub registered_at_ms: i64,
    pub server_now_ms: i64,
}

/// `GET /api/watch/{channel_id}` 응답(`api::dto::WatchPage`). 없는 채널이면 `null`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct WatchPage {
    pub channel_id: String,
    pub channel_name: String,
    /// HLS 재생 주소. 전체 주소이거나 서버 기준 경로다([`ServerUrl::absolutize`]).
    pub playback_url: String,
    /// 지금 열린 라이브. 오프라인이면 `None`
    pub live: Option<OpenLive>,
}

/// 채널에서 지금 열린 라이브(`api::dto::OpenLive`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct OpenLive {
    /// 라이브 ID. 라이브 댓글([`Target::Live`])을 이 ID로 단다.
    pub id: i64,
    pub title: String,
    pub started_at_ms: i64,
}

/// 로그인한 사용자(`api::dto::User`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct User {
    pub id: i64,
    pub nickname: String,
}

/// 댓글이 붙는 방송(`api::dto::BroadcastTarget`). JSON은 `{"Recording": 7}`·`{"Live": 5}`다.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// 녹화방송 ID
    Recording(i64),
    /// 라이브 ID([`OpenLive::id`])
    Live(i64),
}

/// 댓글 하나(`api::dto::Comment`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Comment {
    pub id: i64,
    /// 쓴 사용자 ID. 화면은 작성자에게만 수정·삭제 버튼을 보이고, 권한은 서버가 다시 확인한다.
    pub author_id: i64,
    pub author_nickname: String,
    pub content: String,
    pub created_at_ms: i64,
    pub edited: bool,
}

/// 방송 하나의 댓글 목록(오래된 순, `api::dto::CommentList`).
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct CommentList {
    pub comments: Vec<Comment>,
    pub server_now_ms: i64,
}

/// 댓글 내용 최대 글자 수(`api::rules::MAX_COMMENT_CHARS`, 앞뒤 공백을 지운 뒤 센다). 서버가 다시 확인한다.
pub const MAX_COMMENT_CHARS: usize = 500;

/// 세션 토큰을 담는 쿠키 이름(서버 `api::server::current_user::COOKIE_NAME`).
pub const SESSION_COOKIE: &str = "streamapp_session";

/// API 호출 실패. `status`는 서버가 답했을 때의 HTTP 상태(연결 실패면 `None`), `message`는 화면에 보일 문구.
#[derive(Clone, Debug, PartialEq)]
pub struct ApiError {
    pub status: Option<u16>,
    pub message: String,
}

impl ApiError {
    /// 서버가 401(로그인 필요·세션 끝남)로 거절했는지.
    pub fn is_unauthorized(&self) -> bool {
        self.status == Some(401)
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// 앱 전체가 함께 쓰는 HTTP 클라이언트. 라이브 HLS 중계([`crate::hls_proxy`])도 쓴다.
pub static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("StreamApp-desktop/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP 클라이언트를 만들지 못했습니다")
});

/// API 응답 시간 제한. 라이브 HLS 중계에는 쓰지 않는다(LL-HLS는 재생 목록 요청을 길게 붙잡는다).
const API_TIMEOUT: Duration = Duration::from_secs(15);

/// 서버 함수를 부른다. `body`가 있으면 JSON 본문을 실은 POST, 없으면 GET이다. `token`이 있으면 세션 쿠키로
/// 싣는다. 성공하면 응답 헤더와 JSON 본문을 돌려준다.
async fn call<T: DeserializeOwned>(
    server: &ServerUrl,
    path: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> Result<(reqwest::header::HeaderMap, T), ApiError> {
    let url = server.join(path);
    let mut request = match &body {
        Some(body) => HTTP.post(&url).json(body),
        None => HTTP.get(&url),
    }
    .header("accept", "application/json")
    .timeout(API_TIMEOUT);
    if let Some(token) = token {
        request = request.header(reqwest::header::COOKIE, format!("{SESSION_COOKIE}={token}"));
    }
    let response = request.send().await.map_err(|e| ApiError {
        status: None,
        message: format!("서버({})에 연결하지 못했습니다: {e}", server.as_str()),
    })?;
    let status = response.status();
    let headers = response.headers().clone();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError {
            status: Some(status.as_u16()),
            message: rejection_message(status, &body),
        });
    }
    let value = response.json::<T>().await.map_err(|e| ApiError {
        status: Some(status.as_u16()),
        message: format!("서버 응답을 읽지 못했습니다({url}): {e}"),
    })?;
    Ok((headers, value))
}

/// 거절 응답에서 화면에 보일 문구를 꺼낸다. 서버 함수 오류 본문은 `{"data":{"ServerError":{"message":"…",…}}}`
/// 꼴이다(웹 화면이 보여 주는 문구와 같다). 그 꼴이 아니면 상태와 본문 앞부분을 보인다.
fn rejection_message(status: reqwest::StatusCode, body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/data/ServerError/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .filter(|message| !message.trim().is_empty());
    match message {
        Some(message) => message,
        None => {
            let body: String = body.chars().take(200).collect();
            format!("서버가 요청을 거절했습니다({status}): {body}")
        }
    }
}

async fn get_json<T: DeserializeOwned>(server: &ServerUrl, path: &str) -> Result<T, String> {
    call(server, path, None, None)
        .await
        .map(|(_, value)| value)
        .map_err(|e| e.message)
}

/// 세션 쿠키를 싣고 서버 함수를 부른다(로그인한 사용자로 하는 일).
async fn call_with_session<T: DeserializeOwned>(
    server: &ServerUrl,
    path: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> Result<T, ApiError> {
    call(server, path, token, body)
        .await
        .map(|(_, value)| value)
}

pub async fn live_channels(server: &ServerUrl) -> Result<LiveChannelList, String> {
    get_json(server, "/api/live-channels").await
}

pub async fn recordings(server: &ServerUrl) -> Result<RecordingList, String> {
    get_json(server, "/api/recordings").await
}

pub async fn recording_page(server: &ServerUrl, id: i64) -> Result<Option<RecordingPage>, String> {
    get_json(server, &format!("/api/recordings/{id}")).await
}

pub async fn watch_page(server: &ServerUrl, channel_id: &str) -> Result<Option<WatchPage>, String> {
    let encoded: String = url::form_urlencoded::byte_serialize(channel_id.as_bytes()).collect();
    get_json(server, &format!("/api/watch/{encoded}")).await
}

/// 로그인: 맞으면 사용자와, 서버가 쿠키로 준 세션 토큰을 돌려준다. 틀리면 401과 서버 문구.
pub async fn log_in(
    server: &ServerUrl,
    nickname: &str,
    password: &str,
) -> Result<(User, String), ApiError> {
    let body = serde_json::json!({ "nickname": nickname, "password": password });
    let (headers, user) = call::<User>(server, "/api/auth/login", None, Some(body)).await?;
    let token = session_token_from(&headers).ok_or_else(|| ApiError {
        status: None,
        message: "서버가 세션 쿠키를 주지 않았습니다.".to_string(),
    })?;
    Ok((user, token))
}

/// 로그아웃: 서버의 세션을 지운다.
pub async fn log_out(server: &ServerUrl, token: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    call_with_session(server, "/api/auth/logout", Some(token), Some(body)).await
}

/// 세션 토큰의 사용자. 세션이 끝났거나(로그아웃·기한) 모르는 토큰이면 `None`.
pub async fn current_user(server: &ServerUrl, token: &str) -> Result<Option<User>, ApiError> {
    call_with_session(server, "/api/current-user", Some(token), None).await
}

/// 방송 하나의 댓글 목록(로그인하지 않아도 볼 수 있다).
pub async fn comments(server: &ServerUrl, target: Target) -> Result<CommentList, String> {
    let path = match target {
        Target::Recording(id) => format!("/api/recordings/{id}/comments"),
        Target::Live(id) => format!("/api/lives/{id}/comments"),
    };
    get_json(server, &path).await
}

/// 댓글 쓰기(로그인 필요). 만든 댓글을 돌려준다.
pub async fn write_comment(
    server: &ServerUrl,
    token: Option<&str>,
    target: Target,
    content: &str,
) -> Result<Comment, ApiError> {
    let body = serde_json::json!({ "target": target, "content": content });
    call_with_session(server, "/api/comments", token, Some(body)).await
}

/// 댓글 고치기(쓴 사람만). 고친 댓글을 돌려준다.
pub async fn edit_comment(
    server: &ServerUrl,
    token: Option<&str>,
    id: i64,
    content: &str,
) -> Result<Comment, ApiError> {
    let body = serde_json::json!({ "id": id, "content": content });
    call_with_session(server, "/api/comments/edit", token, Some(body)).await
}

/// 댓글 지우기(쓴 사람만).
pub async fn delete_comment(
    server: &ServerUrl,
    token: Option<&str>,
    id: i64,
) -> Result<(), ApiError> {
    let body = serde_json::json!({ "id": id });
    call_with_session(server, "/api/comments/delete", token, Some(body)).await
}

/// 응답의 `Set-Cookie`에서 세션 토큰을 꺼낸다. 값이 빈 쿠키(지우기)는 무시한다.
fn session_token_from(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|cookie| {
            let pair = cookie.split(';').next()?.trim();
            let (name, value) = pair.split_once('=')?;
            (name.trim() == SESSION_COOKIE && !value.trim().is_empty())
                .then(|| value.trim().to_string())
        })
}

#[cfg(test)]
mod tests {
    //! 서버 dto(`api::dto`)를 직렬화한 JSON을 데스크톱 타입으로 읽을 수 있는지 확인한다.
    use super::*;

    fn roundtrip<S: serde::Serialize, D: DeserializeOwned>(value: &S) -> D {
        serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
    }

    #[test]
    fn 라이브_목록을_읽는다() {
        let server = api::dto::LiveChannelList {
            channels: vec![api::dto::LiveChannel {
                channel_id: "ch1".into(),
                channel_name: "채널".into(),
                title: "제목".into(),
                started_at_ms: 1,
                playback_url: "https://example.com/hls/ch1/index.m3u8".into(),
            }],
            server_now_ms: 2,
        };
        let read: LiveChannelList = roundtrip(&server);
        assert_eq!(read.channels[0].channel_id, "ch1");
        assert_eq!(read.channels[0].title, "제목");
        assert_eq!(
            read.channels[0].playback_url,
            "https://example.com/hls/ch1/index.m3u8"
        );
        assert_eq!(read.server_now_ms, 2);
    }

    #[test]
    fn 녹화방송_목록과_상세를_읽는다() {
        let list = api::dto::RecordingList {
            recordings: vec![api::dto::RecordingSummary {
                id: 7,
                title: "다시보기".into(),
                channel_name: "채널".into(),
                registered_at_ms: 1,
            }],
            server_now_ms: 2,
        };
        let read: RecordingList = roundtrip(&list);
        assert_eq!(read.recordings[0].id, 7);

        let page = Some(api::dto::RecordingPage {
            id: 7,
            title: "다시보기".into(),
            channel_id: "ch1".into(),
            owner_id: 3,
            channel_name: "채널".into(),
            video_url: format!("{}/a.mp4", api::RECORDING_MEDIA_PATH),
            registered_at_ms: 1,
            server_now_ms: 2,
        });
        let read: Option<RecordingPage> = roundtrip(&page);
        assert_eq!(read.unwrap().video_url, "/media/recordings/a.mp4");
        let missing: Option<RecordingPage> = roundtrip(&None::<api::dto::RecordingPage>);
        assert_eq!(missing, None);
    }

    #[test]
    fn 시청_페이지를_읽는다() {
        let page = Some(api::dto::WatchPage {
            channel_id: "ch1".into(),
            channel_name: "채널".into(),
            playback_url: "http://localhost:8888/ch1/index.m3u8".into(),
            live: Some(api::dto::OpenLive {
                id: 5,
                title: "라이브".into(),
                started_at_ms: 1,
            }),
        });
        let read: Option<WatchPage> = roundtrip(&page);
        let read = read.unwrap();
        assert_eq!(read.playback_url, "http://localhost:8888/ch1/index.m3u8");
        assert_eq!(read.live.unwrap().title, "라이브");
    }

    #[test]
    fn 라이브_id를_읽는다() {
        let live = api::dto::OpenLive {
            id: 5,
            title: "라이브".into(),
            started_at_ms: 1,
        };
        let read: OpenLive = roundtrip(&live);
        assert_eq!(read.id, 5);
    }

    #[test]
    fn 사용자와_댓글_목록을_읽는다() {
        let user = Some(api::dto::User {
            id: 2,
            nickname: "영희".into(),
        });
        let read: Option<User> = roundtrip(&user);
        assert_eq!(read.unwrap().nickname, "영희");
        let nobody: Option<User> = roundtrip(&None::<api::dto::User>);
        assert_eq!(nobody, None);

        let comment = api::dto::Comment {
            id: 9,
            author_id: 2,
            author_nickname: "영희".into(),
            content: "안녕".into(),
            created_at_ms: 1,
            edited: true,
        };
        let list = api::dto::CommentList {
            comments: vec![comment.clone()],
            server_now_ms: 2,
        };
        let read: CommentList = roundtrip(&list);
        assert_eq!(read.comments[0].author_id, 2);
        assert!(read.comments[0].edited);
        let one: Comment = roundtrip(&comment);
        assert_eq!(one.content, "안녕");
        // 지우기·로그아웃은 `()`를 돌려준다(본문 `null`).
        let unit: () = roundtrip(&());
        assert_eq!(unit, ());
    }

    #[test]
    fn 방송_대상은_서버와_같은_json이다() {
        for (mine, server) in [
            (
                Target::Recording(7),
                api::dto::BroadcastTarget::Recording(7),
            ),
            (Target::Live(5), api::dto::BroadcastTarget::Live(5)),
        ] {
            assert_eq!(
                serde_json::to_value(mine).unwrap(),
                serde_json::to_value(server).unwrap()
            );
        }
        assert_eq!(
            serde_json::to_string(&Target::Recording(7)).unwrap(),
            r#"{"Recording":7}"#
        );
    }

    #[test]
    fn 댓글_글자_수_제한은_서버와_같다() {
        assert_eq!(MAX_COMMENT_CHARS, api::rules::MAX_COMMENT_CHARS);
    }

    #[test]
    fn 서버_함수_오류에서_문구를_꺼낸다() {
        // 로컬 서버(scripts/dev.ps1)가 비로그인 `POST /api/comments`에 실제로 준 본문.
        let body = r#"{"message":"error running server function: 로그인이 필요합니다 (details: None)","code":401,"data":{"ServerError":{"message":"로그인이 필요합니다","code":401}}}"#;
        assert_eq!(
            rejection_message(reqwest::StatusCode::UNAUTHORIZED, body),
            "로그인이 필요합니다"
        );
        let other = rejection_message(reqwest::StatusCode::BAD_GATEWAY, "<html>bad</html>");
        assert!(other.contains("502"), "{other}");
        assert!(other.contains("<html>bad</html>"));
    }

    #[test]
    fn set_cookie에서_세션_토큰을_꺼낸다() {
        use reqwest::header::{HeaderMap, HeaderValue, SET_COOKIE};
        let mut headers = HeaderMap::new();
        headers.append(SET_COOKIE, HeaderValue::from_static("other=1; Path=/"));
        headers.append(
            SET_COOKIE,
            HeaderValue::from_static(
                "streamapp_session=abcDEF123; Path=/; Max-Age=2592000; SameSite=Lax; HttpOnly; Secure",
            ),
        );
        assert_eq!(session_token_from(&headers).as_deref(), Some("abcDEF123"));

        let mut cleared = HeaderMap::new();
        cleared.append(
            SET_COOKIE,
            HeaderValue::from_static(
                "streamapp_session=; Path=/; Max-Age=0; SameSite=Lax; HttpOnly",
            ),
        );
        assert_eq!(session_token_from(&cleared), None);
    }
}
