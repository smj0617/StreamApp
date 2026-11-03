//! 송출(Publish) 동작.

use super::{AppService, ServiceError};

impl AppService {
    /// 송출 허용 판단: 그 채널이 있고 송출 키가 그 채널의 것이면 `true`(허용), 아니면 `false`(거절).
    pub async fn can_publish(
        &self,
        channel_id: &str,
        stream_key: &str,
    ) -> Result<bool, ServiceError> {
        let allowed: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM channels WHERE id = ? AND stream_key = ?)",
        )
        .bind(channel_id)
        .bind(stream_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(allowed)
    }
}
