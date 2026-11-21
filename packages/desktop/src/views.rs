//! 화면: 홈(라이브·녹화방송 목록), 라이브 시청, 녹화방송 시청, 서버 설정. 로그인은 [`crate::auth`],
//! 시청 화면 아래 댓글은 [`crate::comments`]에 있다.
//! 채팅·후원은 데스크톱 범위 밖이고, 각 시청 화면의 "웹에서 열기"로 웹 앱을 연다.

use dioxus::prelude::*;

use crate::auth::AccountMenu;
use crate::client::{self, LiveChannel, RecordingSummary, Target};
use crate::comments::Comments;
use crate::player::{LivePlayer, RecordingPlayer};
use crate::server_url::{self, ServerUrl};
use crate::Route;

/// 앱 전체가 함께 쓰는 서버 주소. 바꾸면 목록·상세가 새 주소로 다시 불러온다.
#[derive(Clone, Copy)]
pub struct Server(pub Signal<ServerUrl>);

pub(crate) fn use_server() -> Signal<ServerUrl> {
    use_context::<Server>().0
}

/// 모든 화면에 공통인 레이아웃: 상단 바와 본문.
#[component]
pub fn Shell() -> Element {
    let server = use_server();
    rsx! {
        header { class: "topbar",
            Link { class: "brand", to: Route::Home {}, "StreamApp" }
            nav { class: "topnav",
                Link { to: Route::Home {}, "홈" }
                Link { to: Route::Settings {}, "서버 설정" }
            }
            span { class: "server-label", title: "연결한 서버", "{server.read().as_str()}" }
            AccountMenu {}
        }
        main { class: "page", Outlet::<Route> {} }
    }
}

