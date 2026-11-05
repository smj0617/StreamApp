//! StreamApp의 서버 쪽 크레이트. 웹(wasm) 빌드와 서버 빌드 양쪽에서 컴파일된다.
//!
//! 계층은 셋이다.
//!
//! - [`dto`]: 웹과 서버가 함께 쓰는 데이터 타입. serde만 쓴다.
//! - [`rules`]: 웹과 서버가 함께 쓰는 규칙 값(제목·채팅 글자 수, 자동 제목).
//! - 서버 함수(예: [`list_users`]): 도메인별 모듈(`users` 등)에 둔다. 채팅 WebSocket([`connect_chat`])도
//!   서버 함수다. 요청을
//!   [`service::AppService`] 호출로 바꾸기만 하는 얇은 어댑터다. 함수 몸체는 서버 빌드에서만
//!   컴파일되고, 웹 빌드에서는 HTTP 요청을 보내는 함수가 된다.
//! - `server` feature 전용
//!   - [`service`]: Dioxus·HTTP·쿠키를 모르는 앱 서비스. 규칙과 DB를 다루며 주 테스트 경계다.
//!   - [`server`]: 앱 시작(DB 연결·마이그레이션·기본 데이터)과 서버 함수 어댑터가 함께 쓰는 코드.
//!   - [`mediamtx`]: MediaMTX가 부르는 내부 HTTP 엔드포인트(Dioxus 없는 axum 라우터). 보조 테스트 경계다.
//!   - [`recording_files`]: 녹화방송 영상 파일 올리기와 내주기(Dioxus 없는 axum 라우터). 보조 테스트 경계다.
//!   - [`whip`]: 휴대폰 방송 송출 시작·종료를 MediaMTX로 넘기는 중계(Dioxus 없는 axum 라우터). 보조 테스트 경계다.

pub mod dto;
pub mod rules;

mod broadcast;
mod channels;
mod chat;
mod comments;
mod lives;
mod recordings;
mod users;
mod watch;
pub use broadcast::*;
pub use channels::*;
pub use chat::*;
pub use comments::*;
pub use lives::*;
pub use recordings::*;
pub use users::*;
pub use watch::*;

#[cfg(feature = "server")]
pub mod mediamtx;
#[cfg(feature = "server")]
pub mod recording_files;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod service;
#[cfg(feature = "server")]
pub mod whip;
