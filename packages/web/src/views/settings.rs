use api::dto::MyChannelSettings;
// 제목 규칙은 서버와 같은 값을 쓴다. 화면에서는 글자 수만 보여 주고 거절은 서버가 한다.
use api::rules::{fallback_title, MAX_TITLE_CHARS};
use dioxus::prelude::*;

use super::shell::{rejection_reason, CurrentUser};

/// 내 채널 설정: OBS에 넣을 값을 복사하고 다음 라이브 제목을 적어 둔다.
/// 라이브 중이면 진행 중인 라이브의 제목도 바꿀 수 있다.
#[component]
pub fn Settings() -> Element {
    let CurrentUser(current) = use_context::<CurrentUser>();
    let user = current.read().clone();

    rsx! {
        section { class: "settings",
            h1 { "내 채널 설정" }
            match user {
                None => rsx! {
                    ChooseUserFirst {}
                },
                // 사용자를 바꾸면 key가 바뀌어 폼을 다시 마운트하고 새 사용자의 설정을 불러온다.
                Some(user) => rsx! {
                    SuspenseBoundary {
                        fallback: |_| rsx! {
                            p { class: "muted", "불러오는 중…" }
                        },
                        ChannelSettingsForm { key: "{user.id}", nickname: user.nickname }
                    }
                },
            }
        }
    }
}

#[component]
fn ChooseUserFirst() -> Element {
    rsx! {
        p { class: "notice",
            "사용자를 먼저 골라 주세요. 상단 오른쪽 사용자 메뉴(\"시청만 가능\")를 눌러 고르거나 새 닉네임으로 만들면 "
            "그 사용자의 채널 설정이 보입니다."
        }
    }
}

/// 현재 사용자의 채널 설정을 불러와 그린다. 서버 렌더링 때 쿠키로 불러온 값을 이어받는다.
#[component]
fn ChannelSettingsForm(nickname: String) -> Element {
    let loaded = use_server_future(api::my_channel_settings)?;
    let state = (*loaded.read()).clone();

    match state {
        Some(Ok(Some(settings))) => rsx! {
            // 진행 중인 라이브 제목 편집은 라이브 중일 때만 나타난다(페이지를 불러온 시점 기준).
            if let Some(live_title) = settings.live_title.clone() {
                TitleEditor {
                    kind: TitleKind::Live,
                    saved: live_title,
                    nickname: nickname.clone(),
                }
            }
            ObsValues { settings: settings.clone() }
            TitleEditor {
                kind: TitleKind::NextLive,
                saved: settings.next_live_title.unwrap_or_default(),
                nickname,
            }
        },
        // 쿠키의 사용자가 DB에 없다(예: DB 파일을 지운 뒤).
        Some(Ok(None)) => rsx! {
            ChooseUserFirst {}
        },
        Some(Err(error)) => {
            let reason = rejection_reason(error);
            rsx! {
                p { class: "form-error", role: "alert", "채널 설정을 불러오지 못했습니다: {reason}" }
            }
        }
        None => rsx! {},
    }
}

/// OBS "서버"·"스트림 키" 칸에 그대로 넣을 값.
#[component]
fn ObsValues(settings: MyChannelSettings) -> Element {
    rsx! {
        section { class: "settings-card",
            h2 { "OBS 송출 설정" }
            p { class: "muted",
                "OBS의 설정 → 방송에서 서비스를 \"사용자 지정\"으로 고르고 아래 두 값을 그대로 붙여 넣으세요."
            }
            CopyField { label: "서버", value: settings.obs_server }
            CopyField { label: "스트림 키", value: settings.obs_stream_key }
            p { class: "muted small",
                "채널 ID "
                code { "{settings.channel_id}" }
                " · 송출 키 "
                code { "{settings.stream_key}" }
            }
        }
    }
}

