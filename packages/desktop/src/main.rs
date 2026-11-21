//! StreamApp 데스크톱 클라이언트(Windows·macOS). 배포한 백엔드의 라이브·녹화방송 목록을 보고 재생하고,
//! 로그인하면 시청 화면에서 댓글을 쓴다.
//!
//! Windows 릴리스 빌드에서는 콘솔 창을 띄우지 않는다(`dx bundle`도 같은 설정을 쓴다).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use dioxus::desktop::{icon_from_memory, Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

mod auth;
mod client;
mod comments;
mod hls_proxy;
mod player;
mod server_url;
mod session;
mod views;

use auth::Login;
use server_url::ServerUrl;
use views::{Home, Live, Recording, Server, Settings, Shell};

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Shell)]
        #[route("/")]
        Home {},
        #[route("/live/:channel_id")]
        Live { channel_id: String },
        #[route("/recordings/:id")]
        Recording { id: i64 },
        #[route("/settings")]
        Settings {},
        #[route("/login")]
        Login {},
}

const MAIN_CSS: Asset = asset!("/assets/main.css");
const WINDOW_ICON: &[u8] = include_bytes!("../icons/icon.png");

fn main() {
    let mut window = WindowBuilder::new()
        .with_title("StreamApp")
        .with_inner_size(LogicalSize::new(1100.0, 760.0))
        .with_min_inner_size(LogicalSize::new(480.0, 400.0));
    if let Ok(icon) = icon_from_memory(WINDOW_ICON) {
        window = window.with_window_icon(Some(icon));
    }
    let mut config = Config::new().with_window(window);
    // Windows에서는 창 위의 기본 메뉴 막대를 없앤다. macOS는 기본 메뉴(편집)가 있어야
    // 서버 주소 칸에서 Cmd+C·Cmd+V가 동작하므로 그대로 둔다.
    if cfg!(target_os = "windows") {
        config = config.with_menu(None);
    }

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .launch(App);
}

#[component]
fn App() -> Element {
    let server = use_signal(ServerUrl::load);
    use_context_provider(|| Server(server));
    // 로그인 상태: 저장한 세션을 읽어 서버에 확인한다. 서버 주소를 바꾸면 세션을 버린다.
    auth::use_auth_provider(server);
    // 라이브 HLS 중계(`/hlsproxy/...`). Windows WebView2의 hls.js가 쓴다.
    dioxus::desktop::use_asset_handler(hls_proxy::PROXY_NAME, hls_proxy::handle);

    rsx! {
        document::Title { "StreamApp" }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Router::<Route> {}
    }
}
