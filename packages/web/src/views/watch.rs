use api::dto::WatchPage;
use dioxus::prelude::*;

use super::hls_player::HlsPlayer;
use super::shell::rejection_reason;
use crate::Route;

/// 시청 페이지 `/live/<채널ID>`: 라이브 중이면 영상을 자동 재생한다. 사용자를 고르지 않아도 볼 수 있다.
///
/// 들어올 때의 상태(라이브 중, 오프라인, 없는 채널)만 보여 준다. 새로고침해야 갱신된다.
#[component]
pub fn Watch(channel_id: ReadSignal<String>) -> Element {
    // 서버 렌더링 때 불러온 정보를 HTML에 넣고, 클라이언트는 그 결과를 이어받는다.
    // 다른 채널 주소로 옮겨 가면(같은 컴포넌트가 재사용되어도) 다시 불러온다.
    let loaded = use_server_future(move || api::watch_page(channel_id()))?;
    let state = (*loaded.read()).clone();

    rsx! {
        section { class: "watch",
            match state {
                Some(Ok(Some(page))) => rsx! {
                    WatchChannel { page }
                },
                Some(Ok(None)) => rsx! {
                    h1 { "채널을 찾을 수 없습니다" }
                    p { class: "notice",
                        "주소의 채널 ID({channel_id})에 해당하는 채널이 없습니다. "
                        Link { to: Route::Home {}, "홈에서 라이브 중인 채널을 고르세요." }
                    }
                },
                Some(Err(error)) => {
                    let reason = rejection_reason(error);
                    rsx! {
                        p { class: "form-error", role: "alert", "채널 정보를 불러오지 못했습니다: {reason}" }
                    }
                }
                None => rsx! {},
            }
        }
    }
}

/// 있는 채널: 라이브 중이면 플레이어와 라이브 제목, 아니면 "오프라인".
#[component]
fn WatchChannel(page: WatchPage) -> Element {
    match page.live {
        Some(live) => rsx! {
            HlsPlayer { key: "{page.playback_url}", src: page.playback_url.clone() }
            div { class: "watch-info",
                h1 { class: "watch-title",
                    span { class: "live-badge", "LIVE" }
                    " {live.title}"
                }
                p { class: "watch-channel", "{page.channel_name}" }
            }
        },
        None => rsx! {
            div { class: "watch-offline",
                p { class: "offline-badge", "오프라인" }
            }
            div { class: "watch-info",
                p { class: "watch-channel", "{page.channel_name}" }
                p { class: "muted", "지금은 라이브 중이 아닙니다. 송출이 시작되면 새로고침해 주세요." }
            }
        },
    }
}
