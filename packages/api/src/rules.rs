//! 웹과 서버가 함께 쓰는 규칙 값. 서버는 이 값으로 거절하고, 화면은 이 값으로 글자 수와 안내를 보여 준다.
//!
//! 웹(wasm) 빌드에서도 컴파일되므로 서버 전용 타입을 쓰지 않는다.

/// 제목 최대 글자 수(앞뒤 공백을 지운 뒤). 다음 라이브 제목, 라이브 제목, 녹화방송 제목에 함께 쓴다
/// (홈 카드가 깨지지 않게).
pub const MAX_TITLE_CHARS: usize = 100;

/// 녹화방송 영상 파일 최대 크기(MB).
pub const MAX_RECORDING_MB: u64 = 500;

/// 녹화방송 영상 파일 최대 크기(바이트). 화면은 올리기 전에, 서버는 받는 동안 이 값으로 거절한다.
pub const MAX_RECORDING_BYTES: u64 = MAX_RECORDING_MB * 1024 * 1024;

/// 녹화방송으로 등록할 수 있는 영상 파일의 확장자(소문자). 브라우저 `<video>`가 그대로 재생하는 형식만 받는다.
pub const RECORDING_EXTENSIONS: [&str; 2] = ["mp4", "webm"];

/// 파일 이름의 확장자가 [`RECORDING_EXTENSIONS`] 중 하나면 그 확장자(소문자). 아니면 `None`.
pub fn recording_extension(file_name: &str) -> Option<&'static str> {
    let (_, extension) = file_name.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    RECORDING_EXTENSIONS
        .into_iter()
        .find(|allowed| *allowed == extension)
}

/// 채팅 메시지 최대 글자 수(앞뒤 공백을 지운 뒤).
pub const MAX_CHAT_CHARS: usize = 200;

/// 자동 제목 (Fallback Title): 다음 라이브 제목이 비었을 때 라이브에 붙는 "`<채널 이름>`님의 라이브".
/// 진행 중인 라이브의 제목을 비웠을 때도 이 제목이 된다.
pub fn fallback_title(channel_name: &str) -> String {
    format!("{channel_name}님의 라이브")
}

/// 댓글 최대 글자 수(앞뒤 공백을 지운 뒤).
pub const MAX_COMMENT_CHARS: usize = 500;

/// 비밀번호 최소 글자 수.
pub const MIN_PASSWORD_CHARS: usize = 8;

/// 비밀번호 최대 글자 수. 해시 계산이 너무 길어지지 않게 막는다.
pub const MAX_PASSWORD_CHARS: usize = 64;

/// 후원 최소 금액(원).
pub const MIN_DONATION_WON: i64 = 1_000;

/// 후원 최대 금액(원). 테스트 결제라도 한 번에 너무 큰 금액을 막는다.
pub const MAX_DONATION_WON: i64 = 1_000_000;

/// 화면의 후원 금액 바로 고르기 버튼(원).
pub const DONATION_PRESETS_WON: [i64; 4] = [1_000, 5_000, 10_000, 50_000];

/// 후원 메시지 최대 글자 수(앞뒤 공백을 지운 뒤). 메시지는 비워도 된다.
pub const MAX_DONATION_MESSAGE_CHARS: usize = 100;

/// 금액을 "12,345원"으로 쓴다.
pub fn won_text(amount: i64) -> String {
    let digits = amount.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let sign = if amount < 0 { "-" } else { "" };
    format!("{sign}{grouped}원")
}
