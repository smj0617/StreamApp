//! 로그인 상태와 로그인 화면. 세션 토큰은 [`crate::session`]이 저장하고, 요청마다 [`crate::client`]가 쿠키로 싣는다.
//! 회원가입은 웹에서 한다(로그인 화면의 "웹에서 가입" 링크).

use dioxus::prelude::*;

use crate::client::{self, User};
use crate::session::Session;
use crate::views::use_server;
use crate::Route;

/// 앱 전체가 함께 쓰는 로그인 상태. `session`은 지금 서버의 세션 토큰, `user`는 그 세션의 사용자다
/// (저장한 세션을 서버에 확인하기 전에는 `None`).
#[derive(Clone, Copy)]
pub struct Auth {
    pub session: Signal<Option<Session>>,
    pub user: Signal<Option<User>>,
}

impl Auth {
    /// 지금 세션 토큰(읽는 화면은 세션이 바뀌면 다시 그린다).
    pub fn token(&self) -> Option<String> {
        self.session.read().as_ref().map(|s| s.token.clone())
    }

    /// 로그인 성공: 세션을 저장하고 사용자를 기억한다. 저장에 실패하면 사유를 돌려준다(이번 실행 동안은 로그인 상태).
    pub fn sign_in(self, session: Session, user: User) -> Result<(), String> {
        let saved = session.save();
        let (mut current_session, mut current_user) = (self.session, self.user);
        current_session.set(Some(session));
        current_user.set(Some(user));
        saved
    }

    /// 이 앱의 로그인 상태와 저장한 세션 파일을 지운다(로그아웃, 서버 주소 바꾸기, 세션 끝남).
    pub fn clear(self) {
        let (mut session, mut user) = (self.session, self.user);
        session.set(None);
        user.set(None);
        Session::forget_saved();
    }
}

pub fn use_auth() -> Auth {
    use_context::<Auth>()
}

/// 앱 시작 때 로그인 상태를 만든다: 저장한 세션을 읽고, 서버에 그 세션의 사용자를 묻는다.
/// 서버 주소가 바뀌면 세션을 버린다.
pub fn use_auth_provider(server: Signal<crate::server_url::ServerUrl>) -> Auth {
    let session = use_signal(|| Session::load(&server.peek()));
    let user = use_signal(|| None::<User>);
    let auth = use_context_provider(|| Auth { session, user });

    // 서버 주소를 바꾸면 이전 서버의 세션은 쓰지 않는다(다른 서버에 토큰을 보내지 않는다).
    use_effect(move || {
        let current = server();
        let stale = auth
            .session
            .peek()
            .as_ref()
            .is_some_and(|s| s.server != current);
        if stale {
            auth.clear();
        }
    });

    // 세션의 사용자를 확인한다. 세션이 끝났으면(로그아웃·기한) 지운다. 연결 실패면 세션은 그대로 둔다.
    let _checked = use_resource(move || async move {
        let mut user = auth.user;
        let Some(saved) = auth.session.cloned() else {
            user.set(None);
            return;
        };
        match client::current_user(&saved.server, &saved.token).await {
            Ok(Some(found)) => user.set(Some(found)),
            Ok(None) => auth.clear(),
            Err(_) => {}
        }
    });

    auth
}

/// 상단 바 오른쪽: 로그인했으면 닉네임과 "로그아웃", 아니면 "로그인" 링크.
#[component]
pub fn AccountMenu() -> Element {
    let auth = use_auth();
    let mut busy = use_signal(|| false);

    let log_out = move |_| async move {
        if busy() {
            return;
        }
        busy.set(true);
        // 서버의 세션을 지운다. 연결에 실패해도 이 앱의 로그인 상태와 파일은 지운다.
        if let Some(saved) = auth.session.cloned() {
            let _ = client::log_out(&saved.server, &saved.token).await;
        }
        auth.clear();
        busy.set(false);
    };

    let user = auth.user.read().clone();
    rsx! {
        span { class: "account",
            match user {
                Some(user) => rsx! {
                    span { class: "account-name", title: "로그인한 사용자", "{user.nickname}" }
                    button { class: "secondary small", disabled: busy(), onclick: log_out, "로그아웃" }
                },
                None => rsx! {
                    Link { to: Route::Login {}, "로그인" }
                },
            }
        }
    }
}

/// 로그인: 닉네임과 비밀번호. 성공하면 이전 화면으로 돌아간다(없으면 홈).
#[component]
pub fn Login() -> Element {
    let server = use_server();
    let auth = use_auth();
    let mut nickname = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    let submit = move || async move {
        if busy() {
            return;
        }
        busy.set(true);
        let server_now = server();
        let result = client::log_in(&server_now, nickname().trim(), &password()).await;
        busy.set(false);
        match result {
            Ok((user, token)) => {
                password.set(String::new());
                let session = Session {
                    server: server_now,
                    token,
                };
                if let Err(reason) = auth.sign_in(session, user) {
                    // 로그인은 됐지만 다음 실행까지 유지하지 못한다. 알리고 이 화면에 남는다.
                    error.set(Some(format!("로그인했습니다. 다만 {reason}")));
                    return;
                }
                error.set(None);
                let nav = navigator();
                if nav.can_go_back() {
                    nav.go_back();
                } else {
                    nav.push(Route::Home {});
                }
            }
            Err(e) => error.set(Some(e.message)),
        }
    };

    let signed_in = auth.user.read().clone();
    let sign_up_url = server.read().join("/signup");
    rsx! {
        section { class: "settings auth",
            h1 { "로그인" }
            p { class: "muted", "{server.read().as_str()}의 계정으로 로그인합니다." }
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
                        spellcheck: false,
                        value: "{nickname}",
                        oninput: move |event| nickname.set(event.value()),
                    }
                }
                label { class: "field",
                    span { class: "field-label", "비밀번호" }
                    input {
                        r#type: "password",
                        name: "password",
                        autocomplete: "current-password",
                        value: "{password}",
                        oninput: move |event| password.set(event.value()),
                    }
                }
                button { r#type: "submit", disabled: busy(), "로그인" }
                if let Some(reason) = error() {
                    p { class: "error", role: "alert", "{reason}" }
                }
            }
            p { class: "muted",
                "계정이 없으면 "
                a { href: "{sign_up_url}", "웹에서 가입" }
                "한 뒤 로그인하세요. 로그인 상태는 이 컴퓨터에 저장되어 다음 실행에도 유지되고, 로그아웃하면 지워집니다."
            }
        }
    }
}
