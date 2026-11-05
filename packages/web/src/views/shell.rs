use api::dto::User;
use dioxus::prelude::*;

use crate::Route;

/// 앱 전체가 함께 쓰는 로그인한 사용자. `None`이면 로그인하지 않은 상태(시청·목록 조회만 가능)다.
///
/// `Shell`이 컨텍스트로 제공한다. 페이지에서는 `use_context::<CurrentUser>()`로 읽는다.
#[derive(Clone, Copy)]
pub struct CurrentUser(pub Signal<Option<User>>);

/// 모든 페이지에 공통인 레이아웃: 상단 바와 본문.
#[component]
pub fn Shell() -> Element {
    // 서버 렌더링 때 요청의 세션 쿠키로 로그인한 사용자를 읽어 HTML에 넣고,
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
                Link { to: Route::Broadcast {}, "휴대폰 방송" }
                Link { to: Route::NewRecording {}, "녹화방송 등록" }
                Link { to: Route::Settings {}, "내 채널 설정" }
            }
            AuthMenu {}
        }
        main { class: "page", Outlet::<Route> {} }
    }
}

/// 상단 오른쪽 로그인 메뉴. 로그인했으면 닉네임과 "로그아웃", 아니면 "로그인"·"회원가입" 링크.
#[component]
fn AuthMenu() -> Element {
    let CurrentUser(mut current) = use_context::<CurrentUser>();
    let mut error = use_signal(|| None::<String>);
    let user = current.read().clone();

    let log_out = move || async move {
        match api::log_out().await {
            Ok(()) => {
                current.set(None);
                error.set(None);
            }
            Err(e) => error.set(Some(rejection_reason(e))),
        }
    };

    rsx! {
        div { class: "auth-menu",
            match user {
                Some(user) => rsx! {
                    span { class: "auth-nickname", aria_label: "로그인한 사용자", "{user.nickname}" }
                    button { class: "auth-logout", onclick: move |_| async move { log_out().await }, "로그아웃" }
                },
                None => rsx! {
                    Link { class: "auth-link", to: Route::Login {}, "로그인" }
                    Link { class: "auth-link primary", to: Route::SignUp {}, "회원가입" }
                },
            }
            if let Some(reason) = error() {
                span { class: "auth-error", role: "alert", "{reason}" }
            }
        }
    }
}

/// 로그인해야 쓸 수 있는 화면에서, 로그인하지 않았을 때 보여 주는 안내. `what`은 로그인하면 할 수 있는 일이다.
/// 화면 안내일 뿐이고, 실제 거절은 서버가 한다.
#[component]
pub(crate) fn LoginRequired(what: String) -> Element {
    rsx! {
        p { class: "notice",
            "로그인이 필요합니다. "
            Link { to: Route::Login {}, "로그인" }
            "하거나 "
            Link { to: Route::SignUp {}, "회원가입" }
            "하면 {what}"
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
