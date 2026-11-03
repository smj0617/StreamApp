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
            UserMenu {}
        }
        main { class: "page", Outlet::<Route> {} }
    }
}

/// 상단 오른쪽 "현재 사용자" 드롭다운과 새 사용자 만들기.
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
    let mut new_nickname = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    let selected_id = current.read().as_ref().map(|user| user.id);

    let select = move |event: FormEvent| async move {
        let Ok(user_id) = event.value().parse::<i64>() else {
            return;
        };
        match api::select_user(user_id).await {
            Ok(user) => {
                current.set(Some(user));
                error.set(None);
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
            }
            Err(e) => error.set(Some(rejection_reason(e))),
        }
    };

    rsx! {
        div { class: "user-menu",
            select {
                class: "user-select",
                aria_label: "현재 사용자",
                onchange: select,
                if selected_id.is_none() {
                    option { value: "", selected: true, "시청만 가능" }
                }
                match &*users.read() {
                    Some(list) => rsx! {
                        for user in list.iter() {
                            option {
                                key: "{user.id}",
                                value: "{user.id}",
                                selected: Some(user.id) == selected_id,
                                "{user.nickname}"
                            }
                        }
                    },
                    None => rsx! {
                        option { disabled: true, "사용자 목록을 불러오지 못했습니다" }
                    },
                }
            }
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
            if let Some(reason) = error() {
                p { class: "user-menu-error", role: "alert", "{reason}" }
            }
        }
    }
}

/// 서버 함수 오류를 화면에 보여 줄 문장으로 바꾼다. 규칙 위반(400)은 서버가 보낸 사유 그대로.
fn rejection_reason(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
    }
}
