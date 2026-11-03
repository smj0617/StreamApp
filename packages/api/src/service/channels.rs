//! 채널 동작.

use super::{AppService, ServiceError};
use crate::dto::MyChannelSettings;

#[derive(sqlx::FromRow)]
struct ChannelSettingsRow {
    id: String,
    stream_key: String,
}

impl AppService {
    /// 내 채널 설정 조회. 사용자가 없으면 `None`.
    pub async fn my_channel_settings(
        &self,
        user_id: i64,
    ) -> Result<Option<MyChannelSettings>, ServiceError> {
        let row = sqlx::query_as::<_, ChannelSettingsRow>(
            "SELECT id, stream_key FROM channels WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| MyChannelSettings {
            channel_id: row.id,
            stream_key: row.stream_key,
        }))
    }
}
