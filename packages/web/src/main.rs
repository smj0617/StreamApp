use dioxus::prelude::*;

mod views;

use views::{Home, NotFound, Settings, Shell, Watch};

/// 모든 페이지는 `Shell` 레이아웃(상단 바) 안에 그린다.
/// Dioxus Router의 경로 변수 문법은 `/:param`이다(axum·서버 함수의 `/{param}`과 다르다).
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Shell)]
        #[route("/")]
        Home {},
        #[route("/settings")]
        Settings {},
        #[route("/live/:channel_id")]
        Watch { channel_id: String },
        #[route("/:..segments")]
        NotFound { segments: Vec<String> },
}

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // 서버: DB 연결·마이그레이션·기본 데이터를 준비하고, 서버 함수가 꺼내 쓰도록
    // 앱 서비스를 Extension으로 붙인다. 커스텀 라우터를 합칠 때는 `.merge(..)` 뒤에 `.layer(..)`.
    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        let service = api::server::init_from_env().await?;
        Ok(dioxus::server::router(App)
            .merge(api::mediamtx::router(service.clone()))
            .layer(dioxus::server::axum::Extension(service)))
    });
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "StreamApp" }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Router::<Route> {}
    }
}
