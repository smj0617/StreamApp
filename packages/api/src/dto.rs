//! 웹과 서버가 함께 쓰는 데이터 타입. 서버 전용 타입(sqlx 등)을 붙이지 않는다.

use serde::{Deserialize, Serialize};

/// 사용자 (User). 닉네임으로 구별된다.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    /// 입력한 그대로의 닉네임
    pub nickname: String,
}
