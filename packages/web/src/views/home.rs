use api::dto::{LiveChannel, RecordingSummary};
use dioxus::prelude::*;

use super::recordings::ago_text;
use super::shell::rejection_reason;
use crate::Route;

/// 홈: 라이브 중인 채널의 카드 목록(최근에 시작한 순서)과 녹화방송 목록(최근에 등록한 순서).
/// 새로고침해야 갱신된다.
#[component]
pub fn Home() -> Element {
    // 서버 렌더링 때 불러온 목록을 HTML에 넣고, 클라이언트는 그 결과를 이어받는다.
    let loaded = use_server_future(api::list_live_channels)?;
    let state = (*loaded.read()).clone();

    rsx! {
        section { class: "home",
            h1 { "라이브" }
            match state {
                Some(Ok(list)) if list.channels.is_empty() => rsx! {
                    p { class: "notice",
                        "지금 라이브 중인 채널이 없습니다. 채널에 송출이 시작되면 여기에 나타납니다."
                    }
                },
                Some(Ok(list)) => {
                    let server_now_ms = list.server_now_ms;
                    rsx! {
                        ul { class: "live-list",
                            for live in list.channels {
                                li { key: "{live.channel_id}",
                                    LiveCard { live, server_now_ms }
                                }
                            }
                        }
                    }
                }
                Some(Err(error)) => {
                    let reason = rejection_reason(error);
                    rsx! {
                        p { class: "form-error", role: "alert",
                            "라이브 목록을 불러오지 못했습니다: {reason}"
                        }
                    }
                }
                None => rsx! {},
            }
            Recordings {}
        }
    }
}

/// 녹화방송 목록: 최근에 등록한 순서의 카드. 사용자를 고르지 않아도 볼 수 있다.
#[component]
fn Recordings() -> Element {
    let loaded = use_server_future(api::list_recordings)?;
    let state = (*loaded.read()).clone();

    rsx! {
        div { class: "home-heading",
            h1 { "녹화방송" }
            Link { class: "heading-action", to: Route::NewRecording {}, "녹화방송 등록" }
        }
        match state {
            Some(Ok(list)) if list.recordings.is_empty() => rsx! {
                p { class: "notice",
                    "아직 등록된 녹화방송이 없습니다. 상단의 \"녹화방송 등록\"에서 영상 파일을 올려 보세요."
                }
            },
            Some(Ok(list)) => {
                let server_now_ms = list.server_now_ms;
                rsx! {
                    ul { class: "live-list",
                        for recording in list.recordings {
                            li { key: "{recording.id}",
                                RecordingCard { recording, server_now_ms }
                            }
                        }
                    }
                }
            }
            Some(Err(error)) => {
                let reason = rejection_reason(error);
                rsx! {
                    p { class: "form-error", role: "alert",
                        "녹화방송 목록을 불러오지 못했습니다: {reason}"
                    }
                }
            }
            None => rsx! {},
        }
    }
}

/// 녹화방송 카드: 제목, 채널 이름, 등록 후 지난 시간. 누르면 녹화방송 상세 페이지로 간다.
#[component]
fn RecordingCard(recording: RecordingSummary, server_now_ms: i64) -> Element {
    let registered = ago_text(server_now_ms - recording.registered_at_ms);
    rsx! {
        Link {
            class: "live-card-link",
            to: Route::RecordingDetail {
                id: recording.id,
            },
            article { class: "live-card",
                p { class: "live-card-top",
                    span { class: "recording-badge", "녹화" }
                    span { class: "live-elapsed", "{registered}" }
                }
                h2 { class: "live-title", "{recording.title}" }
                p { class: "live-channel", "{recording.channel_name}" }
            }
        }
    }
}

/// 라이브 카드: 채널 이름, 라이브 제목, 시작 후 경과 시간. 썸네일은 없다. 누르면 시청 페이지로 간다.
///
/// 경과 시간은 서버가 목록과 함께 준 현재 시각으로 계산한다. 웹(wasm)에서는 `SystemTime::now()`가
/// panic하고, 서버 렌더링 결과와 하이드레이션 결과가 같아야 하기 때문이다.
#[component]
fn LiveCard(live: LiveChannel, server_now_ms: i64) -> Element {
    let elapsed = elapsed_text(server_now_ms - live.started_at_ms);
    rsx! {
        Link {
            class: "live-card-link",
            to: Route::Watch {
                channel_id: live.channel_id.clone(),
            },
            article { class: "live-card",
                p { class: "live-card-top",
                    span { class: "live-badge", "LIVE" }
                    span { class: "live-elapsed", "{elapsed}" }
                }
                h2 { class: "live-title", "{live.title}" }
                p { class: "live-channel", "{live.channel_name}" }
            }
        }
    }
}

/// 라이브 시작 후 경과 시간을 분 단위로 쓴다: "방금 시작", "시작한 지 12분", "시작한 지 1시간 5분".
fn elapsed_text(elapsed_ms: i64) -> String {
    let minutes = elapsed_ms.max(0) / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "방금 시작".to_string(),
        (0, m) => format!("시작한 지 {m}분"),
        (h, m) => format!("시작한 지 {h}시간 {m}분"),
    }
}
