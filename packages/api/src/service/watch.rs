//! 시청 동작: 시청 페이지 정보. 사용자를 고르지 않은 시청자도 볼 수 있다.

use super::{AppService, ServiceError};
use crate::dto::{OpenLive, WatchPage};
use crate::mediamtx::HLS_BASE_URL;

#[derive(sqlx::FromRow)]
struct WatchPageRow {
    channel_id: String,
    channel_name: String,
    /// 열린 라이브가 없으면 `NULL`
    live_id: Option<i64>,
    title: Option<String>,
    started_at_ms: Option<i64>,
}

impl AppService {
    /// 시청 페이지 정보: 채널 이름, 재생 주소, 지금 열린 라이브(제목, 시작 시각).
    /// 라이브 중이 아니면 `live`가 `None`이고, 없는 채널이면 `None`.
    pub async fn watch_page(&self, channel_id: &str) -> Result<Option<WatchPage>, ServiceError> {
        // 채널마다 열린 라이브는 최대 하나다(부분 유일 인덱스).
        let row = sqlx::query_as::<_, WatchPageRow>(
            "SELECT channels.id AS channel_id, channels.name AS channel_name,
                    lives.id AS live_id, lives.title, lives.started_at_ms
             FROM channels
             LEFT JOIN lives ON lives.channel_id = channels.id AND lives.ended_at_ms IS NULL
             WHERE channels.id = ?",
        )
        .bind(channel_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| {
            let live = match (row.live_id, row.title, row.started_at_ms) {
                (Some(id), Some(title), Some(started_at_ms)) => Some(OpenLive {
                    id,
                    title,
                    started_at_ms,
                }),
                _ => None,
            };
            WatchPage {
                playback_url: format!("{HLS_BASE_URL}/{}/index.m3u8", row.channel_id),
                channel_id: row.channel_id,
                channel_name: row.channel_name,
                live,
            }
        }))
    }
}
