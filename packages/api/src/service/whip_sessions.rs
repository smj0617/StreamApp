//! 휴대폰 방송 송출 세션(WHIP) 표: MediaMTX가 준 세션 ID → 송출을 시작한 채널 ID.
//!
//! 송출 종료 요청이 그 채널 주인의 것인지 확인하는 데 쓴다([`crate::whip`]). 앱 인스턴스가 여럿이어도
//! 종료 요청이 어느 인스턴스로 가든 같은 답을 내도록 DB(`whip_sessions`, 마이그레이션 0006)에 둔다.

use super::{AppService, ServiceError};

impl AppService {
    /// 시작한 송출 세션을 기억한다. 같은 세션 ID가 이미 있으면 채널을 새 값으로 바꾼다.
    pub async fn remember_whip_session(
        &self,
        session_id: &str,
        channel_id: &str,
    ) -> Result<(), ServiceError> {
        sqlx::query(
            "INSERT INTO whip_sessions (session_id, channel_id, created_at_ms) VALUES (?, ?, ?)
             ON CONFLICT (session_id) DO UPDATE SET channel_id = excluded.channel_id,
                                                    created_at_ms = excluded.created_at_ms",
        )
        .bind(session_id)
        .bind(channel_id)
        .bind(self.now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 송출 세션을 시작한 채널 ID. 모르는(또는 이미 끝낸) 세션이면 `None`.
    pub async fn whip_session_channel(
        &self,
        session_id: &str,
    ) -> Result<Option<String>, ServiceError> {
        Ok(
            sqlx::query_scalar("SELECT channel_id FROM whip_sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// 끝낸 송출 세션을 지운다. 없으면 아무것도 하지 않는다.
    pub async fn forget_whip_session(&self, session_id: &str) -> Result<(), ServiceError> {
        sqlx::query("DELETE FROM whip_sessions WHERE session_id = ?")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
