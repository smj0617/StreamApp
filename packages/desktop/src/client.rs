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
use serde::Deserialize;

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
    pub title: String,
    pub started_at_ms: i64,
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

async fn get_json<T: DeserializeOwned>(server: &ServerUrl, path: &str) -> Result<T, String> {
    let url = server.join(path);
    let response = HTTP
        .get(&url)
        .header("accept", "application/json")
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("서버({})에 연결하지 못했습니다: {e}", server.as_str()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let body: String = body.chars().take(200).collect();
        return Err(format!("서버가 요청을 거절했습니다({status}): {body}"));
    }
    response
        .json::<T>()
        .await
        .map_err(|e| format!("서버 응답을 읽지 못했습니다({url}): {e}"))
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
}
