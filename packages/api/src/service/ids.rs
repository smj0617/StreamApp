//! 무작위 식별자. 서버에서만 만든다(rand는 wasm 빌드에 넣지 않는다).

use rand::{distr::Alphanumeric, Rng};

const CHANNEL_ID_LEN: usize = 12;
const STREAM_KEY_LEN: usize = 32;

/// 채널 ID: URL과 MediaMTX 경로에 그대로 넣을 수 있게 소문자 영문과 숫자만 쓴다.
pub(super) fn new_channel_id() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .map(|b| (b as char).to_ascii_lowercase())
        .take(CHANNEL_ID_LEN)
        .collect()
}

/// 송출 키: 추측하기 어렵게 대소문자 영문과 숫자 32자(약 190비트).
pub(super) fn new_stream_key() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .take(STREAM_KEY_LEN)
        .collect()
}
