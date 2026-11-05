use api::rules::{MAX_PASSWORD_CHARS, MIN_PASSWORD_CHARS};
use dioxus::prelude::*;

use super::shell::{rejection_reason, CurrentUser};
use crate::Route;

/// 로그인 `/login`: 닉네임과 비밀번호로 로그인한다. 성공하면 서버가 세션 쿠키를 싣고 홈으로 옮겨 간다.
#[component]
pub fn Login() -> Element {
    rsx! {
        AuthForm { mode: Mode::Login }
    }
}

/// 회원가입 `/signup`: 닉네임과 비밀번호로 사용자와 채널을 만들고 곧바로 로그인한다.
#[component]
pub fn SignUp() -> Element {
    rsx! {
        AuthForm { mode: Mode::SignUp }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Login,
    SignUp,
}

#[component]
fn AuthForm(mode: Mode) -> Element {
    let CurrentUser(mut current) = use_context::<CurrentUser>();
    let mut nickname = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let signed_in = current.read().clone();

    let submit = move || async move {
        if busy() {
            return;
        }
        busy.set(true);
        let result = match mode {
            Mode::Login => api::log_in(nickname(), password()).await,
            Mode::SignUp => api::sign_up(nickname(), password()).await,
        };
        busy.set(false);
        match result {
            Ok(user) => {
                current.set(Some(user));
                password.set(String::new());
                error.set(None);
                navigator().push(Route::Home {});
            }
            Err(e) => error.set(Some(rejection_reason(e))),
        }
    };

    let (title, button, autocomplete) = match mode {
        Mode::Login => ("로그인", "로그인", "current-password"),
        Mode::SignUp => ("회원가입", "가입하고 로그인", "new-password"),
    };

    rsx! {
        section { class: "settings auth",
            h1 { "{title}" }
            if let Some(user) = signed_in {
                p { class: "notice",
                    "{user.nickname}(으)로 로그인해 있습니다. 다른 사용자로 하려면 먼저 상단 오른쪽 \"로그아웃\"을 누르세요."
                }
            }
            form {
                class: "auth-form",
                onsubmit: move |event| async move {
                    event.prevent_default();
                    submit().await;
                },
                label { class: "field",
                    span { class: "field-label", "닉네임" }
                    input {
                        name: "nickname",
                        autocomplete: "username",
                        value: "{nickname}",
                        oninput: move |event| nickname.set(event.value()),
                    }
                }
                label { class: "field",
                    span { class: "field-label", "비밀번호" }
                    input {
                        r#type: "password",
                        name: "password",
                        autocomplete,
                        value: "{password}",
                        oninput: move |event| password.set(event.value()),
                    }
                }
                if mode == Mode::SignUp {
                    p { class: "muted small",
                        "닉네임은 2~10자 한글·영문·숫자, 비밀번호는 {MIN_PASSWORD_CHARS}~{MAX_PASSWORD_CHARS}자입니다. "
                        "가입하면 닉네임과 같은 이름의 채널이 생깁니다."
                    }
                }
                button { r#type: "submit", disabled: busy(), "{button}" }
                if let Some(reason) = error() {
                    p { class: "form-error", role: "alert", "{reason}" }
                }
            }
            match mode {
                Mode::Login => rsx! {
                    p { class: "muted",
                        "처음이면 "
                        Link { to: Route::SignUp {}, "회원가입" }
                        "하세요."
                    }
                },
                Mode::SignUp => rsx! {
                    p { class: "muted",
                        "이미 가입했으면 "
                        Link { to: Route::Login {}, "로그인" }
                        "하세요."
                    }
                },
            }
        }
    }
}
