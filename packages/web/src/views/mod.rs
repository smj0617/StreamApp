mod chat;
mod hls_player;
mod home;
mod not_found;
mod recordings;
mod settings;
mod shell;
mod watch;

pub use home::Home;
pub use not_found::NotFound;
pub use recordings::{NewRecording, RecordingDetail};
pub use settings::Settings;
pub use shell::Shell;
pub use watch::Watch;

/// `document::eval`에 넘길 스크립트: 요소 id를 JS 상수 `elementId`로 앞에 붙인다.
///
/// 스크립트 안에 DOM id를 문자열로 다시 적지 않고, rsx의 `id:`에 쓰는 Rust 상수 하나에서 만들게 한다.
/// id는 영문·숫자·`-`·`_`로 된 상수라 Rust 문자열 표기(`{:?}`)가 그대로 JS 문자열이 된다.
fn script_with_element_id(element_id: &str, script: &str) -> String {
    debug_assert!(element_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    format!("const elementId = {element_id:?};\n{script}")
}
