//! 앱 서비스: Dioxus·HTTP·쿠키를 모르는 순수 Rust 계층이자 주 테스트 경계.
//!
//! 녹화방송 영상 파일도 여기서 녹화방송 폴더에 쓴다(ADR 0003). 받은 조각의 흐름(`Stream`)만 알고,
//! 멀티파트 같은 HTTP 형식은 모른다.
//!
//! 서버 함수, 채팅 WebSocket, MediaMTX 핸들러는 요청을 이 서비스의
//! 동작 호출로 바꾸기만 한다. "현재 사용자"는 사용자 ID 인자로만 받는다.
//! 동작은 도메인별 파일(`users.rs` 등)에 `impl AppService` 블록으로 나눠 둔다.
//!
//! 테스트는 [`db::connect_memory`]로 만든 새 메모리 DB 위에서 공개된 동작만 확인한다.

mod channels;
mod chat;
pub mod db;
mod ids;
mod lives;
mod publish;
mod recordings;
mod users;
mod watch;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;

pub use chat::{ChatEvent, ChatSubscription};
pub use publish::PublishingStatus;
pub use recordings::DEFAULT_RECORDINGS_DIR;

/// 현재 시각(유닉스 밀리초)을 돌려주는 시계.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// 복제해도 같은 DB와 같은 채팅 허브를 함께 쓴다.
#[derive(Clone)]
pub struct AppService {
    pool: SqlitePool,
    clock: Clock,
    /// 열린 채팅마다 tokio broadcast 하나. 메모리에만 있다(채팅은 저장하지 않는다).
    chat: chat::ChatHub,
    /// 녹화방송 영상 파일을 두는 폴더
    recordings_dir: PathBuf,
    /// 녹화방송 영상 파일 최대 크기(바이트). 기본은 [`crate::rules::MAX_RECORDING_BYTES`]
    max_recording_bytes: u64,
}

impl AppService {
    /// 마이그레이션이 적용된 풀로 서비스를 만든다([`db::connect_file`], [`db::connect_memory`]).
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_clock(pool, Arc::new(now_ms))
    }

    /// 시계를 바꿔 끼운 서비스. 테스트가 라이브 시작·종료 시각을 정할 때 쓴다.
    pub fn with_clock(pool: SqlitePool, clock: Clock) -> Self {
        Self {
            pool,
            clock,
            chat: chat::ChatHub::default(),
            recordings_dir: PathBuf::from(DEFAULT_RECORDINGS_DIR),
            max_recording_bytes: crate::rules::MAX_RECORDING_BYTES,
        }
    }

    /// 녹화방송 영상 파일을 둘 폴더를 바꾼다(기본 [`DEFAULT_RECORDINGS_DIR`]). 없으면 처음 등록할 때 만든다.
    pub fn with_recordings_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.recordings_dir = dir.into();
        self
    }

    /// 녹화방송 영상 파일 최대 크기를 바꾼다. 테스트가 큰 파일 없이 크기 제한을 확인할 때 쓴다.
    pub fn with_max_recording_bytes(mut self, max: u64) -> Self {
        self.max_recording_bytes = max;
        self
    }

    /// 녹화방송 영상 파일을 두는 폴더. 영상 주소(`crate::RECORDING_MEDIA_PATH`)로 이 폴더를 내준다.
    pub fn recordings_dir(&self) -> &Path {
        &self.recordings_dir
    }

    /// 이 서비스 시계의 현재 시각(유닉스 밀리초). 라이브 시각과 같은 기준이다.
    pub fn now_ms(&self) -> i64 {
        (self.clock)()
    }
}

/// 서비스 동작이 실패한 이유. 서버 잘못([`ServiceError::is_internal`]: `Database`, `File`) 말고는 모두
/// 규칙 위반이며, 메시지는 화면에 그대로 보여 줄 거절 사유다.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("데이터베이스 오류: {0}")]
    Database(#[from] sqlx::Error),
    /// 녹화방송 영상 파일을 쓰거나 옮기지 못했다.
    #[error("파일 오류: {0}")]
    File(#[from] std::io::Error),
    #[error("닉네임은 앞뒤 공백을 뺀 2~10자여야 합니다")]
    NicknameLength,
    #[error("닉네임에는 한글, 영문, 숫자만 쓸 수 있습니다")]
    NicknameCharacters,
    #[error("이미 있는 닉네임입니다(영문 대소문자는 구별하지 않습니다)")]
    DuplicateNickname,
    #[error("없는 사용자입니다")]
    UnknownUser,
    /// 다음 라이브 제목, 라이브 제목, 녹화방송 제목에 함께 쓴다.
    #[error("제목은 {max}자를 넘을 수 없습니다", max = crate::rules::MAX_TITLE_CHARS)]
    TitleTooLong,
    #[error("없는 채널입니다")]
    UnknownChannel,
    #[error("라이브 중이 아닙니다")]
    NotLive,
    #[error("메시지 내용을 입력해 주세요")]
    ChatMessageEmpty,
    #[error("채팅 메시지는 {max}자를 넘을 수 없습니다", max = crate::rules::MAX_CHAT_CHARS)]
    ChatMessageTooLong,
    #[error("녹화방송 제목을 입력해 주세요")]
    RecordingTitleEmpty,
    #[error("영상 파일을 골라 주세요")]
    RecordingFileMissing,
    #[error("mp4나 webm 영상 파일만 등록할 수 있습니다")]
    RecordingFormat,
    #[error("빈 영상 파일입니다")]
    RecordingFileEmpty,
    #[error("영상 파일은 {max}MB를 넘을 수 없습니다", max = crate::rules::MAX_RECORDING_MB)]
    RecordingTooLarge,
    #[error("영상을 받는 중에 연결이 끊겼습니다. 다시 올려 주세요")]
    UploadInterrupted,
}

impl ServiceError {
    /// 규칙 위반이 아니라 서버 잘못(DB·파일 오류)인지. 자세한 내용은 로그에만 남기고 화면에는 감춘다.
    pub fn is_internal(&self) -> bool {
        matches!(self, ServiceError::Database(_) | ServiceError::File(_))
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
