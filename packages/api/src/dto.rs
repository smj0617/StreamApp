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

/// 내 채널 설정: 스트리머가 자기 채널로 송출할 때 필요한 값.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MyChannelSettings {
    pub channel_id: String,
    /// 송출 키 (StreamKey). 채널 주인만 알아야 하는 비밀 값
    pub stream_key: String,
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
