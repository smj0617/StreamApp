use dioxus::prelude::*;

mod views;

use views::{
    Broadcast, DonationFail, DonationSuccess, Download, Home, Login, NewRecording, NotFound,
    RecordingDetail, Settings, Shell, SignUp, Watch,
};

/// 모든 페이지는 `Shell` 레이아웃(상단 바) 안에 그린다.
/// Dioxus Router의 경로 변수 문법은 `/:param`이다(axum·서버 함수의 `/{param}`과 다르다).
#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Shell)]
        #[route("/")]
        Home {},
        #[route("/login")]
        Login {},
        #[route("/signup")]
        SignUp {},
        #[route("/settings")]
        Settings {},
        #[route("/broadcast")]
        Broadcast {},
        // 데스크톱 앱 다운로드(과제 ⑥)
        #[route("/download")]
        Download {},
        #[route("/live/:channel_id")]
        Watch { channel_id: String },
        // `/recordings/new`가 `/recordings/:id`보다 먼저 와야 한다.
        #[route("/recordings/new")]
        NewRecording {},
        #[route("/recordings/:id")]
        RecordingDetail { id: i64 },
        // 후원 결제창이 돌아오는 주소(`api::DONATION_SUCCESS_PATH`, `api::DONATION_FAIL_PATH`).
        // 결제 서비스가 붙이는 쿼리(`paymentKey` 등)는 이름이 camelCase라 통째로 받아 화면에서 나눈다.
        #[route("/donations/success?:..query")]
        DonationSuccess { query: String },
        #[route("/donations/fail?:..query")]
        DonationFail { query: String },
        #[route("/:..segments")]
        NotFound { segments: Vec<String> },
}

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    #[cfg(not(feature = "server"))]
    dioxus::launch(App);

    // 서버: DB 연결·마이그레이션·기본 데이터를 준비하고, 서버 함수가 꺼내 쓰도록
    // 앱 서비스를 Extension으로 붙인다. 커스텀 라우터를 합칠 때는 `.merge(..)` 뒤에 `.layer(..)`.
    // 인스턴스 이름(STREAMAPP_INSTANCE_NAME, 과제 ⑤)이 있으면 모든 응답에 답한 인스턴스를 붙인다.
    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        let service = api::server::init_from_env().await?;
        let router = dioxus::server::router(App)
            .merge(api::mediamtx::router(service.clone()))
            .merge(api::recording_files::router(service.clone()))
            .merge(api::payment_webhook::router(service.clone()))
            .merge(api::whip::router(
                service.clone(),
                &api::server::webrtc_base_url_from_env(),
            ))
            .merge(api::health::router(
                service.clone(),
                &api::server::mediamtx_api::address_from_env(),
            ))
            .layer(dioxus::server::axum::Extension(service));
        Ok(api::instance::tag_responses(
            router,
            api::instance::instance_name_from_env().as_deref(),
        ))
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