/// 이름표, 읽기 전용 값, 복사 버튼 한 줄.
#[component]
fn CopyField(label: String, value: String) -> Element {
    let mut status = use_signal(|| None::<&'static str>);
    let copy = {
        let value = value.clone();
        move |_| {
            let value = value.clone();
            async move {
                status.set(Some(if copy_to_clipboard(&value).await {
                    "복사했습니다"
                } else {
                    "복사하지 못했습니다. 값을 직접 선택해 복사해 주세요"
                }));
            }
        }
    };

    rsx! {
        div { class: "field",
            span { class: "field-label", "{label}" }
            div { class: "field-row",
                input {
                    class: "copy-value",
                    readonly: true,
                    aria_label: "OBS {label}",
                    value: "{value}",
                }
                button { onclick: copy, aria_label: "OBS {label} 복사", "복사" }
            }
            if let Some(message) = status() {
                p { class: "field-status", role: "status", "{message}" }
            }
        }
    }
}

/// 브라우저 클립보드에 쓴다. 값은 문자열 이스케이프 없이 메시지로 넘긴다.
async fn copy_to_clipboard(text: &str) -> bool {
    let eval = document::eval(
        r#"
        const text = await dioxus.recv();
        try {
            await navigator.clipboard.writeText(text);
            return true;
        } catch (e) {
            return false;
        }
        "#,
    );
    if eval.send(text).is_err() {
        return false;
    }
    eval.join::<bool>().await.unwrap_or(false)
}

/// 내 채널 설정에서 편집하는 제목. 두 제목은 규칙(최대 100자)과 편집 화면이 같다.
#[derive(Clone, Copy, PartialEq)]
enum TitleKind {
    /// 다음 라이브 제목: 송출을 시작하면 라이브 제목이 된다.
    NextLive,
    /// 진행 중인 라이브의 제목: 라이브 중일 때만 편집할 수 있다.
    Live,
}

impl TitleKind {
    fn heading(self) -> &'static str {
        match self {
            TitleKind::NextLive => "다음 라이브 제목",
            TitleKind::Live => "진행 중인 라이브 제목",
        }
    }

    fn description(self, nickname: &str) -> String {
        let fallback = fallback_title(nickname);
        match self {
            TitleKind::NextLive => format!(
                "송출을 시작하면 이 제목이 라이브 제목이 됩니다. 비워 두면 자동 제목 \"{fallback}\"가 붙습니다."
            ),
            TitleKind::Live => format!(
                "지금 라이브 중입니다. 바꾼 제목은 열려 있는 시청 페이지에 바로 반영되고, 홈에는 새로고침하면 보입니다. 비우면 자동 제목 \"{fallback}\"가 됩니다."
            ),
        }
    }

    /// 서버에 저장하고, 서버가 다듬은 제목을 돌려준다(다음 라이브 제목을 지웠으면 빈 문자열).
    async fn save(self, title: String) -> Result<String, ServerFnError> {
        match self {
            TitleKind::NextLive => api::change_next_live_title(title)
                .await
                .map(Option::unwrap_or_default),
            TitleKind::Live => api::change_live_title(title).await,
        }
    }
}

/// 제목 편집. 저장하면 서버가 다듬은 값(앞뒤 공백 제거, 라이브 제목을 비웠으면 자동 제목)으로
/// 바꿔 보여 준다.
#[component]
fn TitleEditor(kind: TitleKind, saved: String, nickname: String) -> Element {
    let mut title = use_signal(|| saved.clone());
    // Ok = 저장 완료 안내, Err = 거절 사유
    let mut outcome = use_signal(|| None::<Result<&'static str, String>>);

    let save = move || async move {
        match kind.save(title()).await {
            Ok(saved) => {
                title.set(saved);
                outcome.set(Some(Ok("저장했습니다")));
            }
            Err(error) => outcome.set(Some(Err(rejection_reason(error)))),
        }
    };

    let length = title.read().trim().chars().count();
    let count_class = if length > MAX_TITLE_CHARS {
        "title-count over"
    } else {
        "title-count"
    };

    let heading = kind.heading();
    let description = kind.description(&nickname);
    let placeholder = fallback_title(&nickname);
    let card_class = match kind {
        TitleKind::NextLive => "settings-card",
        TitleKind::Live => "settings-card live-title-card",
    };

    rsx! {
        section { class: "{card_class}",
            h2 {
                if kind == TitleKind::Live {
                    span { class: "live-badge", "LIVE" }
                    " "
                }
                "{heading}"
            }
            p { class: "muted", "{description}" }
            div { class: "field-row",
                input {
                    class: "title-input",
                    aria_label: "{heading}",
                    placeholder,
                    value: "{title}",
                    oninput: move |event| {
                        title.set(event.value());
                        outcome.set(None);
                    },
                    onkeydown: move |event| async move {
                        if event.key() == Key::Enter {
                            save().await;
                        }
                    },
                }
                button {
                    onclick: move |_| async move { save().await },
                    aria_label: "{heading} 저장",
                    "저장"
                }
            }
            p { class: "{count_class}", "{length}/{MAX_TITLE_CHARS}" }
            match outcome() {
                Some(Ok(message)) => rsx! {
                    p { class: "form-ok", role: "status", "{message}" }
                },
                Some(Err(reason)) => rsx! {
                    p { class: "form-error", role: "alert", "{reason}" }
                },
                None => rsx! {},
            }
        }
    }
}
