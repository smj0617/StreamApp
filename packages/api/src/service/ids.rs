//! 무작위 식별자. 서버에서만 만든다(rand는 wasm 빌드에 넣지 않는다).

use rand::{distr::Alphanumeric, Rng};

const CHANNEL_ID_LEN: usize = 12;
const STREAM_KEY_LEN: usize = 32;
const RECORDING_FILE_STEM_LEN: usize = 16;

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

/// 녹화방송 영상 파일 이름의 앞부분: URL과 파일 이름에 그대로 쓸 수 있게 소문자 영문과 숫자만 쓴다.
/// 사용자가 올린 원래 파일 이름은 쓰지 않는다(경로 문자, 겹치는 이름).
pub(super) fn new_recording_file_stem() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .map(|b| (b as char).to_ascii_lowercase())
        .take(RECORDING_FILE_STEM_LEN)
        .collect()
}
