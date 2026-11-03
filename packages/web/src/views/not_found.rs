use dioxus::prelude::*;

use crate::Route;

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let path = segments.join("/");
    rsx! {
        h1 { "페이지를 찾을 수 없습니다" }
        p { "/{path}" }
        Link { to: Route::Home {}, "홈으로" }
    }
}
