//! 웹과 서버가 함께 쓰는 규칙 값. 서버는 이 값으로 거절하고, 화면은 이 값으로 글자 수와 안내를 보여 준다.
//!
//! 웹(wasm) 빌드에서도 컴파일되므로 서버 전용 타입을 쓰지 않는다.

/// 제목 최대 글자 수(앞뒤 공백을 지운 뒤). 다음 라이브 제목과 라이브 제목에 함께 쓴다(홈 카드가 깨지지 않게).
pub const MAX_TITLE_CHARS: usize = 100;

/// 채팅 메시지 최대 글자 수(앞뒤 공백을 지운 뒤).
pub const MAX_CHAT_CHARS: usize = 200;

/// 자동 제목 (Fallback Title): 다음 라이브 제목이 비었을 때 라이브에 붙는 "`<채널 이름>`님의 라이브".
/// 진행 중인 라이브의 제목을 비웠을 때도 이 제목이 된다.
pub fn fallback_title(channel_name: &str) -> String {
    format!("{channel_name}님의 라이브")
}
