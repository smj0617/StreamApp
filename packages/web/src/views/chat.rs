use api::dto::ChatMessage;
// 채팅 메시지 최대 글자 수는 서버와 같은 값을 쓴다. 서버도 같은 규칙으로 거절한다.
use api::rules::MAX_CHAT_CHARS;
use api::{ChatClientEvent, ChatServerEvent};
use dioxus::fullstack::{use_websocket, WebSocketOptions, WebsocketError};
use dioxus::prelude::*;

use super::script_with_element_id;
use super::shell::CurrentUser;

/// 메시지 목록 요소의 id. 새 메시지가 오면 맨 아래로 내린다.
const MESSAGES_ID: &str = "chat-messages";

/// 메시지 목록을 맨 아래로 내린다. `elementId`는 [`MESSAGES_ID`]다([`script_with_element_id`]).
const SCROLL_TO_BOTTOM_JS: &str =
    "const list = document.getElementById(elementId); if (list) list.scrollTop = list.scrollHeight;";

/// 시청 페이지의 채팅. 라이브 중인 채널에서만 그린다.
///
/// 들어온 뒤의 메시지만 보인다. 받은 메시지는 이 컴포넌트에 두므로, 사용자를 바꿔 연결을 다시
/// 맺어도(아래 `ChatConnection`을 다시 마운트해도) 그대로 남는다. 라이브가 끝나면 `ended`가 켜지고
/// 입력이 막힌다. 스트리머가 라이브 제목을 바꾸면 `live_title`을 바꾼다.
#[component]
pub fn Chat(channel_id: String, ended: Signal<bool>, live_title: Signal<String>) -> Element {
    let CurrentUser(current) = use_context::<CurrentUser>();
    let messages = use_signal(Vec::<ChatMessage>::new);
    // 연결은 브라우저에서 화면을 그린 뒤에만 맺는다(서버 렌더링에서는 `use_effect`가 돌지 않는다).
    let mut mounted = use_signal(|| false);
    use_effect(move || mounted.set(true));

    let user_id = current.read().as_ref().map(|user| user.id);
    // 보낸 사람은 연결할 때 쿠키에 있던 사용자다. 사용자가 바뀌면 key가 바뀌어 연결을 새로 맺는다.
    let connection_key = match user_id {
        Some(id) => format!("user-{id}"),
        None => "viewer".to_string(),
    };

    rsx! {
        section { class: "chat", aria_label: "채팅",
            h2 { class: "chat-heading", "채팅" }
            ChatMessages { messages }
            if ended() {
                ClosedChatInput { placeholder: "라이브가 종료되어 채팅을 보낼 수 없습니다" }
            } else if mounted() {
                ChatConnection {
                    key: "{connection_key}",
                    channel_id: channel_id.clone(),
                    can_send: user_id.is_some(),
                    messages,
                    ended,
                    live_title,
                }
            } else {
                ClosedChatInput { placeholder: "채팅에 연결하는 중입니다" }
            }
        }
    }
}

/// 받은 메시지 목록. 새 메시지가 오면 맨 아래로 내린다.
#[component]
fn ChatMessages(messages: Signal<Vec<ChatMessage>>) -> Element {
    use_effect(move || {
        let _ = messages.read().len();
        document::eval(&script_with_element_id(MESSAGES_ID, SCROLL_TO_BOTTOM_JS));
    });

    rsx! {
        ol { id: MESSAGES_ID, class: "chat-messages", aria_live: "polite",
            if messages.read().is_empty() {
                li { class: "chat-empty muted", "들어온 뒤에 보낸 메시지만 여기에 보입니다." }
            }
            // 메시지는 뒤에 붙기만 하므로 순번을 key로 쓴다.
            for (index, message) in messages.read().iter().enumerate() {
                li { key: "{index}", class: "chat-message",
                    span { class: "chat-nickname", "{message.nickname}" }
                    span { class: "chat-content", "{message.content}" }
                }
            }
        }
    }
}