/// 홈: 라이브 중인 채널과 녹화방송 목록.
#[component]
pub fn Home() -> Element {
    let server = use_server();
    let mut lives = use_resource(move || {
        let server = server();
        async move { client::live_channels(&server).await }
    });
    let mut recordings = use_resource(move || {
        let server = server();
        async move { client::recordings(&server).await }
    });

    rsx! {
        section { class: "home",
            div { class: "heading",
                h1 { "라이브" }
                button {
                    class: "secondary",
                    onclick: move |_| {
                        lives.restart();
                        recordings.restart();
                    },
                    "새로고침"
                }
            }
            match &*lives.read() {
                None => rsx! { p { class: "notice", "불러오는 중…" } },
                Some(Err(reason)) => rsx! { ConnectionError { reason: reason.clone() } },
                Some(Ok(list)) if list.channels.is_empty() => rsx! {
                    p { class: "notice", "지금 라이브 중인 채널이 없습니다." }
                },
                Some(Ok(list)) => rsx! {
                    ul { class: "card-list",
                        for live in list.channels.iter().cloned() {
                            LiveCard { key: "{live.channel_id}", live, server_now_ms: list.server_now_ms }
                        }
                    }
                },
            }
            h1 { "녹화방송" }
            match &*recordings.read() {
                None => rsx! { p { class: "notice", "불러오는 중…" } },
                Some(Err(reason)) => rsx! { ConnectionError { reason: reason.clone() } },
                Some(Ok(list)) if list.recordings.is_empty() => rsx! {
                    p { class: "notice", "아직 등록된 녹화방송이 없습니다." }
                },
                Some(Ok(list)) => rsx! {
                    ul { class: "card-list",
                        for recording in list.recordings.iter().cloned() {
                            RecordingCard { key: "{recording.id}", recording, server_now_ms: list.server_now_ms }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn ConnectionError(reason: String) -> Element {
    rsx! {
        div { class: "error", role: "alert",
            p { "{reason}" }
            p {
                "서버 주소가 맞는지 "
                Link { to: Route::Settings {}, "서버 설정" }
                "에서 확인하세요."
            }
        }
    }
}

#[component]
fn LiveCard(live: LiveChannel, server_now_ms: i64) -> Element {
    let elapsed = elapsed_text(server_now_ms - live.started_at_ms);
    rsx! {
        li {
            Link { class: "card", to: Route::Live { channel_id: live.channel_id.clone() },
                p { class: "card-top",
                    span { class: "badge live", "LIVE" }
                    span { class: "muted", "{elapsed}" }
                }
                h2 { class: "card-title", "{live.title}" }
                p { class: "muted", "{live.channel_name}" }
            }
        }
    }
}

#[component]
fn RecordingCard(recording: RecordingSummary, server_now_ms: i64) -> Element {
    let ago = ago_text(server_now_ms - recording.registered_at_ms);
    rsx! {
        li {
            Link { class: "card", to: Route::Recording { id: recording.id },
                p { class: "card-top",
                    span { class: "badge", "녹화" }
                    span { class: "muted", "{ago}" }
                }
                h2 { class: "card-title", "{recording.title}" }
                p { class: "muted", "{recording.channel_name}" }
            }
        }
    }
}

/// 라이브 시청: 채널 이름·라이브 제목과 플레이어.
#[component]
pub fn Live(channel_id: ReadSignal<String>) -> Element {
    let server = use_server();
    let mut page = use_resource(move || {
        let server = server();
        let channel_id = channel_id();
        async move { client::watch_page(&server, &channel_id).await }
    });

    let server_now = server();
    let content = match &*page.read() {
        None => rsx! { p { class: "notice", "불러오는 중…" } },
        Some(Err(reason)) => rsx! { ConnectionError { reason: reason.clone() } },
        Some(Ok(None)) => rsx! { p { class: "notice", "없는 채널입니다." } },
        Some(Ok(Some(watch))) => {
            // 서버가 준 재생 주소를 쓴다. 비어 있으면 배포 규칙(`<서버>/hls/<채널ID>/index.m3u8`)으로 만든다.
            let src = if watch.playback_url.trim().is_empty() {
                server_now.fallback_hls_url(&watch.channel_id)
            } else {
                server_now.absolutize(&watch.playback_url)
            };
            let web_url = server_now.join(&format!("/live/{}", watch.channel_id));
            match &watch.live {
                Some(live) => rsx! {
                    LivePlayer { key: "{src}", src: src.clone() }
                    h1 { class: "watch-title", "{live.title}" }
                    p { class: "muted", "{watch.channel_name}" }
                    WebLink { url: web_url }
                    Comments { target: Target::Live(live.id) }
                },
                None => rsx! {
                    h1 { class: "watch-title", "{watch.channel_name}" }
                    p { class: "notice",
                        "지금은 오프라인입니다. 라이브가 시작되면 새로고침하세요. 라이브 댓글은 라이브 중에 보입니다."
                    }
                    WebLink { url: web_url }
                },
            }
        }
    };

    rsx! {
        section { class: "watch",
            div { class: "heading",
                Link { class: "back", to: Route::Home {}, "← 목록" }
                button { class: "secondary", onclick: move |_| page.restart(), "새로고침" }
            }
            {content}
        }
    }
}

/// 녹화방송 시청: 제목·채널 이름과 플레이어.
#[component]
pub fn Recording(id: ReadSignal<i64>) -> Element {
    let server = use_server();
    let page = use_resource(move || {
        let server = server();
        let id = id();
        async move { client::recording_page(&server, id).await }
    });

    let server_now = server();
    let content = match &*page.read() {
        None => rsx! { p { class: "notice", "불러오는 중…" } },
        Some(Err(reason)) => rsx! { ConnectionError { reason: reason.clone() } },
        Some(Ok(None)) => rsx! { p { class: "notice", "없거나 지워진 녹화방송입니다." } },
        Some(Ok(Some(recording))) => {
            let src = server_now.absolutize(&recording.video_url);
            let ago = ago_text(recording.server_now_ms - recording.registered_at_ms);
            let web_url = server_now.join(&format!("/recordings/{}", recording.id));
            rsx! {
                RecordingPlayer { key: "{src}", src: src.clone() }
                h1 { class: "watch-title", "{recording.title}" }
                p { class: "muted", "{recording.channel_name} · {ago}" }
                WebLink { url: web_url }
                Comments { target: Target::Recording(recording.id) }
            }
        }
    };

    rsx! {
        section { class: "watch",
            div { class: "heading",
                Link { class: "back", to: Route::Home {}, "← 목록" }
            }
            {content}
        }
    }
}

/// 웹 앱에서 같은 화면을 여는 링크. 외부 주소 링크는 데스크톱 런처가 기본 브라우저로 연다.
#[component]
fn WebLink(url: String) -> Element {
    rsx! {
        p { class: "web-link",
            a { href: "{url}", "웹에서 열기" }
            span { class: "muted", " (채팅·후원은 웹에서)" }
        }
    }
}

/// 서버 설정: 연결할 백엔드 주소를 바꾼다.
#[component]
pub fn Settings() -> Element {
    let Server(mut server) = use_context::<Server>();
    let mut input = use_signal(|| server.read().as_str().to_string());
    let mut message = use_signal(|| None::<(bool, String)>);
    let built = ServerUrl::built_default();
    let file = server_url::settings_file()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(설정 폴더 없음)".to_string());

    let mut save = move |_: ()| match ServerUrl::parse(&input()) {
        Ok(url) => {
            let saved = url.save();
            input.set(url.as_str().to_string());
            server.set(url);
            message.set(Some(match saved {
                Ok(()) => (
                    true,
                    "저장했습니다. 목록을 새 서버에서 불러옵니다.".to_string(),
                ),
                Err(reason) => (false, format!("이번 실행에만 씁니다. {reason}")),
            }));
        }
        Err(reason) => message.set(Some((false, reason))),
    };

    let built_for_reset = built.clone();
    let reset = move |_| {
        ServerUrl::forget_saved();
        input.set(built_for_reset.as_str().to_string());
        server.set(built_for_reset.clone());
        message.set(Some((true, "기본 주소로 되돌렸습니다.".to_string())));
    };

    rsx! {
        section { class: "settings",
            h1 { "서버 설정" }
            p { class: "muted",
                "라이브·녹화방송 목록을 불러올 StreamApp 서버 주소입니다. 예: https://stream.example.com"
            }
            form {
                class: "server-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    save(());
                },
                input {
                    r#type: "text",
                    spellcheck: false,
                    aria_label: "서버 주소",
                    value: "{input}",
                    oninput: move |event| input.set(event.value()),
                }
                button { r#type: "submit", "저장" }
                button { r#type: "button", class: "secondary", onclick: reset, "기본값으로" }
            }
            if let Some((ok, text)) = message() {
                p { class: if ok { "notice" } else { "error" }, role: "status", "{text}" }
            }
            dl { class: "facts",
                dt { "기본 주소(빌드 값)" }
                dd { "{built.as_str()}" }
                dt { "저장 위치" }
                dd { "{file}" }
                dt { "앱 버전" }
                dd { {env!("CARGO_PKG_VERSION")} }
            }
        }
    }
}

/// 라이브 시작 후 경과 시간(웹 홈과 같은 문구): "방금 시작", "시작한 지 12분", "시작한 지 1시간 5분".
fn elapsed_text(elapsed_ms: i64) -> String {
    let minutes = elapsed_ms.max(0) / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "방금 시작".to_string(),
        (0, m) => format!("시작한 지 {m}분"),
        (h, m) => format!("시작한 지 {h}시간 {m}분"),
    }
}

/// 등록 후 지난 시간(웹과 같은 문구): "방금 전", "12분 전", "3시간 전", "5일 전".
pub(crate) fn ago_text(elapsed_ms: i64) -> String {
    let minutes = elapsed_ms.max(0) / 60_000;
    match (minutes / (60 * 24), minutes / 60, minutes) {
        (0, 0, 0) => "방금 전".to_string(),
        (0, 0, m) => format!("{m}분 전"),
        (0, h, _) => format!("{h}시간 전"),
        (d, _, _) => format!("{d}일 전"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 지난_시간_문구() {
        assert_eq!(elapsed_text(30_000), "방금 시작");
        assert_eq!(elapsed_text(65 * 60_000), "시작한 지 1시간 5분");
        assert_eq!(ago_text(-5), "방금 전");
        assert_eq!(ago_text(3 * 24 * 60 * 60_000), "3일 전");
    }
}
