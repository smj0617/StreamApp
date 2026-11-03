use dioxus::prelude::*;

use crate::Route;

/// 모든 페이지에 공통인 레이아웃: 상단 바와 본문.
#[component]
pub fn Shell() -> Element {
    rsx! {
        header { class: "topbar",
            Link { class: "brand", to: Route::Home {}, "StreamApp" }
            UserMenu {}
        }
        main { class: "page", Outlet::<Route> {} }
    }
}

/// 상단 오른쪽 "현재 사용자" 드롭다운. 지금은 사용자 목록만 보여 준다.
///
/// 서버 렌더링 때 목록을 불러와 HTML에 넣고, 클라이언트는 그 결과를 그대로 이어받는다.
#[component]
fn UserMenu() -> Element {
    let users = use_server_future(api::list_users)?;
    let users = users.read();

    rsx! {
        select { class: "user-menu", aria_label: "현재 사용자",
            option { value: "", "사용자 목록" }
            match &*users {
                Some(Ok(list)) => rsx! {
                    for user in list.iter() {
                        option { key: "{user.id}", value: "{user.id}", "{user.nickname}" }
                    }
                },
                Some(Err(_)) => rsx! {
                    option { disabled: true, "사용자 목록을 불러오지 못했습니다" }
                },
                None => rsx! {},
            }
        }
    }
}
