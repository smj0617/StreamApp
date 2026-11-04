use api::dto::User;
use dioxus::prelude::*;

use crate::Route;

/// 앱 전체가 함께 쓰는 현재 사용자. `None`이면 사용자를 고르지 않은 상태("시청만 가능")다.
///
/// `Shell`이 컨텍스트로 제공한다. 페이지에서는 `use_context::<CurrentUser>()`로 읽는다.
#[derive(Clone, Copy)]
pub struct CurrentUser(pub Signal<Option<User>>);

/// 모든 페이지에 공통인 레이아웃: 상단 바와 본문.
#[component]
pub fn Shell() -> Element {
    // 서버 렌더링 때 요청 쿠키로 현재 사용자를 읽어 HTML에 넣고,
    // 클라이언트는 그 결과를 이어받아 다시 묻지 않는다.
    let remembered = use_server_future(api::current_user)?;
    let current = use_signal(|| {
        remembered
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .cloned()
            .flatten()
    });
    use_context_provider(|| CurrentUser(current));

    rsx! {
        header { class: "topbar",
            Link { class: "brand", to: Route::Home {}, "StreamApp" }
            nav { class: "topnav",
                Link { to: Route::NewRecording {}, "녹화방송 등록" }
                Link { to: Route::Settings {}, "내 채널 설정" }
            }
            UserMenu {}
        }
        main { class: "page", Outlet::<Route> {} }
    }
}

/// 상단 오른쪽 사용자 메뉴. 현재 사용자(고르지 않았으면 "시청만 가능")를 보여 주는 버튼을 누르면
/// 펼쳐지는 메뉴 하나에, 기존 사용자 목록(고르기)과 새 닉네임 입력(만들기)이 함께 있다.
/// 고르거나 만들면 그 사용자가 현재 사용자가 되고(서버가 쿠키로 기억한다) 메뉴가 닫힌다.
#[component]
fn UserMenu() -> Element {
    let CurrentUser(mut current) = use_context::<CurrentUser>();
    let loaded = use_server_future(api::list_users)?;
    // 처음 불러온 목록을 복사해 두고 새로 만든 사용자를 여기에 더한다.
    // (목록을 다시 불러오면 불러오는 동안 메뉴가 잠깐 사라진다.) `None` = 불러오지 못함.
    let mut users = use_signal(|| {
        loaded
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .cloned()
    });
    let mut open = use_signal(|| false);
    let mut new_nickname = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    let current_user = current.read().clone();
    let selected_id = current_user.as_ref().map(|user| user.id);
    let current_label = current_user
        .as_ref()
        .map_or("시청만 가능", |user| user.nickname.as_str())
        .to_string();
    // 목록의 (ID, 닉네임). `None` = 불러오지 못함.
    let options: Option<Vec<(i64, String)>> = users.read().as_ref().map(|list| {
        list.iter()
            .map(|user| (user.id, user.nickname.clone()))
            .collect()
    });

    let select = move |user_id: i64| async move {
        match api::select_user(user_id).await {
            Ok(user) => {
                current.set(Some(user));
                error.set(None);
                open.set(false);
            }
            Err(e) => error.set(Some(rejection_reason(e))),
        }
    };

    let create = move || async move {
        match api::create_user(new_nickname()).await {
            Ok(created) => {
                if let Some(list) = users.write().as_mut() {
                    list.push(created.user.clone());
                }
                current.set(Some(created.user));
                new_nickname.set(String::new());
                error.set(None);
                open.set(false);
            }
            Err(e) => error.set(Some(rejection_reason(e))),
        }
    };

    rsx! {
        div { class: "user-menu",
            button {
                class: "user-menu-button",
                aria_label: "현재 사용자: {current_label}",
                aria_haspopup: "true",
                aria_expanded: "{open}",
                onclick: move |_| {
                    // 펼칠 때마다 지난 거절 사유를 지운다.
                    error.set(None);
                    open.toggle();
                },
                span {
                    class: if selected_id.is_some() { "user-menu-current" } else { "user-menu-current viewer-only" },
                    "{current_label}"
                }
                span { class: "user-menu-caret", aria_hidden: "true", "▾" }
            }
            if open() {
                // 메뉴 바깥을 누르면 닫는다.
                div { class: "user-menu-backdrop", onclick: move |_| open.set(false) }
                div {
                    class: "user-menu-panel",
                    role: "dialog",
                    aria_label: "사용자 고르기",
                    onkeydown: move |event| {
                        if event.key() == Key::Escape {
                            open.set(false);
                        }
                    },
                    p { class: "user-menu-heading", "사용자 고르기" }
                    match options {
                        Some(list) => rsx! {
                            ul { class: "user-list",
                                for (id, nickname) in list {
                                    li { key: "{id}",
                                        button {
                                            class: if Some(id) == selected_id { "user-option selected" } else { "user-option" },
                                            aria_pressed: "{Some(id) == selected_id}",
                                            onclick: move |_| async move { select(id).await },
                                            "{nickname}"
                                        }
                                    }
                                }
                            }
                        },
                        None => rsx! {
                            p { class: "user-menu-note muted", "사용자 목록을 불러오지 못했습니다" }
                        },
                    }
                    p { class: "user-menu-heading", "새 사용자 만들기" }
                    div { class: "user-menu-create",
                        input {
                            class: "new-nickname",
                            aria_label: "새 닉네임",
                            placeholder: "새 닉네임",
                            value: "{new_nickname}",
                            oninput: move |event| new_nickname.set(event.value()),
                            onkeydown: move |event| async move {
                                if event.key() == Key::Enter {
                                    create().await;
                                }
                            },
                        }
                        button { onclick: move |_| async move { create().await }, "만들기" }
                    }
                    if let Some(reason) = error() {
                        p { class: "user-menu-error", role: "alert", "{reason}" }
                    }
                }
            }
        }
    }
}

/// 서버 함수 오류를 화면에 보여 줄 문장으로 바꾼다. 규칙 위반(400)은 서버가 보낸 사유 그대로.
pub(crate) fn rejection_reason(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
    }
}
