//! 웹과 서버가 함께 쓰는 데이터 타입. 서버 전용 타입(sqlx 등)을 붙이지 않는다.

use serde::{Deserialize, Serialize};

/// 사용자 (User). 닉네임으로 구별된다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    /// 입력한 그대로의 닉네임
    pub nickname: String,
}

/// 사용자 만들기 결과: 만든 사용자와, 함께 만들어진 그 사용자의 채널.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CreatedUser {
    pub user: User,
    pub channel_id: String,
    /// 채널 이름 = 만들 때의 닉네임
    pub channel_name: String,
}

/// 내 채널 설정: 스트리머가 자기 채널로 송출할 때 필요한 값, 다음 라이브 제목,
/// 라이브 중이면 진행 중인 라이브의 제목.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MyChannelSettings {
    pub channel_id: String,
    /// 송출 키 (StreamKey). 채널 주인만 알아야 하는 비밀 값
    pub stream_key: String,
    /// 다음 라이브 제목 (NextLiveTitle). 적지 않았으면 `None`
    pub next_live_title: Option<String>,
    /// 진행 중인 라이브의 제목. 라이브 중이 아니면 `None`
    pub live_title: Option<String>,
    /// OBS "서버" 칸에 그대로 넣을 값
    pub obs_server: String,
    /// OBS "스트림 키" 칸에 그대로 넣을 값: `<채널ID>?key=<송출 키>`
    pub obs_stream_key: String,
}

/// 라이브 중인 채널 하나: 홈 카드에 보여 줄 값.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LiveChannel {
    pub channel_id: String,
    pub channel_name: String,
    /// 지금 열린 라이브의 제목
    pub title: String,
    /// 라이브가 열린 시각(유닉스 밀리초)
    pub started_at_ms: i64,
}

/// 시청 페이지 정보: 채널 하나와, 그 채널에서 지금 열린 라이브(있으면).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct WatchPage {
    pub channel_id: String,
    pub channel_name: String,
    /// 영상을 재생할 HLS 주소: `<HLS 기본 주소>/<채널ID>/index.m3u8`. HLS 기본 주소는 서버의
    /// `api::mediamtx::HLS_BASE_URL`이다.
    pub playback_url: String,
    /// 지금 열린 라이브. 라이브 중이 아니면(오프라인) `None`
    pub live: Option<OpenLive>,
}

/// 채널에서 지금 열려 있는(송출 중인) 라이브.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenLive {
    /// 라이브 ID. 라이브 댓글([`BroadcastTarget::Live`])을 이 ID로 단다.
    pub id: i64,
    pub title: String,
    /// 라이브가 열린 시각(유닉스 밀리초)
    pub started_at_ms: i64,
}

/// 채팅 메시지 하나. 저장하지 않고 그 채널의 채팅 참여자에게 보내기만 한다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChatMessage {
    /// 보낸 사용자의 닉네임(보낼 때의 현재 사용자)
    pub nickname: String,
    /// 앞뒤 공백을 지운 내용
    pub content: String,
    /// 서버가 받은 시각(유닉스 밀리초)
    pub sent_at_ms: i64,
}

/// 홈의 라이브 중인 채널 목록과, 목록을 만든 서버의 현재 시각.
///
/// 경과 시간은 `server_now_ms - started_at_ms`로 계산한다. 웹(wasm)에서는
/// `SystemTime::now()`가 panic하고, 서버 렌더링 결과와 하이드레이션 결과도 같아야 하기 때문이다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LiveChannelList {
    /// 최근에 시작한 순서
    pub channels: Vec<LiveChannel>,
    /// 서버의 현재 시각(유닉스 밀리초)
    pub server_now_ms: i64,
}

/// 녹화방송 (Recording) 목록의 한 편: 홈 카드에 보여 줄 값.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordingSummary {
    pub id: i64,
    pub title: String,
    pub channel_name: String,
    /// 등록한 시각(유닉스 밀리초)
    pub registered_at_ms: i64,
}

/// 홈의 녹화방송 목록과, 목록을 만든 서버의 현재 시각(등록 후 지난 시간 계산용, [`LiveChannelList`] 참고).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordingList {
    /// 최근에 등록한 순서
    pub recordings: Vec<RecordingSummary>,
    /// 서버의 현재 시각(유닉스 밀리초)
    pub server_now_ms: i64,
}

/// 녹화방송 상세 페이지 정보.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RecordingPage {
    pub id: i64,
    pub title: String,
    pub channel_id: String,
    /// 녹화방송 주인(채널 주인)의 사용자 ID. 화면은 주인에게만 수정·삭제 버튼을 보이고, 권한은 서버가 다시 확인한다.
    pub owner_id: i64,
    pub channel_name: String,
    /// `<video>`에 넣을 영상 주소: `<crate::RECORDING_MEDIA_PATH>/<파일 이름>`(앱과 같은 출처)
    pub video_url: String,
    /// 등록한 시각(유닉스 밀리초)
    pub registered_at_ms: i64,
    /// 정보를 만든 서버의 현재 시각(유닉스 밀리초)
    pub server_now_ms: i64,
}

