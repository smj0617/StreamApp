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

mod auth;
mod channels;
mod chat;
mod comments;
pub mod db;
mod donations;
mod ids;
mod lives;
pub mod payments;
mod publish;
mod recordings;
mod users;
mod watch;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;

pub use auth::SESSION_LIFETIME_MS;
pub use chat::{ChatEvent, ChatSubscription};
pub use donations::NoticeOutcome;
pub use payments::{GatewayError, GatewayPayment, PaymentGateway, Payments};
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
    /// 결제 서비스(토스페이먼츠 테스트 모드). 테스트 키가 설정되지 않았으면 `None`이고 후원을 받지 않는다.
    payments: Option<Payments>,
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
            payments: None,
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

    /// 결제 서비스를 끼운다. 끼우지 않으면 후원 주문을 만들지 않는다([`ServiceError::PaymentsNotConfigured`]).
    pub fn with_payments(mut self, payments: Payments) -> Self {
        self.payments = Some(payments);
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

/// 서비스 동작이 실패한 이유. 서버 잘못([`ServiceError::is_internal`]: `Database`, `File`, `PasswordHash`) 말고는 모두
/// 규칙 위반이며, 메시지는 화면에 그대로 보여 줄 거절 사유다.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("데이터베이스 오류: {0}")]
    Database(#[from] sqlx::Error),
    /// 녹화방송 영상 파일을 쓰거나 옮기지 못했다.
    #[error("파일 오류: {0}")]
    File(#[from] std::io::Error),
    /// 비밀번호 해시를 만들지 못했다(서버 잘못).
    #[error("비밀번호 해시 오류: {0}")]
    PasswordHash(String),
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
    #[error("비밀번호는 {min}~{max}자여야 합니다", min = crate::rules::MIN_PASSWORD_CHARS, max = crate::rules::MAX_PASSWORD_CHARS)]
    PasswordLength,
    /// 닉네임이 없는지 비밀번호가 틀렸는지 알려 주지 않는다.
    #[error("닉네임 또는 비밀번호가 맞지 않습니다")]
    LoginFailed,
    /// 로그인하지 않았거나 세션이 끝났다.
    #[error("로그인이 필요합니다")]
    NotSignedIn,
    #[error("방송 주인만 할 수 있습니다")]
    NotBroadcastOwner,
    #[error("댓글을 쓴 사람만 고치거나 지울 수 있습니다")]
    NotCommentAuthor,
    #[error("없는 녹화방송입니다")]
    UnknownRecording,
    #[error("없는 라이브입니다")]
    UnknownLive,
    #[error("없는 댓글입니다")]
    UnknownComment,
    #[error("댓글 내용을 입력해 주세요")]
    CommentEmpty,
    #[error("댓글은 {max}자를 넘을 수 없습니다", max = crate::rules::MAX_COMMENT_CHARS)]
    CommentTooLong,
    #[error("후원 결제가 설정되지 않았습니다(서버에 결제 서비스 테스트 키가 필요합니다)")]
    PaymentsNotConfigured,
    #[error("후원 금액은 {min} 이상 {max} 이하여야 합니다", min = crate::rules::won_text(crate::rules::MIN_DONATION_WON), max = crate::rules::won_text(crate::rules::MAX_DONATION_WON))]
    DonationAmount,
    #[error("후원 메시지는 {max}자를 넘을 수 없습니다", max = crate::rules::MAX_DONATION_MESSAGE_CHARS)]
    DonationMessageTooLong,
    #[error("자기 방송에는 후원할 수 없습니다")]
    DonationToSelf,
    #[error("없는 후원 주문입니다")]
    UnknownDonation,
    #[error("후원한 사람만 처리할 수 있습니다")]
    NotDonationOwner,
    /// 결제창이 돌려준 금액이 서버의 주문 금액과 다르다(화면에서 금액을 고침).
    #[error("결제 금액이 주문과 다릅니다. 후원을 확정하지 않았습니다")]
    DonationAmountMismatch,
    /// 결제 서비스가 알려 준 결제(주문번호·금액·결제 키)가 서버의 주문과 다르다.
    #[error("결제 정보가 주문과 다릅니다. 후원을 확정하지 않았습니다")]
    PaymentMismatch,
    /// 주문의 받는 채널이 방송의 채널과 다르다.
    #[error("후원 받는 채널이 주문과 다릅니다. 후원을 확정하지 않았습니다")]
    DonationRecipientMismatch,
    /// 결제 서비스가 승인을 거절했다. 결제 서비스가 준 사유 그대로다.
    #[error("결제가 승인되지 않았습니다: {0}")]
    PaymentRejected(String),
    #[error("결제 서비스에 연결하지 못했습니다. 잠시 후 다시 시도해 주세요")]
    PaymentServiceUnavailable,
}

impl ServiceError {
    /// 규칙 위반이 아니라 서버 잘못(DB·파일 오류)인지. 자세한 내용은 로그에만 남기고 화면에는 감춘다.
    pub fn is_internal(&self) -> bool {
        matches!(
            self,
            ServiceError::Database(_) | ServiceError::File(_) | ServiceError::PasswordHash(_)
        )
    }

    /// 이 오류에 맞는 HTTP 상태 코드. 서버 잘못 500, 로그인 필요·로그인 실패 401, 주인·작성자·후원한 사람이
    /// 아님 403, 없는 녹화방송·라이브·댓글·후원 주문 404, 결제 설정 없음·결제 서비스 연결 실패 503, 나머지
    /// 규칙 위반 400.
    pub fn status_code(&self) -> u16 {
        match self {
            ServiceError::Database(_) | ServiceError::File(_) | ServiceError::PasswordHash(_) => {
                500
            }
            ServiceError::NotSignedIn | ServiceError::LoginFailed => 401,
            ServiceError::NotBroadcastOwner
            | ServiceError::NotCommentAuthor
            | ServiceError::NotDonationOwner => 403,
            ServiceError::UnknownRecording
            | ServiceError::UnknownLive
            | ServiceError::UnknownComment
            | ServiceError::UnknownDonation => 404,
            ServiceError::PaymentsNotConfigured | ServiceError::PaymentServiceUnavailable => 503,
            _ => 400,
        }
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
