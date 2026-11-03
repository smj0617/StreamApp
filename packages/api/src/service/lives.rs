//! 라이브(Live) 동작: 송출 시작·종료 반영과 라이브 중인 채널 목록.
//!
//! 라이브는 송출이 시작될 때 열리고 송출이 끊길 때 닫힌다. 송출이 끊겼다가 다시 붙으면
//! 새 라이브가 된다(유예 시간 없음). 채널마다 열린 라이브는 최대 하나다.

use super::{AppService, ServiceError};
use crate::dto::LiveChannel;

/// 다음 라이브 제목이 비어 있을 때 채널 이름 뒤에 붙여 라이브 제목으로 쓴다.
const DEFAULT_TITLE_SUFFIX: &str = "님의 라이브";

#[derive(sqlx::FromRow)]
struct LiveChannelRow {
    channel_id: String,
    channel_name: String,
    title: String,
    started_at_ms: i64,
}

impl From<LiveChannelRow> for LiveChannel {
    fn from(row: LiveChannelRow) -> Self {
        LiveChannel {
            channel_id: row.channel_id,
            channel_name: row.channel_name,
            title: row.title,
            started_at_ms: row.started_at_ms,
        }
    }
}

impl AppService {
    /// 송출 시작 반영: 그 채널의 새 라이브를 연다. 이미 열린 라이브가 있으면 먼저 닫는다.
    ///
    /// 제목은 다음 라이브 제목이고, 비어 있으면 "`<채널 이름>`님의 라이브"다.
    /// 없는 채널이면 [`ServiceError::UnknownChannel`].
    pub async fn publish_started(&self, channel_id: &str) -> Result<(), ServiceError> {
        let now = self.now_ms();
        let mut tx = self.pool.begin().await?;
        // 읽기보다 쓰기를 먼저 해서 트랜잭션이 처음부터 쓰기 잠금을 잡게 한다
        // (읽기로 시작한 트랜잭션이 나중에 쓰기로 올라가면 동시 요청과 SQLITE_BUSY가 날 수 있다).
        close_open_live(&mut tx, channel_id, now).await?;
        let opened = sqlx::query(
            "INSERT INTO lives (channel_id, title, started_at_ms)
             SELECT id,
                    CASE WHEN TRIM(COALESCE(next_live_title, '')) = '' THEN name || ?
                         ELSE next_live_title END,
                    ?
             FROM channels WHERE id = ?",
        )
        .bind(DEFAULT_TITLE_SUFFIX)
        .bind(now)
        .bind(channel_id)
        .execute(&mut *tx)
        .await?;
        if opened.rows_affected() == 0 {
            return Err(ServiceError::UnknownChannel);
        }
        tx.commit().await?;
        Ok(())
    }

    /// 송출 종료 반영: 그 채널의 열린 라이브를 닫고 종료 시각을 기록한다.
    /// 열린 라이브가 없으면(없는 채널 포함) 아무것도 하지 않는다.
    pub async fn publish_ended(&self, channel_id: &str) -> Result<(), ServiceError> {
        let mut conn = self.pool.acquire().await?;
        close_open_live(&mut conn, channel_id, self.now_ms()).await
    }

    /// 라이브 중인 채널 목록. 최근에 시작한 순서로 돌려준다.
    pub async fn live_channels(&self) -> Result<Vec<LiveChannel>, ServiceError> {
        let rows = sqlx::query_as::<_, LiveChannelRow>(
            "SELECT lives.channel_id, channels.name AS channel_name, lives.title, lives.started_at_ms
             FROM lives JOIN channels ON channels.id = lives.channel_id
             WHERE lives.ended_at_ms IS NULL
             ORDER BY lives.started_at_ms DESC, lives.id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(LiveChannel::from).collect())
    }
}

/// 그 채널의 열린 라이브(있으면)를 닫고 종료 시각을 기록한다.
async fn close_open_live(
    conn: &mut sqlx::SqliteConnection,
    channel_id: &str,
    now: i64,
) -> Result<(), ServiceError> {
    sqlx::query("UPDATE lives SET ended_at_ms = ? WHERE channel_id = ? AND ended_at_ms IS NULL")
        .bind(now)
        .bind(channel_id)
        .execute(conn)
        .await?;
    Ok(())
}