/// 연결 하나: 받은 메시지를 목록에 더하고(라이브 제목 바뀜·종료 알림은 신호에 반영), 입력한 메시지를 보낸다.
///
/// 연결이 끊겨도 다시 연결하지 않는다(닫힌 연결에서 다시 연결을 기다리면 곧바로 끝나 무한히 돈다).
/// 다시 연결은 부모가 key를 바꿔 이 컴포넌트를 새로 마운트하는 것으로만 한다.
#[component]
fn ChatConnection(
    channel_id: String,
    can_send: bool,
    messages: Signal<Vec<ChatMessage>>,
    ended: Signal<bool>,
    live_title: Signal<String>,
) -> Element {
    let mut messages = messages;
    let mut ended = ended;
    let mut live_title = live_title;
    let mut joined = use_signal(|| false);
    let mut lost = use_signal(|| false);
    let mut rejection = use_signal(|| None::<String>);
    let mut draft = use_signal(String::new);

    let mut socket =
        use_websocket(move || api::connect_chat(channel_id.clone(), WebSocketOptions::new()));

    use_future(move || async move {
        loop {
            match socket.recv().await {
                Ok(ChatServerEvent::Joined) => joined.set(true),
                Ok(ChatServerEvent::Message(message)) => messages.push(message),
                Ok(ChatServerEvent::LiveTitleChanged { title }) => live_title.set(title),
                Ok(ChatServerEvent::Rejected { reason }) => rejection.set(Some(reason)),
                Ok(ChatServerEvent::LiveEnded) => {
                    ended.set(true);
                    return;
                }
                // 읽을 수 없는 메시지는 건너뛴다.
                Err(WebsocketError::Deserialization(_)) => {}
                Err(_) => {
                    lost.set(true);
                    return;
                }
            }
        }
    });

    let send = move || async move {
        let content = draft();
        let length = content.trim().chars().count();
        if length == 0 || length > MAX_CHAT_CHARS {
            return;
        }
        rejection.set(None);
        match socket.send(ChatClientEvent::Send { content }).await {
            Ok(()) => draft.set(String::new()),
            Err(_) => lost.set(true),
        }
    };

    let usable = can_send && joined() && !lost();
    let placeholder = if lost() {
        "채팅 연결이 끊겼습니다. 새로고침해 주세요"
    } else if !can_send {
        "사용자를 고르면 채팅할 수 있습니다"
    } else if !joined() {
        "채팅에 연결하는 중입니다"
    } else {
        "메시지 보내기"
    };
    let length = draft.read().trim().chars().count();
    let count_class = if length > MAX_CHAT_CHARS {
        "chat-count over"
    } else {
        "chat-count"
    };

    rsx! {
        div { class: "chat-input",
            div { class: "field-row",
                input {
                    aria_label: "채팅 메시지",
                    placeholder,
                    disabled: !usable,
                    value: "{draft}",
                    oninput: move |event| draft.set(event.value()),
                    onkeydown: move |event| async move {
                        if event.key() == Key::Enter {
                            send().await;
                        }
                    },
                }
                button {
                    disabled: !usable || length == 0 || length > MAX_CHAT_CHARS,
                    onclick: move |_| async move { send().await },
                    "보내기"
                }
            }
            if !can_send {
                p { class: "chat-guide muted small",
                    "상단 오른쪽 사용자 메뉴에서 사용자를 고르거나 새로 만들면 채팅을 보낼 수 있습니다. 지금은 시청만 가능합니다."
                }
            } else if usable {
                p { class: "{count_class}", "{length}/{MAX_CHAT_CHARS}" }
            }
            if let Some(reason) = rejection() {
                p { class: "form-error", role: "alert", "{reason}" }
            }
        }
    }
}

/// 연결이 없을 때(연결 전, 라이브 종료 뒤) 보여 주는 막힌 입력창.
#[component]
fn ClosedChatInput(placeholder: String) -> Element {
    rsx! {
        div { class: "chat-input",
            div { class: "field-row",
                input { aria_label: "채팅 메시지", placeholder, disabled: true }
                button { disabled: true, "보내기" }
            }
        }
    }
}
