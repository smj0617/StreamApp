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