/// 방송 하나: 녹화방송 하나 또는 라이브 한 회차. 댓글과 후원이 이 방송에 붙는다.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BroadcastTarget {
    /// 녹화방송 ID
    Recording(i64),
    /// 라이브 ID([`OpenLive::id`])
    Live(i64),
}

/// 댓글 (Comment) 하나.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub id: i64,
    /// 쓴 사용자의 ID. 화면은 작성자에게만 수정·삭제 버튼을 보이고, 권한은 서버가 다시 확인한다.
    pub author_id: i64,
    pub author_nickname: String,
    /// 앞뒤 공백을 지운 내용
    pub content: String,
    /// 쓴 시각(유닉스 밀리초)
    pub created_at_ms: i64,
    /// 고친 적이 있는지
    pub edited: bool,
}

/// 방송 하나의 댓글 목록(오래된 순)과, 목록을 만든 서버의 현재 시각(쓴 뒤 지난 시간 계산용).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommentList {
    pub comments: Vec<Comment>,
    /// 서버의 현재 시각(유닉스 밀리초)
    pub server_now_ms: i64,
}

/// 후원 주문: 서버가 후원 내용을 확인하고 만든 주문. 화면은 이 값 그대로 결제 서비스(토스페이먼츠 테스트 모드)
/// 결제창을 연다. 금액·받는 채널은 서버가 정했고, 확정할 때 서버가 다시 확인한다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DonationOrder {
    /// 결제 서비스에 넘길 주문번호(서버가 만든 무작위 값)
    pub order_id: String,
    /// 결제할 금액(원)
    pub amount: i64,
    /// 결제창에 보일 주문 이름: "<채널 이름>님 후원"
    pub order_name: String,
    /// 받는 채널 이름
    pub recipient_name: String,
    /// 결제 서비스 클라이언트 키(브라우저에 공개되는 값, 테스트 키만 쓴다)
    pub client_key: String,
    /// 결제창의 구매자 식별 값(주문마다 새로 만든 무작위 값)
    pub customer_key: String,
}

/// 결제 서비스가 성공 주소로 돌려준 승인 요청 값. 화면이 받은 그대로 서버에 넘기고, 서버는 이 값을 믿지 않고
/// 주문과 비교한 뒤 결제 서비스에 승인을 요청한다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PaymentApproval {
    pub payment_key: String,
    pub order_id: String,
    pub amount: i64,
}

/// 후원 주문의 상태.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DonationStatus {
    /// 결제창을 열었고 아직 승인을 확인하지 않았다
    Pending,
    /// 서버가 결제 승인을 확인해 확정했다. 후원 내역과 합계에는 이것만 들어간다
    Confirmed,
    /// 후원한 사람이 결제를 취소했다
    Canceled,
    /// 결제가 실패했거나 결제 정보가 주문과 달랐다
    Failed,
}

/// 후원 결제를 처리한 결과(확정·취소·실패). 결과 페이지가 보여 준다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DonationResult {
    pub order_id: String,
    pub status: DonationStatus,
    pub amount: i64,
    pub recipient_name: String,
    /// 받는 채널 ID(라이브 시청 페이지로 돌아갈 때 쓴다)
    pub recipient_channel_id: String,
    /// 후원한 방송. 녹화방송이 지워졌으면 `None`
    pub target: Option<BroadcastTarget>,
    /// 이미 확정된 결제의 승인 통지를 다시 받았는지(새로고침 등). 이때도 다시 적립하지 않는다
    pub already_processed: bool,
    /// 취소·실패한 이유
    pub failure_reason: Option<String>,
}

/// 확정된 후원 하나: 방송별 후원 내역의 한 줄.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Donation {
    pub id: i64,
    pub donor_nickname: String,
    /// 원(KRW)
    pub amount: i64,
    /// 후원 메시지(비어 있을 수 있다)
    pub message: String,
    /// 확정한 시각(유닉스 밀리초)
    pub confirmed_at_ms: i64,
}

/// 방송 하나의 후원 내역(최근 확정순)과 합계, 받는 사람 정보.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DonationBoard {
    pub donations: Vec<Donation>,
    /// 확정된 후원 금액 합계(원)
    pub total_amount: i64,
    /// 확정된 후원 수
    pub count: i64,
    /// 받는 채널 ID와 이름, 채널 주인 사용자 ID(자기 방송에는 후원 칸을 숨긴다)
    pub recipient_channel_id: String,
    pub recipient_name: String,
    pub recipient_user_id: i64,
    /// 서버에 결제 서비스 테스트 키가 설정되어 있는지. 없으면 화면은 후원 칸 대신 안내를 보인다
    pub payments_enabled: bool,
    /// 서버의 현재 시각(유닉스 밀리초)
    pub server_now_ms: i64,
}
