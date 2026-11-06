use api::dto::{BroadcastTarget, WatchPage};
use dioxus::prelude::*;

use super::chat::Chat;
use super::comments::Comments;
use super::donations::Donations;
use super::hls_player::HlsPlayer;
use super::shell::rejection_reason;
use crate::Route;

/// 시청 페이지 `/live/<채널ID>`: 라이브 중이면 영상을 자동 재생하고 채팅과 이 라이브의 댓글을 연다.
/// 로그인하지 않아도 볼 수 있다(채팅·댓글은 읽기만).
///
/// 들어올 때의 상태(라이브 중, 오프라인, 없는 채널)를 보여 준다. 보는 도중 라이브 제목이 바뀌거나
/// 라이브가 끝나면 채팅으로 알림을 받아 바로 표시한다. 오프라인에서 라이브로 바뀌는 것은 새로고침해야
/// 보인다.
#[component]
pub fn Watch(channel_id: ReadSignal<String>) -> Element {
    // 서버 렌더링 때 불러온 정보를 HTML에 넣고, 클라이언트는 그 결과를 이어받는다.
    // 다른 채널 주소로 옮겨 가면(같은 컴포넌트가 재사용되어도) 다시 불러온다.
    let loaded = use_server_future(move || api::watch_page(channel_id()))?;
    let state = (*loaded.read()).clone();

    rsx! {
        section { class: "watch",
            match state {
                // 다른 채널로 옮겨 가면 종료 상태와 받은 메시지를 새로 시작한다.
                Some(Ok(Some(page))) => rsx! {
                    WatchChannel { key: "{page.channel_id}", page }
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

/// 있는 채널: 라이브 중이면 플레이어, 라이브 제목, 채팅. 아니면 "오프라인"(채팅은 닫혀 있다).
#[component]
fn WatchChannel(page: WatchPage) -> Element {
    // 채팅이 라이브 종료 알림을 받으면 켠다: 영상을 멈추고 입력을 막는다. 받은 메시지는 남는다.
    let ended = use_signal(|| false);
    // 보여 주는 라이브 제목. 채팅이 라이브 제목 바뀜 알림을 받으면 바꾼다.
    let title = use_signal(|| {
        page.live
            .as_ref()
            .map(|live| live.title.clone())
            .unwrap_or_default()
    });

    match page.live {
        Some(live) => rsx! {
            div { class: "watch-stage",
                HlsPlayer { key: "{page.playback_url}", src: page.playback_url.clone(), ended }
                if ended() {
                    div { class: "watch-ended", role: "status",
                        p { "라이브가 종료되었습니다" }
                    }
                }
            }
            div { class: "watch-info",
                h1 { class: "watch-title",
                    if ended() {
                        span { class: "ended-badge", "종료" }
                    } else {
                        span { class: "live-badge", "LIVE" }
                    }
                    " {title}"
                }
                p { class: "watch-channel", "{page.channel_name}" }
            }
            Chat { channel_id: page.channel_id.clone(), ended, live_title: title }
            // 후원은 이 라이브 회차에 붙고, 서버가 결제 승인을 확인한 것만 내역과 합계에 들어간다.
            SuspenseBoundary {
                fallback: |_| rsx! {
                    p { class: "muted", "후원 내역을 불러오는 중…" }
                },
                Donations { target: BroadcastTarget::Live(live.id) }
            }
            // 채팅은 흘러가고 저장하지 않지만, 댓글은 이 라이브 회차에 남는다.
            SuspenseBoundary {
                fallback: |_| rsx! {
                    p { class: "muted", "댓글을 불러오는 중…" }
                },
                Comments { target: BroadcastTarget::Live(live.id) }
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
            section { class: "chat", aria_label: "채팅",
                h2 { class: "chat-heading", "채팅" }
                p { class: "chat-closed muted", "채팅은 라이브 중에만 열립니다." }
            }
        },
    }
}
